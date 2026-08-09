# next-plugin-pyths

Next.js plugin for [PythScribe](https://github.com/your-org/pythscribe) — compile `.ps` (Python) files to JavaScript via webpack loader.

## Install

```bash
npm install -D next-plugin-pyths
```

You also need the [`pyths` CLI](https://github.com/your-org/pythscribe) on PATH or built locally.

## Usage

```js
// next.config.mjs
import withPyths from "next-plugin-pyths";

/** @type {import('next').NextConfig} */
const nextConfig = {
  // your config
};

export default withPyths(nextConfig);
```

Now `.ps` files are recognized as pages, components, layouts, and API routes:

```python
# app/page.ps
from pyths.react import component

@component
def Page():
    return main()(
        h1()("Hello from PythScribe on Next.js")
    )
```

## What the plugin does

1. Adds `.ps` to webpack's resolvable extensions.
2. Adds `.ps` to `pageExtensions` so Next.js recognizes it for routing (App Router and Pages Router both work).
3. Installs a webpack loader that shells out to `pyths compile --sourcemap` for every `.ps` file.

## Options

```js
withPyths(nextConfig, {
  pythsBin: "/path/to/pyths",   // default: auto-detect
})
```

## Composition with other Next.js wrappers

```js
import withPyths from "next-plugin-pyths";
import withBundleAnalyzer from "@next/bundle-analyzer";

export default withBundleAnalyzer({ enabled: process.env.ANALYZE === "true" })(
  withPyths(nextConfig)
);
```

## Special exports

Next.js' magic function names work via the codegen's automatic snake→camel:

| Python | JavaScript |
|---|---|
| `get_static_props` | `getStaticProps` |
| `get_server_side_props` | `getServerSideProps` |
| `get_static_paths` | `getStaticPaths` |
| `generate_metadata` | `generateMetadata` |
| `generate_static_params` | `generateStaticParams` |

## App Router conventions

- `app/page.ps` — route component
- `app/layout.ps` — layout
- `app/loading.ps` — loading UI
- `app/error.ps` — error boundary
- `app/not-found.ps` — 404

`"use client"` directives are recognized — emit them at the top of a `.ps` file:

```python
"use client"

from pyths.react import component, use_state
...
```

### `"use client"` islands under Turbopack — precompile to `.client.js`

On **webpack** (`next --webpack`) and the **Vite** plugin, `"use client"` components compile through the loader like any other `.ps`/`.psc` file. On **Turbopack** (the Next.js 16 default), client-reference proxy generation is the one path the loader cannot serve, so client islands must be **pre-compiled to a `.client.js` sibling** ahead of `next build`/`dev`:

```jsonc
// package.json
"scripts": {
  "precompile-client": "pyths compile app/counter/Counter.psc -o app/counter/Counter.client.js",
  "prebuild": "npm run precompile-client",
  "predev": "npm run precompile-client"
}
```

The component stays PythScribe-authored (`Counter.psc`); the loader's importer-aware rewrite resolves an extensionless `./Counter` to `Counter.client.js` **ahead of** the `.psc` source, so the importing server component pulls in a plain `.js` client module. Use the `.client.js` suffix specifically — precompiling to plain `.js` does **not** work (the rewrite still prefers the `.psc` source).

**Why (root cause).** A Turbopack loader rule must declare an output type via `as` (`as: "*.js"`); omitting it is fatal. When Turbopack builds the client-reference proxy for a `"use client"` module it *appends* the `as` extension to the full source filename instead of replacing it, so a custom extension yields an unresolvable id — `Counter.psc` → `Counter.psc.js` → `Module not found: Can't resolve './Counter.psc.js'`. No `turbopack.rules` option preserves or remaps the id; a proper loader-driven fix needs an upstream Turbopack change. Server components, Suspense, and server actions are unaffected (server references don't go through this proxy). See the [`examples/next-app`](../../examples/next-app/README.md) README and issue #46.

## Source maps

Always emitted. Browser DevTools shows the `.ps` source for breakpoints and stack traces.

## Caveats

- **Server Components (RSC streaming)**: `async def Page()` compiles to `async function Page()`, which is a regular React async function. Full RSC streaming protocol emission is a future work item — for now, drop down to a `.tsx` wrapper if you need streaming Suspense boundaries.
- **`.ps` HMR**: changes to `.ps` files trigger Next.js' standard HMR, but compiled-output HMR doesn't preserve component state across edits. Component-level Refresh integration is on the roadmap.

## License

MIT.
