# Multi-File Apps — Local Imports & Module Resolution

A single `.ps` file compiles standalone, but a real app spans many files:
pages importing components, components importing shared data modules,
everything importing stdlib and npm packages. This doc covers how the
compiler emits import specifiers, how the Vite and Next.js plugins resolve
them, and the one convention that trips people up (the kebab-case fallback).

Everything below is the implemented behavior — the emitted-JS annotations
were produced by running `pyths compile --stdout` on the snippets.

## The resolution chain

For `import X` / `from X import y`, the compiler resolves the module name
in this order (`resolve_module` in `crates/pyths_codegen_js/src/emit.rs`):

| # | Rule | Example |
|---|------|---------|
| 1 | Relative imports (`from .x import y`) emit a literal relative specifier — they bypass every rule below | `from .store import make_task` → `"./store"` |
| 2 | `pyths.toml [npm.imports]` overrides emit verbatim | `foo_bar = "@scope/foo-bar"` |
| 3 | Compile-time-only modules are suppressed (no JS import) | `dataclasses`, `pydantic`, `typing` |
| 4 | Python stdlib names map to the runtime's stdlib | `import math` → `"pyths-runtime/stdlib/math"` |
| 5 | `pyths.*` web modules map into the runtime | `from pyths.fetch import get` → `"pyths-runtime/web/fetch"` |
| 6 | Known npm mappings + the `at_<org>.<pkg>` scoped convention | `from at_tanstack.react_query import ...` → `"@tanstack/react-query"` |
| 7 | **Fallback:** kebab-cased npm bare specifier | `from framer_motion import motion` → `"framer-motion"` |

Rule 7 is the important one for local files: **any non-relative,
non-stdlib module name is assumed to be an npm package**, and every
segment is kebab-cased (`_` → `-`). That is correct for the long tail of
npm packages and wrong for your own snake_case files — which is why local
imports should be relative (next section).

## Relative imports — the canonical form for local files

Python-style relative imports emit literal relative ESM specifiers.
No kebab-casing, no npm remapping, no stdlib routing — the dotted path
converts directly, with one leading `.` stripped and each extra `.`
becoming a `../` segment:

```python
from .theme import COLORS            # → import { COLORS } from "./theme";
from .pages.Home import Home        # → import { Home } from "./pages/Home";
from ..lib.store import make_task   # → import { make_task } from "./../lib/store";
x = [COLORS, Home, make_task]
```

Two things to notice about the emitted specifiers:

- **They are extensionless** (`"./theme"`, not `"./theme.ps"`). The host
  bundler picks the file — which is what makes the same emitted JS work
  whether the target on disk is `theme.ps`, `theme.psc`, or even a shared
  `theme.ts` data module. (In `examples/clones/shared/coursera/`,
  `CourseraApp.ps` does `from .fixtures import COURSES, QUIZ` against a
  plain `fixtures.ts` — the plugin defers to the bundler, which resolves
  the TypeScript file.) How each bundler resolves the extension is covered
  in "How the plugins resolve `.ps`/`.psc`" below.
- **Names pass through verbatim.** `from .my_store import x` targets
  `./my_store` — snake_case survives, because the kebab fallback never
  runs for relative imports.

`pyths compile` is per-file: it emits the specifier without checking that
the target exists. A typo'd relative import surfaces at bundle time
(Vite/Next "failed to resolve"), not at compile time.

## Absolute "monorepo" prefixes — what happens and the workarounds

An absolute-looking local path hits the kebab fallback and is emitted as
an npm bare specifier:

```python
from app.components.GapsPanel import GapsPanel
# → import { GapsPanel } from "app/components/GapsPanel";
from app.my_widgets import Widget
# → import { Widget } from "app/my-widgets";   (segments are kebab-cased)
```

The compile succeeds, then the bundler fails with
`Failed to resolve import "app/components/GapsPanel"` — there is no npm
package called `app`. Options, in order of preference:

1. **Use relative imports.** Intra-project, this is the supported path
   and needs zero configuration.
2. **Teach the bundler the prefix.** If you genuinely want a shared
   absolute namespace (monorepo-style), alias it and register the
   PythScribe extensions:

   ```js
   // vite.config.js
   import path from "node:path";
   import pyths from "vite-plugin-pyths";

   export default {
     plugins: [pyths()],
     resolve: {
       alias: { app: path.resolve(__dirname, "src") },
       extensions: [".mjs", ".js", ".ts", ".tsx", ".json", ".ps", ".psc"],
     },
   };
   ```

   Both halves are required: the alias maps the prefix to a directory,
   and the `extensions` entry lets the extensionless emitted specifier
   find a `.ps` file. Mind the kebab-casing — a `src/my_widgets.ps` will
   be looked up as `src/my-widgets` through this path, so aliased trees
   should use kebab-case (or CamelCase, which passes through untouched)
   file names.
3. **Per-module `pyths.toml` override.** `[npm.imports]` keys are exact
   Python module names, values are emitted verbatim:

   ```toml
   [npm.imports]
   "app.components.GapsPanel" = "/src/components/GapsPanel.ps"
   ```

   Exact-key-per-module makes this a niche tool — it exists mainly for
   irregular npm package names, not local trees.

## Stdlib and runtime imports

The bare Python names route to `pyths-runtime`; `pyths.<name>` reaches
web wrappers and is also accepted as an alias for the stdlib modules:

```python
import math                     # → import * as math from "pyths-runtime/stdlib/math";
import json as j                # → import * as j from "pyths-runtime/stdlib/json";
from pyths.math import sqrt     # → import { sqrt } from "pyths-runtime/stdlib/math";
from pyths.fetch import get     # → import { get } from "pyths-runtime/web/fetch";
from pyths.storage import local # → import { local } from "pyths-runtime/web/storage";
print(math.sqrt(4), sqrt(9), j.dumps([1, 2]))
```

Stdlib names: `math`, `json`, `itertools`, `functools`, `collections`,
`random`, `datetime`, `re`, `decimal`, `fractions`. Web modules:
`pyths.fetch`, `pyths.storage`, `pyths.router` (→ `pyths-runtime/web/*`)
plus `pyths.dom` (→ `pyths-runtime/dom`). Any other `pyths.x.y` maps to
`pyths-runtime/x/y`.

## How the plugins resolve `.ps`/`.psc`

The compiler emits extensionless relative specifiers, so the bundler
decides which file wins — which matters in dual-track projects where a
React reference `Counter.tsx` sits beside the `Counter.ps` under test.
Global extension order (`.tsx` before `.ps`) would silently resolve the
`.tsx`; both plugins prevent that, by different mechanisms.

### Vite (`vite-plugin-pyths`) — importer-aware `resolveId`

The plugin registers a `resolveId` hook: when the **importer** is a
`.ps`/`.psc` file and the specifier is relative, it checks for a `.ps`
then `.psc` sibling on disk (then `index.ps`/`index.psc` for directory
imports) and resolves to it directly — bypassing Vite's global
`resolve.extensions` order. Everything else (npm specifiers, `.tsx`
importers, relative targets with no PythScribe sibling, e.g. shared `.ts`
fixture modules) falls through to Vite's normal resolution. Compilation
itself happens in the plugin's `transform` hook, which shells out to
`pyths compile` per module.

### Next.js (`next-plugin-pyths`) — loader + emitted-specifier rewriting

The plugin registers `loader.js` for `*.ps`/`*.psc` on both bundlers
(Turbopack via `turbopack.rules`, webpack via `module.rules`) and appends
`.psc`, `.ps` to the resolvable extensions (`turbopack.resolveExtensions`
/ `resolve.extensions` — `.psc` first, so the compressed variant wins
when both exist). Neither webpack nor Turbopack supports
importer-conditional resolution, so the loader instead **rewrites the
compiled output**: `rewritePsImports` scans emitted `import`/`export ...
from` statements and, for every extensionless relative specifier with a
PythScribe sibling on disk, appends an explicit extension — checking
`.client.js`, then `.psc`, then `.ps`. An explicit extension bypasses
resolve order on both bundlers, so a `.ps` module importing `./Counter`
gets `./Counter.ps` even when a `Counter.tsx` oracle sits beside it.

The `.client.js` preference is the App Router island story: Turbopack's
client-reference proxy can't handle custom-extension module ids in the
client graph, so `"use client"` components are **precompiled** to plain
`.js` siblings ahead of `next dev`/`next build` (see
`examples/clones/scripts/precompile-client.mjs`, wired as
`predev`/`prebuild`) instead of going through the loader. Server
components, layouts, pages, Suspense, and server actions all compile
through the loader live; only client islands take the precompile path.
Details and the tracking note are in
[server-components.md](server-components.md).

## Real multi-file examples in this repo

`examples/clones/shared/` holds six clone demos (`youtube`, `netflix`,
`coursera`, `spotify`, `kanban`, `twitter` + a `hello` smoke), each
tri-track: a `.tsx` React oracle, the `.ps` PythScribe implementation,
and the `.psc` compressed variant, side by side with shared `fixtures.ts`
data and per-component CSS. The `examples/clones/vite/` and
`examples/clones/next/` app shells mount the same shared components, so
the directory demonstrates every mechanism above — relative imports,
`.ts` fixture imports, side-effect CSS imports, dual-track sibling
resolution, and precompiled `.client.js` islands — under both bundlers.

## Walkthrough: a three-file app

A minimal shape — shared data module, a page component, a root:

```
src/
├── App.ps
├── lib/
│   └── store.ps
└── pages/
    └── TaskList.ps
```

```python
# src/lib/store.ps — shared data module (no React)
import json

def make_task(title, done=False):
    return {"id": title.lower().replace(" ", "-"), "title": title, "done": done}

def serialize(tasks):
    return json.dumps(tasks)
```

```python
# src/pages/TaskList.ps — component importing a sibling package
from pyths.react import component, use_state
from ..lib.store import make_task

@component
def TaskList(initial_titles):
    tasks, set_tasks = use_state([make_task(t) for t in initial_titles])
    return ul(class_name="tasks",
        *[li(key=t["id"], t["title"]) for t in tasks],
    )
```

```python
# src/App.ps — root component wiring the pages together
from pyths.react import component
from .pages.TaskList import TaskList

@component
def App():
    return div(class_name="app",
        h1("Tasks"),
        TaskList(initial_titles=["Write docs", "Verify walkthrough"]),
    )

__default__ = App
```

Each file compiles independently (`pyths compile src/App.ps` etc. — the
plugins do exactly this per module); the emitted imports are
`"./../lib/store"` from `TaskList` and `"./pages/TaskList"` from `App`,
which the Vite or Next.js plugin then resolves back to the `.ps` files.
`__default__ = App` produces `export default App` for entry points that
want a default export (Next.js App Router pages require it). Wire
`src/App.ps` into `main.tsx`/`main.ts` with an explicit extension —
`import App from "./App.ps"` — since a `.ts` importer doesn't get the
importer-aware resolution and shouldn't rely on global extension order.
