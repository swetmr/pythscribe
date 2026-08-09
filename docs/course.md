# PythScribe — a short course (React & Next.js, in Python)

A hands-on tour: from "hello" to a small React app to a Next.js page to a
WebAssembly-accelerated function. Every example is real `.ps` source. For the full
reference see [`../README.md`](../README.md), [`language-reference.md`](language-reference.md),
[`getting-started-with-vite.md`](getting-started-with-vite.md), and
[`getting-started-with-next.md`](getting-started-with-next.md).

> This is the short course. A longer, project-based course is planned.

## Setup

```bash
npm i -g pythscribe          # the `pyths` CLI (prebuilt native binary)
# or scaffold a ready-to-run app:
npm create pyths-app@latest my-app && cd my-app && npm install && npm run dev
```

You write `.ps` files in Python syntax; `pyths` compiles them to JavaScript (and, for
pure-numeric functions, WebAssembly).

---

## 1. Hello, PythScribe

```python
# hello.ps
print("Hello, PythScribe")
```

```bash
pyths run hello.ps          # compile + run under Node → Hello, PythScribe
pyths compile hello.ps      # → hello.js (plain ESM)
```

## 2. Python you can trust (semantics, not just syntax)

The compiled JavaScript behaves like the Python you wrote — by default, no flags:

```python
# semantics.ps
print(2**53 + 1)          # 9007254740993  — exact integers (not 9007...992)
print(7 // 2, -7 % 3)     # 3 2            — floor division, divisor-sign modulo
print([1, 2] == [1, 2])   # True           — value equality, not reference
print(len("💩"))           # 1              — code points, not UTF-16 units
d = {"a": 1}
print(d["b"])             # KeyError       — fails loud, no silent undefined
```

This is the whole point: you don't reach for `BigInt`, a comparator, or `Array.from`,
and you don't opt semantics in per module — you write Python, you get Python.

## 3. Your first component

Components are decorated functions; elements are plain calls (PSX). Props and hooks are
**snake_case** and lower to the React names for you (`use_state`→`useState`,
`on_click`→`onClick`).

```python
# counter.ps
from pyths.react import component, use_state

@component
def Counter():
    count, set_count = use_state(0)
    return div(class_name="counter",
        h1(f"Count: {count}"),
        button(on_click=lambda: set_count(count + 1), "+1"),
    )
```

`div(prop=value, child1, child2)` is the default flat form; `div(class_name="x")(children)`
and `div(child)` also work. All lower to `createElement`.

## 4. State, events, and composition

Child components receive props as **named parameters** — plain names, no `props["x"]`
subscripting. Event objects use attribute access (`e.target.value`).

```python
# todo.ps
"use client"
from pyths.react import component, use_state

@component
def TodoItem(text, on_delete):
    return li(text, " ", button(on_click=on_delete, "x"))

@component
def TodoApp():
    items, set_items = use_state([])
    draft, set_draft = use_state("")

    def add():
        if draft:
            set_items([*items, draft])
            set_draft("")

    return div(class_name="app",
        h1("Todo"),
        input(value=draft, on_change=lambda e: set_draft(e.target.value)),
        button(on_click=lambda: add(), "Add"),
        ul(*[TodoItem(text=t, on_delete=lambda t=t: set_items([x for x in items if x != t]))
             for t in items]),
    )
```

Wire it into a Vite project with `vite-plugin-pyths` (see
[`getting-started-with-vite.md`](getting-started-with-vite.md)); `.ps` gets HMR and source
maps like any other module.

## 5. Next.js: server and client components

With `next-plugin-pyths`, `.ps` files are pages/components. A component is a **server
component** by default; add `"use client"` for interactivity. `__default__ = X` lowers to
`export default X` (the App Router page contract).

```python
# app/page.ps  — a server component (can be async, can fetch)
from pyths.react import component

@component
async def Page():
    return main(
        h1("Blog"),
        Counter(),          # a "use client" island, imported from counter.ps
    )

__default__ = Page
```

Full walkthrough (routing, RSC streaming, server actions):
[`getting-started-with-next.md`](getting-started-with-next.md).

## 6. Numbers → WebAssembly (auto-routing)

Pure-numeric functions can be routed to WebAssembly automatically; DOM/React code stays
JavaScript. Same source, two backends.

```python
# kernel.ps
def dot(a: list[float], b: list[float], n: int) -> float:
    s = 0.0
    for i in range(n):
        s += a[i] * b[i]
    return s
```

```bash
pyths compile kernel.ps --target js+wasm   # dot() lands in .wasm; glue runs on Node,
                                           # browsers, Cloudflare Workers, and Deno
```

## Where to go next

- **Reference:** [`language-reference.md`](language-reference.md) (syntax, PSX, extensions).
- **Frameworks:** [`getting-started-with-vite.md`](getting-started-with-vite.md),
  [`getting-started-with-next.md`](getting-started-with-next.md),
  [`multi-file-apps.md`](multi-file-apps.md).
- **Libraries:** the README's ecosystem list (30+ recognized React libraries).
- **Errors:** `pyths run app.ps --explain` prints a Python-style explanation above any crash.
- **What's proved / tested / trusted:** [`../TRUST.md`](../TRUST.md).
