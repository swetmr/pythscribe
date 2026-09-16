"""pythscribe.ffi -- the list-buffer FFI shim (v0.2.5 M1) and its Node runner.

`list_buffer.mjs` is the ONE place JS lays a `list[int]`/`list[float]` parameter out in a
kernel's linear memory and reads an `out` buffer back (the compiler's own list layout, a
declared binding pinned by `tests/pythscribe/test_image_kernel.py`). The Gradio component
carries a byte-identical copy (`frontend/list_buffer.mjs`; identity is a committed gate)
because a pip-installed component cannot reach into this package's source tree.

`run_kernel` runs a kernel through the SAME shim under Node -- the compiled arm of the M1
differential oracle (shim output vs the plain-Python kernel vs a NumPy reference).
"""
from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path
from typing import Any, Sequence

from ..artifacts import ArtifactInfo
from ..decorators import WasmBinding, binding_of

SHIM = Path(__file__).with_name("list_buffer.mjs")
_RUNNER = Path(__file__).with_name("_node_runner.mjs")

SUPPORTED_PARAM_TYPES = ("int", "float", "bool", "list[int]", "list[float]")
SUPPORTED_RETURN_TYPES = ("int", "float", "bool", "None")

# v0.2.5 M2c: the FFI grammar also admits fixed-width numeric arrays `Array[dtype]` /
# `Array[dtype, 1|2]` (marshalled by list_buffer.mjs's typed-array path). The annotation
# normalization AND the dtype alphabet are single-sourced from the array marshalling library
# (N3 unification: one `array_param_spec`, so the FFI grammar, runtime.array_param_spec and
# list_buffer.mjs::arrayParamSpec can never drift apart).
from ..runtime.array_buffer import DTYPES as ARRAY_DTYPES  # noqa: E402  (leaf module: struct/sys only, no cycle)
from ..runtime.array_buffer import array_param_spec  # noqa: E402  (re-exported for `pythscribe.ffi.array_param_spec`)


def is_supported_param(ann: str) -> bool:
    """A param annotation the list-buffer FFI grammar admits: a scalar/list spelling OR an
    admitted `Array[dtype, 1|2]`."""
    return ann in SUPPORTED_PARAM_TYPES or array_param_spec(ann) is not None


class FfiError(RuntimeError):
    pass


def signature_of(fn_or_binding: Any) -> tuple[list[str], str]:
    """(param annotation texts, return annotation text) of a @wasm kernel, from its
    statically-read source; refuses anything outside the shim's grammar."""
    b: WasmBinding = fn_or_binding if isinstance(fn_or_binding, WasmBinding) else binding_of(fn_or_binding)
    if b.params is None or b.return_type is None:
        raise FfiError(f"`{b.name}`: signature unavailable (unreadable source)")
    if not b.positional_only:
        # opus r1/B3: the shim calls by position with an exact arity check (every argument is
        # supplied, so a default is never used); keyword-only, *args and **kwargs have no
        # positional meaning at the WASM boundary and are refused
        raise FfiError(
            f"`{b.name}`: the list-buffer FFI calls kernels by position with every argument supplied; "
            f"keyword-only / *args / **kwargs parameters are refused (signature: {', '.join(f'{n}: {a}' for n, a in b.params)})"
        )
    params = [ann for _, ann in b.params]
    for name, ann in b.params:
        if not is_supported_param(ann):
            raise FfiError(f"`{b.name}`: parameter `{name}: {ann}` is outside the list-buffer FFI grammar {SUPPORTED_PARAM_TYPES} + Array[dtype, 1|2] (dtypes {ARRAY_DTYPES})")
    if b.return_type not in SUPPORTED_RETURN_TYPES:
        raise FfiError(f"`{b.name}`: return type `{b.return_type}` is outside the FFI grammar {SUPPORTED_RETURN_TYPES}")
    return params, b.return_type


_JS_SAFE_INT = 2**53 - 1
_I64_MAX = 2**63 - 1
_I64_MIN = -(2**63)


def _check_json_exact(value: Any, where: str) -> None:
    """Refuse anything the JSON hop to Node would silently round (review r1/B2): a Python int
    beyond 2**53-1 must be passed as {"bigint": "<decimal>"} (and fit i64); floats must be finite."""
    if isinstance(value, bool) or value is None or isinstance(value, str):
        return
    if isinstance(value, int):
        if abs(value) > _JS_SAFE_INT:
            raise FfiError(f'{where}: int {value!r} exceeds JS Number precision; pass {{"bigint": "{value}"}}')
        return
    if isinstance(value, float):
        if value != value or value in (float("inf"), float("-inf")):
            raise FfiError(f"{where}: non-finite float {value!r} cannot cross JSON")
        return
    if isinstance(value, dict):
        if "bigint" in value:
            # a DECIMAL STRING only: a numeric `bigint` would itself be rounded by the JSON hop
            # (codex r2: {"bigint": 2**60+1} arrived as 2**60) -- the escape hatch must not leak
            if not isinstance(value["bigint"], str):
                raise FfiError(f'{where}: bigint must be a decimal STRING, got {type(value["bigint"]).__name__}')
            try:
                b = int(value["bigint"], 10)
            except ValueError:
                raise FfiError(f"{where}: bigint must be a decimal string") from None
            if b > _I64_MAX or b < _I64_MIN:
                raise FfiError(f"{where}: {b} is outside the i64 range")
        return  # file / zeros descriptors
    if isinstance(value, (list, tuple)):
        for i, v in enumerate(value):
            _check_json_exact(v, f"{where}[{i}]")
        return
    raise FfiError(f"{where}: {type(value).__name__} is not a crossable argument")


def run_kernel(
    artifact: ArtifactInfo | Path,
    fn: str,
    param_types: Sequence[str],
    calls: Sequence[dict[str, Any]],
    *,
    return_type: str = "int",
    wasm: Path | None = None,
) -> list[dict]:
    """Run `calls` (each `{"args": [...], "read_back": [i], "out_files": {i: path}}`) through
    the shim under Node against the artifact's .wasm (or an explicit `wasm` path -- the
    negative controls point this at a poisoned/corrupt module)."""
    wasm_path = Path(wasm) if wasm is not None else (artifact.wasm if isinstance(artifact, ArtifactInfo) else Path(artifact))
    for c in calls:
        _check_json_exact(c.get("args", []), "args")
    node = shutil.which("node")
    if not node:
        # T4 (spec 13-09-26): the shim's Node oracle arm is opt-in verification, never the critical
        # path -- a structured, typed message (no spawn traceback); `@wasm` itself needs no Node.
        raise FfiError(
            "node is required to run the compiled kernel through the list-buffer shim (run_kernel), and no `node` "
            "is on PATH. `pyths build`, `@wasm` (wasmtime, in-process) and the adapters do NOT need Node; "
            "install Node (https://nodejs.org) or put it on PATH to use this verify arm."
        )
    req = {"fn": fn, "param_types": list(param_types), "return_type": return_type, "calls": list(calls)}
    proc = subprocess.run(
        [node, str(_RUNNER), str(wasm_path)],
        input=json.dumps(req),
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise FfiError(f"node ffi runner failed (exit {proc.returncode}): {proc.stderr.strip()[:2000]}")
    try:
        results = json.loads(proc.stdout)
    except ValueError as e:
        raise FfiError(f"node ffi runner emitted non-JSON: {proc.stdout[:500]!r}") from e
    for r in results:  # exact scalars back: {"bigint": s} -> int, {"nonfinite": s} -> nan/inf (never a silent null)
        if r.get("ok"):
            r["value"] = _decode_scalar(r.get("value"))
            outs = {}
            for k, v in (r.get("outs") or {}).items():
                if isinstance(v, dict) and ("i32_file" in v or "f64_file" in v):
                    outs[k] = v  # the label names the element type written (int32 / float64)
                elif isinstance(v, dict) and v.get("inline"):
                    # the caller asked for an int32 file but the read-back has wider elements:
                    # an explicit error, not a silently different return shape (codex r3)
                    raise FfiError(f"{fn}: out_files[{k}] not written -- {v.get('note')}; drop out_files to receive the exact values inline")
                else:
                    outs[k] = [_decode_scalar(x) for x in v]
            r["outs"] = outs
    return results


def _decode_scalar(v: Any) -> Any:
    if isinstance(v, dict):
        if "bigint" in v:
            return int(v["bigint"])
        if "nonfinite" in v:
            return float(v["nonfinite"])
    return v
