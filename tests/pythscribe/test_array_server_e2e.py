"""M2a-3 gate: 1-D numeric `Array[dtype]` kernels run END-TO-END on the SERVER path
(real `pyths --target wasm` → `ServerKernel` under wasmtime → `array_buffer.py` marshalling),
out-buffer == NumPy. This is the shipped-path SPOT+PBT+RED-control suite for the codegen chunk.

The oracle is NumPy with the declared dtype (fixed-width WRAP for int32/int64/uint8, IEEE
rounding for f64, ≤tolerance for f32 multi-op — the settled decisions). Every property ships a
positive check AND a paired NEGATIVE CONTROL that goes RED (feedback_anti_vacuity_paired_control).

  E1  per-dtype fill/transform kernel: WASM out-buffer == NumPy bit-for-bit (int/uint8, f64) /
      ≤tolerance (f32); PBT over sizes+values.
  E2  MANDATORY wrap-boundary witnesses: int32 near 2³¹, uint8 255+1 → NumPy WRAP (not refuse,
      not i64-widen). RED control: an i64-widening store (list[int] path) would NOT wrap.
  E3  the total runtime check on the shipped call() path: a wrong-dtype buffer is REFUSED
      (server THROWS ArrayMarshalError), never silently misread.
  E4  RED mutant: skip the write-back (read_back=(), return_type="None") → the caller's out-buffer is unchanged.
  E5  #489 is NOT in this chunk (M2a-3b); f32 tolerance witness pinned here.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

from conftest import gate, gate_import

numpy = gate_import("numpy")
import numpy as np  # noqa: E402

wasmtime = gate_import("wasmtime")

from pythscribe.build import find_pyths  # noqa: E402
from pythscribe.runtime import ServerFfiError, ServerKernel, WasmTrap, wasmtime_available  # noqa: E402
from pythscribe.runtime.array_buffer import ArrayMarshalError  # noqa: E402

# Kernels exercised end-to-end. `out` is a caller-allocated out-buffer filled in place.
KERNEL_SRC = """
def add1_i32(a: Array[int32], out: Array[int32]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + 1

def add1_u8(a: Array[uint8], out: Array[uint8]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + 1

def add1_i64(a: Array[int64], out: Array[int64]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + 1

def scale_i32(a: Array[int32], out: Array[int32], k: int) -> None:
    for i in range(len(a)):
        out[i] = a[i] * k

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

def cube_i32(a: Array[int32], out: Array[int32]) -> None:
    for i in range(len(a)):
        out[i] = a[i] * a[i] * a[i]

def scale_i32_2d(a: Array[int32, 2], out: Array[int32, 2], k: int) -> None:
    for i in range(len(a)):
        for j in range(a.shape[1]):
            out[i, j] = a[i, j] * k

def threshold_u8_2d(a: Array[uint8, 2], out: Array[uint8, 2]) -> None:
    for i in range(len(a)):
        for j in range(a.shape[1]):
            out[i][j] = 255 if a[i][j] > 128 else 0

def total_f64_2d(a: Array[float64, 2]) -> float:
    s = 0.0
    for i in range(len(a)):
        for j in range(a.shape[1]):
            s = s + a[i, j]
    return s
"""


@pytest.fixture(scope="module")
def kernel(tmp_path_factory) -> ServerKernel:
    gate(wasmtime_available(), "wasmtime-py is required for the server path")
    d = tmp_path_factory.mktemp("m2a3")
    src = d / "k.ps"
    src.write_text(KERNEL_SRC)
    wasm = d / "k.wasm"
    pyths = find_pyths()
    r = subprocess.run(
        [str(pyths), "compile", str(src), "--target", "wasm", "-o", str(wasm)],
        capture_output=True, text=True,
    )
    assert r.returncode == 0, f"pyths compile failed: {r.stderr}\n{r.stdout}"
    assert wasm.is_file() and wasm.stat().st_size > 0, "no .wasm emitted (array kernel not admitted?)"
    # Every kernel above must have been WASM-admitted (not JS-routed): the export exists.
    k = ServerKernel.from_wasm(wasm, name="k")
    for fn in ("add1_i32", "add1_u8", "add1_i64", "scale_i32", "scale_f64", "madd_f32", "total_i64", "cube_i32",
               "scale_i32_2d", "threshold_u8_2d", "total_f64_2d"):
        assert fn in k.exports, f"{fn} not exported — array admission did not flip ON: {k.exports}"
    return k


# ---- E1: per-dtype fill/transform == NumPy ------------------------------------------------

def test_e1_add1_int32_matches_numpy(kernel):
    a = np.array([0, 1, -1, 5, 100, -100, 2 ** 20, -(2 ** 20)], dtype=np.int32)
    out = np.zeros_like(a)
    kernel.call("add1_i32", ["Array[int32]", "Array[int32]"], [a, out], read_back=[1], return_type="None")
    np.testing.assert_array_equal(out, (a + np.int32(1)))


def test_e1_scale_int32_matches_numpy(kernel):
    a = np.array([0, 1, -3, 7, 11, -50], dtype=np.int32)
    out = np.zeros_like(a)
    kernel.call("scale_i32", ["Array[int32]", "Array[int32]", "int"], [a, out, 3], read_back=[1], return_type="None")
    np.testing.assert_array_equal(out, (a * np.int32(3)))


def test_e1_scale_float64_matches_numpy(kernel):
    a = np.array([0.0, 1.5, -2.25, 3.125, 1e10, -7.0], dtype=np.float64)
    out = np.zeros_like(a)
    kernel.call("scale_f64", ["Array[float64]", "Array[float64]", "float"], [a, out, 2.5], read_back=[1], return_type="None")
    np.testing.assert_array_equal(out, a * 2.5)  # f64 single-op elementwise is IEEE-exact


def test_e1_add1_uint8_matches_numpy(kernel):
    a = np.array([0, 1, 2, 128, 200, 254], dtype=np.uint8)
    out = np.zeros_like(a)
    kernel.call("add1_u8", ["Array[uint8]", "Array[uint8]"], [a, out], read_back=[1], return_type="None")
    np.testing.assert_array_equal(out, (a + np.uint8(1)))


@pytest.mark.parametrize("n", [1, 2, 7, 33, 256, 1000])
def test_e1_pbt_int32_add1(kernel, n):
    rng = np.random.default_rng(n)
    a = rng.integers(-(2 ** 30), 2 ** 30, size=n, dtype=np.int32)
    out = np.zeros_like(a)
    kernel.call("add1_i32", ["Array[int32]", "Array[int32]"], [a, out], read_back=[1], return_type="None")
    np.testing.assert_array_equal(out, a + np.int32(1))


# ---- E2: MANDATORY wrap-boundary witnesses (== NumPy WRAP, not refuse / i64-widen) --------

def test_e2_wrap_boundary_int32(kernel):
    # int32 max + 1 wraps to int32 min (mod-2³²), exactly as NumPy.
    a = np.array([2 ** 31 - 1, 2 ** 31 - 2, -(2 ** 31)], dtype=np.int32)
    out = np.zeros_like(a)
    kernel.call("add1_i32", ["Array[int32]", "Array[int32]"], [a, out], read_back=[1], return_type="None")
    expect = a + np.int32(1)  # numpy wraps: [-2147483648, 2147483647, -2147483647]
    assert out[0] == -(2 ** 31), f"int32 2³¹-1 +1 must WRAP to -2³¹, got {out[0]}"
    np.testing.assert_array_equal(out, expect)


def test_e2_ring_chain_exact_beyond_i64_intermediate(kernel):
    # A pure ring-op chain (a*a*a) whose i64 INTERMEDIATE overflows 2⁶³ is STILL exact vs
    # NumPy int32: i64 ops wrap mod-2⁶⁴ (suppress_ovf → no refuse), the narrowing store is
    # mod-2³², and mod-2³²∘mod-2⁶⁴ = mod-2³² (ring homomorphism, 2³²|2⁶⁴). So G2 correctly
    # does NOT need to refuse ring chains — only non-ring ops diverge. 3_000_000³ ≈ 2.7e19 > 2⁶³.
    a = np.array([3_000_000, 2_000_000, 1290, -3_000_000, 2 ** 20], dtype=np.int32)
    out = np.zeros_like(a)
    kernel.call("cube_i32", ["Array[int32]", "Array[int32]"], [a, out], read_back=[1], return_type="None")
    with np.errstate(over="ignore"):
        ref = a * a * a
    np.testing.assert_array_equal(out, ref)


def test_e2_wrap_boundary_int32_mul(kernel):
    # (2³¹-1) * 2 wraps mod-2³² to -2, exactly as NumPy int32.
    a = np.array([2 ** 31 - 1], dtype=np.int32)
    out = np.zeros_like(a)
    kernel.call("scale_i32", ["Array[int32]", "Array[int32]", "int"], [a, out, 2], read_back=[1], return_type="None")
    with np.errstate(over="ignore"):
        ref = np.int32(2 ** 31 - 1) * np.int32(2)
    assert out[0] == ref == -2, f"got {out[0]}"


def test_e2_wrap_boundary_uint8(kernel):
    # 255 + 1 wraps to 0 (mod-256), exactly as NumPy uint8.
    a = np.array([255, 254, 0], dtype=np.uint8)
    out = np.zeros_like(a)
    kernel.call("add1_u8", ["Array[uint8]", "Array[uint8]"], [a, out], read_back=[1], return_type="None")
    assert out[0] == 0, f"uint8 255+1 must WRAP to 0, got {out[0]}"
    np.testing.assert_array_equal(out, a + np.uint8(1))


def test_e2_wrap_boundary_int64(kernel):
    # int64 max + 1 wraps to int64 min (mod-2⁶⁴) — the suppress-__ovf array-wrap mode
    # (NOT the Python-int refuse-on-overflow). NumPy int64 wraps identically.
    a = np.array([2 ** 63 - 1, 0, -5], dtype=np.int64)
    out = np.zeros_like(a)
    kernel.call("add1_i64", ["Array[int64]", "Array[int64]"], [a, out], read_back=[1], return_type="None")
    assert out[0] == -(2 ** 63), f"int64 2⁶³-1 +1 must WRAP to -2⁶³, got {out[0]}"
    np.testing.assert_array_equal(out, a + np.int64(1))


# ---- f32 tolerance witness (option (b): f64 compute, narrow on store) ----------------------

def test_f32_multiop_within_tolerance(kernel):
    rng = np.random.default_rng(7)
    a = rng.standard_normal(64).astype(np.float32)
    b = rng.standard_normal(64).astype(np.float32)
    out = np.zeros_like(a)
    k = 1.25
    kernel.call("madd_f32", ["Array[float32]", "Array[float32]", "Array[float32]", "float"],
                [a, b, out, k], read_back=[2], return_type="None")
    ref = (a * np.float32(k) + b)  # NumPy f32: f32(f32(a*k)+b)
    # option (b) computes a*k+b in f64 then narrows — NOT bit-exact, but ≤ a small tolerance.
    np.testing.assert_allclose(out, ref, rtol=1e-6, atol=1e-6)


# ---- E3: the total runtime check on the SHIPPED call() path (B1 soundness gate) -----------

def test_e3_wrong_dtype_buffer_refused(kernel):
    # An f32 buffer handed to an Array[int32] kernel must be REFUSED loudly by the total
    # marshaller check inside ServerKernel.call() — never flat-copied and read at the wrong
    # width (a silent misread). Server path THROWS (no JS twin here).
    a = np.array([1.0, 2.0, 3.0], dtype=np.float32)
    out = np.zeros(3, dtype=np.float32)
    with pytest.raises(ArrayMarshalError):
        kernel.call("add1_i32", ["Array[int32]", "Array[int32]"], [a, out], read_back=[1], return_type="None")


def test_e3_strided_buffer_refused(kernel):
    # A non-contiguous (strided) view is refused, never read at the wrong stride.
    base = np.arange(20, dtype=np.int32)
    a = base[::2]  # strided, not C-contiguous
    out = np.zeros(10, dtype=np.int32)
    with pytest.raises(ArrayMarshalError):
        kernel.call("add1_i32", ["Array[int32]", "Array[int32]"], [a, out], read_back=[1], return_type="None")


def test_e3_2d_ndim_mismatch_refused_on_call_path(kernel):
    # M2b: a 2-D annotation is admitted on the server path, but the TOTAL check on the
    # shipped call() path refuses an ndim MISMATCH (a 1-D buffer to a 2-D param, and a 2-D
    # buffer to a 1-D param) — never silently reshaped or treated as 1-D. ndim>2 in the
    # annotation is refused at the param-spec gate.
    a1 = np.zeros(4, dtype=np.int32)
    out1 = np.zeros(4, dtype=np.int32)
    with pytest.raises(ArrayMarshalError):
        kernel.call("scale_i32_2d", ["Array[int32, 2]", "Array[int32, 2]", "int"], [a1, out1, 2],
                    read_back=[1], return_type="None")
    a2 = np.zeros((2, 2), dtype=np.int32)
    out2 = np.zeros((2, 2), dtype=np.int32)
    with pytest.raises(ArrayMarshalError):
        kernel.call("add1_i32", ["Array[int32]", "Array[int32]"], [a2, out2], read_back=[1], return_type="None")
    with pytest.raises(ServerFfiError):
        kernel.call("scale_i32_2d", ["Array[int32, 3]", "Array[int32, 2]", "int"], [a2, out2, 2],
                    read_back=[1], return_type="None")


def test_e3b_misdeclared_ndim_traps_in_kernel_never_silent(kernel):
    # The derived SILENT arm (Fable r1): the server caller DECLARES param types, so a 2-D
    # kernel handed a 1-D buffer under the 1-D spelling `Array[int32]` gets a header with
    # ndim=1 / shape1=0 — pre-fix the kernel read `a.shape[1] == 0`, ran ZERO iterations
    # and returned with `out` untouched and NO error. The kernel-entry header guard
    # (emit.rs, ndim@4 vs compiled-for) now TRAPS (WasmTrap, loud). Mirror control: a
    # 2-D-spelled 2-D buffer into a 1-D kernel traps too (same class, both ndims guarded).
    a1 = np.array([1, 2, 3, 4, 5, 6], dtype=np.int32)
    out1 = np.zeros_like(a1)
    with pytest.raises(WasmTrap):
        kernel.call("scale_i32_2d", ["Array[int32]", "Array[int32]", "int"], [a1, out1, 10],
                    read_back=[1], return_type="None")
    np.testing.assert_array_equal(out1, np.zeros_like(a1))  # loud, and nothing written back
    a2 = np.array([[1, 2, 3], [4, 5, 6]], dtype=np.int32)
    out2 = np.zeros_like(a2)
    with pytest.raises(WasmTrap):
        kernel.call("add1_i32", ["Array[int32, 2]", "Array[int32, 2]"], [a2, out2],
                    read_back=[1], return_type="None")
    np.testing.assert_array_equal(out2, np.zeros_like(a2))


# ---- E6 (M2b): 2-D kernels end-to-end on the SERVER path == NumPy ------------------------

def test_e6_scale_int32_2d_matches_numpy(kernel):
    # NON-SQUARE (2×3): a transpose-offset miscompile would diverge. `a[i, j]` form.
    a = np.array([[1, 2, 3], [4, 5, 6]], dtype=np.int32)
    out = np.zeros_like(a)
    kernel.call("scale_i32_2d", ["Array[int32, 2]", "Array[int32, 2]", "int"], [a, out, 10],
                read_back=[1], return_type="None")
    np.testing.assert_array_equal(out, a * np.int32(10))


def test_e6_threshold_uint8_2d_image_kernel_nested_form(kernel):
    # The flagship image kernel as a 2-D uint8 array, `a[i][j]` (fused nested) form.
    img = np.array([[0, 100, 200], [128, 129, 255]], dtype=np.uint8)
    out = np.zeros_like(img)
    kernel.call("threshold_u8_2d", ["Array[uint8, 2]", "Array[uint8, 2]"], [img, out],
                read_back=[1], return_type="None")
    np.testing.assert_array_equal(out, np.where(img > np.uint8(128), np.uint8(255), np.uint8(0)))


def test_e6_total_f64_2d_scalar_return(kernel):
    a = np.array([[0.5, 1.5], [2.0, -1.0], [4.25, 0.0]], dtype=np.float64)
    r = kernel.call("total_f64_2d", ["Array[float64, 2]"], [a], read_back=[], return_type="float")
    assert float(r.value) == float(a.sum())


def test_e6_forder_and_strided_2d_refused_on_call_path(kernel):
    # The paired control on the SHIPPED call path: an F-order or strided 2-D buffer (same
    # values, non-C-contiguous) is REFUSED by the total check — never read at the wrong stride.
    c = np.array([[1, 2, 3], [4, 5, 6]], dtype=np.int32)
    out = np.zeros_like(c)
    f = np.asfortranarray(c)
    assert not f.flags.c_contiguous
    with pytest.raises(ArrayMarshalError):
        kernel.call("scale_i32_2d", ["Array[int32, 2]", "Array[int32, 2]", "int"], [f, out, 2],
                    read_back=[1], return_type="None")
    strided = np.arange(12, dtype=np.int32).reshape(2, 6)[:, ::2]
    assert not strided.flags.c_contiguous
    with pytest.raises(ArrayMarshalError):
        kernel.call("scale_i32_2d", ["Array[int32, 2]", "Array[int32, 2]", "int"], [strided, out, 2],
                    read_back=[1], return_type="None")
    np.testing.assert_array_equal(out, np.zeros_like(c))  # nothing crossed / no write-back


# ---- E4: RED mutant — no write-back leaves the out-buffer unchanged ------------------------

def test_e4_no_writeback_leaves_out_unchanged(kernel):
    # The paired negative control for the out-buffer write-back: if read_back is empty, the
    # WASM still ran and mutated its OWN linear-memory copy, but nothing is copied back into
    # the caller's out-buffer — so it stays zeros. (With read_back=[1] it holds the result,
    # proven by E1.) This is what a "drop the copy-back" mutant would silently do.
    a = np.array([10, 20, 30], dtype=np.int32)
    out = np.zeros_like(a)
    kernel.call("add1_i32", ["Array[int32]", "Array[int32]"], [a, out], read_back=(), return_type="None")
    np.testing.assert_array_equal(out, np.zeros_like(a))  # unchanged — write-back is load-bearing


# ---- scalar return over an array param (reduction, within i64 range) -----------------------

def test_scalar_return_sum_int64(kernel):
    a = np.array([1, 2, 3, 4, 5, -10], dtype=np.int64)
    r = kernel.call("total_i64", ["Array[int64]"], [a], return_type="int")
    assert r.value == int(a.sum())


# ---- admission guards G1 (array iteration) + G2 (sub-64-bit non-ring op) -------------------
# The independent-opus R1 blockers: both are routed to JS (sound-by-refusal), never mis-lowered.

def _exports_of(tmp_path, src: str) -> set[str]:
    from pythscribe.build import find_pyths
    d = tmp_path
    p = d / "g.ps"
    p.write_text(src)
    w = d / "g.wasm"
    r = subprocess.run([str(find_pyths()), "compile", str(p), "--target", "wasm", "-o", str(w)],
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    if not w.is_file():
        return set()  # no WASM-eligible functions
    k = ServerKernel.from_wasm(w, name="g")
    return set(k.exports)


def test_g1_array_iteration_refused(tmp_path):
    # `for x in a` over an array param is NOT lowered yet -> routed to JS (not exported).
    # A range(len(a)) twin over the SAME dtype IS admitted -> the paired control.
    exps = _exports_of(tmp_path,
        "def foriter(a: Array[int64]) -> int:\n"
        "    s = 0\n"
        "    for x in a:\n"
        "        s = s + x\n"
        "    return s\n"
        "def byindex(a: Array[int64]) -> int:\n"
        "    s = 0\n"
        "    for i in range(len(a)):\n"
        "        s = s + a[i]\n"
        "    return s\n")
    assert "foriter" not in exps, "array iteration must route to JS (G1)"
    assert "byindex" in exps, "range(len(a)) form must stay WASM-admitted (paired control)"


def test_g2_sub64_nonring_refused(tmp_path):
    # uint8/int32 non-ring ops (//, %, >>, &, ...) route to JS (G2 wrap-fidelity guard);
    # the +/-/* twin and the int64 // twin stay admitted (paired controls).
    exps = _exports_of(tmp_path,
        "def u8avg(a: Array[uint8], b: Array[uint8], out: Array[uint8]) -> None:\n"
        "    for i in range(len(a)):\n"
        "        out[i] = (a[i] + b[i]) // 2\n"
        "def u8add(a: Array[uint8], out: Array[uint8]) -> None:\n"
        "    for i in range(len(a)):\n"
        "        out[i] = a[i] + 1\n"
        "def i64div(a: Array[int64], out: Array[int64]) -> None:\n"
        "    for i in range(len(a)):\n"
        "        out[i] = a[i] // 2\n")
    assert "u8avg" not in exps, "sub-64-bit non-ring op must route to JS (G2)"
    assert "u8add" in exps, "uint8 +/-/* stays WASM-admitted (paired control)"
    assert "i64div" in exps, "int64 // is exact (i64 wrap == NumPy) — not over-refused"


def test_g1_g2_nested_in_try_and_assert_refused(tmp_path):
    # Independent-opus R2 blocker: the guard walker must descend into Try/Assert bodies
    # (check_body admits them), else a non-ring op or `for x in a` nested in a `try`
    # escapes both guards. Paired control: the safe try-wrapped +1 still admits.
    exps = _exports_of(tmp_path,
        "def try_u8avg(a: Array[uint8], b: Array[uint8], out: Array[uint8]) -> None:\n"
        "    for i in range(len(a)):\n"
        "        try:\n"
        "            out[i] = (a[i] + b[i]) // 2\n"
        "        except ValueError:\n"
        "            out[i] = 0\n"
        "def try_foriter(a: Array[int64]) -> int:\n"
        "    s = 0\n"
        "    try:\n"
        "        for x in a:\n"
        "            s = s + x\n"
        "    except ValueError:\n"
        "        pass\n"
        "    return s\n"
        "def assert_nonring(a: Array[int32], out: Array[int32]) -> None:\n"
        "    for i in range(len(a)):\n"
        "        assert (a[i] * a[i]) // 3 < 100\n"
        "        out[i] = a[i]\n"
        "def safe_ctrl(a: Array[uint8], out: Array[uint8]) -> None:\n"
        "    for i in range(len(a)):\n"
        "        try:\n"
        "            out[i] = a[i] + 1\n"
        "        except ValueError:\n"
        "            out[i] = 0\n")
    assert "try_u8avg" not in exps, "G2 must descend into try bodies"
    assert "try_foriter" not in exps, "G1 must descend into try bodies"
    assert "assert_nonring" not in exps, "G2 must descend into assert tests"
    assert "safe_ctrl" in exps, "safe try-wrapped +1 stays admitted (paired control)"


def test_g2_divergence_is_real(tmp_path):
    # Proof the G2 guard is load-bearing: the refused kernel, computed the way the WASM
    # design WOULD (compute-in-i64, wrap-on-store once), diverges from NumPy uint8.
    a, b = np.uint8(200), np.uint8(100)
    with np.errstate(over="ignore"):
        numpy_result = (a + b) // np.uint8(2)          # NumPy: (200+100 wraps to 44)//2 = 22
    i64_wrap_once = np.uint8((int(a) + int(b)) // 2)   # design-if-admitted: 300//2=150 -> uint8 150
    assert int(numpy_result) == 22 and int(i64_wrap_once) == 150, (int(numpy_result), int(i64_wrap_once))
    assert numpy_result != i64_wrap_once, "the guard prevents a real silent divergence"
