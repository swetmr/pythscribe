# PythScribe vs Pyodide Cold-Start Benchmark

A side-by-side benchmark of two Cloudflare Workers running the same compute workload, one compiled by PythScribe to WASM and one running Pyodide.

## What this measures

- **Cold-start latency** — time from first request to first byte after the Worker isolate is freshly cycled.
- **Steady-state latency** — p50, p95, p99 of warm requests under sustained load.
- **Bundle size** — gzipped sizes of the deployed Worker artifacts.

## Why it matters

The pitch slide for PythScribe is *"Python on the edge with sub-millisecond cold start."* Pyodide is the closest comparable: a CPython runtime compiled to WASM that ships with most of the standard library. The tradeoff is roughly:

| | PythScribe | Pyodide |
|---|---|---|
| Bundle size | one purpose-built `.wasm` (typically <10 KB) | ~6 MB compressed runtime + your code |
| Cold start | one `WebAssembly.instantiate()` call | runtime download + Python interpreter init |
| Coverage | ~25-35% of Python (Tier 1 strings; Tiers 2-7 in progress) | ~95% of CPython including stdlib |

PythScribe wins on cold start and bundle size; Pyodide wins on coverage. This benchmark confirms the first claim quantitatively.

## Workload

Both workers expose the same three endpoints:
- `GET /fibonacci?n=30` — recursive Fibonacci
- `GET /sum_squares?n=10000` — sum of squares 1..n
- `GET /sin_sum?n=10000` — sum of `sin(i)` for `i in 1..n` — exercises math.* import path

Each returns `{ result: <number> }` as JSON.

## Layout

```
cloudflare-bench/
  pythscribe-worker/
    src/compute.ps         — the PythScribe source
    src/worker.js          — compiled output (generated; checked in for review)
    src/worker.wasm        — compiled WASM (generated)
    wrangler.toml          — generated
    package.json           — generated
    README.md
  pyodide-worker/
    src/index.js
    wrangler.toml
    package.json
    README.md
  bench/
    run.mjs                — Node script that hits both deployed URLs
    measure-bundles.mjs    — reports gzipped bundle sizes
  RESULTS.md               — captured numbers
```

## Running it

```bash
# 1. Build the PythScribe worker
cd pythscribe-worker
pyths compile src/compute.ps -o src/worker.js --target wasm-edge

# 2. Deploy both
cd pythscribe-worker && wrangler deploy
cd ../pyodide-worker && wrangler deploy

# 3. Bench
node bench/run.mjs --pythscribe=https://YOUR-PS-URL --pyodide=https://YOUR-PY-URL
node bench/measure-bundles.mjs

# 4. View results
cat RESULTS.md
```

If you don't have wrangler accounts, run `wrangler dev` locally for each Worker and point the bench script at `http://localhost:8787` / `http://localhost:8788`.
