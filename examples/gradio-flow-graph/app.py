"""M0 flagship demo (v0.2.5 callback path) -- a live dataflow graph whose compute node runs a
Python `@wasm` kernel SYNCHRONOUSLY in the browser tab (a ReactFlow custom node's compute), zero
server round-trips for the math.

The "Server round-trip" control is the PAIRED NEGATIVE CONTROL for the marker (B4): the SAME kernel
wired through Python moves the server-side counter, proving the in-tab path's `python_calls==0` is
not vacuous.
"""
from __future__ import annotations

import gradio as gr

from pythscribe.gradio import FlowGraph, WasmFunction, client_callback
from kernels import gain

# server-side per-kernel counter (the honest marker source; the in-tab path never moves it)
SERVER_CALLS = {"gain": 0}


def gain_server(x: float, k: float) -> float:
    SERVER_CALLS["gain"] += 1
    return gain(x, k)  # runs the kernel ON THE SERVER (the paired control path)


def read_count() -> float:
    return float(SERVER_CALLS["gain"])


with gr.Blocks() as demo:
    gr.Markdown("# pythscribe FlowGraph (M0) - Python @wasm as a ReactFlow node's synchronous compute")
    spec = client_callback(gain, shape="node")
    FlowGraph(
        nodes=[
            {"id": "x", "type": "source", "value": 2.0, "dtype": "float", "label": "x"},
            {"id": "k", "type": "source", "value": 3.0, "dtype": "float", "label": "k"},
            {"id": "g", "type": "compute", "kernel": spec, "label": "gain(x, k)"},
        ],
        edges=[("x", "g"), ("k", "g")],
        label="dataflow - computed in the tab",
        elem_id="flowgraph",
    )
    # island-absent CONTROL (S9): a scalar (kind-less) WasmFunction payload dispatches to the NON-island
    # branch of the SAME Index.svelte -> it must render NO `.react-flow__node` (the RED case for the
    # island-mount assertion is the existing scalar path, not a build/env bypass flag).
    WasmFunction(
        value={"fn": "f", "args": [], "bundle": None, "source_sha256": None, "nonce": 1, "result": None},
        label="scalar WasmFunction (no island)",
        elem_id="scalar-wf",
    )
    # marker read-out + the paired server-round-trip control
    with gr.Row():
        server_count = gr.Number(label="server gain() calls", elem_id="server-count", value=0)
        gr.Button("read server count", elem_id="read-count").click(read_count, None, server_count)
    with gr.Row():
        server_result = gr.Number(label="server gain(2,3)", elem_id="server-result")
        gr.Button("server round-trip gain(2,3)", elem_id="server-run").click(
            lambda: gain_server(2.0, 3.0), None, server_result
        )

if __name__ == "__main__":
    demo.launch()
