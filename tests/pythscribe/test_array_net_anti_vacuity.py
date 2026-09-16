"""M2a-4b anti-vacuity gate for the array differential NET (`tests/differential/wasm_net/
array_net.py`). Every GREEN net property ships a PAIRED NEGATIVE CONTROL that provably goes
RED when the property is violated (feedback_anti_vacuity_paired_control). This test drives:

  A  clean run -> the ratchet is GREEN (0 unexpected RED) — the positive.
  B  `--mutant no-marshalling` -> the net CATCHES it (>=1 direct row RED) — skip the bulk
     copy-in and the out-buffer diverges from NumPy.
  C  `--mutant wrong-width` -> the net CATCHES it (int32 rows RED) — lay int32 elements at
     an 8-byte stride and the raw kernel misreads at the wrong width.
  D  runtime-check-removed (glue): strip the total dtype/width check from a COPY of the REAL
     emitted glue; a wrong-dtype (f32) buffer to an Array[int32] kernel is then flat-copied +
     truncated (a SILENT misread) — the call SUCCEEDS and returns a WRONG value, vs the real
     glue which REFUSES (RangeError) and reroutes to the JS twin. Proves the total check is
     load-bearing (the sound-by-refusal claim is not vacuous).
  E  wrap-boundary discrimination: the int32 wrap witness (2**31-1 + 1) equals NumPy WRAP
     (-2**31) ONLY because codegen wraps mod-2**32; an i64-widening lowering (stand-in: the
     REAL int64 kernel on the same value) yields +2**31 != the NumPy int32 wrap — so the
     witness ROW is RED for a widening/refusing compiler (not vacuously green on small values).
  F  the wasi 4th arm is a TRACKED, VISIBLE skip (never a silent pass): the net reports a
     `wasi::__arm` row with its skip reason.

The net's raw-WASM (`direct`) arm is un-masked (no JS twin), so a real compiler bug of these
shapes shows there — B/C/E prove the net is SENSITIVE to that class; D proves the runtime
refusal gate is load-bearing on the user-facing glue path.
"""
from __future__ import annotations

import re
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

from conftest import gate_import, gate_node

gate_import("numpy")
import numpy as np  # noqa: E402

from pythscribe.build import build_kernel, find_pyths  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
NET = REPO / "tests" / "differential" / "wasm_net" / "array_net.py"
RUNNER = Path(__file__).with_name("_array_node_runner.mjs")

KERNEL_SRC = """
def add1_i32(a: Array[int32], out: Array[int32]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + 1

def add1_i64(a: Array[int64], out: Array[int64]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + 1
"""


def _run_net(*args, mutant: str | None = None) -> subprocess.CompletedProcess:
    import os
    env = {**os.environ}
    cmd = [sys.executable, str(NET), *args]
    if mutant:
        cmd += ["--mutant", mutant]
    return subprocess.run(cmd, cwd=str(REPO), capture_output=True, text=True, timeout=1200)


# ---- A: clean run — the ratchet is green ---------------------------------------------------

def test_a_clean_net_ratchet_green():
    gate_node()
    r = _run_net()
    assert r.returncode == 0, f"clean net ratchet must be GREEN:\n{r.stdout[-2000:]}\n{r.stderr[-800:]}"
    assert "ratchet OK" in r.stdout, r.stdout[-1500:]


# ---- B/C: the direct-arm false-world mutants are CAUGHT (net goes RED) ----------------------

@pytest.mark.parametrize("mutant,expect_rows", [
    ("no-marshalling", ["arr_i32::add1_i32", "arr_i64::add1_i64", "arr_u8::add1_u8",
                        "arr_f64::scale_f64", "arr_f32::madd_f32"]),
    ("wrong-width", ["arr_i32::add1_i32"]),
    # M2b: the 2-D row-major offset control — a column-major (transposed) element layout
    # under a row-major header turns every NON-SQUARE 2-D value row RED (the sum row is
    # transpose-invariant, correctly unaffected). Lean twin: arrayTranspose_offsetStub_fails.
    ("transpose", ["arr2d_i32::scale_i32_2d", "arr2d_u8_img::threshold_u8_2d",
                   "arr2d_f64::neg_f64_2d", "arr2d_f32::madd_f32_2d"]),
])
def test_bc_direct_mutants_go_red(mutant, expect_rows):
    gate_node()
    r = _run_net(mutant=mutant)
    # exit 0 = the net CAUGHT the false-world (>=1 direct row RED). exit 1 = it slipped (vacuous).
    assert r.returncode == 0, f"the net must CATCH the {mutant} false-world:\n{r.stdout[-2000:]}"
    assert "0 direct row(s) caught" not in r.stdout, f"{mutant} slipped through — net is vacuous:\n{r.stdout[-1500:]}"
    for row in expect_rows:
        assert re.search(rf"RED\s+{re.escape(row)}\b", r.stdout), (
            f"{mutant} must turn {row} RED:\n{r.stdout[-2500:]}"
        )


# ---- D: runtime-check-removed (glue) => silent misread (the total check is load-bearing) ----

@pytest.fixture(scope="module")
def glue_artifact(tmp_path_factory):
    gate_node()
    d = tmp_path_factory.mktemp("m2a4b_av")
    src = d / "k.ps"
    src.write_text(KERNEL_SRC)
    info = build_kernel(src, "k", KERNEL_SRC, pyths=find_pyths(), force=True, quiet=True)
    glue = (info.dir / "k.glue.js").read_text(encoding="utf-8")
    assert "function __array_to_wasm(" in glue, "array admission did not flip ON"
    return info


def _run_glue(entry: Path, fn: str, args, readback):
    import json
    def to_arg(x):
        if isinstance(x, np.ndarray):
            t = {np.int32: "Int32Array", np.int64: "BigInt64Array", np.float32: "Float32Array",
                 np.float64: "Float64Array", np.uint8: "Uint8Array"}[x.dtype.type]
            if t == "BigInt64Array":
                return {"t": t, "d": [str(int(v)) for v in x.tolist()]}
            return {"t": t, "d": x.tolist()}
        return {"s": str(x) if isinstance(x, int) and not isinstance(x, bool) else x}
    spec = {"args": [to_arg(a) for a in args], "readback": list(readback)}
    node = shutil.which("node")
    r = subprocess.run([node, str(RUNNER), str(entry), fn], input=json.dumps(spec),
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stderr[:1200]
    return json.loads(r.stdout)


def test_d_runtime_check_removed_is_silent_misread(glue_artifact, tmp_path):
    # Positive: a wrong-dtype (f32) buffer to Array[int32] is REFUSED by the total check
    # (RangeError) and rerouted to the JS twin, which keeps the float value (never a silent
    # misread). Control: strip the total check -> f32 is coerced to int32 (truncates) -> a
    # DIFFERENT, silently-wrong value; the call SUCCEEDS (no throw).
    a = np.array([1.5, 2.5, 3.5], dtype=np.float32)
    real = _run_glue(glue_artifact.entry, "add1_i32", [a, np.zeros(3, np.float32)], readback=[1])
    assert real["ok"], real
    real_out = np.array(real["readback"][0], dtype=np.float32)
    np.testing.assert_array_equal(real_out, np.array([2.5, 3.5, 4.5], np.float32))  # twin (float) value

    mut = tmp_path / "mut"
    shutil.copytree(glue_artifact.dir, mut)
    gp = mut / "k.glue.js"
    glue = gp.read_text(encoding="utf-8")
    # M2b: the 1-D total check is the `if (!__arrRowOk(arr, d)) { … throw … }` block inside
    # the `ndim === 1` branch of __array_to_wasm; strip it so a wrong-dtype buffer reaches
    # the bulk `.set()` (a silent ToInt32 truncation).
    mutated = re.sub(r"if \(!__arrRowOk\(arr, d\)\) \{\n(?:.*\n)*?    \}\n", "", glue, count=1)
    assert mutated != glue, "the total check block must be present to remove"
    gp.write_text(mutated, encoding="utf-8")
    bad = _run_glue(mut / "k.js", "add1_i32", [np.array([1.5, 2.5, 3.5], np.float32), np.zeros(3, np.float32)], readback=[1])
    assert bad.get("ok"), f"the mutant must NOT throw (a silent misread, not a crash): {bad}"
    bad_out = np.array(bad["readback"][0], dtype=np.float32)
    assert not np.array_equal(bad_out, real_out), (
        f"removing the total check must SILENTLY change the result — got {bad_out.tolist()} "
        f"vs correct rerouted {real_out.tolist()}"
    )
    np.testing.assert_array_equal(bad_out, np.array([2.0, 3.0, 4.0], np.float32))  # 1.5->1 truncation, +1 -> 2


# ---- E: the wrap-boundary witness is discriminating (i64-widen would be RED) ----------------

def test_e_wrap_witness_discriminates(glue_artifact):
    # The int32 kernel WRAPS mod-2**32: 2**31-1 + 1 == -2**31 (== NumPy int32). An i64-widening
    # lowering would NOT wrap: the REAL int64 kernel on the same value yields +2**31, which
    # differs from the NumPy int32 wrap — so the net's wrap witness ROW would be RED for such a
    # compiler (proof the witness isn't vacuously green on small values).
    val = 2 ** 31 - 1
    i32 = _run_glue(glue_artifact.entry, "add1_i32", [np.array([val], np.int32), np.zeros(1, np.int32)], readback=[1])
    wrapped = int(i32["readback"][0][0])
    assert wrapped == -(2 ** 31) == int(np.int32(val) + np.int32(1)), f"int32 must WRAP, got {wrapped}"

    i64 = _run_glue(glue_artifact.entry, "add1_i64", [np.array([val], np.int64), np.zeros(1, np.int64)], readback=[1])
    widened = int(i64["readback"][0][0])
    assert widened == 2 ** 31, f"i64-widen stand-in must NOT wrap, got {widened}"
    assert widened != wrapped, "the wrap witness discriminates wrap from i64-widen (the RED control)"


# ---- F: the wasi 4th arm is a tracked, visible skip (never a silent pass) -------------------

def test_f_wasi_arm_tracked_skip():
    gate_node()
    r = _run_net()
    assert r.returncode == 0, r.stdout[-1500:]
    assert re.search(r"wasi::__arm\s+\{'wasi':", r.stdout), f"wasi arm row must be reported:\n{r.stdout[-1500:]}"
    assert "skipped" in r.stdout and "PYTHS_WASI_PYTHON" in r.stdout, (
        f"the wasi skip must be TRACKED + visible (reason printed):\n{r.stdout[-1500:]}"
    )
