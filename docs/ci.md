# CI/CD setup for PythScribe projects

Reference workflows for the most common cases. Drop into `.github/workflows/` (GitHub Actions), adapt freely for GitLab/CircleCI/Buildkite.

## What CI should do for a PythScribe project

1. **Install the `pyths` binary** (cargo install or download a pre-built binary).
2. **Run `pyths check`** to type-check every `.ps` file.
3. **Run `pyths lint`** for unused imports, naming, unreachable code.
4. **Compile the project** (or run the bundler — Vite/Next.js triggers compilation automatically).
5. **Run unit tests** if you have any.
6. **Optionally**: run end-to-end tests (Playwright) against the built artifacts.

## Workflow A — Vite project

`.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  build-and-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm

      # Install pyths via cargo (cached). For a faster CI, host a
      # pre-built binary as a release asset and `curl` it instead.
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install pyths
        run: cargo install --git https://github.com/your-org/pythscribe --locked

      - name: Install npm deps
        run: npm ci

      - name: Type-check .ps sources
        run: pyths check src/**/*.ps

      - name: Lint .ps sources
        run: pyths lint src/**/*.ps

      - name: Build
        run: npm run build

      - name: Unit tests
        run: npm test
```

## Workflow B — Next.js project

Same as Vite but the build step is `npm run build && npm run start &` if you also want to run e2e tests against the running server. The plugin gates compilation through webpack so `pyths` is invoked automatically — no explicit compile step needed.

## Workflow C — Standalone library (publish to npm)

```yaml
name: Publish

on:
  push:
    tags: ['v*']

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          registry-url: https://registry.npmjs.org
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo install --git https://github.com/your-org/pythscribe --locked
      - run: pyths check src/**/*.ps
      - run: pyths compile src/index.ps --dts   # emit .d.ts for TS consumers
      - run: npm publish
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
```

## Performance tuning

The `pyths` toolchain compiles ~124,000 lines/sec end-to-end. For most projects (<10 KLOC of `.ps`) compile time is in the low tens of milliseconds, dominated by Node startup. If you observe slow CI:

1. **Cache `target/`** — the cargo install step is the slow part; once cached, subsequent runs install in seconds.
2. **Cache `node_modules/`** — standard practice.
3. **Pre-built binary**: rather than `cargo install`, host the compiled binary as a release asset:
   ```yaml
   - name: Download pyths binary
     run: |
       curl -L -o /usr/local/bin/pyths \
         https://github.com/your-org/pythscribe/releases/download/v0.1.0/pyths-x86_64-linux
       chmod +x /usr/local/bin/pyths
   ```
4. **Skip recompilation** with the incremental cache. The cache lives in a
   per-user directory *outside* the source tree (never inside the repo, for
   security — a checked-in cache is never trusted). Pin its location with
   `PYTHS_CACHE_DIR` and persist that path across runs:
   ```yaml
   - uses: actions/cache@v4
     with:
       path: .pyths-cache
       key: pyths-${{ hashFiles('src/**/*.ps') }}
   - run: pyths compile src/app.ps
     env:
       PYTHS_CACHE_DIR: .pyths-cache
   ```
   > Security note: the compiler ignores any `.pyths/cache/` committed into a
   > project. Set `PYTHS_CACHE_DIR` (or rely on the default user cache dir —
   > `%LOCALAPPDATA%\pyths`, `$XDG_CACHE_HOME/pyths`, or `~/.cache/pyths`).

## Cloudflare Workers deployment

```yaml
- run: pyths compile src/worker.ps --target wasm-edge
- uses: cloudflare/wrangler-action@v3
  with:
    apiToken: ${{ secrets.CF_API_TOKEN }}
    command: deploy
```

`--target wasm-edge` produces a single JS file with the WASM bytes embedded as base64 — Workers can't fetch sibling files at module scope, so inlining is required.

## Vercel deployment

Standard Vercel setup just works for Next.js projects with `next-plugin-pyths`. Add a build hook to install `pyths`:

`vercel.json`:

```json
{
  "buildCommand": "cargo install --git https://github.com/your-org/pythscribe --locked && next build",
  "installCommand": "npm ci"
}
```

Or use a `.vercelignore` to skip the cargo step locally and let CI handle it.

## What to monitor in production

- **Bundle size** (`npm run build` output, or Next.js's `_buildManifest.js`). PythScribe adds ~3 KB gzipped runtime overhead. Anything >10 KB suggests something pulled in unexpectedly.
- **Cold-start latency** (Cloudflare Workers analytics, Vercel Edge logs). Target: <50 ms cold-start TTFB.
- **Type-check pass rate** in CI. Failing `pyths check` is your main signal that a recent edit broke something.
- **Compiled output diff size** between PRs (use `gzip -c dist/*.js | wc -c` and post the delta to PR comments).

## Security notes

- The `pyths` binary executes during build only — there's no runtime execution path that could be hijacked by a malicious `.ps` file (compile-time-only).
- The compiled JS has no dynamic `eval` or `Function()` usage; output is statically analyzable.
- The runtime helpers (`pyths-runtime`) are pinned npm packages with no transitive deps. `npm audit` should report clean.
- For SSR / server contexts, treat compiled `.ps` output the same way you'd treat any other JS module — sanitize inputs at the network boundary, not in the runtime.

## What's not covered yet

- **`pyths.toml` for project-wide config** — exists at the repo root as `pyths.toml.example`, but the plugins don't yet read it. Settings are passed via plugin options for now.
- **Source-map upload to Sentry/Rollbar** — works the same way any JS source map works; Next.js / Vite have built-in plugins. PythScribe's source maps are standard v3 maps.
- **Watch mode for non-Vite/Next setups** — `pyths compile --watch` works standalone; `pyths run` for one-off scripts.
