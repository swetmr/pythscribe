# Pyodide Worker (benchmark comparison)

A Cloudflare Worker that runs the same workload as `pythscribe-worker/` but using Pyodide (CPython compiled to WASM, ~6 MB compressed).

The Pyodide runtime is fetched from `cdn.jsdelivr.net` on first request. Each new isolate pays the full Pyodide initialization cost on its first request.

## Endpoints

Same as the PythScribe worker:
- `GET /fibonacci?n=30`
- `GET /sum_squares?n=10000`
- `GET /sin_sum?n=10000`

## Local dev

```bash
npm install
npx wrangler dev --port 8788
curl 'http://localhost:8788/fibonacci?n=30'
```

## Deploy

```bash
npx wrangler deploy
```

## Caveats

- Pyodide on Cloudflare Workers is unofficially supported; depending on your account configuration, you may need to enable `nodejs_compat` (already set in `wrangler.toml`) and the dynamic-import path may need polyfilling.
- Cold-start measurements assume a fresh isolate per first request. Cloudflare reuses isolates aggressively; to force a cold start, you can deploy a no-op change between measurement passes or use `wrangler dev` locally where every restart is a cold start.
