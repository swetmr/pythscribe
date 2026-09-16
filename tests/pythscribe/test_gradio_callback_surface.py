"""V0 + V1 + S-r3-2 (M0 Python surface, spec `12-09-26-lib-gradio-callback-path`):

  V0    `FlowGraph` is DEFINED in the vendored `gradio_wasmfunction` package (so Gradio serves the
        vendored `templates/component/index.js`, not a 404 under pythscribe/gradio/). Paired control:
        a class defined outside `gradio_wasmfunction/` makes the same predicate RED.
  S-r3-2 the vendored backend stays pythscribe-free: `import gradio_wasmfunction` succeeds with
        `pythscribe` NOT on sys.path. Paired control: a pythscribe import in the backend -> RED.
  V1    per-shape return authority (S2) + param admission (B5c): a legal `-> int` comparator/cell is
        ADMITTED (not refused by the JSON-bits float-only rule); out-of-grammar returns/params are
        refused.
"""
from __future__ import annotations

import importlib.util
import inspect
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import pytest

from conftest import gate

REPO = Path(__file__).resolve().parents[2]
BACKEND = REPO / "pythscribe" / "gradio" / "wasm_function" / "backend"


def _worktree_gwf():
    """Import THIS worktree's gradio_wasmfunction (a sibling editable install may otherwise win)."""
    sys.path.insert(0, str(BACKEND))
    try:
        import gradio_wasmfunction  # noqa: F401
        import importlib
        importlib.reload(gradio_wasmfunction)
        return gradio_wasmfunction
    except ImportError as e:
        gate(False, f"gradio (or gradio_wasmfunction) unavailable: {e}")
    finally:
        try:
            sys.path.remove(str(BACKEND))
        except ValueError:
            pass


# ---- V0: FlowGraph is defined under gradio_wasmfunction/ ---------------------------------------

def test_v0_flowgraph_defined_under_gradio_wasmfunction():
    gwf = _worktree_gwf()
    f = Path(inspect.getfile(gwf.FlowGraph)).resolve()
    assert "gradio_wasmfunction" in f.parts, f"FlowGraph not under gradio_wasmfunction/: {f}"
    assert f == (BACKEND / "gradio_wasmfunction" / "wasmfunction.py").resolve(), f


def test_v0_negative_control_class_outside_package_is_red():
    """Paired control: a class defined OUTSIDE gradio_wasmfunction/ fails the same predicate (so a
    refactor that moved FlowGraph into pythscribe/gradio/callback.py would go RED before it 404s at
    mount). A plain class is used (NOT a Component subclass) so gradio does not emit a .pyi stub
    beside the test module as a side effect."""

    class _MovedElsewhere:  # defined HERE, in the test module (outside the vendored package)
        pass

    f = Path(inspect.getfile(_MovedElsewhere)).resolve()
    assert "gradio_wasmfunction" not in f.parts, "the control class must resolve OUTSIDE the package"
    # and the same predicate is GREEN for the real FlowGraph (positive/negative discrimination)
    gwf = _worktree_gwf()
    assert "gradio_wasmfunction" in Path(inspect.getfile(gwf.FlowGraph)).resolve().parts


# ---- S-r3-2: vendored backend is pythscribe-free ----------------------------------------------

def test_sr32_vendored_backend_imports_without_pythscribe():
    env = {**os.environ, "PYTHONPATH": str(BACKEND).replace("\\", "/"), "PYTHONUTF8": "1"}
    code = (
        "import gradio_wasmfunction as g, sys;"
        "print('OK' if (hasattr(g,'FlowGraph') and hasattr(g,'WasmFunction')) else 'MISSING');"
        "print('PYTHSCRIBE_LOADED' if 'pythscribe' in sys.modules else 'CLEAN')"
    )
    try:
        import gradio  # noqa: F401
    except ImportError as e:
        gate(False, f"gradio unavailable: {e}")
    r = subprocess.run([sys.executable, "-c", code], cwd=tempfile.gettempdir(), env=env, capture_output=True, text=True)
    out = r.stdout.strip()
    assert r.returncode == 0, f"import failed: {r.stderr[-800:]}"
    assert "OK" in out, out
    assert "CLEAN" in out, f"the vendored backend imported pythscribe (S-r3-2 violated): {out}"


# ---- V1: per-shape return authority + param admission -----------------------------------------

def _has_compiler() -> bool:
    from pythscribe.build import BuildError, find_pyths
    try:
        find_pyths()
        return True
    except BuildError:
        return False


KSRC = """
from pythscribe import wasm

@wasm
def gain(x: float, k: float) -> float:
    return x * k

@wasm
def cmp_i(a: int, b: int) -> int:
    return a - b

@wasm
def bucket(v: float) -> int:
    return 1

@wasm
def flag_cb(f: bool, x: float) -> float:
    return x

@wasm
def name(x: int) -> str:
    return "n"
"""


@pytest.fixture(scope="module")
def kmod(tmp_path_factory):
    if not _has_compiler():
        gate(False, "pyths compiler unavailable")
    import importlib.util
    d = tmp_path_factory.mktemp("cbsurface")
    src = d / "kmod.py"
    src.write_text(KSRC, encoding="utf-8")
    spec = importlib.util.spec_from_file_location("cb_surface_kmod", src)
    m = importlib.util.module_from_spec(spec)
    sys.modules["cb_surface_kmod"] = m
    spec.loader.exec_module(m)
    # SF-B: `client_callback` now REFUSES a kernel with no usable artifact at app-build time, so the
    # ADMITTED kernels need a real artifact: compile them once here with compile-on-first-call ON
    # (the suite's autouse fixture pins PYTHSCRIBE_NO_JIT=1 per test; the refused kernels need none).
    from pythscribe.decorators import binding_of
    prev = os.environ.get("PYTHSCRIBE_NO_JIT")
    os.environ["PYTHSCRIBE_NO_JIT"] = "0"
    try:
        for f in (m.gain, m.cmp_i, m.bucket):
            binding_of(f).ensure_compiled()
    finally:
        if prev is None:
            os.environ.pop("PYTHSCRIBE_NO_JIT", None)
        else:
            os.environ["PYTHSCRIBE_NO_JIT"] = prev
    return m


def _cc():
    from pythscribe.gradio.callback import client_callback
    return client_callback


def test_v1_node_admits_float(kmod):
    spec = _cc()(kmod.gain, shape="node")
    p = spec.to_payload()
    assert p["param_types"] == ["float", "float"] and p["return_type"] == "float" and p["shape"] == "node"
    assert p["source_sha256"] and isinstance(p["source_sha256"], str)


# ---- B2: the CallbackSpec is TRANSPORT-NEUTRAL (wasm=None; host attaches the transport) --------

def _fresh_gain(tmp_path_factory, monkeypatch):
    """A FRESH single-fn `@wasm gain` module compiled with JIT ON. The shared `kmod` fixture's
    bindings are created under the suite's default `PYTHSCRIBE_NO_JIT=1`, which latches
    `_jit_attempted=True` with no artifact; a fresh binding is needed so compile-on-first-call
    actually produces the `.wasm` the B2 spec records in `wasm_path`."""
    if not _has_compiler():
        gate(False, "pyths compiler unavailable")
    monkeypatch.delenv("PYTHSCRIBE_NO_JIT", raising=False)  # compile-on-first-call ON (real usage)
    d = tmp_path_factory.mktemp("b2gain")
    src = d / "b2mod.py"
    src.write_text("from pythscribe import wasm\n\n@wasm\ndef gain(x: float, k: float) -> float:\n    return x * k\n", encoding="utf-8")
    name = f"b2_kmod_{abs(hash(str(src)))}"
    spec = importlib.util.spec_from_file_location(name, src)
    m = importlib.util.module_from_spec(spec)
    sys.modules[name] = m
    spec.loader.exec_module(m)
    return m.gain


def test_b2_spec_is_transport_neutral(tmp_path_factory, monkeypatch):
    """B2: `client_callback` builds a neutral spec -- `wasm` is None (no eager served URL) and a
    `wasm_path` carries the artifact path for the HOST to resolve. The frontend reads only `wasm`;
    `wasm_path` is a server-side field the host consumes + strips."""
    gain = _fresh_gain(tmp_path_factory, monkeypatch)
    p = _cc()(gain, shape="node").to_payload()
    assert p["wasm"] is None, f"client_callback must NOT compute a transport URL (B2): {p}"
    assert isinstance(p.get("wasm_path"), str) and p["wasm_path"].endswith(".wasm"), p


def test_b2_host_attaches_transport_at_postprocess(tmp_path_factory, monkeypatch):
    """The Gradio host (FlowGraph.postprocess) turns the neutral `wasm_path` into a served
    `/gradio_api/file=...` URL and STRIPS the raw server path before the payload reaches the client.
    B-2 POSITIVE TWIN: a genuine SERVER-built spec's artifact IS registered as static (servable)."""
    from gradio.utils import is_static_file

    gain = _fresh_gain(tmp_path_factory, monkeypatch)
    gwf = _worktree_gwf()
    spec = _cc()(gain, shape="node")
    fg = gwf.FlowGraph(nodes=[{"id": "g", "type": "compute", "kernel": spec, "label": "gain"}], edges=[])
    out = fg.postprocess(fg.value)
    kern = out["nodes"][0]["kernel"]
    assert "wasm_path" not in kern, f"host must strip the server path: {kern}"
    assert isinstance(kern["wasm"], str) and kern["wasm"].startswith("/gradio_api/file="), kern
    assert kern["wasm"].endswith(".wasm"), kern
    assert is_static_file(Path(spec.wasm_path)), "the server-built artifact must be servable (positive twin of the B-2 SPOT)"
    # the constructor path too: Component.__init__ runs postprocess on the initial value
    assert "wasm_path" not in str(fg.get_config().get("value")), fg.get_config().get("value")


# ---- B-2 (SECURITY): a CLIENT-supplied `wasm_path` is NEVER served -----------------------------
# `FlowGraph` has EVENTS=[change], so `fg.change(fn, [fg], [fg])` runs `postprocess` on a value that
# came FROM THE BROWSER through `preprocess`. Before the fix, `_served_wasm_url` registered
# `Path(wasm_path).parent` as a Gradio static path for ANY value -> `GET /gradio_api/file=<dir>/<any>`
# served every file under an attacker-named directory (reproduced live, review 2026-09-13 P2).

def _crafted_echo(secret_dir: Path, *, existing: bool) -> dict:
    """The reviewer's PoC payload shape: a client-shaped flowgraph value naming a server path."""
    target = secret_dir / ("real.wasm" if existing else "whatever.wasm")
    return {
        "kind": "flowgraph",
        "nodes": [{"id": "g", "type": "compute", "label": "gain", "kernel": {
            "fn": "gain", "wasm": None, "param_types": ["float"], "return_type": "float", "shape": "node",
            "source_sha256": "x", "wasm_path": str(target)}}],
        "edges": [], "nonce": 0, "result": None, "error": None,
    }


def _make_secret_dir(tmp_path: Path) -> Path:
    d = tmp_path / "victim_home"
    d.mkdir()
    (d / "id_rsa").write_text("-----BEGIN PRIVATE KEY----- victim secret", encoding="utf-8")
    (d / "real.wasm").write_bytes(b"\0asm\1\0\0\0")  # an EXISTING file: existence must not grant trust
    return d


def _static_paths():
    from gradio.data_classes import _StaticFiles
    return _StaticFiles.all_paths


def test_b2_untrusted_client_wasm_path_is_never_served(tmp_path, tmp_path_factory, monkeypatch):
    """B-2 SPOT (the live PoC, in-process): a crafted value echoed `preprocess -> postprocess` must
    NOT register the attacker's directory as a static path, `is_static_file(<dir>/id_rsa)` stays
    False, the outgoing kernel has `wasm is None` and no `wasm_path` -- for BOTH a non-existent and
    an EXISTING attacker-named file (existence never grants trust). Meanwhile the SAME component's
    genuine server-built node still serves its real artifact (the positive twin, same process)."""
    from gradio.utils import is_static_file

    import gradio as gr

    gain = _fresh_gain(tmp_path_factory, monkeypatch)
    gwf = _worktree_gwf()
    secret = _make_secret_dir(tmp_path)
    spec = _cc()(gain, shape="node")
    with gr.Blocks():
        fg = gwf.FlowGraph(nodes=[{"id": "g", "type": "compute", "kernel": spec, "label": "gain"}], edges=[])
        fg.change(lambda v: v, [fg], [fg])  # the ordinary echo wiring that makes postprocess see CLIENT data
    before = list(_static_paths())
    for existing in (False, True):
        out = fg.postprocess(fg.preprocess(_crafted_echo(secret, existing=existing)))
        kern = out["nodes"][0]["kernel"]
        assert "wasm_path" not in kern, kern
        assert kern["wasm"] is None, f"an untrusted client path was given a transport: {kern}"
    after = list(_static_paths())
    assert secret.resolve() not in after and secret not in after, f"attacker dir registered as static: {after}"
    assert after == before, f"postprocess on client data changed the static paths: {before} -> {after}"
    assert is_static_file(secret / "id_rsa") is False
    assert is_static_file(secret / "real.wasm") is False
    # positive twin in the same process: the genuine node's artifact is served
    good = fg.postprocess(fg.value)["nodes"][0]["kernel"]
    assert isinstance(good["wasm"], str) and good["wasm"].startswith("/gradio_api/file=") and "wasm_path" not in good
    assert is_static_file(Path(spec.wasm_path)) is True


def test_b2_trust_walker_covers_grid_shaped_payloads(tmp_path, tmp_path_factory, monkeypatch):
    """CallbackGrid sibling (M1b) coverage BY CONSTRUCTION: the transport walker is payload-shape
    agnostic, so a grid-shaped value (`sort` comparator + `columns[*].cell` kernels, nested) through
    the SAME `_CallbackHost.postprocess` strips every client `wasm_path` (-> wasm None, nothing
    registered), while a server-serialized (`_kernel_payload`) spec in the same shape IS served."""
    from gradio.utils import is_static_file

    gain = _fresh_gain(tmp_path_factory, monkeypatch)
    gwf = _worktree_gwf()
    wf = gwf.wasmfunction
    secret = _make_secret_dir(tmp_path)
    spec = _cc()(gain, shape="node")
    crafted = {"kind": "grid", "rows": [[1], [2]],
               "sort": {"fn": "cmp", "wasm": None, "shape": "comparator", "wasm_path": str(secret / "real.wasm")},
               "columns": [{"key": "v", "cell": {"fn": "cell", "wasm": None, "shape": "cell", "wasm_path": str(secret / "whatever.wasm")}}],
               "nested": {"deeper": [{"kernel": {"wasm_path": str(secret / "id_rsa")}}]}}
    before = list(_static_paths())
    out = wf._attach_transport(crafted)
    assert out["sort"]["wasm"] is None and "wasm_path" not in out["sort"]
    assert out["columns"][0]["cell"]["wasm"] is None and "wasm_path" not in out["columns"][0]["cell"]
    assert "wasm_path" not in out["nested"]["deeper"][0]["kernel"] and out["nested"]["deeper"][0]["kernel"]["wasm"] is None
    assert "wasm_path" not in repr(out)
    assert list(_static_paths()) == before and is_static_file(secret / "id_rsa") is False
    # a server-side spec object serialized through the ONE trust point, in the grid shape -> served
    served = wf._attach_transport({"kind": "grid", "sort": wf._kernel_payload(spec)})
    assert isinstance(served["sort"]["wasm"], str) and served["sort"]["wasm"].startswith("/gradio_api/file=")
    assert "wasm_path" not in served["sort"]


def test_b2_negative_control_mutant_without_trust_check_registers_attacker_dir(tmp_path, tmp_path_factory, monkeypatch):
    """PAIRED NEGATIVE CONTROL (anti-vacuity, d'): a text MUTANT of `wasmfunction.py` with the trust
    check DELETED (imported from a temp copy, no production flag) makes the SAME crafted echo
    register the attacker's directory (`is_static_file(<dir>/id_rsa)` -> True, `wasm` set) -> proving
    the `_is_trusted_wasm_path` check is load-bearing. The registration is undone afterwards."""
    from gradio.utils import is_static_file

    _fresh_gain(tmp_path_factory, monkeypatch)  # compiler present (same gate as the SPOT)
    _worktree_gwf()
    src_path = BACKEND / "gradio_wasmfunction" / "wasmfunction.py"
    src = src_path.read_text(encoding="utf-8")
    mutated = src.replace(
        "    if rp is None or not _is_trusted_wasm_path(rp):\n        return None",
        "    if rp is None:\n        return None",
        1,
    )
    assert mutated != src, "the trust check must be present to delete (else the control is vacuous)"
    mdir = tmp_path / "mutant_pkg"
    mdir.mkdir()
    mfile = mdir / "wasmfunction_mutant.py"
    mfile.write_text(mutated, encoding="utf-8")
    spec = importlib.util.spec_from_file_location("gradio_wasmfunction_mutant_b2", mfile)
    mut = importlib.util.module_from_spec(spec)
    sys.modules["gradio_wasmfunction_mutant_b2"] = mut
    try:
        spec.loader.exec_module(mut)
        secret = _make_secret_dir(tmp_path)
        assert is_static_file(secret / "id_rsa") is False
        paths = _static_paths()
        before = list(paths)
        try:
            out = mut._attach_transport(_crafted_echo(secret, existing=True))
            kern = out["nodes"][0]["kernel"]
            assert isinstance(kern["wasm"], str) and kern["wasm"].startswith("/gradio_api/file="), (
                f"the MUTANT (trust check deleted) must attach the untrusted path: {kern}"
            )
            assert is_static_file(secret / "id_rsa") is True, "the MUTANT must register the attacker dir (else the control is vacuous)"
        finally:
            paths[:] = before  # undo the mutant's registration (module-global gradio state)
        assert is_static_file(secret / "id_rsa") is False
    finally:
        sys.modules.pop("gradio_wasmfunction_mutant_b2", None)


def test_sfb_missing_artifact_is_refused_at_build_time(tmp_path, monkeypatch):
    """SF-B (spec §6): a kernel with NO usable artifact is refused by `client_callback` with a clear
    RuntimeError naming the build step (app-build time), not a silent `wasm_path=None` that errors
    only in the tab. Distinguished from the B-2 client-echo arm, which stays silent -> `wasm=None`."""
    monkeypatch.setenv("PYTHSCRIBE_NO_JIT", "1")  # no compile-on-first-call -> no artifact
    src = tmp_path / "sfb_mod.py"
    src.write_text("from pythscribe import wasm\n\n@wasm\ndef gain(x: float, k: float) -> float:\n    return x * k\n", encoding="utf-8")
    spec = importlib.util.spec_from_file_location("sfb_kmod", src)
    m = importlib.util.module_from_spec(spec)
    sys.modules["sfb_kmod"] = m
    spec.loader.exec_module(m)
    with pytest.raises(RuntimeError, match=r"no usable artifact.*pythscribe\.build"):
        _cc()(m.gain, shape="node")


def test_b2_client_callback_works_with_gradio_absent(kmod):
    """B2 PAIRED CONTROL (the load-bearing claim that unblocks Streamlit): `import pythscribe.gradio`
    + `client_callback` on a COMPILED kernel work with `gradio` NOT importable. A subprocess blocks
    gradio (`sys.modules['gradio']=None`) and:
      NEUTRAL_OK  -- client_callback returns a spec with wasm=None + wasm_path set (no import gradio);
      EAGER_RED   -- the pre-B2 eager path (`image.wasm_url`, which computes a served URL ->
                     `import gradio`) RAISES under the same block -> proving the neutral spec's
                     avoidance of the eager URL is exactly what makes gradio-absent work (non-vacuous).
    """
    if not _has_compiler():
        gate(False, "pyths compiler unavailable")
    code = r'''
import sys, os, tempfile, importlib.util
sys.modules["gradio"] = None  # simulate gradio NOT on sys.path: `import gradio` -> ImportError
KSRC = "from pythscribe import wasm\n\n@wasm\ndef gain(x: float, k: float) -> float:\n    return x * k\n"
d = tempfile.mkdtemp(); p = os.path.join(d, "k_b2.py"); open(p, "w").write(KSRC)
sp = importlib.util.spec_from_file_location("k_b2", p); m = importlib.util.module_from_spec(sp)
sys.modules["k_b2"] = m; sp.loader.exec_module(m)
from pythscribe.gradio import client_callback           # gradio-free import
cb = client_callback(m.gain, shape="node"); pl = cb.to_payload()
assert pl["wasm"] is None, pl
assert isinstance(pl.get("wasm_path"), str) and pl["wasm_path"].endswith(".wasm"), pl
try:
    import gradio  # noqa: F401
    print("CONTROL_VACUOUS: gradio was importable"); sys.exit(3)
except ImportError:
    pass
print("NEUTRAL_OK")
from pythscribe.gradio.image import wasm_url            # the pre-B2 eager path
try:
    u = wasm_url(m.gain); print("EAGER_GREEN_BAD", u)   # would mean the eager path did NOT need gradio
except Exception as e:
    print("EAGER_RED", type(e).__name__)
'''
    env = {**os.environ, "PYTHONPATH": str(REPO).replace("\\", "/"), "PYTHONUTF8": "1"}
    env["PYTHSCRIBE_NO_JIT"] = "0"  # compile-on-first-call ON in the subprocess (the suite defaults it off)
    r = subprocess.run([sys.executable, "-c", code], cwd=tempfile.gettempdir(), env=env, capture_output=True, text=True)
    out = r.stdout.strip()
    assert r.returncode == 0, f"gradio-absent surface crashed: rc={r.returncode}\nSTDOUT:{out}\nSTDERR:{r.stderr[-1500:]}"
    assert "NEUTRAL_OK" in out, f"client_callback did not work gradio-absent: {out}"
    assert "EAGER_RED" in out, f"the eager-URL mutant did NOT fail gradio-absent (control vacuous): {out}"


def test_v1_return_authority_int_admitted_not_refused(kmod):
    # S2 over-refusal control: a legal `-> int` comparator/cell must be ADMITTED (the #495 float-only
    # rule belongs to the JSON-bits path, which the callback path does not cross).
    cc = _cc()
    assert cc(kmod.cmp_i, shape="comparator").to_payload()["return_type"] == "int"
    assert cc(kmod.bucket, shape="cell").to_payload()["return_type"] == "int"


def test_v1_refusals(kmod):
    from pythscribe.ffi import FfiError
    cc = _cc()
    with pytest.raises(TypeError):  # bool param refused (B5c)
        cc(kmod.flag_cb, shape="node")
    with pytest.raises((TypeError, FfiError)):  # -> str out of grammar (loud refusal)
        cc(kmod.name, shape="node")
    with pytest.raises(TypeError):  # comparator arity must be 2
        cc(kmod.bucket, shape="comparator")
    with pytest.raises(ValueError):  # unknown shape
        cc(kmod.gain, shape="widget")
