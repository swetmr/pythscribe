"""M1.5 server-core gates (`pythscribe.runtime`): the in-process wasmtime channel.

  R1  the K7 declared binding: this module's layout constants == the JS shim's, AND the same
      list written by the Python channel and by the JS shim is BYTE-IDENTICAL in linear memory
  R2  server == CPython == Node/V8 shim on SPOTs + a Hypothesis sweep (ints, floats, bools,
      out-buffers, host `math.pow`); mutated lists are written back into the caller's list
  R3  negative controls: a seeded mutation of the kernel is caught by the cross-path
      differential; a corrupt .wasm is refused; an i64 overflow is REFUSED, never wrapped;
      lossy args (out-of-i64 ints, non-numeric) are refused before they cross
  R4  sandbox: an import outside math.* is refused at link (with the RED half: the same
      module DOES write a file under a WASI linker); an unbounded loop hits the fuel trap
      (RED half: unmetered it runs until an epoch interrupt kills it); memory cap; fuel
      accounting is real (more work -> more fuel)
  R5  host imports == the codegen's list, with JS (not Python) semantics on the domain edges
  R6  modes: auto/server/browser/fallback resolution, explicit-request-never-degrades,
      path markers (`python_calls` vs `server_calls`), env authority
  R7  threads: per-thread instances, GIL released (N threads scale vs the CPython control)
"""
from __future__ import annotations

import json
import math
import os
import struct
import subprocess
import sys
import threading
import time
from pathlib import Path

import pytest
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from conftest import REPO, gate, gate_import, gate_node, import_module_from
from pythscribe import binding_of
from pythscribe.build import build_module
from pythscribe.decorators import ModeError

runtime = gate_import("pythscribe.runtime")
wasmtime = gate_import("wasmtime")
from pythscribe.runtime import (  # noqa: E402
    ELEM_BYTES,
    HEADER_BYTES,
    HOST_FUNCTIONS,
    LAYOUT_VERSION,
    FuelExhausted,
    Sandbox,
    SandboxViolation,
    ServerFfiError,
    ServerKernel,
    WasmTrap,
)
from pythscribe.ffi import SHIM, run_kernel  # noqa: E402

UC = REPO / "examples" / "wasm-use-cases"
sys.path.insert(0, str(UC))
import features_lib as F  # noqa: E402


@pytest.fixture(scope="module")
def K():
    mod = import_module_from(UC / "kernels.py", "kernels_uc_for_runtime_tests")
    b = binding_of(mod.edit_distance)
    gate(b.artifact is not None, f"use-case artifacts not built ({b.artifact_status}); run `python -m pythscribe.build examples/wasm-use-cases/kernels.py`")
    assert b.mode == "server", (b.mode, b.mode_reason)
    return mod


def _build(tmp_path: Path, source: str, name: str):
    tmp_path.mkdir(parents=True, exist_ok=True)
    src = tmp_path / "kernels.py"
    src.write_text(source, encoding="utf-8")
    arts = build_module(src, quiet=True)
    [a] = [a for a in arts if a.function == name]
    return a


# ----------------------------------------------------------------------------- R1 the binding
def test_r1_layout_constants_pin_the_shim():
    shim = SHIM.read_text(encoding="utf-8")
    assert f'LAYOUT_VERSION = "{LAYOUT_VERSION}"' in shim
    assert f"HEADER_BYTES = {HEADER_BYTES}" in shim
    for t, n in ELEM_BYTES.items():
        assert f'"{t}": {n}' in shim
    assert "(HEADER_BYTES + n * ELEM_BYTES[paramTypes[i]] + 7) & ~7" in shim  # the 8-byte rounding both sides do


def test_r1_python_channel_and_js_shim_write_byte_identical_memory(K, tmp_path):
    """Dump the linear memory the Python channel wrote for a list, and the memory the JS shim
    wrote for the SAME list into the SAME kernel; they must be identical bytes (header +
    elements), and start at the same offset. (One i64 list: this pins the header + element
    encoding; the 8-byte ROUNDING is pinned by the two-list test below, opus m1.5 r1/B5.)"""
    b = binding_of(K.mask_digit_runs)
    sample = [0, 1, -1, 2147483647, -2147483648, 4294967296, -4294967297, 9007199254740991, -9007199254740991, 16711935, 2**63 - 1, -(2**63)]
    k = ServerKernel.from_artifact(b.artifact)
    inst = k.new_instance()
    sp = inst.heap_ptr.value(inst.store)
    p0 = inst.alloc(inst.store, (HEADER_BYTES + 8 * len(sample) + 7) & ~7)
    inst.memory.write(inst.store, inst._pack_list("list[int]", sample, "x"), p0)
    py_bytes = inst.memory_bytes(p0, p0 + HEADER_BYTES + 8 * len(sample))
    inst.heap_ptr.set_value(inst.store, sp)
    script = tmp_path / "dump.mjs"
    script.write_text(
        "import { instantiate, HEADER_BYTES } from " + repr(SHIM.resolve().as_uri()) + ";\n"
        "const k = await instantiate(" + repr(b.artifact.wasm.resolve().as_posix()) + ");\n"
        "const ex = k.exports; const sample = " + json.dumps([str(x) for x in sample]) + ".map(BigInt);\n"
        "const p = ex.__alloc((HEADER_BYTES + 8 * sample.length + 7) & ~7);\n"
        "const dv = new DataView(ex.memory.buffer); dv.setInt32(p, sample.length, true); dv.setInt32(p + 4, sample.length, true);\n"
        "new BigInt64Array(ex.memory.buffer, p + HEADER_BYTES, sample.length).set(sample);\n"
        "console.log(JSON.stringify({ p, bytes: Array.from(new Uint8Array(ex.memory.buffer.slice(p, p + HEADER_BYTES + 8 * sample.length))) }));\n",
        encoding="utf-8",
    )
    gate_node()
    out = subprocess.run(["node", str(script)], capture_output=True, text=True, check=False)
    assert out.returncode == 0, out.stderr
    js = json.loads(out.stdout.strip().splitlines()[-1])
    assert js["p"] == p0, (js["p"], p0)  # same first allocation offset in a fresh instance
    assert bytes(js["bytes"]) == py_bytes
    assert py_bytes[:8] == struct.pack("<ii", len(sample), len(sample))


def test_r1_two_lists_odd_bool_then_i64_rounding_and_alignment_both_channels(tmp_path):
    """The allocation sequence the 8-byte rounding actually governs (opus m1.5 r1/B5): an
    ODD-length list[bool] (8 + 4*3 = 20 B -> rounded to 24) followed by a list[int]. Both
    channels must place the second list at the same, 8-aligned offset and write identical
    bytes for BOTH lists; the list[bool] pack + bool read-back arms are exercised. Delete the
    `+ 7) & ~7` on the Python side and this goes RED (the alignment assert fires); delete it
    in the shim and the offsets/bytes differ."""
    gate_node()
    art = _build(tmp_path, "from pythscribe import wasm\n\n@wasm\ndef boolpair(a: list[bool], out: list[int]) -> int:\n    n = 0\n    for x in a:\n        if x:\n            n = n + 1\n    out[0] = n\n    out[1] = len(a)\n    return n\n", "boolpair")
    flags = [True, False, True]
    ints = [7, -7, 2**40, -(2**63)]
    k = ServerKernel.from_artifact(art)
    inst = k.new_instance()
    sp = inst.heap_ptr.value(inst.store)
    r = k.call("boolpair", ["list[bool]", "list[int]"], [flags, ints], read_back=[0, 1], instance=inst)
    assert r.value == 2 and r.outs[0] == flags and r.outs[1] == [2, 3, 2**40, -(2**63)]  # bool read-back is bools; out written back
    p0 = sp
    assert sp % 8 == 0, "heap base is no longer 8-aligned; the rounding invariant assumes it"  # opus m1.5 r2/NEW-8
    p1 = sp + ((HEADER_BYTES + 4 * len(flags) + 7) & ~7)
    assert p1 == sp + 24 and (p1 + HEADER_BYTES) % 8 == 0
    py0 = inst.memory_bytes(p0, p0 + HEADER_BYTES + 4 * len(flags))
    py1 = inst.memory_bytes(p1, p1 + HEADER_BYTES + 8 * len(ints))
    assert py0 == struct.pack("<ii3i", 3, 3, 1, 0, 1)
    assert py1[:8] == struct.pack("<ii", 4, 4) and struct.unpack("<4q", py1[8:]) == (2, 3, 2**40, -(2**63))
    script = tmp_path / "pair.mjs"
    script.write_text(
        "import { instantiate, call, HEADER_BYTES } from " + repr(SHIM.resolve().as_uri()) + ";\n"
        "const k = await instantiate(" + repr(art.wasm.resolve().as_posix()) + ");\n"
        "const sp = k.exports.__heap_ptr.value;\n"
        "const r = call(k, 'boolpair', ['list[bool]', 'list[int]'], [[true, false, true], [7n, -7n, 1099511627776n, -9223372036854775808n]], { readBack: [0, 1] });\n"
        "const p1 = sp + ((HEADER_BYTES + 4 * 3 + 7) & ~7);\n"
        "const m = k.exports.memory.buffer;\n"
        "console.log(JSON.stringify({ sp, p1, value: Number(r.value), out0: Array.from(r.outs[0]), out1: r.outs[1].map(String),\n"
        "  b0: Array.from(new Uint8Array(m.slice(sp, sp + HEADER_BYTES + 12))), b1: Array.from(new Uint8Array(m.slice(p1, p1 + HEADER_BYTES + 32))) }));\n",
        encoding="utf-8",
    )
    out = subprocess.run(["node", str(script)], capture_output=True, text=True, check=False)
    assert out.returncode == 0, out.stderr
    js = json.loads(out.stdout.strip().splitlines()[-1])
    assert js["sp"] == p0 and js["p1"] == p1 and js["value"] == 2
    assert bytes(js["b0"]) == py0 and bytes(js["b1"]) == py1  # byte-identical on BOTH lists, both channels
    assert js["out0"] == [1, 0, 1] and js["out1"] == ["2", "3", "1099511627776", "-9223372036854775808"]


# ----------------------------------------------------------------------------- R2 differential
def test_r2_spots_server_equals_cpython_equals_v8(K):
    gate_node()
    b = binding_of(K.edit_distance)
    spots = [("kitten", "sitting"), ("", ""), ("a", ""), ("", "abc"), ("same", "same"), ("abcdef", "azced"), ("x" * 50, "y" * 49)]
    for a, s in spots:
        m = len(s)
        server = K.edit_distance(F.codes(a), F.codes(s), [0] * (m + 1), [0] * (m + 1))
        py = b.run_python(F.codes(a), F.codes(s), [0] * (m + 1), [0] * (m + 1))
        [r] = run_kernel(b.artifact.wasm, "edit_distance", ["list[int]"] * 4, [{"args": [F.codes(a), F.codes(s), [0] * (m + 1), [0] * (m + 1)]}])
        assert r["ok"], r
        assert server == py == int(r["value"]) == F.edit_distance_ref(a, s), (a, s, server, py, r["value"])
    assert b.server_runs() == len(spots) and b.calls() == len(spots)


def test_r2_float_bits_and_out_buffers_written_back(K):
    b = binding_of(K.dtw_distance)
    a, s = [0.0, 1.0, 2.0, 3.0, 2.0, 1.0, 0.0], [0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 2.0, 1.0, 0.0, 0.0]
    prev, cur = [0.0] * 11, [0.0] * 11
    v = K.dtw_distance(a, s, prev, cur)
    prev_py, cur_py = [0.0] * 11, [0.0] * 11
    vp = b.run_python(a, s, prev_py, cur_py)
    assert struct.pack("<d", v) == struct.pack("<d", vp) and v == F.dtw_ref(a, s)
    # the scratch rows the kernel wrote were written BACK into the caller's lists (Python semantics)
    assert prev == prev_py and cur == cur_py and prev[-1] == v
    assert b.mutated_params == frozenset({"prev", "cur"})
    # keyword calls bind by the Python signature
    assert K.dtw_distance(a=a, b=s, prev=[0.0] * 11, cur=[0.0] * 11) == v


def test_r2_host_math_pow_kernel_bit_identical(demo_kernels):
    """The M0 `rms_gain` kernel imports `math.pow`: served by `_jsmath.js_pow` on this path."""
    b = binding_of(demo_kernels.rms_gain)
    if b.mode != "server":
        pytest.skip(f"rms_gain not on the server path: {b.mode_reason}")
    assert b.server.host_imports == ("pow",)
    for xs, t in [([1.0, 2.0, 3.0, 4.0], 0.5), ([3.0, 4.0], 10.0), ([], 1.0), ([0.0], 1.0), ([1e-200], 1.0), ([1e200], -0.0), ([-1.0], float("inf")),
                  # the r8 host-libm witness: V8's Math.pow is one ULP off CPython here; the wasmtime arm (host = CPython's pow) must NOT be (r9/N9-3)
                  ([0.0] * 12 + [4.726339908522518e+99, 6.950260023638996e+99], 1.0)]:
        assert struct.pack("<d", demo_kernels.rms_gain(xs, t)) == struct.pack("<d", b.run_python(xs, t)), (xs, t)


@settings(max_examples=60, deadline=None, suppress_health_check=[HealthCheck.too_slow])
@given(a=st.text(alphabet="abcxyz", max_size=24), s=st.text(alphabet="abcxyz", max_size=24))
def test_r2_sweep_edit_distance(K, a, s):
    b = binding_of(K.edit_distance)
    m = len(s)
    assert K.edit_distance(F.codes(a), F.codes(s), [0] * (m + 1), [0] * (m + 1)) == b.run_python(F.codes(a), F.codes(s), [0] * (m + 1), [0] * (m + 1)) == F.edit_distance_ref(a, s)


@settings(max_examples=40, deadline=None, suppress_health_check=[HealthCheck.too_slow])
@given(
    a=st.lists(st.floats(-1e6, 1e6, allow_nan=False, allow_infinity=False), min_size=1, max_size=12),
    s=st.lists(st.floats(-1e6, 1e6, allow_nan=False, allow_infinity=False), min_size=1, max_size=12),
)
def test_r2_sweep_dtw_bits(K, a, s):
    b = binding_of(K.dtw_distance)
    m = len(s)
    v = K.dtw_distance(a, s, [0.0] * (m + 1), [0.0] * (m + 1))
    vp = b.run_python(a, s, [0.0] * (m + 1), [0.0] * (m + 1))
    assert struct.pack("<d", v) == struct.pack("<d", vp)


@settings(max_examples=30, deadline=None, suppress_health_check=[HealthCheck.too_slow])
@given(text=st.text(alphabet="ab 0123456789", max_size=40), min_run=st.integers(1, 6))
def test_r2_sweep_redaction_out_buffer(K, text, min_run):
    b = binding_of(K.mask_digit_runs)
    cs = F.codes(text)
    out_s, out_p = [0] * len(cs), [0] * len(cs)
    assert K.mask_digit_runs(cs, min_run, out_s) == b.run_python(cs, min_run, out_p)
    assert out_s == out_p


# ----------------------------------------------------------------------------- R3 negative controls
def test_r3_seeded_mutation_is_caught_by_the_cross_path_differential(tmp_path):
    source = (UC / "kernels.py").read_text(encoding="utf-8")
    mutated = source.replace("            cost = 1\n            if a[i - 1] == b[j - 1]:\n                cost = 0\n", "            cost = 1\n            if a[i - 1] == b[j - 1]:\n                cost = 1\n")
    assert mutated != source
    art = _build(tmp_path, mutated, "edit_distance")
    k = ServerKernel.from_artifact(art)
    a, s = F.codes("kitten"), F.codes("sitting")
    got = k.call("edit_distance", ["list[int]"] * 4, [a, s, [0] * 8, [0] * 8]).value
    assert got != F.edit_distance_ref("kitten", "sitting")  # the differential SEES the mutation


@pytest.mark.parametrize("how", ["bad-magic", "truncated", "flipped-code-byte"])
def test_r3_corrupt_wasm_is_refused(K, how):
    data = bytearray(binding_of(K.edit_distance).artifact.wasm.read_bytes())
    if how == "bad-magic":
        data[0:4] = b"\x00asn"
    elif how == "truncated":
        data = data[: len(data) * 2 // 3]
    else:
        i = 8
        while i < len(data):
            sid = data[i]
            i += 1
            size, shift = 0, 0
            while True:
                x = data[i]
                i += 1
                size |= (x & 0x7F) << shift
                shift += 7
                if not x & 0x80:
                    break
            if sid == 10:
                data[i + size // 2] ^= 0xFF
                break
            i += size
    try:
        k = ServerKernel.from_wasm(bytes(data), name="corrupt")
        got = k.call("edit_distance", ["list[int]"] * 4, [F.codes("kitten"), F.codes("sitting"), [0] * 8, [0] * 8]).value
    except (WasmTrap, SandboxViolation, ServerFfiError):
        return  # refused loudly
    assert got != 3, how  # a flipped code byte that still validates must at least change the answer


def test_r3_i64_overflow_is_refused_not_wrapped(tmp_path):
    art = _build(tmp_path, "from pythscribe import wasm\n\n@wasm\ndef sq(x: int) -> int:\n    return x * x\n", "sq")
    k = ServerKernel.from_artifact(art)
    assert k.call("sq", ["int"], [3_000_000_000]).value == 9_000_000_000_000_000_000
    with pytest.raises(OverflowError):
        k.call("sq", ["int"], [2**40])
    assert k.call("sq", ["int"], [7]).value == 49  # the stale flag was cleared at entry


def test_r3_lossy_or_wrong_typed_args_are_refused(K):
    k = binding_of(K.edit_distance).server
    with pytest.raises(ServerFfiError, match="i64 range"):
        k.call("edit_distance", ["list[int]"] * 4, [[2**63], [1], [0, 0], [0, 0]])
    with pytest.raises(ServerFfiError, match="expected an int"):
        k.call("edit_distance", ["list[int]"] * 4, [[1.5], [1], [0, 0], [0, 0]])
    with pytest.raises(ServerFfiError, match="takes 4 params"):
        k.call("edit_distance", ["list[int]"] * 4, [[1], [1], [0, 0]])
    with pytest.raises(ServerFfiError, match="exports no function"):
        k.call("nope", ["int"], [1])
    with pytest.raises(ServerFfiError, match="unsupported param type"):
        k.call("edit_distance", ["list[str]", "list[int]", "list[int]", "list[int]"], [[1], [1], [0, 0], [0, 0]])
    with pytest.raises(ServerFfiError, match="unsupported return type"):
        k.call("edit_distance", ["list[int]"] * 4, [[1], [1], [0, 0], [0, 0]], return_type="list[int]")
    with pytest.raises(ServerFfiError, match="not a list or array param"):
        k.call("edit_distance", ["list[int]"] * 4, [[1], [1], [0, 0], [0, 0]], read_back=[9])


def test_r3_bool_args_are_strict_never_coerced(tmp_path):
    """opus m1.5 r1/S1: `flag * x` with flag=2 is 6 in Python; a coercing bool arm would run the
    kernel with 1 and answer 3 -- a silent wrong value. Refused instead."""
    art = _build(tmp_path, "from pythscribe import wasm\n\n@wasm\ndef fx(flag: bool, x: int, fs: list[bool]) -> int:\n    n = 0\n    for f in fs:\n        if f:\n            n = n + 1\n    if flag:\n        return x + n\n    return n\n", "fx")
    k = ServerKernel.from_artifact(art)
    assert k.call("fx", ["bool", "int", "list[bool]"], [True, 5, [True, False]]).value == 6
    assert k.call("fx", ["bool", "int", "list[bool]"], [0, 5, [1, 0]]).value == 1  # 0/1 ints are admitted
    for bad in (2, "no", [], 1.0, None):
        with pytest.raises(ServerFfiError, match="expected a bool"):
            k.call("fx", ["bool", "int", "list[bool]"], [bad, 5, [True]])
        with pytest.raises(ServerFfiError, match="expected a bool"):
            k.call("fx", ["bool", "int", "list[bool]"], [True, 5, [bad]])


def test_r3_tuple_for_a_mutated_param_is_refused_before_the_call(K):
    """opus m1.5 r1/S11: the write-back needs a list; a tuple is refused up front with the
    right message (never a TypeError after the WASM already ran)."""
    b = binding_of(K.dtw_distance)
    before = b.server_runs()
    with pytest.raises(ServerFfiError, match="must be a list"):
        K.dtw_distance([1.0, 2.0], [1.0], (0.0, 0.0), [0.0, 0.0])
    assert b.server_runs() == before  # nothing ran


# ----------------------------------------------------------------------------- R4 sandbox
def test_r4_io_import_refused_and_the_red_half_really_does_io(tmp_path):
    wasm_bytes = F.wasi_io_module_bytes()
    with pytest.raises(SandboxViolation, match="wasi_snapshot_preview1.path_open"):
        ServerKernel.from_wasm(wasm_bytes, name="io_attempt")
    assert not (tmp_path / "escaped.txt").exists()
    wrote, content = F._run_io_module_with_wasi(wasm_bytes, tmp_path)  # a caller-built WASI linker, NOT pythscribe
    assert wrote and content == "pwned\n" and (tmp_path / "escaped.txt").is_file()  # the module is a REAL I/O attempt


def test_r4_unbounded_loop_traps_on_fuel_and_the_red_half_runs_until_killed(K):
    b = binding_of(K.spin)
    k = ServerKernel.from_artifact(b.artifact, sandbox=Sandbox(fuel=5_000_000))
    t0 = time.perf_counter()
    with pytest.raises(FuelExhausted):
        k.call("spin", ["int"], [1])
    assert time.perf_counter() - t0 < 5.0
    r = k.call("spin", ["int"], [0])
    assert r.value == 0 and 0 < r.fuel_used < 1000  # the meter is real: a finite call reports its cost
    red = F._spin_without_fuel_needs_external_kill(b.artifact.wasm.read_bytes(), run_for_s=0.25)
    assert red["still_running_at_kill"] and "interrupted" in red["outcome"], red


def test_r4_fuel_accounting_scales_with_work(K):
    b = binding_of(K.edit_distance)
    k = ServerKernel.from_artifact(b.artifact, sandbox=Sandbox(fuel=200_000_000))
    used = []
    for n in (10, 40, 160):
        a, s = F.random_text(n, 1), F.random_text(n, 2)
        used.append(k.call("edit_distance", ["list[int]"] * 4, [F.codes(a), F.codes(s), [0] * (n + 1), [0] * (n + 1)]).fuel_used)
    assert used[0] < used[1] < used[2] and used[2] > 8 * used[1]
    with pytest.raises(FuelExhausted):
        a, s = F.random_text(160, 1), F.random_text(160, 2)
        k.call("edit_distance", ["list[int]"] * 4, [F.codes(a), F.codes(s), [0] * 161, [0] * 161], fuel=used[1])
    with pytest.raises(ValueError):
        Sandbox(fuel=0)
    with pytest.raises(ValueError):  # per-call fuel on an unmetered kernel is refused, not ignored
        ServerKernel.from_artifact(b.artifact).call("edit_distance", ["list[int]"] * 4, [[1], [1], [0, 0], [0, 0]], fuel=10)


def test_r4_memory_cap_is_enforced(K):
    b = binding_of(K.count_vowels)
    k = ServerKernel.from_artifact(b.artifact, sandbox=Sandbox(memory_bytes=1 << 20))
    assert k.call("count_vowels", ["list[int]"], [[97] * 1000]).value == 1000
    with pytest.raises(SandboxViolation, match="memory cap"):
        k.call("count_vowels", ["list[int]"], [[97] * 400_000])  # 3.2 MB > 1 MiB


def test_r4_fresh_instance_shares_nothing(K):
    b = binding_of(K.mask_digit_runs)
    k = ServerKernel.from_artifact(b.artifact)
    i1, i2 = k.new_instance(), k.new_instance()
    cs = F.codes("card 4111111111111111")
    r1 = k.call("mask_digit_runs", ["list[int]", "int", "list[int]"], [cs, 4, [0] * len(cs)], read_back=[2], instance=i1)
    assert "".join(map(chr, r1.outs[2])) == "card ****************"
    # a pattern written into the FIRST instance's memory is invisible at the same offset in the second
    p = i1.alloc(i1.store, 64)
    i1.memory.write(i1.store, b"\xab" * 64, p)
    assert i1.memory_bytes(p, p + 64) == b"\xab" * 64
    assert i2.memory_bytes(p, p + 64) == bytes(64)
    assert k.instances >= 2
    # an instance of ANOTHER kernel is refused (codex m1.5 r2/#3): it would run that module's export
    other_art = binding_of(K.count_vowels).artifact
    assert other_art is not None
    other = ServerKernel.from_artifact(other_art)
    with pytest.raises(ServerFfiError, match="belongs to kernel"):
        k.call("mask_digit_runs", ["list[int]", "int", "list[int]"], [cs, 4, [0] * len(cs)], instance=other.new_instance())
    other.close()
    assert other.module_released
    # close() refuses further calls cleanly and releases the instances + the Module
    k.close()
    with pytest.raises(ServerFfiError, match="closed"):
        k.call("mask_digit_runs", ["list[int]", "int", "list[int]"], [cs, 4, [0] * len(cs)])
    assert i1.closed and i2.closed and k.module_released and not k.release_pending
    k.close()  # idempotent


def test_r4_close_from_another_thread_never_frees_an_in_flight_store(K):
    """opus m1.5 r2/NEW-5 + r3/N2/N3 + r4/NEW-1/NEW-4: a worker parked inside a REAL call's
    prologue (a proxied `heap_ptr.value` -- the very store operation that used to precede the
    in_call flag) keeps its Store while the main thread closes the kernel; the release is
    deferred (`release_pending`), the parked call COMPLETES with the right value, the worker's
    next call is refused, and the worker's own exit completes the release -- the Module is
    never leaked. RED halves: with the flag set after the prologue (5c6ea564) close() frees the
    store under the worker and the parked call raises `ValueError: already closed` (no value);
    a permanently leaked Module leaves `module_released` False after the worker finishes."""
    b = binding_of(K.count_vowels)  # a kernel WITH a heap (spin has no linear memory, hence no __heap_ptr to park on)
    k = ServerKernel.from_artifact(b.artifact, sandbox=Sandbox(fuel=400_000_000))
    started, release = threading.Event(), threading.Event()
    result: dict = {}
    cs = [97, 98, 101] * 10  # 20 vowels

    class ParkedHeapPtr:  # stands where `self.heap_ptr.value(store)` issues its C call in the prologue
        def __init__(self, real):
            self.real = real

        def value(self, store):
            result["in_call_at_park"] = result["inst"].in_call
            started.set()
            release.wait(10)
            return self.real.value(store)

        def set_value(self, store, v):
            self.real.set_value(store, v)

    def worker():
        inst = k.new_instance()
        result["inst"] = inst
        assert inst.heap_ptr is not None
        inst.heap_ptr = ParkedHeapPtr(inst.heap_ptr)
        try:
            result["value"] = k.call("count_vowels", ["list[int]"], [cs], instance=inst).value  # a REAL call, parked mid-prologue
        except Exception as e:  # noqa: BLE001 - the RED half surfaces here as ValueError('already closed')
            result["value"] = f"{type(e).__name__}: {e}"
        try:
            k.call("count_vowels", ["list[int]"], [cs], instance=inst)
            result["after"] = "ran"
        except ServerFfiError as e:
            result["after"] = str(e)

    t = threading.Thread(target=worker)
    t.start()
    assert started.wait(10)
    inst = result["inst"]
    assert result["in_call_at_park"] is True  # the flag was set BEFORE the first store operation
    k.close()  # from the MAIN thread while the worker is in-flight
    assert inst.closed is False and k.release_pending is True and k.module_released is False  # deferred, not freed under it
    release.set()
    t.join(10)
    assert result["value"] == 20, result  # the in-flight call completed correctly on a live Store
    assert "closed" in result["after"], result  # ...and the next call was refused
    assert inst.closed and k.module_released and not k.release_pending  # the worker's own exit completed the release
    with pytest.raises(ServerFfiError, match="closed"):
        k.new_instance()  # r4/NEW-3
    k.close()  # re-entrant, idempotent


def test_r4_close_inside_own_call_is_refused(K):
    """An instance cannot be closed from inside its own call (the flag is real, not decorative)."""
    b = binding_of(K.count_vowels)
    k = ServerKernel.from_artifact(b.artifact)
    inst = k.new_instance()
    inst.in_call = True
    with pytest.raises(ServerFfiError, match="inside its own call"):
        inst.close()
    inst.in_call = False
    inst.close()
    k.close()
    assert k.module_released


# ----------------------------------------------------------------------------- R5 host imports
def test_r5_host_imports_match_codegen():
    """The static five-way binding (bridge.rs <-> emit.rs arities <-> _jsmath <-> shim <-> built
    bundle) lives in test_bindings.py, which needs NO wasmtime (r3/N5); this keeps the
    runtime-side check that the loaded HOST_MATH is what the server links."""
    from pythscribe.runtime._jsmath import HOST_MATH

    assert tuple(sorted(HOST_MATH)) == HOST_FUNCTIONS and all(callable(f) and a in (1, 2) for a, f in HOST_MATH.values())


def test_r5_fabs_kernel_runs_on_the_shim_and_the_server(tmp_path):
    """The concrete NEW-3 scenario: a kernel calling math.fabs -- the shim (Node/V8) and the
    server agree with CPython; before the fix the shim threw `unsupported WASM import math.fabs`."""
    gate_node()
    src = "import math\n\nfrom pythscribe import wasm\n\n\n@wasm\ndef kfabs(x: float) -> float:\n    return math.fabs(x) + math.sqrt(2.0)\n"
    d = tmp_path / "fabs"
    d.mkdir()
    (d / "kernels.py").write_text(src, encoding="utf-8")
    [art] = build_module(d / "kernels.py", quiet=True)  # a refusal of math.fabs is a FINDING, not an environment (r3/N4)
    k = ServerKernel.from_artifact(art)
    assert set(k.host_imports) == {"fabs", "sqrt"}
    import math as _m

    for x in (-3.5, 0.0, -0.0, 2.25, float("inf")):
        expect = _m.fabs(x) + _m.sqrt(2.0)
        assert struct.pack("<d", k.call("kfabs", ["float"], [x], return_type="float").value) == struct.pack("<d", expect)
        [r] = run_kernel(art.wasm, "kfabs", ["float"], [{"args": [x]}], return_type="float") if _m.isfinite(x) else [None]
        if r is not None:
            assert r["ok"], r
            assert struct.pack("<d", float(r["value"])) == struct.pack("<d", expect)
    k.close()


def test_r5_host_math_has_js_semantics_not_python():
    from pythscribe.runtime._jsmath import HOST_MATH, js_pow

    inf, nan = float("inf"), float("nan")
    assert math.isnan(HOST_MATH["sqrt"][1](-1.0)) and HOST_MATH["log"][1](0.0) == -inf and math.isnan(HOST_MATH["log"][1](-1.0))
    assert HOST_MATH["exp"][1](1000.0) == inf and math.isnan(HOST_MATH["sin"][1](inf)) and math.isnan(HOST_MATH["asin"][1](2.0))
    assert js_pow(0.0, -1.0) == inf and js_pow(-0.0, -1.0) == -inf and js_pow(-8.0, 1 / 3) != js_pow(-8.0, 1 / 3)  # NaN
    assert math.isnan(js_pow(1.0, inf)) and math.isnan(js_pow(-1.0, -inf)) and js_pow(nan, 0.0) == 1.0 and js_pow(0.5, inf) == 0.0 and js_pow(2.0, -inf) == 0.0
    # and Python's own math.pow would have RAISED on these (the semantics really differ)
    with pytest.raises(ValueError):
        math.pow(0.0, -1.0)
    with pytest.raises(ValueError):
        math.sqrt(-1.0)
    assert js_pow(-2.0, 3.0) == -8.0 and js_pow(10.0, 400.0) == inf and js_pow(-10.0, 401.0) == -inf
    assert struct.pack("<d", HOST_MATH["ceil"][1](-0.5)) == struct.pack("<d", -0.0)
    # the -0.0 INPUT itself (opus m1.5 r1/B4): Math.ceil(-0) and Math.floor(-0) are -0; +0 stays +0
    neg0, pos0 = struct.pack("<d", -0.0), struct.pack("<d", 0.0)
    assert struct.pack("<d", HOST_MATH["ceil"][1](-0.0)) == neg0 and struct.pack("<d", HOST_MATH["floor"][1](-0.0)) == neg0
    assert struct.pack("<d", HOST_MATH["ceil"][1](0.0)) == pos0 and struct.pack("<d", HOST_MATH["floor"][1](0.5)) == pos0
    assert struct.pack("<d", HOST_MATH["floor"][1](-0.5)) == struct.pack("<d", -1.0) and HOST_MATH["ceil"][1](0.5) == 1.0
    assert HOST_MATH["floor"][1](inf) == inf and HOST_MATH["atan2"][1](0.0, -0.0) == math.pi


def test_r5_module_with_unknown_math_import_is_refused():
    # M2.1: the ABI gate runs FIRST (a non-pyths module is refused before the sandbox even looks at
    # its imports), so these hand-built modules carry a valid `pyths.abi` section to reach the
    # import check they exercise.
    from _abi_helpers import stamp_abi_section

    wat = '(module (import "math" "cbrt" (func (param f64) (result f64))) (func (export "f") (result f64) f64.const 8 call 0))'
    with pytest.raises(SandboxViolation, match="math.cbrt"):
        ServerKernel.from_wasm(stamp_abi_section(wasmtime.wat2wasm(wat)), name="cbrt")
    wat2 = '(module (import "math" "sqrt" (func (param i32) (result i32))) (func (export "f") (result i32) i32.const 4 call 0))'
    with pytest.raises(SandboxViolation, match="signature"):
        ServerKernel.from_wasm(stamp_abi_section(wasmtime.wat2wasm(wat2)), name="badsig")
    # and WITHOUT the section the ABI gate (not the sandbox) is what refuses it -- loudly
    from pythscribe.runtime import AbiMismatchError

    with pytest.raises(AbiMismatchError, match="no `pyths.abi` custom section"):
        ServerKernel.from_wasm(wasmtime.wat2wasm(wat), name="cbrt-unstamped")


# ----------------------------------------------------------------------------- R6 modes
def _copy_uc(tmp_path: Path, K, with_artifact: bool, name: str):
    d = tmp_path / name
    d.mkdir()
    import shutil

    shutil.copyfile(UC / "kernels.py", d / "kernels.py")
    if with_artifact:  # an explicit mode applies to EVERY kernel of the module: copy all artifacts
        shutil.copytree(UC / "__pythscribe__", d / "__pythscribe__")
    return d / "kernels.py"


def test_r6_auto_mode_resolution(K, tmp_path):
    b = binding_of(K.edit_distance)
    assert b.mode == "server" and b.mode_reason.startswith("auto") and b.server_ready and b.browser_ready
    mod = import_module_from(_copy_uc(tmp_path, K, False, "noart"))
    b2 = binding_of(mod.edit_distance)
    assert b2.mode == "fallback" and b2.artifact_status == "absent" and b2.server is None
    assert F.edit_distance_call(mod.edit_distance, "kitten", "sitting") == 3 and b2.calls() == 1 and b2.server_runs() == 0


def test_r6_explicit_modes_and_env_authority(K, tmp_path, monkeypatch):
    # PYTHSCRIBE_MODE=browser: artifact bound, direct calls run Python
    monkeypatch.setenv("PYTHSCRIBE_MODE", "browser")
    mod = import_module_from(_copy_uc(tmp_path, K, True, "browser"))
    b = binding_of(mod.edit_distance)
    assert b.mode == "browser" and b.browser_ready and b.server is None
    F.edit_distance_call(mod.edit_distance, "kitten", "sitting")
    assert b.calls() == 1 and b.server_runs() == 0
    # PYTHSCRIBE_MODE=fallback: nothing bound even though the artifact exists (same authority as disabling)
    monkeypatch.setenv("PYTHSCRIBE_MODE", "fallback")
    mod = import_module_from(_copy_uc(tmp_path, K, True, "fb"))
    b = binding_of(mod.edit_distance)
    assert b.mode == "fallback" and b.artifact is None and b.artifact_status == "disabled"
    # PYTHSCRIBE_MODE=server: honoured when capable ...
    monkeypatch.setenv("PYTHSCRIBE_MODE", "server")
    mod = import_module_from(_copy_uc(tmp_path, K, True, "srv"))
    assert binding_of(mod.edit_distance).mode == "server" and binding_of(mod.edit_distance).mode_reason == "requested"
    # ... and NEVER silently degraded: no artifact -> ModeError at import
    with pytest.raises(ModeError, match="mode 'server' requested but unavailable"):
        import_module_from(_copy_uc(tmp_path, K, False, "srv_noart"))
    monkeypatch.setenv("PYTHSCRIBE_MODE", "browser")
    with pytest.raises(ModeError, match="mode 'browser' requested but no usable artifact"):
        import_module_from(_copy_uc(tmp_path, K, False, "br_noart"))
    monkeypatch.setenv("PYTHSCRIBE_MODE", "warp")
    with pytest.raises(ModeError, match="expected one of"):
        import_module_from(_copy_uc(tmp_path, K, True, "warp"))


def test_r6_decorator_mode_and_fuel_arguments(K, tmp_path, monkeypatch, import_source):
    monkeypatch.delenv("PYTHSCRIBE_MODE", raising=False)
    d = _copy_uc(tmp_path, K, True, "deco").parent
    src = (d / "kernels.py").read_text(encoding="utf-8").replace("@wasm\ndef spin(", "@wasm(fuel=3_000_000)\ndef spin(")
    (d / "kernels.py").write_text(src, encoding="utf-8")
    mod = import_module_from(d / "kernels.py")
    b = binding_of(mod.spin)
    assert b.mode == "server" and b.fuel == 3_000_000 and b.server.sandbox.fuel == 3_000_000
    with pytest.raises(FuelExhausted):
        mod.spin(1)
    assert mod.spin(0) == 0 and b.server_runs() == 1
    # mode= on the decorator wins over the environment
    monkeypatch.setenv("PYTHSCRIBE_MODE", "server")
    mod2 = import_source("from pythscribe import wasm\n\n@wasm(mode='fallback')\ndef f(x: int) -> int:\n    return x + 1\n")
    assert binding_of(mod2.f).mode == "fallback" and mod2.f(1) == 2
    monkeypatch.setenv("PYTHSCRIBE_FUEL", "abc")
    with pytest.raises(ModeError, match="not an int"):
        import_source("from pythscribe import wasm\n\n@wasm\ndef g(x: int) -> int:\n    return x\n")


def test_r6_boxed_target_witnesses_through_the_shipped_path(tmp_path, monkeypatch):
    """opus m1.5 r5/R5-1 witnesses (their own test, r7/NEW-5, so a compiler that stops admitting the
    shapes skips THIS case only): each boxed-target kernel compiles at 0.2.4; without target_leak it
    bound to mode server, returned the right scalar and left the caller's list stale."""
    from pythscribe.build import BuildError

    monkeypatch.delenv("PYTHSCRIBE_MODE", raising=False)
    ran = 0
    for i, body in enumerate(("    [out][0][0] = 7\n", "    [out][0][0] += 6\n", "    [out, out][1][0] = 7\n", "    (out,)[0][0] = 7\n", "    [[out]][0][0][0] = 7\n")):
        d3 = tmp_path / f"box{i}"
        d3.mkdir()
        (d3 / "kernels.py").write_text("from pythscribe import wasm\n\n@wasm\ndef h(out: list[int]) -> int:\n" + body + "    return 1\n", encoding="utf-8")
        try:
            build_module(d3 / "kernels.py", quiet=True)
        except BuildError as e:
            if "not supported" in str(e) or "cannot be honored" in str(e):
                continue  # this shape is compiler-refused now: unreachable, the AST case above is the gate
            raise
        mod3 = import_module_from(d3 / "kernels.py")
        b3 = binding_of(mod3.h)
        assert b3.mode == "browser" and "assignment target" in b3.mode_reason, (body, b3.mode_reason)
        out3 = [1, 2, 3]
        assert mod3.h(out3) == 1 and out3 == [7, 2, 3] and b3.server_runs() == 0, (body, out3)
        ran += 1
    if ran == 0:  # r6/R6-3: a witness loop that ran nothing must SAY so, never pass green silently
        pytest.skip("the compiler now refuses every boxed-target shape; the AST cases above are the gate")


def test_r6_list_comparison_witness_through_the_shipped_path(tmp_path, monkeypatch):
    """opus m1.5 r7/NEW-1 -- the load-bearing row: `if out == o:` compiles at 0.2.4 and the WASM lowering
    compares HANDLES, so on the server path the wrong branch ran and the caller's list came back
    corrupted ([8,2] where CPython gives [9,2]). Refused now: mode browser, CPython semantics kept,
    explicit server -> ModeError. RED at 3af53c4e."""
    from pythscribe.build import BuildError

    monkeypatch.delenv("PYTHSCRIBE_MODE", raising=False)
    d = tmp_path / "cmp"
    d.mkdir()
    (d / "kernels.py").write_text(
        "from pythscribe import wasm\n\n@wasm\ndef c(out: list[int], o: list[int]) -> int:\n    if out == o:\n        out[0] = 9\n        return 1\n    out[0] = 8\n    return 0\n",
        encoding="utf-8",
    )
    try:
        build_module(d / "kernels.py", quiet=True)
    except BuildError as e:
        if "not supported" in str(e) or "cannot be honored" in str(e):
            pytest.skip(f"the compiler now refuses list comparison ({str(e)[:100]}); the AST case is the gate")
        raise
    mod = import_module_from(d / "kernels.py")
    b = binding_of(mod.c)
    assert b.mode == "browser" and "position of" in b.mode_reason, b.mode_reason
    out = [1, 2]
    assert mod.c(out, [1, 2]) == 1 and out == [9, 2] and b.server_runs() == 0  # CPython semantics (the WASM answered 0 / [8, 2])
    monkeypatch.setenv("PYTHSCRIBE_MODE", "server")
    with pytest.raises(ModeError, match="position of"):
        import_module_from(d / "kernels.py")


def test_r6_server_unavailable_when_wasmtime_missing_degrades_to_browser(K, tmp_path, monkeypatch):
    """Simulate a machine without wasmtime: auto -> browser (artifact bound, Python direct
    calls), explicit server -> ModeError. The simulation is the import hook, not a flag."""
    import pythscribe.runtime as rt

    monkeypatch.setattr(rt, "wasmtime_available", lambda: False)
    mod = import_module_from(_copy_uc(tmp_path, K, True, "nowt"))
    b = binding_of(mod.edit_distance)
    assert b.mode == "browser" and "wasmtime-py is not installed" in b.mode_reason
    monkeypatch.setenv("PYTHSCRIBE_MODE", "server")
    with pytest.raises(ModeError, match="wasmtime-py is not installed"):
        import_module_from(_copy_uc(tmp_path, K, True, "nowt2"))


def test_r6_signature_outside_grammar_degrades_to_browser(tmp_path, monkeypatch):
    """A kernel with a keyword-only param builds (the compiler admits it) but the FFI grammar
    refuses to call it by position: mode browser, never a mis-marshalled server call."""
    monkeypatch.delenv("PYTHSCRIBE_MODE", raising=False)
    d = tmp_path / "kw"
    d.mkdir()
    (d / "kernels.py").write_text("from pythscribe import wasm\n\n@wasm\ndef f(x: int, *, k: int) -> int:\n    return x + k\n", encoding="utf-8")
    try:
        build_module(d / "kernels.py", quiet=True)
    except Exception as e:  # the compiler may refuse keyword-only params outright: also fine
        pytest.skip(f"compiler refused the kernel: {e}")
    mod = import_module_from(d / "kernels.py")
    b = binding_of(mod.f)
    assert b.mode == "browser" and "FFI grammar" in b.mode_reason and mod.f(1, k=2) == 3 and b.calls() == 1


def test_r6_list_use_the_write_back_cannot_honour_is_refused(tmp_path, monkeypatch):
    """opus m1.5 r1/S2: aliasing / rebinding / passing / slice-assigning / del / length-changing
    a list parameter would make the read-back silently wrong; each is REFUSED (the server
    path is not bound; direct calls run the Python body) with the reason recorded."""
    import ast

    from pythscribe.runtime import mutated_list_params, unsupported_list_use

    def node(src: str):
        return ast.parse(src).body[0]

    ok = node("def f(out: list[int], k: int) -> int:\n    out[0] = k\n    out[1] += 1\n    return k\n")
    assert unsupported_list_use(ok) is None and mutated_list_params(ok) == frozenset({"out"})
    cases = {
        "aliased": "def f(out: list[int]) -> int:\n    tmp = out\n    tmp[0] = 1\n    return 1\n",
        "rebound": "def f(out: list[int]) -> int:\n    out = [0]\n    out[0] = 1\n    return 1\n",
        "passed to a call": "def f(out: list[int]) -> int:\n    g(out)\n    return 1\n",
        "slice assignment": "def f(out: list[int]) -> int:\n    out[0:2] = [1, 2, 3]\n    return 1\n",
        "changes its length": "def f(out: list[int]) -> int:\n    del out[0]\n    return 1\n",
        "append": "def f(out: list[int]) -> int:\n    out.append(1)\n    return 1\n",
        "aliased via walrus": "def f(out: list[int]) -> int:\n    (t := out)[0] = 1\n    return 1\n",
        # codex m1.5 r2/#1: an alias produced by an EXPRESSION, not a bare name
        "aliased via IfExp": "def f(out: list[int], c: bool) -> int:\n    tmp: list[int] = out if c else out\n    tmp[0] = 7\n    return 1\n",
        "aliased via list literal": "def f(out: list[int]) -> int:\n    tmp = [out][0]\n    tmp[0] = 7\n    return 1\n",
        "aliased via BoolOp": "def f(out: list[int], o: list[int]) -> int:\n    tmp = o or out\n    tmp[0] = 7\n    return 1\n",
        "returned": "def f(out: list[int]) -> list[int]:\n    return out\n",
        # opus m1.5 r3/N1: the THIRD binding site -- a loop variable bound to the list itself (compiles at 0.2.4)
        "aliased via for-over-list-literal": "def f(out: list[int]) -> int:\n    for tmp in [out]:\n        tmp[0] = 7\n    return 1\n",
        "aliased via for-over-two-params": "def f(a: list[int], b: list[int]) -> int:\n    for tmp in [a, b]:\n        tmp[0] = 7\n    return 1\n",
        "aliased via comprehension iterable": "def f(out: list[int]) -> int:\n    n = sum(len(t) for t in [out])\n    return n\n",
        # r4/NEW-5: a BARE comprehension statement is the only shape that reaches the comprehension.iter arm
        "aliased via bare comprehension": "def f(out: list[int]) -> int:\n    [t for t in [out]]\n    return 1\n",
        # opus m1.5 r5/R5-1: assignment TARGETS that box the param (all six compile at 0.2.4 and dropped the mutation)
        "aliased via boxed target": "def f(out: list[int]) -> int:\n    [out][0][0] = 7\n    return 1\n",
        "aliased via boxed augmented target": "def f(out: list[int]) -> int:\n    [out][0][0] += 7\n    return 1\n",
        "aliased via two-element box": "def f(out: list[int]) -> int:\n    [out, out][1][0] = 7\n    return 1\n",
        "aliased via parenthesised box": "def f(out: list[int]) -> int:\n    ([out])[0][0] = 7\n    return 1\n",
        "aliased via tuple box": "def f(out: list[int]) -> int:\n    (out,)[0][0] = 7\n    return 1\n",
        "aliased via nested box": "def f(out: list[int]) -> int:\n    [[out]][0][0][0] = 7\n    return 1\n",
        "aliased via index expression": "def f(out: list[int], o: list[int]) -> int:\n    o[[out][0][0]] = 7\n    return 1\n",
        # r5/R5-2 (latent): a bare expression statement carrying an alias
        "aliased via bare expression": "def f(out: list[int]) -> int:\n    print([out])\n    return 1\n",
        # opus m1.5 r7/NEW-1: a list-to-list COMPARISON (the WASM lowering compares handles: `if out == o` decided the
        # wrong branch and corrupted the caller's list); only `x in out` / `x not in out` is an element-wise read
        "reaches via ==": "def f(out: list[int], o: list[int]) -> int:\n    if out == o:\n        return 1\n    return 0\n",
        "returned via !=": "def f(out: list[int], o: list[int]) -> int:\n    return 1 if out != o else 0\n",
        "reaches via != in a while test": "def f(out: list[int], o: list[int]) -> int:\n    while out != o:\n        return 1\n    return 0\n",
        "aliased via < in a value": "def f(out: list[int], o: list[int]) -> int:\n    n = out < o\n    return 1\n",
        "reaches via == literal": "def f(out: list[int]) -> int:\n    if out == [1, 2]:\n        return 1\n    return 0\n",
        "reaches via left-of-in": "def f(out: list[int], o: list[int]) -> int:\n    if out in o:\n        return 1\n    return 0\n",
        "reaches via truthiness": "def f(out: list[int]) -> int:\n    if out:\n        return 1\n    return 0\n",
    }
    for src in ("def f(out: list[int], k: int) -> int:\n    if k in out:\n        return 1\n    return 0\n",
                "def f(out: list[int], k: int) -> int:\n    if k not in out and out[0] > 0:\n        return 1\n    return 0\n"):
        assert unsupported_list_use(node(src)) is None  # membership on the RIGHT stays allowed
    # (the enclosing bare-expression arm, r5/R5-2, sees it first; the comprehension.iter arm is reached when the
    # comprehension is nested in a statement that has no arm of its own, e.g. a while-test)
    assert "bare expression" in unsupported_list_use(node(cases["aliased via bare comprehension"]))
    # (r6/R6-1: the positional walk refuses it at the `While.test` position before the comprehension arm can)
    assert "`test` position" in unsupported_list_use(node("def f(out: list[int]) -> int:\n    while [t for t in [out]]:\n        return 1\n    return 0\n"))
    loops_fine = node("def f(out: list[int]) -> int:\n    n = 0\n    for i in range(len(out)):\n        n = n + out[i]\n    for x in out:\n        n = n + x\n    return n\n")
    assert unsupported_list_use(loops_fine) is None
    # reads that are NOT aliases stay allowed: indexing, len(), iteration, membership
    fine = node("def f(out: list[int], k: int) -> int:\n    n = len(out)\n    v = out[0] + out[n - 1]\n    for x in out:\n        v = v + x\n    if k in out:\n        v = v + 1\n    out[0] = v\n    return v\n")
    assert unsupported_list_use(fine) is None and mutated_list_params(fine) == frozenset({"out"})
    # the ARM that fired is asserted too (r6/R6-3): "aliased" alone would not discriminate
    arm_of = {"aliased via boxed": "assignment target", "aliased via two-element": "assignment target", "aliased via parenthesised": "assignment target",
              "aliased via tuple box": "assignment target", "aliased via nested box": "assignment target", "aliased via index": "assignment target",
              "aliased via for-over": "loop iterable", "aliased via bare comprehension": "bare expression", "aliased via bare expression": "bare expression",
              "aliased via comprehension iterable": "flows into", "aliased via IfExp": "flows into", "aliased via list literal": "flows into",
              "aliased via BoolOp": "flows into", "aliased via walrus": "assignment target", "aliased": "flows into",
              "reaches via": "position of"}
    for why, src in cases.items():
        reason = unsupported_list_use(node(src))
        assert reason and why.split()[0] in reason, (why, reason)
        arm = next((v for k, v in arm_of.items() if why.startswith(k)), None)
        if arm:
            assert arm in reason, (why, arm, reason)
    # r6/R6-1: the positions that have no shipped-path witness (the compiler refuses them at 0.2.4) are
    # still REFUSED at the AST level -- the walk is positional, not a list of shapes
    for why, src in {
        "for-target box": "def f(out: list[int]) -> int:\n    n = 0\n    for [out][0][0] in range(9, 10):\n        n = n + 1\n    return n\n",
        "del box": "def f(out: list[int]) -> int:\n    del [out][0][0]\n    return 1\n",
        "if-test alias": "def f(out: list[int]) -> int:\n    if [out][0].append(9) is None:\n        return 1\n    return 0\n",
        "while-test alias": "def f(out: list[int]) -> int:\n    while [out][0]:\n        return 1\n    return 0\n",
        "assert alias": "def f(out: list[int]) -> int:\n    assert [out][0].append(9) is None\n    return 1\n",
        "with-as rebinding": "def f(out: list[int], o: list[int]) -> int:\n    with o as out:\n        out[0] = 1\n    return 1\n",
        "except-as rebinding": "def f(out: list[int]) -> int:\n    try:\n        return 1\n    except ValueError as out:\n        return 0\n",
        "augmented slice": "def f(out: list[int]) -> int:\n    out[0:2] += [9]\n    return 1\n",
        "builtin call with boxed second arg": "def f(out: list[int]) -> int:\n    print(out, [out][0])\n    return 1\n",
    }.items():
        assert unsupported_list_use(node(src)), why
    # legitimate targets stay allowed: nested subscripts rooted at the param, and index expressions that only read
    ok2 = node("def f(out: list[int], k: int) -> int:\n    out[out[0]] = k\n    out[k] += out[1]\n    return k\n")
    assert unsupported_list_use(ok2) is None and mutated_list_params(ok2) == frozenset({"out"})
    # through the real decorator: the kernel stays on the Python body with the reason recorded
    monkeypatch.delenv("PYTHSCRIBE_MODE", raising=False)
    d = tmp_path / "alias"
    d.mkdir()
    (d / "kernels.py").write_text("from pythscribe import wasm\n\n@wasm\ndef f(out: list[int]) -> int:\n    tmp = out\n    tmp[0] = 7\n    return 1\n", encoding="utf-8")
    try:
        build_module(d / "kernels.py", quiet=True)
    except Exception as e:
        pytest.skip(f"compiler refused the alias kernel ({e}); the AST predicate above is the gate")
    mod = import_module_from(d / "kernels.py")
    b = binding_of(mod.f)
    assert b.mode == "browser" and "aliased" in b.mode_reason, b.mode_reason
    out = [0]
    assert mod.f(out) == 1 and out == [7] and b.calls() == 1 and b.server_runs() == 0  # Python semantics kept
    monkeypatch.setenv("PYTHSCRIBE_MODE", "server")
    with pytest.raises(ModeError, match="aliased"):
        import_module_from(d / "kernels.py")
    # the N1 witness through the SHIPPED path: the loop-alias kernel compiles, so without the
    # For.iter arm it would bind to server mode and silently drop the caller's mutation
    monkeypatch.delenv("PYTHSCRIBE_MODE", raising=False)
    d2 = tmp_path / "loopalias"
    d2.mkdir()
    (d2 / "kernels.py").write_text("from pythscribe import wasm\n\n@wasm\ndef g(out: list[int]) -> int:\n    for tmp in [out]:\n        tmp[0] = 7\n    return 1\n", encoding="utf-8")
    from pythscribe.build import BuildError

    try:
        build_module(d2 / "kernels.py", quiet=True)
    except BuildError as e:  # ONLY an explicit compiler refusal disables the witness (r4/NEW-8a); anything else is a failure
        if "not supported" in str(e) or "cannot be honored" in str(e):
            pytest.skip(f"compiler now refuses the loop-alias kernel ({str(e)[:120]}); the AST predicate above is the gate")
        raise
    mod2 = import_module_from(d2 / "kernels.py")
    b2 = binding_of(mod2.g)
    assert b2.mode == "browser" and "loop iterable" in b2.mode_reason, b2.mode_reason
    out2 = [1, 2, 3]
    assert mod2.g(out2) == 1 and out2 == [7, 2, 3] and b2.server_runs() == 0  # Python semantics kept


# ----------------------------------------------------------------------------- R7 threads
def test_r7_per_thread_instances_and_gil_released(K):
    """4 threads of the WASM kernel finish in well under 4x the single-thread time, while the
    same 4 threads of the CPython body do not (the GIL). Loose thresholds: direction, not a
    benchmark (the notebook reports the measured scaling)."""
    b = binding_of(K.edit_distance)
    threads = 4
    n = 900
    if (os.cpu_count() or 1) < threads:  # opus m1.5 r1/S8: an environmental limit is not a failed claim
        pytest.skip(f"fan-out scaling needs >= {threads} CPUs (have {os.cpu_count()})")
    rec = F.measure_fanout(K, threads=threads, n=n, seed=3, reps=2)
    assert rec["wasm_scaling"] > 1.5, rec
    assert rec["wasm_scaling"] > rec["cpython_scaling"] * 1.3, rec
    assert b.server.instances >= threads + 1
    ids = set()

    def rec_thread():
        ids.add(b.server._instance().owner)

    ts = [threading.Thread(target=rec_thread) for _ in range(3)]
    for t in ts:
        t.start()
    for t in ts:
        t.join()
    assert len(ids) == 3  # three distinct owners: per-thread instances
    inst = b.server.new_instance()
    err = []

    def cross():
        try:
            inst.call("edit_distance", ["list[int]"] * 4, [[1], [1], [0, 0], [0, 0]], read_back=[], return_type="int", fuel=None)
        except ServerFfiError as e:
            err.append(str(e))

    t = threading.Thread(target=cross)
    t.start()
    t.join()
    assert err and "single-threaded" in err[0]  # a Store is never shared across threads
