"""`_Instance._pack_list` fast path (marshalling perf regression fix).

The server-path list marshaller validated every element in a Python loop (isinstance + i64 range
check + append) before `struct.pack`, ~15x slower than letting `struct.pack` validate at C speed.
For a marshalling-bound `@wasm` kernel (e.g. `hashed_energy`: two 6000-element list inputs, tiny
compute) that per-element loop dominated the runtime and made `@wasm` barely beat plain Python.

The fix packs the happy path directly through `struct.pack` and drops to the per-element loop ONLY
to locate the offending element when the fast path raises. These tests pin that the fast path is
byte-identical to the reference AND that every loud-fail contract (precise `[k]` message, i64 range,
bool-in-int -> 0/1, bool-in-float refused) is preserved -- i.e. the speedup is not a silent
behaviour change.
"""
from __future__ import annotations

import struct

import pytest

from pythscribe.runtime import ServerFfiError, _Instance

pk = lambda t, a, where="p": _Instance._pack_list(None, t, a, where)  # _pack_list uses no instance state


def test_fast_path_bytes_are_identical_to_the_reference():
    a = [0, 1, -1, 2, 3, -5, 1_000_000, 2**62, -(2**62)]
    assert pk("list[int]", a) == struct.pack(f"<ii{len(a)}q", len(a), len(a), *a)
    f = [0.0, 1.5, -3.0, 4, 1e300, -1e-9]  # ints admitted as floats, like the slow path's float(x)
    assert pk("list[float]", f) == struct.pack(f"<ii{len(f)}d", len(f), len(f), *[float(x) for x in f])


def test_bool_in_int_list_packs_as_0_1():
    # loud-fail contract: bool is admitted in a list[int] and packed as 0/1 (matches the slow path)
    assert pk("list[int]", [True, False, 5]) == struct.pack("<ii3q", 3, 3, 1, 0, 5)


@pytest.mark.parametrize(
    "t, a, frag",
    [
        ("list[int]", [1, 2, 3.5, 4], "[2]: expected an int"),          # a float in an int list
        ("list[int]", [1, "x"], "[1]: expected an int"),                # a str in an int list
        ("list[int]", [1, 2**70], "outside the i64 range"),             # out-of-i64
        ("list[float]", [1.0, "x"], "[1]: expected a float"),           # a str in a float list
        ("list[float]", [1.0, True], "[1]: expected a float"),          # bool refused in a float list
    ],
)
def test_error_locating_is_preserved(t, a, frag):
    # NEGATIVE CONTROL: an invalid element must still raise the PRECISE per-index message (the fast
    # path raises a generic struct.error; the fallback loop is what produces this) -- never a silent
    # wrong-bytes pack. If the fallback loop were removed, these would raise a generic error instead.
    with pytest.raises(ServerFfiError) as ei:
        pk(t, a)
    assert frag in str(ei.value)


def test_list_bool_uses_the_loop_and_refuses_non_01():
    # list[bool] stays on the per-element loop; valid 0/1/bool pack, a non-0/1 int is refused.
    packed = pk("list[bool]", [True, 0, 1, False])
    assert packed[:8] == struct.pack("<ii", 4, 4) and len(packed) > 8
    with pytest.raises(ServerFfiError) as ei:
        pk("list[bool]", [1, 2])  # 2 is not a bool/0/1
    assert "expected a bool" in str(ei.value)
