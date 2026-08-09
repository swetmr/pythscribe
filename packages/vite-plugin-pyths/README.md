# vite-plugin-pyths

Vite plugin for [PythScribe](https://github.com/your-org/pythscribe) — compile `.ps` (Python) files to JavaScript on the fly.

## Install

```bash
npm install -D vite-plugin-pyths
```

You also need the [`pyths` CLI](https://github.com/your-org/pythscribe) on PATH or built locally. The plugin auto-detects `target/{debug,release}/pyths[.exe]` and falls back to `pyths` on PATH.

## Usage

```js
// vite.config.js
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import pyths from "vite-plugin-pyths";

export default defineConfig({
  plugins: [pyths(), react()],
});
```

Now `.ps` files are first-class — import them anywhere a `.js`/`.tsx` file would be imported.

```python
# src/Counter.ps
from pyths.react import component, use_state

@component
def Counter():
    count, set_count = use_state(0)
    return div()(
        h2()("Count: ", count),
        button(on_click=lambda: set_count(count + 1))("Increment")
    )
```

```jsx
// src/App.jsx
import { Counter } from "./Counter.ps";
export default function App() { return <Counter />; }
```

## Options

```js
pyths({
  pythsBin: "/path/to/pyths",   // default: auto-detect
})
```

| Option | Default | Purpose |
|---|---|---|
| `pythsBin` | auto-detected | Absolute path to the `pyths` binary. Auto-detection probes `target/{debug,release}/pyths[.exe]`, then `pyths` on PATH. |

## How it works

1. Vite's `transform` hook fires for every `.ps` import.
2. The plugin shells out to `pyths compile <file> --sourcemap`.
3. The resulting `.js` and `.js.map` are read, returned to Vite, and the temp files are deleted.
4. Vite handles the resulting JS like any other module — bundling, HMR, source-map serving for DevTools.

## HMR

`.ps` edits trigger a full reload. Component-level HMR (preserving state across edits) isn't currently supported — would require integrating with React Refresh after compilation.

## Source maps

Enabled by default. Browser DevTools "Sources" panel shows the original `.ps` file; breakpoints and stack traces map back to it.

## Performance

- First compile: ~5–20 ms per file (Node startup + Rust toolchain).
- Warm rebuilds (incremental cache hit): sub-millisecond.
- Bundle overhead from `pyths-runtime`: ~3 KB gzipped.

## Caveats

- The plugin invokes the CLI per file. For projects with hundreds of `.ps` files, this becomes O(N · Node startup) — acceptable but not free. A future version may use the in-process Rust API.
- Type-checking (`pyths check`) is a separate command — run it in CI or via your IDE's Python tooling.

## License

MIT.
