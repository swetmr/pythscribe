"""M4 (b) — gate for the Livermore SERVER-row timing harness.

The harness (`benchmarks/livermore_server_row/run.py`) is launch/positioning content, not shipped
runtime — so this test does NOT re-benchmark at scale. It gates two properties:

  H1  the harness runs end-to-end in `--quick` mode: every @wasm column LANDED on WASM and matched
      CPython bit-exact (exit 0). A miscompile or a NOT-ON-WASM row would make the harness exit 1.
  H2  PAIRED NEGATIVE CONTROL (feedback_anti_vacuity_paired_control): the harness's landed-check
      (`compile_and_load(require_wasm=True)`) RAISES `NotOnWasm` for a kernel that stays on the JS
      path (a dict kernel). Without this the @wasm timings could silently be JS timings. This is
      the harness analogue of the SPOT's A3 discriminator.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

from conftest import REPO, gate, gate_import

gate_import("numpy")
gate_import("numba")
gate_import("wasmtime")

from pythscribe.runtime import wasmtime_available  # noqa: E402

HARNESS = REPO / "benchmarks" / "livermore_server_row" / "run.py"

sys.path.insert(0, str(HARNESS.parent))


def test_h1_harness_quick_runs_green():
    """H1: --quick runs 2 kernels; @wasm landed + == CPython on every row -> exit 0."""
    gate(wasmtime_available(), "wasmtime-py required for the server path")
    r = subprocess.run(
        [sys.executable, str(HARNESS), "--quick", "--reps", "2"],
        cwd=str(REPO), capture_output=True, text=True,
    )
    assert r.returncode == 0, f"harness --quick failed (a miscompile or NOT-ON-WASM row?):\n{r.stdout}\n{r.stderr}"
    assert "Livermore SERVER-row timings" in r.stdout
    assert "k03_inner_product" in r.stdout


def test_h2_landed_check_refuses_js_only_kernel(tmp_path):
    """H2 RED: the landed-check discriminates a JS-routed kernel from a WASM one. A dict kernel
    (stays JS) MUST raise NotOnWasm -> the harness can never report a JS time as a @wasm time."""
    gate(wasmtime_available(), "wasmtime-py required for the server path")
    from run import Kernel, NotOnWasm, compile_and_load  # the harness module

    dict_src = (
        "def dictk(n: int) -> int:\n"
        "    d = {}\n"
        "    i = 0\n"
        "    while i < n:\n"
        "        d[i % 5] = i\n"
        "        i = i + 1\n"
        "    return len(d)\n"
        "print(repr(dictk(10)))\n"
    )
    k = Kernel(id="dictk", fn_name="dictk", src=dict_src, param_types=["int"], return_type="int", args=[10])
    with pytest.raises(NotOnWasm):
        compile_and_load(k, tmp_path, require_wasm=True)


def test_h2b_landed_check_accepts_a_real_wasm_kernel(tmp_path):
    """H2 positive twin: a genuinely WASM-eligible kernel LOADS (the control is not always-red)."""
    gate(wasmtime_available(), "wasmtime-py required for the server path")
    from run import Kernel, compile_and_load

    src = (
        "def addup(n: int) -> int:\n"
        "    s = 0\n"
        "    i = 0\n"
        "    while i < n:\n"
        "        s = s + i\n"
        "        i = i + 1\n"
        "    return s\n"
        "print(repr(addup(10)))\n"
    )
    k = Kernel(id="addup", fn_name="addup", src=src, param_types=["int"], return_type="int", args=[10])
    kern = compile_and_load(k, tmp_path, require_wasm=True)
    assert "addup" in kern.exports
    assert kern.call("addup", ["int"], [10], return_type="int").value == 45


# ---- H3 PAIRED CONTROLS for the admission-tally classifier (feedback_anti_vacuity_paired_control) ----
# The B1 guard's whole point is "the ADMISSION tally cannot be inflated by a non-Numba failure".
# These are its paired negative controls: without them, a later edit widening `except _NUMBA_ERROR`
# back to `except Exception` (or dropping the NUMBA_OK branch) would reopen the r1 hole with every
# test still green. Each drives run_row directly and asserts the classification + the main() hard-fail.

_ADDUP = (
    "def addup(n: int) -> int:\n    s = 0\n    i = 0\n    while i < n:\n        s = s + i\n        i = i + 1\n    return s\nprint(repr(addup(10)))\n"
)
_GUARDED = (  # the SPOT shape: raise + except <SpecificClass> — Numba rejects, @wasm admits
    "def guarded(n: int) -> int:\n    s = 0\n    i = 0\n    while i < n:\n        try:\n            if i % 7 == 0:\n                raise ValueError\n            s = s + i\n        except ValueError:\n            s = s - 1\n        i = i + 1\n    return s\nprint(repr(guarded(200)))\n"
)


def _kernel(run, id, src, args):
    return run.Kernel(id=id, fn_name=id, src=src, param_types=["int"], return_type="int", args=args)


def test_h3_numba_absent_is_na_not_rejected(tmp_path, monkeypatch):
    """RED: with numba unavailable, the Numba column is `n/a`, NEVER `rejected` — so a numba-less CI
    box cannot report inflated ADMISSION wins (Fable r1 scenario B). If the classifier regressed to
    `except Exception`, this note would read `rejected: ...` and the test fails."""
    gate(wasmtime_available(), "wasmtime-py required")
    import run
    monkeypatch.setattr(run, "NUMBA_OK", False)
    row = run.run_row(_kernel(run, "addup", _ADDUP, [10]), tmp_path, reps=1)
    assert row.numba_note == "n/a: numba not installed"
    assert not row.numba_note.startswith("rejected")
    assert row.wasm_ms is not None  # @wasm still ran (the admission-win guard also needs a wasm time)


def test_h3_red_non_numba_error_fails_the_run(tmp_path, monkeypatch):
    """RED: a NON-Numba exception in the Numba column is an ERROR that FAILS the run (a note in
    row.notes → main() exits 1) — it is NEVER silently classified as `rejected`."""
    gate(wasmtime_available(), "wasmtime-py required")
    import run

    def _boom(kernel):
        raise RuntimeError("harness bug, not a Numba rejection")

    monkeypatch.setattr(run, "_numba_target", _boom)
    row = run.run_row(_kernel(run, "addup", _ADDUP, [10]), tmp_path, reps=1)
    assert row.numba_note.startswith("ERROR")
    assert any(n.startswith("ERROR") for n in row.notes), "an ERROR must be a hard-failure note (main exits 1)"
    assert not row.numba_note.startswith("rejected")


def test_h3b_admission_tally_positive_twin(tmp_path):
    """Positive twin: the SPOT shape (raise + except <SpecificClass>) IS classified `rejected` with a
    real @wasm time — so the tally is not always-0 (it is 0 on Livermore only because no Livermore
    kernel is this shape)."""
    gate(wasmtime_available(), "wasmtime-py required")
    import run
    if not run.NUMBA_OK:
        gate(False, "numba required for the positive-twin control")
    row = run.run_row(_kernel(run, "guarded", _GUARDED, [200]), tmp_path, reps=1)
    assert row.numba_note.startswith("rejected"), f"expected a Numba rejection, got {row.numba_note!r}"
    assert row.wasm_ms is not None, "the admission twin needs a real @wasm time"


def test_h4_numpy_mismatch_fails_the_run(tmp_path, monkeypatch):
    """N2 RED: a WRONG NumPy impl is a HARD failure (a note in row.notes → main() exits 1), never a
    silent green cell that just drops NumPy from the fastest-column race."""
    gate(wasmtime_available(), "wasmtime-py required")
    import run
    monkeypatch.setitem(run.NUMPY_IMPLS, "addup", lambda n: 999)  # addup(10)=45, not 999
    row = run.run_row(_kernel(run, "addup", _ADDUP, [10]), tmp_path, reps=1)
    assert "MISMATCH" in row.numpy_note
    assert any("MISMATCH" in n for n in row.notes), "a NumPy MISMATCH must be a hard-failure note"
