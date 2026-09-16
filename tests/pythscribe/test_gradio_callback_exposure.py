"""Pre-launch trust/info-exposure controls for the Gradio callback path (issues #505 + #506).

Both bugs live in `gradio_wasmfunction.wasmfunction`; both controls are HERMETIC (a fake artifact
dir + a directly-constructed `CallbackSpec`), so they run WITHOUT the pyths compiler -- the exposure
is a property of the host's serialization/registration, not of any compiled kernel.

  #505  `_attach_transport` must route an EMBEDDED `CallbackSpec` OBJECT (not its `.to_payload()`
        dict) through the ONE trust authority, so its raw `wasm_path` (the server cache-dir path) is
        NEVER serialized on the wire. Paired negative control: revert `_attach_transport` to the
        scalar pass-through -> the dataclass survives, orjson emits `wasm_path` -> RED.
  #506  `_served_wasm_url` must register the `.wasm` FILE, not `rp.parent` (which also holds the
        kernel SOURCE `*.ps`, `manifest.json`, the `*.js`/`*.glue.js` glue). Paired negative control:
        revert to `set_static_paths([rp.parent])` -> the `.ps` source becomes network-readable -> RED.

Both controls assert their MUTANT is bad (proving the fix is load-bearing) AND their FIXED behavior
is good, in the same file. The gradio global `_StaticFiles.all_paths` is snapshotted and restored.
"""
from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

import orjson

from conftest import gate

REPO = Path(__file__).resolve().parents[2]
BACKEND = REPO / "pythscribe" / "gradio" / "wasm_function" / "backend"
SRC_PATH = BACKEND / "gradio_wasmfunction" / "wasmfunction.py"


def _wf():
    """This worktree's `gradio_wasmfunction.wasmfunction` submodule (holds the private helpers)."""
    try:
        import gradio  # noqa: F401
    except ImportError as e:
        gate(False, f"gradio unavailable: {e}")
    sys.path.insert(0, str(BACKEND))
    try:
        import gradio_wasmfunction
        importlib.reload(gradio_wasmfunction)
        return gradio_wasmfunction.wasmfunction
    except ImportError as e:
        gate(False, f"gradio_wasmfunction unavailable: {e}")
    finally:
        try:
            sys.path.remove(str(BACKEND))
        except ValueError:
            pass


def _callback_spec(wasm_path: str):
    """A genuine SERVER-built `CallbackSpec` OBJECT (gradio-free import)."""
    from pythscribe.gradio.callback import CallbackSpec
    return CallbackSpec(fn="k", wasm=None, param_types=["float"], return_type="float",
                        shape="node", source_sha256="abc", wasm_path=wasm_path)


def _artifact_dir(tmp_path: Path, *, with_source: bool = True) -> tuple[Path, Path]:
    """A fake artifact dir: a real `.wasm` plus the sibling SOURCE / build files a real build writes."""
    d = tmp_path / "__pythscribe__" / "kernel"
    d.mkdir(parents=True)
    wasm = d / "kernel.wasm"
    wasm.write_bytes(b"\0asm\1\0\0\0")  # a real file so `_served_wasm_url`'s is_file() guard passes
    if with_source:
        (d / "kernel.ps").write_text("from pythscribe import wasm\n@wasm\ndef k(x: float) -> float: return x\n", encoding="utf-8")
        (d / "manifest.json").write_text('{"source_sha256": "abc", "build": "secret-buildinfo"}', encoding="utf-8")
        (d / "kernel.js").write_text("// compiled JS glue (source-revealing)", encoding="utf-8")
        (d / "kernel.glue.js").write_text("// wasm glue (source-revealing)", encoding="utf-8")
        rt = d / "pyths-runtime"
        rt.mkdir()
        (rt / "index.js").write_text("// runtime", encoding="utf-8")
    return d, wasm


def _static_paths():
    from gradio.data_classes import _StaticFiles
    return _StaticFiles.all_paths


def _load_mutant(tmp_path: Path, old: str, new: str, tag: str):
    """Load a TEXT-mutant of wasmfunction.py (a fresh module with its own globals). Asserts the target
    text exists (else the control is vacuous)."""
    src = SRC_PATH.read_text(encoding="utf-8")
    mutated = src.replace(old, new, 1)
    assert mutated != src, f"mutation target not found (control vacuous): {old!r}"
    mfile = tmp_path / f"wasmfunction_mutant_{tag}.py"
    mfile.write_text(mutated, encoding="utf-8")
    name = f"gwf_mutant_{tag}"
    spec = importlib.util.spec_from_file_location(name, mfile)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    spec.loader.exec_module(mod)
    return mod, name


# ------------------------------------------------------------------------------------------------
# #505 -- an EMBEDDED CallbackSpec OBJECT must never leak `wasm_path` on the wire
# ------------------------------------------------------------------------------------------------

def test_505_embedded_object_strips_wasm_path_at_any_depth(tmp_path):
    """FIXED behavior: a raw handler dict that embeds a `CallbackSpec` OBJECT (not its payload dict)
    -- at the top, as a bare list element, and nested deep -- is walked so the serialized wire
    payload (orjson, as Gradio's ORJSONResponse would emit) carries NO `wasm_path` anywhere and a
    resolved trusted `wasm` URL. The plain CLIENT-dict twin (a dict carrying `wasm_path`) still
    yields `wasm: None` + no `wasm_path` (untrusted)."""
    wf = _wf()
    _, wasm = _artifact_dir(tmp_path)
    spec = _callback_spec(str(wasm))
    paths = _static_paths()
    before = list(paths)
    try:
        raw = {
            "kind": "flowgraph",
            "nodes": [{"id": "k", "type": "compute", "kernel": spec}],  # embedded OBJECT (the #505 shape)
            "bare": [spec],                                             # object as a bare LIST element
            "deep": {"a": {"b": [{"kernel": spec}]}},                  # object nested deeper
            "edges": [],
        }
        out = wf._attach_transport(raw)

        wire = orjson.dumps(out)  # what Gradio's ORJSONResponse serializes onto the wire
        # The leak is the `wasm_path` FIELD surviving (raw cache-dir path with `wasm: null`); the
        # RESOLVED served `wasm` URL legitimately contains the path (Gradio's file route -- intended).
        assert b"wasm_path" not in wire, f"#505: wasm_path leaked on the wire: {wire!r}"

        for kern in (out["nodes"][0]["kernel"], out["bare"][0], out["deep"]["a"]["b"][0]["kernel"]):
            assert isinstance(kern, dict), f"the object was not serialized to a dict: {kern!r}"
            assert "wasm_path" not in kern, kern
            assert isinstance(kern["wasm"], str) and kern["wasm"].startswith("/gradio_api/file="), kern
            assert kern["wasm"].endswith(".wasm"), kern

        # CLIENT-dict twin: a plain dict naming an UNTRUSTED path stays untrusted (wasm None, stripped)
        client = {"nodes": [{"kernel": {"fn": "k", "wasm": None, "wasm_path": str(tmp_path / "victim" / "x.wasm")}}]}
        cout = wf._attach_transport(client)
        ckern = cout["nodes"][0]["kernel"]
        assert ckern["wasm"] is None and "wasm_path" not in ckern, ckern
    finally:
        paths[:] = before


def test_505_negative_control_passthrough_mutant_leaks_wasm_path(tmp_path):
    """PAIRED NEGATIVE CONTROL (anti-vacuity): a mutant that reverts `_attach_transport`'s object
    branch to the scalar pass-through lets the embedded dataclass survive the walk; orjson then emits
    its raw `wasm_path` on the wire -> RED. Proves the object branch is load-bearing."""
    _wf()  # gate on gradio
    _, wasm = _artifact_dir(tmp_path)
    spec = _callback_spec(str(wasm))
    mut, name = _load_mutant(
        tmp_path,
        '    if hasattr(value, "to_payload"):\n        return _attach_transport_to_kernel(_kernel_payload(value))\n',
        "",
        "505",
    )
    paths = _static_paths()
    before = list(paths)
    try:
        raw = {"nodes": [{"id": "k", "type": "compute", "kernel": spec}]}
        out = mut._attach_transport(raw)
        wire = orjson.dumps(out)
        # the pass-through mutant leaves the dataclass in place; orjson serializes it with `wasm: null`
        # AND the raw `wasm_path` cache-dir path -> the field leaks on the wire.
        assert b"wasm_path" in wire, "the pass-through MUTANT must leak wasm_path (else the control is vacuous)"
        assert b'"wasm":null' in wire, f"the MUTANT must emit the un-transported wasm: null: {wire!r}"
        # and the fixed module does NOT leak the same input (positive/negative discrimination)
        assert b"wasm_path" not in orjson.dumps(_wf()._attach_transport(raw))
    finally:
        paths[:] = before
        sys.modules.pop(name, None)


# ------------------------------------------------------------------------------------------------
# #506 -- the served static path must expose the .wasm ONLY, never the kernel SOURCE
# ------------------------------------------------------------------------------------------------

def test_506_served_static_path_excludes_source(tmp_path):
    """FIXED behavior: after a trusted artifact is served, its `.wasm` is network-readable but NONE of
    its siblings are -- not the kernel SOURCE `*.ps`, not `manifest.json`, not the `*.js`/`*.glue.js`
    glue, not `pyths-runtime/*`. The registered static path is the FILE, never the artifact dir."""
    from gradio.utils import is_static_file

    wf = _wf()
    d, wasm = _artifact_dir(tmp_path)
    spec = _callback_spec(str(wasm))
    paths = _static_paths()
    before = list(paths)
    try:
        wf._attach_transport({"kind": "flowgraph", "nodes": [{"kernel": spec}]})  # trusts + registers

        assert is_static_file(wasm) is True, "the .wasm the frontend fetches must be served"
        for leaked in (d / "kernel.ps", d / "manifest.json", d / "kernel.js", d / "kernel.glue.js", d / "pyths-runtime" / "index.js"):
            assert is_static_file(leaked) is False, f"#506: source/build file exposed: {leaked}"
        assert is_static_file(d) is False, "#506: the artifact DIRECTORY must not be servable"

        added = [p for p in _static_paths() if p not in before]
        assert wasm.resolve() in added, f"the .wasm file itself must be the registered path: {added}"
        assert d.resolve() not in added, f"#506: the artifact dir must NOT be registered: {added}"
    finally:
        paths[:] = before


def test_506_negative_control_dir_registration_exposes_source(tmp_path):
    """PAIRED NEGATIVE CONTROL (anti-vacuity): a mutant that reverts `_served_wasm_url` to register
    `rp.parent` (the artifact dir) makes the kernel SOURCE `.ps` and `manifest.json` network-readable
    -> RED. Proves the file-level registration is load-bearing."""
    from gradio.utils import is_static_file

    _wf()  # gate on gradio
    d, wasm = _artifact_dir(tmp_path)
    spec = _callback_spec(str(wasm))
    mut, name = _load_mutant(tmp_path, "gr.set_static_paths([rp])", "gr.set_static_paths([rp.parent])", "506")
    paths = _static_paths()
    before = list(paths)
    try:
        assert is_static_file(d / "kernel.ps") is False
        mut._attach_transport({"kind": "flowgraph", "nodes": [{"kernel": spec}]})  # trusts + registers (dir)
        assert is_static_file(d / "kernel.ps") is True, "the DIR-registering MUTANT must expose the .ps source (else the control is vacuous)"
        assert is_static_file(d / "manifest.json") is True, "the MUTANT must expose manifest.json (else the control is vacuous)"
    finally:
        paths[:] = before
        sys.modules.pop(name, None)
