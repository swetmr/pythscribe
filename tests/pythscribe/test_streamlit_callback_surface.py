"""M0 unit/surface gates for the v0.2.5 Streamlit callback path (no browser needed):

  * V-neutral / SF-D -- `wasm_b64` is derived ONLY from the SERVER-side `CallbackSpec` object,
    never from a client-shaped value; a client echo cannot inject a wasm path (paired mutant RED).
    codex SF-7: the control is run against the PRODUCTION TRANSPORT -- a genuine server spec
    ALONGSIDE a crafted client echo in `session_state[key]`, capturing the ACTUAL outgoing component
    payload; a source-level mutant of the shipped `slider_compute` that honours the echo puts the
    client-named bytes on the wire -> RED.
  * V0 -- `slider_compute` admission: shape/arity/int-step/`key`-required refusals; a legal
    `-> int` slider kernel is ADMITTED (over-refusal control). codex SF-5: an int range outside JS
    safe-integer bounds is REFUSED up front, at the exact bound the twin shim's `toI64` enforces
    (driven through the twin: 2**53 RangeErrors, 2**53-1 crosses).
  * V-shim (M-1) -- a raising scalar kernel driven through the STREAMLIT TWIN FILE
    (`pythscribe/streamlit/wasm_component/list_buffer.mjs`, NOT the `ffi/` copy) THROWS with the
    mapped `.name`, it does not return the compiler sentinel `0.0`; paired mutant (the `__err_code`
    check removed) RETURNS the sentinel -> RED. This is the base-branch shim dependency, asserted.

Provenance (codex SF-8): a missing demo artifact goes through `gate()` -- a FAILURE under
PYTHSCRIBE_REQUIRE_ORACLE=1, never a silent skip.
"""
from __future__ import annotations

import base64
import inspect
import json
import re
import shutil
import subprocess
import tempfile
from pathlib import Path
from types import SimpleNamespace

import pytest

from conftest import REPO, gate, gate_node, import_module_from

import pythscribe.streamlit as S
from pythscribe.streamlit import CallbackSpec, client_callback

CALLBACK_DIR = REPO / "examples" / "streamlit-callback"
_STREAMLIT_SHIM = REPO / "pythscribe" / "streamlit" / "wasm_component" / "list_buffer.mjs"


def _spec(shape="node", param_types=("float",), return_type="float", wasm_path=None):
    return CallbackSpec(
        fn="k", wasm=None, param_types=list(param_types), return_type=return_type,
        shape=shape, source_sha256="sha", render=None, wasm_path=wasm_path,
    )


@pytest.fixture(scope="module")
def compiled_response():
    mod = import_module_from(CALLBACK_DIR / "kernels.py", "streamlit_cb_kernels")
    from pythscribe import binding_of
    b = binding_of(mod.response)
    # codex SF-8: the committed demo artifact is a REQUIRED oracle prerequisite -- under
    # PYTHSCRIBE_REQUIRE_ORACLE=1 its absence FAILS (gate), it never silently skips.
    gate(b.artifact is not None,
         f"streamlit callback demo artifact not built (status={b.artifact_status}); run "
         "`python -m pythscribe.build examples/streamlit-callback/kernels.py`")
    return mod.response


# ---- codex SF-8: the provenance gate is a FAILURE under REQUIRE_ORACLE, a skip without it -----

_SELF = Path(__file__).resolve()
_SF8_TARGET = f"{_SELF}::test_sfd_wasm_b64_from_server_spec_only"


def _run_self_without_artifact(extra_env: dict[str, str]) -> subprocess.CompletedProcess:
    """Run one artifact-dependent gate of THIS file in a subprocess with the demo artifact DISABLED
    (`PYTHSCRIBE_DISABLE_ARTIFACTS=1` -> `binding_of(...).artifact is None`)."""
    import os
    import sys
    env = {**os.environ, "PYTHSCRIBE_DISABLE_ARTIFACTS": "1"}
    env.pop("PYTHSCRIBE_REQUIRE_ORACLE", None)
    env.update(extra_env)
    return subprocess.run(
        [sys.executable, "-m", "pytest", _SF8_TARGET, "-q", "-ra", "-p", "no:cacheprovider"],
        cwd=str(REPO), env=env, capture_output=True, text=True, check=False,
    )


def test_sf8_missing_artifact_is_red_under_require_oracle():
    """PAIRED CONTROL (codex SF-8): with the demo artifact missing and PYTHSCRIBE_REQUIRE_ORACLE=1
    the provenance gate must FAIL (the old fixture `pytest.skip`-ped unconditionally, so a CI run
    with no artifact reported green with its oracle skipped)."""
    from conftest import GATE_MISSING
    proc = _run_self_without_artifact({"PYTHSCRIBE_REQUIRE_ORACLE": "1"})
    assert proc.returncode != 0, proc.stdout[-2000:]
    assert GATE_MISSING in proc.stdout, proc.stdout[-2000:]
    assert "1 error" in proc.stdout or "1 failed" in proc.stdout, proc.stdout[-2000:]


def test_sf8_missing_artifact_skips_without_require_oracle():
    proc = _run_self_without_artifact({})
    assert proc.returncode == 0, proc.stdout[-2000:]
    assert "1 skipped" in proc.stdout, proc.stdout[-2000:]


# ---- V-neutral / SF-D: transport ONLY from the server-side spec ------------------------------

def test_sfd_wasm_b64_from_server_spec_only(compiled_response, tmp_path):
    spec = client_callback(compiled_response, shape="node")
    # transport-neutral: client_callback attaches NO transport (B2)
    assert spec.wasm is None and spec.wasm_path, spec
    # positive: wasm_b64 is base64 of the SERVER-side spec's own artifact bytes
    b64 = S._callback_wasm_b64(spec)
    assert base64.b64decode(b64) == Path(spec.wasm_path).read_bytes()
    assert base64.b64decode(b64)[:4] == b"\x00asm"

    # a crafted CLIENT value carrying a secret path (an EXISTING file with sentinel bytes)
    secret = tmp_path / "secret.wasm"
    secret.write_bytes(b"\x00asm" + b"SF-D-SECRET-BYTES-MUST-NEVER-BE-SERVED")
    client_echo = {"fn": "k", "wasm_path": str(secret), "source_sha256": "sha"}

    # SF-D structural guard: _callback_wasm_b64 accepts ONLY a CallbackSpec object; a dict / any
    # client-shaped value is refused -> the secret bytes can never be read here.
    with pytest.raises(TypeError):
        S._callback_wasm_b64(client_echo)
    # slider_compute likewise refuses a non-CallbackSpec spec before any byte read
    with pytest.raises(TypeError):
        S.slider_compute(client_echo, min=0.0, max=10.0, step=0.1, key="k")

    # PAIRED MUTANT (observed RED): a host that read `wasm_path` from the CLIENT dict instead of the
    # server spec object WOULD emit the secret bytes -> proving the "spec object only" rule is
    # load-bearing (the real function never yields the secret).
    def _mutant_reads_client(client: dict) -> str:
        return base64.b64encode(Path(client["wasm_path"]).read_bytes()).decode("ascii")

    assert base64.b64decode(_mutant_reads_client(client_echo)) == secret.read_bytes()  # mutant leaks
    assert base64.b64decode(b64) != secret.read_bytes()                                # real never does


def _capturing_streamlit(captured: list) -> SimpleNamespace:
    """A stand-in for the `streamlit` module at the ONE seam `slider_compute` touches it
    (`require()`): `components.v1.declare_component` returns a component whose CALL records the
    outgoing kwargs -- the ACTUAL wire payload the iframe would receive -- and returns None (what
    the real keyed component returns before the iframe publishes). `session_state` is the
    client-returned widget state a malicious tab can shape."""
    def declare_component(name, path=None, url=None):
        def comp(**kwargs):
            captured.append(kwargs)
            return None
        return comp
    return SimpleNamespace(
        components=SimpleNamespace(v1=SimpleNamespace(declare_component=declare_component)),
        session_state={},
    )


def test_sfd_wire_payload_never_carries_client_named_bytes(compiled_response, tmp_path, monkeypatch):
    """codex SF-7: the SF-D control against the PRODUCTION TRANSPORT. A GENUINE server spec is
    rendered while `session_state[key]` holds a crafted client echo naming a secret file; the
    CAPTURED outgoing component payload must carry the spec's own artifact bytes and never the
    client-named ones. PAIRED MUTANT: the SHIPPED `slider_compute` source with one inserted arm
    (override the outgoing bytes from `session_state[key]["wasm_path"]` when present) -- run
    through the SAME capture -- puts the secret bytes on the wire -> RED."""
    spec = client_callback(compiled_response, shape="node")
    genuine = Path(spec.wasm_path).read_bytes()
    secret = tmp_path / "secret.wasm"
    secret.write_bytes(b"\x00asm" + b"SF-D-SECRET-BYTES-MUST-NEVER-BE-SERVED")

    captured: list[dict] = []
    fake_st = _capturing_streamlit(captured)
    # the crafted client echo under the widget key, shaped like a component-returned payload
    fake_st.session_state["flagship"] = {"fn": spec.fn, "kind": "callback", "source_sha256": spec.source_sha256,
                                         "wasm_path": str(secret)}
    monkeypatch.setattr(S, "require", lambda: fake_st)
    monkeypatch.setattr(S, "_component", None)  # (re)declare through the capturing streamlit

    # SHIPPED path: the actual outgoing payload
    assert S.slider_compute(spec, min=0.0, max=10.0, step=0.1, key="flagship") is None
    assert len(captured) == 1 and captured[0]["key"] == "flagship"
    wire = captured[0]["payload"]
    assert base64.b64decode(wire["wasm_b64"]) == genuine
    assert b"SF-D-SECRET" not in base64.b64decode(wire["wasm_b64"])
    assert "wasm_path" not in wire  # the server-side field never reaches the client

    # PAIRED MUTANT: the shipped function's SOURCE with the client-override arm inserted after the
    # server-spec read, exec'd in a copy of the module namespace, driven through the SAME capture.
    src = inspect.getsource(S.slider_compute)
    anchor = "    wasm_b64 = _callback_wasm_b64(spec)\n"
    assert src.count(anchor) == 1, "mutant anchor must be present exactly once (else vacuous)"
    mutated = src.replace(anchor, anchor + (
        '    _echo = require().session_state.get(key)\n'
        '    if isinstance(_echo, dict) and _echo.get("wasm_path"):\n'
        '        wasm_b64 = base64.b64encode(Path(_echo["wasm_path"]).read_bytes()).decode("ascii")\n'
    ), 1)
    ns = dict(vars(S))
    exec(compile(mutated, "<slider_compute-mutant>", "exec"), ns)
    captured.clear()
    ns["slider_compute"](spec, min=0.0, max=10.0, step=0.1, key="flagship")
    assert len(captured) == 1
    leaked = base64.b64decode(captured[0]["payload"]["wasm_b64"])
    assert leaked == secret.read_bytes() and leaked != genuine, "the mutant must put the client-named bytes on the wire"


# ---- V0: slider_compute admission ------------------------------------------------------------

def test_v0_refuses_non_node_shape():
    with pytest.raises(TypeError):
        S.slider_compute(_spec(shape="comparator", param_types=("float", "float")), min=0.0, max=1.0, step=0.1, key="k")


def test_v0_refuses_arity_not_one():
    with pytest.raises(TypeError):
        S.slider_compute(_spec(param_types=("float", "float")), min=0.0, max=1.0, step=0.1, key="k")


def test_v0_requires_key():
    with pytest.raises(TypeError):
        S.slider_compute(_spec(), min=0.0, max=1.0, step=0.1)  # type: ignore[call-arg]


def test_v0_int_param_needs_integral_step():
    with pytest.raises(ValueError):
        S.slider_compute(_spec(param_types=("int",), return_type="int"), min=0.0, max=10.0, step=0.1, key="k")


def test_v0_legal_int_kernel_is_admitted():
    # over-refusal control (S2): a legal `-> int` slider kernel with integral min/max/step passes
    # admission -> it reaches the wasm_path read (RuntimeError: no wasm_path on this synthetic spec),
    # NOT a ValueError/TypeError from admission.
    with pytest.raises(RuntimeError):
        S.slider_compute(_spec(param_types=("int",), return_type="int"), min=0, max=10, step=1, key="k")


def test_v0_refuses_client_shaped_spec():
    with pytest.raises(TypeError):
        S.slider_compute({"fn": "k", "shape": "node"}, min=0.0, max=1.0, step=0.1, key="k")  # type: ignore[arg-type]


def test_v0_int_range_refuses_unsafe_integers():
    """codex SF-5: an int kernel with a range outside JS safe-integer bounds used to PASS admission
    and then RangeError on every drag (the frontend Number-converts each position, the shim's
    `toI64` refuses |v| > 2**53-1). Refused up front, naming the bound."""
    ispec = _spec(param_types=("int",), return_type="int")
    for kw in ({"min": 2**53, "max": 2**53 + 2, "step": 2},       # codex's reproducer
               {"min": 0, "max": 2**53, "step": 1},               # max just past the bound
               {"min": -(2**53), "max": 0, "step": 1},            # negative side
               {"min": 0, "max": 10, "step": 2**53}):             # step, too
        with pytest.raises(ValueError, match="safe integer"):
            S.slider_compute(ispec, key="k", **kw)


def test_v0_int_range_at_safe_bound_is_admitted():
    """over-refusal control: the largest safe range passes admission (reaches the wasm_path read)."""
    with pytest.raises(RuntimeError):
        S.slider_compute(_spec(param_types=("int",), return_type="int"),
                         min=-(2**53 - 1), max=2**53 - 1, step=1, key="k")


def test_v0_refuses_non_finite_bounds():
    for kw in ({"min": float("nan"), "max": 1.0, "step": 0.1},
               {"min": 0.0, "max": float("inf"), "step": 0.1},
               {"min": 0.0, "max": 1.0, "step": float("nan")}):
        with pytest.raises(ValueError, match="finite"):
            S.slider_compute(_spec(), key="k", **kw)
    with pytest.raises(TypeError):
        S.slider_compute(_spec(), min=True, max=1.0, step=0.1, key="k")  # type: ignore[arg-type]


# ---- V-shim (M-1): the STREAMLIT TWIN throws on a raising kernel (not the sentinel) -----------

_RUNNER_MJS = r"""
import { instantiate, call } from "./list_buffer.mjs";
const [, , wasmPath, reqPath] = process.argv;
const k = await instantiate(wasmPath);
const req = JSON.parse(await (await import("node:fs/promises")).readFile(reqPath, "utf8"));
const out = [];
for (const c of req) {
  try { const r = call(k, c.fn, c.param_types, c.args, { returnType: c.return_type });
        out.push({ ok: true, value: String(r.value) }); }
  catch (e) { out.push({ ok: false, name: e && e.name, code: e && e.code }); }
}
process.stdout.write(JSON.stringify(out));
"""

_KERNEL_SRC = """
def response(x: float) -> float:
    if x < 0.0:
        raise ValueError("negative")
    return x * 2.0
"""


_INT_KERNEL_SRC = """
def half(n: int) -> int:
    return n // 2
"""


def _build(tmp_path_factory, label: str, src_text: str) -> Path:
    gate_node()
    from pythscribe.build import BuildError, build_kernel, find_pyths
    try:
        pyths = find_pyths()
    except BuildError as e:
        gate(False, str(e))
    d = tmp_path_factory.mktemp(label)
    src = d / "k.ps"
    src.write_text(src_text)
    info = build_kernel(src, "k", src_text, pyths=pyths, force=True, quiet=True)
    return next(info.dir.glob("*.wasm"))


@pytest.fixture(scope="module")
def raising_wasm(tmp_path_factory):
    return _build(tmp_path_factory, "verr_twin", _KERNEL_SRC)


@pytest.fixture(scope="module")
def int_wasm(tmp_path_factory):
    return _build(tmp_path_factory, "int_twin", _INT_KERNEL_SRC)


def _run_through(shim_text: str, wasm: Path, calls: list[dict]) -> list[dict]:
    node = shutil.which("node")
    with tempfile.TemporaryDirectory() as td:
        tdp = Path(td)
        (tdp / "list_buffer.mjs").write_text(shim_text, encoding="utf-8")
        (tdp / "_runner.mjs").write_text(_RUNNER_MJS, encoding="utf-8")
        req = tdp / "req.json"
        req.write_text(json.dumps(calls), encoding="utf-8")
        r = subprocess.run([node, str(tdp / "_runner.mjs"), str(wasm), str(req)], capture_output=True, text=True)
        assert r.returncode == 0, f"runner failed: {r.stderr[-1200:]}"
        return json.loads(r.stdout)


def test_m1_streamlit_twin_throws_on_raising_kernel(raising_wasm):
    """The base-branch shim dependency, asserted through the TWIN FILE (not the ffi/ copy): a
    raising kernel THROWS `.name=='ValueError'`, it does NOT return the compiler sentinel 0.0."""
    twin = _STREAMLIT_SHIM.read_text(encoding="utf-8")
    calls = [
        {"fn": "response", "param_types": ["float"], "return_type": "float", "args": [-1.0]},  # raises
        {"fn": "response", "param_types": ["float"], "return_type": "float", "args": [3.0]},    # ok -> 6
    ]
    res = _run_through(twin, raising_wasm, calls)
    assert res[0]["ok"] is False and res[0]["name"] == "ValueError", res[0]
    assert res[1]["ok"] is True and res[1]["value"] == "6", res[1]


def test_sf5_twin_refuses_unsafe_int_position(int_wasm):
    """codex SF-5, the bound PINNED to the shipped twin (not a constant): a slider position of
    2**53 (what the frontend's `Number(cbSlider.value)` would produce for an unsafe range) is
    REFUSED by the twin's `toI64` with a RangeError, while 2**53-1 crosses exactly. This is the
    per-drag failure the up-front admission refusal now prevents, and `_JS_SAFE_INT` == the twin's
    bound by construction."""
    twin = _STREAMLIT_SHIM.read_text(encoding="utf-8")
    calls = [
        {"fn": "half", "param_types": ["int"], "return_type": "int", "args": [2**53]},
        {"fn": "half", "param_types": ["int"], "return_type": "int", "args": [S._JS_SAFE_INT]},
    ]
    res = _run_through(twin, int_wasm, calls)
    assert res[0]["ok"] is False and res[0]["name"] == "RangeError", res[0]
    assert res[1]["ok"] is True and res[1]["value"] == str(S._JS_SAFE_INT // 2), res[1]
    assert S._JS_SAFE_INT == 2**53 - 1


def test_m1_negative_control_mutant_twin_returns_sentinel(raising_wasm):
    """PAIRED CONTROL: a mutant copy of the TWIN with the `__err_code` check deleted RETURNS the
    sentinel (0.0), proving the real check is load-bearing (the throw is not vacuous)."""
    twin = _STREAMLIT_SHIM.read_text(encoding="utf-8")
    mutated = re.sub(
        r"\n    if \(ex\.__err_code && ex\.__err_code\.value\) \{\n(?:.*\n)*?      throw e;\n    \}\n",
        "\n",
        twin,
        count=1,
    )
    assert mutated != twin, "the __err_code check block must be present to remove (else vacuous)"
    real = _run_through(twin, raising_wasm, [{"fn": "response", "param_types": ["float"], "return_type": "float", "args": [-1.0]}])
    mut = _run_through(mutated, raising_wasm, [{"fn": "response", "param_types": ["float"], "return_type": "float", "args": [-1.0]}])
    assert real[0]["ok"] is False, f"real twin must throw: {real[0]}"
    assert mut[0]["ok"] is True and mut[0]["value"] == "0", f"mutant must return the sentinel 0.0: {mut[0]}"
