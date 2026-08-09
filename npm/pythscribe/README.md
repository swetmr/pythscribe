# pyths

**PythScribe — Python for the browser and the edge, compiled.**

`pyths` is the ahead-of-time compiler CLI for PythScribe: it compiles `.ps` / `.psc`
(Python-syntax) source to JavaScript and WebAssembly, with Python's semantics preserved
(a machine-checked verified core covers the tricky ones — integers, floor-division,
strings, `round`, `sorted`, bitwise, and more).

## Install

```sh
npm install -g pythscribe      # global CLI
# or
npx pythscribe --help          # one-off
```

This package ships **prebuilt native binaries** via per-platform optional dependencies
(`@pythscribe/cli-<os>-<arch>`); npm installs only the one matching your machine. No Rust
toolchain, no build step, no interpreter shipped to your users.

**Supported platforms:** `win32-x64`, `linux-x64`, `linux-arm64`, `darwin-x64`, `darwin-arm64`.

If your platform isn't prebuilt (or optional dependencies were skipped), build from source
with a Rust toolchain:

```sh
cargo install --git https://github.com/swetmr/pythscribe pyths
```

## Usage

```sh
pyths build app.ps           # compile to JS
pyths --help
```

For framework integration use the plugins, which invoke this compiler for you:
[`vite-plugin-pyths`](https://www.npmjs.com/package/vite-plugin-pyths),
[`next-plugin-pyths`](https://www.npmjs.com/package/next-plugin-pyths), and the
[`pyths-runtime`](https://www.npmjs.com/package/pyths-runtime).

## License

The compiler is licensed under the **Functional Source License 1.1 (FSL-1.1-ALv2)** — source-available,
converting to Apache-2.0 after two years; see `LICENSE.md`. (The PythScribe **runtime** and framework
**plugins** that ship inside your app — `pyths-runtime`, `vite-plugin-pyths`, `next-plugin-pyths` — are MIT.)
