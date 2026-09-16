"""v0.2.5 M0 demo: ONE `@wasm` kernel running IN THE BROWSER inside a Gradio custom component,
with the Python fallback when no artifact is built.

    pip install -e .[gradio]  &&  pip install -e pythscribe/gradio/wasm_function
    python -m pythscribe.build examples/gradio-wasm/kernels.py     # explicit build (optional)
    python examples/gradio-wasm/app.py

Without the build step the app still works: `rms_gain` runs in Python (path=python-fallback).
"""
from __future__ import annotations

import os
import re
import sys
from pathlib import Path

import gradio as gr

sys.path.insert(0, str(Path(__file__).resolve().parent))
from kernels import rms_gain  # noqa: E402

from pythscribe import binding_of  # noqa: E402
from pythscribe.gradio import WasmFunction, bundle_url, deadline_result, describe, dispatch, result_of  # noqa: E402

BINDING = binding_of(rms_gain)
# Registers the artifact directory as a Gradio static path at construction time (None when
# there is no usable artifact -> the Python fallback will run).
BUNDLE_URL = bundle_url(rms_gain)
BROWSER_DEADLINE_S = float(os.environ.get("PYTHSCRIBE_BROWSER_DEADLINE_S", "20"))


def parse_xs(text: str) -> list[float]:
    return [float(t) for t in re.split(r"[,\s]+", (text or "").strip()) if t]


def run(xs_text: str, target: float) -> tuple[dict, str]:
    """Click handler. Browser path: the payload goes to the component (result None) and the
    textbox shows 'running in browser...' until the component's `change` delivers the
    result. Fallback path: the Python result is ALREADY in the payload, so the textbox is
    filled right here -- the fallback never depends on the component's JS having loaded."""
    try:
        payload = dispatch(rms_gain, parse_xs(xs_text), float(target))
    except (ValueError, TypeError) as e:  # non-crossable input (inf/nan, huge int, bad text)
        raise gr.Error(f"input cannot cross the FFI boundary: {e}") from e
    return payload, describe(result_of(rms_gain, payload))


def deadline(payload: dict | None):
    """Server-side deadline: if the browser never answers (component failed to mount, JS
    off, bundle threw at module evaluation) fall back to Python after BROWSER_DEADLINE_S."""
    r = deadline_result(rms_gain, payload, wait_s=BROWSER_DEADLINE_S)
    return gr.skip() if r is None else describe(r)


def show(payload: dict | None) -> str:
    """`change` handler: the component wrote its result (or a browser error -> Python fallback)."""
    return describe(result_of(rms_gain, payload))


with gr.Blocks(title="pythscribe @wasm -- M0") as demo:
    gr.Markdown(
        "## `rms_gain` — a `@wasm` kernel, compiled by `pyths`, running **in this browser tab**\n"
        f"artifact: **{BINDING.artifact_status}** "
        + (f"(`{BINDING.artifact.wasm.name}`, {BINDING.artifact.wasm.stat().st_size} B, served at `{BUNDLE_URL}`)" if BINDING.artifact else "(Python fallback)")
    )
    with gr.Row():
        xs = gr.Textbox(value="1, 2, 3, 4", label="xs (comma-separated floats)", elem_id="xs")
        target = gr.Number(value=0.5, label="target RMS", elem_id="target")
    btn = gr.Button("Run rms_gain", elem_id="run")
    comp = WasmFunction(label="rms_gain (@wasm)", elem_id="wasm")
    out = gr.Textbox(label="result", elem_id="result", interactive=False)

    btn.click(run, [xs, target], [comp, out]).then(deadline, [comp], [out])
    comp.change(show, [comp], [out])

if __name__ == "__main__":
    demo.launch(
        server_name=os.environ.get("GRADIO_SERVER_NAME", "127.0.0.1"),
        server_port=int(os.environ.get("GRADIO_SERVER_PORT", "7860")),
        show_error=True,
        quiet=True,
    )
