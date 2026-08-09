# PythScribe Worker (benchmark)

A Cloudflare Worker exposing three compute functions written in PythScribe and compiled to WASM.

## Build

From this directory:

```bash
pyths compile src/compute.ps -o src/worker.js --target wasm-edge
```

This produces:
- `src/worker.js` — entry module (embeds WASM bytes as base64)
- `src/worker.wasm` — sidecar WASM (kept for inspection; bytes are also inlined)

`wrangler.toml` and `package.json` at this directory point at `src/worker.js`.

## Endpoints

- `GET /fibonacci?n=30`
- `GET /sum_squares?n=10000`
- `GET /sin_sum?n=10000`

Returns `{ result: <number> }`. Returns 404 with `{ available: [...] }` if the path isn't a known export.

## Local dev

```bash
npm install
npx wrangler dev
# Worker is now at http://localhost:8787
curl 'http://localhost:8787/fibonacci?n=30'
```

## Deploy

```bash
npx wrangler deploy
```
