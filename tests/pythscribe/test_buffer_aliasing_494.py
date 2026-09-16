"""Issue #494 -- caller-side buffer ALIASING on the SERVER path is refused LOUDLY, never
silently miscomputed.

CPython aliases the SAME object when a buffer is passed to two params (`shift_arr(x, x)`, or a
NumPy view `shift_arr(x[:], x)`); the server path marshals EACH buffer param to an INDEPENDENT
WASM linear-memory buffer, so a mutated out-param that shares one Python object with another
param diverges SILENTLY (last-writer-wins). The fix routes both the `list[...]` and the
`Array[...]` marshallers through ONE aliasing authority (`check_buffer_aliasing`) that refuses
the call (`PythscribeAliasingError`) BEFORE any byte is packed.

The overlap primitive is memory-footprint overlap (`_buffer_extent` / `_buffers_overlap`),
computed via the C buffer API (`PyObject_GetBuffer` with `PyBUF_INDIRECT`, 0x118) from each
buffer's `(start_address, nbytes)` footprint using the `numpy.byte_bounds` shape/stride rule. It is
TOTAL over every buffer-protocol object -- writable, read-only, contiguous OR STRIDED, NumPy or
not -- so it is sound where `numpy.shares_memory` alone is not: it does not fail open when
`shares_memory` raises, it catches two DIFFERENT exporter objects over one `bytearray`, and --
the regression this closes -- it catches a STRIDED view (`x[::2]`) reaching the check through
the LIST sink (which iterates any sequence) where a contiguous-only probe returns "no extent"
and aliases silently. Disjoint slices (`mv[:2]` / `mv[2:]`) are NOT over-refused; `None` now
means a genuine non-buffer (a `list`) or a PIL-style indirect buffer, where identity is
complete.

The LIST-SINK ADMISSION (`_admit_list_arg`) is the ROOT of the wrapper class: `_pack_list`
marshals through the object's OWN `__len__`/`__iter__`/`__getitem__`, so admission binds THAT
channel -- while an out-param is mutated, a `list[...]` argument is admitted IFF it is an EXACT
`list`/`tuple`/`range`, or a builtin-based buffer (memoryview/bytes/bytearray/array.array/
ndarray/ctypes array) whose sink slots are UNOVERRIDDEN and which is extent-addressable.
Everything else -- a `list`/`tuple` subclass, a ctypes-array subclass overriding `__getitem__`,
an `__array__` wrapper whose `__array__` returns a COPY while `__getitem__` reads live memory,
`pandas.Series` -- is refused LOUDLY. (The earlier `__array__`/`np.asarray` admission tier was
UNSOUND: it checked a DIFFERENT channel than the marshaller reads.)

Every property ships a POSITIVE control (the non-aliased path still computes correctly) AND a
REFUSAL control (the aliased call goes RED), plus MUTATION-VERIFY witnesses proving the guard
is genuine: with the authority stubbed to a no-op, the aliased call returns a value that
DIVERGES from the CPython-aliased reference -- i.e. the row that now RAISES would otherwise
silently miscompute. (feedback_anti_vacuity_paired_control: an unpaired gate is a blocker.)
"""
from __future__ import annotations

import ctypes
import subprocess
import sys

import pytest

from conftest import gate, gate_import

numpy = gate_import("numpy")
import numpy as np  # noqa: E402

wasmtime = gate_import("wasmtime")

import pythscribe.runtime as rt  # noqa: E402
from pythscribe.build import find_pyths  # noqa: E402
from pythscribe.runtime import (  # noqa: E402
    PythscribeAliasingError,
    ServerFfiError,
    ServerKernel,
    _admit_list_arg,
    _buffer_extent,
    _buffers_overlap,
    check_buffer_aliasing,
    wasmtime_available,
)
from pythscribe.runtime.array_buffer import ArrayMarshalError, check_array_buffer  # noqa: E402


def _byte_bounds():
    """`numpy.byte_bounds`, version-tolerant (SF-2): it moved to `numpy.lib.array_utils` in
    numpy 2.0 and is `numpy.byte_bounds` on 1.x. The shipped runtime never imports it; the
    footprint rows below use it ONLY as the oracle the `_buffer_extent` footprint must equal --
    an ImportError here would silently skip the negative-stride anti-vacuity control on 1.x."""
    try:
        from numpy.lib.array_utils import byte_bounds   # numpy >= 2.0
    except ImportError:
        from numpy import byte_bounds                    # numpy < 2.0
    return byte_bounds


class _SeqWrap:
    """A generic sequence over an array: the list sink can ITERATE it (so it reaches the FFI),
    but it exports NO buffer and NO `__array__`, so the aliasing authority cannot introspect it.
    The admission gate must refuse it LOUDLY when a mutated out-param is present -- never marshal
    it to a silent independent copy that hides an alias of the out-buffer."""

    def __init__(self, a):
        self._a = a

    def __len__(self):
        return len(self._a)

    def __getitem__(self, i):
        return float(self._a[i])


# --- the wrapper class the ROOT admission closes (B1 / B1' / B2) -------------------------------
# Each reads a FOREIGN live buffer (the mutated out) through the very slots `_pack_list` uses,
# while whatever storage the address authority can see is DISJOINT from (or invisible for) the
# out -- so the overlap arm alone can NEVER catch them; only binding the marshaller's channel can.

class _LiveList(list):
    """B1: a `list` SUBCLASS whose `__iter__`/`__getitem__` read the out-buffer live. Its own
    C-level storage holds zeros (what a `type(v) in (list,...)`-by-isinstance admission would
    trust); `len()` is the inherited 4."""

    def __init__(self, live, n=4):
        super().__init__([0.0] * n)
        self._live, self._n = live, n

    def __iter__(self):
        return (float(self._live[i]) for i in range(self._n))

    def __getitem__(self, i):
        if i >= self._n:
            raise IndexError(i)
        return float(self._live[i])


class _LiveTuple(tuple):
    """B1 (tuple twin): same shape on a `tuple` subclass."""

    def __new__(cls, live, n=4):
        self = super().__new__(cls, (0.0,) * n)
        self._live, self._n = live, n
        return self

    def __iter__(self):
        return (float(self._live[i]) for i in range(self._n))

    def __getitem__(self, i):
        if i >= self._n:
            raise IndexError(i)
        return float(self._live[i])


class _LiveCtypes(ctypes.c_double * 4):
    """B1' (buffer twin): a ctypes-array SUBCLASS overriding `__getitem__`. `ctypes.Array` has NO
    `__iter__`, so `enumerate()` iterates through `__getitem__` -- the override IS the channel.
    Its OWN buffer (4 zero doubles, what `PyObject_GetBuffer` addresses) is disjoint from the out,
    so the footprint authority sees no overlap; the values marshalled come from the out, live."""

    def __getitem__(self, i):
        if i >= 4:
            raise IndexError(i)
        return float(self._live[i])


class _CopyArrayLiveItems:
    """B2: `__array__` returns a COPY (so `np.asarray(w)` is a fresh, disjoint buffer) while
    `__len__`/`__iter__`/`__getitem__` read the out live. The dropped `__array__` admission tier
    admitted this object (asarray footprint disjoint => 'no alias') and marshalled it SILENTLY."""

    def __init__(self, a, n=4):
        self._a, self._n = a, n

    def __array__(self, dtype=None, copy=None):
        return np.array(self._a[: self._n], dtype=dtype)  # a COPY, never the live storage

    def __len__(self):
        return self._n

    def __iter__(self):
        return (float(self._a[i]) for i in range(self._n))

    def __getitem__(self, i):
        if i >= self._n:
            raise IndexError(i)
        return float(self._a[i])


# --- the BUFFER-PROTOCOL channel (SF-1): PEP 688 `__buffer__` / `__release_buffer__` ----------
# Rule (b) rests on the AUTHORITY (`_buffer_extent`, via `PyObject_GetBuffer`) and the MARSHALLER
# (`__iter__`/`__getitem__`/`__len__`) reading ONE storage. A subclass that overrides the buffer
# protocol -- and NO iteration slot -- desyncs them: the three-slot walk alone admitted it.

_DECOY = bytearray(64)                          # a disjoint allocation the hijacked authority sees
_ND_DECOY = np.zeros(8, dtype=np.float64)       # its ndarray twin


class _EvilBA(bytearray):
    """The SF-1 repro, verbatim: a `bytearray` subclass whose `__buffer__` hands the authority a
    DECOY view while the inherited `__iter__` reads the REAL storage."""

    def __buffer__(self, flags):
        return memoryview(_DECOY)


class _EvilNd(np.ndarray):
    """The ALIASING witness for the channel: `x.view(_EvilNd)` SHARES the out's storage (so its
    iteration reads the out live) while `__buffer__` points the authority at a DISJOINT decoy --
    the footprint arm sees no overlap, so only binding `__buffer__` can refuse it."""

    def __buffer__(self, flags):
        return memoryview(_ND_DECOY)


class _RelBA(bytearray):
    """`__release_buffer__`-ONLY override (no `__buffer__`), the defense-in-depth arm."""

    def __release_buffer__(self, view):
        view.release()


class _RelNd(np.ndarray):
    """`__release_buffer__`-ONLY override on an OWNING ndarray subclass whose release hook
    RELOCATES the storage (`resize(refcheck=False)`) -- i.e. arbitrary Python runs between the
    authority's address read and the marshaller's iteration, and the certified address is stale
    by the time the marshaller reads. The concrete reason release-only is disqualifying."""

    def __release_buffer__(self, view):
        view.release()
        self.resize((4096,), refcheck=False)


def _live_wrappers(x):
    """The four B1/B1'/B2 shapes over the SAME live buffer `x`, labelled."""
    c = _LiveCtypes()
    c._live = x
    return [("list subclass", _LiveList(x)), ("tuple subclass", _LiveTuple(x)),
            ("ctypes-array subclass", c), ("__array__-copy wrapper", _CopyArrayLiveItems(x))]


KERNEL_SRC = """
def shift_arr(src: Array[int32], out: Array[int32]) -> None:
    for i in range(len(src)):
        out[i] = src[i] + 1

def reverse_into(src: Array[int32], out: Array[int32], n: int) -> None:
    for i in range(n):
        out[i] = src[n - 1 - i]

def add_pair(a: Array[int32], b: Array[int32]) -> int:
    s = 0
    for i in range(len(a)):
        s = s + a[i] + b[i]
    return s

def combine3(a: Array[int32], b: Array[int32], out: Array[int32], n: int) -> None:
    for i in range(n):
        out[i] = a[i] + b[n - 1 - i]

def dup2(src: Array[int32], o1: Array[int32], o2: Array[int32], n: int) -> None:
    for i in range(n):
        o1[i] = src[i]
        o2[i] = src[n - 1 - i]

def flip_rows(a: Array[int32, 2], out: Array[int32, 2]) -> None:
    for i in range(len(a)):
        for j in range(a.shape[1]):
            out[i][j] = a[len(a) - 1 - i][j]

def mixed(lst: list[int], a: Array[int32], out: Array[int32]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + lst[0]

def shift_list(src: list[int], out: list[int]) -> None:
    for i in range(len(src)):
        out[i] = src[i] + 1

def reverse_list(src: list[int], out: list[int], n: int) -> None:
    for i in range(n):
        out[i] = src[n - 1 - i]

def reverse_u8(src: Array[uint8], out: Array[uint8], n: int) -> None:
    for i in range(n):
        out[i] = src[n - 1 - i]

def rev_f(src: list[float], out: Array[float64], n: int) -> None:
    for i in range(n):
        out[i] = src[n - 1 - i]
"""


@pytest.fixture(scope="module")
def kernel(tmp_path_factory) -> ServerKernel:
    gate(wasmtime_available(), "wasmtime-py is required for the server path")
    d = tmp_path_factory.mktemp("aliasing494")
    src = d / "k.ps"
    src.write_text(KERNEL_SRC)
    wasm = d / "k.wasm"
    r = subprocess.run(
        [str(find_pyths()), "compile", str(src), "--target", "wasm", "-o", str(wasm)],
        capture_output=True, text=True,
    )
    assert r.returncode == 0, f"pyths compile failed: {r.stderr}\n{r.stdout}"
    assert wasm.is_file() and wasm.stat().st_size > 0, "no .wasm emitted"
    k = ServerKernel.from_wasm(wasm, name="k")
    for fn in ("shift_arr", "reverse_into", "add_pair", "combine3", "dup2", "flip_rows",
               "mixed", "shift_list", "reverse_list", "reverse_u8", "rev_f"):
        assert fn in k.exports, f"{fn} not WASM-admitted: {k.exports}"
    return k


@pytest.fixture(scope="module")
def wasm_module(tmp_path_factory):
    """A real @wasm-decorated module bound to the server path via compile-on-first-call, so the
    aliasing guard can be exercised through the PUBLIC decorator sink (repro A), not only through
    ServerKernel.call."""
    import importlib.util
    gate(wasmtime_available(), "wasmtime-py is required for the server path")
    gate(find_pyths().is_file(), "pyths compiler is required for compile-on-first-call")
    d = tmp_path_factory.mktemp("decomod494")
    mod_path = d / "kmod494.py"
    mod_path.write_text(
        "from __future__ import annotations\n"
        "from pythscribe import wasm\n\n"
        "@wasm\n"
        "def rev_f(src: list[float], out: Array[float64], n: int) -> None:\n"
        "    for i in range(n):\n"
        "        out[i] = src[n - 1 - i]\n"
    )
    spec = importlib.util.spec_from_file_location("kmod494", mod_path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules["kmod494"] = mod
    try:
        spec.loader.exec_module(mod)
        # warm compile-on-first-call with SAFE (non-aliased) args so the server path binds.
        mod.rev_f(np.arange(8, dtype=np.float64)[::2].copy(), np.zeros(4, dtype=np.float64), 4)
        from pythscribe import binding_of
        b = binding_of(mod.rev_f)
        assert b.mode == "server", f"rev_f must bind the server path (got mode={b.mode}: {b.mode_reason})"
        yield mod
    finally:
        sys.modules.pop("kmod494", None)  # do not leak the temp module across the test session


# =====================================================================================
# Unit tests on the overlap PRIMITIVE (no kernel) -- the two codex blockers + edges.
# =====================================================================================

def test_primitive_identity_and_numpy_view():
    x = np.zeros(4, dtype=np.int32)
    assert _buffers_overlap(x, x) is True                       # identity
    assert x[:] is not x and _buffers_overlap(x[:], x) is True  # view shares memory


def test_primitive_distinct_and_equal_valued_not_flagged():
    # distinct allocations never overlap -- NOT value-based.
    assert _buffers_overlap(np.zeros(4, np.int32), np.zeros(4, np.int32)) is False
    assert _buffers_overlap(np.array([1, 2, 3], np.int32), np.array([1, 2, 3], np.int32)) is False
    assert _buffers_overlap([1, 2, 3], [1, 2, 3]) is False      # two distinct lists


def test_primitive_partial_overlap_and_disjoint_slices():
    base = np.arange(8, dtype=np.int32)
    assert _buffers_overlap(base[0:5], base[3:8]) is True       # partial overlap -> refuse
    assert _buffers_overlap(base[0:4], base[4:8]) is False      # disjoint -> NOT over-refused


def test_primitive_strided_views_footprint_overlap():
    # The regression the PyBUF_INDIRECT footprint fixes: a STRIDED view of a buffer shares its
    # memory footprint and MUST be flagged (a contiguous-only PyBUF_SIMPLE probe returned None
    # here -> silent alias). Includes the two repro shapes.
    x = np.arange(8, dtype=np.float64)
    assert _buffers_overlap(x[::2], x) is True                  # repro A: strided ndarray view
    ba = bytearray(8)
    assert _buffers_overlap(memoryview(ba)[::2], ba) is True    # repro B: strided memoryview
    assert _buffers_overlap(x[::-1], x) is True                 # reversed view
    # striding does not cause blanket over-refusal: two contiguous halves are disjoint.
    assert _buffers_overlap(x[0:4], x[4:8]) is False
    # a strided view's footprint matches numpy.byte_bounds exactly.
    byte_bounds = _byte_bounds()
    e = _buffer_extent(x[::2]); lo, hi = byte_bounds(x[::2])
    assert e == (lo, hi - lo)


def test_primitive_negative_stride_footprint_arm():
    # BLOCKER 2 (anti-vacuity): the NEGATIVE-stride arm of the footprint is DISCRIMINATING here
    # -- x[3::-1] = [x3,x2,x1,x0] spans bytes [x0, x3]; dropping the `stride<0 -> low` arm makes
    # its computed nbytes negative -> overlap False. So this row goes RED if that arm is removed.
    byte_bounds = _byte_bounds()
    x = np.arange(8, dtype=np.float64)
    assert _buffers_overlap(x[3::-1], x[0:2]) is True
    for label, arr in (("x[::-1]", x[::-1]),
                       ("nd.T[::-1]", np.arange(12, dtype=np.int32).reshape(3, 4).T[::-1]),
                       ("3-D mixed sign", np.arange(24, dtype=np.int16).reshape(2, 3, 4)[::-1, :, ::-1])):
        e = _buffer_extent(arr); lo, hi = byte_bounds(arr)
        assert e == (lo, hi - lo), f"footprint != byte_bounds for {label}: {e} vs {(lo, hi - lo)}"


def test_primitive_overlap_independent_of_shares_memory(monkeypatch):
    # Regression row: the primitive decides overlap from the C buffer API, never from
    # numpy.shares_memory (which an earlier revision used and could fail open on). Stub it to
    # always raise -- detection is unaffected.
    def boom(*a, **k):
        raise RuntimeError("shares_memory must never be on the path")
    monkeypatch.setattr(np, "shares_memory", boom)
    x = np.arange(6, dtype=np.int32)
    assert _buffers_overlap(x[:], x) is True
    assert _buffers_overlap(x[0:4], x[2:6]) is True
    assert _buffers_overlap(np.zeros(4, np.int32), np.zeros(4, np.int32)) is False


def test_primitive_blocker2_different_exporters_same_storage():
    # BLOCKER 2: two DIFFERENT exporter objects over the SAME bytearray have different
    # memoryview.obj identities but overlapping bytes -> must be detected by address range.
    ba = bytearray(4)
    c1 = (ctypes.c_ubyte * 4).from_buffer(ba)
    c2 = (ctypes.c_ubyte * 4).from_buffer(ba)
    assert memoryview(c1).obj is not memoryview(c2).obj  # the exporter-identity fallback would MISS this
    assert _buffers_overlap(c1, c2) is True


def test_primitive_numpy_absent_uses_c_buffer_api(monkeypatch):
    # numpy-ABSENT path: the total PyObject_GetBuffer primitive is NumPy-free, so overlapping
    # memoryviews are detected and disjoint ones are not.
    monkeypatch.setitem(sys.modules, "numpy", None)
    ba = bytearray(8)
    mv = memoryview(ba)
    assert _buffers_overlap(mv[0:5], mv[3:8]) is True
    assert _buffers_overlap(mv[0:4], mv[4:8]) is False
    c1 = (ctypes.c_ubyte * 4).from_buffer(ba)
    c2 = (ctypes.c_ubyte * 4).from_buffer(ba)
    assert _buffers_overlap(c1, c2) is True


def test_primitive_numpy_absent_readonly_vs_writable_same_storage(monkeypatch):
    # BLOCKER (Fable): a READ-ONLY exporter vs a DIFFERENT WRITABLE exporter over the same
    # storage, with NumPy ABSENT. The old numpy/frombuffer/_mv_root tiers returned None/False
    # here (fail-OPEN = silent wrong). The total C-buffer-API primitive addresses BOTH ->
    # overlap detected. Paired with the E2E RED control below.
    monkeypatch.setitem(sys.modules, "numpy", None)
    ba = bytearray(4)
    ro = memoryview(ba).toreadonly()                 # read-only exporter
    wr = (ctypes.c_ubyte * 4).from_buffer(ba)        # different writable exporter, same bytes
    assert memoryview(ro).obj is not memoryview(wr).obj
    assert _buffers_overlap(ro, wr) is True
    assert _buffers_overlap(wr, ro) is True          # symmetric


def test_primitive_array_wrapper_and_nonbuffer_wrapper():
    # The overlap PRIMITIVE still resolves an __array__ wrapper (pandas.Series copy=False) via
    # np.asarray as EXTRA detection -- but that is NOT an admission path any more (see the
    # admission rows below: every Series is refused by `_admit_list_arg` before overlap runs).
    # A pure Sequence wrapper (no __array__, no buffer) resolves to None.
    pd = gate_import("pandas")
    import pandas as pd  # noqa: F811
    x = np.arange(8, dtype=np.float64)
    assert _buffers_overlap(pd.Series(x, copy=False), x) is True          # no-copy wrapper aliases
    assert _buffers_overlap(pd.Series(x[::2], copy=False), x) is True     # strided no-copy wrapper
    assert _buffers_overlap(pd.Series(x, copy=True), x) is False          # a real copy does not

    class _Wrap:  # a generic Sequence over x: NOT a buffer, NOT __array__
        def __init__(self, a): self._a = a
        def __len__(self): return len(self._a)
        def __getitem__(self, i): return float(self._a[i])
    assert _buffer_extent(_Wrap(x)) is None
    assert _buffers_overlap(_Wrap(x), x) is False   # invisible to the address authority -> refused by admission


def test_primitive_bytearray_and_readonly_memoryview():
    ba = bytearray(8)
    mv = memoryview(ba)
    assert _buffers_overlap(mv[0:4], mv[2:6]) is True   # writable overlap
    assert _buffers_overlap(mv[0:2], mv[2:4]) is False  # writable disjoint
    ro = memoryview(bytes(range(8)))
    assert _buffers_overlap(ro[0:5], ro[3:8]) is True   # read-only overlap (C-buffer-API route)
    assert _buffers_overlap(memoryview(bytes(0)), memoryview(bytes(0))) is False  # empty overlaps nothing
    assert _buffer_extent(memoryview(bytes(0)))[1] == 0  # ...but an empty buffer IS addressable (nbytes 0)
    assert _buffer_extent([1, 2, 3]) is None            # a non-buffer (list) -> None (identity-only)
    assert _buffer_extent(np.arange(20, dtype=np.int32)[::2]) is not None  # strided IS addressable now


# =====================================================================================
# POSITIVE controls: the non-aliased path is untouched.
# =====================================================================================

def test_positive_distinct_array_buffers_compute_correctly(kernel):
    a = np.array([10, 20, 30, 40], dtype=np.int32)
    out = np.zeros_like(a)
    kernel.call("shift_arr", ["Array[int32]", "Array[int32]"], [a, out], read_back=[1], return_type="None")
    np.testing.assert_array_equal(out, a + np.int32(1))


def test_positive_equal_valued_distinct_buffers_with_mutated_out(kernel):
    # src and out hold EQUAL values but are DISTINCT allocations, and out IS mutated -- a
    # value-based detector would wrongly refuse; the address-based guard must allow it and it
    # must compute correctly (should-fix 3).
    src = np.array([1, 2, 3, 4], dtype=np.int32)
    out = np.array([1, 2, 3, 4], dtype=np.int32)  # equal values, separate buffer
    assert not np.shares_memory(src, out)
    kernel.call("reverse_into", ["Array[int32]", "Array[int32]", "int"], [src, out, 4],
                read_back=[1], return_type="None")
    np.testing.assert_array_equal(out, np.array([4, 3, 2, 1], dtype=np.int32))


def test_positive_readonly_inputs_may_share_buffer(kernel):
    # add_pair(a, a): the SAME buffer to two READ-ONLY inputs (no mutated out-param) is correct
    # and must NOT be refused.
    a = np.array([1, 2, 3, 4, 5], dtype=np.int32)
    r = kernel.call("add_pair", ["Array[int32]", "Array[int32]"], [a, a], read_back=[], return_type="int")
    assert r.value == int((2 * a).sum())


def test_positive_distinct_three_buffers_and_2d(kernel):
    a = np.array([1, 2, 3, 4], dtype=np.int32)
    b = np.array([5, 6, 7, 8], dtype=np.int32)
    out = np.zeros_like(a)
    kernel.call("combine3", ["Array[int32]", "Array[int32]", "Array[int32]", "int"], [a, b, out, 4],
                read_back=[2], return_type="None")
    np.testing.assert_array_equal(out, a + b[::-1])
    g = np.array([[1, 2, 3], [4, 5, 6], [7, 8, 9]], dtype=np.int32)
    gout = np.zeros_like(g)
    kernel.call("flip_rows", ["Array[int32, 2]", "Array[int32, 2]"], [g, gout], read_back=[1], return_type="None")
    np.testing.assert_array_equal(gout, g[::-1])


def test_positive_list_distinct_buffers_compute_correctly(kernel):
    # On the list path ServerKernel.call returns the read-back in `outs` (the in-place write is
    # one layer up in decorators.run_server); distinct buffers still marshal + read back.
    r = kernel.call("shift_list", ["list[int]", "list[int]"], [[10, 20, 30], [0, 0, 0]],
                    read_back=[1], return_type="None")
    assert r.outs[1] == [11, 21, 31]


# =====================================================================================
# REFUSAL controls: an aliased mutated out-param goes RED.
# =====================================================================================

def test_refusal_array_identity_alias(kernel):
    x = np.array([1, 2, 3, 4], dtype=np.int32)
    with pytest.raises(PythscribeAliasingError) as ei:
        kernel.call("shift_arr", ["Array[int32]", "Array[int32]"], [x, x], read_back=[1], return_type="None")
    assert "494" in str(ei.value) and "out-param" in str(ei.value)


def test_refusal_array_view_alias(kernel):
    x = np.array([5, 6, 7, 8], dtype=np.int32)
    assert x[:] is not x and np.shares_memory(x[:], x)
    with pytest.raises(PythscribeAliasingError):
        kernel.call("shift_arr", ["Array[int32]", "Array[int32]"], [x[:], x], read_back=[1], return_type="None")


def test_refusal_array_partial_overlap(kernel):
    base = np.arange(8, dtype=np.int32)
    left, right = base[0:5], base[3:8]
    assert left is not right and np.shares_memory(left, right)
    with pytest.raises(PythscribeAliasingError):
        kernel.call("reverse_into", ["Array[int32]", "Array[int32]", "int"], [left, right, 5],
                    read_back=[1], return_type="None")


def test_refusal_shares_memory_broken_still_refuses(kernel, monkeypatch):
    # Regression row (E2E): even with numpy.shares_memory broken, the aliased call is refused --
    # the footprint primitive decides via the C buffer API without ever calling shares_memory.
    monkeypatch.setattr(np, "shares_memory", lambda *a, **k: (_ for _ in ()).throw(RuntimeError("boom")))
    x = np.array([1, 2, 3, 4], dtype=np.int32)
    with pytest.raises(PythscribeAliasingError):
        kernel.call("shift_arr", ["Array[int32]", "Array[int32]"], [x, x], read_back=[1], return_type="None")


def test_refusal_three_buffer_params(kernel):
    # 3+ buffer params: alias b (read, reversed) and out (mutated) -> refused.
    a = np.array([10, 20, 30, 40], dtype=np.int32)
    b = np.array([10, 20, 30, 40], dtype=np.int32)
    with pytest.raises(PythscribeAliasingError):
        kernel.call("combine3", ["Array[int32]", "Array[int32]", "Array[int32]", "int"], [a, b, b, 4],
                    read_back=[2], return_type="None")


def test_refusal_two_out_params_aliasing(kernel):
    # two MUTATED out-params sharing one buffer -> refused (both in read_back).
    src = np.array([10, 20, 30, 40], dtype=np.int32)
    shared = np.array([0, 0, 0, 0], dtype=np.int32)
    with pytest.raises(PythscribeAliasingError):
        kernel.call("dup2", ["Array[int32]", "Array[int32]", "Array[int32]", "int"], [src, shared, shared, 4],
                    read_back=[1, 2], return_type="None")


def test_refusal_2d_overlap(kernel):
    g = np.array([[1, 2, 3], [4, 5, 6], [7, 8, 9]], dtype=np.int32)
    with pytest.raises(PythscribeAliasingError):
        kernel.call("flip_rows", ["Array[int32, 2]", "Array[int32, 2]"], [g, g], read_back=[1], return_type="None")


def test_refusal_mixed_list_and_array(kernel):
    # a kernel with BOTH a list param and array params: the combined buffer_idx pairing still
    # catches the aliased array out-param (list presence does not disrupt the check).
    a = np.array([1, 2, 3], dtype=np.int32)
    with pytest.raises(PythscribeAliasingError):
        kernel.call("mixed", ["list[int]", "Array[int32]", "Array[int32]"], [[7], a, a],
                    read_back=[2], return_type="None")


def test_refusal_list_identity_alias_twin(kernel):
    x = [1, 2, 3]
    with pytest.raises(PythscribeAliasingError):
        kernel.call("shift_list", ["list[int]", "list[int]"], [x, x], read_back=[1], return_type="None")


def test_refusal_is_pythscribe_aliasing_error_subclass_of_serverffi():
    assert issubclass(PythscribeAliasingError, ServerFfiError)


# =====================================================================================
# LIST-SINK ADMISSION (BLOCKER): the list sink's input domain is bounded to what the
# authority can reason about -- an unintrospectable wrapper over the out-buffer is refused.
# =====================================================================================

def test_list_sink_admission_domain():
    # The authority both (1) admits the list-sink domain and (2) refuses aliasing, in one place.
    pd = gate_import("pandas")
    import pandas as pd  # noqa: F811
    import array as _arr
    x = np.arange(8, dtype=np.float64)

    def call(v, read_back):  # v is the list[float] param 0; x is the Array out param 1
        check_buffer_aliasing("f", [v, x, 4], [0], [1], read_back)

    # EVERY pandas.Series -- no-copy (aliases), copy=True, and the pandas-3 CoW default (a copy)
    # -- is refused by the ADMISSION arm (a PLAIN ServerFfiError naming `.to_numpy()`), never by
    # np.asarray reasoning: `__array__` is not the channel `_pack_list` reads.
    for s in (pd.Series(x, copy=False), pd.Series(x, copy=True), pd.Series(x)):
        with pytest.raises(ServerFfiError) as ei:
            call(s, [1])
        assert not isinstance(ei.value, PythscribeAliasingError)
        assert ".to_numpy()" in str(ei.value) and "Series" in str(ei.value) and "494" in str(ei.value)
    # an unintrospectable Sequence wrapper -> refused the same way.
    with pytest.raises(ServerFfiError) as ei:
        call(_SeqWrap(x), [1])
    assert not isinstance(ei.value, PythscribeAliasingError)

    # OVER-REFUSAL control: every admitted, non-aliasing form still PASSES (no raise) -- incl. an
    # EMPTY ndarray (addressable with nbytes 0), bytes/bytearray, a ctypes array, a DISJOINT
    # memoryview over the out itself, and the `.to_numpy()` of a Series copy.
    other = np.arange(8, dtype=np.float64)  # a DIFFERENT allocation: a slice of it is disjoint from x
    for v in ([1.0, 2.0, 3.0], (1.0, 2.0), range(3), memoryview(bytearray(8)), b"\x01\x02",
              bytearray(4), _arr.array("d", [1.0, 2.0]), np.zeros(3), np.zeros(0),
              (ctypes.c_double * 3)(), memoryview(other)[4:8], pd.Series(x, copy=True).to_numpy()):
        call(v, [1])
    # (a memoryview over the OUT itself is caught by the OVERLAP arm -- the aliasing subclass.)
    with pytest.raises(PythscribeAliasingError):
        call(memoryview(x)[4:8], [1])

    # no mutated out-param => no silent-wrong hazard => even a wrapper is admitted (domain not narrowed).
    call(_SeqWrap(x), [])
    call(pd.Series(x, copy=False), [])


# =====================================================================================
# MUTATION-VERIFY: guard off -> silent WRONG value (diverges from CPython); guard on -> raises.
# =====================================================================================

def _rev_py(src, out, n):
    for i in range(n):
        out[i] = src[n - 1 - i]


def test_mutation_verify_array_reverse(kernel, monkeypatch):
    seed = [10, 20, 30, 40]
    cpython = list(seed)
    _rev_py(cpython, cpython, 4)              # src is out: in-place clobber
    assert cpython == [40, 30, 30, 40]
    monkeypatch.setattr(rt, "check_buffer_aliasing", lambda *a, **k: None)
    xa = np.array(seed, dtype=np.int32)
    kernel.call("reverse_into", ["Array[int32]", "Array[int32]", "int"], [xa, xa, 4],
                read_back=[1], return_type="None")
    assert xa.tolist() == [40, 30, 20, 10]   # independent buffers -> clean reverse
    assert xa.tolist() != cpython, "guard-off value must diverge from CPython (the silent bug)"


def test_mutation_verify_three_buffer_combine3(kernel, monkeypatch):
    # b (read reversed) aliases out (written forward) -> genuine read-after-write divergence.
    seed = [10, 20, 30, 40]
    def combine3_py(a, b, out, n):
        for i in range(n):
            out[i] = a[i] + b[n - 1 - i]
    a_ref, shared_ref = list(seed), list(seed)
    combine3_py(a_ref, shared_ref, shared_ref, 4)
    assert shared_ref == [50, 50, 80, 90]    # CPython aliased
    monkeypatch.setattr(rt, "check_buffer_aliasing", lambda *a, **k: None)
    xa = np.array(seed, dtype=np.int32); xshared = np.array(seed, dtype=np.int32)
    kernel.call("combine3", ["Array[int32]", "Array[int32]", "Array[int32]", "int"], [xa, xshared, xshared, 4],
                read_back=[2], return_type="None")
    assert xshared.tolist() == [50, 50, 50, 50]  # independent buffers
    assert xshared.tolist() != shared_ref, "guard-off 3-buffer value must diverge (silent bug)"


def test_mutation_verify_2d_flip_rows(kernel, monkeypatch):
    grid = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
    def flip_py(a, out, rows, cols):
        for i in range(rows):
            for j in range(cols):
                out[i][j] = a[rows - 1 - i][j]
    g_ref = [row[:] for row in grid]
    flip_py(g_ref, g_ref, 3, 3)
    assert g_ref == [[7, 8, 9], [4, 5, 6], [7, 8, 9]]  # CPython aliased in-place clobber
    monkeypatch.setattr(rt, "check_buffer_aliasing", lambda *a, **k: None)
    xg = np.array(grid, dtype=np.int32)
    kernel.call("flip_rows", ["Array[int32, 2]", "Array[int32, 2]"], [xg, xg], read_back=[1], return_type="None")
    assert xg.tolist() == [[7, 8, 9], [4, 5, 6], [1, 2, 3]]  # independent buffers -> clean flip
    assert xg.tolist() != g_ref, "guard-off 2-D value must diverge (silent bug)"


def test_mutation_verify_list_reverse_twin(kernel, monkeypatch):
    # The list twin that GENUINELY diverges (should-fix 4): reverse_list(x, x).
    seed = [10, 20, 30, 40]
    cpython = list(seed)
    _rev_py(cpython, cpython, 4)
    assert cpython == [40, 30, 30, 40]
    monkeypatch.setattr(rt, "check_buffer_aliasing", lambda *a, **k: None)
    x = list(seed)
    r = kernel.call("reverse_list", ["list[int]", "list[int]", "int"], [x, x, 4],  # SAME list object
                    read_back=[1], return_type="None")
    assert r.outs[1] == [40, 30, 20, 10]     # independent buffers
    assert r.outs[1] != cpython, "guard-off list-twin value must diverge (silent bug)"


def test_mutation_verify_guard_toggle_two_out(kernel, monkeypatch):
    # dup2's two-out alias is alias-EQUIVALENT (no divergence), so this proves the REFUSAL is
    # what stops it: guard on -> raises; guard off -> runs (no raise).
    src = np.array([10, 20, 30, 40], dtype=np.int32)
    sh = np.zeros(4, dtype=np.int32)
    with pytest.raises(PythscribeAliasingError):
        kernel.call("dup2", ["Array[int32]", "Array[int32]", "Array[int32]", "int"], [src, sh, sh, 4],
                    read_back=[1, 2], return_type="None")
    monkeypatch.setattr(rt, "check_buffer_aliasing", lambda *a, **k: None)
    sh2 = np.zeros(4, dtype=np.int32)
    kernel.call("dup2", ["Array[int32]", "Array[int32]", "Array[int32]", "int"], [src, sh2, sh2, 4],
                read_back=[1, 2], return_type="None")  # no raise with guard off


def test_e2e_numpy_absent_readonly_vs_writable_refused_and_diverges(kernel, monkeypatch):
    # The Fable BLOCKER, end-to-end through ServerKernel.call with NumPy ABSENT: a read-only
    # exporter (src) and a DIFFERENT writable exporter (out) over the SAME bytearray. Guard on
    # -> refused; guard off -> the aliased call silently miscomputes (diverges from CPython).
    monkeypatch.setitem(sys.modules, "numpy", None)

    def rev_py(src, out, n):
        for i in range(n):
            out[i] = src[n - 1 - i]
    ref = bytearray([10, 20, 30, 40])
    rev_py(ref, ref, 4)                      # CPython aliased (one storage): in-place clobber
    assert list(ref) == [40, 30, 30, 40]

    ba = bytearray([10, 20, 30, 40])
    ro = memoryview(ba).toreadonly()
    wr = (ctypes.c_ubyte * 4).from_buffer(ba)
    with pytest.raises(PythscribeAliasingError):
        kernel.call("reverse_u8", ["Array[uint8]", "Array[uint8]", "int"], [ro, wr, 4],
                    read_back=[1], return_type="None")

    monkeypatch.setattr(rt, "check_buffer_aliasing", lambda *a, **k: None)
    ba2 = bytearray([10, 20, 30, 40])
    ro2 = memoryview(ba2).toreadonly()
    wr2 = (ctypes.c_ubyte * 4).from_buffer(ba2)
    kernel.call("reverse_u8", ["Array[uint8]", "Array[uint8]", "int"], [ro2, wr2, 4],
                read_back=[1], return_type="None")
    del wr2  # release the ctypes export so the bytearray is readable
    assert list(ba2) == [40, 30, 20, 10]     # independent buffers
    assert list(ba2) != list(ref), "guard-off value must diverge from CPython (the silent bug)"


def _rev_view_ref(seed_len=8, n=4):
    # CPython aliased semantics of `out[i] = src[n-1-i]` where src is a strided VIEW of out.
    a = np.arange(seed_len, dtype=np.float64)
    src = a[::2]
    for i in range(n):
        a[i] = src[n - 1 - i]
    return a


def test_e2e_strided_list_input_vs_array_out_refused_and_diverges(kernel, monkeypatch):
    # Repro A through ServerKernel.call: a STRIDED view passed as a read-only `list[float]` input
    # (the LIST sink iterates it) that aliases the mutated `Array[float64]` out-buffer. This is
    # the exact regression the PyBUF_INDIRECT footprint closes.
    aliased = _rev_view_ref()
    assert aliased[:4].tolist() == [6, 4, 2, 6]  # CPython aliased (view src over out)

    x = np.arange(8, dtype=np.float64)
    with pytest.raises(PythscribeAliasingError):
        kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [x[::2], x, 4],
                    read_back=[1], return_type="None")
    np.testing.assert_array_equal(x, np.arange(8, dtype=np.float64))  # untouched

    monkeypatch.setattr(rt, "check_buffer_aliasing", lambda *a, **k: None)
    x2 = np.arange(8, dtype=np.float64)
    kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [x2[::2], x2, 4],
                read_back=[1], return_type="None")
    assert x2[:4].tolist() == [6, 4, 2, 0]                # independent buffers
    assert x2[:4].tolist() != aliased[:4].tolist(), "guard-off strided value must diverge (silent bug)"


def test_e2e_strided_memoryview_list_twin_refused(kernel):
    # Repro B through ServerKernel.call: a strided `memoryview` of a bytearray as a `list[int]`
    # input aliasing the (buffer) out -- refused. (`run_server` additionally requires a real list
    # for a mutated list param, so the pure list-twin is reachable here at the ServerKernel.call
    # level; the production-reachable form is repro A above.)
    ba = bytearray([10, 20, 30, 40, 50, 60, 70, 80])
    with pytest.raises(PythscribeAliasingError):
        kernel.call("reverse_list", ["list[int]", "list[int]", "int"], [memoryview(ba)[::2], ba, 4],
                    read_back=[1], return_type="None")


def test_e2e_decorator_strided_view_aliasing_refused_and_diverges(wasm_module, monkeypatch):
    # Repro A through the PUBLIC @wasm decorator (run_server -> ServerKernel.call): rev_f(x[::2], x).
    from pythscribe import binding_of
    b = binding_of(wasm_module.rev_f)
    assert b.mode == "server"
    aliased = _rev_view_ref()
    assert aliased[:4].tolist() == [6, 4, 2, 6]

    x = np.arange(8, dtype=np.float64)
    with pytest.raises(PythscribeAliasingError):
        wasm_module.rev_f(x[::2], x, 4)
    np.testing.assert_array_equal(x, np.arange(8, dtype=np.float64))  # untouched

    monkeypatch.setattr(rt, "check_buffer_aliasing", lambda *a, **k: None)
    x2 = np.arange(8, dtype=np.float64)
    wasm_module.rev_f(x2[::2], x2, 4)
    assert x2[:4].tolist() == [6, 4, 2, 0]
    assert x2[:4].tolist() != aliased[:4].tolist(), "guard-off decorator value must diverge (silent bug)"


def _rev_full_ref(seed_len=8, n=4):
    # CPython aliased semantics of `out[i] = src[n-1-i]` where src IS out (a full no-copy wrapper).
    a = np.arange(seed_len, dtype=np.float64)
    for i in range(n):
        a[i] = a[n - 1 - i]
    return a


def test_e2e_series_wrapper_aliasing_refused_and_diverges(kernel, monkeypatch):
    # BLOCKER (Fable), repro through ServerKernel.call: a pandas.Series(x, copy=False) -- a
    # non-buffer __array__ wrapper over x -- passed as the read-only list[float] input while x is
    # the mutated Array[float64] out. It aliases silently pre-fix; now refused by ADMISSION (a
    # plain ServerFfiError -- the wrapper never reaches the overlap arm).
    pd = gate_import("pandas")
    import pandas as pd  # noqa: F811
    ref = _rev_full_ref()
    assert ref[:4].tolist() == [3, 2, 2, 3]  # CPython aliased (Series wraps out)

    x = np.arange(8, dtype=np.float64)
    with pytest.raises(ServerFfiError) as ei:
        kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [pd.Series(x, copy=False), x, 4],
                    read_back=[1], return_type="None")
    assert not isinstance(ei.value, PythscribeAliasingError) and ".to_numpy()" in str(ei.value)
    np.testing.assert_array_equal(x, np.arange(8, dtype=np.float64))  # untouched

    monkeypatch.setattr(rt, "check_buffer_aliasing", lambda *a, **k: None)
    x2 = np.arange(8, dtype=np.float64)
    kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [pd.Series(x2, copy=False), x2, 4],
                read_back=[1], return_type="None")
    assert x2[:4].tolist() == [3, 2, 1, 0]                       # independent buffers
    assert x2[:4].tolist() != ref[:4].tolist(), "guard-off wrapper value must diverge (silent bug)"


def test_e2e_series_wrapper_decorator_refused_and_diverges(wasm_module, monkeypatch):
    # Same repro through the PUBLIC @wasm decorator.
    pd = gate_import("pandas")
    import pandas as pd  # noqa: F811
    ref = _rev_full_ref()
    x = np.arange(8, dtype=np.float64)
    with pytest.raises(ServerFfiError) as ei:
        wasm_module.rev_f(pd.Series(x, copy=False), x, 4)
    assert not isinstance(ei.value, PythscribeAliasingError) and ".to_numpy()" in str(ei.value)
    np.testing.assert_array_equal(x, np.arange(8, dtype=np.float64))

    monkeypatch.setattr(rt, "check_buffer_aliasing", lambda *a, **k: None)
    x2 = np.arange(8, dtype=np.float64)
    wasm_module.rev_f(pd.Series(x2, copy=False), x2, 4)
    assert x2[:4].tolist() == [3, 2, 1, 0]
    assert x2[:4].tolist() != ref[:4].tolist(), "guard-off decorator wrapper value must diverge (silent bug)"


def test_e2e_unintrospectable_wrapper_refused_both_sinks(kernel, wasm_module):
    # A generic Sequence wrapper (no buffer, no __array__) over the out-buffer is refused LOUDLY
    # by the admission gate -- through BOTH ServerKernel.call and the @wasm decorator -- never
    # marshalled to a silent independent copy. (A ServerFfiError, not the aliasing subclass.)
    x = np.arange(8, dtype=np.float64)
    with pytest.raises(ServerFfiError) as ei:
        kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [_SeqWrap(x), x, 4],
                    read_back=[1], return_type="None")
    assert not isinstance(ei.value, PythscribeAliasingError)
    np.testing.assert_array_equal(x, np.arange(8, dtype=np.float64))

    x2 = np.arange(8, dtype=np.float64)
    with pytest.raises(ServerFfiError):
        wasm_module.rev_f(_SeqWrap(x2), x2, 4)
    np.testing.assert_array_equal(x2, np.arange(8, dtype=np.float64))


def test_e2e_admitted_list_inputs_still_pass(kernel):
    # OVER-REFUSAL control (E2E): admitted, non-aliasing list-sink inputs still compute correctly
    # -- a plain list AND an independent ndarray as the list[float] src.
    out = np.zeros(8, dtype=np.float64)
    kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [[10.0, 20.0, 30.0, 40.0], out, 4],
                read_back=[1], return_type="None")
    assert out[:4].tolist() == [40.0, 30.0, 20.0, 10.0]
    src = np.array([1.0, 2.0, 3.0, 4.0], dtype=np.float64)  # independent ndarray as list input
    out2 = np.zeros(4, dtype=np.float64)
    kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [src, out2, 4],
                read_back=[1], return_type="None")
    assert out2.tolist() == [4.0, 3.0, 2.0, 1.0]


def test_mutation_verify_guard_on_refuses_the_divergent_call(kernel):
    xb = np.array([10, 20, 30, 40], dtype=np.int32)
    with pytest.raises(PythscribeAliasingError):
        kernel.call("reverse_into", ["Array[int32]", "Array[int32]", "int"], [xb, xb, 4],
                    read_back=[1], return_type="None")
    np.testing.assert_array_equal(xb, np.array([10, 20, 30, 40], dtype=np.int32))  # untouched


# =====================================================================================
# THE ROOT (issue #494, round 2): the list-sink ADMISSION binds the marshaller's channel.
# `_pack_list` reads `__len__` + `__iter__` (or the `__getitem__` fallback); the old admission
# checked a DIFFERENT channel (isinstance / __array__ via np.asarray), so wrappers slipped through
# SILENTLY. Every RED row below is paired with a MUTATION witness that neutralizes ONLY the
# admission arm (`_admit_list_arg -> None`; the overlap arm stays LIVE) and shows the call then
# returns a value that DIVERGES from the CPython-aliased reference -- proving (i) the admission
# arm is what refuses it and (ii) the footprint authority alone cannot see these wrappers.
# =====================================================================================

def _neutralize_admission(monkeypatch):
    monkeypatch.setattr(rt, "_admit_list_arg", lambda v: None)


def test_admission_rule_exact_set_and_slot_binding():
    # (a) EXACT containers admitted; ANY list/tuple subclass refused -- even one with no override
    # (exact-type rule: the subclass may override a slot; we do not reason about which).
    import array as _arr
    for v in ([1.0], (1.0,), range(2)):
        assert _admit_list_arg(v) is None
    class PlainList(list): pass
    class PlainTuple(tuple): pass
    assert _admit_list_arg(PlainList([1.0])) is not None
    assert _admit_list_arg(PlainTuple((1.0,))) is not None
    # (b) builtin-based buffers with UNOVERRIDDEN sink slots admitted -- exact AND subclass.
    class PlainBytes(bytes): pass
    class PlainNd(np.ndarray): pass
    for v in (memoryview(bytearray(2)), b"\x01", bytearray(2), _arr.array("d", [1.0]), np.zeros(2),
              np.zeros(0), (ctypes.c_int64 * 2)(), PlainBytes(b"ab"), np.zeros(2).view(PlainNd)):
        assert _admit_list_arg(v) is None, type(v)
    # ...and a buffer subclass overriding ANY sink slot is refused, naming the slot. Includes the
    # numpy types that override __getitem__ (matrix, MaskedArray) and the ctypes shape where the
    # base LACKS __iter__ and the subclass ADDS one (that changes what enumerate() calls).
    class IterNd(np.ndarray):
        def __iter__(self): return iter(())
    class LenBytes(bytes):
        def __len__(self): return 0
    class IterCtypes(ctypes.c_double * 2):
        def __iter__(self): return iter(())
    x = np.arange(8, dtype=np.float64)
    import warnings
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", PendingDeprecationWarning)  # np.matrix is pending-deprecated
        matrix = np.matrix([[1.0]])
    for v, slot in ((np.zeros(2).view(IterNd), "__iter__"), (LenBytes(b"ab"), "__len__"),
                    (IterCtypes(), "__iter__"), (matrix, "__getitem__"),
                    (np.ma.MaskedArray([1.0]), "__getitem__")):
        why = _admit_list_arg(v)
        assert why is not None and slot in why, (type(v), why)
    for label, w in _live_wrappers(x):
        assert _admit_list_arg(w) is not None, label
    # a slot assigned AFTER class creation is an override too (type.__setattr__ updates the slot).
    class LateList(bytes): pass
    LateList.__getitem__ = lambda self, i: 0
    assert "__getitem__" in _admit_list_arg(LateList(b"ab"))


def test_admission_lookup_bypasses_a_lying_metaclass():
    # The slot check walks `type.__dict__['__mro__']` / `type.__dict__['__dict__']` directly, so
    # a metaclass whose attribute access hides an override cannot make it look unoverridden.
    class Liar(type):
        def __getattribute__(cls, name):
            if name in ("__iter__", "__getitem__", "__len__"):
                return getattr(bytes, name)  # claims the builtin slot
            return super().__getattribute__(name)
    class Sneaky(bytes, metaclass=Liar):
        def __getitem__(self, i): return 0
    assert Sneaky.__getitem__ is bytes.__getitem__          # the lie, via normal lookup
    assert "__getitem__" in _admit_list_arg(Sneaky(b"ab"))  # refused anyway


def test_admission_binds_the_buffer_protocol_channel():
    # SF-1: rule (b) is sound only if the storage the AUTHORITY addresses (the buffer protocol:
    # PEP 688 `__buffer__` / `__release_buffer__`) IS the storage the MARSHALLER iterates. The
    # three-slot walk left that FOURTH channel unbound: a subclass overriding ONLY `__buffer__`
    # was admitted while the authority reasoned over a decoy. Repro FIRST (the desync is real on
    # this interpreter), then the refusal, then the release-only arm, then over-refusal.
    gate(sys.version_info >= (3, 12), "PEP 688 __buffer__ needs CPython 3.12+")
    real = _EvilBA(b"\x01\x02\x03\x04\x05\x06\x07\x08")
    assert _buffer_extent(real) == _buffer_extent(_DECOY)       # the authority sees the DECOY...
    assert list(real) == [1, 2, 3, 4, 5, 6, 7, 8]               # ...iteration reads the REAL storage
    assert _buffer_extent(real)[1] == 64 != len(real)            # (a different storage, different size)
    why = _admit_list_arg(real)
    assert why is not None and "__buffer__" in why and "_EvilBA" in why, why
    with pytest.raises(ServerFfiError) as ei:
        check_buffer_aliasing("f", [real, np.zeros(8), 4], [0], [1], [1])
    assert not isinstance(ei.value, PythscribeAliasingError) and "__buffer__" in str(ei.value)
    # an ndarray subclass hijacking `__buffer__` over the OUT itself: the footprint arm is BLIND
    # (decoy disjoint from x) although the view shares x's memory -- only admission can refuse it.
    x = np.arange(8, dtype=np.float64)
    w = x.view(_EvilNd)
    assert np.shares_memory(w, x) and _buffer_extent(w) == _buffer_extent(_ND_DECOY)
    assert _buffers_overlap(w, x) is False
    assert "__buffer__" in _admit_list_arg(w)
    class LateBuf(bytes): pass                                   # assigned AFTER class creation
    LateBuf.__buffer__ = lambda self, flags: memoryview(_DECOY)
    assert "__buffer__" in _admit_list_arg(LateBuf(b"ab"))
    # `__release_buffer__`-ONLY is disqualifying too: the hook runs arbitrary Python between the
    # authority's address read and the marshaller's iteration and CAN relocate the storage it
    # just certified -- concrete witness: an owning ndarray subclass resizes in the hook.
    a = _RelNd((4,), dtype=np.float64); a[:] = [0.0, 1.0, 2.0, 3.0]
    before = a.ctypes.data
    e = _buffer_extent(a)                     # GetBuffer + Release -> the hook fires and relocates
    assert e == (before, 32) and a.ctypes.data != before, "release hook must have relocated the storage"
    assert "__release_buffer__" in _admit_list_arg(a)
    assert "__release_buffer__" in _admit_list_arg(_RelBA(b"abcd"))
    # OVER-REFUSAL guard: every legitimate buffer base (no buffer-protocol override) stays admitted.
    import array as _arr
    class PlainBA(bytearray): pass
    class PlainNd(np.ndarray): pass
    for v in (bytearray(4), b"ab", memoryview(bytearray(4)), _arr.array("d", [1.0]), np.zeros(3),
              np.zeros(0), (ctypes.c_double * 3)(), PlainBA(b"ab"), np.zeros(2).view(PlainNd)):
        assert _admit_list_arg(v) is None, type(v)


def test_e2e_buffer_protocol_hijack_refused_and_diverges_both_sinks(kernel, wasm_module, monkeypatch):
    # SF-1 end-to-end. (1) The literal `_EvilBA` repro through BOTH sinks: refused LOUDLY by the
    # admission arm (a plain ServerFfiError naming `__buffer__`), never marshalled. (2) The
    # ALIASING witness `x.view(_EvilNd)` over the mutated out: CPython aliases to [3,2,2,3]; the
    # footprint arm sees a disjoint decoy; refused through ServerKernel.call AND the @wasm
    # decorator. (3) MUTATION: admission arm OFF (overlap arm LIVE) -> silent [3,2,1,0].
    gate(sys.version_info >= (3, 12), "PEP 688 __buffer__ needs CPython 3.12+")
    with pytest.raises(ServerFfiError) as ei:
        kernel.call("reverse_list", ["list[int]", "list[int]", "int"],
                    [_EvilBA(b"\x01\x02\x03\x04"), [0, 0, 0, 0], 4], read_back=[1], return_type="None")
    assert not isinstance(ei.value, PythscribeAliasingError) and "__buffer__" in str(ei.value)
    x = np.arange(8, dtype=np.float64)
    with pytest.raises(ServerFfiError) as ei:
        wasm_module.rev_f(_EvilBA(b"\x01\x02\x03\x04"), x, 4)
    assert not isinstance(ei.value, PythscribeAliasingError) and "__buffer__" in str(ei.value)
    np.testing.assert_array_equal(x, np.arange(8, dtype=np.float64))

    ref = _rev_full_ref()
    assert ref[:4].tolist() == [3, 2, 2, 3]
    w = x.view(_EvilNd)
    assert np.shares_memory(w, x) and _buffers_overlap(w, x) is False   # blind to the footprint arm
    with pytest.raises(ServerFfiError) as ei:
        kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [w, x, 4],
                    read_back=[1], return_type="None")
    assert not isinstance(ei.value, PythscribeAliasingError) and "__buffer__" in str(ei.value)
    np.testing.assert_array_equal(x, np.arange(8, dtype=np.float64))    # untouched
    with pytest.raises(ServerFfiError) as ei:
        wasm_module.rev_f(x.view(_EvilNd), x, 4)
    assert not isinstance(ei.value, PythscribeAliasingError) and "__buffer__" in str(ei.value)
    np.testing.assert_array_equal(x, np.arange(8, dtype=np.float64))

    _neutralize_admission(monkeypatch)
    x2 = np.arange(8, dtype=np.float64)
    kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [x2.view(_EvilNd), x2, 4],
                read_back=[1], return_type="None")
    assert x2[:4].tolist() == [3, 2, 1, 0]                                # independent buffers
    assert x2[:4].tolist() != ref[:4].tolist(), "buffer-protocol hijack: admission-off value must diverge (silent bug)"
    x3 = np.arange(8, dtype=np.float64)
    wasm_module.rev_f(x3.view(_EvilNd), x3, 4)
    assert x3[:4].tolist() == [3, 2, 1, 0] and x3[:4].tolist() != ref[:4].tolist()


@pytest.mark.parametrize("label", ["list subclass", "tuple subclass", "ctypes-array subclass",
                                   "__array__-copy wrapper"])
def test_e2e_live_wrapper_refused_and_diverges_kernel_call(kernel, monkeypatch, label):
    # B1 / B1' / B2 through ServerKernel.call. CPython-aliased reference FIRST: rev_f(w, x, 4)
    # with w reading x live == `x[i] = x[3-i]` in place -> [3, 2, 2, 3].
    ref = _rev_full_ref()
    assert ref[:4].tolist() == [3, 2, 2, 3]

    x = np.arange(8, dtype=np.float64)
    w = dict(_live_wrappers(x))[label]
    with pytest.raises(ServerFfiError) as ei:
        kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [w, x, 4],
                    read_back=[1], return_type="None")
    assert not isinstance(ei.value, PythscribeAliasingError)   # ADMISSION, not the overlap arm
    assert "494" in str(ei.value) and ".to_numpy()" in str(ei.value)
    np.testing.assert_array_equal(x, np.arange(8, dtype=np.float64))  # untouched

    # MUTATION: admission arm OFF (overlap arm still live) -> marshals the live-read values into an
    # INDEPENDENT buffer -> clean reverse [3, 2, 1, 0] != CPython's [3, 2, 2, 3]. Silent wrong.
    _neutralize_admission(monkeypatch)
    x2 = np.arange(8, dtype=np.float64)
    w2 = dict(_live_wrappers(x2))[label]
    kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [w2, x2, 4],
                read_back=[1], return_type="None")
    assert x2[:4].tolist() == [3, 2, 1, 0]
    assert x2[:4].tolist() != ref[:4].tolist(), f"{label}: admission-off value must diverge (silent bug)"


@pytest.mark.parametrize("label", ["list subclass", "tuple subclass", "ctypes-array subclass",
                                   "__array__-copy wrapper"])
def test_e2e_live_wrapper_refused_and_diverges_decorator(wasm_module, monkeypatch, label):
    # The same four shapes through the PUBLIC @wasm decorator (run_server -> ServerKernel.call).
    ref = _rev_full_ref()
    x = np.arange(8, dtype=np.float64)
    w = dict(_live_wrappers(x))[label]
    with pytest.raises(ServerFfiError) as ei:
        wasm_module.rev_f(w, x, 4)
    assert not isinstance(ei.value, PythscribeAliasingError)
    np.testing.assert_array_equal(x, np.arange(8, dtype=np.float64))

    _neutralize_admission(monkeypatch)
    x2 = np.arange(8, dtype=np.float64)
    wasm_module.rev_f(dict(_live_wrappers(x2))[label], x2, 4)
    assert x2[:4].tolist() == [3, 2, 1, 0]
    assert x2[:4].tolist() != ref[:4].tolist(), f"{label}: admission-off decorator value must diverge"


def test_e2e_pandas_series_refused_loudly_both_sinks(kernel, wasm_module):
    # The VALUE-DOMAIN CONTRACT: every pandas.Series -- copy=True, the pandas-3 CoW default (a
    # copy), and copy=False -- is refused LOUDLY with the `.to_numpy()` hint, through both sinks.
    # Not silent, not an aliasing error: a wrapper is outside the list sink's admitted domain.
    pd = gate_import("pandas")
    import pandas as pd  # noqa: F811
    x = np.arange(8, dtype=np.float64)
    for s in (pd.Series(x, copy=True), pd.Series(x), pd.Series(x, copy=False)):
        with pytest.raises(ServerFfiError) as ei:
            kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [s, x, 4],
                        read_back=[1], return_type="None")
        assert not isinstance(ei.value, PythscribeAliasingError) and ".to_numpy()" in str(ei.value)
        with pytest.raises(ServerFfiError) as ei:
            wasm_module.rev_f(s, x, 4)
        assert not isinstance(ei.value, PythscribeAliasingError) and ".to_numpy()" in str(ei.value)
    np.testing.assert_array_equal(x, np.arange(8, dtype=np.float64))  # untouched throughout
    # ...and the hinted form WORKS: an independent `.to_numpy()` copy is admitted and computes.
    wasm_module.rev_f(pd.Series(x, copy=True).to_numpy(), x, 4)
    assert x[:4].tolist() == [3, 2, 1, 0]


def test_e2e_admitted_forms_still_pass_over_refusal_controls(kernel):
    # OVER-REFUSAL controls (E2E, with a MUTATED out-param): every admitted exact/builtin form
    # marshals and computes -- plain list, tuple, range, bytes, bytearray, memoryview, array.array,
    # a ctypes array, an ndarray, an EMPTY ndarray, and a DISJOINT memoryview over the out.
    import array as _arr
    want = [4, 3, 2, 1]
    for src in ([1, 2, 3, 4], (1, 2, 3, 4), range(1, 5), b"\x01\x02\x03\x04", bytearray([1, 2, 3, 4]),
                memoryview(bytearray([1, 2, 3, 4])), _arr.array("q", [1, 2, 3, 4]),
                (ctypes.c_int64 * 4)(1, 2, 3, 4)):
        r = kernel.call("reverse_list", ["list[int]", "list[int]", "int"], [src, [0, 0, 0, 0], 4],
                        read_back=[1], return_type="None")
        assert r.outs[1] == want, type(src)
    out = np.zeros(4, dtype=np.float64)
    kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [np.array([1.0, 2.0, 3.0, 4.0]), out, 4],
                read_back=[1], return_type="None")
    assert out.tolist() == [4.0, 3.0, 2.0, 1.0]
    out0 = np.array([9.0, 9.0], dtype=np.float64)  # EMPTY src with n=0: admitted, out untouched
    kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [np.zeros(0), out0, 0],
                read_back=[1], return_type="None")
    assert out0.tolist() == [9.0, 9.0]
    x = np.arange(8, dtype=np.float64)  # a DISJOINT memoryview over the SAME array as the out
    kernel.call("rev_f", ["list[float]", "Array[float64]", "int"], [memoryview(x)[4:8], x[0:4], 4],
                read_back=[1], return_type="None")
    assert x.tolist() == [7, 6, 5, 4, 4, 5, 6, 7]


def test_datetime64_out_param_is_an_array_marshal_error_not_a_raw_value_error(kernel):
    # Nit: a datetime64 buffer is ADDRESSABLE (PyBUF_INDIRECT gives its footprint, so the aliasing
    # authority passes it), but `memoryview()` raises ValueError ("cannot include dtype 'M'") --
    # which escaped `check_array_buffer` raw. It is now wrapped into the total-check's own
    # ArrayMarshalError, at the unit gate and end-to-end through ServerKernel.call.
    dt = np.zeros(4, dtype="M8[ns]")
    assert _buffer_extent(dt) is not None                       # addressable -> aliasing check passes it
    with pytest.raises(ArrayMarshalError) as ei:
        check_array_buffer(dt, "int32", 1)
    assert not isinstance(ei.value, ValueError) and "buffer protocol" in str(ei.value)
    with pytest.raises(ArrayMarshalError) as ei:
        kernel.call("shift_arr", ["Array[int32]", "Array[int32]"], [np.zeros(4, dtype=np.int32), dt],
                    read_back=[1], return_type="None")
    assert not isinstance(ei.value, ValueError)


# --- property-based: the admission binds the three sink slots AND the two buffer-protocol slots
# over GENERATED subclasses (7 bases x 2^5 override subsets) ----------------------------------------
hypothesis = gate_import("hypothesis")
from hypothesis import given, settings, strategies as st  # noqa: E402

_PBT_BASES = ("list", "tuple", "bytes", "bytearray", "array", "ctypes", "ndarray")
_PBT_SLOTS = ("__iter__", "__getitem__", "__len__", "__buffer__", "__release_buffer__")


def _subclass_instance(base: str, ns: dict):
    import array as _arr
    if base == "list":
        return type("S", (list,), ns)([1.0, 2.0])
    if base == "tuple":
        return type("S", (tuple,), ns)((1.0, 2.0))
    if base == "bytes":
        return type("S", (bytes,), ns)(b"ab")
    if base == "bytearray":
        return type("S", (bytearray,), ns)(b"ab")
    if base == "array":
        return type("S", (_arr.array,), ns)("d", [1.0, 2.0])
    if base == "ctypes":
        return type("S", (ctypes.c_double * 2,), ns)()
    return np.zeros(2).view(type("S", (np.ndarray,), ns))


@given(base=st.sampled_from(_PBT_BASES), overridden=st.frozensets(st.sampled_from(_PBT_SLOTS)))
@settings(max_examples=120, deadline=None)
def test_pbt_admission_refuses_exactly_the_overridden_sink_slots(base, overridden):
    # The refusal names an overridden slot from EITHER set (the admission walks the sink slots
    # then the buffer-protocol slots; `any` accepts whichever it names first), and a subclass
    # overriding NONE of the five is admitted -- so neither channel is over- or under-bound.
    ns = {slot: (lambda self, *a: None) for slot in overridden}
    v = _subclass_instance(base, ns)
    why = _admit_list_arg(v)
    if base in ("list", "tuple"):
        assert why is not None                       # (a): exact-type only, any subclass refused
    elif overridden:
        assert why is not None and any(s in why for s in overridden), (base, overridden, why)
    else:
        assert why is None, (base, why)              # (b): unoverridden buffer subclass admitted
