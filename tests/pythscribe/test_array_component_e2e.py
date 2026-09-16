"""M2c: real-browser (Playwright/Chromium) E2E of the typed-array path — the uint8 2-D
nearest-neighbor downscale running IN A REAL BROWSER over first-class typed arrays, marshalled
by the SAME shim (`pythscribe/ffi/list_buffer.mjs`) the Gradio component imports. The page
`examples/gradio-image-preprocess/typed_array_nn.html` is served from the repo root; the test
drives its exposed `window.pythscribeNN(...)` and asserts the in-browser out-buffer == the
independent NumPy nearest-neighbor reference (bit-for-bit uint8), proving the typed-array WASM
path actually ran in Chromium.

Paired control: a mutated call (transposed row read on the JS side) makes the browser result
diverge from the reference — the assertion is not vacuously green. Gated: skips cleanly without
playwright + a chromium build (runs in CI where the browser stack is present)."""
from __future__ import annotations

import contextlib
import functools
import http.server
import socket
import subprocess
import threading
from pathlib import Path

import pytest

from conftest import REPO, gate_import

numpy = gate_import("numpy")
import numpy as np  # noqa: E402

from pythscribe.build import find_pyths  # noqa: E402

PAGE = "/examples/gradio-image-preprocess/typed_array_nn.html"
WASM = REPO / "examples" / "gradio-image-preprocess" / "__pythscribe__" / "downscale_nn" / "downscale_nn.wasm"


@pytest.fixture(autouse=True, scope="module")
def _need_browser():
    gate_import("playwright.sync_api")


@pytest.fixture(scope="module")
def artifact():
    # ensure the demo artifact exists (built from the committed kernels.py)
    if not WASM.is_file():
        r = subprocess.run([str(find_pyths()), "--version"], capture_output=True, text=True)
        assert r.returncode == 0, "pyths not available to build the demo artifact"
        subprocess.run(["python", "-m", "pythscribe.build", "examples/gradio-image-preprocess/kernels.py"],
                       cwd=str(REPO), check=True, capture_output=True)
    assert WASM.is_file(), "downscale_nn.wasm missing (build examples/gradio-image-preprocess/kernels.py)"
    return WASM


class _WasmHandler(http.server.SimpleHTTPRequestHandler):
    # `WebAssembly.compileStreaming(fetch(url))` (what the shim uses in the browser) REQUIRES the
    # response Content-Type to be exactly `application/wasm`; register it explicitly so the E2E does
    # not depend on the host's mimetypes DB (some platforms don't map .wasm).
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm",
                      ".mjs": "text/javascript", ".js": "text/javascript"}


@contextlib.contextmanager
def _serve(root: Path):
    handler = functools.partial(_WasmHandler, directory=str(root))
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
    httpd = http.server.ThreadingHTTPServer(("127.0.0.1", port), handler)
    t = threading.Thread(target=httpd.serve_forever, daemon=True)
    t.start()
    try:
        yield f"http://127.0.0.1:{port}"
    finally:
        httpd.shutdown()


def _nn_reference(img: np.ndarray, scale: int, oh: int, ow: int) -> np.ndarray:
    H, W3 = img.shape
    hwc = img.reshape(H, W3 // 3, 3)
    return hwc[: oh * scale : scale][:oh, : ow * scale : scale][:, :ow, :].reshape(oh, ow * 3).astype(np.uint8)


@contextlib.contextmanager
def _page(url):
    from playwright.sync_api import sync_playwright

    with sync_playwright() as p:
        b = p.chromium.launch()
        try:
            pg = b.new_context().new_page()
            errors = []
            pg.on("pageerror", lambda e: errors.append(str(e)))
            pg.goto(url + PAGE, wait_until="load")
            pg.wait_for_function("() => typeof window.pythscribeNN === 'function'", timeout=20000)
            yield pg, errors
        finally:
            b.close()


def test_e2e_typed_array_nn_matches_numpy_in_real_browser(artifact):
    rng = np.random.default_rng(2026)
    H, W, scale = 10, 8, 2
    img = rng.integers(0, 256, size=(H, W * 3), dtype=np.uint8)
    oh, ow = H // scale, W // scale
    ref = _nn_reference(img, scale, oh, ow)

    with _serve(REPO) as url, _page(url) as (pg, errors):
        res = pg.evaluate(
            """async ([rows, scale, oh, ow]) => {
                 const r = await window.pythscribeNN(rows.map(x => Uint8Array.from(x)), scale, oh, ow);
                 return { ret: r.ret, rows: r.rows, exports: r.exports, layout: r.layout };
               }""",
            [img.tolist(), scale, oh, ow],
        )
        assert not errors, f"browser page errors: {errors}"

    assert res["ret"] == oh * ow
    assert "downscale_nn" in res["exports"], f"the .wasm export must be present: {res['exports']}"
    assert res["layout"] == "pyths-0.2.5-array-v2"
    got = np.array(res["rows"], dtype=np.uint8)
    np.testing.assert_array_equal(got, ref, err_msg="real-browser typed-array output != NumPy reference")


def test_e2e_transpose_control_goes_red(artifact):
    """Paired control: reading the browser out rows transposed (a wrong 2-D read) must NOT
    equal the reference — the equality assertion above is load-bearing, not vacuous."""
    rng = np.random.default_rng(7)
    H, W, scale = 8, 8, 2  # square output so a transpose is well-defined and in-range
    img = rng.integers(0, 256, size=(H, W * 3), dtype=np.uint8)
    oh, ow = H // scale, W // scale
    ref = _nn_reference(img, scale, oh, ow)

    with _serve(REPO) as url, _page(url) as (pg, errors):
        res = pg.evaluate(
            """async ([rows, scale, oh, ow]) => {
                 const r = await window.pythscribeNN(rows.map(x => Uint8Array.from(x)), scale, oh, ow);
                 return r.rows;
               }""",
            [img.tolist(), scale, oh, ow],
        )
        assert not errors, f"browser page errors: {errors}"
    got = np.array(res, dtype=np.uint8)  # [oh, ow*3]
    # transpose the row index against a column index (a deliberately WRONG read of the output)
    hwc = got.reshape(oh, ow, 3)
    transposed = np.transpose(hwc, (1, 0, 2)).reshape(ow, oh * 3).astype(np.uint8)
    assert not np.array_equal(transposed, ref), "a transposed read must diverge from the reference (control not vacuous)"
    # and the CORRECT read still matches (both halves of the control)
    np.testing.assert_array_equal(got, ref)
