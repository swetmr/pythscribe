"""Compile-on-first-call: `@wasm` on a plain function, no pre-built artifact, no `import kernels`,
no `python -m pythscribe.build` -- the FIRST call compiles the (statically-checked) kernel into a
per-user, source-hash-keyed cache and binds the server path. Import stays cheap; the one-time cost
is paid on the first call. This is the SAME artifact + server path an explicit build produces (one
ABI); it only changes WHEN it is produced.

SPOTs pinned through the SHIPPED wrapper (not a model of it). Every positive property is paired
with a NEGATIVE control that provably degrades to the plain-Python fallback (never crashes, never
returns a wrong value):

  * eligible kernel, no artifact  -> first call compiles, mode == 'server', result == CPython
  * NON-eligible kernel           -> first call falls back, mode == 'fallback', result == CPython
  * explicit mode='fallback'      -> never JITs
  * compiler absent               -> falls back (no crash)
"""
from __future__ import annotations

import pytest

from conftest import gate, gate_import
from pythscribe import binding_of
from pythscribe.build import BuildError, find_pyths
from pythscribe.runtime import wasmtime_available

# An eligible kernel (int list -> int; internal scratch; no tuple-unpack, no ternary): edit distance.
ELIGIBLE = '''
from pythscribe import wasm

@wasm
def edit_distance(a: list[int], b: list[int]) -> int:
    n = len(a)
    m = len(b)
    prev = [0] * (m + 1)
    cur = [0] * (m + 1)
    for j in range(m + 1):
        prev[j] = j
    for i in range(1, n + 1):
        cur[0] = i
        for j in range(1, m + 1):
            cost = 1
            if a[i - 1] == b[j - 1]:
                cost = 0
            best = prev[j] + 1
            if cur[j - 1] + 1 < best:
                best = cur[j - 1] + 1
            if prev[j - 1] + cost < best:
                best = prev[j - 1] + cost
            cur[j] = best
        for j in range(m + 1):
            prev[j] = cur[j]
    return prev[m]
'''

# A NON-eligible kernel: a dict literal is not on the WASM fast path -> must fall back gracefully.
NON_ELIGIBLE = '''
from pythscribe import wasm

@wasm
def with_dict(x: int) -> int:
    d = {}
    d[x] = x * 2
    return d[x]
'''

FALLBACK_MODE = '''
from pythscribe import wasm

@wasm(mode="fallback")
def edit_distance(a: list[int], b: list[int]) -> int:
    n = len(a)
    m = len(b)
    prev = [0] * (m + 1)
    cur = [0] * (m + 1)
    for j in range(m + 1):
        prev[j] = j
    for i in range(1, n + 1):
        cur[0] = i
        for j in range(1, m + 1):
            cost = 1
            if a[i - 1] == b[j - 1]:
                cost = 0
            best = prev[j] + 1
            if cur[j - 1] + 1 < best:
                best = cur[j - 1] + 1
            if prev[j - 1] + cost < best:
                best = prev[j - 1] + cost
            cur[j] = best
        for j in range(m + 1):
            prev[j] = cur[j]
    return prev[m]
'''


def _ed_py(a, b):
    n, m = len(a), len(b)
    prev = list(range(m + 1))
    cur = [0] * (m + 1)
    for i in range(1, n + 1):
        cur[0] = i
        for j in range(1, m + 1):
            cost = 0 if a[i - 1] == b[j - 1] else 1
            cur[j] = min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + cost)
        prev, cur = cur, prev
    return prev[m]


SPOTS = [
    ([1, 2, 3], [1, 0, 3]),
    ([], []),
    ([1, 2, 3, 4, 5], [1, 0, 3, 4, 9]),
    ([7, 7, 7], []),
    (list(range(20)), list(range(19, -1, -1))),
]


@pytest.fixture(autouse=True)
def _isolated_cache(tmp_path, monkeypatch):
    """Every test compiles into its OWN cache dir, so a test can never read another's artifact.
    This file exercises compile-on-first-call, so it RE-ENABLES the JIT the suite disables by
    default (conftest `_no_jit_by_default`)."""
    monkeypatch.setenv("PYTHSCRIBE_CACHE", str(tmp_path / "jit"))
    monkeypatch.delenv("PYTHSCRIBE_NO_JIT", raising=False)


def _require_toolchain():
    try:
        find_pyths()
    except BuildError as e:
        gate(False, f"pyths compiler required for compile-on-first-call ({e})")
    gate(wasmtime_available(), "wasmtime-py required for the server path")


def test_compile_on_first_call_binds_server(import_source):
    """The load-bearing property: no artifact, no build step -> first call compiles + runs as WASM."""
    _require_toolchain()
    mod = import_source(ELIGIBLE, "cofc_ok")
    b = binding_of(mod.edit_distance)
    assert b.mode == "fallback", "nothing must be compiled before the first call (import stays cheap)"
    assert b.server_calls == 0

    out = mod.edit_distance([1, 2, 3, 4, 5], [1, 0, 3, 4, 9])

    assert out == _ed_py([1, 2, 3, 4, 5], [1, 0, 3, 4, 9])
    assert b.mode == "server", f"first call must compile+bind the server path (jit_reason={b._jit_reason!r})"
    assert b.mode_reason == "auto: compiled on first call"
    assert b.server_calls == 1


def test_jit_result_matches_cpython_spots(import_source):
    """SPOT battery through the shipped wrapper: the compiled kernel == the plain-Python reference."""
    _require_toolchain()
    mod = import_source(ELIGIBLE, "cofc_spots")
    for a, b in SPOTS:
        assert mod.edit_distance(a, b) == _ed_py(a, b), (a, b)
    assert binding_of(mod.edit_distance).mode == "server"


def test_non_eligible_falls_back_gracefully(import_source):
    """PAIRED NEGATIVE CONTROL: a kernel outside the WASM fast path must run as plain Python --
    correct value, no exception, mode stays 'fallback', and the reason is recorded."""
    _require_toolchain()
    mod = import_source(NON_ELIGIBLE, "cofc_bad")
    b = binding_of(mod.with_dict)
    out = mod.with_dict(21)  # must NOT raise
    assert out == 42
    assert b.mode == "fallback"
    assert b._jit_attempted is True
    assert "not WASM-eligible" in b._jit_reason
    assert b.server_calls == 0


def test_explicit_fallback_mode_never_jits(import_source):
    """CONTROL: an explicit mode='fallback' is honoured -- JIT is the AUTO path only, never an
    override of a requested mode."""
    _require_toolchain()
    mod = import_source(FALLBACK_MODE, "cofc_reqfb")
    b = binding_of(mod.edit_distance)
    out = mod.edit_distance([1, 2, 3], [1, 0, 3])
    assert out == _ed_py([1, 2, 3], [1, 0, 3])
    assert b.mode == "fallback"
    # the wrapper may or may not enter ensure_compiled; either way it must not bind a server
    assert b.server is None
    assert b.server_calls == 0


def test_compiler_absent_falls_back(import_source, monkeypatch):
    """PAIRED NEGATIVE CONTROL: with no compiler reachable (a pip-only environment), an eligible
    kernel still runs -- as plain Python -- and never crashes."""
    gate(wasmtime_available(), "wasmtime-py required to isolate the compiler-absent path")
    monkeypatch.setenv("PYTHSCRIBE_PYTHS", "/no/such/pyths-binary")
    mod = import_source(ELIGIBLE, "cofc_nopyths")
    b = binding_of(mod.edit_distance)
    out = mod.edit_distance([1, 2, 3], [1, 0, 3])
    assert out == _ed_py([1, 2, 3], [1, 0, 3])
    assert b.mode == "fallback"
    assert b._jit_attempted is True
    assert "compiler not found" in b._jit_reason or "not WASM-eligible" in b._jit_reason


def test_second_binding_same_source_is_cache_hit(import_source):
    """Two identical kernels compile to server; the second is served from the source-hash cache
    (the same artifact, produced once)."""
    _require_toolchain()
    m1 = import_source(ELIGIBLE, "cofc_c1")
    assert m1.edit_distance([1, 2, 3], [1, 0, 3]) == _ed_py([1, 2, 3], [1, 0, 3])
    assert binding_of(m1.edit_distance).mode == "server"
    m2 = import_source(ELIGIBLE, "cofc_c2")
    assert m2.edit_distance([1, 2, 3], [1, 0, 3]) == _ed_py([1, 2, 3], [1, 0, 3])
    assert binding_of(m2.edit_distance).mode == "server"


def test_no_jit_env_disables(import_source, monkeypatch):
    """CONTROL for the opt-out knob: PYTHSCRIBE_NO_JIT keeps an eligible kernel on plain Python.
    The whole suite relies on this (conftest sets it), so it must actually work."""
    _require_toolchain()
    monkeypatch.setenv("PYTHSCRIBE_NO_JIT", "1")
    mod = import_source(ELIGIBLE, "cofc_nojit")
    b = binding_of(mod.edit_distance)
    assert mod.edit_distance([1, 2, 3], [1, 0, 3]) == _ed_py([1, 2, 3], [1, 0, 3])
    assert b.mode == "fallback"
    assert "disabled by PYTHSCRIBE_NO_JIT" in b._jit_reason
    assert b.server_calls == 0


def test_disable_artifacts_not_bypassed_by_jit(import_source, monkeypatch):
    """PAIRED NEGATIVE CONTROL (blocker B3): PYTHSCRIBE_DISABLE_ARTIFACTS is an explicit 'stay on
    Python' -- the JIT must NOT flip it to WASM. The guard keys on artifact_status ('disabled'),
    not on `artifact is None` (which is ALSO true for 'disabled'/'unusable')."""
    _require_toolchain()
    monkeypatch.setenv("PYTHSCRIBE_DISABLE_ARTIFACTS", "1")  # set BEFORE import (read at decoration)
    mod = import_source(ELIGIBLE, "cofc_disabled")
    b = binding_of(mod.edit_distance)
    assert b.artifact_status == "disabled"
    assert mod.edit_distance([1, 2, 3], [1, 0, 3]) == _ed_py([1, 2, 3], [1, 0, 3])
    assert b.mode == "fallback", "the JIT overrode an explicit artifact-disable (B3 regressed)"
    assert b.server_calls == 0


def test_no_resolvable_home_does_not_crash(import_source, monkeypatch, tmp_path):
    """PAIRED NEGATIVE CONTROL (blocker B1): when the home dir can't be resolved (docker --user,
    DynamicUser, locked CI) and PYTHSCRIBE_CACHE is unset, the FIRST CALL must NOT raise -- it
    falls back to the temp dir AND still compiles (mode server), it never crashes the call."""
    import pathlib
    import tempfile

    _require_toolchain()
    monkeypatch.delenv("PYTHSCRIBE_CACHE", raising=False)  # force the home-dir path in _jit_cache_root
    # isolate the temp fallback into tmp_path so this test never writes to the real temp dir
    monkeypatch.setattr(tempfile, "gettempdir", lambda: str(tmp_path))

    def _no_home(*a, **k):
        raise RuntimeError("Could not determine home directory.")

    monkeypatch.setattr(pathlib.Path, "home", _no_home)
    mod = import_source(ELIGIBLE, "cofc_nohome")
    out = mod.edit_distance([1, 2, 3], [1, 0, 3])  # must NOT raise RuntimeError
    assert out == _ed_py([1, 2, 3], [1, 0, 3])
    assert binding_of(mod.edit_distance).mode == "server"  # tempdir fallback still compiles


def test_invalid_cache_entry_never_rebuilt_in_place(import_source, monkeypatch, tmp_path):
    """PAIRED NEGATIVE CONTROL (r3 blocker B2, stale/corrupt arm): a present-but-INVALID keyed entry
    under the current version must NEVER be rebuilt IN PLACE in the shared dir (the concurrent in-place
    rmtree+build is the whole hazard). We plant a SENTINEL beside a corrupt manifest at the exact
    version-keyed artifact dir; after a first call the sentinel must SURVIVE (the shared dir was not
    rebuilt) AND the call must still bind server (via a private build). A regression that routes the
    keyed dir through `build_kernel` rmtree's the dir -> sentinel gone -> RED."""
    from pythscribe.artifacts import MANIFEST_NAME
    from pythscribe.build.optimizer import resolve_wasm_opt
    from pythscribe.decorators import jit_cache_key_dir

    _require_toolchain()
    cache = tmp_path / "jit"
    monkeypatch.setenv("PYTHSCRIBE_CACHE", str(cache))
    mod = import_source(ELIGIBLE, "cofc_corrupt")
    b = binding_of(mod.edit_distance)
    # M3: the key carries the AMBIENT optimizer id (`<ver>/<optimizer>/<sha>`)
    adir = jit_cache_key_dir(b.source_sha256, resolve_wasm_opt().id) / "__pythscribe__" / "edit_distance"
    adir.mkdir(parents=True)
    (adir / MANIFEST_NAME).write_text("{ this is not valid json", encoding="utf-8")
    sentinel = adir / "SENTINEL.txt"
    sentinel.write_text("do not rebuild me in place", encoding="utf-8")

    out = mod.edit_distance([1, 2, 3], [1, 0, 3])  # must NOT raise; must recover via a private build
    assert out == _ed_py([1, 2, 3], [1, 0, 3])
    assert b.mode == "server", f"invalid cache entry not recovered (jit_reason={b._jit_reason})"
    assert sentinel.is_file(), "the shared keyed dir was rebuilt IN PLACE (sentinel destroyed) -- B2 regressed"


def test_gradio_dispatch_triggers_compile_on_first_call(import_source):
    """SHOULD-FIX control: with the JIT on (default), a Gradio dispatch of an eligible kernel with no
    pre-built artifact compiles on first dispatch -- the browser bundle needs no explicit build."""
    _require_toolchain()
    gr = gate_import("gradio")  # the adapter's static-file URL needs gradio present
    assert gr is not None
    src = (
        "from pythscribe import wasm\n\n"
        "@wasm\n"
        "def rms(xs: list[float]) -> float:\n"
        "    acc = 0.0\n"
        "    n = 0\n"
        "    for x in xs:\n"
        "        acc = acc + x * x\n"
        "        n = n + 1\n"
        "    if n == 0:\n"
        "        return 1.0\n"
        "    return (acc / n) ** 0.5\n"
    )
    mod = import_source(src, "cofc_gradio")
    b = binding_of(mod.rms)
    assert b.artifact_status == "absent"
    from pythscribe.gradio import dispatch

    payload = dispatch(mod.rms, [3.0, 4.0])
    assert payload["bundle"] is not None, "gradio dispatch did not compile-on-first-call (no bundle)"
    assert binding_of(mod.rms).artifact_status == "resolved"


def test_concurrent_first_calls_all_bind_server(tmp_path):
    """PAIRED NEGATIVE CONTROL (blocker B2): N processes hitting an EMPTY shared cache simultaneously
    on their first call must ALL end on the server path (one builds + publishes atomically, the rest
    adopt). The pre-fix behaviour was the opposite: concurrent rmtree+build in one shared dir left
    every worker in a permanent, mis-diagnosed fallback."""
    import subprocess
    import sys
    import textwrap

    _require_toolchain()
    shared = tmp_path / "shared_cache"  # fresh + empty: the workers race to populate it
    kdir = tmp_path / "kmod"
    kdir.mkdir()
    (kdir / "km.py").write_text(ELIGIBLE, encoding="utf-8")
    runner = tmp_path / "run.py"
    runner.write_text(textwrap.dedent(
        """
        import os, sys
        os.environ[%r] = %r
        os.environ.pop(%r, None)
        sys.path.insert(0, %r)
        import km
        from pythscribe import binding_of
        km.edit_distance([1, 2, 3, 4, 5], [1, 0, 3, 4, 9])
        print(binding_of(km.edit_distance).mode)
        """ % ("PYTHSCRIBE_CACHE", str(shared), "PYTHSCRIBE_NO_JIT", str(kdir))
    ), encoding="utf-8")

    n = 4
    procs = [subprocess.Popen([sys.executable, str(runner)], stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, text=True) for _ in range(n)]
    results = [p.communicate() for p in procs]
    modes = []
    for out, err in results:
        line = out.strip().splitlines()[-1] if out.strip() else f"NO-OUTPUT (stderr tail: {err[-300:]})"
        modes.append(line)
    assert all(m == "server" for m in modes), modes


def test_concurrent_invalid_entry_never_rebuilt_in_place(import_source, tmp_path):
    """PAIRED NEGATIVE CONTROL (r3/r4 blocker B2, concurrent stale/corrupt arm): N processes racing on
    a PRE-PLANTED invalid keyed entry must ALL end server (each via a private build) AND the shared
    corrupt dir must NEVER be rebuilt in place (the planted sentinel survives). The pre-fix behaviour
    was concurrent in-place rmtree+rebuild in the shared dir -> workers stranded in fallback + a
    destroyed sentinel."""
    import importlib
    import os
    import subprocess
    import sys
    import textwrap

    from pythscribe.artifacts import MANIFEST_NAME
    from pythscribe.build.optimizer import resolve_wasm_opt
    from pythscribe.decorators import jit_cache_key_dir

    _require_toolchain()
    shared = tmp_path / "shared_cache"
    kdir = tmp_path / "kmod"
    kdir.mkdir()
    (kdir / "km.py").write_text(ELIGIBLE, encoding="utf-8")

    # read the source-sha WITHOUT compiling (import binds; the JIT only fires on a CALL)
    sys.path.insert(0, str(kdir))
    try:
        km = importlib.import_module("km")
        sha = binding_of(km.edit_distance).source_sha256
    finally:
        sys.path.remove(str(kdir))
        sys.modules.pop("km", None)

    # plant a corrupt entry + sentinel at the exact (version, optimizer-id)-keyed artifact dir the
    # workers consult (M3: the workers inherit this env, so they resolve the same ambient id)
    monkeypatch_env = os.environ.get("PYTHSCRIBE_CACHE")
    os.environ["PYTHSCRIBE_CACHE"] = str(shared)
    try:
        adir = jit_cache_key_dir(sha, resolve_wasm_opt().id) / "__pythscribe__" / "edit_distance"
    finally:
        if monkeypatch_env is None:
            os.environ.pop("PYTHSCRIBE_CACHE", None)
        else:
            os.environ["PYTHSCRIBE_CACHE"] = monkeypatch_env
    adir.mkdir(parents=True)
    (adir / MANIFEST_NAME).write_text("{ this is not valid json", encoding="utf-8")
    sentinel = adir / "SENTINEL.txt"
    sentinel.write_text("do not rebuild me in place", encoding="utf-8")

    runner = tmp_path / "run.py"
    runner.write_text(textwrap.dedent(
        """
        import os, sys
        os.environ[%r] = %r
        os.environ.pop(%r, None)
        sys.path.insert(0, %r)
        import km
        from pythscribe import binding_of
        km.edit_distance([1, 2, 3, 4, 5], [1, 0, 3, 4, 9])
        print(binding_of(km.edit_distance).mode)
        """ % ("PYTHSCRIBE_CACHE", str(shared), "PYTHSCRIBE_NO_JIT", str(kdir))
    ), encoding="utf-8")

    n = 4
    procs = [subprocess.Popen([sys.executable, str(runner)], stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, text=True) for _ in range(n)]
    results = [p.communicate() for p in procs]
    modes = []
    for out, err in results:
        line = out.strip().splitlines()[-1] if out.strip() else f"NO-OUTPUT (stderr tail: {err[-300:]})"
        modes.append(line)
    assert all(m == "server" for m in modes), modes
    assert sentinel.is_file(), "the shared corrupt dir was rebuilt IN PLACE under concurrency (sentinel destroyed)"
