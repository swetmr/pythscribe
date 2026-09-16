"""M2 gates (spec 13-09-26-lib-selfcontained-pip-wheel; plan §M2 + §M2.1; validation §G) -- every
gate ships its PAIRED NEGATIVE CONTROL (the anti-vacuity paired-control convention), each verified RED-on-revert.

  G-first-call  the node-free FIRST-CALL WITNESS on POST-CALL COUNTERS: node scrubbed from PATH,
                the network refused, no beside-source artifact, a COLD `PYTHSCRIBE_CACHE`; then
                `py0, sv0 = b.counts(); r = hello(1.0); py1, sv1 = b.counts()` ->
                sv1 == sv0 + 1 AND py1 == py0 (the counters `record_server_call` increments only
                AFTER `self.server.call(...)` returned), mode == server, artifact_status == resolved,
                exactly ONE new keyed artifact dir under the cache root, float_bits equal.
                `mode_reason` is set at BIND time and is NOT accepted as a witness.
      Controls: forced-Python-dispatch (WasmBinding.run_server -> run_python after a successful
                bind: mode==server, reason set -- the rev-2 witness could not see it) -> RED
                (python_calls +1, server_calls unchanged); compiler removed -> RED; warm cache ->
                the "newly built" clause RED.
  G-fallback    PYTHSCRIBE_NO_JIT=1 -> Python value, python_calls +1 (its own test, never G's pass).
  G-node-scope  build / first call / server spawn no `node`; T3 (run_artifact), T4 (run_kernel),
                T6 (`pyths run`) with node absent -> a structured message, never a traceback.
  G-ABI-1       a fresh artifact exports `__pyths_abi` == SUPPORTED_ABI_MAJOR and its `pyths.abi`
                section carries the runtime's layouts; ServerKernel instantiates. Controls (three
                INDEPENDENT patches, each alone): major+1 / list-v9 / array-v9 -> AbiMismatchError
                naming the FIELD and both sides before any call; missing / duplicated section ->
                refused. The per-field ABSENT-COMPARISON mutants (§G-ABI-3 (v)(vi)) are shown to
                be caught by exactly their control. The same patched modules through the tab shim
                (`list_buffer.mjs::instantiate`, run under Node) -> a loud load error.
  G-ABI-2       manifest `compiler.version: "0.1.0"` -> resolve() `unusable` + WARNING naming the
                version and ACCEPTED_COMPILER_RANGE; the call runs Python (python_calls +1, value
                pinned); explicit mode=server / artifact= raise.
  G-ABI-4       the ABI check never consults the ambient toolchain: a prebuilt artifact resolves +
                instantiates with no compiler reachable and no `wasm-opt`.
  G-ABI-5       CHECK BEFORE EXECUTE (codex m2 blocker + should-fix, one class): a VALID module
                carrying a WRONG ABI and an `unreachable` START function is refused with the ABI
                verdict and the start NEVER runs -- through the generated glue under Node (the
                loader compiles -> checks -> instantiates) AND through ServerKernel, where
                `wt.Module` is called ZERO times on any mismatch (a spy counts). Discriminating
                twin: the SAME start module with the CORRECT contract does run its start (trap),
                so the refusal is not vacuous. Mutants: instantiate-before-check glue -> RED
                (`RuntimeError: unreachable`); compile-before-check server -> RED (count 1, and an
                invalid trailing section flips the verdict to WasmTrap).
  build/JIT     a pre-ABI artifact of the same pin (section stripped) is REBUILT by `build_kernel`
                (not "up to date") and NEVER adopted from the JIT cache (private build instead).
"""
from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import sys
from pathlib import Path

import pytest

from _abi_helpers import module_with_unreachable_start, rewrite_abi_section, rewrite_artifact, strip_abi_section
from conftest import REPO, gate, gate_node, import_module_from
from pythscribe import binding_of
from pythscribe._pin import ACCEPTED_COMPILER_RANGE, COMPILER_VERSION
from pythscribe.artifacts import ArtifactNotFoundError, resolve
from pythscribe.build import BuildError, build_kernel, build_module, find_pyths
from pythscribe.build.runner import float_bits
from pythscribe.decorators import ModeError, WasmBinding
from pythscribe.ffi import SHIM
from pythscribe.runtime import LAYOUT_VERSION, AbiMismatchError, abi, array_buffer, wasmtime_available

HELLO_PY = REPO / "examples" / "hello.py"
HELLO_KSRC = "def hello(x: float) -> float:\n    return x * 2.0 + 1.0\n"
SYSTEM_PATH = "C:\\Windows\\System32" if os.name == "nt" else "/usr/bin:/bin"


def _require_toolchain() -> Path:
    try:
        p = find_pyths()
    except BuildError as e:
        gate(False, f"pyths compiler required ({e})")
    gate(wasmtime_available(), "wasmtime-py required for the server path")
    return p


class _Spawns:
    """Records every subprocess the code under test starts (argv[0] basenames)."""

    def __init__(self, monkeypatch):
        self.argv0: list[str] = []
        real_run, real_popen = subprocess.run, subprocess.Popen
        rec = self

        def run(args, *a, **k):
            rec.argv0.append(Path(str(args[0] if isinstance(args, (list, tuple)) else args)).name.lower())
            return real_run(args, *a, **k)

        class Popen(real_popen):  # type: ignore[misc]
            def __init__(self, args, *a, **k):
                rec.argv0.append(Path(str(args[0] if isinstance(args, (list, tuple)) else args)).name.lower())
                super().__init__(args, *a, **k)

        monkeypatch.setattr(subprocess, "run", run)
        monkeypatch.setattr(subprocess, "Popen", Popen)

    def node_reached(self) -> bool:
        return any(n in ("node", "node.exe", "npm", "npm.cmd", "npx", "npx.cmd") for n in self.argv0)


def _node_free(monkeypatch, pyths: Path) -> _Spawns:
    """The node-free, network-refusing environment: PATH holds only the system dir (the compiler
    is reached through PYTHSCRIBE_PYTHS, an absolute path), `shutil.which('node')` is None, and
    any socket connect raises."""
    monkeypatch.setenv("PATH", SYSTEM_PATH)
    monkeypatch.setenv("PYTHSCRIBE_PYTHS", str(pyths))
    assert shutil.which("node") is None and shutil.which("wasm-opt") is None

    def refuse(self, *a, **k):
        raise AssertionError("network access attempted during the node-free witness")

    monkeypatch.setattr(socket.socket, "connect", refuse)
    monkeypatch.setattr(socket.socket, "connect_ex", refuse)
    return _Spawns(monkeypatch)


def _cold(monkeypatch, tmp_path: Path) -> Path:
    """A COLD JIT cache + the JIT enabled (conftest disables it by default) -- set BEFORE import."""
    cache = tmp_path / "jit"
    monkeypatch.setenv("PYTHSCRIBE_CACHE", str(cache))
    monkeypatch.delenv("PYTHSCRIBE_NO_JIT", raising=False)
    monkeypatch.delenv("PYTHSCRIBE_MODE", raising=False)
    return cache


def _fixture_copy(tmp_path: Path, stem: str = "hello_fx") -> Path:
    """examples/hello.py copied to a fresh dir: no beside-source `__pythscribe__/` by construction
    (and the repo's own examples/__pythscribe__/hello must not exist either)."""
    assert not (REPO / "examples" / "__pythscribe__" / "hello").exists(), "a beside-source artifact would mask the witness"
    d = tmp_path / stem
    d.mkdir()
    p = d / "hello.py"
    p.write_text(HELLO_PY.read_text(encoding="utf-8"), encoding="utf-8")
    return p


def _keyed_dirs(cache: Path) -> set[Path]:
    # M3 keys the JIT cache <ver>/<optimizer id>/<sha> -- descend to the <sha> leaf (the dir that
    # holds __pythscribe__/), not the intermediate optimizer-id level (integration with M3).
    root = cache / COMPILER_VERSION
    if not root.is_dir():
        return set()
    return {sha for optid in root.iterdir() if optid.is_dir()
            for sha in optid.iterdir() if sha.is_dir()}


# ============================================================================ G-first-call
def test_g_first_call_witness_on_post_call_counters_node_free(tmp_path, monkeypatch):
    pyths = _require_toolchain()
    spawns = _node_free(monkeypatch, pyths)
    cache = _cold(monkeypatch, tmp_path)
    src = _fixture_copy(tmp_path)
    assert _keyed_dirs(cache) == set()  # cold

    mod = import_module_from(src, "m2_witness")
    b = binding_of(mod.hello)
    assert b.mode == "fallback" and b.artifact_status == "absent", "nothing is compiled at import"
    py0, sv0 = b.counts()

    r = mod.hello(1.0)

    py1, sv1 = b.counts()
    assert sv1 == sv0 + 1, f"server_calls did not advance: {(py0, sv0)} -> {(py1, sv1)} (jit_reason={b._jit_reason!r})"
    assert py1 == py0, f"the Python body ran: {(py0, sv0)} -> {(py1, sv1)}"
    assert b.mode == "server" and b.artifact_status == "resolved"
    new = _keyed_dirs(cache)
    assert len(new) == 1, f"expected exactly one newly built keyed artifact dir, found {new}"
    [keyed] = new
    assert (keyed / "__pythscribe__" / "hello" / "manifest.json").is_file()
    assert float_bits(r) == float_bits(3.0) == float_bits(b.python_fn(1.0))
    assert not (src.parent / "__pythscribe__").exists()  # the JIT never writes beside the source
    assert not spawns.node_reached(), spawns.argv0
    assert any(n.startswith("pyths") for n in spawns.argv0), spawns.argv0  # the compiler DID run


def test_g_control_forced_python_dispatch_is_red(tmp_path, monkeypatch):
    """The rev-2 blind spot: a bound server binding (artifact verified, mode==server, mode_reason
    set) whose wrapper dispatches to run_python. Every METADATA witness passes; only the post-call
    counters expose it -- python_calls +1, server_calls unchanged -> the G witness is RED."""
    pyths = _require_toolchain()
    _node_free(monkeypatch, pyths)
    _cold(monkeypatch, tmp_path)
    src = _fixture_copy(tmp_path)
    mod = import_module_from(src, "m2_forced")
    b = binding_of(mod.hello)
    b.ensure_compiled()  # bind for real: the artifact IS compiled and verified
    assert b.mode == "server" and b.mode_reason == "auto: compiled on first call" and b.server is not None
    monkeypatch.setattr(WasmBinding, "run_server", WasmBinding.run_python)  # the mutant
    py0, sv0 = b.counts()
    r = mod.hello(1.0)
    py1, sv1 = b.counts()
    assert float_bits(r) == float_bits(3.0)  # value parity MASKS the path ...
    assert b.mode == "server" and b.mode_reason  # ... and so does every metadata witness ...
    assert (py1, sv1) == (py0 + 1, sv0)  # ... only the counters say Python ran: G-first-call RED
    assert not (sv1 == sv0 + 1 and py1 == py0)


def test_g_control_compiler_removed_is_red(tmp_path, monkeypatch):
    gate(wasmtime_available(), "wasmtime-py required to isolate the compiler-absent arm")
    monkeypatch.setenv("PATH", SYSTEM_PATH)
    monkeypatch.setenv("PYTHSCRIBE_PYTHS", str(tmp_path / "no" / "such" / "pyths"))
    cache = _cold(monkeypatch, tmp_path)
    src = _fixture_copy(tmp_path)
    mod = import_module_from(src, "m2_nocompiler")
    b = binding_of(mod.hello)
    py0, sv0 = b.counts()
    r = mod.hello(1.0)
    py1, sv1 = b.counts()
    assert float_bits(r) == float_bits(3.0)  # never a wrong value, never a crash
    assert (py1, sv1) == (py0 + 1, sv0) and b.mode == "fallback"  # G-first-call RED
    assert "compiler not found" in b._jit_reason or "not a file" in b._jit_reason, b._jit_reason
    assert _keyed_dirs(cache) == set()


def test_g_control_warm_cache_makes_the_newly_built_clause_red(tmp_path, monkeypatch):
    pyths = _require_toolchain()
    _node_free(monkeypatch, pyths)
    cache = _cold(monkeypatch, tmp_path)
    src = _fixture_copy(tmp_path)
    warm = import_module_from(src, "m2_warm_a")
    assert warm.hello(2.0) == 5.0 and binding_of(warm.hello).mode == "server"
    before = _keyed_dirs(cache)
    assert len(before) == 1  # the cache is now WARM (pre-populated keyed dir)
    mod = import_module_from(_fixture_copy(tmp_path, "hello_fx2"), "m2_warm_b")
    b = binding_of(mod.hello)
    py0, sv0 = b.counts()
    mod.hello(1.0)
    py1, sv1 = b.counts()
    assert (py1, sv1) == (py0, sv0 + 1) and b.mode == "server"  # the counters are GREEN ...
    assert _keyed_dirs(cache) == before  # ... but NOTHING was newly built: the G clause is RED
    assert len(_keyed_dirs(cache) - before) != 1


# ============================================================================ G-fallback
def test_g_fallback_no_jit_runs_python_and_counts_it(tmp_path, monkeypatch):
    pyths = _require_toolchain()
    _node_free(monkeypatch, pyths)
    _cold(monkeypatch, tmp_path)
    monkeypatch.setenv("PYTHSCRIBE_NO_JIT", "1")
    mod = import_module_from(_fixture_copy(tmp_path), "m2_nojit")
    b = binding_of(mod.hello)
    py0, sv0 = b.counts()
    assert float_bits(mod.hello(1.0)) == float_bits(3.0)
    assert b.counts() == (py0 + 1, sv0) and b.mode == "fallback" and "PYTHSCRIBE_NO_JIT" in b._jit_reason


# ============================================================================ G-node-scope
def test_g_node_scope_build_and_server_spawn_no_node_and_t3_t4_t6_are_graceful(tmp_path, monkeypatch, capsys):
    pyths = _require_toolchain()
    spawns = _node_free(monkeypatch, pyths)
    src = _fixture_copy(tmp_path)
    info = build_kernel(src, "hello", HELLO_KSRC, quiet=True)  # explicit build: no node
    from pythscribe.runtime import ServerKernel

    k = ServerKernel.from_artifact(info)
    assert k.call("hello", ["float"], [1.0], return_type="float").value == 3.0  # server: no node
    assert not spawns.node_reached(), spawns.argv0
    # T3 / T4: the opt-in Node oracle arms -- a TYPED, structured message, no spawn traceback
    from pythscribe.build.runner import RunnerError, run_artifact
    from pythscribe.ffi import FfiError, run_kernel

    with pytest.raises(RunnerError, match="no `node` is on PATH") as t3:
        run_artifact(info, "hello", [[1.0]])
    assert "do NOT need Node" in str(t3.value) and t3.value.__cause__ is None
    with pytest.raises(FfiError, match="no `node` is on PATH") as t4:
        run_kernel(info, "hello", ["float"], [{"args": [1.0]}], return_type="float")
    assert "do NOT need Node" in str(t4.value) and t4.value.__cause__ is None
    assert not spawns.node_reached()
    # T6: the launcher's `pyths run` with node absent -> the graceful message, exit 2, no traceback
    from pythscribe import _launcher

    ps = tmp_path / "m.ps"
    ps.write_text('print("hi")\n', encoding="utf-8")
    monkeypatch.setattr(sys, "argv", ["pyths"])
    rc = _launcher.main(["run", str(ps)])
    err = capsys.readouterr().err
    assert rc == 2 and "pyths run" in err and "Node" in err and "`pyths build`" in err and "Traceback" not in err
    assert not spawns.node_reached()


# ============================================================================ G-ABI-1
@pytest.fixture
def fresh_artifact(tmp_path, monkeypatch):
    _require_toolchain()
    src = _fixture_copy(tmp_path, "abi_fx")
    [info] = build_module(src, quiet=True)
    return info


PATCHES = {
    "abi": {"abi": abi.SUPPORTED_ABI_MAJOR + 1},
    "list_layout": {"list_layout": "pyths-0.2.4-list-v9"},
    "array_layout": {"array_layout": "pyths-0.2.5-array-v9"},
}


def test_abi1_fresh_artifact_carries_the_contract_and_instantiates(fresh_artifact):
    from pythscribe.runtime import ServerKernel

    data = fresh_artifact.wasm.read_bytes()
    sec = abi.read_abi_section(data)
    assert sec["abi"] == abi.SUPPORTED_ABI_MAJOR and sec["list_layout"] == LAYOUT_VERSION and sec["array_layout"] == array_buffer.ARRAY_LAYOUT_VERSION
    k = ServerKernel.from_artifact(fresh_artifact)
    inst = k.new_instance()
    assert inst.exports[abi.ABI_GLOBAL_EXPORT].value(inst.store) == abi.SUPPORTED_ABI_MAJOR
    assert k.call("hello", ["float"], [1.0], return_type="float").value == 3.0
    k.close()


@pytest.mark.parametrize("field", list(PATCHES))
def test_abi1_each_patched_field_alone_is_refused_naming_the_field(fresh_artifact, field):
    """Three INDEPENDENT patches (major+1 / list-v9 / array-v9), each with the other two fields
    untouched -> AbiMismatchError naming exactly that field and both sides, before any call."""
    from pythscribe.runtime import ServerKernel

    patched = rewrite_abi_section(fresh_artifact.wasm.read_bytes(), **PATCHES[field])
    with pytest.raises(AbiMismatchError) as ei:
        ServerKernel.from_wasm(patched, name="patched")
    e = ei.value
    assert e.field == field, str(e)
    assert f"`{field}`" in str(e) and str(e.actual) in str(e) and str(e.expected) in str(e)
    assert e.actual == PATCHES[field][field] and e.expected == abi.expected_contract()[field]
    # and through the artifact path: resolve() classifies it UNUSABLE (auto -> Python fallback + warning)
    rewrite_artifact(fresh_artifact.dir, wasm_bytes=patched)
    info, status = resolve(fresh_artifact.dir.parent.parent / "hello.py", "hello", fresh_artifact.source_sha256)
    assert (info, status) == (None, "unusable")
    with pytest.raises(ArtifactNotFoundError, match=f"`{field}`"):
        resolve(fresh_artifact.dir.parent.parent / "hello.py", "hello", fresh_artifact.source_sha256, explicit=fresh_artifact.dir)


def test_abi1_missing_or_duplicated_section_is_refused(fresh_artifact):
    from pythscribe.runtime import ServerKernel

    data = fresh_artifact.wasm.read_bytes()
    stripped = strip_abi_section(data)
    with pytest.raises(AbiMismatchError, match="no `pyths.abi` custom section") as ei:
        ServerKernel.from_wasm(stripped, name="stripped")
    assert ei.value.field == "section" and ACCEPTED_COMPILER_RANGE in str(ei.value)
    twice = data + abi.encode_custom_section(abi.ABI_SECTION_NAME, json.dumps(abi.read_abi_section(data)).encode())
    with pytest.raises(AbiMismatchError, match="2 `pyths.abi` sections"):
        ServerKernel.from_wasm(twice, name="twice")
    # a section that is present but lacks a contract field
    for f in ("abi", "list_layout", "array_layout"):
        with pytest.raises(AbiMismatchError, match=f"lacks the `{f}` field"):
            ServerKernel.from_wasm(rewrite_abi_section(data, **{f: None}), name="lacking")
    # garbage bytes still get wasmtime's diagnosis (the ABI gate does not hide a corrupt module)
    from pythscribe.runtime import WasmTrap

    with pytest.raises(WasmTrap, match="not a valid WebAssembly module"):
        ServerKernel.from_wasm(b"\x00asn" + data[4:], name="badmagic")


def test_abi1_absent_comparison_mutants_are_each_caught_by_exactly_their_control(fresh_artifact):
    """§G-ABI-3 (v)/(vi), enumerated per field: a comparator that OMITS field f's comparison accepts
    the f-patched module (the mutant's silent-misread) while still refusing the other two -- so
    each patched control discriminates exactly its field, and the SHIPPED comparator refuses all
    three (no field is vacuously covered)."""
    data = fresh_artifact.wasm.read_bytes()
    want = abi.expected_contract()
    modules = {f: rewrite_abi_section(data, **PATCHES[f]) for f in PATCHES}

    def mutant_missing(skip: str):
        def check(wasm_bytes: bytes) -> bool:
            got = abi.read_abi_section(wasm_bytes)
            return all(got[f] == want[f] for f in want if f != skip)

        return check

    for skip in PATCHES:
        accepts = mutant_missing(skip)
        assert accepts(modules[skip]) is True, f"mutant without the `{skip}` comparison must accept the {skip}-patched module"
        for other in PATCHES:
            if other != skip:
                assert accepts(modules[other]) is False
        assert abi.module_problem(modules[skip]) is not None  # the shipped comparator refuses it
    assert abi.module_problem(data) is None
    # the major-only mutant (v): accepts BOTH layout controls
    major_only = lambda b: abi.read_abi_section(b)["abi"] == want["abi"]  # noqa: E731
    assert major_only(modules["list_layout"]) and major_only(modules["array_layout"]) and not major_only(modules["abi"])


def test_abi1_tab_side_shim_refuses_the_same_patched_modules(fresh_artifact, tmp_path):
    """The SAME three patched modules through the tab shim (`list_buffer.mjs::instantiate`, the
    bytes the Gradio / Streamlit tabs run; K7 pins the three copies byte-identical), executed under
    Node -> a loud load error naming the field; the pristine module instantiates. Plus the tab-side
    absent-comparison mutant (vi): a shim copy with the `array_layout` comparison removed
    instantiates the array-v9 module -> the control is discriminating for the tab too."""
    node = gate_node()
    data = fresh_artifact.wasm.read_bytes()
    cases = {"pristine": data, **{f: rewrite_abi_section(data, **PATCHES[f]) for f in PATCHES}, "stripped": strip_abi_section(data)}
    for name, b in cases.items():
        (tmp_path / f"{name}.wasm").write_bytes(b)
    shim_text = SHIM.read_text(encoding="utf-8")
    mutant = tmp_path / "shim_no_array_cmp.mjs"
    line = '  if (got.array_layout !== ARRAY_LAYOUT_VERSION) throw abiMismatch("array_layout", got.array_layout, ARRAY_LAYOUT_VERSION);\n'
    assert line in shim_text
    mutant.write_text(shim_text.replace(line, ""), encoding="utf-8")

    def run(shim: Path, wasm: Path) -> dict:
        prog = (
            f"import * as m from {json.dumps(shim.resolve().as_uri())};\n"
            "import { readFile } from 'node:fs/promises';\n"
            f"const bytes = await readFile({json.dumps(str(wasm))});\n"
            "try { const k = await m.instantiate(bytes); process.stdout.write(JSON.stringify({ok: true, value: k.exports.hello(1.0), abi: k.abi.abi})); }\n"
            "catch (e) { process.stdout.write(JSON.stringify({ok: false, name: e.name, field: e.field ?? null, message: String(e.message)})); }\n"
        )
        r = subprocess.run([node, "--input-type=module", "-e", prog], capture_output=True, text=True)
        assert r.returncode == 0, r.stderr
        return json.loads(r.stdout)

    assert run(SHIM, tmp_path / "pristine.wasm") == {"ok": True, "value": 3.0, "abi": abi.SUPPORTED_ABI_MAJOR}
    for f in PATCHES:
        got = run(SHIM, tmp_path / f"{f}.wasm")
        assert got["ok"] is False and got["name"] == "AbiMismatchError" and got["field"] == f, got
        assert f"`{f}`" in got["message"] and str(PATCHES[f][f]) in got["message"], got
    got = run(SHIM, tmp_path / "stripped.wasm")
    assert got["ok"] is False and "expected exactly one `pyths.abi`" in got["message"], got
    # the tab-side mutant (vi): drops the array comparison -> array-v9 instantiates (silent misread)
    assert run(mutant, tmp_path / "array_layout.wasm")["ok"] is True
    assert run(mutant, tmp_path / "list_layout.wasm")["ok"] is False  # still discriminating on the others
    assert run(SHIM, tmp_path / "array_layout.wasm")["ok"] is False  # the shipped shim refuses it


# ============================================================================ G-ABI-5
# Check BEFORE execute (codex m2 review: blocker + should-fix, the same class). The existing
# G-ABI-1 controls only order the check against RETURNING exports -- a loader that instantiated
# first and checked second passed them while a wrong-ABI module's start function already ran.
GLUE_CHECK_LINE = "  __checkPythsAbi(__module);\n"
GLUE_INSTANTIATE_LINE = "  const __instance = await WebAssembly.instantiate(__module, imports);\n"


def _node_import(node: str, glue: Path) -> dict:
    """Import a generated glue module under Node (its top-level await runs the loader) and report
    whether it loaded or which error refused it."""
    prog = (
        f"try {{ await import({json.dumps(glue.resolve().as_uri())}); process.stdout.write(JSON.stringify({{ok: true}})); }}\n"
        "catch (e) { process.stdout.write(JSON.stringify({ok: false, name: e.name, message: String(e.message)})); }\n"
    )
    r = subprocess.run([node, "--input-type=module", "-e", prog], capture_output=True, text=True, timeout=60)
    assert r.returncode == 0, r.stderr
    return json.loads(r.stdout)


def test_abi5_glue_refuses_wrong_abi_before_the_start_function_can_run(fresh_artifact):
    """The generated loader (Node branch of the universal glue): a wrong-major module whose START is
    `unreachable` -> the ABI mismatch Error, never `RuntimeError: unreachable`. Twin: the same start
    module with the CORRECT contract -> `RuntimeError: unreachable` (the start really runs on
    instantiation, so the refusal is discriminating). In-test mutant: the glue with the two lines
    swapped (instantiate, THEN check) runs the start on the wrong-ABI module -> RED."""
    node = gate_node()
    glue = fresh_artifact.dir / fresh_artifact.manifest["glue"]
    wrong = module_with_unreachable_start(abi=abi.SUPPORTED_ABI_MAJOR + 1)
    right = module_with_unreachable_start()
    # BEHAVIOR first (the witness a re-ordered loader fails regardless of how its text reads):
    # the shipped glue refuses with the ABI verdict; the start never executed
    fresh_artifact.wasm.write_bytes(wrong)
    got = _node_import(node, glue)
    assert got["name"] != "RuntimeError" and "unreachable" not in got["message"], f"the wrong-ABI module's start RAN before the check: {got}"
    assert got["ok"] is False and got["name"] == "Error" and "WASM ABI mismatch on `abi`" in got["message"], got
    # the twin: correct contract -> the start runs and traps (proves the start is live)
    fresh_artifact.wasm.write_bytes(right)
    got = _node_import(node, glue)
    assert got["ok"] is False and got["name"] == "RuntimeError" and "unreachable" in got["message"], got
    # a v9-layout start module is refused the same way by the shipped glue (field-independent)
    fresh_artifact.wasm.write_bytes(module_with_unreachable_start(list_layout="pyths-0.2.4-list-v9"))
    got = _node_import(node, glue)
    assert got["ok"] is False and "WASM ABI mismatch on `list_layout`" in got["message"] and "unreachable" not in got["message"], got
    # TEXT second: the pinned form, and the in-test mutant (instantiate first) built from it --
    # the wrong-ABI module's start runs BEFORE the check -> the mutant is RED on the same witness
    text = glue.read_text(encoding="utf-8")
    assert GLUE_CHECK_LINE + GLUE_INSTANTIATE_LINE in text, "glue no longer orders check -> instantiate in the pinned form"
    assert "WebAssembly.instantiate(bytes" not in text and "instantiateStreaming" not in text
    mutant = glue.with_name("hello.mutant.glue.js")
    mutant.write_text(text.replace(GLUE_CHECK_LINE + GLUE_INSTANTIATE_LINE, GLUE_INSTANTIATE_LINE + GLUE_CHECK_LINE), encoding="utf-8")
    assert mutant.read_text(encoding="utf-8") != text
    fresh_artifact.wasm.write_bytes(wrong)
    got = _node_import(node, mutant)
    assert got["ok"] is False and got["name"] == "RuntimeError" and "unreachable" in got["message"], got


@pytest.fixture
def module_spy(monkeypatch):
    """Count `wasmtime.Module(...)` constructions (the compile step) without changing behaviour."""
    _require_toolchain()
    import wasmtime

    real = wasmtime.Module
    calls = []

    def spy(*a, **kw):
        calls.append(1)
        return real(*a, **kw)

    monkeypatch.setattr(wasmtime, "Module", spy)
    return calls


def test_abi5_server_never_compiles_a_mismatched_module(fresh_artifact, module_spy):
    """ServerKernel short-circuits on the ABI verdict: `wt.Module` is called ZERO times for every
    mismatch shape (the three patched fields, stripped, duplicated, field-lacking, and the start-
    function module), and the codex reproduction -- a wrong-major module with an INVALID trailing
    section -- still reports the ABI verdict, not WasmTrap. Mutant (compile before the verdict) ->
    count 1 and the reproduction flips to WasmTrap -> RED."""
    from pythscribe.runtime import ServerKernel, WasmTrap

    data = fresh_artifact.wasm.read_bytes()
    cases = {
        **{f: rewrite_abi_section(data, **PATCHES[f]) for f in PATCHES},
        "stripped": strip_abi_section(data),
        "twice": data + abi.encode_custom_section(abi.ABI_SECTION_NAME, json.dumps(abi.read_abi_section(data)).encode()),
        "lacking": rewrite_abi_section(data, abi=None),
        "start_wrong_major": module_with_unreachable_start(abi=abi.SUPPORTED_ABI_MAJOR + 1),
        "start_v9_array": module_with_unreachable_start(array_layout="pyths-0.2.5-array-v9"),
        # codex reproduction: wrong major + an invalid section (id 0x7f, 1 byte) appended -- a loader
        # that compiled first would report wasmtime's rejection and hide the ABI verdict
        "wrong_major_plus_invalid_section": rewrite_abi_section(data, **PATCHES["abi"]) + b"\x7f\x01\x00",
    }
    for name, b in cases.items():
        module_spy.clear()
        with pytest.raises(AbiMismatchError) as ei:
            ServerKernel.from_wasm(b, name=name)
        assert len(module_spy) == 0, f"{name}: wt.Module was called {len(module_spy)}x before the ABI verdict"
        assert "unreachable" not in str(ei.value)
    # the reproduction's verdict names the field and both sides (the invalid section did not hide it)
    with pytest.raises(AbiMismatchError) as ei:
        ServerKernel.from_wasm(cases["wrong_major_plus_invalid_section"], name="repro")
    assert ei.value.field == "abi" and ei.value.actual == abi.SUPPORTED_ABI_MAJOR + 1
    # the container-level garbage arm is unchanged: no verdict is possible, wasmtime diagnoses (1 call)
    module_spy.clear()
    with pytest.raises(WasmTrap, match="not a valid WebAssembly module"):
        ServerKernel.from_wasm(b"\x00asn" + data[4:], name="badmagic")
    assert len(module_spy) == 1
    # the pristine module compiles exactly once
    module_spy.clear()
    k = ServerKernel.from_wasm(data, name="pristine")
    assert len(module_spy) == 1
    k.close()


def test_abi5_server_start_function_is_live_only_behind_a_passing_abi(fresh_artifact, module_spy):
    """Discriminating twin on the server: the start module with the CORRECT contract compiles
    (count 1) and its start TRAPS at instantiation (`unreachable`) -- so the wrong-ABI refusal in
    the previous test is a refusal of a module that would otherwise have executed."""
    from pythscribe.runtime import ServerKernel

    k = ServerKernel.from_wasm(module_with_unreachable_start(), name="start_ok")
    assert len(module_spy) == 1 and k.abi["abi"] == abi.SUPPORTED_ABI_MAJOR
    with pytest.raises(Exception) as ei:
        k.new_instance()
    assert not isinstance(ei.value, AbiMismatchError) and "unreachable" in str(ei.value).lower(), repr(ei.value)
    k.close()


# ============================================================================ G-ABI-6 (codex B7)
# The `pyths.abi` custom section is provenance a forger can copy without touching the module body;
# the EXPORTED immutable i32 `__pyths_abi` global (emit.rs) is the load-bearing half. check_module
# now requires BOTH. Each shape below has a MATCHING section but a broken global export.
ABI_GLOBAL_BAD = ["missing", "mutable", "wrong_type", "wrong_value", "func_kind"]


def test_abi6_ok_module_passes_both_halves():
    """The discriminating twin: a hand-built module with the correct section AND a correct immutable
    i32 `__pyths_abi` export is loadable (module_problem None)."""
    ok = module_with_unreachable_start(abi_global="ok")
    assert abi.module_problem(ok) is None
    assert abi.check_abi_global_export(ok) == abi.SUPPORTED_ABI_MAJOR


@pytest.mark.parametrize("shape", ABI_GLOBAL_BAD)
def test_abi6_bad_abi_global_export_is_refused_naming_the_global(shape):
    """B7: section matches the runtime, but the exported `__pyths_abi` global is missing / mutable /
    wrong-type / wrong-value / wrong-kind -> AbiMismatchError(field='abi_global'). The section alone
    no longer authenticates the module."""
    bad = module_with_unreachable_start(abi_global=shape)
    sec = abi.read_abi_section(bad)  # the SECTION half still matches (so the refusal is the global half)
    assert (sec["abi"], sec["list_layout"], sec["array_layout"]) == (
        abi.SUPPORTED_ABI_MAJOR, LAYOUT_VERSION, array_buffer.ARRAY_LAYOUT_VERSION)
    with pytest.raises(AbiMismatchError) as ei:
        abi.check_module(bad, name=shape)
    assert ei.value.field == "abi_global", str(ei.value)
    assert abi.module_problem(bad) is not None


def test_abi6_export_check_is_load_bearing_mutation_control(monkeypatch):
    """Anti-vacuity paired control: the shipped check refuses every bad-global shape; a mutant that
    SKIPS the export check (`check_abi_global_export` -> no-op) ACCEPTS all of them. Proves B7 is
    caught by the export half, not incidentally by the section comparison."""
    bads = {s: module_with_unreachable_start(abi_global=s) for s in ABI_GLOBAL_BAD}
    for s, b in bads.items():
        assert abi.module_problem(b) is not None, f"{s}: shipped check must refuse it"
    monkeypatch.setattr(abi, "check_abi_global_export", lambda *a, **k: abi.SUPPORTED_ABI_MAJOR)  # the mutant
    for s, b in bads.items():
        assert abi.module_problem(b) is None, f"{s}: mutant (no export check) must accept a bad-global module"


def test_abi6_server_never_compiles_a_bad_abi_global_module(module_spy):
    """ServerKernel short-circuits on the abi_global verdict too: `wt.Module` is called ZERO times
    for every bad-global shape (the check runs before compilation/instantiation)."""
    from pythscribe.runtime import ServerKernel

    for s in ABI_GLOBAL_BAD:
        module_spy.clear()
        with pytest.raises(AbiMismatchError) as ei:
            ServerKernel.from_wasm(module_with_unreachable_start(abi_global=s), name=s)
        assert ei.value.field == "abi_global", str(ei.value)
        assert len(module_spy) == 0, f"{s}: wt.Module was called {len(module_spy)}x before the abi_global verdict"


def test_abi6_boolean_or_float_major_is_rejected(fresh_artifact):
    """B7: JSON `true` / `1.0` in the section `abi` field must NOT pass as the int major. The shipped
    check rejects it (field='abi'); the loose `!=` comparator a mutant would use accepts `true`
    because `True == 1` in Python."""
    data = fresh_artifact.wasm.read_bytes()
    for bad in (True, 1.0):
        with pytest.raises(AbiMismatchError) as ei:
            abi.check_module(rewrite_abi_section(data, abi=bad), name="typed")
        assert ei.value.field == "abi", str(ei.value)
    # the mutant comparator (`got["abi"] != major`) would ACCEPT JSON true: True == 1 is Python-true
    got = abi.read_abi_section(rewrite_abi_section(data, abi=True))
    assert (got["abi"] != abi.SUPPORTED_ABI_MAJOR) is False


# ============================================================================ G-ABI-2
def test_abi2_out_of_range_manifest_is_unusable_runs_python_and_explicit_raises(tmp_path, monkeypatch, caplog):
    _require_toolchain()
    monkeypatch.setenv("PYTHSCRIBE_NO_JIT", "1")  # the beside-source artifact is the only candidate
    src = _fixture_copy(tmp_path, "range_fx")
    [info] = build_module(src, quiet=True)

    def stamp(m):
        m["compiler"]["version"] = "0.1.0"

    rewrite_artifact(info.dir, manifest_edit=stamp)
    caplog.set_level("WARNING", logger="pythscribe")
    mod = import_module_from(src, "m2_range")  # auto mode
    b = binding_of(mod.hello)
    assert b.artifact_status == "unusable" and b.mode == "fallback"
    warn = [r.getMessage() for r in caplog.records if r.levelname == "WARNING"]
    assert any("0.1.0" in w and ACCEPTED_COMPILER_RANGE in w for w in warn), warn
    py0, sv0 = b.counts()
    assert float_bits(mod.hello(1.0)) == float_bits(3.0)  # never a wrong value
    assert b.counts() == (py0 + 1, sv0)
    # explicit mode=server -> ModeError; explicit artifact= -> ArtifactNotFoundError (both name the range)
    monkeypatch.setenv("PYTHSCRIBE_MODE", "server")
    with pytest.raises(ModeError, match="unusable"):
        import_module_from(src, "m2_range_server")
    monkeypatch.delenv("PYTHSCRIBE_MODE")
    with pytest.raises(ArtifactNotFoundError, match="0.1.0") as ei:
        resolve(src, "hello", b.source_sha256, explicit=info.dir)
    assert ACCEPTED_COMPILER_RANGE in str(ei.value)
    # the widened-range mutant (§G-ABI-3): a range that admits 0.1.0 would resolve it -> this gate RED
    monkeypatch.setattr(abi, "compiler_version_problem", lambda v: None)
    assert resolve(src, "hello", b.source_sha256)[1] == "resolved"


# ============================================================================ G-ABI-4
def test_abi4_prebuilt_artifact_loads_with_no_compiler_and_no_optimizer(tmp_path, monkeypatch):
    """No freshness leakage: the ABI check + range check pass on a prebuilt-only machine."""
    _require_toolchain()
    src = _fixture_copy(tmp_path, "prebuilt_fx")
    build_module(src, quiet=True)
    monkeypatch.setenv("PATH", SYSTEM_PATH)
    monkeypatch.setenv("PYTHSCRIBE_PYTHS", str(tmp_path / "no" / "pyths"))
    monkeypatch.setenv("PYTHSCRIBE_NO_JIT", "1")
    assert shutil.which("wasm-opt") is None and shutil.which("pyths") is None
    with pytest.raises(BuildError):
        find_pyths()
    mod = import_module_from(src, "m2_prebuilt")
    b = binding_of(mod.hello)
    assert b.artifact_status == "resolved" and b.mode == "server", b.mode_reason
    py0, sv0 = b.counts()
    assert mod.hello(1.0) == 3.0 and b.counts() == (py0, sv0 + 1)


# ============================================================================ build / JIT
def test_build_rebuilds_a_pre_abi_artifact_of_the_same_pin(tmp_path):
    """A committed artifact built by the SAME pin but before the section existed is not "up to date"."""
    _require_toolchain()
    src = _fixture_copy(tmp_path, "rebuild_fx")
    info = build_kernel(src, "hello", HELLO_KSRC, quiet=True)
    rewrite_artifact(info.dir, wasm_bytes=strip_abi_section(info.wasm.read_bytes()))
    assert abi.module_problem(info.wasm.read_bytes()) is not None
    again = build_kernel(src, "hello", HELLO_KSRC, quiet=True)  # no --force
    assert abi.module_problem(again.wasm.read_bytes()) is None, "build_kernel kept a pre-ABI artifact as up to date"
    assert build_kernel(src, "hello", HELLO_KSRC, quiet=True).wasm.read_bytes() == again.wasm.read_bytes()  # now stable


def test_jit_never_adopts_a_pre_abi_cache_entry(tmp_path, monkeypatch):
    pyths = _require_toolchain()
    _node_free(monkeypatch, pyths)
    cache = _cold(monkeypatch, tmp_path)
    src = _fixture_copy(tmp_path, "jit_fx")
    first = import_module_from(src, "m2_jit_a")
    assert first.hello(1.0) == 3.0
    [keyed] = _keyed_dirs(cache)
    adir = keyed / "__pythscribe__" / "hello"
    # a pre-ABI entry of the SAME pin that otherwise VERIFIES: the sentinel is LISTED in the manifest
    # (an unlisted file would make `verify()` reject the entry by itself and mask the ABI arm -- the
    # mutant that adopts pre-ABI entries would then pass vacuously; caught by the M10 drill)
    sentinel = adir / "SENTINEL.txt"
    sentinel.write_text("never rebuilt in place", encoding="utf-8")
    from pythscribe.artifacts import sha256_file

    rewrite_artifact(
        adir,
        wasm_bytes=strip_abi_section((adir / "hello.wasm").read_bytes()),
        manifest_edit=lambda m: m["files"].__setitem__("SENTINEL.txt", sha256_file(sentinel)),
    )
    from pythscribe.artifacts import verify

    verify(adir, function="hello", expected_source_sha256=binding_of(first.hello).source_sha256)  # integrity GREEN
    assert abi.module_problem((adir / "hello.wasm").read_bytes()) is not None  # loadability RED: the ABI arm alone decides
    mod = import_module_from(_fixture_copy(tmp_path, "jit_fx2"), "m2_jit_b")
    b = binding_of(mod.hello)
    py0, sv0 = b.counts()
    assert mod.hello(1.0) == 3.0 and b.counts() == (py0, sv0 + 1) and b.mode == "server", b._jit_reason
    assert sentinel.is_file()  # the shared invalid entry was left alone (private build)
    assert abi.module_problem(b.artifact.wasm.read_bytes()) is None


def test_doctor_prints_the_accepted_range(capsys, monkeypatch):
    from pythscribe import _launcher

    rc = _launcher.main(["doctor"])
    out = capsys.readouterr().out
    assert rc == 0 and ACCEPTED_COMPILER_RANGE in out and f"ABI major {abi.SUPPORTED_ABI_MAJOR}" in out
