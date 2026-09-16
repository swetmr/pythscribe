# gradio_wasmfunction

The pythscribe `WasmFunction` Gradio custom component: runs a `@wasm` kernel's compiled
JS/WASM bundle in the browser tab and returns the value to the Gradio script. Built with Gradio's
custom-component tooling (Svelte 5). See `pythscribe/gradio/__init__.py` for the adapter API.

Build the frontend after checkout: `cd frontend && npm install && cd .. && gradio cc build --no-generate-docs`
(on Windows pass `--python-path <your python.exe>`: the tool otherwise shells out to the Store `python3`
alias, and a broken interpreter makes the frontend step report success while writing nothing).

The component also supports **image mode** (`payload.kind == "image"`, see `frontend/types.ts`): it shows a file
picker, decodes the picked image in the tab, runs the `@wasm` `box_scale` + `downscale_box` kernels through
the list-buffer FFI shim (`frontend/list_buffer.mjs` -- a byte-identical copy of `pythscribe/ffi/list_buffer.mjs`,
enforced by a test), encodes the result with the canvas and uploads the small JPEG via `gradio.shared.client.upload`.
Any failure on that path uploads the ORIGINAL (`path: "upload-original"`) so the server's Python fallback runs.
