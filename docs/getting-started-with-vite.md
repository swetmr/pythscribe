# Getting started with PythScribe + Vite

Five-minute path from empty directory to a working Vite dev server rendering a PythScribe-authored React component.

## Prerequisites

- **Node 18+** (for native `fetch`, ESM, `node:test`)
- **`pyths` binary on PATH**, or built locally at `target/release/pyths`. Install options:
  - `cargo install --path crates/pyths_cli` (from this repo)
  - `cargo build --release --bin pyths` then add `target/release/` to PATH

Verify with `pyths --version`.

## 1. Scaffold a Vite + React project

```bash
npm create vite@latest my-app -- --template react
cd my-app
npm install
```

## 2. Install the plugin

```bash
npm install -D vite-plugin-pyths
```

If you're working from this repo (not yet on npm):

```bash
npm install -D file:../packages/vite-plugin-pyths
```

## 3. Wire it into `vite.config.js`

```js
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import pyths from "vite-plugin-pyths";

export default defineConfig({
  plugins: [
    pyths(),   // <— compiles .ps files to JS before Vite handles them
    react(),
  ],
});
```

Plugin options (all optional):

| Option | Default | Purpose |
|---|---|---|
| `pythsBin` | auto-detect | Path to the `pyths` binary. Auto-detection looks at `target/{debug,release}/pyths[.exe]`, then `pyths` on PATH. |
| `react` | auto-detect from source | Reserved for future explicit toggle; not currently consumed. |
| `reactRefresh` | `"auto"` | React Refresh (HMR with state preservation). `"auto"`: on in dev mode, off in build. `true`: always on. `false`: always off (HMR falls back to full-reload). |

## 4. Write your first component in Python

`src/Counter.ps`:

```python
from pyths.react import component, use_state

@component
def Counter():
    count, set_count = use_state(0)
    return div(class_name="counter")(
        h2()("Count: ", count),
        button(on_click=lambda: set_count(count + 1))("Increment")
    )
```

## 5. Use it from your existing React entry

`src/App.jsx`:

```jsx
import { Counter } from "./Counter.ps";

export default function App() {
  return (
    <main>
      <h1>Hello from React + PythScribe</h1>
      <Counter />
    </main>
  );
}
```

## 6. Run it

```bash
npm run dev
```

The dev server starts. Click the button — `set_count` runs, the component re-renders, all routed through React's normal reconciler. Source maps are wired so DevTools points back to your `.ps` source on errors and breakpoints.

## How it works

The plugin's `transform` hook intercepts every `.ps` import, shells out to `pyths compile --sourcemap` (with `--react-refresh` in dev mode by default), reads the resulting `.js` and `.js.map`, returns them to Vite, then deletes the temp files so they don't pollute your source tree.

Multi-file projects: import your own `.ps` files with Python relative imports (`from .Counter import Counter`) — the plugin resolves them back to `.ps`/`.psc` sources via an importer-aware `resolveId` hook, even when a same-named `.tsx` sits beside them. Full conventions (stdlib routing, the npm kebab-fallback, monorepo aliases, a verified walkthrough) in [multi-file-apps.md](multi-file-apps.md).

In dev mode, the compiled module gets a small React Refresh prelude/postlude that hooks into Vite's `/@react-refresh` runtime. Edits to a `@component` function preserve component state across reload — adding a `setCount(count + 5)` mid-render doesn't reset the counter unless the hook layout changed. If you don't have `@vitejs/plugin-react` in your config, set `reactRefresh: false` on the pyths plugin and HMR falls back to full-page reload.

## Common issues

- **`Could not find pyths binary`** — install the CLI (see Prerequisites) or set `pythsBin: "/abs/path/to/pyths"` in the plugin options.
- **Slow first compile** — the plugin invokes the CLI per `.ps` file. The CLI uses incremental caching (`PYTHS_NO_CACHE=1` to disable), so warm rebuilds are sub-millisecond. Cold builds for a single component are ~5 ms.
- **Type errors from `@vitejs/plugin-react` about `.ps` files** — the React plugin only sees the compiled JS the pyths plugin emits, not the `.ps` source. If you see TypeScript errors from your IDE on `.ps` imports, install Pyright/mypy locally for Python-side type checking; we don't yet emit `.d.ts` for `.ps` files in the plugin path.

## Production build

```bash
npm run build
```

Vite calls the plugin during build the same way it does for dev — every `.ps` becomes JS, then Rollup bundles. Output lands in `dist/` with the rest of your app. Total bundle overhead from PythScribe runtime helpers is ~3 KB gzipped.

## Using `.psc` files (optional)

If you're sourcing components from an AI tool that emits compressed PythScribe, save them with the `.psc` extension. The plugin's `transform` hook matches both `.ps` and `.psc`; the CLI expands `.psc` deterministically before compilation. No additional configuration is needed.

```python
# src/Counter.psc — same component as above, compressed
R*

@c
def Counter():
    c, sc = us(0)
    return <div .counter>
        <h2>{f"Count: {c}"}</h2>
        <button oc=(lambda: sc(c+1))>{"Increment"}</button>
    </div>
```

The output JS is byte-identical to compiling the canonical `.ps` form. See [`docs/compression.md`](./compression.md) for the full `.psc` reference.

## What's next

- **WASM auto-routing**: switch `pyths()` to invoke `pyths compile --target js+wasm` for fixtures that have pure-numeric functions. Vite + WASM works via the standard `?init` query suffix or the `vite-plugin-wasm` plugin.
- **`@psx` helpers**: write JSX-returning utility functions outside `@component` — see `getting-started-with-psx.md` (TBD) or the `@psx` example in `examples/`.
- **Type checking**: run `pyths check src/**/*.ps` in your CI to catch type errors before bundling.
