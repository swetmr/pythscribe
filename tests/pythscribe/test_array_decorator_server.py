"""M2c regression guards for the `@wasm` DECORATOR server path with `Array[dtype, ndim]` params
(the path opened by admitting arrays into the FFI grammar). Two blockers Fable r1 found:

  B1  a mutated array out-param ALIASED inside the kernel (`o = out; o[i] = ...`) must be REFUSED
      from the server fast path (kept on the Python/browser path), never bound-and-run with a
      SILENT lost write-back. Paired control: the clean (non-aliased) kernel is NOT refused.
  B2  a DIRECT call of an admitted array kernel with buffer-protocol args (np.ndarray) must run
      the server path and write the out-buffer back IN PLACE == the reference — not raise
      "must be a list" (the pre-fix loud regression).
"""
from __future__ import annotations

import ast
import subprocess
import sys
import textwrap
from pathlib import Path

import pytest

from conftest import REPO, gate_import
from pythscribe.runtime import mutated_list_params, unsupported_list_use

numpy = gate_import("numpy")
import numpy as np  # noqa: E402


def _fn_node(src: str) -> ast.FunctionDef:
    for n in ast.walk(ast.parse(textwrap.dedent(src))):
        if isinstance(n, ast.FunctionDef):
            return n
    raise AssertionError("no function")


# --------------------------------------------------------------------- B1: the alias guard (unit)
def test_b1_aliased_array_out_param_is_refused():
    # alias of an Array out-param -> the write-back through `o` could not be seen: REFUSED
    prob = unsupported_list_use(_fn_node("""
        def k(a: Array[int32], out: Array[int32]) -> None:
            o = out
            for i in range(len(a)):
                o[i] = a[i] + 1
    """))
    assert prob is not None, "an aliased Array out-param must be refused from the server path (B1)"
    assert "out" in prob


def test_b1_rebound_array_param_is_refused():
    prob = unsupported_list_use(_fn_node("""
        def k(a: Array[int32], out: Array[int32]) -> None:
            out = a
            out[0] = 1
    """))
    # refused (whether reported as `out` rebound or `a` flowing into the RHS): the point is the
    # server fast path does NOT bind this kernel -> no silent lost write-back.
    assert prob is not None


def test_b1_clean_2d_array_kernel_is_not_refused():
    # the committed demo shape: direct out[oy][o] = img[iy][ix], no alias -> NOT refused
    node = _fn_node("""
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
    """)
    assert unsupported_list_use(node) is None, "a clean 2-D array kernel must NOT be refused"
    assert "out" in mutated_list_params(node), "the array out-param must be detected as mutated (read_back)"
    assert "img" not in mutated_list_params(node)


# --------------------------------------------------------------------- B2: direct-call server path
@pytest.fixture(scope="module")
def demo_kernels(tmp_path_factory):
    gate_import("wasmtime")
    from pythscribe.runtime import wasmtime_available
    if not wasmtime_available():
        pytest.skip("wasmtime-py required")
    # ensure the committed demo artifact exists
    wasm = REPO / "examples" / "gradio-image-preprocess" / "__pythscribe__" / "downscale_nn" / "downscale_nn.wasm"
    if not wasm.is_file():
        subprocess.run([sys.executable, "-m", "pythscribe.build", "examples/gradio-image-preprocess/kernels.py"],
                       cwd=str(REPO), check=True, capture_output=True)
    sys.path.insert(0, str(REPO / "examples" / "gradio-image-preprocess"))
    import kernels  # noqa
    return kernels


def _nn_ref(img, scale, oh, ow):
    H, W3 = img.shape
    hwc = img.reshape(H, W3 // 3, 3)
    return hwc[: oh * scale : scale][:oh, : ow * scale : scale][:, :ow, :].reshape(oh, ow * 3).astype(np.uint8)


def test_b2_direct_call_array_kernel_writes_back_numpy(demo_kernels):
    from pythscribe import binding_of

    b = binding_of(demo_kernels.downscale_nn)
    # S7 (Fable r2): the fixture guarantees wasmtime + the artifact, so downscale_nn MUST bind the
    # server path; a regression that drops it off (e.g. a future over-broad refusal) must FAIL here,
    # not silently SKIP -- this is the B2 regression guard.
    assert b.mode == "server", f"downscale_nn must be on the server path (got mode={b.mode}: {b.mode_reason})"
    rng = np.random.default_rng(99)
    H, W, scale = 8, 6, 2
    img = rng.integers(0, 256, size=(H, W * 3), dtype=np.uint8)
    oh, ow = H // scale, W // scale
    out = np.zeros((oh, ow * 3), dtype=np.uint8)

    before = b.server_runs()
    ret = demo_kernels.downscale_nn(img, scale, oh, ow, out)  # DIRECT call -> server path (B2)
    assert b.server_runs() == before + 1, "the WASM/server path must have run (not the Python fallback)"
    assert ret == oh * ow
    np.testing.assert_array_equal(out, _nn_ref(img, scale, oh, ow),
                                  err_msg="the out-buffer must be written back in place == reference (B2)")


def test_b2_run_server_explicit_matches_reference(demo_kernels):
    from pythscribe import binding_of

    b = binding_of(demo_kernels.downscale_nn)
    assert b.mode == "server", f"downscale_nn must be on the server path (got mode={b.mode}: {b.mode_reason})"
    rng = np.random.default_rng(5)
    H, W, scale = 9, 7, 3
    img = rng.integers(0, 256, size=(H, W * 3), dtype=np.uint8)
    oh, ow = H // scale, W // scale
    out = np.zeros((oh, ow * 3), dtype=np.uint8)
    ret = b.run_server(img, scale, oh, ow, out)
    assert ret == oh * ow
    np.testing.assert_array_equal(out, _nn_ref(img, scale, oh, ow))
