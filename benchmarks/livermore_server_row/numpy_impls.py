"""Faithful NumPy (vectorised) implementations of the Livermore kernels that HAVE a clean
vectorised form. Keyed by the kernel's function name (as it appears in the `.ps` `def`).

Deliberately PARTIAL: the loop-carried kernels (recurrences, tridiagonal solves, ADI sweeps,
PIC scatter, first-min scan with early state) either cannot be expressed as a NumPy expression at
all, or only via `np.cumsum`-style primitives. Where no faithful vectorised form exists, the
kernel is simply ABSENT here and the harness reports `n/a (loop-carried / not vectorisable)`.
That absence is not a gap — it is the honest half of the story: NumPy wins on the kernels it can
express, and cannot express the rest (which is exactly where `@wasm`/Numba live).

Each function takes `(n, loop)` to match the kernel signature; most results are loop-independent
(the driver loop recomputes the same array), so `loop` is accepted and ignored where that holds.

NOTE ON EXACTNESS: NumPy reductions use pairwise summation, so a NumPy result matches the
sequential CPython/@wasm result only to FP tolerance (~1e-9 rel), NOT bit-for-bit. The harness
compares NumPy/Numba to CPython with a tolerance and reports the actual values. The @wasm column
is compared bit-exact (same sequential order as CPython).
"""
from __future__ import annotations

import numpy as np

__all__ = ["NUMPY_IMPLS"]


def _k01_hydro(n: int, loop: int) -> float:
    y = 0.0001 * (np.arange(n + 12, dtype=np.float64) + 1.0)
    z = 0.0002 * (np.arange(n + 12, dtype=np.float64) + 1.0)
    q, r, t = 0.5, 1.5, 0.25
    x = q + y[:n] * (r * z[10:10 + n] + t * z[11:11 + n])
    return float(x.sum())


def _k03_inner_product(n: int, loop: int) -> float:
    x = 0.001 * (np.arange(n, dtype=np.float64) + 1.0)
    z = 0.002 * (np.arange(n, dtype=np.float64) + 1.0)
    return float((z * x).sum())


def _k07_state(n: int, loop: int) -> float:
    u = 0.00025 * (np.arange(n + 6, dtype=np.float64) + 1.0)
    y = 0.0003 * (np.arange(n, dtype=np.float64) + 1.0)
    z = 0.00015 * (np.arange(n, dtype=np.float64) + 1.0)
    r, t = 0.5, 0.25
    x = (u[:n] + r * (z + r * y)
         + t * (u[3:3 + n] + r * (u[2:2 + n] + r * u[1:1 + n])
                + t * (u[6:6 + n] + r * (u[5:5 + n] + r * u[4:4 + n]))))
    return float(x.sum())


def _k11_first_sum(n: int, loop: int) -> float:
    y = 0.0001 * (np.arange(n, dtype=np.float64) + 1.0)
    x = np.cumsum(y)  # prefix sum — the vectorised form of the loop-carried recurrence
    return float(x[n - 1])


def _k12_first_diff(n: int, loop: int) -> float:
    idx = np.arange(n + 1, dtype=np.float64)
    y = 0.0003 * (idx + 1.0) * ((np.arange(n + 1) % 7) + 1).astype(np.float64)
    x = y[1:n + 1] - y[:n]
    return float(x.sum())


def _k24_first_min(n: int, loop: int) -> int:
    x = 0.001 * ((np.arange(n) * 7919) % 1009).astype(np.float64)
    x[n // 2] = -1.0e-2
    return int(np.argmin(x))  # argmin returns the FIRST minimum — matches the kernel


NUMPY_IMPLS = {
    "k01_hydro": _k01_hydro,
    "k03_inner_product": _k03_inner_product,
    "k07_state": _k07_state,
    "k11_first_sum": _k11_first_sum,
    "k12_first_diff": _k12_first_diff,
    "k24_first_min": _k24_first_min,
}
