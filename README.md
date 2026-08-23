# PythScribe

**Python for the browser and the edge, compiled.**

[![npm](https://img.shields.io/npm/v/pythscribe.svg)](https://www.npmjs.com/package/pythscribe)
[![CI](https://github.com/swetmr/pythscribe/actions/workflows/ci.yml/badge.svg)](https://github.com/swetmr/pythscribe/actions/workflows/ci.yml)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.21875694.svg)](https://doi.org/10.5281/zenodo.21875694)
[![License: FSL-1.1-ALv2](https://img.shields.io/badge/license-FSL--1.1--ALv2-blue.svg)](./LICENSE.md)

PythScribe is an ahead-of-time compiler that transcribes Python into JavaScript and WebAssembly via a blazing-fast Rust toolchain. Classes, components, and APIs live in `.ps` files as Python syntax and compile to production-ready JS (and WASM for compute-heavy functions) that slots into React, Next.js, or any web project.


## Design Philosophy

Language is deeply connected to how human beings operate and feel, the syntax, semantics and readability
matter as much as aesthetics. PythScribe is designed to eliminate the syntactic struggle and reduce cognitive load for python developers who can now pick up frontend work in a shorter time by just learning React or Next (the framework matters more than the language).

As is often the case, the more elegant way is also the more concise. PythScribe delivers **up to ~25%
token savings** versus regular JavaScript/TypeScript — from its inherent conciseness plus the optional
`.psc` compression layer, and, for AI-emitted code, a measured generation-token reduction (the precise
per-model figures, medians, and confidence intervals are in the [`.psc` section below](#psc-an-llm-oriented-ir)
and the [compression paper](https://doi.org/10.5281/zenodo.21386779)) — while also incorporating useful
operators like optional chaining and nullish coalescing. It can be useful for enterprises/startups looking
to cut down on token costs.

The hybrid compilation to JS and WASM will serve well in compute-heavy and edge/serverless deployments.

Finally, even if AI-generated code is the future, readability, efficiency and aesthetics matter — if anything, *more*, since we now review more code than we write. That argument, at length, is [**The Aesthetics of Code**](https://swetmrigank.substack.com/p/the-aesthetics-of-code).

### Correct-by-default semantics — not just Pythonic syntax

Syntax is half the story; the other half is **semantics**. On every axis where JavaScript carries a decades-old quirk, Python's model is simply *less surprising* — and PythScribe gives you Python's. Semantic preservation is **machine-checked in Lean over selected language fragments** (20 preservation waves and an observational-preservation taxonomy with a representative union theorem), bound to the shipping compiler by differential testing rather than proved end-to-end. The rows below are the behaviors PythScribe targets: some are covered by the verified fragments, and all are exercised by the CPython differential corpus:

| | JavaScript's quirk | What PythScribe gives you |
|---|---|---|
| **Strings** | UTF-16 units: `"💩".length === 2`, `s[0]` can be half a character; `.indexOf` returns UTF-16 offsets | code points end-to-end: `len("💩") == 1`, `s[0] == "💩"`, and `.index`/`.find` return code-point offsets |
| **Integers** | `Number` loses precision past 2⁵³ (`2**53 + 1` is wrong) | exact, arbitrary precision (incl. `**`) |
| **Floats** | silent rounding everywhere | proved **exact on the safe-integer domain** (`|v| ≤ 2⁵³`) — no ε where Python has none |
| **Division** | `%` truncates toward zero, sign of the dividend | `//` floors, `%` takes the divisor's sign (`-7 // 2 == -4`) — on both the JS and WASM backends |
| **Bitwise** | 32-bit coercion: `1 << 40 === 256`; negatives truncate to 32 bits | arbitrary precision; `~x` and `&`/`\|`/`^` in Python's infinite two's-complement |
| **`round()`** | `Math.round` is half-**up**: `round(0.5) === 1` | banker's rounding (half-to-**even**): `round(0.5) == 0`, `round(2.5) == 2` |
| **`sorted()`** | default `.sort()` is lexicographic: `[1, 2, 10]` → `[1, 10, 2]` | ordered by value: `[1, 2, 10]` |
| **Missing key** | silent `undefined` | `KeyError` — fails loud |
| **Equality** | `==` coercion (`[] == ![]` is `true`) | Python's `==` and truthiness |

Each row is a place a naive Python→JS transpile *silently breaks*. Several of these behaviors are proved in the verified core over their covered fragment — for example, astral-string indexing corrected to code-point behavior, and WASM `i64` arithmetic equal to the wrapped Python value — and all of them are exercised by the CPython differential corpus; the naive translation demonstrably differs on the listed cases (naive JS `.indexOf` returns UTF-16, not code-point, offsets once an astral character precedes the match; native 32-bit `<<` cannot compute `1 << 40`). This matters most for **real-world text and LLM-generated content** — emoji, mixed scripts, non-BMP characters, where JS's UTF-16 indexing quietly corrupts strings and counts — and for data/numeric work where exact integers, banker's rounding, value-ordered sorts, and fail-loud collections prevent silent bugs. The output is **readable Python syntax with CPython-faithful semantics**, compiled to the browser and edge — correct semantics are the *default*, checked by differential testing and, for the covered fragments, by proof, rather than left to remembering `Array.from` / `BigInt` / a comparator / an explicit `Map`. In an era where code is generated more than hand-written, that is the point: the **syntax** stays readable for *review*, and the **semantics** stay faithful enough to *trust* what was generated. (Honest caveats: **fractional** float arithmetic and transcendentals follow IEEE-754 in both — only the safe-integer float domain is proved exact so far; and the correctness helpers carry a runtime cost — both documented, not hidden.)

---

## Hello, Counter

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

PSX uses pure Python syntax — HTML elements are function calls. The default form nests props and children together as positional/keyword args of one call: `tag(prop=v, child1, child2)`. Two equivalent forms are also accepted: curried `tag(prop=v)(child1, child2)` (separates props from children visually — preferred by some for deep trees) and direct `tag(child)` (single positional arg, no props). All compile to the same `createElement(tag, props, ...children)`. Flat form is enabled by a deliberate parser relaxation — positional args may follow keyword args in `.ps`, which standard Python rejects. The relaxation applies only in `.ps` (don't try it in `.py`). Works for capitalized user components too — `Link(to="/x", "click me")` → `createElement(Link, {to: "/x"}, "click me")`. See [`docs/language-reference.md`](docs/language-reference.md#psx-pythonic-jsx) for the full PSX section.

```bash
pyths compile counter.ps -o counter.js
```

## Features

- **Python syntax** — `def`, `class`, `if/elif/else`, `for/while`, `match/case`, list comprehensions, f-strings, decorators, generators, async/await
- **Python semantics, not JavaScript's** — `[] + []` is `[]` (not `""`), `if []:` is falsy, `[1,2] == [1,2]` is `True`, `7 % -3` is `-2`, `1 + "1"` is a type error, and `int` is **arbitrary-precision** (`2**53 + 1` is `9007199254740993`, exact — hybrid `Number`/`BigInt`). Every well-known JS footgun is closed at the codegen layer — your compiled code behaves like the Python you wrote. See [`technical_summary.md` §1.5](./technical_summary.md#15-javascript-semantics--python-faithful).
- **Python-flavored runtime errors** — `items[10]` raises `IndexError`, `d["missing"]` raises `KeyError`, `total // 0` raises `ZeroDivisionError`, all with CPython-matching message text. Source-mapped to `.ps` line+col. Run with `pyths run app.ps --explain` for a Python-style explanation paragraph above any crash. See [`technical_summary.md` §1.6](./technical_summary.md#16-errors--debugging--what-users-actually-see).
- **React & Next.js** — `@component`, `@psx` (helper functions returning JSX), all React hooks via generic `use_*` rule, async server components, Suspense, `use()` (React 19), `"use client"`/`"use server"` directives
- **Ecosystem libraries** — TanStack Query, React Router (+DOM), React Hook Form, Framer Motion, Zustand, Jotai, Recoil, XState, MobX, SWR all natively recognized; arbitrary npm packages resolve via the kebab-case fallback (`from foo_bar import x` → `import { x } from "foo-bar"`)
- **Type system** — full inference, `pyths check`, bundled `.pyi` stubs for React/Next.js/React-Router/TanStack Query so imports bind to declared `Callable`/class types at check time
- **TypeScript interop** — every module compiles with a `.d.ts` declaration **by default** (`--no-dts` to skip), so your typed `.ps` exports drop into any TypeScript/JS codebase with full types at the boundary — the same `.js` + `.d.ts` shape typed npm libraries ship in
- **`@dataclass`** — Auto-generates constructor with type validation, `toString()`, `__eq__()`, `toDict()`/`fromDict()`, frozen support; constructors accept both positional and `{kwargs}` forms
- **Standard library** — `math`, `json`, `itertools`, `functools`, `collections`, `random`, `datetime`, `re`, `decimal`, `fractions` (exact arithmetic: `Decimal('0.1') + Decimal('0.2') == Decimal('0.3')` is `True`, exactly like CPython — BigInt coefficient/exponent, not a float wrapper)
- **Web / edge modules** — `fetch`, `storage`, `router` (browser APIs); plus `pyths.web`'s `handler` / `Response` for a Cloudflare-Worker entry (`__default__ = handler(fetch)`)
- **Tooling** — `fmt`, `lint`, `test`, `bundle`, `cache` — a complete development workflow; Vite + Next.js plugins ship with **React Fast Refresh** (state-preserving HMR for `.ps`) and source maps
- **WASM auto-routing (the default)** — pure numeric functions are **automatically routed to WebAssembly** while DOM/React code stays JS; this is the no-flag `pyths compile` default (`--target js` pins JS-only). The universal glue runs in browsers, Cloudflare Workers, Deno, and Node; numeric bundles import the DOM-free `pyths-runtime/core` subpath. *(The Vite/Next plugins emit JS-only for now — carrying WASM sidecars through the bundler is on the roadmap.)*
- **Fast** — Rust-native compiler; ~124,000 lines/second; sub-millisecond compile times for typical files
- **Source maps** — `--sourcemap` for debugging in browser DevTools
- **Optional `.psc` compression** — opt-in compressed superset for AI-emitted code, **8.9% o200k / 9.3% cl100k additional token savings** on idiomatic code, on top of PythScribe's inherent reduction; `.ps` users see zero behavior change. See [`docs/compression.md`](docs/compression.md).
- **Tested** — **4,000+ automated checks across 12 layers**: 1,973 Rust unit/integration tests, a **1,376-entry** CPython semantic differential corpus (fully green, cross-checked on a second JS engine — 1,375/1,376 identical across V8 and JavaScriptCore), the 24 Livermore kernels × {cpython, js, wasm}, a grammar acceptor gate, tri-track clone DOM parity (React as oracle), browser pixel + DOM-bytecode parity, Node auto-routing E2E, panic-resistance fuzzing, machine-checked Lean proofs bound to the shipping compiler, and a per-compilation subscript-routing certificate. `cargo test --workspace`: **green, 0 failing**. Full per-layer counts are in the [assurance paper](https://doi.org/10.5281/zenodo.21875694).

> **Technical summary** — see [`technical_summary.md`](./technical_summary.md) for a 10-minute snapshot of where the project stands toward production parity with React + Next.js (gaps documented). Written for engineers, contributors, and anyone evaluating the toolchain.

## Assurance scope — what's *proved*, *tested*, and *trusted*

PythScribe is agent-written, and this README won't pretend otherwise. Every claim above sits in exactly one of three tiers, and the boundaries are published in [`TRUST.md`](./TRUST.md) — where each guarantee names its enforcement mechanism and the command that regenerates its evidence. The full write-up is the [assurance paper](https://doi.org/10.5281/zenodo.21875694).

| Tier | What's in it | How it's enforced |
|---|---|---|
| **Proved** (Lean 4, `verification/`) | `.psc`→`.ps` expansion (determinism, zone-safety, alias round-trip); subscript-routing read-safety plus a per-compilation certificate checker proved *sound and complete* against a model of the emitter; slice/index in-bounds safety; truthiness; `==` as an equivalence relation; WASM scratch non-interference; identifier-naming soundness (no Python identifier ever emits a bare JS reserved word); and statement/expression **preservation waves** over selected language fragments (arithmetic, bitwise, string methods, dict methods) | `lake build` + pinned `#print axioms`, in CI |
| **Tested** (oracle-diverse — what proof doesn't reach) | Runtime semantics vs **CPython**: a 1,376-entry differential corpus, fully green and cross-checked on a second JS engine (V8 + JavaScriptCore — 1,375/1,376 identical); 24 Livermore kernels ×{JS, WASM}; React-oracle DOM-parity tests; pixel/DOM parity; a grammar acceptor gate; 1,973 Rust tests; panic fuzzing | `cargo test --workspace` + the differential/parity CI jobs |
| **Trusted** (audited, not proved) | The `.ps`→JS/WASM codegen *body fragment*; the Rust byte-scanner's refinement of the proved Lean classifier; the type-inference evidence feeding routing certificates; the runtime JS; and the toolchain (rustc, the Lean kernel, Node, CPython-as-oracle) | Audited + differentially bound — the honest floor |

The value here is **oracle diversity and honest scope accounting**, not a claim of end-to-end proof. Proof covers selected fragments; everything else is held by the CPython differential and parity oracles; the rest is trusted and named as such.

## What doesn't work yet (honest scope)

The correctness story is scoped, not universal. Where PythScribe stops:

- **Floating-point is IEEE-754, not exact.** Only the *safe-integer* float domain (`|v| ≤ 2⁵³`) is proved exact. `0.1 + 0.2` is still `0.30000000000000004`, and transcendentals (`sin`, `exp`, …) follow the platform's IEEE-754 — exactly as in JavaScript. PythScribe fixes the missing *integer* type, not float rounding.
- **Preservation is proved over *fragments*, not the whole language.** The Lean waves cover selected fragments and are bound to the shipping compiler by differential testing, not proved end-to-end. Everything outside those fragments rests on the CPython corpus, not on proof.
- **The FFI boundary is untyped at runtime.** Values crossing in from the DOM, `JSON.parse`, or third-party JS are untyped and unchecked — PythScribe's semantics govern *your* code, not data handed to it by foreign JS.
- **Generic npm interop works, but is untyped.** Any npm package resolves (30+ recognized libraries plus a kebab-case fallback) and you write the real JS member names — but foreign libraries degrade to `unknown` at the type boundary (typed `.pyi` stubs via `stubgen` are in progress).
- **Some Python surface is deliberately narrower than CPython.** printf-style `%`-formatting raises `NotImplementedError` (use an f-string); a few stdlib method corners and language features remain partial. `technical_summary.md` tracks the gaps toward full React + Next.js parity.
- **WASM is for pure-numeric functions only.** Auto-routing sends only pure numeric functions to WebAssembly; anything with DOM access, effects, or objects stays on the JS path.

None of these are hidden in the codebase — they are the same boundaries the [assurance paper](https://doi.org/10.5281/zenodo.21875694) draws. Bugs in the trusted surface are expected; the append-only defect ledger and per-file regeneration commands are how they get absorbed in the open.

## PythScribe vs TypeScript

TypeScript checks types at compile time, then **erases them** — so JavaScript's *runtime* footguns survive it. PythScribe carries semantics into the runtime, so it closes classes TypeScript structurally can't:

| JavaScript defect | After TypeScript | After PythScribe |
|---|---|---|
| No integer type (precision past 2⁵³) | ✗ `number` is still IEEE-754 float | ✓ real arbitrary-precision `int` — `2**53 + 1` → `9007199254740993`, exact |
| Coercion (`[] + {}`, `1 + "1"`) | ◐ typed at compile time, **coerces at runtime** | ✓ `[] + []` is `[]`; `1 + "1"` is a type error, not `"11"` |
| Silent `undefined` / `NaN` | ◐ runtime values stay silent | ✓ fails loud — `xs[10]` → `IndexError`, `d["x"]` → `KeyError`, and **arithmetic on a missing value** `None * x` → `TypeError` (where JS/TS silently yield `NaN` and propagate it) |
| Automatic Semicolon Insertion | ✗ same JS rules | ✓ gone — Python syntax, no ASI |
| 32-bit bitwise truncation / lossy `**` | ✗ same JS (`-1 & x` wraps; `2**60` is a float) | ✓ infinite two's-complement bitwise; exact integer `**` |

**And it's not just the type system that stops at the boundary — TypeScript's does, and ours doesn't.** A TS interface is *erased*: `await res.json() as User` is a lie the compiler believes, `any` from one untyped import silently spreads, and every JS runtime footgun survives the type check. PythScribe carries semantics into the runtime — and the arithmetic cases (`//`, `%`, `**`, bitwise, integer precision) are **machine-proved in Lean**, not merely tested.

**Honestly:** PythScribe fixes the missing *integer* type, **not** floating-point rounding — `0.1 + 0.2` is still `0.30000000000000004` (that's IEEE-754, not a language defect); and TS's strict null-checking is genuinely good — we don't claim that one. It can't police the FFI boundary either: data from the DOM, `JSON.parse`, or third-party JS is untyped at runtime. The full, TS-literate breakdown — which of JavaScript's classic ten defects actually survive TypeScript, and where PythScribe does *not* differentiate — is in [**docs/pythscribe-vs-typescript.md**](docs/pythscribe-vs-typescript.md). *(Every behavioral claim above is exercised by the CPython differential corpus; the arithmetic cases are additionally Lean-proved.)*

## `.psc`: an LLM-Oriented IR

In the framing of Amarasinghe's PLDI'26 keynote, `.psc` is a **LOIR** (a compressed, model-facing surface) and `.ps` is the **Source-of-Record Language** humans review — joined by a deterministic derivation (`pyths expand --verify`) whose core rewrite properties (determinism, protected-zone safety, alias round-trip) are **machine-checked in Lean 4** (`verification/`, 0 `sorry`), with the shipping zone classifier additionally exercised by bounded model checking (Kani; see [`KANI.md`](KANI.md)).

**Storage tokens:** `.psc` reduces stored tokens by **8.9% (o200k) / 9.3% (cl100k)** over canonical `.ps` on idiomatic code and **1.5%** on faithful React ports. Many visually shorter aliases yield no benefit under fixed BPE tokenizers — a negative result we call the **BPE wall**.

**Generation tokens** (does an LLM emit fewer tokens writing `.psc`?): zero-shot, at component scale, the `.psc` condition produces lower paired code-block token counts than `.ps` on **69 of 72** model–task pairs across **eight models from three vendors** (per-model median **7–25%**; pooled **20.1%**, 95% CI 14.9–25.2%). A decomposition attributes **3–5 percentage points** directly to the compressed representation; the remainder reflects prompt-associated structural economy.

Full study, methodology, and reproducibility artifact — *"A Compressed Model-Facing Source Layer with Partially Verified Expansion"*: **https://doi.org/10.5281/zenodo.21386779**. The five-requirement LOIR analysis is in [`docs/loir.md`](docs/loir.md); the tier reference is [`docs/compression.md`](docs/compression.md).

## Installation

Everything installs from npm — no Rust toolchain required (installs `@latest` by default; all packages share one version).

### Add PythScribe to a project

```bash
npm install pythscribe
```

**That one package brings the whole chain** — the `pyths` compiler (with your platform's prebuilt native binary), the `pyths-runtime`, and **both** framework plugins (`vite-plugin-pyths` and `next-plugin-pyths`) — so a **React / Vite _or_ Next.js** project is ready to compile `.ps` files out of the box. The plugins declare `vite`/`next` as *optional* peers, so you pull in only whichever framework you actually use — no extra install, no peer warnings.

```bash
pyths --version
```

### Scaffold a new app (fastest)

```bash
npm create pyths-app@latest my-app
cd my-app && npm install && npm run dev
```

This scaffolds a ready-to-run **Next.js** + PythScribe app — compiler, runtime, and plugin already wired up.

**Prefer your own base?** Scaffold with the framework's own tool, then add PythScribe and wire its plugin:

```bash
# React + Vite  (see docs/getting-started-with-vite.md)
npm create vite@latest my-app
cd my-app && npm install pythscribe

# Next.js  (see docs/getting-started-with-next.md)
npx create-next-app@latest my-app
cd my-app && npm install pythscribe
```

### Standalone scripts

`pyths run app.ps` needs only the compiler — the runtime is built into the CLI, which pulls the right binary for your platform (Linux, macOS, Windows — x64 and arm64) as an optional dependency:

```bash
npm install -g pythscribe
```

### Build from source (alternative)

```bash
git clone https://github.com/swetmr/pythscribe.git
cd pythscribe
cargo build --release   # binary at target/release/pyths (requires Rust 1.70+)
```

Then `npm link` the `runtime/` and `packages/*` folders into your project instead of installing from npm.

## Quick Start

### 1. Hello World

```python
# hello.ps
print("Hello, World!")
```

```bash
pyths run hello.ps
# → Hello, World!
```

### 2. Classes and f-strings

```python
# todo.ps
class Todo:
    def __init__(self, title, done):
        self.title = title
        self.done = done

    def __str__(self):
        status = "x" if self.done else " "
        return f"[{status}] {self.title}"

    def toggle(self):
        self.done = not self.done

todos = [
    Todo("Learn PythScribe", False),
    Todo("Build a web app", False),
]

todos[0].toggle()

for todo in todos:
    print(todo)

pending = [t for t in todos if not t.done]
print(f"{len(pending)} items remaining")
```

```bash
pyths compile todo.ps -o todo.js
node todo.js
# → [x] Learn PythScribe
# → [ ] Build a web app
# → 1 items remaining
```

### 3. Dataclasses

```python
# user.ps
from dataclasses import dataclass, field

@dataclass
class User:
    name: str
    age: int
    email: str = ""

user = User("Alice", 30, "alice@example.com")
print(user)
print(user.toDict())
```

Generates constructor with type validation, `toString()`, `__eq__()`, `toDict()`/`fromDict()` — all from a single decorator.

### 4. React Component with PSX

```python
# app.ps
"use client"

from pyths.react import component, use_state

@component
def TodoApp():
    todos, set_todos = use_state([])
    text, set_text = use_state("")

    def add():
        if text:
            set_todos([*todos, {"text": text, "done": False}])
            set_text("")

    return div(class_name="app",
        h1("Todo List"),
        input(value=text, on_change=lambda e: set_text(e.target.value)),
        button(on_click=lambda: add(), "Add"),
        ul(*[li(t["text"]) for t in todos]),
    )
```

## CLI Reference

```
pyths <command> [options]

Commands:
  compile   Compile a .ps file to JavaScript (or WASM with --target)
  check     Type-check a .ps file without compiling
  run       Compile and run a .ps file using Node.js
  init      Initialize a new PythScribe project
  test      Run PythScribe test files
  fmt       Format PythScribe source files
  lint      Lint PythScribe files for common issues
  bundle    Bundle a PythScribe project into a single JS file

Global flags:
  --quiet     Suppress non-error output
  --verbose   Show verbose output
```

### `pyths compile`

```bash
pyths compile app.ps                    # → app.js + app.d.ts (automatic routing:
                                        #   a numeric kernel also emits app.wasm + app.glue.js)
pyths compile app.ps --target js        # pin JS-only (no WASM sidecars)
pyths compile app.ps --no-dts           # skip the .d.ts declaration
pyths compile app.ps -o dist/app.js     # custom output path
pyths compile app.ps --stdout           # print JS to stdout (single module, no sidecars)
pyths compile app.ps --sourcemap        # emit app.js.map
pyths compile app.ps --timings          # show per-phase timing
pyths compile app.ps --target wasm      # compile numeric functions to .wasm only
pyths compile app.ps --target js+wasm   # explicit JS + WASM (the default for kernel modules)
```

### `pyths check`

```bash
pyths check app.ps                      # type-check without compiling
```

### `pyths run`

```bash
pyths run hello.ps                      # compile + execute via Node.js
pyths run hello.ps --explain            # add a Python-style explanation
                                        # paragraph above any crash trace
```

### `pyths fmt`

```bash
pyths fmt src/                          # format all .ps files
pyths fmt app.ps --check                # check formatting (CI mode)
pyths fmt app.ps --indent 2             # use 2-space indent
```

### `pyths lint`

```bash
pyths lint src/                         # lint all .ps files
pyths lint app.ps                       # lint single file
```

Rules: `W001` unused variable, `W002` unused import, `W003` unreachable code, `W004` naming convention, `W005` unnecessary pass, `W006` mutable default argument.

### `pyths test`

```bash
pyths test                              # discover and run test_*.ps files
pyths test tests/ --verbose             # run tests in directory
```

### `pyths bundle`

```bash
pyths bundle app.ps                     # → app.bundle.js
pyths bundle app.ps -o dist/app.js      # custom output
pyths bundle app.ps --minify            # minified output
```

## Framework Integration

### Vite

`vite-plugin-pyths` already ships with `npm install pythscribe` — no extra install. *(From-source checkout only: `npm link pyths-runtime vite-plugin-pyths`.)*

```js
// vite.config.js
import pyths from 'vite-plugin-pyths';

export default {
  plugins: [pyths()]
};
```

### Next.js

`next-plugin-pyths` already ships with `npm install pythscribe` — no extra install. *(From-source checkout only: `npm link pyths-runtime next-plugin-pyths`.)*

```js
// next.config.mjs
import withPythScribe from "next-plugin-pyths";

export default withPythScribe({
  // your Next.js config
});
```

Multi-file projects — how relative imports between `.ps` files work, how
each plugin resolves `.ps`/`.psc` modules (including dual-track `.tsx`
siblings and Next.js client islands), and a verified three-file
walkthrough — are covered in
[`docs/multi-file-apps.md`](docs/multi-file-apps.md).

## Python-to-JS Mapping

| Python | JavaScript |
|--------|-----------|
| `def f(x):` | `function f(x) {` |
| `class Foo:` | `class Foo {` |
| `self.x` | `this.x` |
| `__init__` | `constructor` |
| `print(x)` | `console.log(x)` |
| `len(x)` | `pyLen(x)` |
| `range(n)` | `pyRange(n)` |
| `a // b` | `Math.floor(a / b)` |
| `a ** b` | `a ** b` |
| `a % b` | `((a % b) + b) % b` (Python-sign modulo) |
| `[1,2] + [3,4]` | `[...a, ...b]` (Python concat, not string coerce) |
| `[1,2] == [1,2]` | `pyEq(a, b)` (element-wise, not reference) |
| `if []:` | `if (pyBool(x))` (collection-empty is falsy) |
| `f"hello {x}"` | `` `hello ${x}` `` |
| `[x for x in items]` | `items.map((x) => x)` |
| `[x for x in items if p]` | `items.filter((x) => p).map((x) => x)` |
| `lambda x: x + 1` | `(x) => x + 1` |
| `True / False / None` | `true / false / null` |
| `not / and / or` | `! / && / \|\|` |
| `x if cond else y` | `cond ? x : y` |
| `match x:` | `if/else chain` |
| `yield x` | `yield x` (generator) |
| `@dataclass` | Auto-generated class with constructor, validation, serialization |
| `@component` | `memo(function ...)` with JSX output |
| `s.strip()`, `xs.append(x)` | Python builtin methods → JS equivalent (`.trim()`, `.push(x)`) |
| `e.preventDefault()` | Verbatim — native JS/DOM/library methods pass through unchanged |

**Member access is verbatim.** snake→camel conversion applies only to
React import names (`use_state` → `useState`) and JSX props (`on_click` →
`onClick`). It does **not** rename `obj.method(...)` calls. Python builtin
methods (str/list/dict/set) are lowered to their JS equivalent, but native
JS/DOM/library methods have no Python analog and are emitted as-is — so write
the real API name (`e.preventDefault()`, `el.addEventListener(...)`,
`query.invalidateQueries(...)`). There is no working snake_case form for them.

## Project Structure

```
pyths/
├── crates/
│   ├── pyths_lexer/       # Tokenization (logos)
│   ├── pyths_syntax/      # AST definitions
│   ├── pyths_parser/      # Recursive descent parser
│   ├── pyths_codegen_js/  # JavaScript code generation
│   ├── pyths_diagnostic/  # Error reporting (ariadne)
│   ├── pyths_resolve/     # Name resolution (LEGB scopes)
│   ├── pyths_types/       # Type checker
│   ├── pyths_cli/         # CLI binary (clap)
│   ├── pyths_runtime/     # Runtime crate wrapper
│   ├── pyths_hir/         # WASM eligibility analysis
│   └── pyths_codegen_wasm/# WASM code generation (wasm-encoder)
├── runtime/                  # JS runtime library (npm)
│   ├── src/stdlib/           # Python stdlib modules
│   └── src/web/              # Web API wrappers
├── packages/
│   ├── vite-plugin-pyths/ # Vite build plugin
│   ├── next-plugin-pyths/ # Next.js webpack plugin
│   └── create-pyths-app/  # Project scaffolding
├── examples/                 # Example projects
├── tests/fixtures/           # Test fixture files
└── docs/                     # Documentation
```

## Development

```bash
# Rust workspace — compiler, parser, type checker, WASM codegen, CLI (1,973 tests)
cargo test --workspace

# Specific crate
cargo test -p pyths_codegen_js
cargo test -p pyths_codegen_wasm

# Node-side runtime tests (~79 — helpers 44, Worker-safe `core` 19, `web` 8, bigint 8)
node --test crates/pyths_runtime/js/runtime.test.js
node --test runtime/src/core.test.mjs runtime/src/web.test.mjs runtime/src/operators.bigint.test.mjs

# decimal / fractions stdlib unit tests (27)
node --test runtime/src/stdlib/decimal.test.mjs runtime/src/stdlib/fractions.test.mjs

# Format-spec differential vs CPython (~30; requires `python` on PATH)
node --test crates/pyths_runtime/js/format_diff_test.mjs

# CPython semantic differential corpus (1,376 entries, fully green vs CPython 3.12; requires `python` on PATH)
node tests/differential/run.mjs

# Promise / async-JS interop suite (43, pinned expectations — CPython cannot oracle raw Promises)
node tests/jsinterop/run.mjs

# Promise-interop behavioral specs (4, vitest + jsdom via examples/clones deps)
node tests/jsinterop/behavioral/run.mjs

# Library-interop behavioral parity (28 — Radix Dialog/DropdownMenu/Checkbox, cva+clsx+tailwind-merge,
# lucide-react, react-hook-form, @tanstack/react-query, framer-motion; TSX oracle vs .ps twin)
node tests/libinterop/run.mjs

# Auto-routing E2E (~8) — proves JS for DOM, WASM for compute end-to-end
node --test tests/differential/auto_route_test.mjs

# Playwright DOM + pixel parity (28 — fixture-level + generative fuzz)
cd tests/e2e && npm install && npx playwright install chromium && npx playwright test

# Benchmarks
cargo bench -p pyths_codegen_js

# Release build
cargo build --release
```

Total: **4,000+ automated checks across 12 layers** (1,973 cargo + 1,376 differential + 24 Livermore ×3 + clone-parity + pixel/DOM parity + acceptor corpus + Lean proofs + certificate corpus). CI runs them on every push (`.github/workflows/ci.yml`), including the Lean `verification` job and the tri-track `clones` job; the panic-resistance fuzz harness lives in `crates/pyths_cli/tests/fuzz_inputs.rs`. A separate weekly fuzz cron (`.github/workflows/fuzz.yml`) runs coverage-guided `cargo-fuzz` targets from `fuzz/`.

The full assurance study behind these layers — oracle-diverse testing, per-compilation routing certificates, machine-checked Lean verification, and explicit trust accounting — is written up in *"Layered Assurance for an Agent-Written Python-to-JavaScript/WebAssembly Compiler"*: **https://doi.org/10.5281/zenodo.21875694**.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, coding standards, and contribution guidelines.

## License

PythScribe is **source-available** under the **Functional Source License, Version 1.1 (Apache-2.0 Future License)** — `FSL-1.1-ALv2`. See [`LICENSE.md`](./LICENSE.md) for the full text.

- ✅ **You may** use, copy, modify, self-host, and redistribute PythScribe for almost any purpose — including commercial use, internal tooling, production deployments, research, and building your own products on top of it.
- 🚫 **You may not** put it to a **Competing Use** — i.e. make PythScribe (or a substantially similar substitute built from it) available to others as a commercial product or service that competes with PythScribe.
- ⏳ Each released version additionally becomes **Apache-2.0** on the **second anniversary** of that version's release.

The FSL is a source-available license, not an OSI-approved open-source license. Full text and FAQ: <https://fsl.software>.
