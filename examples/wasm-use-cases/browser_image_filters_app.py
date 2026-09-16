"""v0.2.6 fix B -- IN-TAB image filters: `@wasm` typed-array kernels run ENTIRELY in the browser
tab from a Gradio `js=` hook (no custom component, no server round-trip), the filtered image
displayed in the page. Contrast: `browser_wasm_demos.ipynb`'s image tab ran the same kernels
SERVER-side (~50 ms round-trip) because the in-tab image path did not exist.

    python -m pythscribe.build examples/wasm-use-cases/kernels.py   # threshold_lum, sobel
    python examples/wasm-use-cases/browser_image_filters_app.py

Data path (per slider move): gr.Image value (a FileData URL, fetched ONCE and cached in the tab)
-> createImageBitmap -> canvas getImageData -> H rows of Uint8Array(W*3) -> the kernel's own
.wasm through the pythscribe typed-array FFI shim (`ffi.call(..., {readBack: [out]})`) -> `out`
rows read back from WASM memory -> canvas -> PNG data URL -> the output gr.Image. The server
does NOTHING for the transform (the E2E asserts 0 /gradio_api compute requests AND a 0 delta on
the kernels' server-side counters). NOT claimed: that the input never reaches the server -- it
is a gr.Image value (a preset is served from a `file=` URL; an upload is POSTed to the server
first); the tab fetches it once and caches the decoded rows.

The "Server round-trip" tab is the PAIRED NEGATIVE CONTROL: the same kernels wired through a
Python `fn` -- driving it makes the same assertions go red (requests > 0, counters > 0). The
`/pythscribe-probe` route reports the server-side counters for the E2E.
"""
from __future__ import annotations

import os
import sys
import time
from pathlib import Path

import gradio as gr
import numpy as np
from fastapi import FastAPI
from PIL import Image

from pythscribe import binding_of
from pythscribe.gradio.browser import browser_image_loader_js

HERE = Path(__file__).resolve().parent
M1_DIR = HERE.parent / "gradio-image-preprocess"
TEST_IMAGE = M1_DIR / "test_images" / "photo_640x480.jpg"


def _load_kernels(path: Path, modname: str):
    """Load an example `kernels.py` by PATH under a distinct module name: this dir and
    examples/gradio-image-preprocess both ship a `kernels.py`, and a bare `import kernels`
    (or a sys.path insert) would let one shadow the other for every later importer."""
    import importlib.util

    spec = importlib.util.spec_from_file_location(modname, path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[modname] = mod
    spec.loader.exec_module(mod)
    return mod


_UC = _load_kernels(HERE / "kernels.py", "wasm_use_case_kernels")
_M1 = _load_kernels(M1_DIR / "kernels.py", "image_preprocess_kernels")
threshold_lum, sobel, downscale_nn = _UC.threshold_lum, _UC.sobel, _M1.downscale_nn
KERNELS = {"threshold": threshold_lum, "sobel": sobel, "downscale": downscale_nn}

# the js= handler: (image, kind, thr, scale) -> [output image, status]; runs CLIENT-SIDE
HANDLER_JS = """(img, kind, thr, scale) => {
  if (!img) return [null, 'load an image'];
  const c = window.pythscribeImage;
  if (!c || !c.ready) return [null, 'loading the in-tab @wasm client...'];
  if (kind === 'threshold') return c.filter(img, kind, { thr: Math.round(thr) * 3 });
  if (kind === 'downscale') { const s = Math.max(1, Math.round(scale));
    return c.filter(img, kind, { scale: s }, (im) => ({ oh: Math.floor(im.h / s), ow: Math.floor(im.w / s) })); }
  return c.filter(img, kind, {});
}"""


def load_rgb(path: Path = TEST_IMAGE) -> np.ndarray:
    return np.asarray(Image.open(path).convert("RGB"), dtype=np.uint8)


def server_filter(rgb: np.ndarray, kind: str, thr: float, scale: float):
    """The SERVER path (the negative control): the same kernel run in-process by Python."""
    if rgb is None:
        return None, "load an image"
    h, w = rgb.shape[:2]
    im = np.ascontiguousarray(rgb[:, :, :3]).reshape(h, w * 3)
    fn = KERNELS[kind]
    b = binding_of(fn)
    before = b.counts()
    t0 = time.perf_counter()
    if kind == "threshold":
        out = np.zeros_like(im)
        fn(im, out, h, w, int(round(thr)) * 3)
    elif kind == "downscale":
        s = max(1, int(round(scale)))
        oh, ow = h // s, w // s
        out = np.zeros((oh, ow * 3), dtype=np.uint8)
        fn(im, s, oh, ow, out)
    else:
        out = np.zeros_like(im)
        fn(im, out, h, w)
    ms = (time.perf_counter() - t0) * 1e3
    after = b.counts()
    return out.reshape(out.shape[0], out.shape[1] // 3, 3), (
        f"{kind}: {ms:.1f} ms on the SERVER (mode={b.mode}); 1 round-trip; "
        f"python_calls+={after[0] - before[0]} server_calls+={after[1] - before[1]}")


def build_demo(image: np.ndarray | None = None) -> gr.Blocks:
    rgb0 = load_rgb() if image is None else image
    load_js = browser_image_loader_js(KERNELS)  # validates + resolves every kernel's artifact HERE
    with gr.Blocks(title="pythscribe @wasm - image filters in the browser tab") as demo:
        gr.Markdown("## `@wasm` image filters **in your browser tab** — typed arrays, `js=` hook, no server round-trip")
        demo.load(None, None, None, js=load_js)
        with gr.Tabs():
            with gr.Tab("In-tab (client-side)", id="client"):
                gr.Markdown("Drag the slider: the filter runs **in the tab** over `Array[uint8, 2]` typed arrays "
                            "(the kernel's own `.wasm` via the pythscribe FFI shim) -- no server round-trip for the transform. "
                            "The input is fetched from its Gradio file URL once and cached; the server does nothing per move.")
                with gr.Row():
                    c_in = gr.Image(type="numpy", value=rgb0, format="png", height=260, label="input (decoded in the tab)", elem_id="c-in")
                    c_out = gr.Image(height=260, label="@wasm output (in-tab)", elem_id="c-out", interactive=False)
                c_kind = gr.Radio(list(KERNELS), value="threshold", label="filter", elem_id="c-kind")
                with gr.Row():
                    c_thr = gr.Slider(0, 255, value=128, step=1, label="threshold (per-channel mean)", elem_id="c-thr")
                    c_scale = gr.Slider(1, 8, value=2, step=1, label="downscale factor (nearest)", elem_id="c-scale")
                c_status = gr.Textbox(label="status", interactive=False, elem_id="c-status")
                for comp in (c_in, c_kind, c_thr, c_scale):
                    comp.change(None, [c_in, c_kind, c_thr, c_scale], [c_out, c_status], js=HANDLER_JS)
            with gr.Tab("Server round-trip (comparison / negative control)", id="server"):
                gr.Markdown("The SAME kernels wired through a Python `fn`: one round-trip per slider move, "
                            "the server's counters advance. (The E2E's negative control: this tab must FAIL the client-side assertions.)")
                with gr.Row():
                    s_in = gr.Image(type="numpy", value=rgb0, format="png", height=260, label="input", elem_id="s-in")
                    s_out = gr.Image(height=260, label="server output", elem_id="s-out", interactive=False)
                s_kind = gr.Radio(list(KERNELS), value="threshold", label="filter", elem_id="s-kind")
                with gr.Row():
                    s_thr = gr.Slider(0, 255, value=128, step=1, label="threshold (per-channel mean)", elem_id="s-thr")
                    s_scale = gr.Slider(1, 8, value=2, step=1, label="downscale factor (nearest)", elem_id="s-scale")
                s_status = gr.Textbox(label="status", interactive=False, elem_id="s-status")
                for comp in (s_in, s_kind, s_thr, s_scale):
                    comp.change(server_filter, [s_in, s_kind, s_thr, s_scale], [s_out, s_status])
    return demo


def build_app(image: np.ndarray | None = None) -> FastAPI:
    api = FastAPI()

    @api.get("/pythscribe-probe")
    def probe():
        return {name: {"python_calls": binding_of(fn).counts()[0], "server_calls": binding_of(fn).counts()[1],
                       "mode": binding_of(fn).mode, "artifact_status": binding_of(fn).artifact_status}
                for name, fn in KERNELS.items()}

    return gr.mount_gradio_app(api, build_demo(image), path="/")


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(
        build_app(),
        host=os.environ.get("GRADIO_SERVER_NAME", "127.0.0.1"),
        port=int(os.environ.get("GRADIO_SERVER_PORT", "7860")),
        log_level="warning",
    )
