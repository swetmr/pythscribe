# pythscribe `@wasm` demos

Three self-contained demos of compiling Python to `@wasm` and running it **in the browser** — no
server round-trip. Everything here needs only the `pythscribe` pip package (which bundles the `pyths`
compiler) plus the extras in `requirements.txt`.

```
pip install -r requirements.txt
```

## 1. `gradio_multitab_demo.ipynb` — Gradio, client-side `@wasm` across a full app
A multi-tab Gradio app (launched inline from the notebook), four focused tabs, each making a distinct
point: an **isomorphic card (Luhn) validator** and an **on-device PII scan** (client-side via Gradio's
`js=` hook — the number/text **never leaves the browser**); a **server-side image filter** (`@wasm` vs
plain Python, ~50×); and the **placement-ceiling** tab — a compiled-`@wasm` slider (the loan calculator
looped 20k×, ~tens of ms in the browser, 0 server calls) beside a native server-callback slider that
round-trips (~1 s per drag).

A **🔒 network monitor** (top-right) proves the privacy claim rather than asserting it: it instruments
`fetch`/`XHR` and counts data actually POSTed to the server. Type a card number in the Luhn/PII tabs
and it stays **0** (nothing left the browser); it only ticks when a tab genuinely round-trips (the
image filter, or the placement-ceiling native slider).

Run: open in Jupyter and run all cells (the last cell calls `app.launch(...)`).

## 2. `wasm_features_demo.ipynb` — the honest `@wasm` capability + timing tour
Compiled `@wasm` vs plain Python vs NumPy vs Numba across representative kernels (edit-distance,
crc32, viterbi, k-means, feature-hashing, mandelbrot, …): median-of-100 timing with bootstrap CIs,
`@wasm == Python` bit-for-bit correctness, a fuel-sandbox trap, and the server == browser == CPython
isomorphism. Honest about where NumPy/Numba win — the point is fidelity + browser/sandbox placement.

Run: open in Jupyter and run all cells.

**Speedup gate (0.2.9).** The notebook carries a benchmark of record (`BENCHMARK_SPEEDUPS`: the
median-of-100 `@wasm` speedup over plain Python per kernel, all seven on the `server` path) and refuses a
run where any kernel drops below half its benchmark or below 1.0×. A plain local run is **HARD** (an
`AssertionError` stops it); set `PYTHSCRIBE_PERF_MODE=soft` to get a `WARNING` per kernel instead — the
mode CI / shared runners use, since timing is host-varying and must never red a shared runner. The gate
is exercised both ways by `tests/pythscribe/test_wasm_speedup.py`.

## 3. `streamlit_slider_demo/` — Streamlit, a slider with **zero server reruns**
Streamlit reruns the whole script on every widget interaction. This demo puts a compiled-`@wasm`
slider (recomputes **inside the component iframe**, 0 reruns, a few ms) beside a native `st.slider`
wired to the SAME function in plain Python on the server (a full script rerun + "Running…" spinner,
~1 s per drag). Same math, two placements — the sharpest illustration of the Streamlit rerun ceiling.

It uses `client_callback(fn, shape="node")` + `slider_compute(...)` from `pythscribe.streamlit`. Note
this Streamlit path is narrower and less ergonomic than `@wasm` in Gradio (which has a first-class `js=`
hook): today the host is one slider driving an arity-1 scalar kernel. See **"Streamlit (the callback
path)"** in [`../pythscribe/README.md`](../pythscribe/README.md) for the API and the future-usage note.

```
streamlit run streamlit_slider_demo/app.py
```

## Notes
- The notebooks' `@wasm` kernels **compile on first call** (they need `pyths`, bundled in the wheel);
  no build step required.
- The Streamlit app ships **prebuilt artifacts** in `streamlit_slider_demo/__pythscribe__/`, so it
  runs immediately; to rebuild after editing the kernel:
  `python -m pythscribe.build streamlit_slider_demo/kernels.py`.
- `assets/photo_640x480.jpg` is the sample image for the Gradio image-filter tab.
