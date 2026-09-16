"""Helper library for `full_features_v0_2_5.ipynb` -- the v0.2.5 full-`@wasm`-features demo.

Everything here REUSES the shipped runtime + the M2 typed-array shim + the M4 harness; nothing
re-implements marshalling or timing:

  * SERVER path  -> `pythscribe.runtime.ServerKernel` (wasmtime) + `pythscribe.runtime.array_buffer`
                    (the 16-byte typed-array header + bulk element copy). We never touch bytes here.
  * BROWSER path -> the EXACT `pythscribe/ffi/list_buffer.mjs` shim the Gradio/Streamlit components
                    import, driven under Node through `tests/pythscribe/_array_shim_runner.mjs`. This
                    is the browser CHANNEL (V8 + the component's own shim), NOT a literal tab -- the
                    real-tab isomorphic run is `iso_drive.measure_isomorphic` (a Gradio app in a
                    headless Chromium). Stated plainly wherever it is used.
  * timing       -> `features_lib.time_best` (best-of-N `perf_counter`), the same helper the M1.5
                    notebook uses. The millisecond numbers are MEASURED, never thresholds; only the
                    honest-DIRECTION checks (NumPy beats @wasm on the reduction; @wasm wins 0 Livermore
                    speed columns) are asserted.

Honesty (M4 positioning, do not soften): `@wasm` does NOT beat NumPy/Numba on vectorised numerics --
its value is the sandbox, bit-for-bit isomorphism across engines, a single deployable `.wasm`, and
source-compatibility, plus a NARROW admission edge (a specific-class `except` Numba rejects). The
Livermore counterweight (`run_livermore`) reports @wasm winning 0 speed columns.
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

import numpy as np

from pythscribe.build import find_pyths
from pythscribe.ffi import SHIM  # pythscribe/ffi/list_buffer.mjs -- the shipped browser shim
from pythscribe.runtime import ServerKernel

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
ARRAY_SHIM_RUNNER = REPO / "tests" / "pythscribe" / "_array_shim_runner.mjs"
LIVERMORE = REPO / "benchmarks" / "livermore_server_row" / "run.py"

# features_lib.time_best -- reuse the M1.5 notebook's measured best-of-N timer, do not re-invent.
sys.path.insert(0, str(HERE))
from features_lib import time_best  # noqa: E402,F401  (re-exported for the notebook)

# --------------------------------------------------------------------------- compile

def compile_wasm(src: str, out_dir: Path, stem: str = "k") -> Path:
    """Compile `.ps` source to WASM with the SHIPPED compiler (`pyths compile --target wasm`).
    Returns the `.wasm` path, or None when the compiler emitted no WASM-eligible function (the
    kernel was routed to JS -- e.g. a dict/class kernel, or a G1/G2-guarded array op)."""
    out_dir.mkdir(parents=True, exist_ok=True)
    p = out_dir / f"{stem}.ps"
    p.write_text(src, encoding="utf-8", newline="\n")
    wasm = out_dir / f"{stem}.wasm"
    r = subprocess.run(
        [str(find_pyths()), "compile", str(p), "--target", "wasm", "-o", str(wasm)],
        capture_output=True, text=True,
    )
    if r.returncode != 0:
        raise RuntimeError(f"pyths compile failed:\n{r.stderr}\n{r.stdout}")
    return wasm if (wasm.is_file() and wasm.stat().st_size > 0) else None


# --------------------------------------------------------------------------- the array-kernel matrix
# All ring ops (+/-/*) + comparisons only, so every kernel WASM-admits on its dtype (uint8/int32
# non-ring ops like //,&,>> route to JS by the G2 wrap-fidelity guard; array `for x in a` routes to
# JS by G1 -- see tests/pythscribe/test_array_server_e2e.py). `range(len(a))` indexing throughout.
ARRAY_MATRIX_SRC = """\
def add1_i32(a: Array[int32], out: Array[int32]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + 1

def add1_i64(a: Array[int64], out: Array[int64]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + 1

def add1_u8(a: Array[uint8], out: Array[uint8]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + 1

def scale_f32(a: Array[float32], out: Array[float32], k: float) -> None:
    for i in range(len(a)):
        out[i] = a[i] * k

def scale_f64(a: Array[float64], out: Array[float64], k: float) -> None:
    for i in range(len(a)):
        out[i] = a[i] * k

def total_f64(a: Array[float64]) -> float:
    s = 0.0
    for i in range(len(a)):
        s = s + a[i]
    return s

def scale_i32_2d(a: Array[int32, 2], out: Array[int32, 2], k: int) -> None:
    for i in range(len(a)):
        for j in range(a.shape[1]):
            out[i][j] = a[i][j] * k

def scale_i64_2d(a: Array[int64, 2], out: Array[int64, 2], k: int) -> None:
    for i in range(len(a)):
        for j in range(a.shape[1]):
            out[i][j] = a[i][j] * k

def scale_f32_2d(a: Array[float32, 2], out: Array[float32, 2], k: float) -> None:
    for i in range(len(a)):
        for j in range(a.shape[1]):
            out[i][j] = a[i][j] * k

def total_f64_2d(a: Array[float64, 2]) -> float:
    s = 0.0
    for i in range(len(a)):
        for j in range(a.shape[1]):
            s = s + a[i][j]
    return s

def threshold_u8_2d(a: Array[uint8, 2], out: Array[uint8, 2]) -> None:
    for i in range(len(a)):
        for j in range(a.shape[1]):
            out[i][j] = 255 if a[i][j] > 128 else 0

def downscale_nn(img: Array[uint8, 2], scale: int, oh: int, ow: int, out: Array[uint8, 2]) -> int:
    for oy in range(oh):
        iy = oy * scale
        for ox in range(ow):
            ix = ox * scale * 3
            o = ox * 3
            out[oy][o] = img[iy][ix]
            out[oy][o + 1] = img[iy][ix + 1]
            out[oy][o + 2] = img[iy][ix + 2]
    return oh * ow
"""

ARRAY_MATRIX_FNS = (
    "add1_i32", "add1_i64", "add1_u8", "scale_f32", "scale_f64", "total_f64",
    "scale_i32_2d", "scale_i64_2d", "scale_f32_2d", "total_f64_2d", "threshold_u8_2d", "downscale_nn",
)


# --------------------------------------------------------------------------- browser channel (V8 shim)
# Thin glue over the SHIPPED shim (the marshalling lives in list_buffer.mjs, not here): encode a
# NumPy array as the runner's arg descriptor, call, decode the read-back. Lifted verbatim from
# tests/pythscribe/test_array_isomorphic.py so the notebook exercises the exact shim the tests do.
_CTOR = {np.dtype("int32"): "int32", np.dtype("int64"): "int64", np.dtype("float32"): "float32",
         np.dtype("float64"): "float64", np.dtype("uint8"): "uint8"}


def _shim_arg(x):
    if isinstance(x, np.ndarray):
        dt = _CTOR[x.dtype]
        if x.ndim == 1:
            d = [str(int(v)) for v in x.tolist()] if dt == "int64" else x.tolist()
            return {"a1": dt, "d": d}
        rows = [[str(int(v)) for v in row] for row in x.tolist()] if dt == "int64" else x.tolist()
        return {"a2": dt, "rows": rows}
    if isinstance(x, int) and not isinstance(x, bool):
        return {"s": str(x)}   # `int` crosses as BigInt
    return {"s": x}


def shim_array_call(wasm: Path, fn: str, param_types, args, read_back, return_type):
    """Run a compiled array kernel through the BROWSER shim (list_buffer.mjs) under Node/V8.
    Returns {"ok": bool, "ret": ..., "readback": [dumped, ...]} | {"ok": False, "error": ...}."""
    node = shutil.which("node")
    if not node:
        raise RuntimeError("node is required for the browser-channel (V8 shim) arm")
    spec = {"paramTypes": list(param_types), "args": [_shim_arg(a) for a in args],
            "readBack": list(read_back), "returnType": return_type}
    r = subprocess.run([node, str(ARRAY_SHIM_RUNNER), str(SHIM), str(wasm), fn],
                       input=json.dumps(spec), capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"shim runner crashed: {r.stderr[:1500]}")
    return json.loads(r.stdout)


def shim_readback_np(dumped, dtype: str, ndim: int) -> np.ndarray:
    if ndim == 1:
        if dtype == "int64":
            return np.array([int(v) for v in dumped], dtype=np.int64)
        return np.array(dumped, dtype=getattr(np, dtype))
    if dtype == "int64":
        return np.array([[int(v) for v in row] for row in dumped], dtype=np.int64)
    return np.array(dumped, dtype=getattr(np, dtype))


# --------------------------------------------------------------------------- M4 admission SPOT sources
# (verbatim from tests/pythscribe/test_m4_numba_admission.py -- the honest, NARROW admission edge.)
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


def guarded_sum_ref(n: int) -> int:
    """Independent CPython oracle for the admission-SPOT kernel."""
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


# --------------------------------------------------------------------------- M4 Livermore counterweight

def run_livermore(quick: bool = False, kernels: str | None = None, reps: int = 3) -> str:
    """Run the SHIPPED M4 server-row harness (benchmarks/livermore_server_row/run.py) and return
    its printed table. The harness itself is the authority: it times CPython / NumPy / Numba / @wasm
    in one process and tallies the fastest column. @wasm wins 0 speed columns -- the honest
    counterweight, not restated here but produced by the harness."""
    cmd = [sys.executable, str(LIVERMORE), "--reps", str(reps)]
    if quick:
        cmd.append("--quick")
    if kernels:
        cmd += ["--kernels", kernels]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"Livermore harness failed:\n{r.stderr[-2000:]}\n{r.stdout[-2000:]}")
    return r.stdout
