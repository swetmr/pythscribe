"""M2a-2 gates: the SERVER typed-array marshalling library (`pythscribe.runtime.array_buffer`),
unit-tested IN ISOLATION -- there is no compiled array kernel yet (admission flips ON in M2a-3;
arrays still JS-route). NumPy is the ARRAY-SEMANTICS oracle (test-only; the runtime never
imports it). Every property ships a positive SPOT/PBT vs NumPy AND a paired NEGATIVE CONTROL
that goes RED (feedback_anti_vacuity_paired_control).

  AM1  the ×8 typed-array header layout (declared binding): byte positions, dtype tags ==
       ArrayDtype decl order, element region 8-aligned per dtype (B3)
  AM2  pack -> read-back round-trip == NumPy bit-for-bit across all 5 dtypes (+ PBT)
  AM3  the out-buffer write-back fills the caller's array in place, bulk, across dtypes
  AM4  the TOTAL runtime check refuses EVERY mismatch (wrong dtype/ndim, strided, F-order,
       big-endian, compound, read-only) -- server THROWS, never a silent misread
  AM5  the three required RED mutants: runtime-check-removed, no-write-back, wrong-dtype-width
"""
from __future__ import annotations

import struct

import pytest

from conftest import gate_import

numpy = gate_import("numpy")  # the array oracle; under REQUIRE_ORACLE a missing NumPy FAILS
import numpy as np  # noqa: E402

from pythscribe.runtime.array_buffer import (  # noqa: E402
    ARRAY_HEADER_BYTES,
    ARRAY_LAYOUT_VERSION,
    DTYPE_ITEMSIZE,
    DTYPE_TAG,
    DTYPES,
    ELEMENT_REGION_OFFSET,
    ArrayMarshalError,
    array_alloc_size,
    array_header_bytes,
    assert_element_region_aligned,
    check_array_buffer,
    element_offset_aligned,
    pack_array_buffer,
    read_back_element_bytes,
    unpack_array_into,
)

NP = {"int32": np.int32, "int64": np.int64, "float32": np.float32, "float64": np.float64, "uint8": np.uint8}

# a representative reference buffer per dtype (includes wrap-boundary + fractional values;
# marshalling is a pure copy, so it is bit-exact even for f32 here -- the tolerance question is
# a COMPUTE question deferred to M2a-3, not a marshalling one)
def _ref(dtype: str) -> "np.ndarray":
    if dtype == "int32":
        return np.array([0, 1, -1, 2147483647, -2147483648, 42], dtype=np.int32)
    if dtype == "int64":
        return np.array([0, 1, -1, 9223372036854775807, -9223372036854775808, 7], dtype=np.int64)
    if dtype == "float32":
        return np.array([0.0, 1.5, -2.25, np.float32(3.1415927), 1e30, -1e-30], dtype=np.float32)
    if dtype == "float64":
        return np.array([0.0, 1.5, -2.25, 3.141592653589793, 1e300, -1e-300], dtype=np.float64)
    if dtype == "uint8":
        return np.array([0, 1, 254, 255, 128, 42], dtype=np.uint8)
    raise AssertionError(dtype)


# ---------------------------------------------------------------------- AM1 layout / binding
def test_am1_header_is_x8_and_layout_version_bumped():
    assert ARRAY_HEADER_BYTES == 16 and ARRAY_HEADER_BYTES % 8 == 0
    assert ELEMENT_REGION_OFFSET == 16
    assert ARRAY_LAYOUT_VERSION == "pyths-0.2.5-array-v2"  # M2b: pad@12 → shape1 (DIVERGES from list)
    assert ARRAY_LAYOUT_VERSION != "pyths-0.2.4-list-v1"


def test_am1_dtype_tags_match_arraydtype_decl_order():
    # crates/pyths_types/src/types.rs::ArrayDtype declaration order (M2a-3/M2a-4 bind to these)
    assert DTYPE_TAG == {"int32": 0, "int64": 1, "float32": 2, "float64": 3, "uint8": 4}
    assert DTYPE_ITEMSIZE == {"int32": 4, "int64": 8, "float32": 4, "float64": 8, "uint8": 1}


@pytest.mark.parametrize("dtype", DTYPES)
def test_am1_header_byte_positions(dtype):
    hdr = array_header_bytes(dtype, 1, 13)
    assert len(hdr) == 16
    tag, ndim, shape0, pad = struct.unpack("<iiii", hdr)
    assert tag == DTYPE_TAG[dtype] and ndim == 1 and shape0 == 13 and pad == 0


@pytest.mark.parametrize("dtype", DTYPES)
def test_am1_element_region_width_aligned_per_dtype(dtype):
    # B3: a JS TypedArray view throws when the byte offset is not a multiple of its element
    # width (8 for Float64Array/BigInt64Array, 4 for Int32/Float32Array, 1 for Uint8Array).
    # The real 16-byte header lands elements at +16 -> width-aligned for EVERY dtype.
    assert_element_region_aligned(dtype, 1, base_ptr=0)
    assert_element_region_aligned(dtype, 1, base_ptr=8)
    assert element_offset_aligned(dtype, ELEMENT_REGION_OFFSET) is True


def test_am1_naive_12byte_header_would_misalign_i64_f64():
    # the FAITHFUL negative control (not a precondition strawman): a naive 12-byte
    # [dtype][ndim][shape0] header would put elements at +12. That is fine for the 1/4-byte
    # dtypes but MIS-aligns the 8-byte ones -> their JS views would throw. This is exactly why
    # the header is padded to 16 (×8). Model the failure directly at the offset.
    for dt in ("uint8", "int32", "float32"):
        assert element_offset_aligned(dt, 12) is True  # width 1/4 divides 12
    for dt in ("int64", "float64"):
        assert element_offset_aligned(dt, 12) is False  # width 8 does NOT divide 12 -> RED
        assert element_offset_aligned(dt, ELEMENT_REGION_OFFSET) is True  # the real ×8 header is fine


@pytest.mark.parametrize("dtype", DTYPES)
def test_am1_alloc_size_rounds_to_8(dtype):
    for n in range(0, 17):
        size = array_alloc_size(n, dtype)
        assert size % 8 == 0
        assert size >= ARRAY_HEADER_BYTES + n * DTYPE_ITEMSIZE[dtype]


# ------------------------------------------------------ AM2 pack round-trip == NumPy (SPOT)
@pytest.mark.parametrize("dtype", DTYPES)
def test_am2_pack_roundtrip_equals_numpy_bit_for_bit(dtype):
    a = _ref(dtype)
    payload, meta = pack_array_buffer(a, dtype, 1)
    assert meta["n"] == a.shape[0]
    assert meta["elem_offset"] == 16
    # header is correct
    tag, ndim, shape0, _ = struct.unpack("<iiii", payload[:16])
    assert (tag, ndim, shape0) == (DTYPE_TAG[dtype], 1, a.shape[0])
    # BIT-FOR-BIT vs NumPy: the element region == np.asarray(a).tobytes()
    elem = read_back_element_bytes(payload, dtype, 1, meta["n"])
    assert elem == np.asarray(a).tobytes()
    back = np.frombuffer(elem, dtype=NP[dtype])
    assert back.tobytes() == a.tobytes()  # bit-for-bit incl. any NaN/inf payloads


@pytest.mark.parametrize("dtype", DTYPES)
def test_am2_pack_roundtrip_2d_equals_numpy_bit_for_bit(dtype):
    # M2b: a 2-D (non-square) C-contiguous buffer packs + reads back bit-for-bit
    # vs NumPy, row-major. The header carries shape0 (rows) @8 AND shape1 (cols)
    # @12; the element region is `np.asarray(a).tobytes()` (row-major C-order).
    base = _ref(dtype)
    rows, cols = 2, 3
    a = np.ascontiguousarray(base[: rows * cols].reshape(rows, cols))
    payload, meta = pack_array_buffer(a, dtype, 2)
    assert meta["n"] == rows * cols and meta["shape0"] == rows and meta["shape1"] == cols
    tag, ndim, shape0, shape1 = struct.unpack("<iiii", payload[:16])
    assert (tag, ndim, shape0, shape1) == (DTYPE_TAG[dtype], 2, rows, cols)
    elem = read_back_element_bytes(payload, dtype, 2, meta["n"])
    assert elem == np.asarray(a).tobytes()  # bit-for-bit, row-major
    back = np.frombuffer(elem, dtype=NP[dtype]).reshape(rows, cols)
    assert back.tobytes() == a.tobytes()
    # write-back into a fresh C-contiguous out-buffer round-trips exactly.
    out = np.zeros((rows, cols), dtype=NP[dtype])
    unpack_array_into(elem, out, dtype, 2)
    assert out.tobytes() == a.tobytes()


def test_am2_read_back_2d_validates_shape():
    # M2b: the 2-D read-back is self-checking on BOTH shape dims (shape0*shape1).
    a = np.ascontiguousarray(_ref("int32")[:6].reshape(2, 3))
    payload, meta = pack_array_buffer(a, "int32", 2)
    assert read_back_element_bytes(payload, "int32", 2, 6) == a.tobytes()  # matched
    with pytest.raises(ArrayMarshalError):  # wrong ndim vs the header
        read_back_element_bytes(payload, "int32", 1, 6)
    with pytest.raises(ArrayMarshalError):  # wrong element count (shape) vs the header
        read_back_element_bytes(payload, "int32", 2, 5)


def test_am2_read_back_validates_the_payload_header():
    # read_back is self-checking: a payload whose header disagrees with the caller's expected
    # (dtype, ndim, n) is REFUSED, never read at the wrong width or truncated (review r1/B1).
    payload, meta = pack_array_buffer(_ref("int32"), "int32", 1)
    n = meta["n"]
    assert read_back_element_bytes(payload, "int32", 1, n) == _ref("int32").tobytes()  # matched: ok
    with pytest.raises(ArrayMarshalError):  # wrong dtype vs the header tag
        read_back_element_bytes(payload, "float32", 1, n)
    with pytest.raises(ArrayMarshalError):  # wrong element count vs the header shape
        read_back_element_bytes(payload, "int32", 1, n + 1)
    # a forged header (tag says float64) is caught, not trusted
    forged = bytearray(payload)
    forged[0:4] = struct.pack("<i", DTYPE_TAG["float64"])
    with pytest.raises(ArrayMarshalError):
        read_back_element_bytes(bytes(forged), "int32", 1, n)


@pytest.mark.parametrize("dtype", DTYPES)
def test_am2_empty_array_roundtrips(dtype):
    a = np.array([], dtype=NP[dtype])
    payload, meta = pack_array_buffer(a, dtype, 1)
    assert meta["n"] == 0
    assert read_back_element_bytes(payload, dtype, 1, 0) == b""
    assert len(payload) % 8 == 0


# ------------------------------------------------------ AM2 PBT vs NumPy over sizes + values
from hypothesis import given, settings  # noqa: E402
from hypothesis import strategies as st  # noqa: E402
from hypothesis.extra import numpy as npst  # noqa: E402


@pytest.mark.parametrize("dtype", DTYPES)
@settings(max_examples=60, deadline=None)
@given(data=st.data())
def test_am2_pbt_pack_roundtrip_matches_numpy(dtype, data):
    n = data.draw(st.integers(min_value=0, max_value=64))
    arr = data.draw(npst.arrays(dtype=np.dtype(NP[dtype]), shape=n))
    payload, meta = pack_array_buffer(arr, dtype, 1)
    elem = read_back_element_bytes(payload, dtype, 1, meta["n"])
    assert elem == arr.tobytes()  # bit-for-bit vs the NumPy oracle across generated inputs


# ------------------------------------------------------------- AM3 out-buffer write-back
@pytest.mark.parametrize("dtype", DTYPES)
def test_am3_write_back_fills_caller_buffer_in_place(dtype):
    src = _ref(dtype)
    n = src.shape[0]
    # the bytes M2a-3 would read back out of WASM memory
    elem = src.tobytes()
    out = np.zeros(n, dtype=NP[dtype])
    unpack_array_into(elem, out, dtype, 1)
    assert out.tobytes() == src.tobytes()  # caller's array now holds the result, bulk-copied


def test_am3_write_back_rejects_readonly_and_size_mismatch():
    a = np.arange(4, dtype=np.int32)
    # a genuinely read-only buffer of the CORRECT dtype: passes dtype/ndim/contiguity, so it
    # reaches (and is refused by) the readonly branch of unpack_array_into
    ro = np.arange(4, dtype=np.int32)
    ro.setflags(write=False)
    assert memoryview(ro).readonly
    with pytest.raises(ArrayMarshalError):  # read-only buffer cannot receive a write-back
        unpack_array_into(a.tobytes(), ro, "int32", 1)
    out = np.zeros(4, dtype=np.int32)
    with pytest.raises(ArrayMarshalError):  # wrong byte length
        unpack_array_into(a[:3].tobytes(), out, "int32", 1)


# ----------------------------------------------------------- AM4 the TOTAL check refuses
def test_am4_matched_buffer_is_admitted():
    for dtype in DTYPES:
        mv = check_array_buffer(_ref(dtype), dtype, 1)
        assert mv.c_contiguous and mv.ndim == 1


@pytest.mark.parametrize("dtype", DTYPES)
def test_am4_wrong_dtype_refused(dtype):
    # give a buffer of every OTHER dtype to a `dtype` kernel -> the total check throws
    for other in DTYPES:
        if other == dtype:
            continue
        buf = _ref(other)
        # skip the (uint8 vs int8) look-alikes that share width but differ in sign/kind: all
        # 5 admitted dtypes are mutually distinguishable by (kind, signed, itemsize)
        with pytest.raises(ArrayMarshalError):
            check_array_buffer(buf, dtype, 1)


def test_am4_int8_vs_uint8_distinguished_by_sign():
    # same itemsize (1), different signedness -> must be refused, not silently accepted
    signed8 = np.array([-1, 0, 1], dtype=np.int8)
    with pytest.raises(ArrayMarshalError):
        check_array_buffer(signed8, "uint8", 1)


def test_am4_wrong_ndim_refused():
    a2d = np.zeros((3, 4), dtype=np.int32)
    # a 2-D buffer to a 1-D-compiled kernel is refused (ndim mismatch).
    with pytest.raises(ArrayMarshalError):
        check_array_buffer(a2d, "int32", 1)
    # M2b: a 2-D C-contiguous buffer to a 2-D-compiled kernel is now ADMITTED.
    mv = check_array_buffer(a2d, "int32", 2)
    assert mv.ndim == 2 and mv.shape == (3, 4) and mv.c_contiguous
    # a 1-D buffer to a 2-D-compiled kernel is refused (ndim mismatch).
    a1d = np.zeros(12, dtype=np.int32)
    with pytest.raises(ArrayMarshalError):
        check_array_buffer(a1d, "int32", 2)
    # ndim>2 (3-D) is refused outright (the compiled-for ndim gate).
    a3d = np.zeros((2, 3, 4), dtype=np.int32)
    with pytest.raises(ArrayMarshalError):
        check_array_buffer(a3d, "int32", 3)
    # a genuine 3-D buffer to a 2-D-compiled kernel is refused (ndim mismatch).
    with pytest.raises(ArrayMarshalError):
        check_array_buffer(a3d, "int32", 2)


def test_am4_strided_noncontiguous_refused():
    # the genuine 1-D contiguity control: a strided slice is non-C-contiguous and is REFUSED.
    # (On the server the pack uses memoryview.tobytes(), which would re-order a strided view to
    # dense C-order; refusal enforces the contiguous-only CONTRACT so every path -- incl. the
    # browser raw-view path (M2c) and the WASM flat copy -- agrees, never a silent stride misread.)
    base = np.arange(20, dtype=np.int32)
    strided = base[::2]  # non-C-contiguous
    assert not memoryview(strided).c_contiguous
    with pytest.raises(ArrayMarshalError):
        check_array_buffer(strided, "int32", 1)


def test_am4_2d_forder_refused_c_admitted():
    # M2b: the true F-order-vs-C discrimination (a genuine 2-D property). A 2-D
    # C-contiguous buffer to a 2-D kernel is ADMITTED; the SAME data in Fortran
    # order is NON-C-contiguous and REFUSED (never a silent transposed/strided
    # read). This is the paired control for 2-D contiguity.
    c2d = np.arange(12, dtype=np.float64).reshape(3, 4)
    f2d = np.asfortranarray(c2d)
    assert not memoryview(f2d).c_contiguous  # F-order 2-D is not C-contiguous
    mv = check_array_buffer(c2d, "float64", 2)  # C-order 2-D: admitted
    assert mv.c_contiguous and mv.shape == (3, 4)
    with pytest.raises(ArrayMarshalError):
        check_array_buffer(f2d, "float64", 2)  # F-order 2-D: refused
    # A strided (non-contiguous) 2-D slice is likewise refused.
    strided2d = np.arange(24, dtype=np.float64).reshape(4, 6)[:, ::2]
    assert not memoryview(strided2d).c_contiguous
    with pytest.raises(ArrayMarshalError):
        check_array_buffer(strided2d, "float64", 2)


def test_am4_big_endian_refused():
    be = np.arange(4, dtype=">i4")  # big-endian int32; WASM memory is little-endian
    with pytest.raises(ArrayMarshalError):
        check_array_buffer(be, "int32", 1)


def test_am4_compound_format_refused():
    rec = np.zeros(3, dtype=[("a", np.int32), ("b", np.int32)])
    with pytest.raises(ArrayMarshalError):
        check_array_buffer(rec, "int32", 1)


def test_am4_unknown_dtype_or_ndim_refused():
    with pytest.raises(ArrayMarshalError):
        check_array_buffer(np.arange(4, dtype=np.int32), "int16", 1)
    with pytest.raises(ArrayMarshalError):
        check_array_buffer(np.arange(4, dtype=np.int32), "int32", 3)


# ---------------------------------------------------------------- AM5 the RED mutants
def test_am5_mutant_runtime_check_removed_goes_red(monkeypatch):
    """RED: neutralize ONLY the total check and run the REAL pack path. A same-width, wrong-KIND
    buffer (int32 bits given to an Array[float32] kernel -- both 4 B, so the residual byte-length
    assert still passes) flows through and is reinterpreted at the compiled dtype -> a silent
    misread. The total check is the ONLY thing that would have stopped it (positive control:
    with the check in place, the kind mismatch is refused)."""
    import pythscribe.runtime.array_buffer as ab

    buf = np.array([1, 2, 3, 4], dtype=np.int32)  # to an Array[float32] kernel; both 4 bytes wide
    # positive control: the real total check REFUSES the kind mismatch (no silent misread)
    with pytest.raises(ArrayMarshalError):
        pack_array_buffer(buf, "float32", 1)
    # mutant: replace the total check with a passthrough; the rest of pack runs UNCHANGED
    monkeypatch.setattr(ab, "check_array_buffer", lambda b, dtype, ndim: memoryview(b))
    payload, meta = ab.pack_array_buffer(buf, "float32", 1)  # now succeeds (widths match)
    elem = read_back_element_bytes(payload, "float32", 1, meta["n"])
    misread = np.frombuffer(elem, dtype=np.float32)
    # int32 bit patterns 1,2,3,4 read as float32 are tiny denormals, NOT [1.0,2.0,3.0,4.0]
    assert not np.array_equal(misread, buf.astype(np.float32))  # RED: silent misread


def test_am5_mutant_no_write_back_goes_red():
    """RED: drop the write-back and the caller's out-buffer stays unchanged."""
    src = np.array([10, 20, 30, 40], dtype=np.int64)
    out = np.zeros(4, dtype=np.int64)
    before = out.copy()
    # (mutant: the unpack_array_into call is intentionally NOT made)
    assert np.array_equal(out, before)  # RED: out is still zeros, != src
    assert not np.array_equal(out, src)
    # positive control: WITH the write-back the out-buffer holds the result
    unpack_array_into(src.tobytes(), out, "int64", 1)
    assert np.array_equal(out, src)


def test_am5_mutant_wrong_dtype_width_goes_red():
    """RED: pack a float32 buffer but read it back as float64 (wrong width) -> garbled."""
    a = np.array([1.5, 2.5, 3.5, 4.5], dtype=np.float32)
    # positive control: correct-width round-trip is bit-exact
    payload, meta = pack_array_buffer(a, "float32", 1)
    good = read_back_element_bytes(payload, "float32", 1, meta["n"])
    assert np.frombuffer(good, dtype=np.float32).tobytes() == a.tobytes()
    # mutant: interpret the 16 f32 bytes as f64 (a kernel compiled for float64 reading f32 bytes)
    mutant = np.frombuffer(a.tobytes(), dtype=np.float64)  # 2 nonsense elements from 4 f32
    assert not np.allclose(mutant, a[: mutant.shape[0]].astype(np.float64))  # RED

    # and the real total check would REFUSE an actual f32 buffer at a float64 kernel:
    with pytest.raises(ArrayMarshalError):
        check_array_buffer(a, "float64", 1)
