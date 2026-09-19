"""0.2.9: the once-per-process "wasmtime is not available" warning (`decorators._warn_if_no_runtime`).

wasmtime is a CORE dependency; a stripped install (`pip install --no-deps`, a vendored copy that dropped it)
makes every `@wasm` kernel silently run as plain Python with no speedup -- the footgun that made users
think `@wasm` was broken. The warning must be PRECISE (codex 0.2.9 review blocker 2): it fires ONLY when
wasmtime is the SOLE blocker -- auto mode, a usable artifact bound, the signature inside the FFI grammar,
the list use honourable, wasmtime absent -> the `browser` branch -- because that is the one case where
reinstalling wasmtime IS the fix. It must NOT fire for a fallback that wasmtime cannot repair (absent /
unusable / disabled / unreadable-source artifacts, a signature outside the grammar, an unsupported list
use), nor for an explicit `mode=browser|fallback|server`, and it must fire exactly ONCE even under a
thread race (the once-flag is lock-protected; a two-thread probe emitted two warnings before).

Deleting `_warn_if_no_runtime`, or calling it from the `fallback` branch again, or dropping the lock,
makes a test here RED. Captured via the `pythscribe` logger (caplog).
"""
from __future__ import annotations

import logging
import shutil
import sys
import threading
from pathlib import Path

import pytest

import pythscribe.decorators as dec
import pythscribe.ffi as ffi
import pythscribe.runtime as rt
from conftest import REPO, gate, import_module_from
from pythscribe import ModeError, binding_of
from pythscribe.artifacts import DISABLE_ENV, MANIFEST_NAME
from pythscribe.decorators import NO_RUNTIME_WHY

UC = REPO / "examples" / "wasm-use-cases"
NEEDLE = "wasm runtime (wasmtime) is not available"


def _warnings(caplog) -> list[logging.LogRecord]:
    return [r for r in caplog.records if r.name == "pythscribe" and r.levelno == logging.WARNING and NEEDLE in r.getMessage()]


@pytest.fixture
def art_dir(tmp_path) -> Path:
    """kernels.py + its committed artifacts (edit_distance resolves -> artifact bound)."""
    gate((UC / "__pythscribe__" / "edit_distance" / MANIFEST_NAME).is_file(), "use-case artifacts not built")
    d = tmp_path / "art"
    d.mkdir()
    shutil.copy(UC / "kernels.py", d / "kernels.py")
    shutil.copytree(UC / "__pythscribe__", d / "__pythscribe__")
    return d


@pytest.fixture
def noart_dir(tmp_path) -> Path:
    d = tmp_path / "noart"
    d.mkdir()
    shutil.copy(UC / "kernels.py", d / "kernels.py")
    return d


@pytest.fixture
def fresh(monkeypatch, caplog):
    """wasmtime ABSENT (the runtime probe, not a flag), the once-flag reset, warnings captured."""
    monkeypatch.setattr(rt, "wasmtime_available", lambda: False)
    monkeypatch.setattr(dec, "_warned_no_runtime", False)
    caplog.set_level(logging.INFO, logger="pythscribe")
    return caplog


def test_warns_once_when_wasmtime_is_the_sole_blocker(fresh, art_dir):
    """artifact bound + grammar ok + list use ok + wasmtime absent -> browser, ONE warning (even across
    a second module / kernel in the same process), naming the reinstall + `pyths doctor`."""
    m1 = import_module_from(art_dir / "kernels.py", "warn_art_1")
    b = binding_of(m1.edit_distance)
    assert b.mode == "browser" and b.mode_reason.endswith(f"({NO_RUNTIME_WHY})"), b.mode_reason
    import_module_from(art_dir / "kernels.py", "warn_art_2")  # a second binding: still once
    ws = _warnings(fresh)
    assert len(ws) == 1, [r.getMessage() for r in ws]
    msg = ws[0].getMessage()
    assert "pip install --upgrade pythscribe" in msg and "pyths doctor" in msg and "[server]" not in msg


def test_no_warning_when_no_artifact_is_bound(fresh, noart_dir):
    """status 'absent': fallback -- reinstalling wasmtime cannot change the outcome -> silent."""
    b = binding_of(import_module_from(noart_dir / "kernels.py", "warn_noart").edit_distance)
    assert b.mode == "fallback" and b.artifact_status == "absent"
    assert _warnings(fresh) == []


def test_no_warning_when_the_artifact_is_unusable(fresh, art_dir):
    """EVERY kernel's manifest corrupted (kernels.py carries several `@wasm` kernels; leaving one valid
    would make the warning CORRECT for that one -- the first draft of this arm proved exactly that)."""
    manifests = list((art_dir / "__pythscribe__").glob(f"*/{MANIFEST_NAME}"))
    assert manifests
    for m in manifests:
        m.write_text("{not json", encoding="utf-8")
    mod = import_module_from(art_dir / "kernels.py", "warn_unusable")
    b = binding_of(mod.edit_distance)
    assert b.mode == "fallback" and b.artifact_status == "unusable", (b.mode, b.artifact_status)
    assert all(binding_of(getattr(mod, n)).artifact_status == "unusable" for n in dir(mod) if hasattr(getattr(mod, n), "__pythscribe__"))
    assert _warnings(fresh) == []


def test_no_warning_when_artifacts_are_disabled(fresh, art_dir, monkeypatch):
    monkeypatch.setenv(DISABLE_ENV, "1")
    b = binding_of(import_module_from(art_dir / "kernels.py", "warn_disabled").edit_distance)
    assert b.mode == "fallback" and b.artifact_status == "disabled", (b.mode, b.artifact_status)
    assert _warnings(fresh) == []


def test_no_warning_when_the_source_is_unreadable(fresh, art_dir):
    """A .pyc-only module (zipapp / frozen / stripped wheel -- the `test_static_check` sourceless case) WITH
    the artifacts beside it: status 'unreadable-source', nothing bound (the sourceless path logs its OWN
    fallback warning) -- never the runtime warning."""
    import py_compile

    src = art_dir / "kernels.py"
    pyc = art_dir / "kernels_sourceless.pyc"
    py_compile.compile(str(src), cfile=str(pyc), doraise=True)
    src.unlink()
    mod = import_module_from(pyc, "warn_sourceless")
    b = binding_of(mod.edit_distance)
    assert b.mode == "fallback" and b.artifact is None and b.artifact_status == "unreadable-source", (b.mode, b.artifact_status)
    assert _warnings(fresh) == []


def test_no_warning_when_the_signature_is_outside_the_ffi_grammar(fresh, art_dir, monkeypatch):
    """artifact bound + wasmtime absent, but the grammar check (which precedes the wasmtime probe) refuses:
    browser for THAT reason, silent -- wasmtime is not the sole blocker."""
    def refuse(binding):
        raise ffi.FfiError("probe: keyword-only parameter")
    monkeypatch.setattr(ffi, "signature_of", refuse)
    b = binding_of(import_module_from(art_dir / "kernels.py", "warn_grammar").edit_distance)
    assert b.mode == "browser" and "FFI grammar" in b.mode_reason and NO_RUNTIME_WHY not in b.mode_reason, b.mode_reason
    assert _warnings(fresh) == []


def test_no_warning_when_the_list_use_is_unsupported(fresh, art_dir, monkeypatch):
    monkeypatch.setattr(rt, "unsupported_list_use", lambda node: "probe: list compared by position")
    b = binding_of(import_module_from(art_dir / "kernels.py", "warn_listuse").edit_distance)
    assert b.mode == "browser" and "list use" in b.mode_reason and NO_RUNTIME_WHY not in b.mode_reason, b.mode_reason
    assert _warnings(fresh) == []


@pytest.mark.parametrize("mode", ["browser", "fallback"])
def test_no_warning_for_an_explicit_mode(fresh, art_dir, monkeypatch, mode):
    monkeypatch.setenv(dec.MODE_ENV, mode)
    b = binding_of(import_module_from(art_dir / "kernels.py", f"warn_explicit_{mode}").edit_distance)
    assert b.mode == mode and b.mode_reason.startswith("requested"), (b.mode, b.mode_reason)
    assert _warnings(fresh) == []


def test_explicit_server_raises_mode_error_and_does_not_warn(fresh, art_dir, monkeypatch):
    monkeypatch.setenv(dec.MODE_ENV, "server")
    with pytest.raises(ModeError, match=r"mode 'server' requested but unavailable.*wasm runtime \(wasmtime\) is not available"):
        import_module_from(art_dir / "kernels.py", "warn_explicit_server")
    assert _warnings(fresh) == []  # LOUD by the exception, not by the once-warning


def test_no_warning_when_wasmtime_is_present(monkeypatch, caplog, noart_dir, art_dir):
    """wasmtime importable: neither the fallback (no artifact) nor a browser degradation for another
    reason ever emits the runtime warning."""
    monkeypatch.setattr(rt, "wasmtime_available", lambda: True)
    monkeypatch.setattr(dec, "_warned_no_runtime", False)
    caplog.set_level(logging.INFO, logger="pythscribe")
    binding_of(import_module_from(noart_dir / "kernels.py", "warn_wt_noart").edit_distance)
    monkeypatch.setattr(ffi, "signature_of", lambda binding: (_ for _ in ()).throw(ffi.FfiError("probe")))
    b = binding_of(import_module_from(art_dir / "kernels.py", "warn_wt_grammar").edit_distance)
    assert b.mode == "browser"
    assert _warnings(caplog) == []


def test_once_flag_check_and_set_are_serialized_by_the_lock(fresh, art_dir):
    """DETERMINISTIC lock control (codex round 3: a barrier race stayed green 30/30 with the lock removed --
    the GIL hides the interleaving). Force it: a `sys.settrace` line hook PAUSES thread 1 between the flag
    CHECK and the SET (on the `_warned_no_runtime = True` line) until thread 2 reports it has finished.
      * with the lock: thread 2 blocks on `_warn_lock` while thread 1 is paused -> thread 1's wait times out,
        sets the flag, warns; thread 2 then sees the flag -> exactly ONE warning;
      * lock removed: thread 2 runs the whole helper while thread 1 is paused (check passed, flag still
        False) -> BOTH warn -> 2 warnings -> RED.
    The hook firing is asserted (never vacuous). Thread 1 waits for thread 2 to have ENTERED the helper before its
    bounded wait, so delaying thread 2 cannot flip the verdict; the only residual timing assumption is that an
    unlocked thread 2 finishes a trivial function within 5 s. The structural twin below
    (`test_once_flag_check_and_set_live_inside_the_lock_block`) is the DISCRIMINATING control of record for a
    removed/deleted lock (AST, zero timing); this one is its behavioural companion."""
    import linecache

    b = binding_of(import_module_from(art_dir / "kernels.py", "warn_lock").edit_distance)
    assert len(_warnings(fresh)) == 1
    fresh.clear()
    dec._warned_no_runtime = False
    code = dec._warn_if_no_runtime.__code__
    t1_paused, t2_entered, t2_done = threading.Event(), threading.Event(), threading.Event()

    def tracer(frame, event, arg):
        if frame.f_code is not code:
            return None
        if event == "line" and "_warned_no_runtime = True" in linecache.getline(code.co_filename, frame.f_lineno):
            t1_paused.set()               # CHECK done (flag was False), SET not yet executed
            assert t2_entered.wait(timeout=60)  # NOT wall-clock-sensitive: thread 1 stays paused until thread 2 has
            t2_done.wait(timeout=5.0)     # actually ENTERED the helper (codex r4/1c); only then a bounded wait for it
        return tracer                     # to FINISH -- which it cannot while thread 1 holds the lock (-> times out)

    def t1():
        sys.settrace(tracer)
        try:
            dec._warn_if_no_runtime(b, NO_RUNTIME_WHY)
        finally:
            sys.settrace(None)

    def t2():
        assert t1_paused.wait(timeout=60)
        t2_entered.set()                  # signalled right BEFORE the call: thread 1 waits for THIS, not for a timer
        dec._warn_if_no_runtime(b, NO_RUNTIME_WHY)
        t2_done.set()

    a, c = threading.Thread(target=t1), threading.Thread(target=t2)
    a.start(); c.start()
    a.join(timeout=120); c.join(timeout=120)
    assert not a.is_alive() and not c.is_alive()
    assert t1_paused.is_set() and t2_entered.is_set() and t2_done.is_set()  # the interleaving was forced
    assert len(_warnings(fresh)) == 1, [r.getMessage() for r in _warnings(fresh)]


def test_once_flag_check_and_set_live_inside_the_lock_block():
    """Structural twin: in `_warn_if_no_runtime` the `if _warned_no_runtime` test AND the `_warned_no_runtime =
    True` assignment are both nested inside `with _warn_lock:` (a module-level `threading.Lock`). Deleting the
    lock, or moving either statement out of the block, is RED."""
    import ast
    import inspect

    assert isinstance(getattr(dec, "_warn_lock", None), type(threading.Lock()))
    tree = ast.parse(inspect.getsource(dec._warn_if_no_runtime))
    fn = tree.body[0]
    withs = [n for n in ast.walk(fn) if isinstance(n, ast.With)
             and any(isinstance(i.context_expr, ast.Name) and i.context_expr.id == "_warn_lock" for i in n.items)]
    assert len(withs) == 1, ast.dump(fn)
    inside = list(ast.walk(withs[0]))
    checks = [n for n in inside if isinstance(n, ast.If) and isinstance(n.test, ast.Name) and n.test.id == "_warned_no_runtime"]
    sets = [n for n in inside if isinstance(n, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "_warned_no_runtime" for t in n.targets)]
    assert len(checks) == 1 and len(sets) == 1
    # and no check/set of the flag exists OUTSIDE the lock block
    outside = [n for n in ast.walk(fn) if n not in inside and (
        (isinstance(n, ast.If) and isinstance(n.test, ast.Name) and n.test.id == "_warned_no_runtime")
        or (isinstance(n, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "_warned_no_runtime" for t in n.targets)))]
    assert outside == []


def test_once_flag_is_thread_safe(fresh, art_dir):
    """N threads race the auto resolution of a bound artifact with wasmtime absent (the real
    `resolve_mode` path, a barrier so they arrive together): exactly ONE warning. A load-style companion to
    the deterministic forced-interleaving control above (this one alone does NOT reliably catch a removed
    lock under the GIL -- codex round 3)."""
    b = binding_of(import_module_from(art_dir / "kernels.py", "warn_race").edit_distance)
    assert len(_warnings(fresh)) == 1
    fresh.clear()
    dec._warned_no_runtime = False
    n = 32
    bar = threading.Barrier(n)
    errors: list[BaseException] = []

    def worker():
        try:
            bar.wait(timeout=30)
            dec.resolve_mode(b, "auto")
        except BaseException as e:  # noqa: BLE001 -- surfaced below
            errors.append(e)

    ts = [threading.Thread(target=worker) for _ in range(n)]
    for t in ts:
        t.start()
    for t in ts:
        t.join(timeout=60)
    assert errors == []
    assert len(_warnings(fresh)) == 1, [r.getMessage() for r in _warnings(fresh)]
    # and the helper itself, raced directly with a NON-runtime why-not, never warns
    fresh.clear()
    dec._warned_no_runtime = False
    bar = threading.Barrier(n)

    def worker2():
        bar.wait(timeout=30)
        dec._warn_if_no_runtime(b, "signature outside the FFI grammar: probe")

    ts = [threading.Thread(target=worker2) for _ in range(n)]
    for t in ts:
        t.start()
    for t in ts:
        t.join(timeout=60)
    assert _warnings(fresh) == []
