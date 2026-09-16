# `@wasm` use cases — the three leads, and every row, run

| File | What |
|---|---|
| `simple_demo.ipynb` | **start here** — zero-plumbing demo (`from pythscribe import wasm`, **no build step, no `import kernels`**): decorate a plain function, call it, the **first call compiles it to WASM** (`mode: server`). Five use cases: **speed** (`edit_distance` ~20×), **bit-for-bit determinism** (float kernel), **the sandbox** (`@wasm(fuel=…)` traps a runaway loop), **1-D arrays** and **2-D / image** (`== NumPy`, bit-for-bit) — plus a Gradio/Streamlit wrapping snippet + pointer. Executed, with outputs. |
| `full_features_demo.ipynb` | same zero-plumbing style, **plus a head-to-head table** vs plain Python / NumPy / **Numba** (honest: Numba/NumPy win raw speed, `@wasm` wins the browser / sandbox / bit-for-bit / no-runtime columns), a live `FuelExhausted` sandbox trap, and a capability matrix. `@wasm` via compile-on-first-call throughout; NumPy/Numba/pandas only as comparison + table tooling. Executed, with outputs. |
| `browser_image_filters.ipynb` | **Image filters IN THE BROWSER TAB**: `threshold_lum` / `sobel` / `downscale_nn` (`Array[uint8, 2]` typed arrays) run client-side from Gradio's built-in `js=` hook — no custom component, no server round-trip — and the filtered image updates in the page. Headless-measured (executed, with outputs): 0 compute requests + 0 server kernel calls on the in-tab path (the server tab is the paired negative control and trips both), every output bit-for-bit == NumPy (the `out` rows read back from WASM memory AND the displayed PNG), in-tab vs server latency. |
| `browser_image_filters_app.py` / `browser_image_probe.py` | the app behind that notebook (`pythscribe.gradio.browser.browser_image_loader_js` + a 6-line `js=` handler; a `/pythscribe-probe` route for the counters) and its headless driver (shared with `tests/pythscribe/test_browser_image_filters_e2e.py`) |
| `browser_scalar_client.ipynb` | **`pythscribe.gradio.client_side`**: the four SCALAR use cases (client-side filter, Luhn card validator, on-device PII scan, loan calculator) run in the tab from Gradio's `js=` hook with **no hand-written JS** — `client_side({...}).loader_js` + `call_js([Arg.digits], ...)`. Headless-measured (executed, with outputs): 0 compute requests + 0 server kernel calls per kernel on the in-tab path (the server tab is the paired negative control), every float bit-for-bit == the CPython kernel (and the displayed text), in-tab vs server latency. |
| `browser_scalar_client_app.py` / `browser_scalar_probe.py` | the app behind that notebook (`client_side` + `Arg`s, no JS; a `/pythscribe-probe` route) and its headless driver (shared with `tests/pythscribe/test_browser_scalar_client_e2e.py`) |
| `kernels.py` | the `@wasm` kernels: `edit_distance`, `dtw_distance`, `viterbi`, `mask_digit_runs`, `spin`, `sum_squares`, `is_vowel` / `count_vowels`, the fix-B image kernels `threshold_lum` / `sobel`, and the fix-A scalar kernels `count_above` / `luhn_ok` / `pii_scan` / `monthly_payment` — ordinary Python, compiled by the explicit build step |
| `app.py` | the **isomorphic** Gradio demo: `dtw_distance` in the tab (the M1 component) and in-process on the server (wasmtime), bit patterns side by side with CPython's |
| `iso_drive.py` | drives `app.py` in a headless Chromium (Playwright) and returns the record; runs itself in a subprocess inside a Jupyter kernel |
| `features_lib.py` | every measurement (`measure_*`) and the table (`summarize` / `format_table`); numbers are functions of the inputs |
| `three_use_cases.ipynb` | the three leads: non-vectorisable decoding, sandboxed LLM-generated code (with the escape attempts), isomorphic |
| `full_features.ipynb` | every M1.5 use-case row + fallback + the two honest "where it loses" cases; writes `full_features_records.json` / `full_features_summary.json` |
| `full_features_v0_2_5.ipynb` | the **comprehensive full-feature demo** (executed, with outputs): the canonical idiom; typed arrays (all 5 dtypes, 1-D + 2-D, buffers not elements) == NumPy; the three execution paths (server == browser == CPython, bit-for-bit); the admission edge + the Livermore counterweight (@wasm wins 0 speed columns); and the Gradio + Streamlit adapter markers. Every "WASM ran" claim carries the resolution marker; every correctness claim asserts `== CPython/NumPy`; timings are measured. |
| `wasm_full_features.py` | the helper library behind `full_features_v0_2_5.ipynb` — reuses the shipped runtime + the M2 shim + the M4 harness (no re-implemented marshalling or timing) |
| `__pythscribe__/` | the committed artifacts (`python -m pythscribe.build kernels.py`; hashed, LF, `.gitattributes` inside) |

```bash
cargo build --release --bin pyths               # a `pyths` with typed-array support (or set PYTHSCRIBE_PYTHS)
pip install -e .[test]                           # from the repository root
python -m playwright install chromium            # the real-tab (isomorphic) cells only
python -m pythscribe.build examples/wasm-use-cases/kernels.py
jupyter nbconvert --to notebook --execute --inplace examples/wasm-use-cases/simple_demo.ipynb  # start here (deps: pythscribe + stdlib only)
jupyter nbconvert --to notebook --execute --inplace examples/wasm-use-cases/full_features.ipynb
jupyter nbconvert --to notebook --execute --inplace examples/wasm-use-cases/full_features_v0_2_5.ipynb  # the comprehensive demo
python examples/wasm-use-cases/app.py            # the isomorphic demo, interactively
```

Gates: `tests/pythscribe/test_runtime.py` (the server core, the K7 binding, the sandbox and its RED halves),
`test_use_cases.py` (the three leads incl. the real-tab run and its re-signed-mutant control),
`test_full_features_notebook.py` (the numbers are derived; both notebooks execute from a clean copy).

Positioning: the server path competes with NumPy/Numba/Cython, not with nothing — its wins are the cases
they *cannot* do; the speedup is against interpreted CPython; the vectorised control shows NumPy winning.
