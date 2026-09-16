"""M2a-3b gate: 1-D numeric `Array[dtype]` kernels run END-TO-END on the js+wasm
EMITTED-GLUE path (real `pyths --target js+wasm` -> Node -> the `__array_to_wasm` /
`__array_write_back` glue over WASM linear memory), out-buffer == NumPy. The
browser/node counterpart of `test_array_server_e2e.py` (which exercises the SERVER
wasmtime path). Every property ships a positive shipped-path SPOT AND a paired
NEGATIVE CONTROL that goes RED (feedback_anti_vacuity_paired_control).

  N1  per-dtype fill/transform kernel: js+wasm glue out-buffer == NumPy bit-for-bit
      (int/uint8, f64) / <=tolerance (f32); PBT over sizes+values.
  N2  MANDATORY wrap-boundary witnesses: int32 near 2**31, uint8 255+1, int64 near
      2**63 -> NumPy WRAP on the glue path too.
  N3  layout byte-agreement: the emitted glue header offsets == array_buffer.py
      constants (16-byte x8 header, shape0@8, elems@16, dtype tags) — a layout
      change goes RED.
  N4  RED mutant: drop the `__array_write_back` call -> the caller's out-buffer is
      unchanged (write-back is load-bearing).
  N5  RED mutant: remove the total runtime dtype check -> a wrong-dtype buffer is
      flat-copied and read at the WRONG width (silent misread) — vs the real glue,
      which throws RangeError and REROUTES to the JS twin (sound-by-refusal).
  N6  WASM-path marker (anti dual-track-masking): a uint8 kernel SUCCEEDS on the
      glue path, but its JS twin THROWS ('bytes' is immutable in the runtime) — so
      a correct uint8 wrap result PROVES the WASM path ran, not the twin.
  #489 is NOT in this chunk (sub-split to M2a-3c — see the report).
"""
from __future__ import annotations

import json
import re
import shutil
import subprocess
from pathlib import Path

import pytest

from conftest import gate, gate_node

numpy = pytest.importorskip("numpy") if False else None  # numpy gated below
from conftest import gate_import  # noqa: E402

gate_import("numpy")
import numpy as np  # noqa: E402

from pythscribe.build import build_kernel, find_pyths  # noqa: E402
from pythscribe.runtime import array_buffer as ab  # noqa: E402

_RUNNER = Path(__file__).with_name("_array_node_runner.mjs")

# The SAME kernels the server e2e exercises (isomorphic story): `out` is a
# caller-allocated out-buffer filled in place; `total_i64` is a scalar reduction.
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
"""

_CTOR = {
    "int32": "Int32Array",
    "int64": "BigInt64Array",
    "float32": "Float32Array",
    "float64": "Float64Array",
    "uint8": "Uint8Array",
}


@pytest.fixture(scope="module")
def artifact(tmp_path_factory):
    gate_node()
    d = tmp_path_factory.mktemp("m2a3b")
    src = d / "k.ps"
    src.write_text(KERNEL_SRC)
    info = build_kernel(src, "k", KERNEL_SRC, pyths=find_pyths(), force=True, quiet=True)
    # Every kernel must have been WASM-admitted (the glue exports it, not just a
    # JS twin): the emitted glue defines the array marshallers.
    glue = (info.dir / "k.glue.js").read_text(encoding="utf-8")
    assert "function __array_to_wasm(" in glue, "array admission did not flip ON (no glue marshaller)"
    return info


def _to_arg(x):
    if isinstance(x, np.ndarray):
        dt = x.dtype
        t = {np.int32: "int32", np.int64: "int64", np.float32: "float32",
             np.float64: "float64", np.uint8: "uint8"}[dt.type]
        if t == "int64":
            return {"t": "BigInt64Array", "d": [str(int(v)) for v in x.tolist()]}
        return {"t": _CTOR[t], "d": x.tolist()}
    if isinstance(x, (int,)) and not isinstance(x, bool):
        return {"s": str(x)}  # an `int` param crosses as BigInt
    return {"s": x}


def run(entry: Path, fn: str, args, readback):
    spec = {"args": [_to_arg(a) for a in args], "readback": list(readback)}
    node = shutil.which("node")
    r = subprocess.run([node, str(_RUNNER), str(entry), fn], input=json.dumps(spec),
                       capture_output=True, text=True)
    assert r.returncode == 0, f"node runner failed: {r.stderr[:1500]}"
    return json.loads(r.stdout)


def _readback_int(out, dtype):
    """Reconstruct a NumPy array of the given dtype from the runner's readback
    (BigInt elements arrive as decimal strings)."""
    if dtype in ("int64",):
        return np.array([int(v) for v in out], dtype=np.int64)
    return np.array(out, dtype=getattr(np, dtype))


# ---- N1: per-dtype fill/transform == NumPy (glue path) ------------------------------------

def test_n1_add1_int32(artifact):
    a = np.array([0, 1, -1, 5, 100, -100, 2 ** 20, -(2 ** 20)], dtype=np.int32)
    out = np.zeros_like(a)
    r = run(artifact.entry, "add1_i32", [a, out], readback=[1])
    assert r["ok"], r
    got = _readback_int(r["readback"][0], "int32")
    np.testing.assert_array_equal(got, a + np.int32(1))


def test_n1_scale_int32(artifact):
    a = np.array([0, 1, -3, 7, 11, -50], dtype=np.int32)
    out = np.zeros_like(a)
    r = run(artifact.entry, "scale_i32", [a, out, 3], readback=[1])
    got = _readback_int(r["readback"][0], "int32")
    np.testing.assert_array_equal(got, a * np.int32(3))


def test_n1_scale_float64(artifact):
    a = np.array([0.0, 1.5, -2.25, 3.125, 1e10, -7.0], dtype=np.float64)
    out = np.zeros_like(a)
    r = run(artifact.entry, "scale_f64", [a, out, 2.5], readback=[1])
    got = np.array(r["readback"][0], dtype=np.float64)
    np.testing.assert_array_equal(got, a * 2.5)  # f64 single-op elementwise is IEEE-exact


def test_n1_add1_int64(artifact):
    a = np.array([0, 1, -1, 5, 2 ** 40, -(2 ** 40)], dtype=np.int64)
    out = np.zeros_like(a)
    r = run(artifact.entry, "add1_i64", [a, out], readback=[1])
    got = _readback_int(r["readback"][0], "int64")
    np.testing.assert_array_equal(got, a + np.int64(1))


@pytest.mark.parametrize("n", [1, 2, 7, 33, 256, 1000])
def test_n1_pbt_int32_add1(artifact, n):
    rng = np.random.default_rng(n + 7)
    a = rng.integers(-(2 ** 30), 2 ** 30, size=n, dtype=np.int32)
    out = np.zeros_like(a)
    r = run(artifact.entry, "add1_i32", [a, out], readback=[1])
    got = _readback_int(r["readback"][0], "int32")
    np.testing.assert_array_equal(got, a + np.int32(1))


# ---- N2: MANDATORY wrap-boundary witnesses (== NumPy WRAP on the glue path) ----------------

def test_n2_wrap_boundary_int32(artifact):
    a = np.array([2 ** 31 - 1, 2 ** 31 - 2, -(2 ** 31)], dtype=np.int32)
    out = np.zeros_like(a)
    r = run(artifact.entry, "add1_i32", [a, out], readback=[1])
    got = _readback_int(r["readback"][0], "int32")
    assert got[0] == -(2 ** 31), f"int32 2**31-1 +1 must WRAP to -2**31, got {got[0]}"
    np.testing.assert_array_equal(got, a + np.int32(1))


def test_n2_wrap_boundary_int32_mul(artifact):
    a = np.array([2 ** 31 - 1], dtype=np.int32)
    out = np.zeros_like(a)
    r = run(artifact.entry, "scale_i32", [a, out, 2], readback=[1])
    got = _readback_int(r["readback"][0], "int32")
    with np.errstate(over="ignore"):
        ref = np.int32(2 ** 31 - 1) * np.int32(2)
    assert got[0] == ref == -2, f"got {got[0]}"


def test_n2_wrap_boundary_int64(artifact):
    a = np.array([2 ** 63 - 1, 0, -5], dtype=np.int64)
    out = np.zeros_like(a)
    r = run(artifact.entry, "add1_i64", [a, out], readback=[1])
    got = _readback_int(r["readback"][0], "int64")
    assert got[0] == -(2 ** 63), f"int64 2**63-1 +1 must WRAP to -2**63, got {got[0]}"
    np.testing.assert_array_equal(got, a + np.int64(1))


# ---- N6: uint8 wrap AND the WASM-path marker (anti dual-track-masking) ---------------------

def test_n6_wrap_boundary_uint8_is_wasm_path(artifact):
    # The JS twin CANNOT fill a Uint8Array out-param — the runtime treats it as
    # immutable `bytes` and throws on item assignment. So a SUCCESSFUL uint8
    # write-back with the correct mod-256 wrap PROVES the WASM path ran (not the
    # twin): the dual-track-masking marker for this chunk.
    a = np.array([255, 254, 0, 200], dtype=np.uint8)
    out = np.zeros_like(a)
    r = run(artifact.entry, "add1_u8", [a, out], readback=[1])
    assert r["ok"], r
    got = np.array(r["readback"][0], dtype=np.uint8)
    assert got[0] == 0, f"uint8 255+1 must WRAP to 0 on the WASM path, got {got[0]}"
    np.testing.assert_array_equal(got, a + np.uint8(1))

    # PIN THE MARKER PREMISE: force the twin path (a wrong-dtype buffer fails the
    # total check -> RangeError -> reroute to __jsfb.add1_u8), and confirm the
    # twin THROWS on the Uint8Array out-param (the runtime treats it as immutable
    # `bytes`). This verifies the marker's discriminating power: because the twin
    # cannot fill a uint8 out-buffer, the happy-path success above could ONLY have
    # come from the WASM path, not a silent twin fallback.
    wrong = np.array([1, 2, 3, 4], dtype=np.int32)  # wrong dtype -> refused -> twin
    twin = run(artifact.entry, "add1_u8", [wrong, np.zeros(4, dtype=np.uint8)], readback=[1])
    assert not twin.get("ok"), f"the uint8 twin must THROW on a Uint8Array out-param: {twin}"
    assert "item assignment" in twin["error"] or "bytes" in twin["error"], (
        f"the twin failure must be the immutable-bytes throw (marker premise): {twin['error']}"
    )


# ---- f32 tolerance witness (option (b): f64 compute, narrow on store) ----------------------

def test_f32_multiop_within_tolerance(artifact):
    rng = np.random.default_rng(11)
    a = rng.standard_normal(64).astype(np.float32)
    b = rng.standard_normal(64).astype(np.float32)
    out = np.zeros_like(a)
    k = 1.25
    r = run(artifact.entry, "madd_f32", [a, b, out, k], readback=[2])
    got = np.array(r["readback"][0], dtype=np.float32)
    ref = (a * np.float32(k) + b)
    np.testing.assert_allclose(got, ref, rtol=1e-6, atol=1e-6)


# ---- scalar return over an array param ----------------------------------------------------

def test_scalar_return_sum_int64(artifact):
    a = np.array([1, 2, 3, 4, 5, -10], dtype=np.int64)
    r = run(artifact.entry, "total_i64", [a], readback=[])
    assert int(r["ret"]) == int(a.sum())


# ---- N3: layout byte-agreement — the glue header == array_buffer.py constants --------------

def test_n3_layout_byte_agreement_with_server_channel(artifact):
    # The emitted glue's header offsets/tags are byte-identical to the SERVER
    # channel `pythscribe/runtime/array_buffer.py` (the single source): a layout
    # change on EITHER side goes RED here.
    glue = (artifact.dir / "k.glue.js").read_text(encoding="utf-8")
    assert ab.ELEMENT_REGION_OFFSET == 16 and ab.ARRAY_HEADER_BYTES == 16
    # elements @ ELEMENT_REGION_OFFSET, shape0 @8 (== emit.rs ARRAY_ELEM_OFFSET / server header)
    assert f"ptr + {ab.ELEMENT_REGION_OFFSET}, n).set(arr)" in glue, "elems offset drift"
    assert "view.setInt32(ptr + 8, nrows, true)" in glue, "shape0 (rows) must be @8 (server header)"
    assert "view.setInt32(ptr + 12, ncols, true)" in glue, "shape1 (cols) must be @12 (M2b server header)"
    # dtype tag codes == array_buffer.DTYPE_TAG (single source of the wire codes)
    for dtype, tag in ab.DTYPE_TAG.items():
        m = re.search(rf"{dtype}:\s*\{{[^}}]*tag:\s*(\d+)", glue)
        assert m and int(m.group(1)) == tag, f"glue tag for {dtype} != array_buffer.DTYPE_TAG {tag}"
    assert ab.ARRAY_LAYOUT_VERSION == "pyths-0.2.5-array-v2"  # M2b consistency (pad@12 → shape1)


# ---- N4: RED mutant — drop the write-back => out-buffer unchanged --------------------------

def test_n4_no_writeback_mutant_leaves_out_unchanged(artifact, tmp_path):
    # Copy the artifact, delete the `__array_write_back(out, ...)` call, and show
    # the caller's out-buffer stays zeros — the write-back is load-bearing (the
    # positive is every N1 test, which reads the result back).
    mut = tmp_path / "mut"
    shutil.copytree(artifact.dir, mut)
    glue_p = mut / "k.glue.js"
    glue = glue_p.read_text(encoding="utf-8")
    mutated = re.sub(r"^\s*__array_write_back\(out, __wb_arg_\d+, .*?\);\n", "", glue, flags=re.M)
    assert mutated != glue, "the write-back call must be present to remove"
    glue_p.write_text(mutated, encoding="utf-8")
    a = np.array([10, 20, 30], dtype=np.int32)
    out = np.zeros_like(a)
    r = run(mut / "k.js", "add1_i32", [a, out], readback=[1])
    got = _readback_int(r["readback"][0], "int32")
    np.testing.assert_array_equal(got, np.zeros_like(a))  # unchanged => write-back was load-bearing


# ---- N5: RED mutant — remove the total dtype check => silent misread -----------------------

def test_n5_dtype_mismatch_reroutes_not_silent_misread(artifact, tmp_path):
    # Positive: a wrong-dtype buffer (f32 to an Array[int32] kernel) is REFUSED by
    # the total check (RangeError) and REROUTED to the JS twin, which computes the
    # pure-JS FLOAT value (NOT a NumPy-exact guarantee on the mismatch arm, but
    # never a silent misread). The paired RED control: strip the total check, and
    # the f32 values are coerced to int32 (ToInt32 TRUNCATES) before the kernel —
    # a DIFFERENT, silently-wrong value. NON-integer inputs discriminate the two:
    # the twin keeps the fraction, the mutant truncates it.
    a = np.array([1.5, 2.5, 3.5], dtype=np.float32)
    out = np.zeros(3, dtype=np.float32)  # f32 out too (the twin writes floats)
    real = run(artifact.entry, "add1_i32", [a, out], readback=[1])
    assert real["ok"], real
    real_out = np.array(real["readback"][0], dtype=np.float32)
    np.testing.assert_array_equal(real_out, np.array([2.5, 3.5, 4.5], np.float32))  # twin (float) value

    # Mutant: delete the total runtime check (constructor/width guard).
    mut = tmp_path / "mut5"
    shutil.copytree(artifact.dir, mut)
    glue_p = mut / "k.glue.js"
    glue = glue_p.read_text(encoding="utf-8")
    # M2b: the 1-D total check is the `if (!__arrRowOk(arr, d)) { … throw … }`
    # block inside the `ndim === 1` branch; strip it so a wrong-dtype buffer
    # flows into the bulk `.set()` (a silent ToInt32 truncation).
    mutated = re.sub(
        r"if \(!__arrRowOk\(arr, d\)\) \{\n(?:.*\n)*?    \}\n",
        "",
        glue,
        count=1,
    )
    assert mutated != glue, "the total check block must be present to remove"
    glue_p.write_text(mutated, encoding="utf-8")
    a2 = np.array([1.5, 2.5, 3.5], dtype=np.float32)
    out2 = np.zeros(3, dtype=np.float32)
    bad = run(mut / "k.js", "add1_i32", [a2, out2], readback=[1])
    # Without the check the f32 values are coerced to int32 (1.5->1, ...) before
    # the kernel: a SILENT misread — the call SUCCEEDS (no throw; .set() converts)
    # and returns a WRONG value, NOT the twin's [2.5,3.5,4.5]. Asserting
    # succeeds-AND-differs (not merely "differs-or-throws") pins that the check
    # prevents a *silent* corruption, not just a crash.
    assert bad.get("ok"), f"the mutant must NOT throw (a silent misread, not a crash): {bad}"
    bad_out = np.array(bad["readback"][0], dtype=np.float32)
    assert not np.array_equal(bad_out, real_out), (
        f"removing the total check must SILENTLY change the result (misread) — got {bad_out.tolist()} "
        f"vs correct rerouted {real_out.tolist()}; the check is load-bearing"
    )
    # concretely: 1.5 -> 1 (int32 truncation) then +1 -> 2 (vs the twin's 2.5).
    np.testing.assert_array_equal(bad_out, np.array([2.0, 3.0, 4.0], np.float32))
