"""v0.2.5 M1.5 -- the ISOMORPHIC demo: ONE `@wasm` function (`dtw_distance`) run in the
browser tab (the M1 Gradio component) AND in-process on the server (wasmtime), the two
IEEE-754 bit patterns shown side by side with the CPython body's.

    pip install pythscribe[gradio,server]
    python -m pythscribe.build examples/wasm-use-cases/kernels.py     # explicit build
    python examples/wasm-use-cases/app.py

Result line (the E2E driver parses it):
    path=browser-wasm python_calls=0 server_calls=0 bits=<browser> server_bits=<in-process> python_bits=<cpython> identical=True ...
`python_calls` / `server_calls` are the SERVER-SIDE counters since dispatch (the server did
no work for the browser's answer); the server's own run happens BEFORE dispatch so it is not
mistaken for a fallback.
"""
from __future__ import annotations

import os
import re
import sys
from pathlib import Path

import gradio as gr

sys.path.insert(0, str(Path(__file__).resolve().parent))
from kernels import dtw_distance  # noqa: E402

from pythscribe import binding_of  # noqa: E402
from pythscribe.build.runner import float_bits  # noqa: E402
from pythscribe.gradio import WasmFunction, bundle_url, deadline_result, describe, dispatch, result_of  # noqa: E402

BINDING = binding_of(dtw_distance)
BUNDLE_URL = bundle_url(dtw_distance)
BROWSER_DEADLINE_S = float(os.environ.get("PYTHSCRIBE_BROWSER_DEADLINE_S", "20"))
_server_bits: dict[int, tuple[str | None, str]] = {}


def parse_series(text: str) -> list[float]:
    return [float(t) for t in re.split(r"[,\s]+", (text or "").strip()) if t]


def line(r: dict | None, nonce) -> str:
    if r is None:
        return "running in browser..."
    sb, pb = _server_bits.get(nonce, (None, None))
    bits = r.get("bits")
    # FAIL CLOSED (opus m1.5 r1/B3): `identical` means all THREE arms agree; a missing server arm
    # (mode != server) is reported as False, never as a two-way agreement in disguise
    ident = bits is not None and sb is not None and pb is not None and bits == sb == pb
    return describe(r) + f" server_bits={sb} python_bits={pb} identical={ident} mode={BINDING.mode}"


def run(a_text: str, b_text: str) -> tuple[dict, str]:
    a, b = parse_series(a_text), parse_series(b_text)
    m = len(b)
    # the server's own two runs FIRST (in-process WASM when mode == server; the Python body always)
    sb = float_bits(BINDING.run_server(a, b, [0.0] * (m + 1), [0.0] * (m + 1))) if BINDING.mode == "server" else None
    pb = float_bits(BINDING.run_python(a, b, [0.0] * (m + 1), [0.0] * (m + 1)))
    try:
        payload = dispatch(dtw_distance, a, b, [0.0] * (m + 1), [0.0] * (m + 1))
    except (ValueError, TypeError) as e:
        raise gr.Error(f"input cannot cross the FFI boundary: {e}") from e
    _server_bits[payload["nonce"]] = (sb, pb)
    while len(_server_bits) > 1024:  # bounded like the adapter's own dispatch history
        _server_bits.pop(next(iter(_server_bits)))
    return payload, line(result_of(dtw_distance, payload), payload["nonce"])


def deadline(payload: dict | None):
    r = deadline_result(dtw_distance, payload, wait_s=BROWSER_DEADLINE_S)
    return gr.skip() if r is None else line(r, payload.get("nonce") if payload else None)


def show(payload: dict | None) -> str:
    return line(result_of(dtw_distance, payload), payload.get("nonce") if payload else None)


with gr.Blocks(title="pythscribe @wasm -- isomorphic (M1.5)") as demo:
    gr.Markdown(
        "## `dtw_distance` — one `@wasm` function, **in this tab** and **in-process on the server**, bit for bit\n"
        f"artifact: **{BINDING.artifact_status}**, mode: **{BINDING.mode}** ({BINDING.mode_reason}) "
        + (f"(`{BINDING.artifact.wasm.name}`, {BINDING.artifact.wasm.stat().st_size} B, served at `{BUNDLE_URL}`)" if BINDING.artifact else "(Python fallback)")
    )
    with gr.Row():
        a_in = gr.Textbox(value="0, 1, 2, 3, 2, 1, 0", label="series a", elem_id="xs")
        b_in = gr.Textbox(value="0, 0, 1, 2, 3, 3, 2, 1, 0, 0", label="series b", elem_id="ys")
    btn = gr.Button("Run dtw_distance", elem_id="run")
    comp = WasmFunction(label="dtw_distance (@wasm)", elem_id="wasm")
    out = gr.Textbox(label="result", elem_id="result", interactive=False)

    btn.click(run, [a_in, b_in], [comp, out]).then(deadline, [comp], [out])
    comp.change(show, [comp], [out])

if __name__ == "__main__":
    demo.launch(
        server_name=os.environ.get("GRADIO_SERVER_NAME", "127.0.0.1"),
        server_port=int(os.environ.get("GRADIO_SERVER_PORT", "7860")),
        show_error=True,
        quiet=True,
    )
