"""pythscribe.runtime.array_buffer -- the SERVER typed-array marshalling channel
(v0.2.5 M2a-2: the 1-D numeric `Array[dtype, ndim]` ABI, server/wasmtime path).

`@wasm` today crosses numeric scalars and `list` params (`pythscribe/ffi/list_buffer.mjs`
+ `pythscribe/runtime/__init__.py`). This module extends the SAME K7 "declared binding"
discipline to fixed-width **typed arrays** accepted through the Python buffer protocol
(`memoryview` / `np.ndarray` / `array.array` -- anything C-contiguous; **NumPy is NOT
imported here**, it is only the test-time oracle). It is the server (in-process wasmtime)
half; the browser typed-array view is M2c and the codegen/`bridge.rs` glue that actually
CALLS a compiled array kernel is M2a-3. This chunk is the marshalling LIBRARY + its total
soundness check, unit-tested in isolation (there is no compiled array kernel yet -- array
params still JS-route until admission flips ON in M2a-3).

DECLARED BINDING -- the typed-array header (DIVERGES from the list header, plan requirements
"Array header DIVERGES"). One-D layout, all offsets in bytes, all i32 little-endian:

    ptr -> [dtype:i32 @0][ndim:i32 @4][shape0:i32 @8][pad:i32 @12][elements @16 ...]

    dtype tag: int32=0 int64=1 float32=2 float64=3 uint8=4  (== ArrayDtype decl order,
               crates/pyths_types/src/types.rs::ArrayDtype -- M2a-3 bridge.rs + M2a-4 Lean
               bind to THESE codes; single-sourced here for the server channel)
    ndim: 1 (present from M2a so M2b only APPENDS shape dims -- requirements B7)
    element width: uint8=1, int32/float32=4, int64/float64=8  (== ArrayDtype::size_bytes)

The header is **16 bytes = a multiple of 8**, so with an 8-aligned base pointer the element
region at ptr+16 is 8-aligned -- MANDATORY (requirements B3, the Chunk 1/2 misalignment
class): a JS `Float64Array`/`BigInt64Array` view over WASM memory THROWS on a non-8-aligned
byte offset, and a naive 12-byte `[dtype][ndim][shape0]` header would land elements at ptr+12
(4-aligned) and throw. `assert_element_region_aligned` pins this per dtype.

THE TOTAL RUNTIME CHECK (requirements B1 -- the soundness gate). The static annotation only
tells the compiler which element WIDTH to bake into the kernel; it CANNOT see a runtime
buffer's real dtype/ndim/contiguity. So before any copy this module inspects the ACTUAL
buffer (`memoryview` format/itemsize/ndim/c_contiguous) and **refuses on ANY mismatch** with
the compiled-for (dtype, ndim) -- wrong dtype, wrong ndim, non-C-contiguous, strided,
Fortran-order, big-endian, or a compound format. On the SERVER path refusal is a **loud
throw** (`ArrayMarshalError`): there is no JS twin here to fall back to (the js+wasm glue's
`RangeError` -> #364-twin reroute is M2a-3). The check is TOTAL -- every non-admitted buffer
shape hits it -- and is itself covered by paired negative controls in the tests
(`test_array_marshalling.py`): remove the check and a wrong-width/strided buffer is packed at
the wrong width/stride = a silent misread the round-trip-vs-NumPy control catches (RED).

"Buffer, not elements": the element region is crossed in ONE bulk copy each way
(`memoryview.tobytes()` in, a bulk `memoryview[:] =` write-back out), never element-by-element.
"""
from __future__ import annotations

import re as _re
import struct
import sys
from typing import Any

__all__ = [
    "ARRAY_LAYOUT_VERSION",
    "ARRAY_HEADER_BYTES",
    "ELEMENT_REGION_OFFSET",
    "DTYPE_TAG",
    "DTYPE_ITEMSIZE",
    "DTYPES",
    "array_param_spec",
    "ArrayMarshalError",
    "check_array_buffer",
    "pack_array_buffer",
    "unpack_array_into",
    "read_back_element_bytes",
    "array_alloc_size",
    "array_header_bytes",
    "assert_element_region_aligned",
    "element_offset_aligned",
]

# --- the declared binding (K7, array variant) --------------------------------------------
# A NEW version string: the array header DIVERGES from the list layout (list stays
# "pyths-0.2.4-list-v1"), so it carries its own version, bumped when the array layout changes.
# M2b bump v1 -> v2: the reserved `pad` slot @12 is now `shape1` (columns) for a 2-D
# array; the 1-D bytes are byte-identical (shape1 = 0 == the old pad), but the slot's
# SEMANTICS extend, so the version is bumped and byte-agreement re-pinned across the
# three reps (this module, the js glue, the Lean marshallingTable).
ARRAY_LAYOUT_VERSION = "pyths-0.2.5-array-v2"

# dtype tag @0, ndim @4, shape0 (rows) @8, shape1 (cols; 0 for 1-D) @12 -> elements @16
# (a multiple of 8: see B3 above).
ARRAY_HEADER_BYTES = 16
ELEMENT_REGION_OFFSET = 16

# dtype tag codes == ArrayDtype declaration order (crates/pyths_types/src/types.rs). M2a-3
# (bridge.rs) and M2a-4 (Lean marshallingTable) MUST bind to these exact codes.
DTYPE_TAG = {"int32": 0, "int64": 1, "float32": 2, "float64": 3, "uint8": 4}
DTYPES = tuple(DTYPE_TAG)  # ("int32","int64","float32","float64","uint8")

# element width in bytes == ArrayDtype::size_bytes.
DTYPE_ITEMSIZE = {"int32": 4, "int64": 8, "float32": 4, "float64": 8, "uint8": 1}

# --- the ONE `Array[dtype, ndim]` annotation normalization (N3) ----------------------------
# single-sourced here in the leaf module both channels already import, so the server runtime
# (`runtime.array_param_spec`), the FFI grammar (`ffi.array_param_spec`) and the browser glue
# (`list_buffer.mjs::arrayParamSpec`) can never drift apart. Any spelling with ndim ∉ {1, 2},
# an unknown dtype, or a malformed bracket → None, so the caller refuses it loudly.
_ARRAY_ANN = _re.compile(r"^\s*Array\[\s*([A-Za-z0-9_]+)\s*(?:,\s*([0-9]+)\s*)?\]\s*$")


def array_param_spec(ann: "str | None" = None, *, param_type: "str | None" = None) -> "tuple[str, int] | None":
    """(dtype, ndim) for an admitted `Array[dtype]` / `Array[dtype, 1|2]` annotation, else None.

    `ann` is the annotation text. `param_type=` is accepted as a legacy keyword alias so the
    pre-N3 `runtime.array_param_spec(param_type=...)` call site keeps working (back-compat);
    passing BOTH is a caller error."""
    if ann is not None and param_type is not None:
        raise TypeError("array_param_spec: pass either `ann` or the legacy `param_type=`, not both")
    if ann is None:
        ann = param_type
    m = _ARRAY_ANN.match(ann or "")
    if not m:
        return None
    dtype, ndim = m.group(1), m.group(2)
    if dtype not in DTYPE_TAG:
        return None
    n = 1 if ndim is None else int(ndim)
    return (dtype, n) if n in (1, 2) else None

# the buffer-protocol shape each dtype demands: (kind, signed, itemsize). `signed` is None
# for floats. The runtime check compares the ACTUAL buffer's (kind, signed, itemsize) against
# this -- itemsize comes from the buffer protocol (authoritative across platforms), so the
# platform-dependent int format chars ('l' is 4 B on Windows, 8 B on LP64 Linux) never fool it.
_DTYPE_SHAPE = {
    "int32": ("int", True, 4),
    "int64": ("int", True, 8),
    "float32": ("float", None, 4),
    "float64": ("float", None, 8),
    "uint8": ("int", False, 1),
}

# struct format char per dtype -- used ONLY by the tests / for documentation of the element
# encoding; the bulk copy uses raw bytes, never per-element struct calls.
DTYPE_STRUCT_CHAR = {"int32": "i", "int64": "q", "float32": "f", "float64": "d", "uint8": "B"}

# map a single buffer-protocol format char -> (kind, signed). `None` signed = float.
# platform-width chars ('l','L','n','N') are accepted for KIND/sign; the actual width is read
# from memoryview.itemsize, never inferred from the char.
_FMT_KIND = {
    "b": ("int", True), "B": ("int", False),
    "h": ("int", True), "H": ("int", False),
    "i": ("int", True), "I": ("int", False),
    "l": ("int", True), "L": ("int", False),
    "q": ("int", True), "Q": ("int", False),
    "n": ("int", True), "N": ("int", False),
    "e": ("float", None), "f": ("float", None), "d": ("float", None),
}
_BYTE_ORDER_PREFIXES = "<>=@!"


class ArrayMarshalError(RuntimeError):
    """A runtime buffer does not match the kernel's compiled-for (dtype, ndim) -- wrong dtype,
    wrong ndim, non-C-contiguous / strided / Fortran-order, big-endian, or a compound format.
    On the server path this is thrown loudly (there is no JS twin to reroute to). It is the
    total soundness gate (requirements B1): every non-admitted buffer shape raises it, so no
    strided / wrong-width buffer is ever silently misread."""


def _require_dtype(dtype: str) -> None:
    if dtype not in DTYPE_TAG:
        raise ArrayMarshalError(
            f"unknown array dtype {dtype!r}; admitted dtypes are {', '.join(DTYPES)}"
        )


def _require_ndim(ndim: int) -> None:
    # M2b: 1-D and 2-D C-contiguous arrays cross. ndim>2 / strided / F-order is
    # refused (loud, never a silent strided read).
    if ndim not in (1, 2):
        raise ArrayMarshalError(
            f"server array marshalling supports ndim in {{1, 2}} (C-contiguous only), got ndim={ndim}"
        )


def _actual_shape(mv: memoryview) -> tuple[str, Any, int]:
    """(kind, signed, itemsize) of the buffer's element type, or raise on a shape the WASM
    bulk copy cannot honour (big-endian bytes, a compound/struct format, an unknown char)."""
    fmt = mv.format or "B"
    order = ""
    if fmt and fmt[0] in _BYTE_ORDER_PREFIXES:
        order, fmt = fmt[0], fmt[1:]
    if len(fmt) != 1 or fmt not in _FMT_KIND:
        raise ArrayMarshalError(
            f"buffer format {mv.format!r} is not a single admitted numeric element type "
            f"(compound/struct formats are refused)"
        )
    # WASM linear memory is little-endian; a '<' buffer is LE bytes on any host (fine to bulk
    # copy), a '>' buffer is big-endian (refuse), a native ('=','@', or no prefix) buffer is
    # only LE on a little-endian host.
    if order == ">":
        raise ArrayMarshalError("big-endian buffer refused (WASM linear memory is little-endian)")
    if order in ("", "=", "@", "!") and sys.byteorder != "little":
        raise ArrayMarshalError(
            f"native byte order on a {sys.byteorder}-endian host refused (WASM memory is little-endian)"
        )
    if order == "!":  # network/big-endian alias
        raise ArrayMarshalError("network (big-endian) byte order refused (WASM memory is little-endian)")
    kind, signed = _FMT_KIND[fmt]
    return kind, signed, mv.itemsize


def check_array_buffer(buf: Any, dtype: str, ndim: int) -> memoryview:
    """THE TOTAL runtime marshaller check (requirements B1). Return a validated, C-contiguous,
    native-little-endian `memoryview` of `buf` whose element (kind, signed, itemsize) and ndim
    EXACTLY match the compiled-for (`dtype`, `ndim`), or raise `ArrayMarshalError`. This is the
    ONLY gate; every caller marshals through it, so a mismatch can never be silently misread.
    No NumPy import: any buffer-protocol object is accepted."""
    _require_dtype(dtype)
    _require_ndim(ndim)
    try:
        mv = memoryview(buf)
    except (TypeError, ValueError, BufferError) as e:
        # TypeError: not a buffer at all. ValueError/BufferError: a buffer memoryview cannot
        # represent (e.g. a datetime64 ndarray: "cannot include dtype 'M' in a buffer") -- still
        # refused through THIS gate as an ArrayMarshalError, never escaping as a raw ValueError.
        raise ArrayMarshalError(f"object does not support the buffer protocol: {e}") from e
    if mv.ndim != ndim:
        raise ArrayMarshalError(
            f"buffer ndim {mv.ndim} != compiled-for ndim {ndim} for Array[{dtype}] (refused, never reshaped)"
        )
    if not mv.c_contiguous:
        raise ArrayMarshalError(
            f"buffer is not C-contiguous (strided / Fortran-order) -- refused, never read at the wrong stride "
            f"(copy to a C-contiguous array before crossing)"
        )
    kind, signed, itemsize = _actual_shape(mv)
    want_kind, want_signed, want_size = _DTYPE_SHAPE[dtype]
    if (kind, signed, itemsize) != (want_kind, want_signed, want_size):
        got = f"{'unsigned ' if signed is False else 'signed ' if signed else ''}{kind}{itemsize * 8}"
        raise ArrayMarshalError(
            f"buffer dtype {got} (format {mv.format!r}, itemsize {itemsize}) != compiled-for {dtype} -- refused "
            f"(a wrong-width read would silently corrupt values)"
        )
    return mv


def array_header_bytes(dtype: str, ndim: int, shape0: int, shape1: int = 0) -> bytes:
    """The 16-byte ×8 typed-array header: [dtype:i32][ndim:i32][shape0:i32][shape1:i32].
    `shape1` is 0 for a 1-D array (the reserved slot) and the column count for a 2-D
    array (M2b). shape0 is the row count (1-D: the element count)."""
    _require_dtype(dtype)
    _require_ndim(ndim)
    if shape0 < 0 or shape1 < 0:
        raise ArrayMarshalError(f"negative shape ({shape0}, {shape1})")
    if ndim == 1 and shape1 != 0:
        raise ArrayMarshalError(f"1-D array header must have shape1 = 0, got {shape1}")
    hdr = struct.pack("<iiii", DTYPE_TAG[dtype], ndim, shape0, shape1)
    assert len(hdr) == ARRAY_HEADER_BYTES  # the ×8 header invariant, by construction
    return hdr


def array_alloc_size(n: int, dtype: str) -> int:
    """Total bytes to request from `__alloc` for an `n`-element `dtype` array: header +
    elements, rounded UP to a multiple of 8 so the NEXT allocation stays 8-aligned (the list
    channel's `+7 & ~7` rule, applied to the array header)."""
    _require_dtype(dtype)
    raw = ARRAY_HEADER_BYTES + n * DTYPE_ITEMSIZE[dtype]
    return (raw + 7) & ~7


def pack_array_buffer(buf: Any, dtype: str, ndim: int) -> tuple[bytes, dict]:
    """Validate `buf` (the total check) and produce the linear-memory payload for it:
    the 16-byte header followed by the element region, bulk-copied from the buffer's own
    bytes, padded so the total length is a multiple of 8. Returns (payload, meta) with
    meta = {n, elem_offset, elem_bytes, alloc_size}. ONE bulk copy, never per-element."""
    mv = check_array_buffer(buf, dtype, ndim)
    if ndim == 1:
        shape0, shape1 = mv.shape[0], 0
    else:  # ndim == 2 (C-contiguous, validated) — row-major [rows, cols]
        shape0, shape1 = mv.shape[0], mv.shape[1]
    n = shape0 if ndim == 1 else shape0 * shape1
    elem = mv.tobytes()  # C-contiguous, native-LE == WASM element bytes: the single bulk copy in
    assert len(elem) == n * DTYPE_ITEMSIZE[dtype]
    payload = bytearray(array_header_bytes(dtype, ndim, shape0, shape1))
    payload += elem
    while len(payload) % 8 != 0:
        payload += b"\x00"
    meta = {
        "n": n,
        "shape0": shape0,
        "shape1": shape1,
        "elem_offset": ELEMENT_REGION_OFFSET,
        "elem_bytes": len(elem),
        "alloc_size": len(payload),
    }
    return bytes(payload), meta


def read_back_element_bytes(payload: bytes, dtype: str, ndim: int, n: int) -> bytes:
    """Slice the element region (skip the 16-byte header) out of a full linear-memory payload.
    The bytes M2a-3 reads back from WASM memory after a call, before writing them into the
    caller's out-buffer.

    The payload's OWN header is parsed and VALIDATED against the expected (`dtype`, `ndim`,
    `n`) before the element region is trusted (review r1/B1): a payload whose header tag,
    ndim, or shape disagrees with what the caller compiled for is a drift / corruption and is
    REFUSED, never read at the wrong width or truncated silently. So the read-back is
    self-checking, not a blind slice."""
    _require_dtype(dtype)
    _require_ndim(ndim)
    if len(payload) < ARRAY_HEADER_BYTES:
        raise ArrayMarshalError(f"payload too short for a header: {len(payload)} B < {ARRAY_HEADER_BYTES} B")
    tag, hdr_ndim, shape0, shape1 = struct.unpack("<iiii", payload[:ARRAY_HEADER_BYTES])
    if tag != DTYPE_TAG[dtype]:
        got = next((name for name, t in DTYPE_TAG.items() if t == tag), f"tag {tag}")
        raise ArrayMarshalError(
            f"payload header dtype {got} != expected {dtype}: refused (a wrong-width read would corrupt values)"
        )
    if hdr_ndim != ndim:
        raise ArrayMarshalError(f"payload header ndim {hdr_ndim} != expected {ndim}: refused")
    # The header's element count (shape0 for 1-D, shape0*shape1 for 2-D) must
    # match the caller's expected `n` — no silent truncation / wrong-shape read.
    hdr_n = shape0 if ndim == 1 else shape0 * shape1
    if ndim == 1 and shape1 != 0:
        raise ArrayMarshalError(f"payload header 1-D shape1 {shape1} != 0: refused")
    if hdr_n != n:
        raise ArrayMarshalError(
            f"payload header shape ({shape0}, {shape1}) = {hdr_n} elements != expected {n}: "
            f"refused (no silent truncation)"
        )
    end = ELEMENT_REGION_OFFSET + n * DTYPE_ITEMSIZE[dtype]
    if len(payload) < end:
        raise ArrayMarshalError(
            f"payload too short: {len(payload)} B < header+{n}×{DTYPE_ITEMSIZE[dtype]} = {end} B"
        )
    return payload[ELEMENT_REGION_OFFSET:end]


def unpack_array_into(element_bytes: bytes, out_buf: Any, dtype: str, ndim: int) -> None:
    """Bulk-write the element bytes read back from WASM memory INTO the caller's writable
    out-buffer, in place (the symmetric write-back, extended to typed arrays). `out_buf` is
    re-validated by the total check and must be writable; the byte length must match. ONE bulk
    copy out, never per-element."""
    dst = check_array_buffer(out_buf, dtype, ndim)
    if dst.readonly:
        raise ArrayMarshalError("out-buffer is read-only; the write-back needs a writable buffer")
    if len(element_bytes) != dst.nbytes:
        raise ArrayMarshalError(
            f"write-back size mismatch: {len(element_bytes)} B != out-buffer {dst.nbytes} B"
        )
    dst.cast("B")[:] = element_bytes  # bulk write-back


def element_offset_aligned(dtype: str, element_offset: int) -> bool:
    """Is `element_offset` a valid element-region offset for `dtype`? A JS TypedArray view
    over WASM memory throws when the byte offset is not a multiple of the view's element width
    (`BYTES_PER_ELEMENT`): 8 for `Float64Array`/`BigInt64Array`, 4 for `Int32Array`/
    `Float32Array`, 1 for `Uint8Array`. So the requirement is width-alignment, dtype by dtype.
    The ×8 header (offset 16) is the UNIFORM choice that satisfies the WORST case (i64/f64 = 8);
    a naive 12-byte header would put elements at +12, which is fine for uint8/i32/f32 but
    MIS-aligns i64/f64 (12 % 8 = 4) so their views would throw -- which is exactly why the
    header is padded to ×8 (requirements B3)."""
    _require_dtype(dtype)
    return element_offset % DTYPE_ITEMSIZE[dtype] == 0


def assert_element_region_aligned(dtype: str, ndim: int = 1, base_ptr: int = 0) -> None:
    """Assert the element region is aligned to the dtype's element width, for an 8-aligned base
    pointer (the allocator's guarantee). The uniform 16-byte ×8 header means the region lands at
    a multiple of 8, so EVERY dtype's width (1/4/8) divides it -- the B3 invariant. `base_ptr`
    defaults to 0 (a fresh 8-aligned allocation)."""
    _require_dtype(dtype)
    _require_ndim(ndim)
    assert base_ptr % 8 == 0, f"base_ptr {base_ptr} is not 8-aligned (the allocator hands back 8-aligned pointers)"
    off = base_ptr + ELEMENT_REGION_OFFSET
    assert element_offset_aligned(dtype, off), (
        f"Array[{dtype}] element region at +{off} is not aligned to its {DTYPE_ITEMSIZE[dtype]}-byte width"
    )
