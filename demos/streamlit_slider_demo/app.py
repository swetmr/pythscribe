"""pythscribe @wasm on Streamlit -- a slider that recomputes in your browser, with no server rerun.

    pip install -e .[streamlit]
    python -m pythscribe.build examples/streamlit-callback/kernels.py     # explicit build
    streamlit run examples/streamlit-callback/app.py

Left slider: the compiled @wasm kernel recomputes INSIDE the component iframe on every drag -- the
browser never calls back, so Streamlit does not rerun (the sidebar counter stays put) and the
recompute takes a few milliseconds. Right slider: the SAME function in plain Python, the ordinary
Streamlit way -- every drag reruns the whole script on the server (the counter climbs, the compute
takes ~100+ ms, and you see the "Running..." spinner). Same math, two placements.
"""
from __future__ import annotations

import time

_script_t0 = time.perf_counter()

import os  # noqa: E402
import sys  # noqa: E402
from pathlib import Path  # noqa: E402

import streamlit as st  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent))
import counter  # noqa: E402  (the rerun counter lives in an imported module so it survives reruns)
from kernels import response, response_py  # noqa: E402  (response == @wasm, response_py == plain Python)

from pythscribe.streamlit import client_callback, slider_compute  # noqa: E402

st.set_page_config(page_title="pythscribe @wasm — no rerun on a drag", layout="centered")

# Pull the custom-component iframe up so the in-iframe @wasm slider lines up with the native
# st.slider in the other column (Streamlit adds a top gap to the component wrapper). Tuned by
# measuring both slider bounding boxes with Playwright.
st.markdown(
    "<style>"
    "div[data-testid='element-container']:has(> iframe){margin-top:-34px}"  # align the @wasm iframe slider with the native one
    "[data-testid='stTickBar']{display:none}"  # hide the native 0.0/10.0 min/max ticks (custom 0/10 shown below instead)
    ".slider-ends{display:flex;justify-content:space-between;font-size:11px;color:#888;margin-top:8px;font-family:system-ui,sans-serif}"
    ".slider-line{margin-top:-2px;font-family:ui-monospace,monospace;font-size:15px}"
    ".below-slider{margin-top:-11px}"  # pull up into the native slider's blank reserved space so the result lines align (tuned by measuring both result boxes with Playwright)
    "</style>",
    unsafe_allow_html=True,
)

runs = counter.bump()
# Test-only release barrier (production apps never set it); kept so the instantiate-once E2E works.
hold = os.environ.get("PYTHSCRIBE_TEST_HOLD", "").strip() not in ("", "0", "false", "no")

GREEN, RED = "#2b8a3e", "#c92a2a"

st.title("A slider that recomputes in your browser — no Streamlit rerun")
st.caption(
    "Same function f(x) = x³ − 2x + 3 (evaluated ten million times so the compute is measurable), two "
    "placements. Drag each slider and compare the latency; watch the cumulative rerun counter in the sidebar."
)

left, right = st.columns(2)

with left:
    st.html(f'<h4 style="color:{GREEN};margin:.2rem 0">① compiled <code>@wasm</code> — in your browser</h4>')
    spec = client_callback(response, shape="node")
    # Slider + recompute live inside the component iframe: a drag recomputes in the tab with ZERO
    # server reruns; the result and its latency (ms) render inside the component, in green.
    slider_compute(spec, min=0.0, max=10.0, step=0.1, key="flagship", label="x  (0 – 10)", _test_hold=hold)

with right:
    st.html(f'<h4 style="color:{RED};margin:.2rem 0">② plain Python — on the server</h4>')
    xv = st.slider("x  (0 – 10)", min_value=0.0, max_value=10.0, value=0.0, step=0.1, key="native", label_visibility="collapsed", format="%.1f")
    # The ordinary Streamlit way: every drag reruns the whole script and recomputes here, in Python.
    t0 = time.perf_counter()
    res = response_py(float(xv))
    server_ms = (time.perf_counter() - t0) * 1000.0
    # All the below-slider content in ONE block (so it's a single element with one gap), pulled up to
    # match the tight in-iframe layout of the @wasm column so the result lines line up.
    st.html(
        f'<div class="below-slider">'
        f'<div class="slider-ends"><span>0</span><span>10</span></div>'
        f'<div class="slider-line"><b data-testid="contrast-result">f(x) = {res:.1f}</b>'
        f'<span style="color:{RED};font-size:12px;margin-left:10px">{server_ms:.0f} ms · on server · reran</span></div>'
        f'</div>'
    )

# The visceral number, in the sidebar: cumulative across the whole session.
st.sidebar.subheader("Cumulative server reruns")
st.sidebar.html(
    f'<div style="font-size:2.6rem;font-weight:700;color:{RED}" data-testid="server-rerun-count">{runs}</div>'
    '<div style="color:#888;font-size:12px">total whole-script reruns this session — a count, not milliseconds</div>'
)
st.sidebar.metric("Last server compute", f"{server_ms:.0f} ms")
st.sidebar.divider()
st.sidebar.markdown(
    f"<span style='color:{GREEN}'>**Left (`@wasm`)**</span>: drag it → this counter stays **frozen** "
    "(recompute happens in your browser, 0 server calls).  \n"
    f"<span style='color:{RED}'>**Right (native)**</span>: drag it → the counter **climbs by one each "
    "drag**, and you see Streamlit's “Running…” spinner.",
    unsafe_allow_html=True,
)
