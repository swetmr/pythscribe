"""Server-rerun counter (S-1). It lives in an IMPORTED module (cached in `sys.modules`), NOT in
`app.py` module globals -- which Streamlit re-executes top-to-bottom on every rerun, so a global
`runs = 0; runs += 1` would always read 1 and the "no rerun" delta would be vacuously 0. Because
this module is imported once and cached, `bump()` accumulates across reruns within the process, so
a before/after DELTA around an interaction measures how many server reruns that interaction caused.

The E2E measures a DELTA (not an absolute), so the initial connect reruns do not matter."""
from __future__ import annotations

import threading

_lock = threading.Lock()
_runs = 0


def bump() -> int:
    """Increment once per script run and return the running total."""
    global _runs
    with _lock:
        _runs += 1
        return _runs


# Cumulative plain-Python-vs-@wasm timing, accumulated across reruns (same discipline as _runs: an
# imported-module global, so it survives Streamlit's top-to-bottom re-execution).
_py_ms_sum = 0.0
_wasm_ms_sum = 0.0


def record_speedup(py_ms: float, wasm_ms: float) -> float | None:
    """Accumulate one (plain-Python ms, @wasm ms) pair and return the cumulative mean speedup ratio
    (total plain-Python time / total @wasm time). None until at least one @wasm sample > 0."""
    global _py_ms_sum, _wasm_ms_sum
    with _lock:
        if wasm_ms > 0:
            _py_ms_sum += py_ms
            _wasm_ms_sum += wasm_ms
        return (_py_ms_sum / _wasm_ms_sum) if _wasm_ms_sum > 0 else None
