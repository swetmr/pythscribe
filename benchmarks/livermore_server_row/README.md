# Livermore SERVER-row timing harness

The **server row** of the AOT-vs-JIT 3×2 grid. Runs each of the 24
Livermore Fortran Kernels (`tests/differential/livermore/*.ps`) four ways **in one CPython
process** and prints a per-kernel timing table:

| column | what it is |
|---|---|
| **CPython** | the pure-Python function — the semantic ground truth |
| **NumPy** | a faithful vectorised form **where one exists** (`numpy_impls.py`); else `n/a: loop-carried` |
| **Numba** | `@njit` of the pure-Python function; else `rejected: <error>` — the admission story |
| **@wasm** | the SAME source compiled `--target wasm`, run in-process under wasmtime (`ServerKernel`) |

## Run

```bash
python benchmarks/livermore_server_row/run.py            # all 24 kernels
python benchmarks/livermore_server_row/run.py --quick    # k03, k06 (smoke)
python benchmarks/livermore_server_row/run.py --kernels k03,k11 --reps 7
```

`--reps N` reports the best (min) wall time over N runs (min rejects scheduler noise). Numba is
warmed up before timing so its JIT-compile cost is excluded.

## What this is NOT

- **Not the browser row.** The Pyodide+NumPy / Numba-wasm / @wasm browser row, and the full
  *published* grid, are a separate launch / Paper-C-v2 deliverable. This harness is the server row
  only.
- **Not a claim that @wasm is fast at numerics.** It is the opposite: it measures, with numbers,
  the columns where @wasm **loses**.

## The honest reading (do not soften)

On this machine (indicative, your numbers will differ):

- **Numba or NumPy is the fastest column on every kernel** (Numba 23, NumPy 1 — once helpers are
  jitted), often 10–100× faster than @wasm; @wasm wins **0** speed columns. On the vectorisable
  kernels **NumPy is far faster than
  @wasm too** (it is C with SIMD; @wasm is a scalar interpreter loop). `@wasm` does **not** beat
  NumPy/Numba on vectorised numerics — ever.
- On the smallest kernels `@wasm` can even be **slower than interpreted CPython** (per-call
  marshalling dominates a trivial loop).
- `@wasm` is faster than CPython on the larger loop nests (a real, if unremarkable, compiled-loop
  speedup), but that is **not** the pitch.

**Where `@wasm` actually wins** is not on this table's speed axis:

1. **Admission.** `@wasm` runs kernels Numba *rejects*. The harness counts these as **ADMISSION
   wins** — but on the Livermore set the honest count is **0**: every kernel is nopython-eligible
   once its helpers are `@njit`ed (the harness jits *all* top-level defs, so `k22_planckian`'s
   `pexp` helper is jitted just as a real Numba user would — it is **not** a rejection). The genuine
   admission case is narrower and lives in the M4 SPOT
   (`tests/pythscribe/test_m4_numba_admission.py`): a kernel that `raise`s a built-in exception and
   catches it with `except <SpecificClass>:` — which Numba's nopython frontend rejects. (Operator-
   raised exceptions like `ZeroDivisionError` from `//` are handled soundly as of #496: those shapes
   are **refused admission** and stay on the JS path — CPython-faithful, no WASM trap — rather than
   being admitted and trapping. See the pip README's Numba section.)
2. **Determinism** (fixed WASM float/int semantics), the **capability sandbox** + fuel metering,
   **GIL-free** fan-out, and a **single deployable `.wasm`** artifact.

## Paired anti-vacuity control

Every @wasm row asserts the kernel actually **landed on WASM** (the compiled module exports it).
A kernel that silently stayed on the JS path (no WASM export) is reported as `NOT-ON-WASM` — a red
cell — never as a @wasm time. `compile_and_load(..., require_wasm=True)` raises `NotOnWasm` in that
case; the gate `tests/pythscribe/test_m4_livermore_harness.py::test_h2_landed_check_refuses_js_only_kernel`
drives that control with a JS-only (dict) kernel.

NumPy/Numba results are compared to CPython within FP tolerance (they use pairwise summation, so
they match sequential CPython only to ~1e-9 rel). The **@wasm** column is compared **bit-exact**
(same sequential order as CPython) — a mismatch there fails the run.
