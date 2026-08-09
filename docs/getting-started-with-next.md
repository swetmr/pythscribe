# Getting started with PythScribe + Next.js

Five-minute path from empty Next.js project to a Python-authored page rendering through the App Router.

## Prerequisites

- **Node 18+**
- **`pyths` binary on PATH**, or built locally at `target/release/pyths`. See [getting-started-with-vite.md](./getting-started-with-vite.md#prerequisites) for install options.
- **Next.js 13+** (App Router or Pages Router both supported)

## 1. Scaffold a Next.js project

```bash
npx create-next-app@latest my-app
cd my-app
```

Choose your usual options. The plugin works with App Router, Pages Router, TypeScript, JavaScript, and Tailwind interchangeably.

## 2. Install the plugin

```bash
npm install -D next-plugin-pyths
```

From this repo:

```bash
npm install -D file:../packages/next-plugin-pyths
```

## 3. Wire it into `next.config.mjs`

```js
import withPyths from "next-plugin-pyths";

/** @type {import('next').NextConfig} */
const nextConfig = {
  // your existing Next.js config
};

export default withPyths(nextConfig);
```

If you have other config wrappers (e.g., `withMDX`, `withBundleAnalyzer`), compose them as usual:

```js
import withPyths from "next-plugin-pyths";
import withBundleAnalyzer from "@next/bundle-analyzer";

export default withBundleAnalyzer({ enabled: process.env.ANALYZE === "true" })(
  withPyths({ /* nextConfig */ })
);
```

The plugin:
- Adds `.ps` to webpack's resolvable extensions.
- Adds `.ps` to `pageExtensions` so files like `app/page.ps` and `pages/index.ps` are recognized.
- Installs a webpack loader that shells out to `pyths compile --sourcemap`.

Multi-file projects: import your own `.ps` files with Python relative imports (`from .Counter import Counter`). The loader rewrites the compiled output's extensionless relative specifiers to explicit `.client.js`/`.psc`/`.ps` extensions when a PythScribe sibling exists on disk, so dual-track `.tsx` siblings never shadow the PythScribe module — and `"use client"` islands resolve to their precompiled `.client.js` form. Full conventions in [multi-file-apps.md](multi-file-apps.md).

Plugin options (all optional):

| Option | Default | Purpose |
|---|---|---|
| `pythsBin` | auto-detect | Path to the pyths binary |
| `reactRefresh` | `"auto"` | React Refresh / Fast Refresh. `"auto"`: on in `next dev`, off in `next build`. `true`: always on. `false`: HMR falls back to webpack's default module-reload. |

## 4. Write a page in Python

**App Router** — `app/page.ps`:

```python
from pyths.react import component

@component
def Page():
    return main(class_name="container")(
        h1()("Hello from PythScribe on Next.js"),
        p()("This page is authored in Python.")
    )
```

**Pages Router** — `pages/index.ps`:

```python
from pyths.react import component

@component
def HomePage():
    return main()(
        h1()("Hello from PythScribe")
    )
```

For Pages Router, the special exports also work:

```python
async def get_server_side_props(context):
    return {"props": {"now": "2026-05-08"}}

@component
def HomePage(now):
    return main()(p()("Server time: ", now))
```

`get_server_side_props` lowers to `getServerSideProps` automatically; same for `get_static_props`, `get_static_paths`, `generate_metadata`, `generate_static_params`.

## 5. Run it

```bash
npm run dev
```

Visit `http://localhost:3000`. Source maps are emitted so the browser DevTools "Sources" panel shows your `.ps` files for breakpoints and stack traces.

## App Router conventions

You can mix `.ps` and `.tsx` freely — Next.js doesn't care once both are listed in `pageExtensions`. Common conventions that work:

- `app/page.ps` — route component
- `app/layout.ps` — layout wrapper
- `app/loading.ps` — loading UI (with React Suspense)
- `app/error.ps` — error boundary
- `app/not-found.ps` — 404 handler

`generate_metadata` works as a top-level export:

```python
async def generate_metadata(params):
    return {"title": f"Post {params.id}"}
```

## Server Components

Async server components work end-to-end. PythScribe's `async def @component` compiles to `export async function`, which is exactly the shape Next.js expects for an RSC. The streaming protocol itself is Next.js's job (the runtime serializes server components to the wire format); the codegen just has to emit the right JS shape, which it does.

### Async server component with data fetching

```python
# app/posts/page.ps
from pyths.react import component

async def fetch_posts():
    res = await fetch("https://api.example.com/posts")
    return await res.json()

@component
async def PostsPage():
    posts = await fetch_posts()
    return main()(
        h1()("Recent posts"),
        ul()(
            *[li()(p["title"]) for p in posts]
        )
    )
```

This compiles to a regular `async function PostsPage()` with `await fetch_posts()` inside — Next.js streams it through its RSC protocol automatically.

### React 19's `use()` hook

For unwrapping promises (or context) inside an async component:

```python
from pyths.react import component, use

@component
async def Profile(user_promise):
    user = use(user_promise)
    return div()(h2()(user["name"]))
```

`use()` is recognized as a React import (not as the SVG `<use>` element) — the codegen disambiguates based on what's imported.

### Suspense boundaries

```python
from pyths.react import component, Suspense

@component
def ProfilePage():
    return Suspense(fallback=div()("Loading…"))(
        Profile(user_id=42)
    )
```

`<Suspense fallback={...}>children</Suspense>` works because `Suspense` is a capitalized React import — routes through the `createElement(Suspense, {fallback: ...}, children)` path.

### Server Actions

Two equivalent forms work:

**Module-level** — every function in the file is a Server Action:

```python
# actions.ps
"use server"

from typing import Any

async def create_post(form_data: Any):
    # Runs on the server. Callable from client components.
    return {"saved": True}
```

**Function-level** — single function within a mixed module:

```python
async def create_post(form_data):
    "use server"
    # Same as above; first statement of an async function body.
    return {"saved": True}
```

PythScribe emits the directive verbatim at the top of the function body, which is what Next.js's RSC compiler expects.

### What's NOT yet wired (defer to v2)

- **RSC payload serialization** — Next.js handles this at runtime, but if you're building a custom RSC framework on top of the compiler, the payload generation isn't a codegen concern (it's a runtime library).
- **`'use cache'` (React 19 experimental)** — recognized as a directive but no special caching semantics emitted.
- **Streaming Suspense boundary tracking** — the codegen emits Suspense as a regular component; React's runtime handles the streaming. For complex nested-Suspense apps, test against the framework's expected behavior.

## Production build

```bash
npm run build
npm run start
```

Each `.ps` file becomes part of the Next.js production bundle the same way `.tsx` files do — webpack runs the loader, the resulting JS gets tree-shaken and code-split alongside everything else.

## Common issues

- **`Could not find pyths binary`** — see Vite guide.
- **`page.ps` not recognized as a route** — confirm `next-plugin-pyths` is in `next.config.mjs` and that you restarted the dev server.
- **`document` / `window` not defined errors** — `.ps` files default to running on both server and client; mark client-only files with the `"use client"` directive at the top:

  ```python
  "use client"

  from pyths.react import component, use_state
  ...
  ```

  PythScribe recognizes the directive and emits it correctly.

## Using `.psc` files (optional)

For AI-emitted components, save files with the `.psc` extension instead of `.ps`. The webpack rule `test: /\.psc?$/` and `pageExtensions: [..., "ps", "psc"]` are already wired by `next-plugin-pyths`, so `.psc` files work as page entries (`app/page.psc`), layouts, route handlers, and API routes without any additional configuration.

The CLI expands `.psc` deterministically before compilation; output JS is byte-identical to compiling the canonical `.ps` form. See [`docs/compression.md`](./compression.md) for the full reference.

## What's next

- **API routes**: `app/api/*/route.ps` works the same way pages do — export `GET`, `POST`, etc. as Python functions.
- **Middleware**: `middleware.ps` at the project root.
- **Testing**: `pyths test` runs Python-side unit tests; for component snapshot tests, integrate with `@testing-library/react` against the compiled output.
