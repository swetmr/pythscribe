"""M4 (a) — the honest server-path admission win over Numba, as an end-to-end SPOT.

Claim under test: a `@wasm` kernel that `raise`s a built-in exception and catches it with
`except <SpecificClass>:` — a shape Numba's nopython mode REJECTS (it accepts `raise`, bare
`except:`, and `except Exception:`, but not catching a specific class) — is admitted to the WASM
fast path and runs IN-PROCESS under wasmtime (the server path) under a fuel budget, bit-exact with
CPython.

Catching a SPECIFIC exception class is the NARROW Numba-rejected shape `@wasm` genuinely admits to
the SERVER path. Verified while scoping M4 against the release binary: classes + methods, dict
literals, and generators all "stay JS" (not WASM-eligible), so they never reach wasmtime — only the
exception control-flow kernel lands on WASM. This SPOT is therefore honest about the NARROW, real
admission win, not the broad "Numba rejects classes/dicts/generators/exceptions" overclaim the
README used to imply (corrected in `pythscribe/README.md`; see `specs/08-09-26-lib-m4-numba-polish/`).
A6 (FIXED, #496): operator-raised exceptions (`ZeroDivisionError` from `//`/`%`/`/`) used to be
admitted to the WASM path and then TRAP; the refuse-to-admit fix now keeps that shape on the JS
backend (which runs the handler like CPython). A6 pins the fix; the JS-path == CPython differential
(SPOT + PBT + siblings) lives in `test_wasm_zerodiv_admission_496.py`.

Every claim ships a PAIRED NEGATIVE CONTROL that goes RED when the property is violated or the
"it actually ran WASM" check is bypassed (feedback_anti_vacuity_paired_control):

  A1  positive: the try/except/raise kernel lands on WASM (exported), runs under Sandbox(fuel=N),
      == CPython, fuel_used > 0. PBT over n.
  A2  admission control (anti-strawman): numba.njit(kernel) RAISES on call — "Numba rejects" is
      real, not a strawman.
  A3  RED: a dict kernel (Numba also rejects, but so does @wasm's WASM path) does NOT land on WASM
      (no export / no wasm) — proving the A1 "landed on WASM" check DISCRIMINATES a JS-routed
      function from a WASM one. Without A3 the A1 landed-assertion would be vacuous.
  A4  RED: ServerKernel refuses a wrong export name (no silent wrong callee).
  A5  RED via the decorator markers: mode=fallback -> server_calls==0 / python_calls>0;
      mode=server -> server_calls>0. The A1 win is load-bearing only because server_calls>0.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

from conftest import REPO, gate, gate_import
from pythscribe.build import find_pyths
from pythscribe.runtime import (
    Sandbox,
    ServerFfiError,
    ServerKernel,
    wasmtime_available,
)

gate_import("wasmtime")


# The admission-win kernel: try/except/raise (Numba-rejected) with a scalar int->int boundary.
GUARDED_SUM_SRC = """\
def guarded_sum(n: int) -> int:
    s = 0
    i = 0
    while i < n:
        try:
            if i % 7 == 0:
                raise ValueError
            s = s + i
        except ValueError:
            s = s - 1
        i = i + 1
    return s
"""

# A dict kernel: Numba rejects it AND @wasm's WASM path does not admit it (stays JS) — the RED
# discriminator for the "landed on WASM" check.
DICT_MODE_SRC = """\
def dict_mode(n: int, seed: int) -> int:
    counts = {}
    x = seed
    i = 0
    while i < n:
        x = (x * 1103515245 + 12345) % 2147483648
        b = x % 7
        try:
            counts[b] = counts[b] + 1
        except KeyError:
            counts[b] = 1
        i = i + 1
    return len(counts)
"""


def _guarded_sum_ref(n: int) -> int:
    s = 0
    i = 0
    while i < n:
        try:
            if i % 7 == 0:
                raise ValueError
            s = s + i
        except ValueError:
            s = s - 1
        i = i + 1
    return s


def _compile_wasm(src: str, tmp: Path, stem: str) -> Path | None:
    """Compile `src` with `--target wasm`. Returns the .wasm path, or None if the compiler emitted
    no wasm (no WASM-eligible function — the kernel stayed on JS)."""
    p = tmp / f"{stem}.ps"
    p.write_text(src, encoding="utf-8")
    wasm = tmp / f"{stem}.wasm"
    r = subprocess.run(
        [str(find_pyths()), "compile", str(p), "--target", "wasm", "-o", str(wasm)],
        capture_output=True, text=True,
    )
    assert r.returncode == 0, f"pyths compile failed: {r.stderr}\n{r.stdout}"
    return wasm if (wasm.is_file() and wasm.stat().st_size > 0) else None


@pytest.fixture(scope="module")
def guarded_kernel(tmp_path_factory) -> ServerKernel:
    gate(wasmtime_available(), "wasmtime-py is required for the server path")
    tmp = tmp_path_factory.mktemp("m4_admission")
    wasm = _compile_wasm(GUARDED_SUM_SRC, tmp, "guarded_sum")
    # A1 landed control: the try/except/raise kernel MUST have landed on WASM (exported), not JS.
    assert wasm is not None, "try/except/raise kernel produced no .wasm (WASM admission regressed?)"
    k = ServerKernel.from_wasm(wasm, name="guarded_sum", sandbox=Sandbox(fuel=50_000_000))
    assert "guarded_sum" in k.exports, (
        f"guarded_sum not exported — the try/except/raise kernel did NOT land on WASM: {k.exports}"
    )
    return k


# ------------------------------------------------------------------ A1: the positive win + PBT
def test_a1_guarded_sum_server_path_matches_cpython(guarded_kernel):
    r = guarded_kernel.call("guarded_sum", ["int"], [1000], return_type="int")
    assert r.value == _guarded_sum_ref(1000)
    assert r.fuel_used is not None and r.fuel_used > 0, "the WASM kernel must actually have executed under the fuel meter"


@pytest.mark.parametrize("n", [0, 1, 7, 8, 49, 123, 1000, 4001])
def test_a1_pbt_server_equals_cpython(guarded_kernel, n):
    r = guarded_kernel.call("guarded_sum", ["int"], [n], return_type="int")
    assert r.value == _guarded_sum_ref(n), f"server != cpython at n={n}"


# ------------------------------------------------------------------ A2: Numba genuinely rejects it
def test_a2_numba_rejects_the_kernel():
    """Anti-strawman: the admission win is real only if Numba actually rejects this shape."""
    numba = gate_import("numba")
    from numba import njit

    j = njit(_guarded_sum_ref)  # njit types lazily on first call
    with pytest.raises(numba.core.errors.NumbaError):
        j(100)


# ------------------------------------------------------------------ A3: RED — the landed check discriminates
def test_a3_dict_kernel_does_not_land_on_wasm(tmp_path):
    """RED control: a dict kernel (also Numba-rejected) does NOT reach the WASM/server path — it
    stays JS. If this ever exported `dict_mode`, the A1 'landed on WASM' assertion would be
    satisfiable by a JS-routed function and thus vacuous. Here it must NOT export."""
    wasm = _compile_wasm(DICT_MODE_SRC, tmp_path, "dict_mode")
    if wasm is None:
        return  # no wasm emitted at all -> the discriminator holds (dict stayed JS)
    k = ServerKernel.from_wasm(wasm, name="dict_mode")
    assert "dict_mode" not in k.exports, (
        "dict_mode unexpectedly landed on WASM — the 'landed on WASM' discriminator is no longer "
        "load-bearing; revisit the M4 admission story"
    )


# ------------------------------------------------------------------ A4: RED — no silent wrong callee
def test_a4_wrong_export_name_is_refused(guarded_kernel):
    with pytest.raises(ServerFfiError):
        guarded_kernel.call("not_the_kernel", ["int"], [10], return_type="int")


# ------------------------------------------------------------------ A5: RED — the path marker is load-bearing
@pytest.fixture(scope="module")
def built_module(tmp_path_factory):
    """A real `@wasm` module built with `pythscribe build`, imported for its path markers."""
    gate(wasmtime_available(), "wasmtime-py is required for the server path")
    d = tmp_path_factory.mktemp("m4_decorator")
    src = d / "kmod.py"
    src.write_text("from pythscribe import wasm\n\n@wasm\n" + GUARDED_SUM_SRC, encoding="utf-8")
    # build the artifact beside the source (the same machinery the demos use)
    r = subprocess.run(
        [sys.executable, "-m", "pythscribe.build", str(src)],
        cwd=str(REPO), capture_output=True, text=True,
    )
    assert r.returncode == 0, f"pythscribe.build failed: {r.stderr}\n{r.stdout}"
    sys.path.insert(0, str(d))
    import importlib
    mod = importlib.import_module("kmod")
    return mod


def test_a5_server_marker_is_load_bearing(built_module):
    """The 'ran WASM' claim is only meaningful because the server marker distinguishes it from the
    Python fallback. mode=server -> server_calls increments; the Python body does not run."""
    from pythscribe import binding_of

    b = binding_of(built_module.guarded_sum)
    assert b.mode == "server", f"guarded_sum must bind the server path (got {b.mode}: {b.mode_reason})"
    py_before, srv_before = b.counts()
    out = built_module.guarded_sum(1000)
    py_after, srv_after = b.counts()
    assert out == _guarded_sum_ref(1000)
    assert srv_after == srv_before + 1, "the WASM/server path must have run"
    assert py_after == py_before, "the Python body must NOT have run on the server path"


# ------------------------------------------------------------------ A6: FIXED (#496) — operator-raised exc refused from WASM
# The admission win covers an EXPLICIT `raise` caught by `except <Class>` (A1). An exception raised by
# an OPERATOR used to be the gap: a `try/except ZeroDivisionError` around `//` could be admitted to
# WASM and then TRAP where CPython runs the handler (C3 error-occurrence divergence). #496 CLOSED that
# gap with the refuse-to-admit fix (`wasm_analysis.rs`): the shape now STAYS ON THE JS BACKEND, which
# raises a catchable ZeroDivisionError and runs the handler exactly as CPython does. This test PINS the
# FIX: zdiv no longer lands on WASM. The deeper JS-path == CPython differential (SPOT + PBT + siblings)
# lives in `test_wasm_zerodiv_admission_496.py`.
ZDIV_SRC = """\
def zdiv(n: int) -> int:
    total = 0
    i = 0
    while i < n:
        try:
            total = total + 100 // (i % 3)
        except ZeroDivisionError:
            total = total - 1
        i = i + 1
    return total
"""


def _zdiv_ref(n: int) -> int:
    total = 0
    i = 0
    while i < n:
        try:
            total = total + 100 // (i % 3)
        except ZeroDivisionError:
            total = total - 1
        i = i + 1
    return total


def test_a6_operator_raised_exception_refused_from_wasm_routes_to_js(tmp_path):
    """#496 (was the M4 A6 KNOWN GAP): a `try/except ZeroDivisionError` around `//` is now REFUSED
    from the WASM fast path (a zero divisor would trap instead of running the handler), so it stays on
    the JS backend and matches CPython. PAIRED CONTROL: if the refuse-to-admit guard is removed, zdiv
    re-lands on WASM and this assertion goes RED (and the WASM path would trap → wrong result)."""
    wasm = _compile_wasm(ZDIV_SRC, tmp_path, "zdiv")
    # The fix: zdiv is NOT WASM-admitted. Either no .wasm was emitted, or it does not export zdiv.
    landed = wasm is not None and "zdiv" in ServerKernel.from_wasm(wasm, name="zdiv").exports
    assert not landed, (
        "zdiv unexpectedly landed on WASM — the #496 refuse-to-admit guard regressed; the "
        "`except ZeroDivisionError` handler would be dropped by a trap on the WASM fast path"
    )
    assert _zdiv_ref(10) == 446  # CPython catches ZeroDivisionError -> 446; the JS path matches (see the 496 file)


def test_a5_red_python_marker_is_distinct(built_module):
    """Paired RED: the SAME binding's Python body (`run_python`) increments python_calls and leaves
    server_calls untouched. So a run that did NOT execute WASM is provably distinguishable from one
    that did; the A5 win-assertion (server_calls>0) is load-bearing, not decorative. If the two
    markers were the same counter, a Python-fallback run would masquerade as a WASM win — this
    control goes RED in that world."""
    from pythscribe import binding_of

    b = binding_of(built_module.guarded_sum)
    py_before, srv_before = b.counts()
    out = b.run_python(1000)  # explicitly the Python body, not the server path
    py_after, srv_after = b.counts()
    assert out == _guarded_sum_ref(1000)
    assert srv_after == srv_before, "run_python must NOT touch the server marker"
    assert py_after == py_before + 1, "run_python must increment the python marker"
