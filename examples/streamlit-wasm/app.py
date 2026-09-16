"""v0.2.5 M3 demo: ONE `@wasm` kernel running IN THE BROWSER inside a Streamlit custom
component, with the Python fallback when no artifact is built.

    pip install -e .[streamlit]
    python -m pythscribe.build examples/streamlit-wasm/kernels.py     # explicit build (optional)
    streamlit run examples/streamlit-wasm/app.py

Without the build step the app still works: `rms_gain` runs in Python (path=python-fallback).

THE SEAM (requirements §3): the artifact `.wasm` is base64-encoded into the component args and
instantiated in the iframe through the SAME FFI shim the Gradio component uses -- the SAME
compiled kernel, the SAME bit-identical result. The Streamlit rerun model is handled by gating
`dispatch` behind the Run button and storing the payload in `st.session_state` (a rerun reuses
the same nonce instead of minting a new one).
"""
from __future__ import annotations

import os
import re
import sys
import time
from pathlib import Path

import streamlit as st

sys.path.insert(0, str(Path(__file__).resolve().parent))
from kernels import rms_gain  # noqa: E402

from pythscribe import binding_of  # noqa: E402
from pythscribe.streamlit import (  # noqa: E402
    WasmComponent,
    deadline_result,
    describe,
    dispatch,
    result_of,
)

BINDING = binding_of(rms_gain)
# The load-bearing fallback deadline: if the browser never answers (component never mounts, JS
# off, .mjs MIME misconfig) the app falls back to Python after this many seconds (SF1).
BROWSER_DEADLINE_S = float(os.environ.get("PYTHSCRIBE_BROWSER_DEADLINE_S", "20"))


def parse_xs(text: str) -> list[float]:
    return [float(t) for t in re.split(r"[,\s]+", (text or "").strip()) if t]


st.title("pythscribe @wasm -- Streamlit (M3)")
st.markdown(
    f"`rms_gain` -- a `@wasm` kernel, compiled by `pyths`, running **in this browser tab**. "
    f"artifact: **{BINDING.artifact_status}**"
    + (f" (`{BINDING.artifact.wasm.name}`, {BINDING.artifact.wasm.stat().st_size} B)" if BINDING.artifact else " (Python fallback)")
)

xs_text = st.text_input("xs (comma-separated floats)", value="1, 2, 3, 4", key="xs")
target = st.number_input("target RMS", value=0.5, key="target")

if st.button("Run rms_gain", key="run"):
    try:
        payload = dispatch(rms_gain, parse_xs(xs_text), float(target))
        # Fault-injection hook for the E2E in-browser-fallback control (SF2): corrupt the bundle
        # so the iframe's instantiate throws -> it publishes an error -> the app runs the Python
        # fallback. Off unless the test sets the env var.
        if os.environ.get("PYTHSCRIBE_M3_CORRUPT_WASM") == "1" and payload.get("wasm_b64"):
            payload["wasm_b64"] = "!!!not-a-wasm!!!"
        st.session_state["payload"] = payload
    except (ValueError, TypeError) as e:
        st.session_state["payload"] = None
        st.error(f"input cannot cross the FFI boundary: {e}")

payload = st.session_state.get("payload")

# Browser path: render the component (returns None until its setComponentValue triggers a
# rerun) and read its value back. `expect_nonce` guards against a KEYED component handing back a
# PREVIOUS dispatch's value for new inputs (B1). Fallback path: the result is inline in the
# payload. result_of trusts only server-side state either way.
r = None
waiting = False
if payload is not None:
    if payload.get("wasm_b64"):
        comp = WasmComponent()
        value = comp(payload=payload, key="wasm", default=None)
        r = result_of(rms_gain, value, expect_nonce=payload["nonce"])
        if r is None:  # still computing (or a stale retained value) -> maybe the deadline fired
            r = deadline_result(rms_gain, payload, wait_s=BROWSER_DEADLINE_S)
            waiting = r is None
    else:
        r = result_of(rms_gain, payload, expect_nonce=payload["nonce"])  # inline Python fallback

st.code(describe(r), language=None)

# SF1: while the browser has not answered and the deadline has not passed, re-run on a short
# timer so a tab that NEVER mounts the component still falls back to Python once the deadline
# elapses (a browser that mounts-then-fails self-heals: the iframe publishes an error, which
# triggers a rerun where result_of runs the fallback). The component's setComponentValue also
# triggers a rerun, so the happy path resolves before this fires.
if waiting:
    time.sleep(0.5)
    st.rerun()
