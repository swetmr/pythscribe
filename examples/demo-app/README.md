# PythScribe Demo App

A minimal end-to-end demo: a `@component` counter compiled to React, plus a pure-numeric `compute.ps` compiled to WASM.

## Run it

```bash
# Compile the React component
pyths compile Counter.ps

# Compile the WASM compute (separate file, separate target)
pyths compile compute.ps --target wasm

# Open index.html in any browser. Click +1.
open index.html  # macOS
xdg-open index.html  # Linux
start index.html  # Windows
```

## Files

- `Counter.ps` — `@component` with `use_state` and a button (15 lines).
- `compute.ps` — pure-numeric Fibonacci (WASM-eligible).
- `index.html` — static loader that imports the compiled `Counter.js`. Uses `esm.sh` for React so no bundler is needed.

## What this is for

This minimal demo walks through:
1. Open `Counter.ps` — show the @component decorator.
2. `pyths compile Counter.ps` — show JS output.
3. Open `index.html` — click the counter.
4. `pyths compile compute.ps --target wasm` — show the small `.wasm`.
5. (Optional) — show the hybrid pattern where Counter calls `fibonacci` from `compute.ps`.
