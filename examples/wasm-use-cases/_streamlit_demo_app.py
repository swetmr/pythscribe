"""Streamlit @wasm demo, launched by streamlit_demo.ipynb via `streamlit run`.

`pairwise` (sum of |xi - xj|, an O(n^2) loop NumPy/Numba can express but @wasm keeps portable and
deterministic) runs IN THE BROWSER through the pythscribe Streamlit component (a sandboxed iframe;
the compiled .wasm is base64'd into the component args), with the server-side @wasm-vs-Python
speedup shown alongside as live metrics. Run:  streamlit run _streamlit_demo_app.py
"""
from __future__ import annotations

import random
import time

import streamlit as st

from pythscribe import binding_of, wasm
from pythscribe.streamlit import WasmComponent, deadline_result, describe, dispatch, result_of


@wasm
def pairwise(xs: list[float]) -> float:
    n = len(xs)
    total = 0.0
    for i in range(n):
        for j in range(i + 1, n):
            d = xs[i] - xs[j]
            if d < 0.0:
                d = -d
            total = total + d
    return total


def pairwise_py(xs):
    n = len(xs)
    total = 0.0
    for i in range(n):
        for j in range(i + 1, n):
            d = xs[i] - xs[j]
            if d < 0.0:
                d = -d
            total = total + d
    return total


def _med(fn, k=5):
    t = []
    for _ in range(k):
        t0 = time.perf_counter()
        fn()
        t.append((time.perf_counter() - t0) * 1e3)
    return sorted(t)[len(t) // 2]


st.title("pythscribe @wasm — Streamlit")
st.markdown(
    "`pairwise` (sum of |xi − xj|) — the **same `@wasm` kernel** running **in this browser tab** "
    f"(artifact: **{binding_of(pairwise).artifact_status}**), with the server-side speedup shown below."
)

n = st.slider("number of points", 100, 800, 400, 50, key="n")
rng = random.Random(0)
xs = [rng.random() for _ in range(n)]

if st.button("Run @wasm in the browser", key="run"):
    st.session_state["payload"] = dispatch(pairwise, xs)

payload = st.session_state.get("payload")
r = None
if payload is not None:
    if payload.get("wasm_b64"):
        comp = WasmComponent()
        value = comp(payload=payload, key="wasm", default=None)
        r = result_of(pairwise, value, expect_nonce=payload["nonce"])
        if r is None:
            r = deadline_result(pairwise, payload, wait_s=20)
    else:
        r = result_of(pairwise, payload, expect_nonce=payload["nonce"])

if r is not None:
    st.subheader("browser result")
    st.code(describe(r), language=None)
    st.caption("path=browser-wasm and python_calls=0 mean the value was computed in your tab, not the server.")
    tw = _med(lambda: pairwise(xs))
    tp = _med(lambda: pairwise_py(xs))
    st.subheader("server-side timing (same kernel)")
    c1, c2, c3 = st.columns(3)
    c1.metric("server @wasm", f"{tw:.1f} ms")
    c2.metric("plain Python", f"{tp:.1f} ms")
    c3.metric("speedup", f"×{tp / tw:.0f}")
