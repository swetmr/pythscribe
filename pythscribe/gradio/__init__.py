"""pythscribe.gradio -- the Svelte adapter (plan §3/§4a): run a `@wasm` kernel's compiled
bundle in the user's browser tab inside a Gradio custom component, return the value to the
script, and fall back to Python when there is no usable artifact.

    from pythscribe.gradio import WasmFunction, dispatch, result_of

    comp = WasmFunction(label="rms_gain (@wasm)")
    btn.click(lambda xs, t: dispatch(rms_gain, parse(xs), t), [xs, t], [comp, out])
    comp.change(lambda payload: fmt(result_of(rms_gain, payload)), [comp], [out])

ONE authority decides which path runs (`dispatch`): artifact resolved at import -> the
payload carries `bundle` and `result: None` and the browser computes; otherwise the Python
wrapper runs here and the payload already carries `result` with `path: "python-fallback"`.
`result_of` is the single place a result is read back.

Trust boundary (review R1/B2, SF2, SF6): everything that comes back from the component is
CLIENT data. The server keeps, per dispatch nonce, the arguments it dispatched and the
Python-call counter at dispatch time; `result_of` uses only those (never the client's copy)
for the fallback re-run and for the `python_calls` path marker. The float itself is carried
as its IEEE-754 bit pattern (`bits`, 16 hex chars) and reconstructed here, so -0.0, inf and
nan survive the JSON hop instead of silently becoming 0 / null.
"""
from __future__ import annotations

import collections
import itertools
import math
import re
import struct
import threading
import time
from pathlib import Path
from typing import Any, Callable
from urllib.parse import quote

from ..decorators import WasmBinding, binding_of

__all__ = ["WasmFunction", "FlowGraph", "client_callback", "CallbackSpec", "bundle_url", "dispatch", "result_of",
           "deadline_result", "describe", "float_from_bits",
           "dispatch_image", "image_result_of", "wasm_url", "client_side", "ClientSide", "Arg"]

_nonce = itertools.count(1)
_registered: set[Path] = set()
_reg_lock = threading.Lock()

# nonce -> (function name, python_calls at dispatch, dispatched args); bounded so a
# long-running app cannot grow it. `_completed` records nonces whose result reached the server.
_DISPATCH_HISTORY = 1024
_dispatched: "collections.OrderedDict[int, tuple[str, int, tuple[Any, ...], int]]" = collections.OrderedDict()
_completed: set[int] = set()
_dispatch_lock = threading.Lock()

def __getattr__(name: str) -> Any:
    """LAZY framework import (M1.5 packaging): `import pythscribe.gradio` and the pure helpers
    (dispatch / result_of / describe ...) need no Gradio at all; only touching `WasmFunction`
    imports the vendored custom component -- and therefore Gradio -- with a helpful error
    when the framework is absent. The component ships INSIDE the pythscribe wheel
    (`gradio_wasmfunction`, vendored from pythscribe/gradio/wasm_function/backend)."""
    if name == "WasmFunction":
        try:
            from gradio_wasmfunction import WasmFunction
        except ImportError as e:
            raise ImportError(
                "pythscribe.gradio.WasmFunction needs Gradio: `pip install gradio` (or `pip install pythscribe[gradio]`); "
                f"underlying error: {e}"
            ) from e
        globals()["WasmFunction"] = WasmFunction
        return WasmFunction
    if name in ("FlowGraph", "CallbackGrid"):
        # the callback-path Component subclasses live in the SAME vendored package as WasmFunction
        # (B1-residual: Gradio serves a custom component's frontend from the module that DEFINES the
        # class, so they must resolve under gradio_wasmfunction/).
        try:
            import gradio_wasmfunction as _gwf
            comp = getattr(_gwf, name)
        except (ImportError, AttributeError) as e:
            raise ImportError(
                f"pythscribe.gradio.{name} needs Gradio: `pip install gradio` (or `pip install pythscribe[gradio]`); "
                f"underlying error: {e}"
            ) from e
        globals()[name] = comp
        return comp
    if name in ("client_callback", "CallbackSpec"):
        # the pythscribe-side callback builder (needs no Gradio; may import pythscribe)
        from .callback import CallbackSpec, client_callback
        globals()["client_callback"] = client_callback
        globals()["CallbackSpec"] = CallbackSpec
        return {"client_callback": client_callback, "CallbackSpec": CallbackSpec}[name]
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


_HEX16 = re.compile(r"[0-9a-fA-F]{16}")


def float_from_bits(bits: Any) -> float | None:
    """Reconstruct a float from its little-endian IEEE-754 hex (16 chars); None if malformed."""
    if not isinstance(bits, str) or not _HEX16.fullmatch(bits):
        return None  # (bytes.fromhex skips whitespace -> struct.error; the regex is the authority, R2/S1)
    try:
        return struct.unpack("<d", bytes.fromhex(bits))[0]
    except (ValueError, struct.error):
        return None


def _static_file_url(path: Path) -> str:
    """Register the artifact directory as a Gradio static path and return the URL Gradio's
    file route serves it under. Relative module imports (`./pyths-runtime/index.js`,
    `new URL('./x.wasm', import.meta.url)`) resolve against this URL, so the WHOLE artifact
    directory must be servable, not just the entry. The path is percent-encoded so `#`,
    `?`, `%` or spaces in a checkout path cannot break the relative resolution (R1/SF5).

    SCOPE NOTE (#506): this NON-callback bundle path serves the WHOLE artifact dir BY NECESSITY
    (a bundle's relative ES-module imports need its siblings) -- pre-existing, OUT OF SCOPE, left
    unchanged. The CALLBACK path fetches only the single `.wasm` and so registers just that FILE:
    see `gradio_wasmfunction.wasmfunction._served_wasm_url`."""
    import gradio as gr

    adir = path.parent.resolve()
    with _reg_lock:
        if adir not in _registered:
            gr.set_static_paths([adir])
            _registered.add(adir)
    return "/gradio_api/file=" + quote(path.resolve().as_posix(), safe="/:")


def bundle_url(fn: Callable[..., Any]) -> str | None:
    b = binding_of(fn)
    if b.artifact is None:
        return None
    return _static_file_url(b.artifact.entry)


def _next_nonce() -> int:
    return next(_nonce)


def _remember(nonce: int, name: str, calls_before: int, args: tuple[Any, ...], server_before: int = 0) -> None:
    with _dispatch_lock:
        _dispatched[nonce] = (name, calls_before, args, server_before)
        while len(_dispatched) > _DISPATCH_HISTORY:
            old, _ = _dispatched.popitem(last=False)
            _completed.discard(old)


def _recall(nonce: Any, name: str) -> tuple[int, tuple[Any, ...], int] | None:
    """The (calls_before, args, server_before) dispatched under `nonce` FOR THIS FUNCTION;
    None otherwise (a nonce dispatched for another kernel must never be answered with this
    one -- R2/S4). ONE snapshot under the lock: both path markers (`python_calls`,
    `server_calls`) derive from the same entry, so an eviction between two lookups can never
    turn `server_calls` into a lifetime total (opus m1.5 r1/S3)."""
    if not isinstance(nonce, int) or isinstance(nonce, bool):
        return None
    with _dispatch_lock:
        entry = _dispatched.get(nonce)
    if entry is None or entry[0] != name:
        return None
    return entry[1], entry[2], entry[3]


def _mark_completed(nonce: Any) -> None:
    if isinstance(nonce, int) and not isinstance(nonce, bool):
        with _dispatch_lock:
            _completed.add(nonce)


def is_completed(nonce: Any) -> bool:
    with _dispatch_lock:
        return nonce in _completed


_JS_SAFE_INT = 2**53 - 1


def _check_crossable(value: Any, where: str = "args") -> None:
    """M0's crossable ARGUMENT grammar: finite floats, ints within JS Number precision,
    bools, str, None, and lists of those. Anything JSON cannot carry exactly (inf/nan, a
    2**53+ int, arbitrary objects) is refused HERE with a clear error rather than being
    mangled on the wire (plan §7: watch the BigInt/Number drift at the FFI surface)."""
    if isinstance(value, bool) or value is None or isinstance(value, str):
        return
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError(f"{where}: non-finite float {value!r} cannot cross the JSON return path")
        return
    if isinstance(value, int):
        if abs(value) > _JS_SAFE_INT:
            raise ValueError(f"{where}: int {value!r} exceeds JS Number precision (|v| > 2**53-1)")
        return
    if isinstance(value, (list, tuple)):
        for i, v in enumerate(value):
            _check_crossable(v, f"{where}[{i}]")
        return
    raise TypeError(f"{where}: {type(value).__name__} is not a crossable argument type")


def _require_scalar_float_return(b: WasmBinding) -> tuple[list[str], str]:
    """ONE authority for the M0 scalar crossing contract shared by BOTH the Gradio and the
    Streamlit `dispatch` (issue #495). Read the kernel's signature (refusing anything outside
    the list-buffer FFI grammar) and refuse a crossed return type other than `float`.

    M0 admits exactly ONE crossed scalar return type. `_python_result` already refuses non-float
    on the FALLBACK path, so refusing HERE too makes the contract path-INDEPENDENT: a `-> None`
    kernel must not silently yield `nan` and a `-> int`/`-> bool` kernel must not yield a float on
    the browser path while the fallback raises TypeError (an asymmetric silent-wrong-value class).
    Both adapters call this so the rule lives in ONE place, not a divergent copy.

    NOT used by the image/Array path (`dispatch_image`), which crosses a scalar `int` pixel count
    under its own declared contract. Returns (param_types, return_type) for the payload."""
    from ..ffi import signature_of

    params, ret = signature_of(b)  # refuses a signature outside the FFI grammar
    if ret != "float":
        raise TypeError(
            f"`{b.name}`: @wasm kernels must return float in M0 (the one crossed scalar return "
            f"type), got return type {ret!r}"
        )
    return params, ret


def dispatch(fn: Callable[..., Any], *args: Any) -> dict[str, Any]:
    """Build the component value for one call. Browser path when an artifact is bound;
    otherwise the Python fallback runs HERE and the result is already in the payload."""
    b: WasmBinding = binding_of(fn)
    b.ensure_compiled()  # compile-on-first-call: no explicit build step needed to get a browser bundle (graceful: never raises)
    _require_scalar_float_return(b)  # #495: refuse a non-float crossed return at the ONE shared boundary
    _check_crossable(list(args))
    nonce = _next_nonce()
    calls_before, server_before = b.counts()  # one snapshot
    _remember(nonce, b.name, calls_before, tuple(args), server_before)
    payload: dict[str, Any] = {
        "fn": b.name,
        "args": list(args),
        "bundle": bundle_url(fn),
        "source_sha256": b.source_sha256,
        "artifact_status": b.artifact_status,
        "nonce": nonce,
        "result": None,
        "error": None,
    }
    if payload["bundle"] is None:
        payload["result"] = _python_result(fn, args, calls_before)
        _mark_completed(nonce)
    return payload


def _python_result(fn: Callable[..., Any], args: tuple[Any, ...], calls_before: int) -> dict[str, Any]:
    from ..build.runner import float_bits

    b = binding_of(fn)
    try:
        value = b.run_python(*args)  # the PYTHON BODY, explicitly and counted (M1.5: a bare call could run the server path)
    except Exception as e:  # the user's function raised: report it, do not break the app
        return {"value": None, "bits": None, "path": "python-fallback", "python_calls": b.calls() - calls_before,
                "error": f"{type(e).__name__}: {e}"}
    is_float = isinstance(value, float)
    if not is_float:  # M0 admits exactly one crossed return type (R2/N-d: refuse at OUR boundary)
        return {"value": None, "bits": None, "path": "python-fallback", "python_calls": b.calls() - calls_before,
                "error": f"TypeError: @wasm kernels must return float in M0, got {type(value).__name__}"}
    return {
        # JSON cannot carry inf/nan (Python would emit non-standard tokens the browser rejects):
        # the authoritative field is `bits`; `value` is a convenience for finite results only
        "value": value if (is_float and math.isfinite(value)) or isinstance(value, bool) or not is_float else None,
        "bits": float_bits(value) if is_float else None,
        "path": "python-fallback",
        "python_calls": b.calls() - calls_before,
    }


def result_of(fn: Callable[..., Any], payload: dict[str, Any] | None) -> dict[str, Any] | None:
    """Read a result back from the component value. None while the browser is still
    computing. Only SERVER-side state is trusted for the fallback re-run and the path
    marker; a browser error / malformed result / unknown nonce is answered with the Python
    fallback on the dispatched arguments so the app keeps working."""
    if not payload or not isinstance(payload, dict):
        return None
    b = binding_of(fn)
    nonce = payload.get("nonce")
    known = _recall(nonce, b.name)
    result = payload.get("result")
    if known is None:
        return {"value": None, "bits": None, "path": "unknown", "python_calls": None, "server_calls": None,
                "error": "unknown dispatch nonce for this function (stale client state); re-run"}
    calls_before, args, server_before = known
    calls_now, server_now = b.counts()  # one snapshot at result time too
    server_calls = server_now - server_before  # server-side truth since THIS dispatch
    if isinstance(result, dict) and result.get("path") == "python-fallback":
        _mark_completed(nonce)
        return {**result, "server_calls": server_calls}  # produced server-side by dispatch()
    if isinstance(result, dict) and result.get("path") == "browser-wasm":
        value = float_from_bits(result.get("bits"))
        if value is None:
            r = _python_result(fn, args, calls_before)
            r["browser_error"] = "malformed browser result (no valid bits)"
            r["server_calls"] = server_calls
            _mark_completed(nonce)
            return r
        r = dict(result)
        r["value"] = value  # reconstructed from bits: -0.0/inf/nan-exact, never the client's JSON number
        r["python_calls"] = calls_now - calls_before  # server-side truth since THIS dispatch, same snapshot as server_calls
        r["server_calls"] = server_calls
        _mark_completed(nonce)
        return r
    if payload.get("error"):
        r = _python_result(fn, args, calls_before)
        r["browser_error"] = str(payload["error"])
        r["server_calls"] = server_calls
        _mark_completed(nonce)
        return r
    return None


def deadline_result(fn: Callable[..., Any], payload: dict[str, Any] | None, wait_s: float = 20.0) -> dict[str, Any] | None:
    """Server-side deadline (R2/S8): wait up to `wait_s` for the browser to deliver the
    result for `payload`'s nonce; if nothing arrived (the component never mounted, JS off,
    the bundle threw during module evaluation) run the Python fallback on the DISPATCHED
    args and report browser_error='timeout'. Returns None when the browser already answered
    (the caller should leave the displayed result alone)."""
    if not payload or not isinstance(payload, dict):
        return None
    b = binding_of(fn)
    nonce = payload.get("nonce")
    known = _recall(nonce, b.name)
    if known is None or is_completed(nonce):
        return None
    deadline = time.monotonic() + max(0.0, wait_s)
    while time.monotonic() < deadline:
        if is_completed(nonce):
            return None
        time.sleep(0.25)
    if is_completed(nonce):
        return None
    calls_before, args, server_before = known
    server_calls = b.counts()[1] - server_before
    r = _python_result(fn, args, calls_before)
    r["browser_error"] = f"timeout: no browser result within {wait_s:g} s"
    r["server_calls"] = server_calls
    _mark_completed(nonce)
    return r


def describe(r: dict[str, Any] | None) -> str:
    if r is None:
        return "running in browser..."
    fetched = r.get("wasm_fetched")
    parts = [
        f"value={r.get('value')!r}",
        f"bits={r.get('bits')}",
        f"path={r.get('path')}",
        f"python_calls={r.get('python_calls')}",
    ]
    if fetched is not None:
        parts.append(f"wasm_fetched={len(fetched)}")
    if r.get("server_calls") is not None:
        parts.append(f"server_calls={r['server_calls']}")
    if r.get("browser_error"):
        parts.append(f"browser_error={r['browser_error']!r}")
    if r.get("error"):
        parts.append(f"error={r['error']!r}")
    return " ".join(parts)


from .image import dispatch_image, image_result_of, wasm_url  # noqa: E402  (M1; imports the private helpers above)
from .browser import Arg, ClientSide, client_side  # noqa: E402  (fix A: the in-tab scalar client; gradio-free import)
