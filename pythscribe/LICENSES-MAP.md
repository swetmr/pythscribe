# Licences inside the `pythscribe` wheel -- the per-path map

The wheel's licence expression is **`MIT AND LicenseRef-FSL-1.1-ALv2`**: the Python package is MIT and
the compiler binary it bundles is FSL-1.1-ALv2. This file maps every path the wheel installs to the
licence it is under and to the notice text shipped for it (all texts live under
`<dist-info>/licenses/`; the paths below are relative to that directory unless noted).

| Installed path | Component | Licence | Notice text |
|---|---|---|---|
| `pythscribe/**` (decorators, build step, FFI shim, server runtime, Gradio + Streamlit adapters) | the pip runtime | **MIT** | `pythscribe/LICENSE` |
| `gradio_wasmfunction/**` | the vendored Gradio custom component | **MIT** | `pythscribe/LICENSE` |
| `pythscribe/streamlit/wasm_component/**` | the Streamlit custom component | **MIT** | `pythscribe/LICENSE` |
| `pythscribe/_runtime/pyths-runtime/**` | the vendored `pyths-runtime` npm payload (the JS runtime your compiled code imports; also copied into every `__pythscribe__/` artifact) | **MIT** | `pythscribe/_runtime/pyths-runtime/LICENSE` (the payload's own notice, also installed next to the payload) |
| `pythscribe/_bin/pyths` / `pythscribe/_bin/pyths.exe` (absent in a binary-less source install) | the `pyths` compiler binary (Rust crates `pyths_cli`, `pyths_parser`, `pyths_codegen_js`, `pyths_codegen_wasm`, ...) | **FSL-1.1-ALv2** (Functional Source License 1.1, Apache-2.0 future licence: converts to Apache-2.0 two years after each release) | `LICENSE.md` |
| (statically linked into `pythscribe/_bin/pyths[.exe]`) | the third-party Rust crates of the compiler, per release target | each crate's own licence (MIT / Apache-2.0 / Unicode-3.0 / Zlib / ...; see the inventory) | `third_party/inventory.json`, `third_party/notices/*.txt`, `third_party/THIRD_PARTY_NOTICES.md` |
| the compiled output of YOUR code (`.js`, `.wasm`, `__pythscribe__/` artifacts) | -- | **yours** -- the compiler places no licence on its output; the runtime copy inside an artifact is MIT | -- |

**What the split means for you.** Everything that ends up *inside your application* -- the Python
runtime, the adapters, the JS runtime in your bundle or artifact -- is MIT, so using PythScribe places no
source-available licence on your app. The compiler binary is a build tool you run and never redistribute
as part of your app; it is FSL-1.1-ALv2 (source-available, free for any non-competing use, Apache-2.0 after
two years). The `[web-bundled]` extra installs a Node runtime from the `nodejs-wheel-binaries` package,
whose licence notices ship inside THAT package's own wheel, not this one.

The repository-level map is `LICENSING.md`; this file is the wheel-level one and is verified on every
built wheel by `scripts/verify_wheel_license.py`.
