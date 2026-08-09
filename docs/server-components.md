# React Server Components in PythScribe

PythScribe compiles `async def @component` functions to async JavaScript exports that React Server Components (RSC) and the Next.js App Router can consume directly. This document explains what works today, what doesn't, and the integration path for full streaming-Suspense support.

For the broader getting-started flow, see [`getting-started-with-next.md`](./getting-started-with-next.md). For the compiled-output shape, see [`language-reference.md`](./language-reference.md).

## What works today

### Async server components

`async def @component` lowers to `export async function`, which is exactly the shape Next.js expects for an async RSC.

```python
# app/posts/page.ps
from pyths.react import component
from pyths.web import fetch as web_fetch

async def fetch_posts():
    res = await web_fetch("https://api.example.com/posts")
    return await res.json()

@component
async def PostsPage():
    posts = await fetch_posts()
    return main()(
        h1()("Posts"),
        ul()(
            *[li()(post["title"]) for post in posts],
        ),
    )
```

Compiled output:

```js
import { createElement } from "react";

async function fetch_posts() {
    const res = await fetch("https://api.example.com/posts");
    return await res.json();
}

export async function PostsPage() {
    const posts = await fetch_posts();
    return createElement("main", null,
        createElement("h1", null, "Posts"),
        createElement("ul", null,
            ...posts.map((post) => createElement("li", null, post["title"])),
        ),
    );
}
```

This is byte-for-byte what `@vitejs/plugin-react` would emit from the equivalent `.tsx`. The Next.js runtime invokes it on the server, serializes the returned React element tree to the RSC wire format, and streams it to the client.

### Special Next.js exports

Next.js's data-fetching and metadata exports are recognized by name and lowered with their canonical camelCase names so the App Router picks them up:

| PythScribe export | Compiled name |
|---|---|
| `async def generate_metadata(...)` | `generateMetadata` |
| `async def generate_static_params(...)` | `generateStaticParams` |
| `async def get_server_side_props(...)` | `getServerSideProps` |
| `async def get_static_props(...)` | `getStaticProps` |
| `async def get_static_paths(...)` | `getStaticPaths` |

### Module-level directives

The `"use client"` and `"use server"` directives are preserved verbatim at the top of the compiled module:

```python
"use client"

from pyths.react import component, use_state

@component
def Counter():
    count, set_count = use_state(0)
    return button(on_click=lambda: set_count(count + 1))(f"Count: {count}")
```

```js
"use client";
import { createElement } from "react";
import { useState } from "react";
export function Counter() {
    const [count, set_count] = useState(0);
    return createElement("button", { onClick: () => set_count(count + 1) }, `Count: ${count}`);
}
```

### Function-level `"use server"`

Inside an otherwise-client module, marking a function with `"use server"` as its first statement opts it into the server-action protocol:

```python
"use client"

@component
def Form():
    async def submit(data):
        "use server"
        # ...this body executes server-side; the client invokes
        # it as a regular await call.
        await db.insert(data)
    return form(on_submit=submit)(...)
```

### Suspense + `use()` (React 19)

The `use()` hook reads promises and contexts inside a suspending component:

```python
from pyths.react import component, use

@component
async def Profile(user_promise):
    user = use(user_promise)
    return div()(h2()(user["name"]))
```

`<Suspense>` boundaries work via the standard React API:

```python
from pyths.react import component, Suspense

@component
def Page():
    return Suspense(fallback=Spinner())(
        Profile(user_promise=fetch_user(),),
    )
```

## RSC streaming — verified on Next.js 16 (2026-06-19)

The runtime gaps below were **verified end-to-end against Next.js 16.2.9 + React 19 (Turbopack)** via [`examples/next-app/`](../examples/next-app/) (`next build` + `next start`):

1. **Out-of-order chunk serialization** — ✅ `/stream` (`force-dynamic` + `<Suspense>` around a slow async server child): the shell + fallback flush first (`Transfer-Encoding: chunked`), the resolved sub-tree streams as a later chunk, with React's `template id="B:…"` boundary-replacement markers present.
2. **Client/server boundary serialization** — ✅ `/counter`: a server page renders a `"use client"` island and passes a serializable prop (`start=5` → `Count: 5`) across the boundary; the client chunk is referenced in the flight payload.
3. **Server-action FormData handling** — ✅ `/actions`: a `"use server"` module export is wired to `<form action=…>`, registered in the build's server-actions manifest and rendered server-side (full browser POST round-trip is the one part not curl-exercised).

Plus async server-component streaming (`/posts` → `await fetch` + `__next_f` flight payload).

**Integration fixes this surfaced** (all shipped): the plugin gained a Turbopack path (`turbopack.rules` + `resolveExtensions`); `"use client"`/`"use server"` are hoisted above a module docstring (directive must be first); and `__default__ = X` → `export default X` (App Router page/layout contract — previously PythScribe emitted only named exports, so no App Router page could resolve its component).

**Known limitation** — `"use client"` *components* are pre-compiled `.psc`→`.js` (the example's `precompile-client` step) rather than transformed by the Turbopack loader: Turbopack's client-reference proxy appends `.js` to a custom-extension module id (`Counter.psc.js`) and can't resolve it. Server components, Suspense, and server actions all compile via the loader fine. Tracked for a Turbopack-native fix.

## How to test what works today

```bash
# Scaffold a Next.js app with the PythScribe plugin
npx create-next-app@latest my-rsc-app --app
cd my-rsc-app
npm install -D next-plugin-pyths
# ... wire next-plugin-pyths into next.config.mjs per
# docs/getting-started-with-next.md

# Author your async server component
cat > app/posts/page.ps <<EOF
from pyths.react import component
from pyths.web import fetch as web_fetch

@component
async def PostsPage():
    res = await web_fetch("https://jsonplaceholder.typicode.com/posts?_limit=5")
    posts = await res.json()
    return main()(
        h1()("Posts"),
        *[article()(h2()(p["title"]), p()(p["body"])) for p in posts],
    )
EOF

npm run dev
```

Hit `http://localhost:3000/posts` — the component renders on the server, streams to the client, and (with React Refresh wired through the plugin) edits preserve state on the unaffected portions.

## Caveats

- **No `pyths.react.server` runtime module yet**. If the RSC integration tests surface helpers we need (e.g., custom serialization for non-standard return shapes), they'll land under that namespace. Today, all RSC interaction routes through React's own runtime.
- **Type stubs for server-only APIs are sparse**. `next/server`, `next/headers`, `next/cookies` aren't yet in the bundled `.pyi` stubs. Add via `pyths.toml [stubs.paths]` to your project's `stubs/` directory; contribute upstream when stable.
