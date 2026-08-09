# PythScribe + Next.js App Router example

End-to-end integration of PythScribe with the **Next.js 16 App Router (Turbopack)**, verified at runtime against Next.js 16.2.9 + React 19. Every route below was authored in PythScribe (`.ps`/`.psc`), built with `next build`, and exercised with `next start`.

## Routes / what each verifies

| Route | Source | Verifies |
|---|---|---|
| `/` | `app/page.ps` | Server component renders; `__default__` → `export default` (App Router page contract) |
| `/posts` | `app/posts/page.ps` | **Async server component** — `await fetch(...)` server-side, renders the fetched data, ships the RSC flight payload (`__next_f`); `generate_metadata` → `generateMetadata` |
| `/stream` | `app/stream/page.ps` | **Out-of-order Suspense streaming** — `force-dynamic`; shell + `<Suspense fallback>` flush first (`Transfer-Encoding: chunked`), the slow async child streams as a later chunk (React `template id="B:…"` boundary markers) |
| `/counter` | `app/counter/page.ps` + `Counter.psc` | **Client-in-server boundary** — a server page renders a `"use client"` island, passing a serializable prop (`start=5` → `Count: 5`) |
| `/actions` | `app/actions/page.ps` + `actions.ps` | **Server action** — a `"use server"` module export wired to a `<form action=…>` (FormData), registered in the server-actions manifest at build |

## Run it

```bash
cd ../.. && cargo build --release --bin pyths   # one-time
cd examples/next-app
npm install            # Next 16 + React 19; plugin/runtime via file: links
npm run dev            # or: npm run build && npm start
```

## How the plugin wires into Next 16

`next-plugin-pyths` registers the `.ps`/`.psc` loader on **both** bundlers:
- **Turbopack** (Next 16 default) via `turbopack.rules` + `turbopack.resolveExtensions` (so extensionless relative imports resolve to `.ps`/`.psc` siblings).
- **webpack** (`next --webpack`, Next < 16) via `config.module.rules`.

`next.config.mjs` sets `turbopack.root` to the repo root so Turbopack follows the `file:`-linked `pyths-runtime` / plugin symlinks (they live outside this folder).

## Client components under Turbopack — the `.client.js` precompile path

`"use client"` **components** are compiled ahead of time to a **`.client.js`** sibling (the `precompile-client` npm step, run by `prebuild`/`predev`), not transformed by the Turbopack loader. This is the **supported, first-class workflow** for client islands under the Next.js App Router + Turbopack — not a temporary hack. **Server components, Suspense, and server actions all compile via the loader normally** (server references don't hit this); only the client-reference graph needs the precompile path.

**Why the loader can't handle client islands (root cause).** A Turbopack loader rule must declare an output type via `as` (e.g. `as: "*.js"`); omitting it is fatal (`Expected process result to be a module`). When Turbopack generates the **client-reference proxy** for a `"use client"` module, it builds the client module id by *appending the `as` target extension to the full source filename* rather than replacing the extension. For a built-in extension this is a no-op, but for a custom extension the id becomes `Counter.psc` + `.js` = `Counter.psc.js`, which resolves to no file on disk → `Module not found: Can't resolve './Counter.psc.js'`. Setting `as: "*.jsx"` just moves the break to `Counter.psc.jsx` — the suffix always tracks the `as` value, and there is no `turbopack.rules` knob to preserve or remap the original id. **This can only be fixed upstream in Turbopack** (extension-replace instead of extension-append for custom-extension client references, or a client-reference id remap hook). Tracked in issue #46.

**How the precompile path routes correctly.** The island stays PythScribe-authored (`Counter.psc`); `precompile-client` emits a real `Counter.client.js` beside it. The loader's importer-aware import rewrite (`rewritePsImports`) resolves an extensionless `./Counter` to `Counter.client.js` **ahead of** `Counter.psc`, so the server page imports a plain `.js` module that Turbopack's client-reference proxy handles natively — its id is already `.js`-resolvable. The `.client.js` suffix is load-bearing: precompiling to plain `Counter.js` does **not** work, because the rewrite still prefers the `Counter.psc` source and routes it back through the broken loader path.

Verified end-to-end: `next build` prerenders `/counter` with `Count: 5` (the server-passed prop), emits the `Counter_client_*` chunk, and registers the client reference manifest.

## File layout

```
next-app/
├── next.config.mjs            # next-plugin-pyths (Turbopack + webpack), turbopack.root, emitDts:false
├── package.json               # precompile-client → prebuild/predev
└── app/
    ├── layout.ps              # root layout (server)  + __default__
    ├── page.ps                # home (server)
    ├── posts/page.ps          # async server component + generate_metadata
    ├── stream/page.ps         # force-dynamic + Suspense out-of-order streaming
    ├── counter/
    │   ├── page.ps            # server page (renders the client island)
    │   └── Counter.psc        # "use client" component (pre-compiled to Counter.client.js)
    └── actions/
        ├── page.ps            # form wired to the server action
        └── actions.ps         # "use server" action module (FormData)
```
