---
title: pythscribe @wasm — resize in the browser before upload (7–206x fewer bytes, 89x aggregate)
emoji: 🖼️
colorFrom: indigo
colorTo: blue
sdk: gradio
sdk_version: 6.26.0
python_version: "3.12"
app_file: app.py
pinned: false
license: mit
short_description: A @wasm-compiled Python resize runs in your tab before upload
---

# Image preprocessing before upload, in the browser, by a `@wasm`-compiled Python kernel

A Gradio app uploads a 12 MB photo so the server can resize it to 512 px. Here the resize is ordinary Python
(`kernels.py::downscale_box`), compiled to WebAssembly by `pyths`, and run **in your browser tab** by the
`pythscribe` Gradio component before anything is uploaded. The server receives ~10–60 KB instead of the full file.

**Positioning:** this is bandwidth + server CPU per request. It is not "faster ML" — no model,
no GPU is involved, and the plan never claims otherwise.

## Two panels, one measurement

| | A. client-side `@wasm` preprocess | B. naive control |
|---|---|---|
| upload | the small JPEG encoded from the WASM output | the original, untouched (stock `gr.File`) |
| resize runs | in the tab (WASM) | on the server (Pillow) |
| what the server measures | request-body bytes it received, `python_calls` (0), the received dims | request-body bytes, Pillow decode/resize/encode ms, CPU ms |

Both panels report the **server-measured** bytes (an ASGI middleware counts the request *body* bytes of every upload —
multipart framing included, HTTP headers/TCP framing not — and maps them to the saved file by content hash, consumed in
upload order, expiring after 120 s), so the reduction is measured against a real control in the same app — never asserted.
Attribution is exact for the sequential single-session measurement the notebook runs; concurrent identical uploads from
several sessions are attributed in upload order. Every record's top-level fields are **server-derived** (received file size, upload bytes, the
decoded output dimensions, whether *this* handler ran the kernel's Python body, timings); everything the browser reported
about itself sits under `client` and is only ever compared against server values (`consistent`, `checksum_match`). The
recorded `path` originates as the browser's claim but is only kept as `browser-wasm` when the server did not run the
kernel for that dispatch, the received file decodes, and its dimensions are consistent with the dispatched `max_dim`;
`python_calls` is *by construction* "did this handler run the kernel" (so it is 0 on every browser-path record — the
non-tautological signal is `python_calls_global_delta`), and that WASM really executed is established by the test
suite's poisoned-`.wasm` control (A4), not by anything the browser says.

## Measured (see `metrics.ipynb`, computed from real runs on the committed `test_images/`)

Run the notebook for the table; the committed `metrics_summary.json` / `bytes_reduction.png` are the last real run.
The notebook drives both panels with a real headless browser, prints per-image bytes uploaded (client vs naive), the
reduction %, the server-side resize time that no longer runs, and asserts its own anti-vacuity controls (browser-path
markers, a full-upload negative control that must go red, derivation of the table from the records).

## Run it

```bash
pip install -r requirements.txt          # a Space installs this; locally: pip install -e ../..[gradio,test] && pip install -e ../../pythscribe/gradio/wasm_function
python app.py                            # http://127.0.0.1:7860
```

The artifacts under `__pythscribe__/` are committed (built by `python -m pythscribe.build kernels.py`; byte-verified against
the kernel source + compiler pin at import). Without them the app still works: the client panel uploads the original and the
**same** kernel runs in Python on the server (`path=python-fallback`, `python_calls=1`).

## Reproduce the metrics

```bash
pip install -e ../..[gradio,test] && pip install -e ../../pythscribe/gradio/wasm_function
python -m playwright install chromium
python make_test_images.py               # regenerates the seeded test set (byte-identical, manifest-pinned)
jupyter nbconvert --to notebook --execute --inplace metrics.ipynb    # or open it and run all cells
python -m pytest ../../tests/pythscribe/test_image_kernel.py ../../tests/pythscribe/test_image_app.py ../../tests/pythscribe/test_notebook_derivation.py
```

## How the seam works (M1's real unknown, de-risked first)

`@wasm` admits scalar returns only, and the compiler's glue copies `list` parameters into WASM memory but never back.
So the kernel fills `out: list[int]` **in place** (plain Python semantics — the CPython fallback does the same) and returns
one scalar (the pixel count). The component instantiates the kernel's own `.wasm`, lays the lists out in linear memory in
the compiler's own list layout (`pythscribe/ffi/list_buffer.mjs`, a declared binding pinned by a test), calls the WASM
export directly — there is **no JS twin on that path**, a trap or overflow is an error, never a silent re-run — reads
`out` back from memory, encodes it with the canvas, and uploads the blob through Gradio's normal upload route.

## Deploy (the human does this; nothing here auto-deploys)

Push this directory as a Space (`sdk: gradio`). `requirements.txt` pins pythscribe to a **release tag on the
public mirror** (`github.com/swetmr/pythscribe`, one squashed commit + tag per release — a dev-repo commit sha can never
resolve there). Until that tag exists, deploy from a checkout by vendoring the two packages into the Space root and
dropping the two `git+` lines:

```bash
# from the pythscribe checkout that contains this directory
cp -r pythscribe <space>/pythscribe              # the runtime package (import pythscribe)
pip wheel ./pythscribe/gradio/wasm_function -w <space>/wheels   # the Gradio component
# in the Space: requirements.txt -> ./wheels/gradio_wasmfunction-*.whl  (pythscribe/ is importable from the app dir)
```

The app binds `0.0.0.0` when `SPACE_ID` is set (loopback locally). Provenance of the committed artifacts, test images and
notebook evidence is recorded in `__pythscribe__/*/manifest.json`; the notebook prints the exact commit it ran at.

Server-side authority for client-named files: the adapter refuses any `result.file.path` outside Gradio's upload
folder, and the app refuses to decode a file whose content the byte meter did not see arrive through `/upload` in this
process — Gradio's own upload-folder check is inert under `mount_gradio_app`, so this is the one gate.
