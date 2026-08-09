# npm distribution (maintainer notes)

esbuild/swc-style native-binary distribution for the `pyths` compiler.

```
npm/
  pythscribe/                # the wrapper users install (`npm i -g pythscribe`); provides `pyths`; no binary inside
    bin/pyths.js             #   resolves the host platform package at runtime + execs its binary
  @pythscribe/cli-win32-x64/       # per-platform packages: os/cpu-gated, each holds ONE binary in bin/
  @pythscribe/cli-linux-x64/
  @pythscribe/cli-linux-arm64/
  @pythscribe/cli-darwin-x64/
  @pythscribe/cli-darwin-arm64/
  build-platform-packages.mjs  # (re)writes each platform package.json + copies any built binary
  publish.mjs                  # publishes platform packages FIRST, then the wrapper (dry-run by default)
```

How install works: `pyths` declares every `@pythscribe/cli-*` as an **optionalDependency**; each
platform package pins `os`/`cpu`, so npm installs only the matching one. `bin/pyths.js`
resolves that package at runtime and forwards argv + exit code. Nothing is downloaded in a
postinstall — the binary arrives as a normal package.

## Release procedure

1. **Build each triple's release binary** (one CI job per triple — CI matrix; do NOT hand-build
   cross targets on one host). Land each in `target/<rust-triple>/release/pyths[.exe]`, or run
   the build script on that platform so the host-native `target/release/` binary is picked up:
   - `x86_64-pc-windows-msvc` → `pyths.exe`
   - `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` → `pyths`
   - `x86_64-apple-darwin`, `aarch64-apple-darwin` → `pyths`
2. On each build, run `node npm/build-platform-packages.mjs` — it writes the platform
   `package.json`s and copies whatever binary is present into `bin/` (chmod +x on unix).
   Collect all five populated `bin/` dirs (CI artifact upload/download) onto one publish host.
3. Bump `VERSION` in `build-platform-packages.mjs` **and** `pyths/package.json`
   `version` + `optionalDependencies` in lockstep. (Currently `0.1.0`.)
4. `node npm/publish.mjs` (dry run) → verify tarball contents → `node npm/publish.mjs --yes`
   (requires `npm login`; publishes platform packages first, wrapper last).

## Status (2026-07-31)

- Scaffold + wrapper + all five platform `package.json` generated.
- **win32-x64 binary populated** (host build). The other four triples need cross-compilation —
  scheduled for the CI matrix (CI returns 2026-08-01). `publish.mjs` refuses to publish until all
  five binaries are present, so no half-published state is possible.
- Version pinned to `0.1.0` (aligned with the JS packages; honest pre-1.0).


## Automated publish (CI, recommended)

The `Release` workflow (`.github/workflows/release.yml`) now publishes to npm on a version tag:
push `vX.Y.Z` → it builds all 5 platform binaries (incl. arm64-linux + darwin on macOS runners),
stages each into `target/<triple>/release/`, runs `build-platform-packages.mjs`, then
`publish.mjs --yes` (platform packages first, wrapper last, `--provenance`). **One-time setup:**
add an npm **automation access token** for the `@pythscribe` org (+ the unscoped `pyths` name) as
the repo secret **`NPM_TOKEN`**. Then `git tag v0.1.0 && git push --tags` publishes everything.
