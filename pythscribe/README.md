# pythscribe — `@wasm` for ordinary Python

**One decorator. Your function stays Python. It runs compiled — in the browser tab, in-process on the
server (sandboxed, GIL-free, bit-for-bit), or as plain Python when nothing is built — and you can always
tell which.**

```bash
pip install pythscribe            # the decorator, the loader, the fallback, both framework adapters
pip install pythscribe[server]    # + wasmtime: run kernels in-process on the server
pip install pythscribe[gradio]    # + Gradio (the custom component ships inside the wheel)
```

```python
from pythscribe import wasm

@wasm
def dot(xs: list[float], ys: list[float]) -> float:
    acc = 0.0
    for i in range(len(xs)):
        acc = acc + xs[i] * ys[i]
    return acc
```

```bash
python -m pythscribe.build kernels.py   # compile each @wasm def with `pyths` -> __pythscribe__/<fn>/
```

Without the build step the function is still your function (plain Python). With it, the same call runs
the compiled kernel, and the decorator tells you which path ran:

```python
>>> from pythscribe import binding_of
>>> binding_of(dot).mode          # 'server' | 'browser' | 'fallback'
'server'
```

## Documentation & demos

- **Installation, supported platforms, and the full guide** — see the
  [project README](https://github.com/swetmr/pythscribe#readme) (Installation section).
- **Runnable demos** (Gradio and Streamlit, browser + server) —
  [`demos/`](https://github.com/swetmr/pythscribe/tree/main/demos).
- **Worked examples** (`@wasm` kernels, image preprocessing, typed arrays) —
  [`examples/`](https://github.com/swetmr/pythscribe/tree/main/examples).

## Licence

The pip package is **MIT** (`pythscribe/LICENSE`). The wheel bundles the `pyths` compiler binary
(`pythscribe/_bin/`, FSL-1.1-ALv2), so the wheel's licence expression is
`MIT AND LicenseRef-FSL-1.1-ALv2`; its `.dist-info/licenses/` carries both texts, the vendored runtime's
notice, a per-installed-path map, and the third-party notices of the crates linked into the binary.
Compiled output of your own code is yours.
