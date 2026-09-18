"""M3 gates (spec 13-09-26-lib-selfcontained-pip-wheel; plan §M3 exit (a)-(i), validation §F) -- the
optimizer: integrity vs freshness, JIT-cache addressing, the Python-owned never-in-place pass and its
wasmtime publish gate. Every gate ships its PAIRED NEGATIVE CONTROL (the anti-vacuity paired-control convention); SPOTs are driven
through the SHIPPED build (`build_module` / `build_kernel`) and the SHIPPED `@wasm` wrapper, and the
value SPOT `float_bits(hello(1.0)) == float_bits(3.0)` is asserted on the PUBLISHED bytes (a
validator is a type check, not an equivalence check -- rev-7).

  F1   absent -> {id: none, applied: false}, exit 0.
  F2   present (real binaryen wasm-opt + wasmtime) -> applied: true, .wasm smaller, verify() GREEN,
       the ownership section preserved byte-for-byte, hello(1.0) == 3.0 on the optimized bytes.
  F2b  tab-path limitation stated: wasm-opt present, wasmtime NOT importable -> NOT published,
       error names the validator requirement + the browser-tab sentence; README/doctor carry it.
  F3   none->present, present->none, X->Y rebuild.   F4 repeated build no-op.
  F5   optimized artifact in an optimizer-free env -> resolve() adopts, mode server, runs.
       Control: a verify() mutant that consults the ambient optimizer -> unusable (RED here).
  F6   JIT: entry under id X not adopted when ambient is none; `<ver>/none/<sha>` appears; both
       coexist. Defensive half: an X-manifest planted under the none key is not adopted.
  F7   resolver parity: procutil.rs's fixture table through resolve_wasm_opt()'s resolver, and each
       fixture literal is asserted to still be in procutil.rs (a shared table, not a copy).
  F8   available-vs-applied: probe failure -> applied: false + error, exit 0, unoptimized bytes hashed.
  F8b  partial output + non-zero exit -> original byte-identical, hashed, runs.  F8c garbage-with-exit-0.
  F8d  section stripped -> re-appended into the candidate; published == payload + input's section.
  F8e  killed mid-write (timeout) -> original intact, private tmp dir gone.
  F8f  type-invalid-but-structurally-valid (f64.add -> i32.add): (i) wasmtime rejects on the FINAL
       bytes; (ii) no wasmtime -> never published. Mutant: publish on the structural walk alone ->
       the invalid bytes ARE hashed and instantiation fails (the control discriminates).
  F8g  validate AFTER the re-append: a fault-injected malformed re-append is rejected.
  F9   the compiler's own pass is off by contract (PYTHS_WASM_OPT = absolute-missing): a stub on PATH
       is NOT invoked by `pyths compile` (verbose says not found; sha == no-wasm-opt build), and IS
       invoked without the override (the pass is reachable -- the control), and under build_kernel
       it is invoked exactly once, by the Python pass. Both shipped passes (the Rust compiler's and
       the Python one) log an IDENTICAL argv shape: `-Os <in> -o <out> --enable-mutable-globals`.
  F10  the wasm-opt feature flags: `WASM_OPT_FEATURE_FLAGS` is parsed out of optimize.rs and must
       equal the Python tuple (a shared table, not a copy); the flag is LOAD-BEARING on the real
       binaryen (the compiled module exports a mutable global: `--disable-mutable-globals` on the same
       argv -> non-zero exit naming `mutable`; the shipped argv -> exit 0); and the OFF sentinel
       (`PYTHS_WASM_OPT=<abs>/.no-wasm-opt`) is a SILENT `none` whose forced build is byte-identical
       -- manifest included -- to an optimizer-free build even with a real wasm-opt on PATH (the
       pip gate's committed-artifact rebuild relies on it), while a non-sentinel missing override
       keeps its `refused` diagnostic (control).

Needs the pinned `pyths` (find_pyths) and wasmtime-py; the F2/F3/F4/F5/F6 real-optimizer gates need
binaryen's `wasm-opt` discoverable in the AMBIENT environment (PATH or PYTHS_WASM_OPT) -- CI installs
it; without it those gates skip (or FAIL under PYTHSCRIBE_REQUIRE_ORACLE=1).
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import struct
import subprocess
import sys
from pathlib import Path

import pytest

from conftest import REPO, gate, import_module_from
from pythscribe import COMPILER_VERSION, binding_of
from pythscribe.artifacts import MANIFEST_NAME, ArtifactError, manifest_self_hash, resolve, verify
from pythscribe.build import BuildError, artifact_is_fresh, build_kernel, build_module, find_pyths
from pythscribe.build import main as build_main
from pythscribe.build import optimizer as optmod
from pythscribe.build.optimizer import (
    NO_VALIDATOR_DOCTOR_LINE,
    NO_VALIDATOR_ERROR,
    NO_WASM_OPT_NAME,
    OPTIMIZER_NONE,
    TAB_PATH_SENTENCE,
    WASM_OPT_ENV,
    WASM_OPT_FEATURE_FLAGS,
    Optimizer,
    custom_sections,
    encode_custom_section,
    is_bare_name,
    is_truly_absolute,
    optimizer_key,
    resolve_program,
    resolve_wasm_opt,
    wasm_opt_argv,
)
from pythscribe.decorators import jit_cache_key_dir
from pythscribe.runtime import wasmtime_available

HELLO_SRC = "from pythscribe import wasm\n\n@wasm\ndef hello(x: float) -> float:\n    return x + 2.0\n"
MARKER_NAME = "pythscribe.generated"
MARKER_CONTENTS = b"@generated by PythScribe"
MARKER_SECTION = encode_custom_section(MARKER_NAME, MARKER_CONTENTS)
SYSTEM_PATH = "C:\\Windows\\System32" if os.name == "nt" else "/usr/bin:/bin"
PROCUTIL_RS = REPO / "crates" / "pyths_cli" / "src" / "commands" / "procutil.rs"
OPTIMIZE_RS = REPO / "crates" / "pyths_codegen_wasm" / "src" / "optimize.rs"
# the AMBIENT optimizer, read ONCE before any test monkeypatches the environment
AMBIENT = resolve_wasm_opt()
# the argv shape BOTH shipped passes must log (paths abstracted): the positional contract, then the flags
ARGV_SHAPE = ["-Os", "<in>", "-o", "<out>", *WASM_OPT_FEATURE_FLAGS]


def _argv_shape(stub_log_line: str) -> list[str]:
    """`<argv> | PYTHS_WASM_OPT=...` (what `_make_stub(log=...)` records) -> the argv with the two
    path positions abstracted, so the Rust and Python passes can be compared token-for-token."""
    toks = stub_log_line.split(" | PYTHS_WASM_OPT=")[0].split()
    assert len(toks) >= 4, toks
    return [toks[0], "<in>", toks[2], "<out>", *toks[4:]]


def _bits(x: float) -> int:
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def _sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def _require_toolchain():
    try:
        find_pyths()
    except BuildError as e:
        gate(False, f"pyths compiler required ({e})")
    gate(wasmtime_available(), "wasmtime-py required (the publish gate + the value SPOT)")


def _require_real_wasm_opt() -> Path:
    gate(AMBIENT.available, "binaryen `wasm-opt` must be discoverable in the ambient env (PATH / PYTHS_WASM_OPT) for the real-optimizer gates")
    return AMBIENT.path  # type: ignore[return-value]


def _env_no_optimizer(monkeypatch, tmp_path: Path) -> None:
    """No wasm-opt anywhere: PATH = an empty dir (+ the system dir the compiler needs), no override."""
    empty = tmp_path / "empty-path"
    empty.mkdir(parents=True, exist_ok=True)
    monkeypatch.setenv("PATH", os.pathsep.join([str(empty), SYSTEM_PATH]))
    monkeypatch.delenv(WASM_OPT_ENV, raising=False)
    assert resolve_wasm_opt() == Optimizer(OPTIMIZER_NONE)


def _env_real_optimizer(monkeypatch) -> None:
    monkeypatch.setenv(WASM_OPT_ENV, str(_require_real_wasm_opt()))
    assert resolve_wasm_opt().available


def _write_kernel(tmp_path: Path, stem: str = "k") -> Path:
    tmp_path.mkdir(parents=True, exist_ok=True)
    src = tmp_path / f"{stem}.py"
    src.write_text(HELLO_SRC, encoding="utf-8")
    return src


def _build(src: Path):
    [info] = build_module(src, quiet=True)
    manifest = json.loads((info.dir / MANIFEST_NAME).read_text(encoding="utf-8"))
    return info, manifest


def _assert_clean(adir: Path) -> None:
    leaked = [p.name for p in adir.iterdir() if p.name.startswith(".pyths-opt-")]
    assert not leaked, f"private optimizer tmp dir leaked into the artifact: {leaked}"


def _run_hello(src: Path) -> tuple[float, str]:
    """Import the module beside its artifact through the SHIPPED wrapper and call it (server path)."""
    mod = import_module_from(src, f"m3_{src.stem}_{abs(hash(str(src)))}")
    b = binding_of(mod.hello)
    assert b.artifact_status == "resolved", b.artifact_status
    assert b.mode == "server", b.mode_reason
    out = mod.hello(1.0)
    assert b.server_calls == 1
    return out, b.mode


def _unoptimized_reference(tmp_path: Path, monkeypatch) -> bytes:
    """The bytes a no-optimizer build of HELLO_SRC produces (deterministic compiler output)."""
    with monkeypatch.context() as m:
        _env_no_optimizer(m, tmp_path)
        info, manifest = _build(_write_kernel(tmp_path / "ref", "ref"))
    assert manifest["optimizer"]["id"] == OPTIMIZER_NONE
    return info.wasm.read_bytes()


# ----------------------------------------------------------------------------- stubs + hand-built modules


def _make_stub(d: Path, *, probe_ok: bool = True, payload: Path | None = None, passthrough: bool = False,
               exit_code: int = 0, hang: bool = False, log: Path | None = None) -> Path:
    """A `wasm-opt` stand-in on PATH (`wasm-opt.cmd` on Windows, a sh script elsewhere). Called as
    `wasm-opt -Os <in> -o <out>`: writes `payload` (or copies `<in>` when passthrough) to `<out>`,
    optionally logs `<argv> | PYTHS_WASM_OPT=<value>` to `log`, optionally hangs, then exits `exit_code`."""
    d.mkdir(parents=True, exist_ok=True)
    log_s = str(log) if log else ""
    if os.name == "nt":
        p = d / "wasm-opt.cmd"
        lines = ["@echo off"]
        if probe_ok:
            lines.append('if "%~1"=="--version" ( echo wasm-opt version 999 ^(stub^) & exit /b 0 )')
        else:
            lines.append('if "%~1"=="--version" ( echo stub: broken & exit /b 1 )')
        if log:
            lines.append(f'echo %* ^| PYTHS_WASM_OPT=%PYTHS_WASM_OPT%>> "{log_s}"')
        if passthrough:
            lines.append('copy /b "%~2" "%~4" >nul')
        elif payload is not None:
            lines.append(f'copy /b "{payload}" "%~4" >nul')
        if hang:
            lines.append("ping -n 6 127.0.0.1 >nul 2>&1")
        lines.append(f"exit /b {exit_code}")
        p.write_text("\r\n".join(lines) + "\r\n", encoding="ascii")
    else:
        p = d / "wasm-opt"
        lines = ["#!/bin/sh"]
        if probe_ok:
            lines.append('if [ "$1" = "--version" ]; then echo "wasm-opt version 999 (stub)"; exit 0; fi')
        else:
            lines.append('if [ "$1" = "--version" ]; then echo "stub: broken" >&2; exit 1; fi')
        if log:
            lines.append(f'echo "$* | PYTHS_WASM_OPT=$PYTHS_WASM_OPT" >> "{log_s}"')
        if passthrough:
            lines.append('cp "$2" "$4"')
        elif payload is not None:
            lines.append(f'cp "{payload}" "$4"')
        if hang:
            lines.append("sleep 5")
        lines.append(f"exit {exit_code}")
        p.write_text("\n".join(lines) + "\n", encoding="ascii")
        p.chmod(0o755)
    return p


def _env_stub(monkeypatch, stub_dir: Path) -> None:
    """The stub is the ONLY wasm-opt: found by a bare-name PATH search (PATHEXT on Windows)."""
    monkeypatch.setenv("PATH", os.pathsep.join([str(stub_dir), SYSTEM_PATH]))
    monkeypatch.delenv(WASM_OPT_ENV, raising=False)


def _section(sid: int, payload: bytes) -> bytes:
    return bytes([sid]) + optmod._leb128_u32(len(payload)) + payload


def _hand_module(add_opcode: int, *, marker: bool, extra: bytes = b"") -> bytes:
    """A minimal module exporting `hello: (f64) -> f64 = x <add> 2.0`. add_opcode 0xA0 = f64.add
    (valid); 0x6A = i32.add (type-INVALID but structurally well-formed -- the rev-6 reproduced case)."""
    from pythscribe.runtime.abi import ABI_GLOBAL_EXPORT, SUPPORTED_ABI_MAJOR

    type_sec = _section(1, b"\x01\x60\x01\x7c\x01\x7c")
    func_sec = _section(3, b"\x01\x00")
    # B7: the ABI gate requires an exported immutable i32 `__pyths_abi` global == the major (in
    # addition to the pyths.abi section carried by MARKER_SECTION) -- global 0, i32.const major.
    # (major is small, so a single-byte sLEB128 suffices.)
    global_sec = _section(6, b"\x01" + b"\x7f\x00\x41" + bytes([SUPPORTED_ABI_MAJOR & 0x7F]) + b"\x0b")
    _an = ABI_GLOBAL_EXPORT.encode()
    export_sec = _section(7, b"\x02" + b"\x05hello\x00\x00" + bytes([len(_an)]) + _an + b"\x03\x00")
    body = b"\x00" + b"\x20\x00" + b"\x44" + struct.pack("<d", 2.0) + bytes([add_opcode]) + b"\x0b"
    code_sec = _section(10, b"\x01" + optmod._leb128_u32(len(body)) + body)
    out = b"\0asm\x01\x00\x00\x00" + type_sec + func_sec + global_sec + export_sec + code_sec + extra
    return out + (MARKER_SECTION if marker else b"")


def test_hand_module_fixtures_are_what_they_claim():
    """The fixtures' OWN control: the valid twin validates and runs 3.0; the type-invalid twin is
    structurally identical (same sections, same size) yet rejected by wasmtime."""
    _require_toolchain()
    import wasmtime as wt

    eng = wt.Engine()
    good, bad = _hand_module(0xA0, marker=True), _hand_module(0x6A, marker=True)
    assert len(good) == len(bad) and dict(custom_sections(good)) == dict(custom_sections(bad))
    wt.Module.validate(eng, good)
    with pytest.raises(wt.WasmtimeError):
        wt.Module.validate(eng, bad)
    store = wt.Store(eng)
    inst = wt.Instance(store, wt.Module(eng, good), [])
    assert _bits(inst.exports(store)["hello"](store, 1.0)) == _bits(3.0)


def test_marker_encoder_matches_the_compiler(tmp_path, monkeypatch):
    """Binds the hand-built fixtures to the SHIPPED compiler: the ownership section the pinned
    `pyths` emits is byte-equal to our encoding, and it is the only custom section (pre-M2.1)."""
    _require_toolchain()
    _env_no_optimizer(monkeypatch, tmp_path)
    info, _ = _build(_write_kernel(tmp_path))
    secs = dict(custom_sections(info.wasm.read_bytes()))
    assert secs[MARKER_NAME] == MARKER_SECTION
    assert set(secs) <= set(optmod.PRESERVED_SECTIONS)


# ----------------------------------------------------------------------------- F1 / F2 / F2b


def test_f1_absent_records_none_exit_0(tmp_path, monkeypatch):
    _require_toolchain()
    _env_no_optimizer(monkeypatch, tmp_path)
    src = _write_kernel(tmp_path)
    assert build_main([str(src), "--quiet"]) == 0
    info = verify(tmp_path / "__pythscribe__" / "hello", function="hello", expected_source_sha256=None)
    assert info.manifest["optimizer"] == {"id": OPTIMIZER_NONE, "path_sha256": None, "applied": False, "error": None}
    assert dict(custom_sections(info.wasm.read_bytes()))[MARKER_NAME] == MARKER_SECTION
    _assert_clean(info.dir)
    assert _bits(_run_hello(src)[0]) == _bits(3.0)


def test_f2_present_applied_size_reduced_validated_and_runs(tmp_path, monkeypatch):
    _require_toolchain()
    ref = _unoptimized_reference(tmp_path, monkeypatch)
    _env_real_optimizer(monkeypatch)
    src = _write_kernel(tmp_path)
    info, manifest = _build(src)
    opt = manifest["optimizer"]
    assert opt["applied"] is True and opt["error"] is None
    assert opt["id"].startswith("wasm-opt/") and opt["id"] == AMBIENT.id
    assert opt["path_sha256"] == _sha(AMBIENT.path.read_bytes())
    published = info.wasm.read_bytes()
    assert 0 < len(published) < len(ref), (len(published), len(ref))
    assert dict(custom_sections(published))[MARKER_NAME] == MARKER_SECTION  # (c) preserved byte-for-byte
    assert manifest["files"][manifest["wasm"]] == _sha(published)  # only published bytes are hashed
    verify(info.dir, function="hello", expected_source_sha256=info.source_sha256)  # the pass ran BEFORE the manifest step
    _assert_clean(info.dir)
    out, mode = _run_hello(src)  # the value SPOT on the REAL optimized bytes (rev-7)
    assert mode == "server" and _bits(out) == _bits(3.0)


def test_f2b_no_validator_never_publishes_and_states_the_tab_path_limitation(tmp_path, monkeypatch):
    _require_toolchain()
    ref = _unoptimized_reference(tmp_path, monkeypatch)
    src = _write_kernel(tmp_path)
    with monkeypatch.context() as m:
        _env_real_optimizer(m)
        m.setitem(sys.modules, "wasmtime", None)  # `import wasmtime` -> ImportError: the base install
        assert not wasmtime_available()
        info, manifest = _build(src)
    opt = manifest["optimizer"]
    assert opt["applied"] is False and opt["id"] == AMBIENT.id  # available-vs-applied: the identity is still recorded
    assert opt["error"] == NO_VALIDATOR_ERROR
    assert "no validator available (pip install pythscribe[server]); optimized output not adopted" in opt["error"]
    assert "applies to browser-tab artifacts too" in opt["error"]  # surface (3): the manifest
    assert info.wasm.read_bytes() == ref  # the ORIGINAL is the artifact, byte-identical
    assert manifest["files"][manifest["wasm"]] == _sha(ref)
    _assert_clean(info.dir)
    # The no-validator limitation is surfaced by the ENFORCED code constants: the build error (asserted
    # above: names the validator requirement + "applies to browser-tab artifacts too") and the doctor line.
    # The PyPI readme (pythscribe/README.md) is now a minimal landing page and delegates this detail to the
    # repo (user 2026-09-18), so the doc binding is the code surfaces, not that file's prose.
    assert TAB_PATH_SENTENCE in NO_VALIDATOR_DOCTOR_LINE and "install `pythscribe[server]`" in NO_VALIDATOR_DOCTOR_LINE
    assert "validator" in NO_VALIDATOR_DOCTOR_LINE  # the doctor line names the validator requirement
    assert _bits(_run_hello(src)[0]) == _bits(3.0)  # and the unoptimized artifact is a valid one


# ----------------------------------------------------------------------------- F3 / F4 / F8 (transitions)


def test_f3_f4_transitions_rebuild_and_repeat_is_noop(tmp_path, monkeypatch):
    _require_toolchain()
    src = _write_kernel(tmp_path)
    # start: none
    with monkeypatch.context() as m:
        _env_no_optimizer(m, tmp_path)
        info0, m0 = _build(src)
        b0 = info0.wasm.read_bytes()
        assert m0["optimizer"]["id"] == OPTIMIZER_NONE
    # F3 none -> present: rebuilt, bytes change, manifest updates
    with monkeypatch.context() as m:
        _env_real_optimizer(m)
        info1, m1 = _build(src)
        b1 = info1.wasm.read_bytes()
        assert m1["optimizer"]["applied"] is True and m1["optimizer"]["id"] == AMBIENT.id
        assert b1 != b0 and len(b1) < len(b0)
        # F4 repeated: no-op (manifest bytes + wasm mtime unchanged)
        mbytes, mtime = (info1.dir / MANIFEST_NAME).read_bytes(), info1.wasm.stat().st_mtime_ns
        info1b, _ = _build(src)
        assert (info1b.dir / MANIFEST_NAME).read_bytes() == mbytes and info1b.wasm.stat().st_mtime_ns == mtime
    # F3c X -> Y: a different wasm-opt version (a passthrough stub reporting version 999) -> rebuild
    with monkeypatch.context() as m:
        _env_stub(m, _make_stub(tmp_path / "stubY", passthrough=True).parent)
        assert resolve_wasm_opt().id == "wasm-opt/999"
        info2, m2 = _build(src)
        assert m2["optimizer"] == {"id": "wasm-opt/999", "path_sha256": _sha((tmp_path / "stubY" / os.listdir(tmp_path / "stubY")[0]).read_bytes()), "applied": True, "error": None}
        assert info2.wasm.read_bytes() == b0  # passthrough: the (validated) candidate is the input
    # F3b present -> none: rebuilt unoptimized
    with monkeypatch.context() as m:
        _env_no_optimizer(m, tmp_path)
        info3, m3 = _build(src)
        assert m3["optimizer"]["id"] == OPTIMIZER_NONE and info3.wasm.read_bytes() == b0
    _assert_clean(info3.dir)


def test_legacy_manifest_without_optimizer_field_is_valid_but_stale(tmp_path, monkeypatch):
    """A pre-M3 manifest (no `optimizer`) still passes INTEGRITY (verify) but is not FRESH -> the
    explicit build rebuilds it and the rebuilt manifest carries the field."""
    _require_toolchain()
    _env_no_optimizer(monkeypatch, tmp_path)
    src = _write_kernel(tmp_path)
    info, manifest = _build(src)
    del manifest["optimizer"]
    manifest["manifest_sha256"] = manifest_self_hash(manifest)
    (info.dir / MANIFEST_NAME).write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    legacy = verify(info.dir, function="hello", expected_source_sha256=info.source_sha256)  # integrity GREEN
    assert "optimizer" not in legacy.manifest
    assert artifact_is_fresh(legacy.manifest, Optimizer(OPTIMIZER_NONE)) is False
    info2, m2 = _build(src)
    assert m2["optimizer"]["id"] == OPTIMIZER_NONE


def test_f8_probe_failure_is_available_not_applied(tmp_path, monkeypatch):
    _require_toolchain()
    ref = _unoptimized_reference(tmp_path, monkeypatch)
    stub = _make_stub(tmp_path / "stub", probe_ok=False)
    _env_stub(monkeypatch, stub.parent)
    amb = resolve_wasm_opt()
    assert amb.id == OPTIMIZER_NONE and amb.path is None and "probe" in (amb.error or "")
    src = _write_kernel(tmp_path)
    assert build_main([str(src), "--quiet"]) == 0  # exit 0
    info = verify(tmp_path / "__pythscribe__" / "hello", function="hello", expected_source_sha256=None)
    opt = info.manifest["optimizer"]
    assert opt["applied"] is False and opt["id"] == OPTIMIZER_NONE and "--version probe" in opt["error"]
    assert opt["path_sha256"] == _sha(stub.read_bytes())  # diagnostic: WHICH binary failed
    assert info.wasm.read_bytes() == ref
    # the discriminator for the `applied: true`-on-failure mutant: the hashed sha IS the unoptimized one
    assert info.manifest["files"][info.manifest["wasm"]] == _sha(ref)
    _assert_clean(info.dir)


# ----------------------------------------------------------------------------- F5 / F6 (integrity vs freshness; JIT)


def test_f5_optimized_artifact_adopted_in_optimizer_free_env(tmp_path, monkeypatch):
    _require_toolchain()
    src = _write_kernel(tmp_path)
    with monkeypatch.context() as m:
        _env_real_optimizer(m)
        info, manifest = _build(src)
        assert manifest["optimizer"]["applied"] is True
    optimized = info.wasm.read_bytes()
    # the "deployment": same artifact, a machine with NO wasm-opt
    _env_no_optimizer(monkeypatch, tmp_path)
    assert resolve_wasm_opt().id == OPTIMIZER_NONE != manifest["optimizer"]["id"]
    got, status = resolve(src, "hello", info.source_sha256)
    assert status == "resolved" and got is not None and got.wasm.read_bytes() == optimized
    out, mode = _run_hello(src)
    assert mode == "server" and _bits(out) == _bits(3.0)
    assert info.wasm.read_bytes() == optimized  # loading never rebuilt/touched it

    # CONTROL: a verify() that consults the ambient optimizer makes the SAME artifact 'unusable' here
    real_verify = verify

    def freshness_leaking_verify(adir, *, function, expected_source_sha256):
        i = real_verify(adir, function=function, expected_source_sha256=expected_source_sha256)
        if i.manifest.get("optimizer", {}).get("id") != resolve_wasm_opt().id:
            raise ArtifactError("mutant: artifact built under a different optimizer")
        return i

    import pythscribe.artifacts as artmod

    monkeypatch.setattr(artmod, "verify", freshness_leaking_verify)
    _, status_mutant = artmod.resolve(src, "hello", info.source_sha256)
    assert status_mutant == "unusable", "the F5 gate would not catch a freshness-leaking verify()"


@pytest.fixture
def jit_env(tmp_path, monkeypatch):
    monkeypatch.setenv("PYTHSCRIBE_CACHE", str(tmp_path / "jit"))
    monkeypatch.delenv("PYTHSCRIBE_NO_JIT", raising=False)
    return tmp_path / "jit"


def _jit_call(tmp_path: Path, stem: str) -> tuple[Path, object]:
    src = tmp_path / f"{stem}.py"
    src.write_text(HELLO_SRC, encoding="utf-8")
    mod = import_module_from(src, f"m3jit_{stem}_{abs(hash(str(src)))}")
    b = binding_of(mod.hello)
    assert b.artifact_status == "absent"
    assert _bits(mod.hello(1.0)) == _bits(3.0)
    assert b.mode == "server", b._jit_reason
    return src, b


def test_f6_jit_cache_keyed_by_optimizer_id(jit_env, tmp_path, monkeypatch):
    _require_toolchain()
    with monkeypatch.context() as m:
        _env_real_optimizer(m)
        src_x, bx = _jit_call(tmp_path, "jx")
        key_x = jit_cache_key_dir(bx.source_sha256, AMBIENT.id)
        assert key_x == jit_env / COMPILER_VERSION / optimizer_key(AMBIENT.id) / bx.source_sha256  # the address shape
        assert (key_x / "__pythscribe__" / "hello" / MANIFEST_NAME).is_file(), key_x
        assert bx.artifact.dir == key_x / "__pythscribe__" / "hello"
        assert bx.artifact.manifest["optimizer"]["applied"] is True
        wasm_x = bx.artifact.wasm.read_bytes()
    # ambient none: the X entry is NOT adopted; a new entry appears under <ver>/none/<sha>; both coexist
    _env_no_optimizer(monkeypatch, tmp_path)
    src_n, bn = _jit_call(tmp_path, "jn")
    assert bn.source_sha256 == bx.source_sha256
    key_n = jit_cache_key_dir(bn.source_sha256, OPTIMIZER_NONE)
    assert key_n != key_x and key_n.parent.name == "none" and key_n.parent.parent == key_x.parent.parent
    assert bn.artifact.dir == key_n / "__pythscribe__" / "hello"
    assert bn.artifact.manifest["optimizer"]["id"] == OPTIMIZER_NONE
    assert bn.artifact.wasm.read_bytes() != wasm_x and len(bn.artifact.wasm.read_bytes()) > len(wasm_x)
    assert (key_x / "__pythscribe__" / "hello" / MANIFEST_NAME).is_file()  # the X entry still coexists


def test_f6_defensive_valid_at_refuses_cross_state_entry(jit_env, tmp_path, monkeypatch):
    """The defensive half: an artifact whose manifest says `wasm-opt/…` PLANTED under the `none` key
    (a mis-filed / hand-copied entry) is not adopted; the process uses a private `none` build."""
    _require_toolchain()
    with monkeypatch.context() as m:
        _env_real_optimizer(m)
        _, bx = _jit_call(tmp_path, "px")
        key_x = jit_cache_key_dir(bx.source_sha256, AMBIENT.id)
    _env_no_optimizer(monkeypatch, tmp_path)
    key_n = jit_cache_key_dir(bx.source_sha256, OPTIMIZER_NONE)
    shutil.copytree(key_x, key_n)  # plant the X entry at the none address (verify() passes on it)
    planted = key_n / "__pythscribe__" / "hello"
    assert verify(planted, function="hello", expected_source_sha256=bx.source_sha256).manifest["optimizer"]["id"] == AMBIENT.id
    _, bn = _jit_call(tmp_path, "pn")
    assert bn.artifact.manifest["optimizer"]["id"] == OPTIMIZER_NONE, "a cross-optimizer-state entry was adopted"
    assert bn.artifact.dir != planted
    assert verify(planted, function="hello", expected_source_sha256=bx.source_sha256).manifest["optimizer"]["id"] == AMBIENT.id  # untouched


def test_optimizer_key_is_one_path_segment():
    assert optimizer_key(OPTIMIZER_NONE) == "none"
    assert optimizer_key("wasm-opt/112") == "wasm-opt_112"
    for bad in ("wasm-opt/1.2.3", "a/b\\c:d", "../x", "wasm-opt/version_123 (x)"):
        k = optimizer_key(bad)
        assert k and "/" not in k and "\\" not in k and ":" not in k and k not in (".", "..") and not k.startswith(".")


# ----------------------------------------------------------------------------- F7 resolver parity


# The fixture table SHARED with `procutil.rs`'s unit tests (relative_env_override_is_refused,
# drive_relative_and_rooted_overrides_are_refused, forward_slash_unc_is_absolute_like_backslash_unc,
# bare_name_classification, resolve_program_skips_cwd_planted_binary, relative_path_entries_are_skipped).
RELATIVE_OVERRIDES = ["./evil.exe", ".\\evil.exe", "sub/evil"]
WINDOWS_NONBARE_OVERRIDES = ["C:evil.exe", "C:sub\\evil.exe", "\\evil.exe", "prog:ads.exe"]
WINDOWS_ABSOLUTE_SHAPES = ["//server/share/x.exe", "\\\\server/share/x.exe", "/\\server\\share\\x.exe", "\\/server/share/x.exe",
                           "\\\\server\\share\\x.exe", "\\\\?\\C:\\x.exe", "\\\\.\\COM1"]
WINDOWS_NOT_ABSOLUTE_SHAPES = ["//server", "//server/", "//server//x.exe", "///a/b.exe", "\\\\server", "\\\\server\\\\share\\x.exe"]
UNC_MISSING_OVERRIDES = ["//server/share/pyths_probe.exe", "\\\\server\\share\\pyths_probe.exe"]
BARE = ["node", "wasm-opt"]
NOT_BARE = ["./node", "sub/node", ".."]
WINDOWS_NOT_BARE = ["C:evil.exe", "\\evil.exe", "node:ads", "C:\\real\\node.exe"]


def test_f7_fixture_table_is_shared_with_procutil_rs():
    """Drift gate: every fixture literal above is present, verbatim, in procutil.rs's tests -- the
    table is SHARED, not a copy that can silently diverge."""
    rs = PROCUTIL_RS.read_text(encoding="utf-8")

    def rust_literal(s: str) -> str:
        return '"' + s.replace("\\", "\\\\") + '"'

    for s in RELATIVE_OVERRIDES + WINDOWS_NONBARE_OVERRIDES + WINDOWS_ABSOLUTE_SHAPES + WINDOWS_NOT_ABSOLUTE_SHAPES + UNC_MISSING_OVERRIDES + BARE + NOT_BARE + WINDOWS_NOT_BARE:
        assert rust_literal(s) in rs, f"fixture {s!r} is not in procutil.rs (table drifted)"
    for name in ("pyths_fake_prog", "pyths_probe_bin", "pyths_relpath_probe", 'set_var("PATH", "tools")'):
        assert name in rs


def test_f7_bare_name_classification():
    for s in BARE:
        assert is_bare_name(s)
    for s in NOT_BARE:
        assert not is_bare_name(s)
    if os.name == "nt":
        for s in WINDOWS_NOT_BARE:
            assert not is_bare_name(s), s


@pytest.mark.skipif(os.name != "nt", reason="Windows path prefixes (the #442 anchor)")
def test_f7_windows_absoluteness_matches_std_path():
    for s in WINDOWS_ABSOLUTE_SHAPES:
        assert is_truly_absolute(s) and not is_bare_name(s), s
    for s in WINDOWS_NOT_ABSOLUTE_SHAPES + ["C:x", "\\x", "x"]:
        assert not is_truly_absolute(s), s
    assert is_truly_absolute("C:\\x") and is_truly_absolute("C:/x")


def test_f7_relative_and_drive_relative_overrides_are_refused(tmp_path, monkeypatch):
    planted = tmp_path / ("evil.exe" if os.name == "nt" else "evil")
    planted.write_bytes(b"x")
    monkeypatch.chdir(tmp_path)  # a cwd-resolving mutant WOULD find ./evil.exe here -> RED
    cases = RELATIVE_OVERRIDES + (WINDOWS_NONBARE_OVERRIDES if os.name == "nt" else [])
    for rel in cases:
        monkeypatch.setenv(WASM_OPT_ENV, rel)
        assert resolve_program("node", WASM_OPT_ENV) is None, rel
        r = resolve_wasm_opt()
        assert r.id == OPTIMIZER_NONE and r.path is None and "refused" in (r.error or ""), rel
    # absolute-missing: refused (the exact contract build_kernel relies on for the compiler's pass)
    monkeypatch.setenv(WASM_OPT_ENV, str(tmp_path / NO_WASM_OPT_NAME))
    assert resolve_program("wasm-opt", WASM_OPT_ENV) is None and resolve_wasm_opt().id == OPTIMIZER_NONE
    for unc in UNC_MISSING_OVERRIDES:
        monkeypatch.setenv(WASM_OPT_ENV, unc)
        assert resolve_program("node", WASM_OPT_ENV) is None, unc
    # an ABSOLUTE override to an existing file is honoured
    monkeypatch.setenv(WASM_OPT_ENV, str(planted))
    assert resolve_program("node", WASM_OPT_ENV) == planted
    if os.name == "nt":
        assert resolve_program("C:evil.exe", None) is None and resolve_program("\\evil.exe", None) is None


def test_f7_cwd_planted_binary_is_never_found(tmp_path, monkeypatch):
    name = "pyths_fake_prog"
    (tmp_path / (name + (".exe" if os.name == "nt" else ""))).write_bytes(b"#!/bin/sh\n")
    monkeypatch.chdir(tmp_path)
    monkeypatch.setenv("PATH", SYSTEM_PATH)
    monkeypatch.delenv(WASM_OPT_ENV, raising=False)
    assert resolve_program(name, None) is None
    monkeypatch.setenv(WASM_OPT_ENV, name)  # a BARE override is PATH-searched, never the cwd
    assert resolve_program("wasm-opt", WASM_OPT_ENV) is None
    # empty and `.` PATH entries mean "cwd" on Windows: skipped
    monkeypatch.setenv("PATH", os.pathsep.join(["", ".", SYSTEM_PATH]))
    assert resolve_program(name, None) is None


def test_f7_path_dir_and_relative_path_entries(tmp_path, monkeypatch):
    name = "pyths_probe_bin"
    fname = name + (".exe" if os.name == "nt" else "")
    (tmp_path / fname).write_bytes(b"x")
    monkeypatch.setenv("PATH", os.pathsep.join([str(tmp_path), SYSTEM_PATH]))
    monkeypatch.delenv(WASM_OPT_ENV, raising=False)
    got = resolve_program(name, None)
    assert got is not None and got.is_absolute() and got.resolve() == (tmp_path / fname).resolve()
    if os.name == "nt":  # PATHEXT: `.exe` is appended, and a lowercased/space-padded PATHEXT still works
        monkeypatch.setenv("PATHEXT", " .com; .exe ;.bat")
        assert resolve_program(name, None) == tmp_path / fname
    # `PATH=tools` (relative entry) must never resolve `tools/x` against the cwd
    sub = tmp_path / "tools"
    sub.mkdir()
    (sub / ("pyths_relpath_probe" + (".exe" if os.name == "nt" else ""))).write_bytes(b"x")
    monkeypatch.chdir(tmp_path)
    monkeypatch.setenv("PATH", "tools")
    assert resolve_program("pyths_relpath_probe", None) is None


# ----------------------------------------------------------------------------- F8b-F8g: the never-in-place pass


def _stub_build(tmp_path, monkeypatch, **stub_kw):
    """Build HELLO_SRC with a stub as the only wasm-opt; returns (ref bytes, info, manifest, stub)."""
    ref = _unoptimized_reference(tmp_path, monkeypatch)
    stub = _make_stub(tmp_path / "stub", **stub_kw)
    _env_stub(monkeypatch, stub.parent)
    assert resolve_wasm_opt().id == "wasm-opt/999"
    src = _write_kernel(tmp_path)
    info, manifest = _build(src)
    return ref, src, info, manifest, stub


def _assert_original_kept(ref: bytes, info, manifest, *, error_fragment: str, src: Path) -> None:
    opt = manifest["optimizer"]
    assert opt["applied"] is False and opt["id"] == "wasm-opt/999", opt
    assert error_fragment in (opt["error"] or ""), opt["error"]
    assert info.wasm.read_bytes() == ref  # (a) original byte-identical
    assert manifest["files"][manifest["wasm"]] == _sha(ref)  # (d) the original is what is hashed
    verify(info.dir, function="hello", expected_source_sha256=info.source_sha256)
    _assert_clean(info.dir)
    assert _bits(_run_hello(src)[0]) == _bits(3.0)  # (e) instantiation + value SPOT


def test_f8b_partial_output_nonzero_exit_keeps_original(tmp_path, monkeypatch):
    _require_toolchain()
    ref0 = _unoptimized_reference(tmp_path / "pre", monkeypatch)
    partial = tmp_path / "partial.wasm"
    partial.write_bytes(ref0[:40])  # truncated mid-module
    ref, src, info, manifest, _ = _stub_build(tmp_path, monkeypatch, payload=partial, exit_code=1)
    assert ref == ref0
    _assert_original_kept(ref, info, manifest, error_fragment="wasm-opt failed (exit 1)", src=src)
    # the in-place mutant's signature: the artifact would BE the truncated bytes and hash them
    assert info.wasm.read_bytes() != partial.read_bytes() and manifest["files"][manifest["wasm"]] != _sha(partial.read_bytes())


def test_f8c_garbage_with_exit_0_is_refused(tmp_path, monkeypatch):
    _require_toolchain()
    garbage = tmp_path / "garbage.wasm"
    garbage.write_bytes(b"this is not a wasm module, exit code says otherwise")
    ref, src, info, manifest, _ = _stub_build(tmp_path, monkeypatch, payload=garbage, exit_code=0)
    _assert_original_kept(ref, info, manifest, error_fragment="not a well-formed WebAssembly module", src=src)


def test_f8d_stripped_section_is_reappended_into_the_candidate(tmp_path, monkeypatch):
    _require_toolchain()
    stripped = tmp_path / "stripped.wasm"
    stripped.write_bytes(_hand_module(0xA0, marker=False))  # valid, runs x+2.0, NO ownership section
    ref, src, info, manifest, _ = _stub_build(tmp_path, monkeypatch, payload=stripped, exit_code=0)
    opt = manifest["optimizer"]
    assert opt["applied"] is True and opt["error"] is None
    published = info.wasm.read_bytes()
    # M2.1 integration: `pyths.abi` is now ALSO a PRESERVED_SECTION, re-appended after the marker (in
    # PRESERVED_SECTIONS order). The candidate carried neither, so both raw sections come back.
    abi_raw = dict(custom_sections(ref)).get("pyths.abi", b"")
    assert published == stripped.read_bytes() + MARKER_SECTION + abi_raw  # both preserved sections re-appended, nothing else
    assert dict(custom_sections(published))[MARKER_NAME] == dict(custom_sections(ref))[MARKER_NAME]
    assert manifest["files"][manifest["wasm"]] == _sha(published)
    _assert_clean(info.dir)
    assert _bits(_run_hello(src)[0]) == _bits(3.0)


def test_f8d_altered_section_is_refused(tmp_path, monkeypatch):
    """A candidate whose ownership section is PRESENT but different is not 'ours' -> refused."""
    _require_toolchain()
    altered = tmp_path / "altered.wasm"
    altered.write_bytes(_hand_module(0xA0, marker=False) + encode_custom_section(MARKER_NAME, b"@generated by SomethingElse"))
    ref, src, info, manifest, _ = _stub_build(tmp_path, monkeypatch, payload=altered, exit_code=0)
    _assert_original_kept(ref, info, manifest, error_fragment="altered the `pythscribe.generated` custom section", src=src)


def test_f8e_killed_mid_write_keeps_original_and_removes_tmp(tmp_path, monkeypatch):
    _require_toolchain()
    monkeypatch.setattr(optmod, "OPTIMIZER_TIMEOUT_S", 1.5)
    ref0 = _unoptimized_reference(tmp_path / "pre", monkeypatch)
    partial = tmp_path / "partial.wasm"
    partial.write_bytes(ref0[:40])
    ref, src, info, manifest, _ = _stub_build(tmp_path, monkeypatch, payload=partial, exit_code=0, hang=True)
    _assert_original_kept(ref, info, manifest, error_fragment="timed out", src=src)


def test_f8f_type_invalid_rejected_by_the_validator(tmp_path, monkeypatch):
    _require_toolchain()
    bad = tmp_path / "typeinvalid.wasm"
    bad.write_bytes(_hand_module(0x6A, marker=True))  # structurally valid, section intact, i32.add on f64s
    ref, src, info, manifest, _ = _stub_build(tmp_path, monkeypatch, payload=bad, exit_code=0)
    _assert_original_kept(ref, info, manifest, error_fragment="failed validation", src=src)
    assert manifest["files"][manifest["wasm"]] != _sha(bad.read_bytes())


def test_f8f_ii_type_invalid_without_wasmtime_is_never_published(tmp_path, monkeypatch):
    _require_toolchain()
    bad = tmp_path / "typeinvalid.wasm"
    bad.write_bytes(_hand_module(0x6A, marker=True))
    ref = _unoptimized_reference(tmp_path, monkeypatch)
    stub = _make_stub(tmp_path / "stub", payload=bad, exit_code=0)
    src = _write_kernel(tmp_path)
    with monkeypatch.context() as m:
        _env_stub(m, stub.parent)
        m.setitem(sys.modules, "wasmtime", None)
        info, manifest = _build(src)
    opt = manifest["optimizer"]
    assert opt["applied"] is False and opt["error"] == NO_VALIDATOR_ERROR
    assert info.wasm.read_bytes() == ref and manifest["files"][manifest["wasm"]] == _sha(ref)
    _assert_clean(info.dir)


def test_f8f_mutant_publish_on_structural_walk_is_caught(tmp_path, monkeypatch):
    """PAIRED CONTROL for the publish gate (rev-6 B1): with the validator neutered (the rev-5 hole --
    a section walk 'validating' the module), the type-invalid output IS published and hashed as fresh,
    and a `[server]` instantiation then FAILS. This proves the wasmtime gate is load-bearing: the
    F8f assertions discriminate the mutant."""
    _require_toolchain()
    from pythscribe.runtime import ServerKernel, WasmTrap

    bad = tmp_path / "typeinvalid.wasm"
    bad.write_bytes(_hand_module(0x6A, marker=True))
    monkeypatch.setattr(optmod, "_load_validator", lambda: (lambda final: custom_sections(final)))  # structural walk only
    ref, src, info, manifest, _ = _stub_build(tmp_path, monkeypatch, payload=bad, exit_code=0)
    assert manifest["optimizer"]["applied"] is True  # the mutant believes it succeeded
    # M2.1: `bad` carries the marker but not `pyths.abi`, so the missing preserved section is re-appended
    # (re-append precedes the neutered validator); the type-invalid body still ships.
    published_bad = bad.read_bytes() + dict(custom_sections(ref)).get("pyths.abi", b"")
    assert info.wasm.read_bytes() == published_bad and manifest["files"][manifest["wasm"]] == _sha(published_bad)
    verify(info.dir, function="hello", expected_source_sha256=info.source_sha256)  # integrity cannot see it...
    with pytest.raises(WasmTrap):  # ...but the server path does: silent corruption shipped
        ServerKernel.from_artifact(info)


def test_f8g_validation_runs_on_the_final_bytes_after_reappend(tmp_path, monkeypatch):
    """Fault-inject the re-append (a malformed section: the input's raw bytes truncated) with a stub
    that strips the section: the correct order (validate AFTER re-append) REJECTS the final bytes and
    keeps the original. A validate-before-re-append mutant would have published malformed bytes."""
    _require_toolchain()
    real = optmod.custom_sections

    def truncating(b: bytes):
        secs = real(b)
        if any(n == MARKER_NAME for n, _ in secs) and b.endswith(MARKER_SECTION):
            return [(n, raw[:-3] if n == MARKER_NAME else raw) for n, raw in secs]  # only the INPUT carries it here
        return secs

    monkeypatch.setattr(optmod, "custom_sections", truncating)
    stripped = tmp_path / "stripped.wasm"
    stripped.write_bytes(_hand_module(0xA0, marker=False))
    ref, src, info, manifest, _ = _stub_build(tmp_path, monkeypatch, payload=stripped, exit_code=0)
    assert manifest["optimizer"]["applied"] is False and "failed validation" in manifest["optimizer"]["error"]
    assert info.wasm.read_bytes() == ref
    _assert_clean(info.dir)


# ----------------------------------------------------------------------------- F9 compiler pass off by contract


def test_f9_compiler_own_pass_is_off_by_contract(tmp_path, monkeypatch):
    _require_toolchain()
    pyths = find_pyths()
    log = tmp_path / "stub.log"
    stub = _make_stub(tmp_path / "stub", passthrough=True, exit_code=0, log=log)
    _env_stub(monkeypatch, stub.parent)

    def compile_to(d: Path, env_extra: dict[str, str], verbose: bool = False) -> subprocess.CompletedProcess:
        d.mkdir(parents=True, exist_ok=True)
        ps = d / "hello.ps"
        ps.write_text("def hello(x: float) -> float:\n    return x + 2.0\n", encoding="utf-8", newline="\n")
        cmd = [str(pyths), "compile", str(ps), "--target", "js+wasm", "-o", str(d / "hello.js"), "--no-dts"] + (["--verbose"] if verbose else ["--quiet"])
        return subprocess.run(cmd, capture_output=True, text=True, check=False, cwd=str(d), env={**os.environ, **env_extra}, timeout=120)

    # (i) the contract: absolute-missing override -> the compiler does NOT run the stub, says so under --verbose
    off_dir = tmp_path / "off"
    r = compile_to(off_dir, {WASM_OPT_ENV: str(off_dir / NO_WASM_OPT_NAME)}, verbose=True)
    assert r.returncode == 0, r.stderr
    assert not log.exists(), f"the compiler ran wasm-opt despite the refused override: {log.read_text()}"
    assert "wasm-opt not found" in (r.stdout + r.stderr)
    # (ii) same bytes as a compile with NO wasm-opt reachable at all
    none_dir = tmp_path / "none"
    with monkeypatch.context() as m:
        _env_no_optimizer(m, tmp_path)
        r2 = compile_to(none_dir, {})
    assert r2.returncode == 0 and (none_dir / "hello.wasm").read_bytes() == (off_dir / "hello.wasm").read_bytes()
    # (iii) CONTROL: without the override the compiler's pass IS reachable (the stub runs) -- so it was
    # the contract, not an unreachable stub, that kept it off
    on_dir = tmp_path / "on"
    r3 = compile_to(on_dir, {})
    assert log.exists() and "-o" in log.read_text(encoding="utf-8"), (r3.stdout, r3.stderr)
    [compiler_line] = log.read_text(encoding="utf-8").strip().splitlines()
    log.unlink()
    # (iv) through build_kernel: the stub runs EXACTLY ONCE, by the Python pass (env has no `.no-wasm-opt`
    # in that invocation), never by the compiler subprocess (whose env carries the refused override)
    src = _write_kernel(tmp_path)
    info, manifest = _build(src)
    lines = log.read_text(encoding="utf-8").strip().splitlines()
    assert len(lines) == 1, lines
    assert NO_WASM_OPT_NAME not in lines[0] and "-Os" in lines[0]
    assert manifest["optimizer"]["applied"] is True and manifest["optimizer"]["id"] == "wasm-opt/999"
    assert ".pyths-opt-" in lines[0] and "-o" in lines[0]  # never in place: the -o target is in the private tmp dir
    assert info.wasm.read_bytes() == (none_dir / "hello.wasm").read_bytes()  # passthrough == the compiler's bytes
    # (v) TWO-COPIES gate through BOTH shipped passes: the Rust compiler's argv (iii) and the Python
    # pass's argv (iv) are the SAME shape -- positional contract, then the feature flags, nothing else.
    # Dropping `--enable-mutable-globals` from either copy (or reordering it) -> RED.
    assert _argv_shape(compiler_line) == ARGV_SHAPE, compiler_line
    assert _argv_shape(lines[0]) == ARGV_SHAPE, lines[0]
    assert wasm_opt_argv("W", "I", "O") == ["W", "-Os", "I", "-o", "O", *WASM_OPT_FEATURE_FLAGS]


# ----------------------------------------------------------------------------- F10 the wasm-opt feature flags


def test_f10_feature_flags_are_shared_with_optimize_rs():
    """Drift gate (same discipline as F7): `WASM_OPT_FEATURE_FLAGS` is PARSED out of optimize.rs and
    must equal the Python tuple, in order -- a shared table, not a copy that can silently diverge.
    Mutating either side (drop / add / reorder a flag) -> RED."""
    rs = OPTIMIZE_RS.read_text(encoding="utf-8")
    m = re.search(r"pub const WASM_OPT_FEATURE_FLAGS: &\[&str\] = &\[(.*?)\];", rs, re.S)
    assert m, "optimize.rs no longer defines `pub const WASM_OPT_FEATURE_FLAGS: &[&str] = &[...]`"
    rust_flags = tuple(re.findall(r'"([^"]*)"', m.group(1)))
    assert rust_flags == WASM_OPT_FEATURE_FLAGS, (rust_flags, WASM_OPT_FEATURE_FLAGS)
    assert "--enable-mutable-globals" in WASM_OPT_FEATURE_FLAGS  # the flag this class exists for
    # and the Rust argv builder places them TRAILING (positions 1-4 stay the positional contract)
    assert re.search(r'"-o"\.to_string\(\),\s*output\.to_string\(\),\s*\];\s*argv\.extend\(WASM_OPT_FEATURE_FLAGS', rs), (
        "optimize.rs::wasm_opt_argv no longer appends the feature flags AFTER `<level> <in> -o <out>`"
    )


def test_f10_mutable_globals_flag_is_load_bearing_on_the_real_optimizer(tmp_path, monkeypatch):
    """The compiled module EXPORTS a mutable global (the FFI contract), so the feature is REQUIRED:
    the shipped argv exits 0, and the SAME argv with the feature switched off is refused by the real
    binaryen naming `mutable` (the negative control -- what an unflagged older binaryen, e.g.
    version_105 / Ubuntu jammy apt, does to the shipped argv MINUS the flag)."""
    _require_toolchain()
    wasm_opt = _require_real_wasm_opt()
    ref = _unoptimized_reference(tmp_path, monkeypatch)
    inp = tmp_path / "ref.wasm"
    inp.write_bytes(ref)
    out_ok, out_bad = tmp_path / "ok.wasm", tmp_path / "bad.wasm"
    shipped = wasm_opt_argv(wasm_opt, inp, out_ok)
    assert shipped[-len(WASM_OPT_FEATURE_FLAGS):] == list(WASM_OPT_FEATURE_FLAGS)
    r = subprocess.run(shipped, capture_output=True, text=True, check=False, timeout=120)
    assert r.returncode == 0 and 0 < out_ok.stat().st_size < len(ref), (r.stderr, r.stdout)
    r_bad = subprocess.run([*wasm_opt_argv(wasm_opt, inp, out_bad), "--disable-mutable-globals"],
                           capture_output=True, text=True, check=False, timeout=120)
    assert r_bad.returncode != 0, "the compiled module does not need mutable-globals -- revisit WASM_OPT_FEATURE_FLAGS"
    assert "mutable" in (r_bad.stderr + r_bad.stdout).lower(), (r_bad.stderr, r_bad.stdout)
    assert not out_bad.exists() or out_bad.stat().st_size == 0


def test_f10_off_sentinel_is_silent_none_and_byte_identical_to_an_optimizer_free_build(tmp_path, monkeypatch):
    """The pip gate's committed-artifact rebuild property: with a REAL wasm-opt on PATH, a forced build
    under `PYTHS_WASM_OPT=<abs>/.no-wasm-opt` is byte-identical (wasm AND manifest) to the build on a
    machine with no wasm-opt at all. Controls: (a) without the sentinel the same env DOES optimize
    (the sentinel is load-bearing); (b) a non-sentinel missing override keeps its `refused`
    diagnostic in the manifest (the silence is narrow)."""
    _require_toolchain()
    real = _require_real_wasm_opt()
    src = _write_kernel(tmp_path)
    with monkeypatch.context() as m:
        _env_no_optimizer(m, tmp_path)
        [info0] = build_module(src, quiet=True, force=True)
        wasm0, manifest0 = info0.wasm.read_bytes(), (info0.dir / MANIFEST_NAME).read_bytes()
    assert json.loads(manifest0)["optimizer"] == {"id": OPTIMIZER_NONE, "path_sha256": None, "applied": False, "error": None}
    # the real optimizer on PATH + the OFF sentinel as the override
    monkeypatch.setenv("PATH", os.pathsep.join([str(real.parent), SYSTEM_PATH]))
    monkeypatch.setenv(WASM_OPT_ENV, str(tmp_path / NO_WASM_OPT_NAME))
    assert resolve_wasm_opt() == Optimizer(OPTIMIZER_NONE)  # exactly: no path, no sha, NO error
    [info1] = build_module(src, quiet=True, force=True)
    assert info1.wasm.read_bytes() == wasm0
    assert (info1.dir / MANIFEST_NAME).read_bytes() == manifest0
    _assert_clean(info1.dir)
    # control (a): drop the sentinel -> the real wasm-opt on PATH is used and the bytes change
    monkeypatch.delenv(WASM_OPT_ENV)
    assert resolve_wasm_opt().id == AMBIENT.id
    [info2] = build_module(src, quiet=True, force=True)
    assert info2.wasm.read_bytes() != wasm0 and json.loads((info2.dir / MANIFEST_NAME).read_text())["optimizer"]["applied"] is True
    # control (b): a NON-sentinel missing absolute override is still refused LOUDLY -- in the manifest too
    monkeypatch.setenv(WASM_OPT_ENV, str(tmp_path / "wasm-opt-missing"))
    r = resolve_wasm_opt()
    assert r.id == OPTIMIZER_NONE and "refused" in (r.error or "")
    [info3] = build_module(src, quiet=True, force=True)
    m3 = json.loads((info3.dir / MANIFEST_NAME).read_text())["optimizer"]
    assert m3["id"] == OPTIMIZER_NONE and "refused" in m3["error"] and info3.wasm.read_bytes() == wasm0
    assert (info3.dir / MANIFEST_NAME).read_bytes() != manifest0


def test_f10_relative_no_wasm_opt_is_refused_not_the_silent_sentinel(tmp_path, monkeypatch):
    """codex review (b/e): the SILENT OFF sentinel is ONLY a TRULY-ABSOLUTE `.no-wasm-opt`. A RELATIVE
    or drive-relative override ending in `.no-wasm-opt` must be REFUSED (loud) -- else a misconfigured
    relative override is indistinguishable from "no optimizer" and silently ships UNOPTIMIZED output.
    CONTROL: drop the `is_truly_absolute` guard in `is_off_sentinel` and the relative cases return
    Optimizer('none', error=None), so the `"refused"` assertions below go RED."""
    monkeypatch.chdir(tmp_path)
    rels = ["tools/.no-wasm-opt", "./.no-wasm-opt", "sub/.no-wasm-opt"]
    if os.name == "nt":
        rels += [r"sub\.no-wasm-opt", "C:.no-wasm-opt"]  # drive-relative (C:<no slash>) is NOT absolute
    for rel in rels:
        assert not optmod.is_off_sentinel(rel), f"relative {rel!r} must NOT be the silent sentinel"
        monkeypatch.setenv(WASM_OPT_ENV, rel)
        r = resolve_wasm_opt()
        assert r.id == OPTIMIZER_NONE and "refused" in (r.error or ""), (rel, r.error)
    # positive: a TRULY-ABSOLUTE (missing) `.no-wasm-opt` stays the SILENT none -- no `refused` diagnostic
    absent = tmp_path / NO_WASM_OPT_NAME
    assert optmod.is_off_sentinel(str(absent))
    monkeypatch.setenv(WASM_OPT_ENV, str(absent))
    assert resolve_wasm_opt() == Optimizer(OPTIMIZER_NONE)
