#!/usr/bin/env python3
"""array_net -- the 1-D numeric typed-array `@wasm` differential net (M2a-4b).

The sibling of `wasm_net.py` for the numeric-array ABI. It enumerates 1-D `Array[dtype]`
kernels across the FIVE admitted dtypes (int32/int64/float32/float64/uint8) and, per call,
diffs the shipped `pyths --target js+wasm` output against the ARRAY-SEMANTICS oracle --
**NumPy** with the declared dtype (validation.md makes NumPy THE authority for fixed-width
wrap / f32 rounding / reductions) -- on TWO shipped emitted paths:

    * `direct` -- the raw `.wasm` export (run_wasm_direct.mjs), marshalled through the exact
      shipped x8 typed-array header (16-byte [dtype][ndim][shape0][pad], elements @ptr+16,
      8-aligned ptr).  NO glue, NO JS twin: a wrong value in the EMITTED CODE cannot be
      masked by the twin returning the oracle answer (dual-track masking).
    * `glue`   -- the user-facing `--target js+wasm` entry (run_module.mjs), whose
      `__array_to_wasm` / `__array_write_back` marshal a real TypedArray and reroute a
      dtype mismatch to the JS twin (sound-by-refusal).

A 4th differential arm -- the CPython-`wasm32-wasi` REFERENCE run (validation.md Oracles) --
is **availability-PROBED** and **gated**: absent (this box has no wasi CPython+NumPy), it
records a TRACKED, VISIBLE skip state (never a silent pass), pinned by the ratchet so it
cannot silently vanish. NOTE: the actual reference-differential-UNDER-wasi run (running each
kernel's reference in the wasi interpreter and diffing per row) is **not yet implemented** --
the arm currently probes availability and reports a tracked state only; wiring the per-row
diff is a follow-up (it needs a wasi CPython+NumPy build, unavailable here). On this host and
CI the arm is honestly `skipped`, so there is no false-green.

Correctness bars (validation.md Float): **bit-exact** vs NumPy for int32/int64/uint8 and for
float64 elementwise single-op; **<= tolerance** for float32 multi-op (option (b): f64 compute,
narrow on store) with a pinned witness.  The MANDATORY wrap-boundary witnesses (int32 @2**31,
int64 @2**63, uint8 255+1) are discriminating rows -- GREEN only because codegen WRAPS mod-width
(a refusing or i64-widening lowering makes them RED; the i64-widen discrimination control is
`test_array_net_anti_vacuity.py`).

`array_baseline.json` is the SAME two-sided ratchet as wasm_net (per-arm state pinned; a RED not
in the baseline is a regression, a baseline RED gone GREEN is stale, row/call COUNT pinned; NO
--skip flag) -- so the array rows are a standing ratchet.  Paired negative controls (anti-vacuity
d'): `--mutant {no-marshalling,wrong-width}` inject false-worlds the net MUST catch (RED);
`test_array_net_anti_vacuity.py` drives them + the glue runtime-check-removed + wrap discrimination.

Run from the repo root (needs target/{release,debug}/pyths, node, numpy):
    python tests/differential/wasm_net/array_net.py [--only NAME] [--update-baseline]
      [--mutant NAME] [--report FILE]
Env: PYTHS_BIN, ARRAY_NET_SCRATCH, PYTHS_WASI_PYTHON (the 4th-arm gate).
"""
from __future__ import annotations

import argparse
import json
import math
import os
import shutil
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import wasm_net  # noqa: E402  (shared driving primitives: compile, arm_json, Row, ratchet, ...)

try:
    import numpy as np
except ImportError:  # numpy is the array-semantics oracle -- a hard requirement for this net.
    print("[array_net] numpy is required (the array-semantics oracle)", file=sys.stderr)
    raise

ROOT = wasm_net.ROOT
PYTHS = wasm_net.PYTHS
SCRATCH = Path(os.environ.get("ARRAY_NET_SCRATCH") or (HERE / ".scratch_array"))
BASELINE = HERE / "array_baseline.json"
ARRAY_MUTANT = os.environ.get("WASM_NET_ARRAY_MUTANT", "")

DTYPES = ("int32", "int64", "float32", "float64", "uint8")
NP_DTYPE = {"int32": np.int32, "int64": np.int64, "float32": np.float32,
            "float64": np.float64, "uint8": np.uint8}
INT_DTYPES = {"int32", "int64", "uint8"}
FLOAT_DTYPES = {"float32", "float64"}
F32_RTOL = 1e-6
F32_ATOL = 1e-6

# ---------------------------------------------------------------------------- corpus model


@dataclass
class ArrFn:
    name: str
    params: list[str]           # e.g. ["Array[int32]", "Array[int32]", "int"]
    ret: str                    # "None" | "int"
    calls: list[list]           # per call: list of args (array arg = list of element values)
    ref: object = None          # ref(decoded_args) -> (outs: dict[int->np.ndarray], ret_val)
    readback: list = field(default_factory=list)  # out-buffer param indices to compare
    expect_wasm: bool = True     # False => admission must REFUSE it (direct = refused(expected))


@dataclass
class ArrModule:
    name: str
    source: str
    fns: list[ArrFn]


def dtype_of(param: str) -> str:
    return param[len("Array["):-1].split(",")[0].strip()


def ndim_of(param: str) -> int:
    parts = param[len("Array["):-1].split(",")
    return int(parts[1].strip()) if len(parts) > 1 else 1


def enc_elems(values, dtype: str) -> list:
    """Encode array element values for the JSON hop: ints as decimal strings (exact), floats
    via wasm_net.enc (numbers or inf/-inf/nan/-0.0 tokens)."""
    if dtype in INT_DTYPES:
        return [str(int(v)) for v in values]
    return [wasm_net.enc(float(v)) for v in values]


def enc_array(values, dtype: str, ndim: int) -> list:
    """Encode a 1-D flat OR 2-D nested (list-of-rows) array for the JSON hop."""
    if ndim == 2:
        return [enc_elems(row, dtype) for row in values]
    return enc_elems(values, dtype)


def _decode_row(values, dtype: str) -> list:
    if dtype in INT_DTYPES:
        return [int(v) for v in values]
    dec = []
    for v in values:
        if v == "inf":
            dec.append(math.inf)
        elif v == "-inf":
            dec.append(-math.inf)
        elif v == "nan":
            dec.append(math.nan)
        elif v == "-0.0":
            dec.append(-0.0)
        else:
            dec.append(float(v))
    return dec


def decode_elems(values, dtype: str, ndim: int = 1) -> np.ndarray:
    """Decode a 1-D flat or 2-D nested encoded array into a C-contiguous NumPy ndarray of
    the declared dtype (the array-semantics oracle reference buffer)."""
    if ndim == 2:
        return np.array([_decode_row(row, dtype) for row in values], dtype=NP_DTYPE[dtype])
    return np.array(_decode_row(values, dtype), dtype=NP_DTYPE[dtype])


def decode_scalar(v, ty: str):
    if ty == "int":
        return int(v)
    if ty == "float":
        if isinstance(v, str) and v in ("inf", "-inf", "nan", "-0.0"):
            return {"inf": math.inf, "-inf": -math.inf, "nan": math.nan, "-0.0": -0.0}[v]
        return float(v)
    raise ValueError(ty)


def decode_args(fn: ArrFn, call: list) -> list:
    out = []
    for arg, ty in zip(call, fn.params):
        if ty.startswith("Array["):
            out.append(decode_elems(arg, dtype_of(ty), ndim_of(ty)))
        else:
            out.append(decode_scalar(arg, ty))
    return out


# ---------------------------------------------------------------------------- oracle norm


def py_bits(v: float) -> str:
    """A float -> little-endian IEEE-754 hex ('nan' for any NaN): the SAME transport the
    runners now emit for float array elements (lossless: preserves -0.0 / inf)."""
    if math.isnan(v):
        return "nan"
    return struct.pack("<d", float(v)).hex()


def decode_float_hex(h) -> float:
    if h == "nan":
        return math.nan
    return struct.unpack("<d", bytes.fromhex(h))[0]


def norm_np(arr: np.ndarray, dtype: str) -> list:
    # Flatten row-major (C-contiguous) so 1-D and 2-D out-buffers compare against the
    # runner's flat row-major read-back uniformly.
    flat = np.asarray(arr).reshape(-1).tolist()
    if dtype in INT_DTYPES:
        return [str(int(x)) for x in flat]
    return [float(x) for x in flat]


def norm_ret(val, ret: str) -> str:
    if ret == "None":
        return "None"
    if ret == "int":
        return str(int(val))
    if ret == "float":
        return wasm_net.enc(float(val)) if not isinstance(val, str) else val
    raise ValueError(ret)


def elems_equal(exp: list, got: list, dtype: str) -> tuple[bool, str]:
    if len(exp) != len(got):
        return False, f"len {len(exp)} != {len(got)}"
    if dtype in INT_DTYPES:
        for i, (a, b) in enumerate(zip(exp, got)):
            if str(a) != str(b):
                return False, f"[{i}] {a} != {b}"
        return True, ""
    # floats arrive as IEEE bit-hex from the runners. exp is a python float (from NumPy).
    # float64 => BIT-EXACT (catches -0.0 vs +0.0 and NaN-vs-finite); float32 => <= tolerance.
    for i, (a, b) in enumerate(zip(exp, got)):
        if dtype == "float64":
            if py_bits(float(a)) != str(b):
                return False, f"[{i}] f64 bits {py_bits(float(a))} != {b} (exp {float(a)!r})"
        else:  # float32 tolerance (option (b): f64 compute, narrow)
            fa, fb = float(a), decode_float_hex(b)
            if math.isnan(fa) and math.isnan(fb):
                continue
            if not (math.isclose(fa, fb, rel_tol=F32_RTOL, abs_tol=F32_ATOL) or fa == fb):
                return False, f"[{i}] f32 {fa!r} !~= {fb!r} (>tol)"
    return True, ""


# ---------------------------------------------------------------------------- corpus


def _u8(v):  # python int -> wrapped uint8 element (inputs stay in-range; helper for clarity)
    return int(v) & 0xFF


def value_modules() -> list[ArrModule]:
    mods = []

    # ---- int32 -------------------------------------------------------------------------
    i32_src = (
        "def add1_i32(a: Array[int32], out: Array[int32]) -> None:\n"
        "    for i in range(len(a)):\n        out[i] = a[i] + 1\n\n"
        "def scale_i32(a: Array[int32], out: Array[int32], k: int) -> None:\n"
        "    for i in range(len(a)):\n        out[i] = a[i] * k\n\n"
        "def cube_i32(a: Array[int32], out: Array[int32]) -> None:\n"
        "    for i in range(len(a)):\n        out[i] = a[i] * a[i] * a[i]\n"
    )
    i32_in = [0, 1, -1, 5, 100, -100, 2 ** 20, -(2 ** 20)]
    mods.append(ArrModule("arr_i32", i32_src, [
        ArrFn("add1_i32", ["Array[int32]", "Array[int32]"], "None",
              [[enc_elems(i32_in, "int32"), enc_elems([0] * len(i32_in), "int32")]],
              ref=lambda a, out: ({1: a + np.int32(1)}, None), readback=[1]),
        ArrFn("scale_i32", ["Array[int32]", "Array[int32]", "int"], "None",
              [[enc_elems([0, 1, -3, 7, 11, -50], "int32"), enc_elems([0] * 6, "int32"), wasm_net.enc(3)]],
              ref=lambda a, out, k: ({1: a * np.int32(k)}, None), readback=[1]),
        ArrFn("cube_i32", ["Array[int32]", "Array[int32]"], "None",
              [[enc_elems([3_000_000, 2_000_000, 1290, -3_000_000, 2 ** 20], "int32"), enc_elems([0] * 5, "int32")]],
              ref=lambda a, out: ({1: a * a * a}, None), readback=[1]),
    ]))

    # ---- int64 (+ scalar reduction) ----------------------------------------------------
    i64_src = (
        "def add1_i64(a: Array[int64], out: Array[int64]) -> None:\n"
        "    for i in range(len(a)):\n        out[i] = a[i] + 1\n\n"
        "def total_i64(a: Array[int64]) -> int:\n"
        "    s = 0\n    for i in range(len(a)):\n        s = s + a[i]\n    return s\n"
    )
    i64_in = [0, 1, -1, 5, 2 ** 40, -(2 ** 40)]
    mods.append(ArrModule("arr_i64", i64_src, [
        ArrFn("add1_i64", ["Array[int64]", "Array[int64]"], "None",
              [[enc_elems(i64_in, "int64"), enc_elems([0] * len(i64_in), "int64")]],
              ref=lambda a, out: ({1: a + np.int64(1)}, None), readback=[1]),
        ArrFn("total_i64", ["Array[int64]"], "int",
              [[enc_elems([1, 2, 3, 4, 5, -10], "int64")]],
              ref=lambda a: ({}, int(a.sum())), readback=[]),
    ]))

    # ---- uint8 -------------------------------------------------------------------------
    u8_src = (
        "def add1_u8(a: Array[uint8], out: Array[uint8]) -> None:\n"
        "    for i in range(len(a)):\n        out[i] = a[i] + 1\n"
    )
    mods.append(ArrModule("arr_u8", u8_src, [
        ArrFn("add1_u8", ["Array[uint8]", "Array[uint8]"], "None",
              [[enc_elems([0, 1, 2, 128, 200, 254], "uint8"), enc_elems([0] * 6, "uint8")]],
              ref=lambda a, out: ({1: a + np.uint8(1)}, None), readback=[1]),
    ]))

    # ---- float64 (elementwise single-op == IEEE-exact, incl. signed zero / non-finite) --
    f64_src = (
        "def scale_f64(a: Array[float64], out: Array[float64], k: float) -> None:\n"
        "    for i in range(len(a)):\n        out[i] = a[i] * k\n\n"
        "def neg_f64(a: Array[float64], out: Array[float64]) -> None:\n"
        "    for i in range(len(a)):\n        out[i] = -a[i]\n"
    )
    f64_in = [0.0, 1.5, -2.25, 3.125, 1e10, -7.0, math.inf, -math.inf]
    # neg over [+0.0, 1.0, inf] => [-0.0, -1.0, -inf]: the -0.0 and non-finite OUTPUT witness
    # (review S1) -- BIT-EXACT float64 compare distinguishes -0.0 from +0.0, which raw-number
    # transport would have collapsed.
    neg_in = [0.0, 1.0, -0.0, math.inf, -math.inf]
    mods.append(ArrModule("arr_f64", f64_src, [
        ArrFn("scale_f64", ["Array[float64]", "Array[float64]", "float"], "None",
              [[enc_elems(f64_in, "float64"), enc_elems([0.0] * len(f64_in), "float64"), wasm_net.enc(2.5)]],
              ref=lambda a, out, k: ({1: a * np.float64(k)}, None), readback=[1]),
        ArrFn("neg_f64", ["Array[float64]", "Array[float64]"], "None",
              [[enc_elems(neg_in, "float64"), enc_elems([0.0] * len(neg_in), "float64")]],
              ref=lambda a, out: ({1: -a}, None), readback=[1]),
    ]))

    # ---- float32 multi-op (option (b): <= tolerance) -----------------------------------
    f32_src = (
        "def madd_f32(a: Array[float32], b: Array[float32], out: Array[float32], k: float) -> None:\n"
        "    for i in range(len(a)):\n        out[i] = a[i] * k + b[i]\n"
    )
    _rng = np.random.default_rng(11)
    f32_a = _rng.standard_normal(32).astype(np.float32)
    f32_b = _rng.standard_normal(32).astype(np.float32)
    mods.append(ArrModule("arr_f32", f32_src, [
        ArrFn("madd_f32", ["Array[float32]", "Array[float32]", "Array[float32]", "float"], "None",
              [[enc_elems(f32_a.tolist(), "float32"), enc_elems(f32_b.tolist(), "float32"),
                enc_elems([0.0] * 32, "float32"), wasm_net.enc(1.25)]],
              ref=lambda a, b, out, k: ({2: (a * np.float32(k) + b)}, None), readback=[2]),
    ]))
    return mods


def value_modules_2d() -> list[ArrModule]:
    """M2b: 2-D C-contiguous row-major array kernels across the 5 dtypes. Shapes are
    NON-SQUARE (2×3 / 3×2) so a transpose-offset (`i*nrows+j`) miscompile diverges from
    NumPy — the discriminating shape the transpose mutant exploits. Each out-buffer row is
    compared FLAT row-major against NumPy."""
    mods = []

    # ---- int32 2-D scale (non-square 2×3): the transpose-discriminating kernel -------
    i32_src = (
        "def scale_i32_2d(a: Array[int32, 2], out: Array[int32, 2], k: int) -> None:\n"
        "    for i in range(len(a)):\n"
        "        for j in range(a.shape[1]):\n"
        "            out[i, j] = a[i, j] * k\n"
    )
    a_i32 = [[1, 2, 3], [4, 5, 6]]
    mods.append(ArrModule("arr2d_i32", i32_src, [
        ArrFn("scale_i32_2d", ["Array[int32, 2]", "Array[int32, 2]", "int"], "None",
              [[enc_array(a_i32, "int32", 2), enc_array([[0, 0, 0], [0, 0, 0]], "int32", 2), wasm_net.enc(10)]],
              ref=lambda a, out, k: ({1: a * np.int32(k)}, None), readback=[1]),
    ]))

    # ---- int64 2-D NESTED form `a[i][j]` + AugAssign (non-square 2×3) ----------------
    # The fused nested-subscript form is a separate codegen path from the tuple form; pin it
    # (and the `out[i][j] += v` desugar) so a regression in either is its own RED row.
    nested_src = (
        "def nested_i64_2d(a: Array[int64, 2], out: Array[int64, 2]) -> None:\n"
        "    for i in range(len(a)):\n"
        "        for j in range(a.shape[1]):\n"
        "            out[i][j] = a[i][j] * 2\n"
        "            out[i][j] += 1\n"
    )
    mods.append(ArrModule("arr2d_nested", nested_src, [
        ArrFn("nested_i64_2d", ["Array[int64, 2]", "Array[int64, 2]"], "None",
              [[enc_array([[1, 2, 3], [4, 5, 6]], "int64", 2), enc_array([[0, 0, 0], [0, 0, 0]], "int64", 2)]],
              ref=lambda a, out: ({1: a * np.int64(2) + np.int64(1)}, None), readback=[1]),
    ]))

    # ---- uint8 2-D image threshold (the flagship image kernel as a 2-D uint8 array) ---
    u8_src = (
        "def threshold_u8_2d(a: Array[uint8, 2], out: Array[uint8, 2]) -> None:\n"
        "    for i in range(len(a)):\n"
        "        for j in range(a.shape[1]):\n"
        "            out[i, j] = 255 if a[i, j] > 128 else 0\n"
    )
    img = [[0, 100, 200], [128, 129, 255]]   # a 2×3 uint8 "image"
    mods.append(ArrModule("arr2d_u8_img", u8_src, [
        ArrFn("threshold_u8_2d", ["Array[uint8, 2]", "Array[uint8, 2]"], "None",
              [[enc_array(img, "uint8", 2), enc_array([[0, 0, 0], [0, 0, 0]], "uint8", 2)]],
              ref=lambda a, out: ({1: np.where(a > np.uint8(128), np.uint8(255), np.uint8(0)).astype(np.uint8)}, None),
              readback=[1]),
    ]))

    # ---- float64 2-D negate (non-square, signed-zero / non-finite OUTPUT witness) -----
    f64_src = (
        "def neg_f64_2d(a: Array[float64, 2], out: Array[float64, 2]) -> None:\n"
        "    for i in range(len(a)):\n"
        "        for j in range(a.shape[1]):\n"
        "            out[i, j] = -a[i, j]\n"
    )
    a_f64 = [[0.0, 1.0, -0.0], [math.inf, -math.inf, 2.5]]
    mods.append(ArrModule("arr2d_f64", f64_src, [
        ArrFn("neg_f64_2d", ["Array[float64, 2]", "Array[float64, 2]"], "None",
              [[enc_array(a_f64, "float64", 2), enc_array([[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]], "float64", 2)]],
              ref=lambda a, out: ({1: -a}, None), readback=[1]),
    ]))

    # ---- int64 2-D total (scalar reduction return; 3×2) -------------------------------
    i64_src = (
        "def total_i64_2d(a: Array[int64, 2]) -> int:\n"
        "    s = 0\n"
        "    for i in range(len(a)):\n"
        "        for j in range(a.shape[1]):\n"
        "            s = s + a[i, j]\n"
        "    return s\n"
    )
    mods.append(ArrModule("arr2d_i64", i64_src, [
        ArrFn("total_i64_2d", ["Array[int64, 2]"], "int",
              [[enc_array([[1, 2], [3, 4], [5, 6]], "int64", 2)]],
              ref=lambda a: ({}, int(a.sum())), readback=[]),
    ]))

    # ---- float32 2-D multiply-add (option (b): <= tolerance) --------------------------
    f32_src = (
        "def madd_f32_2d(a: Array[float32, 2], b: Array[float32, 2], out: Array[float32, 2], k: float) -> None:\n"
        "    for i in range(len(a)):\n"
        "        for j in range(a.shape[1]):\n"
        "            out[i, j] = a[i, j] * k + b[i, j]\n"
    )
    _rng = np.random.default_rng(23)
    fa = _rng.standard_normal((2, 3)).astype(np.float32)
    fb = _rng.standard_normal((2, 3)).astype(np.float32)
    mods.append(ArrModule("arr2d_f32", f32_src, [
        ArrFn("madd_f32_2d", ["Array[float32, 2]", "Array[float32, 2]", "Array[float32, 2]", "float"], "None",
              [[enc_array(fa.tolist(), "float32", 2), enc_array(fb.tolist(), "float32", 2),
                enc_array([[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]], "float32", 2), wasm_net.enc(1.25)]],
              ref=lambda a, b, out, k: ({2: (a * np.float32(k) + b)}, None), readback=[2]),
    ]))
    return mods


def wrap_modules() -> list[ArrModule]:
    """The MANDATORY wrap-boundary witnesses (requirements: 'bit-exact int' rows MUST include a
    wrap-boundary witness). Each is GREEN only because codegen wraps mod-width == NumPy; a
    refusing / i64-widening lowering makes it RED (discrimination proven in the anti-vacuity test)."""
    src = (
        "def add1_i32(a: Array[int32], out: Array[int32]) -> None:\n"
        "    for i in range(len(a)):\n        out[i] = a[i] + 1\n\n"
        "def scale_i32(a: Array[int32], out: Array[int32], k: int) -> None:\n"
        "    for i in range(len(a)):\n        out[i] = a[i] * k\n\n"
        "def add1_i64(a: Array[int64], out: Array[int64]) -> None:\n"
        "    for i in range(len(a)):\n        out[i] = a[i] + 1\n\n"
        "def add1_u8(a: Array[uint8], out: Array[uint8]) -> None:\n"
        "    for i in range(len(a)):\n        out[i] = a[i] + 1\n"
    )
    return [ArrModule("arr_wrap", src, [
        ArrFn("add1_i32", ["Array[int32]", "Array[int32]"], "None",
              [[enc_elems([2 ** 31 - 1, 2 ** 31 - 2, -(2 ** 31)], "int32"), enc_elems([0] * 3, "int32")]],
              ref=lambda a, out: ({1: a + np.int32(1)}, None), readback=[1]),
        ArrFn("scale_i32", ["Array[int32]", "Array[int32]", "int"], "None",
              [[enc_elems([2 ** 31 - 1], "int32"), enc_elems([0], "int32"), wasm_net.enc(2)]],
              ref=lambda a, out, k: ({1: a * np.int32(k)}, None), readback=[1]),
        ArrFn("add1_i64", ["Array[int64]", "Array[int64]"], "None",
              [[enc_elems([2 ** 63 - 1, 0, -5], "int64"), enc_elems([0] * 3, "int64")]],
              ref=lambda a, out: ({1: a + np.int64(1)}, None), readback=[1]),
        ArrFn("add1_u8", ["Array[uint8]", "Array[uint8]"], "None",
              [[enc_elems([255, 254, 0, 200], "uint8"), enc_elems([0] * 4, "uint8")]],
              ref=lambda a, out: ({1: a + np.uint8(1)}, None), readback=[1]),
    ])]


def refusal_modules() -> list[ArrModule]:
    """Shapes admission must REFUSE (they stay JS): the direct arm reports `refused(expected)`
    and the glue path runs the faithful twin. The direct-arm refusal channel (validation:
    'strided/wrong-dtype/wrong-ndim ... direct arm refused(expected)')."""
    src = (
        # M2b: a BARE 2-D row `a[i]` used as a VALUE (no row-view in the ABI) → G3 refuse.
        "def bare_row_2d(a: Array[int32, 2], out: Array[int32, 2]) -> None:\n"
        "    for i in range(len(a)):\n        row = a[i]\n"
        "        for j in range(a.shape[1]):\n            out[i, j] = row[j] + 1\n\n"
        "def foriter(a: Array[int64]) -> int:\n"
        "    s = 0\n    for x in a:\n        s = s + x\n    return s\n\n"
        "def u8avg(a: Array[uint8], b: Array[uint8], out: Array[uint8]) -> None:\n"
        "    for i in range(len(a)):\n        out[i] = (a[i] + b[i]) // 2\n"
    )
    return [ArrModule("arr_refuse", src, [
        ArrFn("bare_row_2d", ["Array[int32, 2]", "Array[int32, 2]"], "None",
              [[enc_array([[1, 2], [3, 4]], "int32", 2), enc_array([[0, 0], [0, 0]], "int32", 2)]],
              expect_wasm=False),
        ArrFn("foriter", ["Array[int64]"], "int",
              [[enc_elems([1, 2, 3], "int64")]], expect_wasm=False),
        ArrFn("u8avg", ["Array[uint8]", "Array[uint8]", "Array[uint8]"], "None",
              [[enc_elems([200, 4], "uint8"), enc_elems([100, 2], "uint8"), enc_elems([0, 0], "uint8")]],
              expect_wasm=False),
    ])]


def corpus(only: str | None) -> list[ArrModule]:
    mods = value_modules() + value_modules_2d() + wrap_modules() + refusal_modules()
    if only:
        mods = [m for m in mods if m.name == only]
    return mods


# ---------------------------------------------------------------------------- wasi 4th arm


def wasi_status() -> tuple[str, str]:
    """The CPython-wasm32-wasi 4th arm gate (validation Oracles). Returns (status, detail).
    status is 'skipped' (TRACKED, VISIBLE -- never a silent pass) when no wasi CPython+NumPy
    reference is configured/runnable. When one IS present it is 'probed-no-diff': the arm
    confirms availability but the per-row reference-differential UNDER wasi is NOT yet
    implemented (a follow-up), so it deliberately does NOT claim a passing diff -- no
    overclaim (review S2). The arm is always REPORTED; the ratchet pins that the wasi row
    EXISTS (a silent removal is a SHRINK => RED)."""
    cmd = os.environ.get("PYTHS_WASI_PYTHON", "").strip()
    if not cmd:
        return "skipped", "PYTHS_WASI_PYTHON unset (no CPython-wasm32-wasi+numpy reference on this host)"
    import shlex
    try:
        probe = wasm_net.run([*shlex.split(cmd), "-c", "import numpy,sys;print(sys.version)"])
    except Exception as e:  # noqa: BLE001
        return "skipped", f"wasi python not runnable: {e!r}"
    if probe.returncode != 0:
        return "skipped", f"wasi python probe failed (numpy import?): {probe.stderr.strip()[:160]}"
    return "probed-no-diff", (f"wasi CPython+NumPy present ({probe.stdout.strip()[:60]}); "
                              "per-row reference-differential under wasi NOT yet implemented (follow-up)")


# ---------------------------------------------------------------------------- driving


def run_module(mod: ArrModule, scratch: Path) -> list[wasm_net.Row]:
    src_dir = scratch / "src"
    src_dir.mkdir(parents=True, exist_ok=True)
    src = src_dir / f"{mod.name}.ps"
    src.write_text(mod.source, encoding="utf-8")
    spec = scratch / f"{mod.name}.spec.json"
    spec.write_text(json.dumps({"functions": {
        f.name: {"params": f.params, "ret": f.ret, "calls": f.calls, "alias": []} for f in mod.fns
    }}), encoding="utf-8")

    w_out, log, det_note = wasm_net.compile_wasm_repeated(src, scratch, mod.name)
    skipped = wasm_net.wasm_skipped(log)
    rows: list[wasm_net.Row] = []

    # determinism row (free, from compile_wasm_repeated) -- same discipline as wasm_net.
    if det_note is not None:
        rows.append(wasm_net.Row(f"{mod.name}::__determinism", True, {"compile": "RED"}, [det_note],
                                 {"compile": {"status": "RED", "ovf": 0, "gaps": 0}}))
    else:
        rows.append(wasm_net.Row(f"{mod.name}::__determinism", False, {"compile": "ok"}, [],
                                 {"compile": {"status": "ok", "ovf": 0, "gaps": 0}}))

    if w_out is None:
        for f in mod.fns:
            rows.append(wasm_net.Row(f"{mod.name}::{f.name}", True, {"compile": "compile-fail"},
                                     [log.strip().splitlines()[-1] if log.strip() else "compile failed"],
                                     {"direct": {"status": "compile-fail", "ovf": 0, "gaps": 0},
                                      "glue": {"status": "compile-fail", "ovf": 0, "gaps": 0}}))
        return rows

    glue = wasm_net.arm_json(["node", str(HERE / "run_module.mjs"), str(w_out), str(spec)], cwd=str(scratch))
    wasm = w_out.with_suffix(".wasm")
    if wasm.exists():
        direct = wasm_net.arm_json(["node", str(HERE / "run_wasm_direct.mjs"), str(wasm), str(spec)], cwd=str(scratch))
    else:
        # No `.wasm` emitted => every function JS-routed (all refused). The raw-WASM arm
        # sees each as not-exported (the direct-arm refusal channel), never masked.
        direct = {f.name: [{"kind": "not-exported"} for _ in f.calls] for f in mod.fns}

    for f in mod.fns:
        rid = f"{mod.name}::{f.name}"
        arms: dict[str, str] = {}
        state: dict[str, dict] = {}
        notes: list[str] = []
        red = False
        # oracle expectations per call
        exp_arrays = []
        exp_values = []
        for call in f.calls:
            if f.ref is None:
                exp_arrays.append(None)
                exp_values.append(None)
                continue
            args = decode_args(f, call)
            outs, ret_val = f.ref(*args)
            exp_arrays.append({idx: norm_np(outs[idx], dtype_of(f.params[idx])) for idx in f.readback})
            exp_values.append(norm_ret(ret_val, f.ret))

        for arm, res in (("direct", direct), ("glue", glue)):
            if isinstance(res, str):
                arms[arm] = "arm-failed"
                state[arm] = {"status": "arm-failed", "ovf": 0, "gaps": 0}
                notes.append(f"{arm}: {res}")
                red = True
                continue
            outs = res.get(f.name)
            if outs is None:
                arms[arm] = "missing"
                state[arm] = {"status": "missing", "ovf": 0, "gaps": 0}
                notes.append(f"{arm}: function missing from arm output")
                red = True
                continue
            # refusal handling on the direct arm (raw wasm) -- expect not-exported.
            if arm == "direct" and all(o.get("kind") == "not-exported" for o in outs):
                reason = skipped.get(f.name, "no `Skipped` line in verbose log")
                if f.expect_wasm is False:
                    arms[arm] = "refused(expected)"
                    state[arm] = {"status": "refused(expected)", "ovf": 0, "gaps": 0}
                    notes.append(f"direct: refused -- {reason}")
                else:
                    arms[arm] = "REFUSED"
                    state[arm] = {"status": "REFUSED", "ovf": 0, "gaps": 0}
                    notes.append(f"direct: admission refused a shape the net expects admitted -- {reason}")
                    red = True
                continue
            if f.expect_wasm is False:
                # the glue arm runs the faithful JS twin for a refused shape -> that is fine
                # (a refused shape has no NumPy-exact contract here; the point is the DIRECT
                # arm refused it and nothing silently misread). Informational, never RED.
                kinds = {o.get("kind") for o in outs}
                arms[arm] = "twin(" + ",".join(sorted(kinds)) + ")"
                state[arm] = {"status": "twin", "ovf": 0, "gaps": 0}
                continue
            mism = 0
            first = []
            for i, o in enumerate(outs):
                if o.get("kind") != "ok":
                    mism += 1
                    if len(first) < 3:
                        first.append(f"call#{i}: kind={o.get('kind')} {o.get('exc', o.get('value', ''))}")
                    continue
                # value (scalar return)
                if norm_ret_ok(o.get("value"), exp_values[i]) is False:
                    mism += 1
                    if len(first) < 3:
                        first.append(f"call#{i} value: {o.get('value')} != {exp_values[i]}")
                # out-buffers
                got_arrays = o.get("arrays", {})
                for idx in f.readback:
                    exp = exp_arrays[i][idx]
                    got = got_arrays.get(str(idx))
                    if got is None:
                        mism += 1
                        first.append(f"call#{i} arr[{idx}]: missing")
                        continue
                    ok, note = elems_equal(exp, got, dtype_of(f.params[idx]))
                    if not ok:
                        mism += 1
                        if len(first) < 3:
                            first.append(f"call#{i} arr[{idx}]: {note}")
            tag = "ok" if mism == 0 else f"RED {mism}/{len(outs)}"
            arms[arm] = tag
            state[arm] = {"status": "ok" if mism == 0 else "RED", "ovf": 0, "gaps": 0, "mism": mism}
            if mism:
                red = True
                notes.extend(f"{arm}: {x}" for x in first)
        rows.append(wasm_net.Row(rid, red, arms, notes, state))
    return rows


def norm_ret_ok(got, expected) -> bool:
    if expected is None:
        return True
    return str(got) == str(expected)


# ---------------------------------------------------------------------------- main


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--only")
    ap.add_argument("--update-baseline", action="store_true")
    ap.add_argument("--mutant", help="inject a false-world into the direct arm (no-marshalling|wrong-width|transpose); the net MUST catch it (RED)")
    ap.add_argument("--report")
    args = ap.parse_args()

    if not Path(PYTHS).exists():
        print(f"[array_net] pyths binary not found: {PYTHS}", file=sys.stderr)
        return 2
    if not shutil.which("node"):
        print("[array_net] node not on PATH", file=sys.stderr)
        return 2

    if args.mutant:
        os.environ["WASM_NET_ARRAY_MUTANT"] = args.mutant

    ver = wasm_net.run([str(PYTHS), "--version"]).stdout.strip()
    sha = wasm_net.run(["git", "rev-parse", "--short", "HEAD"], cwd=str(ROOT)).stdout.strip()
    print(f"[array_net] pyths={ver} ({PYTHS}) git={sha} numpy={np.__version__} mutant={args.mutant or '-'}")

    if SCRATCH.exists():
        shutil.rmtree(SCRATCH)
    SCRATCH.mkdir(parents=True)

    rows: list[wasm_net.Row] = []
    mods = corpus(args.only)
    for mod in mods:
        rows.extend(run_module(mod, SCRATCH))

    # the wasi 4th arm -- a wired, availability-gated, TRACKED row (never silent).
    if not args.only:
        st, detail = wasi_status()
        rows.append(wasm_net.Row("wasi::__arm", False, {"wasi": f"{st}: {detail}"}, [],
                                 {"wasi": {"status": "wired", "ovf": 0, "gaps": 0}}))

    n_calls = sum(len(f.calls) for m in mods for f in m.fns)
    red = [r for r in rows if r.red]
    print(f"[array_net] {len(rows)} rows ({len(mods)} modules, {n_calls} calls x arms): {len(rows) - len(red)} green / {len(red)} RED")
    for r in rows:
        mark = "RED  " if r.red else "ok   "
        print(f"  {mark}{r.id:34s} {r.arms}")
        for n in r.notes:
            print(f"        - {n}")

    if args.report:
        Path(args.report).write_text(json.dumps({"pyths": ver, "git": sha, "mutant": args.mutant,
                                                  "rows": [r.__dict__ for r in rows]}, indent=1), encoding="utf-8")

    if args.mutant:
        # In mutant mode the net MUST catch the false-world: >=1 direct row RED. Exit 0 =
        # caught (the paired control fires), 1 = the mutant slipped through (vacuous net).
        caught = [r.id for r in red if any(v == "RED" for k, v in
                  {k: s.get("status") for k, s in r.state.items()}.items()) and "direct" in r.state]
        print(f"[array_net] MUTANT {args.mutant}: {len(caught)} direct row(s) caught RED: {caught[:12]}")
        return 0 if caught else 1

    if args.only:
        print("[array_net] --only: ratchet not evaluated")
        return 1 if red else 0

    base = load_baseline()
    if args.update_baseline:
        old_state = base.get("state", {})
        state = {}
        for r in rows:
            entry = {"arms": r.state}
            if r.red:
                entry["note"] = old_state.get(r.id, {}).get("note", "NEW: unclassified -- annotate")
            state[r.id] = entry
        out = {"_note": base.get("_note", "array_net standing ratchet -- see array_net.py"),
               "rows": len(rows), "calls": n_calls, "state": state}
        BASELINE.write_text(json.dumps(out, indent=1) + "\n", encoding="utf-8")
        print(f"[array_net] baseline updated: rows={len(rows)} calls={n_calls} red={len(red)}")
        return 0

    problems = wasm_net.ratchet(rows, n_calls, base)
    for p in problems:
        print(f"[array_net] FAIL: {p}")
    if not problems:
        print(f"[array_net] ratchet OK: {len(red)} known-RED rows (baseline), every arm pinned")
    return 1 if problems else 0


def load_baseline() -> dict:
    if BASELINE.exists():
        return json.loads(BASELINE.read_text(encoding="utf-8"))
    return {"rows": 0, "calls": 0, "state": {}}


if __name__ == "__main__":
    sys.exit(main())
