# Licensing — which component is under which licence

PythScribe is one repository with two licences, drawn along the line between the **compiler** (the thing
we sell and protect) and the **runtime you ship to users** (which must be free to embed, redistribute and
audit). Each component carries its own LICENSE file; this page is the map.

| Component | Path(s) | Licence | LICENSE file |
|---|---|---|---|
| The compiler: Rust crates (`pyths_cli`, `pyths_parser`, `pyths_codegen_js`, `pyths_codegen_wasm`, …) and the Rust side of `pyths_runtime` | `crates/` | **FSL-1.1-ALv2** (Functional Source License 1.1, Apache-2.0 future licence — converts to Apache-2.0 two years after each release) | [`LICENSE.md`](./LICENSE.md) |
| The `pyths` CLI / npm wrapper and the framework plugins (`@pythscribe/*`, `vite-plugin-pyths`, `next-plugin-pyths`, `create-pyths-app`) | `npm/`, `packages/` | **FSL-1.1-ALv2** | [`LICENSE.md`](./LICENSE.md) |
| The Lean verification layer | `verification/` | **FSL-1.1-ALv2** | [`LICENSE.md`](./LICENSE.md) |
| The JS runtime the emitted code imports (`pyths-runtime`) — the code that ends up in **your** bundle | `runtime/`, and its byte-identical embedded copy `crates/pyths_runtime/js/` | **MIT** | [`runtime/LICENSE`](./runtime/LICENSE), [`crates/pyths_runtime/js/LICENSE`](./crates/pyths_runtime/js/LICENSE) |
| The pip package `pythscribe` — `@wasm`, the in-process server runtime, the FFI shim, the build step, and the framework adapters incl. the vendored Gradio component | `pythscribe/` (wheel: `pythscribe`, `gradio_wasmfunction`) | **MIT** | [`pythscribe/LICENSE`](./pythscribe/LICENSE) |
| **The pip wheel composition** (`pip install pythscribe`) — a MIXED distribution: the MIT Python package above **plus** the vendored `pyths-runtime` payload (`pythscribe/_runtime/pyths-runtime/`, MIT) **plus** the bundled `pyths` compiler binary (`pythscribe/_bin/pyths[.exe]`, FSL-1.1-ALv2, statically linking third-party Rust crates under their own licences) | the wheel `pythscribe-<v>-py3-none-<platform>.whl` | **`MIT AND LicenseRef-FSL-1.1-ALv2`** (PEP 639 `License-Expression`) | in every wheel's `.dist-info/licenses/`: `pythscribe/LICENSE` (MIT), `LICENSE.md` (FSL), `pythscribe/_runtime/pyths-runtime/LICENSE` (the runtime's own MIT notice), [`pythscribe/LICENSES-MAP.md`](./pythscribe/LICENSES-MAP.md) (the per-installed-path map) and [`third_party/`](./third_party/) (the per-target third-party inventory generated from `Cargo.lock`) |
| Compiled output of your own `.ps` / `@wasm` code (the `.js`, `.wasm`, `__pythscribe__/` artifacts) | wherever you put it | **yours** — the compiler places no licence on its output; the runtime copy inside an artifact is MIT | — |

**Why this split.** Anything that is *linked into or shipped with your application* — the JS runtime, the
`.wasm` loader/marshalling glue, the pip runtime, the Gradio/Streamlit components — is MIT, so using
PythScribe never places a source-available licence on your app (the "GCC exception" principle). The
compiler itself, which you run at build time and never redistribute, is FSL-1.1-ALv2: source-available,
free for any non-competing use, and automatically Apache-2.0 after two years.

**PyPI metadata (mixed wheel, spec 13-09-26 M5).** `pyproject.toml` declares
`license = "MIT AND LicenseRef-FSL-1.1-ALv2"` — the Python package is MIT, the compiler binary it bundles is
FSL — and `license-files` ships every notice into `<dist-info>/licenses/`: `pythscribe/LICENSE` (MIT),
`LICENSE.md` (FSL), the vendored runtime's own `pythscribe/_runtime/pyths-runtime/LICENSE` (MIT),
`pythscribe/LICENSES-MAP.md` (which installed path is under which licence) and `third_party/**`
(`inventory.json` + `notices/*.txt` + `THIRD_PARTY_NOTICES.md`: every third-party crate statically linked
into the binary, per release target, with its licence text). `third_party/` is GENERATED from `Cargo.lock`
by `scripts/gen_third_party_notices.py` (`cargo metadata` per target + `cargo about`, config `about.toml`)
and committed; it records the `Cargo.lock` sha it came from, so a dependency bump without regeneration is
RED. `scripts/verify_wheel_license.py` checks every built wheel (expression, every notice by name AND
content, the inventory bound to `Cargo.lock` and — with `--check-closure` — to the live dependency
closure). The `[web-bundled]` extra adds nothing to OUR wheel's notices: it installs Node from the
`nodejs-wheel-binaries` package, whose licence ships inside that package's own wheel, not ours. Third-party
notices for other vendored code, if any, live next to the component that vendors it.
