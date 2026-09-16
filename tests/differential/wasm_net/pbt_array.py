#!/usr/bin/env python3
"""Property-based differential for the 1-D numeric array ABI (M2a-4b) — the shipped WASM
array kernels vs the NumPy oracle, on the RAW `.wasm` export (no JS-twin masking).

Hypothesis (`py -3.14`) generates random-size, random-value arrays per dtype; each batch is
compiled ONCE through the SHIPPED `pyths --target js+wasm` and driven on the raw export
(`run_wasm_direct.mjs`, the extended Array marshalling). The out-buffer / scalar result is
diffed against NumPy with the declared dtype:

    int32 / int64 / uint8   -> bit-exact (fixed-width WRAP)
    float64 single-op       -> bit-exact (IEEE deterministic)
    float32 multi-op        -> <= tolerance (option (b): f64 compute, narrow on store)

This is the PBT half of the anti-vacuity forms (a SPOT witness set lives in the net rows of
`array_net.py`; together they satisfy `feedback_anti_vacuity_paired_control`). Run from repo root:
    python tests/differential/wasm_net/pbt_array.py
Env: PYTHS_BIN, HYPOTHESIS_MAX (default 30 batches).
"""
from __future__ import annotations

import json
import math
import os
import shutil
import sys
import tempfile
from pathlib import Path

from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import wasm_net  # noqa: E402

try:
    import numpy as np
except ImportError:
    print("[pbt_array] numpy required (array oracle)", file=sys.stderr)
    raise

MAX = int(os.environ.get("HYPOTHESIS_MAX", "30"))
SCRATCH = Path(tempfile.mkdtemp(prefix="pbt_array_"))

SRC = """
def add1_i32(a: Array[int32], out: Array[int32]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + 1

def scale_i32(a: Array[int32], out: Array[int32], k: int) -> None:
    for i in range(len(a)):
        out[i] = a[i] * k

def add1_i64(a: Array[int64], out: Array[int64]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + 1

def add1_u8(a: Array[uint8], out: Array[uint8]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + 1

def scale_f64(a: Array[float64], out: Array[float64], k: float) -> None:
    for i in range(len(a)):
        out[i] = a[i] * k

def madd_f32(a: Array[float32], b: Array[float32], out: Array[float32], k: float) -> None:
    for i in range(len(a)):
        out[i] = a[i] * k + b[i]

def total_i64(a: Array[int64]) -> int:
    s = 0
    for i in range(len(a)):
        s = s + a[i]
    return s

def scale_i32_2d(a: Array[int32, 2], out: Array[int32, 2], k: int) -> None:
    for i in range(len(a)):
        for j in range(a.shape[1]):
            out[i, j] = a[i, j] * k

def add1_u8_2d(a: Array[uint8, 2], out: Array[uint8, 2]) -> None:
    for i in range(len(a)):
        for j in range(a.shape[1]):
            out[i, j] = a[i, j] + 1

def scale_f64_2d(a: Array[float64, 2], out: Array[float64, 2], k: float) -> None:
    for i in range(len(a)):
        for j in range(a.shape[1]):
            out[i, j] = a[i, j] * k
"""

_WASM = None


def wasm() -> Path:
    global _WASM
    if _WASM is None:
        d = SCRATCH / "k"
        d.mkdir(parents=True, exist_ok=True)
        ps = d / "k.ps"
        ps.write_text(SRC.lstrip("\n"), encoding="utf-8")
        out = d / "k.js"
        r = wasm_net.run([str(wasm_net.PYTHS), "compile", str(ps), "--target", "js+wasm", "-o", str(out),
                          "--verbose"], env={**os.environ, "PYTHS_NO_CACHE": "1"})
        log = wasm_net.ANSI.sub("", (r.stdout or "") + (r.stderr or ""))
        assert r.returncode == 0, log
        w = out.with_suffix(".wasm")
        assert w.exists(), log
        _WASM = w
    return _WASM


def enc_i(vs):
    return [str(int(v)) for v in vs]


def enc_f(vs):
    return [wasm_net.enc(float(v)) for v in vs]


def run_direct(spec: dict) -> dict:
    sp = wasm().with_suffix(".spec.json")
    sp.write_text(json.dumps(spec), encoding="utf-8")
    res = wasm_net.arm_json(["node", str(HERE / "run_wasm_direct.mjs"), str(wasm()), str(sp)], cwd=str(SCRATCH))
    assert isinstance(res, dict), res
    return res


import struct  # noqa: E402


def _dec(h):
    """Decode a float array element: the runners emit little-endian IEEE-754 hex ('nan' for
    NaN) so -0.0 / inf / nan survive the JSON hop (raw numbers would map inf/nan to `null`)."""
    if h == "nan":
        return math.nan
    return struct.unpack("<d", bytes.fromhex(h))[0]


def eq_int(exp: np.ndarray, got: list) -> None:
    got_i = [int(x) for x in got]
    assert exp.tolist() == got_i, f"int mismatch: {exp.tolist()} != {got_i}"


def eq_f64(exp: np.ndarray, got: list) -> None:
    # f64 single-op elementwise == IEEE-deterministic => BIT-EXACT (nan-aware).
    for a, b in zip(exp.tolist(), got):
        fa, fb = float(a), _dec(b)
        if math.isnan(fa) and math.isnan(fb):
            continue
        assert struct.pack("<d", fa) == struct.pack("<d", fb), f"f64 bit mismatch: {fa!r} != {fb!r}"


def eq_f32(exp: np.ndarray, got: list) -> None:
    for a, b in zip(exp.tolist(), got):
        fa, fb = float(a), _dec(b)
        if math.isnan(fa) and math.isnan(fb):
            continue
        assert math.isclose(fa, fb, rel_tol=1e-6, abs_tol=1e-6) or fa == fb, f"f32 >tol: {fa!r} !~= {fb!r}"


i32v = st.integers(min_value=-(2 ** 31), max_value=2 ** 31 - 1)
i64v = st.integers(min_value=-(2 ** 63), max_value=2 ** 63 - 1)
u8v = st.integers(min_value=0, max_value=255)
f64v = st.one_of(st.floats(allow_nan=True, allow_infinity=True, width=64),
                 st.sampled_from([0.0, -0.0, 1.0, -1.0, 1e10, 1e-10]))
f32v = st.one_of(st.floats(allow_nan=False, allow_infinity=False, width=32, min_value=-1e6, max_value=1e6),
                 st.sampled_from([0.0, -0.0, 1.0, -1.0, 0.5, -0.25]))

batch = st.fixed_dictionaries({
    "i32": st.lists(st.lists(i32v, min_size=1, max_size=8), min_size=3, max_size=8),
    "i64": st.lists(st.lists(i64v, min_size=1, max_size=8), min_size=3, max_size=8),
    "u8": st.lists(st.lists(u8v, min_size=1, max_size=8), min_size=3, max_size=8),
    "f64": st.lists(st.lists(f64v, min_size=1, max_size=8), min_size=3, max_size=8),
    "f32": st.lists(st.tuples(st.lists(f32v, min_size=1, max_size=6), st.lists(f32v, min_size=1, max_size=6)),
                    min_size=3, max_size=6),
    "k32": st.integers(min_value=-5, max_value=5),
    "k64": st.floats(allow_nan=False, allow_infinity=False, width=64, min_value=-3.0, max_value=3.0),
})


@settings(max_examples=MAX, deadline=None, suppress_health_check=list(HealthCheck))
@given(batch)
def test_array_direct_vs_numpy(b):
    calls_add_i32, calls_scale_i32, refs_i32, refs_scale = [], [], [], []
    for xs in b["i32"]:
        a = np.array(xs, dtype=np.int32)
        calls_add_i32.append([enc_i(xs), enc_i([0] * len(xs))])
        refs_i32.append(a + np.int32(1))
        calls_scale_i32.append([enc_i(xs), enc_i([0] * len(xs)), wasm_net.enc(int(b["k32"]))])
        refs_scale.append(a * np.int32(b["k32"]))
    calls_add_i64, refs_i64, calls_sum, refs_sum = [], [], [], []
    for xs in b["i64"]:
        a = np.array(xs, dtype=np.int64)
        calls_add_i64.append([enc_i(xs), enc_i([0] * len(xs))])
        refs_i64.append(a + np.int64(1))
        calls_sum.append([enc_i(xs)])
        refs_sum.append(int(a.sum()))
    calls_u8, refs_u8 = [], []
    for xs in b["u8"]:
        a = np.array(xs, dtype=np.uint8)
        calls_u8.append([enc_i(xs), enc_i([0] * len(xs))])
        refs_u8.append(a + np.uint8(1))
    calls_f64, refs_f64 = [], []
    for xs in b["f64"]:
        a = np.array(xs, dtype=np.float64)
        calls_f64.append([enc_f(xs), enc_f([0.0] * len(xs)), wasm_net.enc(float(b["k64"]))])
        refs_f64.append(a * np.float64(b["k64"]))
    calls_f32, refs_f32 = [], []
    for xa, xb in b["f32"]:
        n = min(len(xa), len(xb))
        xa, xb = xa[:n], xb[:n]
        a = np.array(xa, dtype=np.float32)
        bb = np.array(xb, dtype=np.float32)
        calls_f32.append([enc_f(xa), enc_f(xb), enc_f([0.0] * n), wasm_net.enc(float(b["k64"]))])
        refs_f32.append(a * np.float32(b["k64"]) + bb)

    spec = {"functions": {
        "add1_i32": {"params": ["Array[int32]", "Array[int32]"], "ret": "None", "calls": calls_add_i32, "alias": []},
        "scale_i32": {"params": ["Array[int32]", "Array[int32]", "int"], "ret": "None", "calls": calls_scale_i32, "alias": []},
        "add1_i64": {"params": ["Array[int64]", "Array[int64]"], "ret": "None", "calls": calls_add_i64, "alias": []},
        "total_i64": {"params": ["Array[int64]"], "ret": "int", "calls": calls_sum, "alias": []},
        "add1_u8": {"params": ["Array[uint8]", "Array[uint8]"], "ret": "None", "calls": calls_u8, "alias": []},
        "scale_f64": {"params": ["Array[float64]", "Array[float64]", "float"], "ret": "None", "calls": calls_f64, "alias": []},
        "madd_f32": {"params": ["Array[float32]", "Array[float32]", "Array[float32]", "float"], "ret": "None", "calls": calls_f32, "alias": []},
    }}
    res = run_direct(spec)
    for i, o in enumerate(res["add1_i32"]):
        assert o["kind"] == "ok", o
        eq_int(refs_i32[i], o["arrays"]["1"])
    for i, o in enumerate(res["scale_i32"]):
        assert o["kind"] == "ok", o
        eq_int(refs_scale[i], o["arrays"]["1"])
    for i, o in enumerate(res["add1_i64"]):
        eq_int(refs_i64[i], o["arrays"]["1"])
    for i, o in enumerate(res["total_i64"]):
        if o["kind"] == "ovf":
            continue  # by contract the twin answers an out-of-i64 sum
        assert o["kind"] == "ok", o
        assert int(o["value"]) == refs_sum[i], f"sum {o['value']} != {refs_sum[i]}"
    for i, o in enumerate(res["add1_u8"]):
        eq_int(refs_u8[i], o["arrays"]["1"])
    for i, o in enumerate(res["scale_f64"]):
        eq_f64(refs_f64[i], o["arrays"]["1"])
    for i, o in enumerate(res["madd_f32"]):
        eq_f32(refs_f32[i], o["arrays"]["2"])


# ---- M2b: 2-D property-based differential (random NON-SQUARE shapes) ----------------------
# A random rectangular rows×cols matrix (rows≠cols allowed, the transpose-discriminating
# regime). The runner marshals a 2-D array as a nested list-of-rows; the out-buffer reads
# back FLAT row-major, compared against NumPy's C-order flatten.
mat_i32 = st.lists(st.lists(i32v, min_size=1, max_size=6), min_size=1, max_size=6)
mat_u8 = st.lists(st.lists(u8v, min_size=1, max_size=6), min_size=1, max_size=6)
mat_f64 = st.lists(st.lists(f64v, min_size=1, max_size=6), min_size=1, max_size=6)


def _rect(rows):
    # Coerce a jagged list-of-rows into a rectangle by cropping to the min row length
    # (>=1), so every generated case is a valid C-contiguous 2-D array.
    c = min(len(r) for r in rows)
    c = max(c, 1)
    return [r[:c] for r in rows]


batch2d = st.fixed_dictionaries({
    "i32": st.lists(mat_i32, min_size=2, max_size=5),
    "u8": st.lists(mat_u8, min_size=2, max_size=5),
    "f64": st.lists(mat_f64, min_size=2, max_size=5),
    "k32": st.integers(min_value=-5, max_value=5),
    "k64": st.floats(allow_nan=False, allow_infinity=False, width=64, min_value=-3.0, max_value=3.0),
})


@settings(max_examples=MAX, deadline=None, suppress_health_check=list(HealthCheck))
@given(batch2d)
def test_array_2d_direct_vs_numpy(b):
    calls_i32, refs_i32 = [], []
    for m in b["i32"]:
        m = _rect(m)
        a = np.array(m, dtype=np.int32)
        zeros = [[0] * a.shape[1] for _ in range(a.shape[0])]
        calls_i32.append([[enc_i(r) for r in m], [enc_i(r) for r in zeros], wasm_net.enc(int(b["k32"]))])
        refs_i32.append((a * np.int32(b["k32"])).reshape(-1))
    calls_u8, refs_u8 = [], []
    for m in b["u8"]:
        m = _rect(m)
        a = np.array(m, dtype=np.uint8)
        zeros = [[0] * a.shape[1] for _ in range(a.shape[0])]
        calls_u8.append([[enc_i(r) for r in m], [enc_i(r) for r in zeros]])
        refs_u8.append((a + np.uint8(1)).reshape(-1))
    calls_f64, refs_f64 = [], []
    for m in b["f64"]:
        m = _rect(m)
        a = np.array(m, dtype=np.float64)
        zeros = [[0.0] * a.shape[1] for _ in range(a.shape[0])]
        calls_f64.append([[enc_f(r) for r in m], [enc_f(r) for r in zeros], wasm_net.enc(float(b["k64"]))])
        refs_f64.append((a * np.float64(b["k64"])).reshape(-1))

    spec = {"functions": {
        "scale_i32_2d": {"params": ["Array[int32, 2]", "Array[int32, 2]", "int"], "ret": "None", "calls": calls_i32, "alias": []},
        "add1_u8_2d": {"params": ["Array[uint8, 2]", "Array[uint8, 2]"], "ret": "None", "calls": calls_u8, "alias": []},
        "scale_f64_2d": {"params": ["Array[float64, 2]", "Array[float64, 2]", "float"], "ret": "None", "calls": calls_f64, "alias": []},
    }}
    res = run_direct(spec)
    for i, o in enumerate(res["scale_i32_2d"]):
        assert o["kind"] == "ok", o
        eq_int(refs_i32[i], o["arrays"]["1"])
    for i, o in enumerate(res["add1_u8_2d"]):
        assert o["kind"] == "ok", o
        eq_int(refs_u8[i], o["arrays"]["1"])
    for i, o in enumerate(res["scale_f64_2d"]):
        assert o["kind"] == "ok", o
        eq_f64(refs_f64[i], o["arrays"]["1"])


def main() -> int:
    if not Path(wasm_net.PYTHS).exists() or not shutil.which("node"):
        print("[pbt_array] pyths binary or node missing", file=sys.stderr)
        return 2
    print(f"[pbt_array] pyths={wasm_net.PYTHS} max_examples={MAX} numpy={np.__version__} python={sys.version.split()[0]}")
    test_array_direct_vs_numpy()
    print(f"[pbt_array] {MAX} batches x 5 dtypes (int32/int64/uint8/float64/float32) x random size+value — no divergence vs NumPy")
    test_array_2d_direct_vs_numpy()
    print(f"[pbt_array] {MAX} batches x 2-D (int32/uint8/float64) random rows×cols — no divergence vs NumPy")
    shutil.rmtree(SCRATCH, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
