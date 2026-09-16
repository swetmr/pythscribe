"""M2c HEADLINE control: ISOMORPHIC typed-array kernels — the SAME compiled `.wasm` run on
the SERVER (wasmtime, `array_buffer.py` marshalling) AND in the BROWSER shim
(`pythscribe/ffi/list_buffer.mjs` typed-array path — the exact marshaller the Gradio component
imports, driven under Node), asserted BIT-FOR-BIT server == browser == CPython/NumPy.

uint8/int are exact (no tolerance); f64 elementwise-single-op is IEEE-exact. The flagship is the
uint8 2-D NEAREST-NEIGHBOR downscale (Option A: pure `*`/`+` indexing, no floor-div/bitwise, so
it admits on sub-64-bit uint8 arrays — box-average on uint8 needs `//` and is NOT yet admitted,
tracked for M6/#493, see README).

Anti-vacuity (feedback_anti_vacuity_paired_control): every positive ships a paired NEGATIVE
CONTROL that goes RED:
  I-MUT  a MUTATED shim copy (wrong 2-D row read — broadcasts row 0 into every row, a
         transpose-class marshalling bug, in-bounds so it does NOT trap) makes the
         server==browser bit-for-bit assertion FAIL SILENTLY (the isomorphic control is
         load-bearing, not vacuously green).
  I-DTYPE the REAL shim REFUSES a wrong-dtype-view buffer loudly (no JS twin on this path —
         never a silent wrong-width read).
"""
from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path

import pytest

from conftest import gate, gate_import, gate_node
from pythscribe.ffi import SHIM

numpy = gate_import("numpy")
import numpy as np  # noqa: E402

wasmtime = gate_import("wasmtime")

from pythscribe.build import find_pyths  # noqa: E402
from pythscribe.runtime import ServerKernel, wasmtime_available  # noqa: E402

_SHIM_RUNNER = Path(__file__).with_name("_array_shim_runner.mjs")

# The isomorphic kernels. `downscale_nn` is the flagship (uint8 2-D nearest-neighbor); the rest
# exercise the per-dtype 1-D/2-D marshalling matrix. Every `out` is a caller-allocated buffer
# filled in place; bounds (oh/ow) are int PARAMS so the uint8 kernel needs no floor-div.
KERNEL_SRC = """
def downscale_nn(img: Array[uint8, 2], scale: int, oh: int, ow: int, out: Array[uint8, 2]) -> int:
    for oy in range(oh):
        iy = oy * scale
        for ox in range(ow):
            ix = ox * scale * 3
            o = ox * 3
            out[oy][o] = img[iy][ix]
            out[oy][o + 1] = img[iy][ix + 1]
            out[oy][o + 2] = img[iy][ix + 2]
    return oh * ow

def add1_i32(a: Array[int32], out: Array[int32]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + 1

def add1_u8(a: Array[uint8], out: Array[uint8]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + 1

def add1_i64(a: Array[int64], out: Array[int64]) -> None:
    for i in range(len(a)):
        out[i] = a[i] + 1

def scale_f64(a: Array[float64], out: Array[float64], k: float) -> None:
    for i in range(len(a)):
        out[i] = a[i] * k

def scale_i32_2d(a: Array[int32, 2], out: Array[int32, 2], k: int) -> None:
    for i in range(len(a)):
        for j in range(a.shape[1]):
            out[i][j] = a[i][j] * k
"""


@pytest.fixture(scope="module")
def art(tmp_path_factory):
    gate_node()
    gate(wasmtime_available(), "wasmtime-py is required for the server arm")
    d = tmp_path_factory.mktemp("m2c_iso")
    src = d / "k.ps"
    src.write_text(KERNEL_SRC)
    wasm = d / "k.wasm"
    r = subprocess.run([str(find_pyths()), "compile", str(src), "--target", "wasm", "-o", str(wasm)],
                       capture_output=True, text=True)
    assert r.returncode == 0, f"pyths compile failed: {r.stderr}\n{r.stdout}"
    assert wasm.is_file() and wasm.stat().st_size > 0, "no .wasm (array kernel not admitted?)"
    k = ServerKernel.from_wasm(wasm, name="k")
    for fn in ("downscale_nn", "add1_i32", "add1_u8", "add1_i64", "scale_f64", "scale_i32_2d"):
        assert fn in k.exports, f"{fn} not exported — array admission did not flip ON: {k.exports}"
    return {"dir": d, "wasm": wasm, "kernel": k}


# --------------------------------------------------------------------------- arm helpers
def _shim_arg(x):
    """Encode a NumPy array / scalar as the shim runner's arg descriptor."""
    _CTOR = {np.dtype("int32"): "int32", np.dtype("int64"): "int64", np.dtype("float32"): "float32",
             np.dtype("float64"): "float64", np.dtype("uint8"): "uint8"}
    if isinstance(x, np.ndarray):
        dt = _CTOR[x.dtype]
        if x.ndim == 1:
            d = [str(int(v)) for v in x.tolist()] if dt == "int64" else x.tolist()
            return {"a1": dt, "d": d}
        rows = [[str(int(v)) for v in row] for row in x.tolist()] if dt == "int64" else x.tolist()
        return {"a2": dt, "rows": rows}
    if isinstance(x, int) and not isinstance(x, bool):
        return {"s": str(x)}   # `int` param crosses as BigInt
    return {"s": x}


def shim_call(shim: Path, wasm: Path, fn, param_types, args, readback, return_type):
    spec = {"paramTypes": list(param_types), "args": [_shim_arg(a) for a in args],
            "readBack": list(readback), "returnType": return_type}
    node = shutil.which("node")
    r = subprocess.run([node, str(_SHIM_RUNNER), str(shim), str(wasm), fn],
                       input=json.dumps(spec), capture_output=True, text=True)
    assert r.returncode == 0, f"shim runner crashed: {r.stderr[:1500]}"
    return json.loads(r.stdout)


def _shim_readback_np(dumped, dtype, ndim):
    if ndim == 1:
        if dtype == "int64":
            return np.array([int(v) for v in dumped], dtype=np.int64)
        return np.array(dumped, dtype=getattr(np, dtype))
    if dtype == "int64":
        return np.array([[int(v) for v in row] for row in dumped], dtype=np.int64)
    return np.array(dumped, dtype=getattr(np, dtype))


# --------------------------------------------------------------------------- flagship: uint8 2-D NN
def _nn_reference(img: np.ndarray, scale: int, oh: int, ow: int) -> np.ndarray:
    """Independent NumPy nearest-neighbor downscale of a [H, W*3] uint8 image, row-major RGB."""
    H, W3 = img.shape
    hwc = img.reshape(H, W3 // 3, 3)
    picked = hwc[: oh * scale : scale][:oh, : ow * scale : scale][:, :ow, :]
    return picked.reshape(oh, ow * 3).astype(np.uint8)


@pytest.mark.parametrize("H,W,scale", [(8, 6, 2), (9, 7, 3), (12, 5, 4), (2, 2, 1)])
def test_iso_downscale_nn_uint8_2d(art, H, W, scale):
    rng = np.random.default_rng(H * 100 + W * 10 + scale)
    img = rng.integers(0, 256, size=(H, W * 3), dtype=np.uint8)
    oh, ow = H // scale, W // scale
    pt = ["Array[uint8, 2]", "int", "int", "int", "Array[uint8, 2]"]

    # SERVER (wasmtime)
    out_s = np.zeros((oh, ow * 3), dtype=np.uint8)
    ks = art["kernel"].call("downscale_nn", pt, [img, scale, oh, ow, out_s], read_back=[4], return_type="int")
    # BROWSER (shim under Node)
    r = shim_call(SHIM, art["wasm"], "downscale_nn", pt, [img, scale, oh, ow, np.zeros((oh, ow * 3), np.uint8)],
                  readback=[4], return_type="int")
    assert r["ok"], r
    out_b = _shim_readback_np(r["readback"][0], "uint8", 2)
    # CPython/NumPy reference
    ref = _nn_reference(img, scale, oh, ow)

    assert ks.value == oh * ow and int(r["ret"]) == oh * ow
    np.testing.assert_array_equal(out_s, ref, err_msg="server != reference")
    np.testing.assert_array_equal(out_b, ref, err_msg="browser(shim) != reference")
    np.testing.assert_array_equal(out_s, out_b, err_msg="server != browser (NOT isomorphic)")


# --------------------------------------------------------------------------- per-dtype matrix
def test_iso_add1_int32(art):
    a = np.array([0, 1, -1, 2 ** 20, -(2 ** 20), 2 ** 31 - 1], dtype=np.int32)
    pt = ["Array[int32]", "Array[int32]"]
    out_s = np.zeros_like(a)
    art["kernel"].call("add1_i32", pt, [a, out_s], read_back=[1], return_type="None")
    r = shim_call(SHIM, art["wasm"], "add1_i32", pt, [a, np.zeros_like(a)], readback=[1], return_type="None")
    out_b = _shim_readback_np(r["readback"][0], "int32", 1)
    ref = a + np.int32(1)
    np.testing.assert_array_equal(out_s, ref)
    np.testing.assert_array_equal(out_b, ref)
    np.testing.assert_array_equal(out_s, out_b)


def test_iso_add1_uint8_wrap(art):
    a = np.array([0, 1, 254, 255, 200], dtype=np.uint8)
    pt = ["Array[uint8]", "Array[uint8]"]
    out_s = np.zeros_like(a)
    art["kernel"].call("add1_u8", pt, [a, out_s], read_back=[1], return_type="None")
    r = shim_call(SHIM, art["wasm"], "add1_u8", pt, [a, np.zeros_like(a)], readback=[1], return_type="None")
    out_b = _shim_readback_np(r["readback"][0], "uint8", 1)
    ref = a + np.uint8(1)  # 255 -> 0 wrap
    assert out_s[3] == 0 and out_b[3] == 0
    np.testing.assert_array_equal(out_s, ref)
    np.testing.assert_array_equal(out_b, ref)
    np.testing.assert_array_equal(out_s, out_b)


def test_iso_add1_int64(art):
    a = np.array([0, 1, -1, 2 ** 40, 2 ** 63 - 1], dtype=np.int64)  # BigInt64Array path
    pt = ["Array[int64]", "Array[int64]"]
    out_s = np.zeros_like(a)
    art["kernel"].call("add1_i64", pt, [a, out_s], read_back=[1], return_type="None")
    r = shim_call(SHIM, art["wasm"], "add1_i64", pt, [a, np.zeros_like(a)], readback=[1], return_type="None")
    out_b = _shim_readback_np(r["readback"][0], "int64", 1)
    ref = a + np.int64(1)  # 2**63-1 -> -2**63 wrap
    assert out_s[4] == -(2 ** 63) and out_b[4] == -(2 ** 63)
    np.testing.assert_array_equal(out_s, ref)
    np.testing.assert_array_equal(out_b, ref)
    np.testing.assert_array_equal(out_s, out_b)


def test_iso_scale_float64(art):
    a = np.array([0.0, 1.5, -2.25, 3.125, 1e10, -7.0], dtype=np.float64)
    pt = ["Array[float64]", "Array[float64]", "float"]
    out_s = np.zeros_like(a)
    art["kernel"].call("scale_f64", pt, [a, out_s, 2.5], read_back=[1], return_type="None")
    r = shim_call(SHIM, art["wasm"], "scale_f64", pt, [a, np.zeros_like(a), 2.5], readback=[1], return_type="None")
    out_b = _shim_readback_np(r["readback"][0], "float64", 1)
    ref = a * 2.5  # f64 single-op elementwise is IEEE-exact
    np.testing.assert_array_equal(out_s, ref)
    np.testing.assert_array_equal(out_b, ref)  # bit-for-bit
    np.testing.assert_array_equal(out_s, out_b)


def test_iso_scale_int32_2d(art):
    a = np.array([[1, 2, 3], [4, 5, 6]], dtype=np.int32)
    pt = ["Array[int32, 2]", "Array[int32, 2]", "int"]
    out_s = np.zeros_like(a)
    art["kernel"].call("scale_i32_2d", pt, [a, out_s, 3], read_back=[1], return_type="None")
    r = shim_call(SHIM, art["wasm"], "scale_i32_2d", pt, [a, np.zeros_like(a), 3], readback=[1], return_type="None")
    out_b = _shim_readback_np(r["readback"][0], "int32", 2)
    ref = a * np.int32(3)
    np.testing.assert_array_equal(out_s, ref)
    np.testing.assert_array_equal(out_b, ref)
    np.testing.assert_array_equal(out_s, out_b)


# --------------------------------------------------------------------------- I-MUT: paired RED control
def test_imut_wrong_2d_row_read_breaks_isomorphism(art, tmp_path):
    """A MUTATED shim that marshals a WRONG row of a 2-D input (broadcasts row 0 into every
    row — a transpose-class read bug, in-bounds so it does NOT trap) makes the WASM kernel read
    the wrong source rows: the browser result SILENTLY DIVERGES from the server, so the
    server==browser bit-for-bit assertion goes RED. Proves the isomorphic control is
    load-bearing (a silent-wrong-value RED, the strongest anti-vacuity form)."""
    mut = tmp_path / "list_buffer_mut.mjs"
    txt = SHIM.read_text(encoding="utf-8")
    # unique to arrayToWasm's 2-D marshal-in: read row `r` -> read row 0 for all rows.
    needle = "ptr + ARRAY_ELEM_OFFSET + r * ncols * d.esize, ncols).set(arr[r]);"
    assert needle in txt, "the 2-D marshal-in row copy must be present to mutate"
    mut.write_text(txt.replace(needle, "ptr + ARRAY_ELEM_OFFSET + r * ncols * d.esize, ncols).set(arr[0]);"),
                   encoding="utf-8")

    # a multi-row image with DISTINCT rows so a row-0 broadcast necessarily diverges
    rng = np.random.default_rng(4242)
    img = rng.integers(0, 256, size=(8, 6 * 3), dtype=np.uint8)
    scale, oh, ow = 2, 4, 3
    pt = ["Array[uint8, 2]", "int", "int", "int", "Array[uint8, 2]"]
    out_s = np.zeros((oh, ow * 3), dtype=np.uint8)
    art["kernel"].call("downscale_nn", pt, [img, scale, oh, ow, out_s], read_back=[4], return_type="int")
    r = shim_call(mut, art["wasm"], "downscale_nn", pt, [img, scale, oh, ow, np.zeros((oh, ow * 3), np.uint8)],
                  readback=[4], return_type="int")
    assert r.get("ok"), f"the broadcast mutant is in-bounds and must NOT trap (a silent misread): {r}"
    out_b = _shim_readback_np(r["readback"][0], "uint8", 2)
    assert not np.array_equal(out_s, out_b), (
        "a wrong 2-D row read must SILENTLY break server==browser; the isomorphic assertion is vacuous otherwise"
    )


def test_idtype_wrong_dtype_view_is_refused(art):
    """The REAL shim REFUSES a wrong-dtype buffer loudly (no JS twin on this path): an int32
    view handed to an Array[uint8] kernel throws in arrayToWasm, never a silent wrong-width read.
    Pins the total runtime check (sound-by-refusal), the shim's B1 counterpart."""
    a = np.array([1, 2, 3, 4], dtype=np.int32)  # WRONG dtype for Array[uint8]
    pt = ["Array[uint8]", "Array[uint8]"]
    r = shim_call(SHIM, art["wasm"], "add1_u8", pt, [a, np.zeros(4, np.int32)], readback=[1], return_type="None")
    assert not r.get("ok"), f"a wrong-dtype buffer must be REFUSED, not marshalled: {r}"
    assert "expects a Uint8Array" in r["error"] or "dtype/width" in r["error"], r["error"]
