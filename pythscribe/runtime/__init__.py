"""pythscribe.runtime -- the SERVER path (v0.2.5 M1.5): a `@wasm` kernel's compiled `.wasm`
run IN-PROCESS in plain CPython under wasmtime-py, inside a capability sandbox.

    from pythscribe.runtime import ServerKernel, Sandbox
    k = ServerKernel.from_artifact(binding.artifact, sandbox=Sandbox(fuel=50_000_000))
    r = k.call("edit_distance", ["list[int]", "list[int]"], [a_codes, b_codes], return_type="int")

ONE buffer channel. Lists cross the boundary in the compiler's OWN list layout, exactly as
the browser shim `pythscribe/ffi/list_buffer.mjs` lays them out (the declared binding K7):

    ptr -> [len:i32 LE][cap:i32 LE][elements...]   i64 for list[int], f64 for list[float], i32 for list[bool]

Every allocation THIS channel and the shim make is rounded up to 8 bytes (the compiler's bump
allocator does not align, and the compiler's OWN glue marshaller does not round either -- the
two pythscribe channels never share a heap with the glue, so the invariant is scoped to them;
opus m1.5 r2/NEW-4); the 8-byte alignment of the element block is ASSERTED; a stale `__ovf` is cleared at call
entry and an overflow after the call is REFUSED (OverflowError -- there is no JS twin on
this path, so nothing re-runs the call at arbitrary precision); the heap pointer is restored
in `finally`. `tests/pythscribe/test_runtime.py` pins this module's layout constants to the
shim's AND proves both channels write byte-identical linear memory for the same list.

The sandbox is STRUCTURAL, not a flag: the only host functions the linker ever defines are
the codegen's `math.*` f64 functions (`_jsmath`, JS semantics). There is no WASI object on
this path -- a module that imports `wasi_snapshot_preview1.fd_write` (or anything else) is
refused at link time (SandboxViolation), before a single instruction runs. Fuel metering
(`Sandbox(fuel=N)`) bounds execution: an unbounded loop traps with FuelExhausted. A memory
cap bounds linear memory. Nothing here can reach the filesystem, the network or the
environment on the kernel's behalf, because nothing here is wired to them.

Fuel is OPT-IN per kernel (`@wasm(fuel=N)`, `PYTHSCRIBE_FUEL`, `Sandbox(fuel=N)`): the default
`Sandbox()` is unmetered, so a trusted kernel pays nothing; for untrusted / generated code you
MUST set a budget -- without one an unbounded loop runs unbounded (no epoch interrupt is armed).

Threads: each Python thread gets its OWN Store + Instance (own linear memory) of the shared
compiled Module, so N threads run N WASM instances with no shared state; wasmtime-py's
ctypes bridge releases the GIL for the duration of the WASM call, which is what makes the
GIL-free fan-out real (measured, not asserted -- see the full-features notebook).
"""
from __future__ import annotations

import atexit
import hashlib
import inspect
import struct
import threading
import weakref
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Sequence

from ..artifacts import ArtifactInfo
from ._jsmath import HOST_MATH

__all__ = [
    "ServerKernel",
    "Sandbox",
    "CallResult",
    "RuntimeUnavailable",
    "SandboxViolation",
    "FuelExhausted",
    "WasmTrap",
    "ServerFfiError",
    "PythscribeAliasingError",
    "check_buffer_aliasing",
    "wasmtime_available",
    "AbiMismatchError",
    "LAYOUT_VERSION",
    "HEADER_BYTES",
    "ELEM_BYTES",
    "HOST_MODULE",
    "HOST_FUNCTIONS",
]

# --- the declared binding (K7): must equal pythscribe/ffi/list_buffer.mjs -------------------
LAYOUT_VERSION = "pyths-0.2.4-list-v1"
HEADER_BYTES = 8
ELEM_BYTES = {"list[int]": 8, "list[float]": 8, "list[bool]": 4}
_ELEM_FMT = {"list[int]": "q", "list[float]": "d", "list[bool]": "i"}
SCALAR_PARAMS = frozenset({"int", "float", "bool"})
SCALAR_RETURNS = frozenset({"int", "float", "bool", "None"})

# M2a-3/M2b: parse an `Array[dtype]` / `Array[dtype, ndim]` annotation to its
# (dtype, ndim), ndim ∈ {1, 2} (1-D default; 2-D C-contiguous row-major, M2b).
# Any other spelling (ndim>2, unknown dtype, malformed) → None, so the caller
# refuses it loudly. The normalization is single-sourced in the leaf marshalling
# library (`array_buffer.array_param_spec`, N3 unification) so the server runtime,
# the FFI grammar and the browser glue can never drift apart.
from . import array_buffer as _ab
from .array_buffer import array_param_spec  # re-exported for `pythscribe.runtime.array_param_spec`

# M2.1 (spec 13-09-26 §5.6): the ONE ABI authority. Imported AFTER LAYOUT_VERSION is bound above
# (abi.expected_layouts() reads it lazily from this module -- the K7 home is unchanged).
from . import abi as _abi
from .abi import AbiMismatchError


def array_param_dtype(param_type: str) -> str | None:
    """dtype spelling for an admitted `Array[dtype, 1|2]` param, else None."""
    spec = array_param_spec(param_type)
    return None if spec is None else spec[0]


# --- caller-side buffer-aliasing guard (issue #494) ----------------------------------------
# ONE total address primitive via the C buffer API. `PyObject_GetBuffer(obj, &view,
# PyBUF_INDIRECT)` returns the exporter's data pointer + shape/strides/suboffsets for ANY
# buffer-protocol object -- writable OR read-only, contiguous OR STRIDED, NumPy or not, ctypes
# array, `bytes`, `bytearray`, `memoryview` (incl. an offset or strided slice). From those we
# compute the memory FOOTPRINT (the `numpy.byte_bounds` rule) so a strided view's true span is
# covered. INDIRECT (not the earlier SIMPLE) matters because a STRIDED view can reach the
# aliasing check through the LIST sink: `_pack_list` iterates ANY int/float sequence, so a
# strided `memoryview`/ndarray view passes as a read-only `list[...]` input and would otherwise
# get "no extent" under SIMPLE and alias a mutated out-buffer SILENTLY.
#
# The list sink admits sequences BEYOND buffers, so the SINK's input domain is bounded to what
# this authority can reason about. `_pack_list` marshals through the object's OWN
# `__len__`/`__iter__`/`__getitem__`, so the admission (`_admit_list_arg` below) binds THAT SAME
# channel: when any out-param is mutated, a `list[...]` argument is admitted IFF (a) it is an
# EXACT `list`/`tuple`/`range` (C-level iteration over Python objects: cannot reference a
# foreign byte buffer), or (b) it is a buffer-protocol object built on one of the builtin
# buffer types whose sink slots AND buffer-protocol slots (`__buffer__`/`__release_buffer__`)
# are UNOVERRIDDEN (so its iteration reads exactly the storage the address authority sees)
# AND is extent-addressable. Anything else -- a `list` subclass, an
# `__array__` wrapper (`__array__` may return a COPY while `__getitem__` reads live memory, so
# `np.asarray` cannot make it sound), a generic Sequence -- is REFUSED LOUDLY, never marshalled
# to a silent independent copy. The old NumPy `byte_bounds` / `frombuffer` / `shares_memory` /
# exporter-identity tiers and the `__array__` admission tier are gone.
import array as _array
import ctypes as _ctypes
import sys as _sys


class _Py_buffer(_ctypes.Structure):
    _fields_ = [
        ("buf", _ctypes.c_void_p),
        ("obj", _ctypes.c_void_p),
        ("len", _ctypes.c_ssize_t),
        ("itemsize", _ctypes.c_ssize_t),
        ("readonly", _ctypes.c_int),
        ("ndim", _ctypes.c_int),
        ("format", _ctypes.c_char_p),
        ("shape", _ctypes.POINTER(_ctypes.c_ssize_t)),
        ("strides", _ctypes.POINTER(_ctypes.c_ssize_t)),
        ("suboffsets", _ctypes.POINTER(_ctypes.c_ssize_t)),
        ("internal", _ctypes.c_void_p),
    ]


# PyBUF_INDIRECT = PyBUF_STRIDES | 0x0100 = 0x0118: request shape + strides + suboffsets,
# read-only, WITHOUT PyBUF_FORMAT. Dropping FORMAT matters: with FORMAT set, GetBuffer raises
# for a FORMAT-unrepresentable dtype (e.g. datetime64 -> "cannot include dtype 'M'") on a
# buffer that IS addressable -- which would return None where the footprint is well-defined.
# 0x118 gives the address + shape/strides/suboffsets (itemsize is filled regardless of FORMAT).
_PyBUF_INDIRECT = 0x118
try:
    _PyObject_GetBuffer = _ctypes.pythonapi.PyObject_GetBuffer
    _PyObject_GetBuffer.argtypes = [_ctypes.py_object, _ctypes.POINTER(_Py_buffer), _ctypes.c_int]
    _PyObject_GetBuffer.restype = _ctypes.c_int
    _PyBuffer_Release = _ctypes.pythonapi.PyBuffer_Release
    _PyBuffer_Release.argtypes = [_ctypes.POINTER(_Py_buffer)]
    _PyBuffer_Release.restype = None
    _PyErr_Clear = _ctypes.pythonapi.PyErr_Clear
    _PyErr_Clear.argtypes = []
    _PyErr_Clear.restype = None
except Exception:  # pragma: no cover - pythonapi is always present on CPython
    _PyObject_GetBuffer = None


def _buffer_extent(obj: Any) -> "tuple[int, int] | None":
    """The half-open byte interval `(start_address, nbytes)` covering `obj`'s ENTIRE memory
    footprint -- including a STRIDED view's real span (the `numpy.byte_bounds` rule) -- or None
    when `obj` is not a directly addressable buffer: a plain `list`/int/None (GetBuffer raises),
    or a PIL-style indirect buffer whose `suboffsets` is non-NULL (genuinely not a flat address
    range). An EMPTY buffer is addressable with `nbytes == 0` (it overlaps nothing, but it IS
    something the authority can reason about, so admission must not refuse it). TOTAL and
    NumPy-free, so two buffers alias iff their footprints intersect, independent of
    dtype/shape/value/writability."""
    if _PyObject_GetBuffer is None:  # pragma: no cover
        return None
    view = _Py_buffer()
    try:
        # ctypes re-raises the exception GetBuffer sets for a NON-buffer (TypeError on a list).
        rc = _PyObject_GetBuffer(_ctypes.py_object(obj), _ctypes.byref(view), _PyBUF_INDIRECT)
    except Exception:
        return None
    if rc != 0:  # pragma: no cover - defensive: a build that returns -1 without ctypes raising
        _PyErr_Clear()
        return None
    try:
        if view.suboffsets:  # non-NULL -> indirect (PIL-style): not a flat addressable range
            return None
        buf = int(view.buf or 0)
        length = int(view.len)
        if length == 0:
            return buf, 0  # addressable, overlaps nothing (`_buffers_overlap` requires nbytes > 0)
        ndim = int(view.ndim)
        itemsize = int(view.itemsize) or 1
        if ndim == 0 or not view.strides or not view.shape:
            return buf, length  # scalar, or C-contiguous (len IS the footprint; strides NULL)
        low = high = buf  # byte_bounds: accumulate the min/max byte the strided view can touch
        for i in range(ndim):
            stride, n = int(view.strides[i]), int(view.shape[i])
            if stride >= 0:
                high += stride * (n - 1)
            else:  # NEGATIVE stride (a reversed view): extends the LOW bound, not the high
                low += stride * (n - 1)
        return low, (high + itemsize) - low
    finally:
        _PyBuffer_Release(_ctypes.byref(view))


def _resolve_extent(obj: Any) -> "tuple[int, int] | None":
    """`_buffer_extent(obj)`, but also resolving an `__array__`-exporting WRAPPER (e.g.
    `pandas.Series(x, copy=False)`) that does not itself export the buffer protocol: `np.asarray`
    hands back the underlying array (sharing memory for a no-copy wrapper), whose footprint IS
    addressable. Returns None for a genuine non-buffer / non-array object (a plain `list`, a
    user `Sequence` wrapper). NumPy stays OPTIONAL (the `__array__` tier is skipped when absent).

    This is EXTRA overlap DETECTION inside `_buffers_overlap` only -- NEVER an admission path:
    `__array__` may return a COPY while the wrapper's `__getitem__`/`__iter__` read live memory,
    so a disjoint `np.asarray` footprint proves nothing about what the list sink will read. The
    list-sink admission is `_admit_list_arg`, which does not consult `__array__` at all."""
    e = _buffer_extent(obj)
    if e is not None:
        return e
    if hasattr(type(obj), "__array__"):
        try:
            import numpy as _np
            return _buffer_extent(_np.asarray(obj))
        except Exception:
            return None
    return None


def _buffers_overlap(a: Any, b: Any) -> bool:
    """True when `a` and `b` are the SAME object or share (overlapping) backing memory, decided
    EXACTLY by memory-footprint overlap (`_resolve_extent`): distinct allocations and disjoint
    slices (`mv[:2]` / `mv[2:]`) are NOT over-refused, while ANY true overlap -- `x[:]`, a
    partial slice, a STRIDED view of the buffer (`x[::2]`), two DIFFERENT exporter objects over
    one `bytearray`, or an `__array__` wrapper (`pandas.Series(x, copy=False)`) over the buffer
    -- IS refused, regardless of dtype, shape, value, writability, or NumPy's presence. (Two
    interleaved strided views `x[::2]`/`x[1::2]` share no element but do share a byte range, so
    this refuses them -- conservative/loud, never silently wrong.) When an operand is neither an
    addressable buffer nor an array-exporting wrapper (e.g. a `list`), only object identity can
    alias it, so `a is b` above is the complete rule for that case and this returns False."""
    if a is b:
        return True
    ea, eb = _resolve_extent(a), _resolve_extent(b)
    if ea is None or eb is None:
        return False  # a non-buffer (list) / indirect operand: only identity (handled) aliases it
    (sa, na), (sb, nb) = ea, eb
    return na > 0 and nb > 0 and sa < sb + nb and sb < sa + na  # half-open intervals intersect


# --- list-sink ADMISSION (issue #494, the root) ------------------------------------------------
# `_pack_list` reads its argument through exactly three slots: `len(a)` (`__len__`) and
# `enumerate(a)` (`__iter__`, or the `__getitem__` sequence fallback when a type has no
# `__iter__` -- e.g. a ctypes array). The admission below binds THAT channel, so the invariant
# is: admitted => (its iteration reads exactly the storage the address authority addressed)
# OR (it cannot alias a foreign buffer at all).
#
# Rule (b) rests on TWO readers agreeing on one storage: the MARSHALLER reads through the
# iteration slots, the AUTHORITY (`_buffer_extent`) reads through the BUFFER PROTOCOL
# (`PyObject_GetBuffer` -> `bf_getbuffer` / `bf_releasebuffer`, which a Python subclass
# overrides via PEP 688's `__buffer__` / `__release_buffer__`, 3.12+). Those are DIFFERENT
# channels, so the identity "iteration storage == addressed storage" holds only while NEITHER
# channel is overridden ahead of the builtin base: an overriding `__buffer__` can hand the
# authority a DECOY view (disjoint from the out) while the inherited `__iter__` still reads the
# real -- aliased -- storage; an overriding `__release_buffer__` runs arbitrary Python between
# the authority's address read and the marshaller's iteration (it can resize/relocate the very
# storage it just certified). Hence two slot sets, kept SEPARATE because they name different
# readers: `_LIST_SINK_SLOTS` is what the marshaller reads, `_BUFFER_PROTO_SLOTS` is what the
# authority reads. Both must be unoverridden for (b) to be sound. (On < 3.12 a class-level
# `__buffer__` is an inert attribute; refusing it there is the conservative, loud direction.)
_EXACT_CONTAINERS = (list, tuple, range)  # EXACT types only: a subclass may override a slot
_LIST_SINK_SLOTS = ("__iter__", "__getitem__", "__len__")
_BUFFER_PROTO_SLOTS = ("__buffer__", "__release_buffer__")
# The raw `type` descriptors, so the MRO / per-class dict we inspect is the one CPython's own
# special-method lookup (`_PyType_Lookup`) walks -- a metaclass cannot answer for them.
_type_mro = type.__dict__["__mro__"].__get__
_type_dict = type.__dict__["__dict__"].__get__


def _safe_buffer_bases() -> "tuple[type, ...]":
    """The builtin buffer types whose sink slots are C-level reads of the SAME storage
    `PyObject_GetBuffer` exposes. `numpy.ndarray` is looked up WITHOUT importing NumPy: if it was
    never imported no ndarray can exist, so a missing entry can only over-refuse (loudly)."""
    bases: list[type] = [memoryview, bytes, bytearray, _array.array, _ctypes.Array]
    np = _sys.modules.get("numpy")
    nd = getattr(np, "ndarray", None) if np is not None else None
    if isinstance(nd, type):
        bases.append(nd)
    return tuple(bases)


def _admit_list_arg(v: Any) -> "str | None":
    """The ONE list-sink admission rule (issue #494): None when `v` may be marshalled by
    `_pack_list` while an out-param is mutated, else the reason it is refused.

    (a) `type(v)` is EXACTLY `list`, `tuple` or `range`: iteration is C-level over Python objects
        and cannot reference a foreign byte buffer. A SUBCLASS is NOT admitted here -- it may
        override `__iter__`/`__getitem__` to read a live buffer -- and it has no buffer base
        either, so it falls through to the refusal.
    (b) `v` is an instance of a builtin buffer type (`memoryview`/`bytes`/`bytearray`/
        `array.array`/`numpy.ndarray`/a ctypes array) and NO class ahead of that base in its MRO
        defines any of the ITERATION slots `__iter__`/`__getitem__`/`__len__` (a slot the base
        itself lacks -- ctypes arrays have no `__iter__` -- counts as overridden when a subclass
        ADDS it, because that changes the channel `enumerate` uses) NOR either BUFFER-PROTOCOL
        slot `__buffer__`/`__release_buffer__` (PEP 688: the channel the aliasing authority
        reads; an override can point it at a decoy or relocate the storage after it was
        addressed), AND `_buffer_extent(v)` is addressable. With all five slots unoverridden,
        both readers are the builtin base's own C-level reads of ONE storage, so its iteration
        reads exactly the storage the aliasing authority reasons over.
    Anything else -- a `list`/`tuple` subclass, an `__array__`-only wrapper (`pandas.Series`),
    a generic Sequence, a PIL-style indirect buffer -- is refused."""
    t = type(v)
    if t in _EXACT_CONTAINERS:
        return None
    mro = _type_mro(t)
    bases = _safe_buffer_bases()
    base = next((b for b in mro if b in bases), None)
    if base is None:
        return (
            f"got {t.__name__}: the list FFI marshals through the object's own "
            f"__len__/__iter__/__getitem__, which the buffer-aliasing authority cannot bind for "
            f"this type -- pass a plain list/tuple or an array; for a wrapper use "
            f".tolist()/.to_numpy()"
        )
    for cls in mro:
        if cls is base:
            break
        d = _type_dict(cls)
        for slot in _LIST_SINK_SLOTS:
            if slot in d:
                return (
                    f"got {t.__name__}, a {base.__name__} subclass that overrides {slot} "
                    f"(in {cls.__name__}), so its iteration need not read the storage the "
                    f"aliasing authority addresses -- pass a plain list/tuple or an unwrapped "
                    f"array; for a wrapper use .tolist()/.to_numpy()"
                )
        for slot in _BUFFER_PROTO_SLOTS:
            if slot in d:
                return (
                    f"got {t.__name__}, a {base.__name__} subclass that overrides the buffer "
                    f"protocol slot {slot} (in {cls.__name__}), so the storage the aliasing "
                    f"authority addresses need not be the storage its iteration reads -- pass "
                    f"a plain list/tuple or an unwrapped array; for a wrapper use "
                    f".tolist()/.to_numpy()"
                )
    if _buffer_extent(v) is None:
        return (
            f"got {t.__name__}, a buffer the aliasing authority cannot address (indirect "
            f"/ suboffsets buffer) -- pass a plain list/tuple or a flat array"
        )
    return None


def check_buffer_aliasing(
    fn: str,
    args: Sequence[Any],
    list_idx: Sequence[int],
    array_idx: Sequence[int],
    read_back: Sequence[int],
) -> None:
    """The ONE caller-side buffer authority for the server path (issue #494): it bounds the LIST
    sink's input domain AND refuses caller-side aliasing, so both are decided in one place.

    CPython aliases the SAME object across parameters (`shift_arr(x, x)`, a NumPy view
    `shift_arr(x[:], x)`, a strided view `rev(x[::2], x)`, or an `__array__` wrapper
    `rev(pd.Series(x), x)`); the server path marshals EACH buffer param to an INDEPENDENT WASM
    linear-memory buffer, so a mutated out-param and a second param that share one Python object
    diverge SILENTLY (last-writer-wins). This refuses that call LOUDLY (`PythscribeAliasingError`)
    instead of silently miscomputing -- CPython-faithful-by-refusal.

    (1) LIST-SINK ADMISSION. `_pack_list` iterates ANY sequence through the object's OWN
        `__len__`/`__iter__`/`__getitem__`, so a `list` subclass, an `__array__` wrapper
        (`pandas.Series`), or a user `Sequence` over `x` could read a mutated out-buffer that the
        address authority cannot see (or sees at a DIFFERENT address than the one it reads).
        When any out-param is mutated (`read_back` non-empty -- the only way an alias becomes a
        silent WRONG value), every `list[...]` argument must pass `_admit_list_arg`: an EXACT
        `list`/`tuple`/`range`, or a builtin-based buffer with UNOVERRIDDEN sink slots AND
        buffer-protocol slots (`__buffer__`/`__release_buffer__`, so the authority and the
        marshaller read ONE storage) that is extent-addressable. Anything else is refused with a
        `ServerFfiError`, never marshalled
        to a silent independent copy. (No mutation => no silent-wrong => the domain is not
        narrowed.)

    (2) ALIASING. Both the `list[...]` and `Array[...]` marshallers route their buffer indices
        through the SAME overlap check, so the list twin and the array twin are guarded
        identically. It fires only when at least one of the two shared params is a MUTATED
        out-param (`read_back`): two READ-ONLY inputs that share a buffer marshal to two identical
        copies, which is correct, so those are NOT refused -- the non-aliased path is never
        over-refused."""
    out = frozenset(read_back)
    if out:  # a mutated out-param exists -> bound the list sink to the authority's reasoning domain
        for i in list_idx:
            why = _admit_list_arg(args[i])
            if why is not None:
                raise ServerFfiError(
                    f"{fn}: parameter {i} is not admitted as a list[...] argument while an "
                    f"out-param is mutated -- {why} (refused, never silently aliased; issue #494)."
                )
    idx = list(list_idx) + list(array_idx)
    for p in range(len(idx)):
        i = idx[p]
        for q in range(p + 1, len(idx)):
            j = idx[q]
            if i not in out and j not in out:
                continue  # neither buffer is mutated: sharing marshals to identical copies (correct)
            if _buffers_overlap(args[i], args[j]):
                lo, hi = (i, j) if i < j else (j, i)
                mutated = i if i in out else j
                other = j if mutated == i else i
                raise PythscribeAliasingError(
                    f"{fn}: parameters {lo} and {hi} are the SAME (or memory-overlapping) buffer, and "
                    f"parameter {mutated} is a mutated out-param: the server path marshals each buffer "
                    f"param to an INDEPENDENT WASM buffer (last-writer-wins), which diverges SILENTLY "
                    f"from CPython's shared-object aliasing. Pass an independent copy so the buffers do "
                    f"not alias (e.g. copy parameter {other} before the call) -- refused, never silently "
                    f"miscomputed (issue #494)."
                )


HOST_MODULE = "math"
HOST_FUNCTIONS = tuple(sorted(HOST_MATH))

_I64_MAX = 2**63 - 1
_I64_MIN = -(2**63)
DEFAULT_MEMORY_BYTES = 256 * 1024 * 1024


class RuntimeUnavailable(RuntimeError):
    """wasmtime-py is not installed (`pip install pythscribe[server]`)."""


class ServerFfiError(RuntimeError):
    """An argument/return outside the crossable grammar, a bad arity, a missing export."""


class PythscribeAliasingError(ServerFfiError):
    """A single call passed the SAME (identical or memory-overlapping) buffer to two params
    where at least one is a MUTATED out-param (issue #494). CPython aliases the one object;
    the server path marshals each buffer param to an INDEPENDENT WASM buffer, so the result
    would diverge silently (last-writer-wins). Refused LOUDLY -- CPython-faithful-by-refusal --
    rather than silently miscomputed. A subclass of ServerFfiError so existing handlers still
    catch it, but a distinct type so a caller can special-case the aliasing contract."""


class SandboxViolation(RuntimeError):
    """The module asked for a capability the sandbox does not grant (an import outside
    `math.*`), or exceeded a resource bound."""


class FuelExhausted(SandboxViolation):
    """Execution consumed the whole fuel budget (an unbounded / too-long computation)."""


class WasmTrap(RuntimeError):
    """The kernel trapped (unreachable / out-of-bounds / integer divide by zero ...)."""


def wasmtime_available() -> bool:
    try:
        import wasmtime  # noqa: F401
    except ImportError:
        return False
    return True


def _wasmtime():
    try:
        import wasmtime
    except ImportError as e:
        raise RuntimeUnavailable(
            "the server path needs wasmtime-py: `pip install pythscribe[server]` (or `pip install wasmtime`)"
        ) from e
    return wasmtime


@dataclass(frozen=True)
class Sandbox:
    """Resource bounds for in-process execution. `fuel=None` = unmetered (trusted code);
    an int meters execution and traps with FuelExhausted when exhausted. `memory_bytes`
    caps linear memory (a `memory.grow` past it fails inside the kernel)."""

    fuel: int | None = None
    memory_bytes: int = DEFAULT_MEMORY_BYTES

    def __post_init__(self) -> None:
        if self.fuel is not None and (isinstance(self.fuel, bool) or not isinstance(self.fuel, int) or self.fuel <= 0):
            raise ValueError(f"Sandbox.fuel must be a positive int or None, got {self.fuel!r}")
        if isinstance(self.memory_bytes, bool) or not isinstance(self.memory_bytes, int) or self.memory_bytes < 65536:
            raise ValueError(f"Sandbox.memory_bytes must be an int >= 65536, got {self.memory_bytes!r}")

    @property
    def metered(self) -> bool:
        return self.fuel is not None


@dataclass
class CallResult:
    value: Any
    outs: dict[int, list]  # read-back list params by index
    fuel_used: int | None
    heap_bytes: int
    thread_id: int = field(default_factory=threading.get_ident)


_engines: dict[bool, Any] = {}
_engines_lock = threading.Lock()
_kernels: "weakref.WeakSet[ServerKernel]" = None  # type: ignore[assignment]  (set below)


def _shutdown() -> None:
    """Close instances -> modules -> engines in order at interpreter exit, so wasmtime-py's
    `Managed.__del__` never runs after its ctypes bindings were torn down."""
    kernels = list(_kernels) if _kernels is not None else []
    for k in kernels:
        try:
            k.close()
        except Exception:  # pragma: no cover
            pass
    # a daemon thread still inside a WASM call at exit keeps its Module outstanding: THAT engine must not
    # be closed under it (r3/N2) and is left to interpreter teardown; every other engine is closed in
    # order as the docstring promises (r4/NEW-6 -- scoped, not all-or-nothing)
    outstanding = {id(k._engine) for k in kernels if k.release_pending}
    with _engines_lock:
        for key, eng in list(_engines.items()):
            if id(eng) in outstanding:
                continue
            try:
                eng.close()
            except Exception:  # pragma: no cover
                pass
            del _engines[key]


def _engine(metered: bool):
    """One Engine per fuel setting (a Module compiled with fuel accounting must run in a
    Store of the SAME engine); compilation is cached per engine by wasmtime itself."""
    with _engines_lock:
        eng = _engines.get(metered)
        if eng is None:
            wt = _wasmtime()
            cfg = wt.Config()
            cfg.consume_fuel = metered
            cfg.cranelift_nan_canonicalization = True  # NaN payloads never leak host/platform bits
            eng = wt.Engine(cfg)
            _engines[metered] = eng
        return eng


class ServerKernel:
    """A compiled kernel module + the sandbox it runs in. Thread-safe: every calling thread
    gets its own Store/Instance (own memory); the Module is shared."""

    def __init__(self, wasm_bytes: bytes, *, sandbox: Sandbox | None = None, name: str = "<wasm>"):
        wt = _wasmtime()
        self.sandbox = sandbox or Sandbox()
        self.name = name
        self.wasm_sha256 = hashlib.sha256(wasm_bytes).hexdigest()
        self.wasm_bytes_len = len(wasm_bytes)
        self._engine = _engine(self.sandbox.metered)
        # M2.1 (spec 13-09-26 §5.6, Layer 1): the module's OWN `pyths.abi` section must agree with
        # this runtime -- major AND both layout strings -- AND it must export exactly one defined,
        # immutable i32 `__pyths_abi` global == the major (codex B7: the section is forgeable
        # provenance; the exported global is the load-bearing half), BEFORE wasmtime compiles it and
        # before any export is called. A mismatched / absent section is refused LOUDLY (AbiMismatchError names
        # the field and both sides) and SHORT-CIRCUITS: `wt.Module` is never reached on a mismatch
        # (codex m2 should-fix -- compiling first let an invalid core section flip the diagnosis to
        # WasmTrap and hide the ABI verdict; control: zero `wt.Module` calls, §G-ABI-5). Only bytes
        # our section walk cannot parse AS A CONTAINER (bad magic / truncated section -- no verdict
        # is possible) get wasmtime's own diagnosis (WasmTrap); compilation never runs `start`.
        try:
            self.abi = _abi.check_module(wasm_bytes, name=name)
        except AbiMismatchError:
            raise  # the verdict IS the diagnosis: no compilation, no instantiation
        except ValueError as walk_err:
            try:
                wt.Module(self._engine, wasm_bytes)
            except wt.WasmtimeError as e:
                raise WasmTrap(f"{name}: not a valid WebAssembly module ({str(e).splitlines()[0]})") from e
            # a module wasmtime accepts but our section walk could not read: refuse, never skip
            raise AbiMismatchError(f"{name}: cannot read the `{_abi.ABI_SECTION_NAME}` section ({walk_err})", field="section") from walk_err
        try:
            self._module = wt.Module(self._engine, wasm_bytes)
        except wt.WasmtimeError as e:
            raise WasmTrap(f"{name}: not a valid WebAssembly module ({str(e).splitlines()[0]})") from e
        # the capability check happens at LINK time, once, on the module's import list --
        # before any instance exists
        self.host_imports: tuple[str, ...] = tuple(dict.fromkeys(self._check_imports()))  # deduped: a repeated import must not double-define
        self.exports: tuple[str, ...] = tuple(e.name for e in self._module.exports)
        self._linker = self._build_linker()
        self._local = threading.local()
        # RE-ENTRANT (opus m1.5 r4/NEW-2): an _Instance finalizer fired by GC while this thread holds the
        # lock re-enters _try_release_module; a plain Lock self-deadlocked the process
        self._stats_lock = threading.RLock()
        # weak: a thread that dies (or resets) releases its Store + linear memory; nothing here
        # pins an instance per thread forever (opus m1.5 r1/S9)
        self._all_instances: "weakref.WeakSet[_Instance]" = weakref.WeakSet()
        self.calls = 0
        self.instances = 0
        self._closed = False
        self.module_released = False
        self.release_pending = False
        _kernels.add(self)

    # ----------------------------------------------------------------- construction helpers
    @classmethod
    def from_wasm(cls, path: str | Path | bytes, *, sandbox: Sandbox | None = None, name: str | None = None) -> "ServerKernel":
        if isinstance(path, (bytes, bytearray)):
            return cls(bytes(path), sandbox=sandbox, name=name or "<bytes>")
        p = Path(path)
        return cls(p.read_bytes(), sandbox=sandbox, name=name or p.name)

    @classmethod
    def from_artifact(cls, info: ArtifactInfo, *, sandbox: Sandbox | None = None) -> "ServerKernel":
        """The verified artifact's `.wasm` (`artifacts.verify` already hashed it against the manifest)."""
        return cls(info.wasm.read_bytes(), sandbox=sandbox, name=info.function)

    def _check_imports(self) -> list[str]:
        wt = _wasmtime()
        names: list[str] = []
        for imp in self._module.imports:
            mod, name = imp.module, imp.name
            if mod != HOST_MODULE or name not in HOST_MATH or not isinstance(imp.type, wt.FuncType):
                raise SandboxViolation(
                    f"{self.name}: import `{mod}.{name}` refused -- the sandbox grants no capability beyond "
                    f"the pure `{HOST_MODULE}.*` functions ({', '.join(HOST_FUNCTIONS)}); no filesystem, "
                    "network, clock or environment is reachable from a kernel"
                )
            arity, _ = HOST_MATH[name]
            ty = imp.type
            if len(ty.params) != arity or len(ty.results) != 1 or any(str(p) != "f64" for p in ty.params) or str(ty.results[0]) != "f64":
                raise SandboxViolation(f"{self.name}: import `{mod}.{name}` has signature {ty}, not f64^{arity} -> f64")
            names.append(name)
        return names

    def _build_linker(self):
        wt = _wasmtime()
        linker = wt.Linker(self._engine)
        f64 = wt.ValType.f64()
        for name in self.host_imports:
            arity, impl = HOST_MATH[name]
            linker.define_func(HOST_MODULE, name, wt.FuncType([f64] * arity, [f64]), impl)
        # deliberately NO linker.define_wasi(): the absence is the sandbox
        return linker

    # ----------------------------------------------------------------- instances
    def _instance(self):
        if self._closed:
            raise ServerFfiError(f"{self.name}: this ServerKernel was closed")
        inst = getattr(self._local, "inst", None)
        if inst is None:
            inst = self.new_instance()
            self._local.inst = inst
        return inst

    def new_instance(self) -> "_Instance":
        """A fresh Store + Instance (fresh linear memory). Called once per thread lazily; call
        it yourself for an isolated run of untrusted code (nothing survives between instances)."""
        if self._closed:  # r4/NEW-3: a closed kernel hands out no new instances (they would block the release)
            raise ServerFfiError(f"{self.name}: this ServerKernel was closed")
        wt = _wasmtime()
        store = wt.Store(self._engine)
        store.set_limits(memory_size=self.sandbox.memory_bytes)
        if self.sandbox.metered:
            store.set_fuel(self.sandbox.fuel)  # type: ignore[arg-type]
        try:
            instance = self._linker.instantiate(store, self._module)
        except wt.WasmtimeError as e:
            raise SandboxViolation(f"{self.name}: instantiation refused: {str(e).splitlines()[0]}") from e
        ex = instance.exports(store)
        inst = _Instance(store, ex, self)
        with self._stats_lock:
            self.instances += 1
            self._all_instances.add(inst)
        return inst

    def close(self) -> None:
        """Mark closed (every later call is refused) and release what can be released SAFELY:
        every instance that is not currently inside a WASM call. An instance owned by another
        thread that IS inside a call (GIL released; `_Instance.in_call`) must not have its Store
        freed under it (opus m1.5 r2/NEW-5; keyed on the in-call flag, not thread liveness --
        r3/N8); it is deferred, its next call is refused, and its own `close()` (or GC) retries
        the Module release. RE-ENTRANT: a later `close()` retries a pending release (r3/N2), so
        the Module/Linker are never leaked for the process lifetime. `module_released` says
        whether the release has happened."""
        self._closed = True
        with self._stats_lock:
            insts = list(self._all_instances)
        self._local = threading.local()
        for inst in insts:
            # check-and-claim UNDER the lock that the call prologue also takes (r4/NEW-1): an instance
            # that is (or is about to be) inside a call is deferred, never freed under its owner.
            # DECISION (r5/R5-5): the claim (`closed = True`) is made under the lock, the Store is freed
            # outside it; between the two, _try_release_module may see "no busy instance" and release
            # the Module while this Store is still open -- safe, because wasmtime clones the module
            # into the store at instantiation, so the host Module handle is not what keeps it alive.
            with self._stats_lock:
                claim = not inst.in_call and not inst.closed
                if claim:
                    inst.closed = True
            if claim:
                inst._release_store()
        self._try_release_module()

    def _try_release_module(self) -> None:
        """Release the Linker + Module once no instance is still inside a call (idempotent)."""
        if not self._closed or self.module_released:
            return
        with self._stats_lock:
            busy = [i for i in self._all_instances if i.in_call or not i.closed]
            if busy:
                self.release_pending = True
                return
            self.release_pending = False
            self.module_released = True  # published under the lock as the CLAIM; the closes run outside it on
        try:  # purpose (never hold the lock across a ctypes close -- r5/R5-4): a reader may see True a moment early
            self._linker.close()
            self._module.close()
        except Exception:  # pragma: no cover
            pass

    def reset_thread_instance(self) -> None:
        """Drop (and close) THIS thread's instance; the next call gets fresh linear memory."""
        inst = getattr(self._local, "inst", None)
        self._local.inst = None
        if inst is not None:
            inst.close()

    # ----------------------------------------------------------------- the call
    def call(
        self,
        fn: str,
        param_types: Sequence[str],
        args: Sequence[Any],
        *,
        read_back: Sequence[int] = (),
        return_type: str = "int",
        fuel: int | None = None,
        instance: "_Instance | None" = None,
    ) -> CallResult:
        """Call export `fn`. Mirrors `list_buffer.mjs::call` exactly (see module docstring).
        `fuel` overrides the sandbox's per-call budget (only in a metered sandbox)."""
        inst = instance or self._instance()
        if inst.kernel is not self:  # codex m1.5 r2/#3: an instance of ANOTHER module would run that module's export
            raise ServerFfiError(f"{fn}: the given instance belongs to kernel `{inst.kernel.name}`, not `{self.name}`")
        r = inst.call(fn, list(param_types), list(args), read_back=list(read_back), return_type=return_type, fuel=fuel)
        with self._stats_lock:
            self.calls += 1
        return r


class _Instance:
    """One Store + Instance: NOT thread-safe (a Store is single-threaded by wasmtime's rules)."""

    def __init__(self, store, exports, kernel: ServerKernel):
        self.store = store
        self.exports = exports
        self.kernel = kernel
        self.memory = exports.get("memory")
        self.alloc = exports.get("__alloc")
        self.heap_ptr = exports.get("__heap_ptr")
        self.ovf = exports.get("__ovf")
        self.owner = threading.get_ident()
        self.closed = False
        self.in_call = False  # True while inside the wasmtime section (GIL may be released)

    def _release_store(self) -> None:
        try:
            self.store.close()
        except Exception:  # pragma: no cover
            pass

    def close(self) -> None:
        with self.kernel._stats_lock:  # the flag transition in call() and this check exclude each other
            if self.closed:
                return
            if self.in_call:
                raise ServerFfiError("cannot close an instance from inside its own call")
            self.closed = True
        self._release_store()
        self.kernel._try_release_module()  # the last deferred instance completes a pending kernel release (r3/N2)

    def __del__(self) -> None:  # a deferred instance collected by GC also retries the release
        try:
            do_release = False
            with self.kernel._stats_lock:
                if not self.closed and not self.in_call:
                    self.closed = True
                    do_release = True
            if do_release:
                self._release_store()
            self.kernel._try_release_module()
        except Exception:
            pass

    def _scalar_in(self, t: str, v: Any, where: str) -> Any:
        if t == "int":
            if isinstance(v, bool):
                return int(v)
            if not isinstance(v, int):
                raise ServerFfiError(f"{where}: expected an int, got {type(v).__name__}")
            if v > _I64_MAX or v < _I64_MIN:
                raise ServerFfiError(f"{where}: {v} is outside the i64 range (refused, never wrapped)")
            return v
        if t == "float":
            if isinstance(v, bool) or not isinstance(v, (int, float)):
                raise ServerFfiError(f"{where}: expected a float, got {type(v).__name__}")
            return float(v)
        if t == "bool":
            # as strict as the int/float arms (opus m1.5 r1/S1): a bool, or 0/1 -- never a
            # truthy object silently coerced (`flag * x` with flag=2 would differ from Python)
            if isinstance(v, bool) or (isinstance(v, int) and v in (0, 1)):
                return 1 if v else 0
            raise ServerFfiError(f"{where}: expected a bool, got {type(v).__name__} {v!r} (refused, never coerced)")
        raise ServerFfiError(f"{where}: unsupported param type {t}")

    def _pack_list(self, t: str, a: Sequence[Any], where: str) -> bytes:
        n = len(a)
        fmt = _ELEM_FMT[t]
        # FAST PATH (#494-marshalling perf): let struct.pack validate + pack at C speed on the happy
        # path (all-valid inputs). It has the SAME accept/reject as the per-element loop below --
        # for list[int], `q` packs int (and bool as 0/1, matching the loop) and rejects float/str
        # and out-of-i64 with struct.error; for list[float], `d` packs int/float. The per-element
        # Python loop is ~15x slower (isinstance + range-check + append per element), so it runs ONLY
        # to LOCATE the offending element for a precise `[k]` message when the fast path raises. A
        # bool inside a list[float] is refused by the loud-fail contract but `d` would silently
        # accept it, so the float fast path is skipped when any bool is present (a cheap C-speed scan)
        # and falls to the loop, which raises the precise message. list[bool] always uses the loop.
        if t == "list[int]":
            try:
                return struct.pack(f"<ii{n}{fmt}", n, n, *a)
            except (struct.error, TypeError, OverflowError):
                pass  # -> per-element loop below locates the offending element
        elif t == "list[float]" and not any(x.__class__ is bool for x in a):
            try:
                return struct.pack(f"<ii{n}{fmt}", n, n, *a)
            except (struct.error, TypeError, OverflowError):
                pass
        try:
            if t == "list[int]":
                vals = []
                for k, x in enumerate(a):
                    if isinstance(x, bool):
                        x = int(x)
                    elif not isinstance(x, int):
                        raise ServerFfiError(f"{where}[{k}]: expected an int, got {type(x).__name__}")
                    if x > _I64_MAX or x < _I64_MIN:
                        raise ServerFfiError(f"{where}[{k}]: {x} is outside the i64 range (refused, never wrapped)")
                    vals.append(x)
            elif t == "list[float]":
                vals = []
                for k, x in enumerate(a):
                    if isinstance(x, bool) or not isinstance(x, (int, float)):
                        raise ServerFfiError(f"{where}[{k}]: expected a float, got {type(x).__name__}")
                    vals.append(float(x))
            else:
                vals = []
                for k, x in enumerate(a):
                    if isinstance(x, bool) or (isinstance(x, int) and x in (0, 1)):
                        vals.append(1 if x else 0)
                    else:
                        raise ServerFfiError(f"{where}[{k}]: expected a bool, got {type(x).__name__} {x!r} (refused, never coerced)")
            return struct.pack(f"<ii{n}{fmt}", n, n, *vals)
        except struct.error as e:  # pragma: no cover - the range checks above make this unreachable
            raise ServerFfiError(f"{where}: {e}") from e

    def call(self, fn: str, param_types: list[str], args: list[Any], *, read_back: list[int], return_type: str, fuel: int | None) -> CallResult:
        if threading.get_ident() != self.owner:
            raise ServerFfiError("a wasmtime Store is single-threaded: use ServerKernel.call (per-thread instances) or new_instance() on this thread")
        wt = _wasmtime()
        f = self.exports.get(fn)
        if f is None or not isinstance(f, wt.Func):
            raise ServerFfiError(f"wasm exports no function named {fn} (exports: {', '.join(self.kernel.exports)})")
        if len(param_types) != len(args):
            raise ServerFfiError(f"{fn} takes {len(param_types)} params, got {len(args)} args")
        if return_type not in SCALAR_RETURNS:
            raise ServerFfiError(f"{fn}: unsupported return type {return_type} (server path returns {sorted(SCALAR_RETURNS)}; lists cross as out-buffers)")
        list_idx = [i for i, t in enumerate(param_types) if t in ELEM_BYTES]
        # M2a-3: array params (`Array[dtype]`) marshal through the typed-array
        # channel (array_buffer.py) — the 16-byte header + bulk element copy —
        # rather than the 8-byte list header.
        array_idx: dict[int, str] = {}
        array_ndim: dict[int, int] = {}  # M2b: 1 or 2, threaded to pack / read-back
        for i, t in enumerate(param_types):
            spec = array_param_spec(t)
            if spec is not None:
                array_idx[i], array_ndim[i] = spec
            elif t not in ELEM_BYTES and t not in SCALAR_PARAMS:
                raise ServerFfiError(f"unsupported param type {t} (param {i} of {fn})")
        for i in read_back:
            if i not in list_idx and i not in array_idx:
                raise ServerFfiError(f"read_back index {i} is not a list or array param")
        # #494: refuse caller-side buffer aliasing (the SAME/overlapping object passed to two
        # buffer params, one a mutated out-param) BEFORE any pack -- the list + array marshallers
        # both route through this ONE authority, so both twins are guarded identically.
        check_buffer_aliasing(fn, args, list(list_idx), list(array_idx), read_back)
        if (list_idx or array_idx) and (self.alloc is None or self.heap_ptr is None or self.memory is None):
            raise ServerFfiError(f"{fn} has list/array params but the wasm exports no __alloc/__heap_ptr/memory")
        store = self.store
        # claim the instance BEFORE the first store operation (r4/NEW-1): from here every touch of the
        # Store happens with in_call set, under the same lock close() uses for its check-and-claim
        with self.kernel._stats_lock:
            if self.closed or self.kernel._closed:
                raise ServerFfiError(f"{fn}: this instance/kernel was closed")
            self.in_call = True
        sp: int | None = None
        budget = None
        try:
            if self.kernel.sandbox.metered:
                budget = self.kernel.sandbox.fuel if fuel is None else fuel
                if not isinstance(budget, int) or isinstance(budget, bool) or budget <= 0:
                    raise ValueError(f"fuel must be a positive int, got {budget!r}")
                store.set_fuel(budget)
            elif fuel is not None:
                raise ValueError("per-call fuel needs a metered sandbox: Sandbox(fuel=...)")
            sp = self.heap_ptr.value(store) if self.heap_ptr is not None else 0
            if self.ovf is not None:
                self.ovf.set_value(store, 0)  # a previous overflow that trapped must not poison this call
            # 1. allocate EVERY list first (__alloc may grow memory), 8-byte rounded, alignment asserted
            ptrs: dict[int, int] = {}
            for i in list_idx:
                n = len(args[i])
                size = (HEADER_BYTES + n * ELEM_BYTES[param_types[i]] + 7) & ~7
                try:
                    p = self.alloc(store, size)
                except wt.Trap as e:
                    if e.trap_code == wt.TrapCode.OUT_OF_FUEL:
                        raise FuelExhausted(f"{fn}: fuel budget of {budget} exhausted while allocating list param {i}") from e
                    raise SandboxViolation(f"{fn}: allocating {size} B for list param {i} exceeded the sandbox memory cap ({self.kernel.sandbox.memory_bytes} B): {e.message.splitlines()[0]}") from e
                if (p + HEADER_BYTES) % 8 != 0:
                    raise ServerFfiError(f"list {i} of {fn} landed at unaligned offset {p}; the allocator layout changed")
                if p + size > self.memory.data_len(store):
                    # the compiler's bump allocator hands back a pointer even when memory.grow
                    # was refused by the store limit: catch it HERE, before a single byte is written
                    raise SandboxViolation(f"{fn}: allocating {size} B for list param {i} exceeded the sandbox memory cap ({self.kernel.sandbox.memory_bytes} B): memory.grow refused")
                ptrs[i] = p
            # 2. headers + elements
            for i in list_idx:
                try:
                    self.memory.write(store, self._pack_list(param_types[i], args[i], f"{fn} param {i}"), ptrs[i])
                except IndexError as e:  # pragma: no cover - the bound check above makes this unreachable
                    raise SandboxViolation(f"{fn}: list param {i} does not fit in linear memory (memory cap {self.kernel.sandbox.memory_bytes} B): {e}") from e
            # 2b. M2a-3 array params: pack through the TOTAL runtime marshaller
            # check (array_buffer.pack_array_buffer — the B1 soundness gate: a
            # wrong-dtype / strided / wrong-ndim buffer throws ArrayMarshalError
            # loudly HERE, before a single byte crosses), alloc the ×8-header +
            # element payload, write it in ONE bulk copy.
            array_meta: dict[int, dict] = {}
            for i, dt in array_idx.items():
                payload, meta = _ab.pack_array_buffer(args[i], dt, array_ndim[i])
                array_meta[i] = meta
                size = meta["alloc_size"]  # already a multiple of 8
                try:
                    p = self.alloc(store, size)
                except wt.Trap as e:
                    if e.trap_code == wt.TrapCode.OUT_OF_FUEL:
                        raise FuelExhausted(f"{fn}: fuel budget of {budget} exhausted while allocating array param {i}") from e
                    raise SandboxViolation(f"{fn}: allocating {size} B for array param {i} exceeded the sandbox memory cap ({self.kernel.sandbox.memory_bytes} B): {e.message.splitlines()[0]}") from e
                # The ×8 header needs an 8-aligned BASE so the element region at
                # ptr+16 keeps every dtype's width-alignment (B3).
                if p % 8 != 0:
                    raise ServerFfiError(f"array {i} of {fn} landed at unaligned base {p}; the allocator layout changed")
                if p + size > self.memory.data_len(store):
                    raise SandboxViolation(f"{fn}: allocating {size} B for array param {i} exceeded the sandbox memory cap ({self.kernel.sandbox.memory_bytes} B): memory.grow refused")
                try:
                    self.memory.write(store, payload, p)
                except IndexError as e:  # pragma: no cover
                    raise SandboxViolation(f"{fn}: array param {i} does not fit in linear memory: {e}") from e
                ptrs[i] = p
            # 3. scalars
            call_args = []
            for i, t in enumerate(param_types):
                if i in ptrs:  # a list or array param passes its buffer pointer
                    call_args.append(ptrs[i])
                else:
                    call_args.append(self._scalar_in(t, args[i], f"{fn} param {i}"))
            # 4. the export itself -- a trap propagates, classified
            try:
                raw = f(store, *call_args)
            except wt.Trap as e:
                code = e.trap_code
                if code == wt.TrapCode.OUT_OF_FUEL:
                    raise FuelExhausted(f"{fn}: fuel budget of {budget} exhausted (execution bounded by the sandbox)") from e
                raise WasmTrap(f"{fn}: wasm trap {code.name if code is not None else '?'}: {e.message.splitlines()[0]}") from e
            except wt.WasmtimeError as e:
                raise WasmTrap(f"{fn}: {str(e).splitlines()[0]}") from e
            if self.ovf is not None and self.ovf.value(store):
                self.ovf.set_value(store, 0)
                raise OverflowError(f"{fn}: i64 overflow inside the WASM kernel (refused; no arbitrary-precision re-run on this path)")
            # 5. read the out-buffers back BEFORE releasing the heap
            outs: dict[int, Any] = {}
            for i in read_back:
                if i in array_idx:
                    # M2a-3 array write-back: read the full payload (header +
                    # elements) back, re-validate its header against the
                    # compiled-for (dtype, ndim, n) via read_back_element_bytes
                    # (drift/corruption is REFUSED, never truncated), and bulk
                    # copy the element region INTO the caller's out-buffer in
                    # place through the total check (unpack_array_into). The
                    # caller's buffer is mutated directly — the out-buffer
                    # contract — so `outs[i]` is left None (run_server must not
                    # slice-assign an array).
                    dt = array_idx[i]
                    n = array_meta[i]["n"]
                    total = _ab.array_alloc_size(n, dt)
                    payload = bytes(self.memory.read(store, ptrs[i], ptrs[i] + total))
                    nd = array_ndim[i]
                    elem_bytes = _ab.read_back_element_bytes(payload, dt, nd, n)
                    _ab.unpack_array_into(elem_bytes, args[i], dt, nd)
                    outs[i] = None
                    continue
                n = len(args[i])
                t = param_types[i]
                start = ptrs[i] + HEADER_BYTES
                raw_bytes = self.memory.read(store, start, start + n * ELEM_BYTES[t])
                vals = list(struct.unpack(f"<{n}{_ELEM_FMT[t]}", raw_bytes))
                outs[i] = [bool(v) for v in vals] if t == "list[bool]" else vals
            if return_type == "int":
                value: Any = int(raw)
            elif return_type == "float":
                value = float(raw)
            elif return_type == "bool":
                value = bool(raw)
            else:
                value = None
            fuel_used = (budget - store.get_fuel()) if budget is not None else None
            heap = self.memory.data_len(store) if self.memory is not None else 0
            return CallResult(value=value, outs=outs, fuel_used=fuel_used, heap_bytes=heap)
        finally:
            # cleanup store ops inside the in-call window; a failure here must never pin in_call (r5/R5-3),
            # and each op is guarded on its own so one failure does not skip the other (r6/R6-5)
            try:
                if self.heap_ptr is not None and sp is not None:  # sp is None iff the prologue raised before reading it
                    self.heap_ptr.set_value(store, sp)
            except Exception:  # pragma: no cover
                pass
            try:
                if self.ovf is not None:
                    self.ovf.set_value(store, 0)
            except Exception:  # pragma: no cover
                pass
            with self.kernel._stats_lock:
                self.in_call = False
                closed_meanwhile = self.kernel._closed
            if closed_meanwhile:  # closed while we were inside the call: release now, on the owner thread
                self.close()

    def memory_bytes(self, start: int, stop: int) -> bytes:
        """Raw linear memory (the K7 byte-identity test dumps a written list with this)."""
        return bytes(self.memory.read(self.store, start, stop))


_kernels = weakref.WeakSet()
atexit.register(_shutdown)


def mutated_list_params(node) -> frozenset[str]:
    """Names of list params the kernel assigns INTO (`out[i] = ...`, `out[i] += ...`): the
    server path reads exactly those back and writes them into the caller's lists in place, so
    the Python semantics (mutation visible to the caller) are preserved. Static, from the same
    statically-checked def the build compiled."""
    import ast

    names: set[str] = set()

    def base_name(t) -> str | None:
        while isinstance(t, ast.Subscript):
            t = t.value
        return t.id if isinstance(t, ast.Name) else None

    def targets(t) -> None:
        if isinstance(t, ast.Subscript):
            nm = base_name(t)
            if nm:
                names.add(nm)
        elif isinstance(t, (ast.Tuple, ast.List)):
            for e in t.elts:
                targets(e)
        elif isinstance(t, ast.Starred):
            targets(t.value)

    for sub in ast.walk(node):
        if isinstance(sub, ast.Assign):
            for t in sub.targets:
                targets(t)
        elif isinstance(sub, (ast.AugAssign, ast.AnnAssign)):
            targets(sub.target)
        elif isinstance(sub, (ast.For, ast.comprehension)):
            targets(sub.target)
        elif isinstance(sub, ast.Call) and isinstance(sub.func, ast.Attribute):
            nm = base_name(sub.func.value)
            if nm and sub.func.attr in {"append", "extend", "insert", "pop", "clear", "sort", "reverse", "remove"}:
                names.add(nm)
    params = {a.arg for a in node.args.posonlyargs + node.args.args}
    return frozenset(n for n in names if n in params)


_LENGTH_CHANGING = {"append", "extend", "insert", "pop", "clear", "remove", "sort", "reverse"}
# builtins that only READ a list (or copy it): a list parameter may be passed to these
_READ_ONLY_BUILTINS = {"len", "min", "max", "sum", "abs", "any", "all", "sorted", "list", "tuple", "enumerate", "zip",
                       "reversed", "iter", "range", "float", "int", "bool", "str", "print", "isinstance"}


def unsupported_list_use(node) -> str | None:
    """A reason the server path must NOT bind this kernel, or None. The read-back honours
    exactly one shape -- element assignment INTO a list parameter through its own name, with
    the length unchanged. Anything the read-back could get silently wrong is refused here
    (opus m1.5 r1/S2), never ignored: a list parameter that is ALIASED (`tmp = out`), REBOUND
    (`out = ...`), passed to a call, deleted from, slice-assigned, or mutated by a
    length-changing method.

    Completeness note (opus m1.5 r2/NEW-7, corrected r3/N7): the predicate is syntactic and
    checks every BINDING SITE a list parameter can flow through -- assignment / annotated /
    walrus values, loop iterables (`for tmp in [out]`), call arguments, and return values --
    via `flows_out`; a nested-list box (`box = [out]; box[0][0] = k`) compiles and is caught by
    the assignment arm; a tuple-RHS alias `tmp, n = out, len(out)` is caught by the same arm
    (flows_out walks the RHS tuple) AND refused by the compiler at pyths 0.2.4. Assignment TARGETS
    are checked too (`target_leak`, r5/R5-1): `[out][0][0] = v` and its literal/tuple/nested/
    augmented siblings compile at 0.2.4 and are refused here. The rule is positional, not a list of
    shapes: every expression of every statement -- values, targets, iterables, call arguments,
    returns, bare expressions -- is walked, and a list parameter may appear only indexed by an element (never a slice), iterated,
    tested for membership (`x in out`), or passed to a read-only builtin. A future tier that admits new statement kinds must
    add them to the walk."""
    import ast

    # v0.2.5 M2c: the SAME aliasing/rebinding/length-change hazards apply to a numeric
    # `Array[dtype, ndim]` out-parameter (the server writes it back IN PLACE through the total
    # marshaller, so an alias / rebinding / non-standard mutation would be lost exactly as for a
    # list). Guard both; an over-broad refusal here is SOUND (the kernel falls to the Python/browser
    # path, still correct) — never a silent wrong write-back. `.shape`/attribute use on an array
    # param is treated as a flow-out (conservatively refused from the server fast path).
    list_params = {
        a.arg for a in node.args.posonlyargs + node.args.args
        if a.annotation is not None
        and (ast.unparse(a.annotation).startswith("list[") or ast.unparse(a.annotation).startswith("Array["))
    }
    if not list_params:
        return None

    def is_param(e) -> bool:
        return isinstance(e, ast.Name) and e.id in list_params

    def base_name(t) -> str | None:
        while isinstance(t, ast.Subscript):
            t = t.value
        return t.id if isinstance(t, ast.Name) else None

    def rebinds(t) -> str | None:
        if isinstance(t, ast.Name) and t.id in list_params:
            return t.id
        if isinstance(t, (ast.Tuple, ast.List)):
            for e in t.elts:
                r = rebinds(e)
                if r:
                    return r
        if isinstance(t, ast.Starred):
            return rebinds(t.value)
        return None

    def flows_out(expr) -> str | None:
        """A list param used as a VALUE anywhere in `expr` (not merely indexed by an element, iterated, tested for membership
        or passed to a read-only builtin) may create an alias the read-back cannot see -- e.g.
        `tmp = out if c else out`, `tmp = [out][0]`, `tmp = out or other` (codex m1.5 r2/#1)."""
        allowed_ids: set[int] = set()
        for e in ast.walk(expr):
            if isinstance(e, ast.Subscript) and is_param(e.value) and not isinstance(e.slice, ast.Slice):
                allowed_ids.add(id(e.value))  # out[i]: an element read (a SLICE is a list value -- r8/N8-2 -- not admitted;
                # NOTE r9/N9-8: on a nested list[list[int]] param `out[0][0:2]` would be an inner slice -- compiler-refused at 0.2.4)
            elif isinstance(e, ast.Call) and isinstance(e.func, ast.Name) and e.func.id in _READ_ONLY_BUILTINS:
                for a in e.args:
                    if is_param(a):
                        allowed_ids.add(id(a))  # len(out)
            elif isinstance(e, ast.Compare):
                # ONLY `x in out` / `x not in out` (the param as the RIGHT operand of a membership test)
                # is an element-wise read. A list param on either side of ==/!=/</<=/>/>= (or as the
                # LEFT operand of `in`) is a list-to-list comparison, which the WASM lowering performs on
                # HANDLES, not contents (opus m1.5 r7/NEW-1: `if out == o:` decided the wrong branch and
                # corrupted the caller's list) -- refused, and recorded upstream.
                for op, c in zip(e.ops, e.comparators):
                    if isinstance(op, (ast.In, ast.NotIn)) and is_param(c):
                        allowed_ids.add(id(c))
        for e in ast.walk(expr):
            if is_param(e) and id(e) not in allowed_ids:
                return e.id
        return None

    def target_leak(t) -> str | None:
        """An assignment TARGET may reach a list param ONLY as a subscript chain rooted at the
        param's own name (`out[i]`, `out[i][j]` -- the mutation the read-back honours). A target
        that reaches it any other way (`[out][0][0] = v`, `(out,)[0][0] = v`, `[[out]][0][0][0]`)
        is a boxed alias the read-back cannot see (opus m1.5 r5/R5-1)."""
        if rebinds(t):
            return None  # a plain rebinding (`out = ...`) is reported by the rebinding arm with its own message
        if isinstance(t, (ast.Tuple, ast.List)):
            for e in t.elts:
                r = target_leak(e)
                if r:
                    return r
            return None
        if isinstance(t, ast.Starred):
            return target_leak(t.value)
        if isinstance(t, ast.Subscript) and base_name(t) in list_params:
            # the legitimate shape; the INDEX expressions may still carry an alias (`out[[out][0][0]]`)
            n = t
            while isinstance(n, ast.Subscript):
                r = flows_out(n.slice)
                if r:
                    return r
                n = n.value
            return None
        return flows_out(t)

    def call_arm_finding(v) -> bool:
        """A DIRECT list-param argument to a call the Call arm will REPORT (not a read-only builtin,
        which that arm admits) -- the one case the generic walk defers so the message stays
        "passed to a call" (r6/R6-2: a read-only builtin never qualifies, so `print(out, [out])`
        is still refused by the walk)."""
        return (isinstance(v, ast.Call)
                and not (isinstance(v.func, ast.Name) and v.func.id in _READ_ONLY_BUILTINS)
                and (any(is_param(a) for a in v.args) or any(is_param(kw.value) for kw in v.keywords)))

    # fields that BIND (target position): routed through target_leak; every other expression child
    # of a statement-like node is a VALUE position: routed through flows_out (r6/R6-1 -- positional,
    # not a list of shapes; `for`/`if`/`while`/`assert`/`del`/`with`/`except`/`match` included)
    target_fields = {ast.Assign: {"targets"}, ast.AnnAssign: {"target"}, ast.AugAssign: {"target"}, ast.For: {"target"},
                     ast.AsyncFor: {"target"}, ast.comprehension: {"target"}, ast.withitem: {"optional_vars"},
                     ast.NamedExpr: {"target"}, ast.Delete: {"targets"}}
    stmt_like = (ast.stmt, ast.comprehension, ast.withitem, ast.ExceptHandler, ast.match_case, ast.NamedExpr)

    for sub in ast.walk(node):
        # ---- specific arms first: they own the precise messages ------------------------------------
        if isinstance(sub, (ast.Assign, ast.AnnAssign, ast.NamedExpr)):
            value = sub.value
            targets = sub.targets if isinstance(sub, ast.Assign) else [sub.target]
            if value is not None:
                leaked = flows_out(value)
                if leaked:
                    return f"list parameter `{leaked}` is aliased (it flows into `{ast.unparse(value)}`): writes through the alias could not be read back"
            for t in targets:
                r = rebinds(t)
                if r:
                    return f"list parameter `{r}` is rebound inside the kernel: the caller's list would no longer be the one mutated"
                if isinstance(t, ast.Subscript) and isinstance(t.slice, ast.Slice) and base_name(t) in list_params:
                    return f"slice assignment into list parameter `{base_name(t)}` may change its length"
            for t in targets:
                leaked = target_leak(t)
                if leaked:
                    return f"list parameter `{leaked}` is aliased through the assignment target `{ast.unparse(t)}`: a boxed write could not be read back"
        elif isinstance(sub, (ast.AugAssign, ast.For, ast.AsyncFor, ast.comprehension)):
            r = rebinds(sub.target)
            if r:
                return f"list parameter `{r}` is rebound inside the kernel"
            t = sub.target
            if isinstance(sub, ast.AugAssign) and isinstance(t, ast.Subscript) and isinstance(t.slice, ast.Slice) and base_name(t) in list_params:
                return f"slice assignment into list parameter `{base_name(t)}` may change its length"
            leaked = target_leak(t)
            if leaked:
                return f"list parameter `{leaked}` is aliased through the assignment target `{ast.unparse(t)}`: a boxed write could not be read back"
            # the THIRD binding site (opus m1.5 r3/N1): `for tmp in [out]:` binds tmp to the list itself;
            # `for x in out:` yields ELEMENTS and is fine -- reuse flows_out (len(out)/range(len(out)) stay allowed).
            # (A comprehension's iterable is reached first by the positional walk of its enclosing statement,
            # so this arm is only ever the `For` statement's -- r7/NEW-3.)
            if isinstance(sub, (ast.For, ast.AsyncFor)) and not is_param(sub.iter):
                leaked = flows_out(sub.iter)
                if leaked:
                    return f"list parameter `{leaked}` is aliased: it flows into a loop iterable (`{ast.unparse(sub.iter)}`), so the loop variable may alias it"
        elif isinstance(sub, ast.Delete):
            for t in sub.targets:
                if base_name(t) in list_params:
                    return f"`del` on list parameter `{base_name(t)}` changes its length"
        elif isinstance(sub, ast.Call):
            if isinstance(sub.func, ast.Attribute) and is_param(sub.func.value) and sub.func.attr in _LENGTH_CHANGING:
                return f"`{sub.func.value.id}.{sub.func.attr}(...)` changes/reorders list parameter `{sub.func.value.id}` in a way the read-back cannot honour"
            if call_arm_finding(sub):
                who = next((a for a in sub.args if is_param(a)), None) or next(kw.value for kw in sub.keywords if is_param(kw.value))
                return f"list parameter `{who.id}` is passed to a call: mutation through the callee could not be read back"
        elif isinstance(sub, ast.Return) and sub.value is not None:
            leaked = flows_out(sub.value)
            if leaked:
                return f"list parameter `{leaked}` is returned (or flows into the return value)"
        elif isinstance(sub, ast.Expr):  # a bare expression statement (`print([out])`) -- r5/R5-2, latent today
            if not call_arm_finding(sub.value):
                leaked = flows_out(sub.value)
                if leaked:
                    return f"list parameter `{leaked}` is aliased: it flows into a bare expression `{ast.unparse(sub.value)}` (a loop iterable or a call), so an alias could escape"
        elif isinstance(sub, ast.ExceptHandler) and sub.name in list_params:
            return f"list parameter `{sub.name}` is rebound by `except ... as {sub.name}`"
        elif isinstance(sub, ast.match_case):
            for pat in ast.walk(sub.pattern):
                nm = getattr(pat, "name", None) or getattr(pat, "rest", None)
                if isinstance(nm, str) and nm in list_params:
                    return f"list parameter `{nm}` is rebound by a match pattern"
        # ---- the positional walk: EVERY immediate expression child of a statement-like node ---------
        if isinstance(sub, stmt_like):
            tf = target_fields.get(type(sub), set())
            # target fields FIRST so a rebinding is diagnosed as such (`with o as out` names `out`, not `o` -- r7/NEW-4)
            fields = sorted(ast.iter_fields(sub), key=lambda fv: 0 if fv[0] in tf else 1)
            for field, value in fields:
                for ch in (value if isinstance(value, list) else [value]):
                    if not isinstance(ch, ast.expr):
                        continue
                    if field in tf:
                        r = rebinds(ch)
                        if r:
                            return f"list parameter `{r}` is rebound in the `{field}` position of a `{type(sub).__name__}`"
                        leaked = target_leak(ch)
                    elif field == "iter" and is_param(ch):
                        leaked = None  # `for x in out:` yields elements
                    elif isinstance(sub, ast.Expr) and call_arm_finding(ch):
                        leaked = None  # reported by the Call arm (visited next by ast.walk)
                    else:
                        leaked = flows_out(ch)
                    if leaked:
                        return (f"list parameter `{leaked}` reaches `{ast.unparse(ch)}` in the `{field}` position of a "
                                f"`{type(sub).__name__}`: a list parameter may only be indexed, iterated, tested for membership (`x in out`), or passed to a read-only builtin")
    return None


def bind_positional(fn, args: tuple, kwargs: dict) -> list[Any]:
    """Map a Python call (positional and/or keyword) onto the kernel's positional parameter
    order, applying defaults, so the server path accepts every call the Python body would."""
    sig = inspect.signature(fn)
    bound = sig.bind(*args, **kwargs)
    bound.apply_defaults()
    return [bound.arguments[p] for p in sig.parameters]
