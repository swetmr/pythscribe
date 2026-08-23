# Contributing to `examples/clones`

This workspace hosts heavy interactive website clones (`/youtube`, `/netflix`, `/coursera`,
`/spotify`, `/kanban`, `/twitter` stretch) built **once** as shared tri-track components and
mounted by **two** thin app shells: a React (Vite + react-router v7) app and a Next.js 16
(App Router) app. It is both PythScribe's internal heavy-interactive stress-test and the
public launch demo material.

This file is the contract every clone-building agent/contributor follows. Read it before
adding a component.

## Layout

```
examples/clones/
  shared/<clone>/          ← ALL clone content lives here, framework-agnostic
    Name.tsx                 React oracle ("use client" if interactive)
    Name.ps                  canonical PythScribe, dual-track with Name.tsx
    Name.psc                 compressed PythScribe, round-trips to Name.ps
    fixtures.ts               local mock data ONLY (no network)
    Name.test.tsx             vitest render-parity (mounts all 3 tracks)
  vite/                     ← React 19 + react-router v7 shell
  next/                     ← Next.js 16 App Router shell
  e2e/                      ← Playwright specs, run against BOTH apps
  public/media/             ← canonical tiny offline media (copy into vite/public/media
                               and next/public/media — see "Media assets" below)
  scripts/                  ← verify-psc.mjs, precompile-client.mjs
```

## The per-clone contract

1. **All clone content lives under `shared/<clone>/`.** Nothing clone-specific lives inside
   `vite/` or `next/` beyond a route file that imports and mounts the shared component(s).
   The two apps are thin shells — if you find yourself writing clone logic inside
   `vite/src/pages/` or `next/app/<clone>/page.tsx` beyond a mount + fixture pass-through,
   that logic belongs in `shared/<clone>/` instead.

2. **Every component is tri-track.** For each `shared/<clone>/Name`:
   - `Name.tsx` — the React reference oracle. Built first (per the repo-root build
     methodology: React is ground truth, PythScribe is the system under test).
   - `Name.ps` — canonical PythScribe, matching the `.tsx` snapshot exactly (same DOM, same
     behavior).
   - `Name.psc` — compressed PythScribe. Must round-trip byte-identically to `Name.ps` via
     `pyths expand --verify` (`npm run verify:psc`). See the `compressing-pythscribe-to-psc`
     skill for the tier-by-tier authoring workflow — apply one tier per pass, verify the
     round-trip after each pass, never skip the check.
   - If the component is interactive (has state, event handlers), mark **all three tracks**
     `"use client"` (`'use client'` in `.tsx`, `"use client"` as the first statement after
     any module docstring in `.ps`/`.psc`).

3. **`fixtures.ts` is local mock data ONLY.** No `fetch`, no network calls, no imports from
   a backend client. If a clone needs to *look* like it's calling an API, mock it entirely
   in `fixtures.ts` with a synchronous or `Promise.resolve`-wrapped shape. This keeps every
   clone runnable fully offline, which is a hard requirement (see "Media assets").

4. **Every component ships a parity test**, `Name.test.tsx`, next to it in `shared/<clone>/`.
   Mount all three tracks against the SAME props/fixtures and assert:
   - each track individually satisfies the behavioral contract (a shared `contract()`
     helper — see `shared/hello/HelloCard.test.tsx` for the template);
   - a DOM-snapshot equality check between the `.tsx` oracle and the `.ps`/`.psc` tracks,
     both at initial render and after at least one state-changing interaction.

   These tests run from the **workspace root** (`npm test`, driven by `vitest.config.ts`,
   which globs `shared/**/*.{test,spec}.{ts,tsx}`) — not nested inside either app. Shared
   components must be testable without booting either app's dev server.

5. **No git commands.** Agents building clones do not run `git add`/`commit`/`push`/branch
   operations. The controller commits. This applies to every file under `examples/clones/`.

## Styling

- Global/shared tokens (dark theme, shell/grid/card chrome) live in `shared/theme.css`,
  imported once at each app's entry point (`vite/src/main.tsx`, `next/app/layout.tsx`).
  Do not re-import `theme.css` from individual clone components.
- Component-scoped CSS lives beside the component (`shared/<clone>/Name.css`) and is
  imported as a side effect **from the `.ps`/`.psc` track** using PythScribe's A2 extension:
  ```python
  import "./Name.css"
  ```
  This is a real `.ps`-only language extension (Python has no bare-string import form) —
  see `shared/hello/HelloCard.ps` / `HelloCard.css` for the dogfooded example. The `.tsx`
  oracle imports the same file the normal ESM way: `import './Name.css'`.
- Prefer `className`/`class_name` + the stylesheet over inline `style={}` objects — cheaper
  in tokens and matches production practice (see the repo's PythScribe style-skill).

## Binary + plugin resolution

- `pyths` binary: `process.env.PYTHS_BIN ?? 'pyths'`
  — this exact fallback pattern is used in `vite/vite.config.ts`, `vitest.config.ts`, and
  `next/next.config.mjs`. Keep new configs consistent with it (reference-app-style env-var
  override).
- `pyths-runtime` / `vite-plugin-pyths` / `next-plugin-pyths` are `file:` deps pointing at
  this repo's `runtime/` and `packages/*` — never floated to a registry version. See the
  repo's "pin, don't float" discipline if you're touching these.
- Next's `turbopack.root` is set to the **repo root** (`examples/clones/next/next.config.mjs`
  resolves `../../..`), not just `examples/clones/`, so Turbopack can follow the `file:`
  symlinks into `runtime/`/`packages/` AND resolve relative imports that reach up into
  `../../shared/<clone>/`.

## `"use client"` islands in the Next shell

Next's Turbopack cannot transform a custom-extension (`.ps`/`.psc`) module through its
client-reference graph (a known upstream Turbopack limitation for webpack-loader rules on
client components — server components, layouts, and pages are unaffected and compile
through the loader normally). The workaround, already wired into this scaffold:

1. Any `shared/<clone>/Name.ps` whose first statement (after an optional module docstring)
   is literally `"use client"` gets precompiled to `shared/<clone>/Name.client.js` by
   `scripts/precompile-client.mjs`.
2. That script runs automatically via `next/package.json`'s `predev`/`prebuild` hooks — you
   do not need to register new components anywhere. Adding a new interactive `shared/<clone>/Name.ps`
   is enough; the next `npm run dev -w next` / `npm run build -w next` picks it up.
3. `next-plugin-pyths`'s loader rewrites extensionless relative imports to prefer
   `.client.js` over `.psc`/`.ps` when a sibling `.client.js` exists on disk (see
   `packages/next-plugin-pyths/loader.js`'s `rewritePsImports`). So a server page written as
   ```python
   from ....shared.hello.HelloCard import HelloCard
   ```
   resolves to the precompiled island automatically — **no per-app import-path change is
   ever needed.** Write the import as if you were importing the `.ps` source directly.
4. `Name.client.js` is gitignored (`shared/**/*.client.js` in `.gitignore`) — it is a build
   artifact, regenerated every `predev`/`prebuild`. Never hand-edit it, never commit it.

The Vite shell has no such limitation: `vite-plugin-pyths` uses an importer-aware
`resolveId` hook and loads `.ps`/`.psc` live, so the same `shared/<clone>/Name.ps` is
imported directly with no precompile step from `vite/src/**`.

**Relative-import dot-counting gotcha:** PythScribe's `from ...pkg import X` uses Python's
relative-import convention — dot count `N` means `N - 1` directory levels up from the
current file, then descend into `pkg`. From `next/app/<clone>/page.ps` reaching
`shared/<clone>/Name`, that's 3 levels up (`<clone>/` → `app/` → `next/` → workspace root),
so the import needs **4 dots**: `from ....shared.<clone>.Name import Name`. Miscounting by
one silently resolves to the wrong (nonexistent, or worse, wrong-but-existing) directory —
always `pyths compile --stdout` a new page in isolation and check the emitted `import`
line before wiring it into the app.

## Test harness

| Command (workspace root) | What it does |
|---|---|
| `npm run test` | vitest — runs every `shared/**/*.test.tsx` render-parity suite (no app boot required) |
| `npm run verify:psc` | `pyths expand --verify` on every `shared/**/*.psc`; must be N/N |
| `npm run dev:vite` / `npm run dev:next` | dev servers for the two shells (ports 5173 / 3000) |
| `npm run build` | production build of BOTH apps (vite build, then next build — next's `prebuild` hook runs `precompile-client` first) |
| `npm run e2e` | builds both apps, then runs Playwright against **both**: `vite preview` on a FIXED port `4173`, `next start` on a FIXED port `3999` (serial — vite first, then next) |
| `npm run e2e:vite` / `npm run e2e:next` | run just one app's Playwright suite (still expects a prior `npm run build`) |

Playwright specs live in `e2e/<topic>.spec.ts` and are **framework-agnostic** — the same
spec file runs against both apps via two configs (`e2e/playwright.vite.config.ts`,
`e2e/playwright.next.config.ts`) that only differ in `baseURL`/`webServer`. Write specs
against routes, not against either app's internals, so one spec always covers both tracks.
See `e2e/hello.spec.ts` as the template.

Before running `npm run e2e` locally, make sure ports `4173` and `3999` are free (kill any
stale `vite preview` / `next start` processes) — both configs use FIXED ports on purpose (no
port-hunting) so CI and local runs are reproducible.

## Media assets

Clones that play real media (YouTube, Spotify) must work **fully offline** — no network
fetches, no CDN URLs. Canonical tiny public-domain assets live in `public/media/` at the
workspace root:
- `sample.webm` — ~1.5s silent video clip
- `sample.wav` — ~1s PCM tone

Both `vite/public/media/` and `next/public/media/` carry a **physical copy** of these files
(each app's own dev/prod server only serves its own `public/`; there is no cross-app
symlinking on this Windows dev box). If you regenerate the canonical assets, copy them into
both app `public/media/` directories again.

**Container note:** the brief's original ask was for `.mp4`; the assets shipped here are
`.webm`/`.wav` instead. The only `ffmpeg` binary available on this dev machine is
Playwright's bundled `ffmpeg-win64.exe` (installed for screen-capture recording), and it is
a stripped build with **no `libx264`, no AAC, no audio encoder of any kind, and no `lavfi`
synthetic-source filter** — only `image2pipe`(mjpeg-decode) → `libvpx`(VP8) → `webm` muxing,
plus a PNG encoder. A general-purpose `ffmpeg` was not installed. If you need a real `.mp4`
for a specific clone, install a full `ffmpeg` build first; otherwise stay with `.webm`
(every evergreen browser plays it natively via `<video>`) or fall back to a `data:` URI
placeholder poster image, per the original brief's escape hatch.

## Known friction

**Turbopack panics on non-ASCII module docstrings.** Next.js 16's Turbopack (Rust `hstr`
crate) panics with `do not lie on character boundary` when it prescans a leading top-level
*string-literal statement* (i.e. a real triple-quoted module docstring, not a `#` comment)
whose bytes contain a multi-byte UTF-8 character within roughly the first ~16 bytes (e.g. an
em dash `—`, a curly quote, an arrow). The scan appears to be part of Turbopack's
directive-detection preamble (looking for `"use client"`/`"use server"`), which slices
leading statement strings by byte offset without checking UTF-8 character boundaries — so it
crashes on ANY `.ps`/`.psc` file (client or server) whose module docstring has non-ASCII
content near the start, not just interactive islands. **Workaround used throughout this
scaffold: module-level documentation in `.ps`/`.psc` files is written as `#` comment blocks,
never triple-quoted docstrings** — comments are stripped entirely by the PythScribe compiler
and never reach the emitted JS as a statement, so they can contain any Unicode safely (and
avoid the minor runtime cost of constructing a throwaway docstring string on every module
evaluation). Keep following this convention for `.ps`/`.psc` module docs; save
triple-quoted docstrings for the Vite-only pieces of a clone if you want real `__doc__`
introspection there. This is an upstream Next.js/Turbopack bug (not a PythScribe compiler
bug — verified via `pyths compile --stdout` producing correct JS on the same source), so it
is not tracked in the PythScribe repo's issue queue; flagged here for awareness.

## Checklist: adding a new clone

1. `mkdir shared/<clone>` and add each component's `.tsx` (oracle, build first) →
   `.ps` (canonical, match the oracle snapshot) → `.psc` (compressed, `pyths expand --verify`
   after every authoring pass) → `fixtures.ts` → `Name.test.tsx`.
2. `npm run test` and `npm run verify:psc` green before wiring either app.
3. Vite: routes are PRE-WIRED — replace the body of `vite/src/pages/clones/<Name>.tsx` and `<Name>Reference.tsx` (do NOT edit `App.tsx`)
   directly (live-compiled, no build step needed in dev).
4. Next: replace the `<clone>/page.tsx` "Coming soon" stub with a `<clone>/page.ps` (or
   `.tsx` if the route is a pure static server component with no interactive island) that
   imports from `shared/<clone>/`. Mirror it at `react-reference/<clone>/page.tsx` importing
   the `.tsx` oracle directly.
5. Add `e2e/<clone>.spec.ts` covering the route in both apps (one spec file, both configs).
6. `npm run build` and `npm run e2e` green in both apps before considering the clone done.


## Parallel-agent rules (controller-enforced)
- NEVER edit: `vite/src/App.tsx`, root/workspace `package.json`, another clone's dirs, this file.
- NO new npm dependencies — implement interactions with vanilla React (pointer events for drag, IntersectionObserver for infinite scroll). This is deliberate: it stress-tests PythScribe, not libraries.
- NO git commands. NO full `npm run e2e` (port conflicts) — write your `e2e/<clone>.spec.ts`, verify locally with vitest on your dir + `verify:psc` + ONE dev-server smoke on YOUR assigned port, then kill it.
- PythScribe compiler bugs you hit: document precisely in your report (repro + error) and work around in-app if possible. Do NOT modify the compiler/runtime — the controller triages upstream fixes.
