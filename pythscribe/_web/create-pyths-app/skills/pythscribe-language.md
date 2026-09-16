---
name: pythscribe-language
description: Use when authoring or editing PythScribe `.ps` / `.psc` files — Pythonic frontend code that compiles to React/JS/WASM. Triggers on a `.ps`/`.psc` file, the words "PythScribe" or "pyths", writing PSX components/hooks in Python syntax, or porting React/TSX to `.ps`. Not for ordinary Python, `.py`, or TypeScript/JSX files.
---

# PythScribe — Language Skill for AI Agents

> **Write Python. Ship to the Web.**

PythScribe is a Pythonic frontend language (file extension `.ps`) that compiles to JavaScript and WebAssembly via a Rust toolchain. It uses strict Python syntax — no semicolons, no braces, no angle brackets — and integrates natively with React, Next.js, and Vite. Pure numeric functions are automatically eligible for WASM compilation, delivering near-native compute performance.

**CLI binary**: `pyths`

---

## Core conventions (read first — these two defaults cover almost all `.ps` authoring)

Apply both unless a later section explicitly says otherwise.

1. **snake_case everything — except browser/DOM/library methods.**
   Write *all your own* identifiers in `snake_case`: variables, functions, React
   props (`on_click`), hooks (`use_state`), Next.js exports (`get_static_props`).
   The compiler auto-converts to camelCase wherever the JS ecosystem needs it.
   The **only** exception is *member-access calls into native browser / DOM / JS-library
   APIs and globals* — write those **verbatim** in their real name (usually camelCase):
   `e.preventDefault()`, `el.addEventListener(...)`, `navigator.clipboard.writeText(...)`,
   `localStorage.getItem(...)`, `window.scrollTo(...)`. There is **no** snake_case
   spelling of these that works. (Python builtin methods stay Python-style and are
   lowered for you: `s.strip()`, `xs.append(x)`, `d.get(k)`.) → details: §"Calling JS / DOM / library methods".

2. **Nested (flat) call form is THE default — never reach for curry.**
   Write elements as `tag(prop=v, child1, child2)` — props as kwargs, children
   **nested** as positional args — for HTML elements and capitalized components
   alike: `div(class_name="card", h2("Title"), Button(on_click=f, "Go"))`. Do
   **not** use the curried `tag(props)(children)` form; it compiles identically but
   is non-default and harder to scan. → details: §"Call forms".

```python
# ✅ default: snake_case identifiers + nested/flat form; browser methods verbatim
def on_submit(e):
    e.preventDefault()                       # browser method → verbatim
    navigator.clipboard.writeText(draft)     # browser global → verbatim
return div(class_name="card",
    h2(user["name"]),
    button(on_click=lambda: set_open(not open), "Toggle"),
)
# ❌ avoid: curried call form, camelCase identifiers, snake-cased browser methods
return div(class_name="card")(                 # curry — don't
    button(onClick=toggle)("Toggle"))          # camelCase prop + curry — don't
# e.prevent_default()  ← compiles literally and throws at runtime
```

3. **HTML-tag names are RESERVED element intrinsics — never name your own identifiers after them.**
   Lowercase HTML/React element tags (`div`, `span`, `pre`, `img`, `a`, `p`, `ul`, `li`,
   `table`, `input`, `form`, `button`, …) are lowered to `createElement("tag", …)` **only
   inside `@psx` and `@component`** (both admit elements; a Capitalized name inside PSX is a
   user *component*, never an intrinsic). **Do not** define a function, variable, or import
   with a lowercase HTML-tag name anywhere — it collides with the intrinsic. **Guideline:**
   author a PSX-returning helper with **`@psx`** (lighter than `@component`); use a
   **Capitalized** name for a component; otherwise pick a **distinct** name
   (`make_div`, `div_box`). Using an element tag *outside* `@psx`/`@component` is not valid
   PSX — decorate the function. (The compiler must diagnose both collisions; never rely on
   the silent behavior.)

```python
# ✅ a PSX-returning helper is decorated @psx — elements are only admitted inside @psx/@component
@psx
def field_row(label, child):
    return div(class_name="row", span(label), child)   # intrinsics OK here

# ❌ never name your own identifier after an HTML tag
def div(x):                 # collides with the <div> intrinsic
    return wrap(x)          # inside a component this is silently shadowed by <div>;
                            # in a plain (undecorated) helper, div(...) is undefined at runtime
```

---

## Installation

PythScribe is not yet published to npm. Install from source using `npm link`.

### 1. Build the compiler (requires Rust 1.70+)

```bash
git clone https://github.com/pyths-lang/pyths.git
cd pyths
cargo build --release
```

Add the binary to your PATH:

```bash
# Linux/macOS — add to ~/.bashrc or ~/.zshrc
export PATH="$PATH:/path/to/pyths/target/release"

# Windows — add to PATH via System Environment Variables
# target\release\pyths.exe
```

### 2. Link the runtime and plugins

From the PythScribe repo root, link each package globally:

```bash
# Link the runtime (required for any project using builtins like len, range, etc.)
cd runtime
npm link

# Link the Vite plugin (for Vite/React projects)
cd ../packages/vite-plugin-pyths
npm link

# Link the Next.js plugin (for Next.js projects)
cd ../packages/next-plugin-pyths
npm link
```

Then in your project, link the packages you need:

```bash
cd /path/to/your-project

# For any project using PythScribe builtins/stdlib
npm link pyths-runtime

# For Vite projects
npm link vite-plugin-pyths

# For Next.js projects
npm link next-plugin-pyths
```

---

## Quickstart: Add a `.ps` File to an Existing Project

### 1. Standalone script (no framework)

```python
# hello.ps
def greet(name):
    return f"Hello, {name}!"

print(greet("world"))
```

```bash
pyths compile hello.ps        # → hello.js
pyths run hello.ps            # compile + execute via Node.js
```

### 2. Add to an existing Vite/React project

```bash
npm link pyths-runtime vite-plugin-pyths
```

```js
// vite.config.js
import pyths from 'vite-plugin-pyths';
export default { plugins: [pyths()] };
```

Now create `.ps` files alongside your `.jsx`/`.tsx` files — Vite compiles them automatically:

```python
# src/components/Counter.ps
from pyths.react import component, use_state

@component
def Counter():
    count, set_count = use_state(0)
    return div(class_name="counter",
        h1(f"Count: {count}"),
        button(on_click=lambda: set_count(count + 1), "+"),
        button(on_click=lambda: set_count(count - 1), "-"),
    )
```

Import from JS/TS files:

```js
// src/App.jsx
import { Counter } from './components/Counter.ps';
```

### 3. Add to an existing Next.js project

```bash
npm link pyths-runtime next-plugin-pyths
```

```js
// next.config.mjs
import withPyths from "next-plugin-pyths";
export default withPyths({ /* your Next.js config */ });
```

```python
# app/page.ps
"use client"

from pyths.react import component, use_state

@component
def Page():
    count, set_count = use_state(0)
    return main(
        h1("PythScribe + Next.js"),
        button(on_click=lambda: set_count(count + 1), f"Clicked {count}"),
    )
```

### 4. Scaffold a new project from scratch

```bash
npx create-pyths-app my-app
cd my-app && npm link pyths-runtime && npm install && npm run dev
```

---

## CLI Reference

```bash
pyths compile <file.ps>                  # → file.js + file.d.ts (auto-routes numeric kernels to .wasm + glue)
pyths compile app.ps -o dist/app.js      # custom output path
pyths compile app.ps --target js         # pin JS-only (no WASM sidecars)
pyths compile app.ps --no-dts            # skip the default .d.ts declaration
pyths compile app.ps --stdout            # print JS to stdout (single module, no sidecars)
pyths compile app.ps --sourcemap         # emit .js.map
pyths compile app.ps --dts               # emit .d.ts (default-on; explicit form)
pyths compile app.ps --timings           # print per-phase timing
pyths compile app.ps --target wasm       # compile numeric functions to .wasm only
pyths compile app.ps --target js+wasm    # explicit JS + WASM (the default for kernel modules)

pyths check <file.ps>                    # type-check only (no output)
pyths run <file.ps>                      # compile + execute via Node.js
pyths init [name]                        # initialize new project

pyths test [path] [--verbose]            # discover and run test_*.ps files
pyths fmt <path> [--check] [--indent N]  # auto-format .ps files
pyths lint <path>                        # lint for common issues
pyths bundle app.ps [-o out.js] [--minify]  # bundle for production
```

---

## Language Syntax

### Variables

```python
x = 42
name: str = "Alice"
a, b = 1, 2                  # destructuring → const [a, b] = [1, 2]
```

### Functions

```python
def add(a: int, b: int) -> int:
    return a + b

def greet(name, greeting="Hello"):
    return f"{greeting}, {name}!"

def variadic(*args, **kwargs):
    print(args, kwargs)

square = lambda x: x ** 2
```

### Classes

```python
class Animal:
    def __init__(self, name, sound):
        self.name = name
        self.sound = sound

    def speak(self):
        return f"{self.name} says {self.sound}"

class Dog(Animal):
    def __init__(self, name):
        self.name = name
        self.sound = "woof"

dog = Dog("Rex")              # uppercase → new Dog("Rex")
```

### Dataclasses

Auto-generates constructor (with type validation), `toString()`, `__eq__()`, `toDict()`, `fromDict()`:

```python
from dataclasses import dataclass, field, Field

@dataclass
class User:
    name: str
    age: int = 0
    email: str = Field(pattern=r".*@.*")
    tags: list = field(default_factory=list)

    @validator
    def validate(self):
        if self.age < 0:
            raise ValueError("Age must be non-negative")

u = User("Alice", 30, "alice@example.com")
print(u)                      # User(name=Alice, age=30, ...)
print(u.toDict())             # {"name": "Alice", "age": 30, ...}
copy = User.fromDict(u.toDict())
```

**Field constraints**: `gt`, `ge`, `lt`, `le`, `min_length`, `max_length`, `pattern`, `default_factory`.

**Frozen**: `@dataclass(frozen=True)` — prevents mutation after construction.

### Control Flow

```python
if x > 0:
    print("positive")
elif x == 0:
    print("zero")
else:
    print("negative")

for item in items:
    print(item)

for i in range(10):
    print(i)

for i, item in enumerate(items):
    print(i, item)

while condition:
    process()
```

### Pattern Matching

```python
match command:
    case "quit":
        print("Goodbye")
    case ["greet", name]:
        print(f"Hello, {name}!")
    case x if x < 0:
        return "negative"
    case _:
        print("Unknown")
```

### Exception Handling

```python
try:
    result = parse(data)
except ValueError as e:
    print(f"Error: {e}")
finally:
    cleanup()

raise ValueError("invalid input")
assert x > 0, "x must be positive"
```

### Comprehensions

```python
doubled = [n * 2 for n in numbers]
evens = [n for n in numbers if n % 2 == 0]
mapping = {k: v * 2 for k, v in items}
total = sum(x ** 2 for x in range(10))
```

### Generators

```python
def fibonacci():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b
```

### Async/Await

```python
async def fetch_data(url):
    response = await fetch(url)
    return await response.json()
```

### PythScribe Extensions (beyond Python)

```python
# Nullish coalescing
value = x ?? default_value

# Optional chaining
name = obj?.user?.name
item = arr?.[0]
result = func?.(arg)

# Pipeline operator
result = data |> parse |> transform |> render
# → render(transform(parse(data)))

# Kwarg shorthand
func(=name)                   # same as func(name=name)

# Dict spread
merged = {**defaults, "key": value}

# Positional args allowed AFTER keyword args (parser relaxation)
# Standard Python rejects this with SyntaxError; PythScribe allows it
# specifically so PSX flat form `tag(prop=v, child1, child2)` works.
button(on_click=cb, "Click me")
```

---

## WASM Compilation

PythScribe compiles eligible **numeric-kernel** functions to WebAssembly for near-native performance. Eligibility is auto-detected (no annotation needed), and emission is **automatic by default**: with no `--target`, `pyths compile` uses automatic routing (`js+wasm` semantics) — a module with a numeric kernel emits `.js` + `.wasm` + `.glue.js`, and a module with none degrades to a single `.js`. Pin the axis with `--target`: `js` (pure JavaScript, no WASM), `wasm` (`.wasm` only), `js+wasm` (explicit JS + WASM), `wasm-edge` (Cloudflare Workers), `wasi`, `deno`. (The Vite/Next bundler plugins pin `--target js` for now; WASM-through-the-bundler is a follow-up.) A user-written `@wasm` is a **strict assertion** — it hard-errors if the function cannot be admitted, or if combined with an explicit JS-only `--target`.

### Eligible functions — the numeric-kernel whitelist

WASM eligibility is **sound**: only functions the backend compiles correctly are admitted (a numeric-kernel whitelist — default-reject; everything outside it is correct JS, never a WASM miscompile). A function is a WASM numeric kernel when ALL of:
- **Parameters** are numeric scalars (`int` / `float` / `bool`) or a flat list of them (`list[int]` / `list[float]` / `list[bool]`). No `str`, `dict`, `set`, `tuple`, nested-list, or untyped params.
- **Return** is a numeric scalar (`int` / `float` / `bool`) or void. (A value-returning function with no return annotation stays JS; non-scalar returns stay JS.)
- **Body** uses only: scalar/`list`-literal values, arithmetic / comparison / boolean / bitwise ops, `if`/`elif`/`else`, `while`, `for … in range(…)` or `for … in <list param>`, list **indexing** (`a[i]`) + subscript store (`a[i] = …`) + `len`, the ternary `a if c else b`, `abs`, `int`/`float` casts, `math.*` functions, and calls to other eligible kernels.
- **NOT eligible (→ JS):** strings & string methods; slicing; `set()`/`dict`/`tuple`/comprehensions; `sorted`/`min`/`max`/`sum`/`enumerate`/`zip`/`map`/`filter`/`reversed`; list mutation (`.append`/`.pop`/…); tuple-unpacking; `lambda`/closures; `async`/`yield`; classes; decorators other than `@wasm`; `*args`/`**kwargs`; a call to any function that is not itself an eligible kernel.

**Why the whitelist:** the WASM backend miscompiled general string/list/dict code under earlier (broader) admission — a correctness bug (#364). Restricting WASM to the proven-correct numeric-kernel subset keeps the `--target js+wasm` path sound. Widening it back (correct WASM for strings/lists/dicts) is the v3.x path (#364), gated by the K9 admitted-shape 3-way audit.

### Example

```python
# compute.ps
def fibonacci(n: int) -> int:
    if n <= 1:
        return n
    a: int = 0
    b: int = 1
    i: int = 2
    while i <= n:
        temp: int = a + b
        a = b
        b = temp
        i += 1
    return b

def gcd(a: int, b: int) -> int:
    while b != 0:
        temp: int = b
        b = a % b
        a = temp
    return a

def dot(a: list[float], b: list[float]) -> float:   # eligible — list[float] via linear memory
    total: float = 0.0
    for i in range(len(a)):
        total += a[i] * b[i]
    return total
```

```bash
pyths compile compute.ps --target wasm      # → compute.wasm (all three functions)
pyths compile compute.ps --target js+wasm   # → compute.js + compute.wasm + compute.glue.js
pyths compile compute.ps --target wasm --verbose  # shows eligible/rejected with reasons
```

### Type mapping

| PythScribe | WASM type |
|------------|-----------|
| `int` | `i64` |
| `float` | `f64` |
| `bool` | `i32` |
| `list` / `dict` / `set` / `tuple` of eligible types | linear memory |

Python-correct semantics are preserved: floor division rounds toward negative infinity, and modulo follows Python's sign convention.

---

## React / PSX

Inside `@component` functions, HTML elements are called as functions (no angle brackets, no JSX). The same applies to capitalized user components — they route through `createElement(Component, ...)` (not `new Component(...)`).

### The PSX boundary — element/React lowering is gated to `@component` / `@psx`

PSX element lowering happens **only inside a `@component` or `@psx` function**. Everywhere else — module level, plain undecorated `def`s, and `.py` files — a call is **pure Python**: `div("hi")` is just a function call (`div("hi")`), not `createElement("div", …)`; `map(xs)` is the `map` builtin, not `<map>`. So React/HTML logic lives strictly at the `@component`/`@psx` boundary; the rest is ordinary Python.

Inside that boundary, a bare `Name(...)` call resolves in this precedence:

1. **Known classes** — a local `class`, **or any CapWords name imported from a stdlib module** (`Counter`, `OrderedDict`, `ChainMap`, `Decimal`, `Fraction`, …) → `new X(...)` (constructor). So stdlib containers construct correctly inside a component.
2. **JS built-in constructors** (`Map`, `URL`, `EventSource`, `Date`, …) → `new X(...)`.
3. **React imports** (capitalized: `Fragment`, `Suspense`, a component import) → `createElement(X, ...)`.
4. **HTML/SVG element name** (or an unbound tag-shaped name) → `createElement("tag", ...)`.

### DESIGN RULE — builtin ∩ HTML/SVG-element name collision: **HTML wins inside a component**

A few names are *both* a Python builtin *and* an HTML/SVG element. Inside a `@component`/`@psx`, these lower as the **element**, not the builtin — HTML wins the collision:

| Name | Inside a component | Note |
|---|---|---|
| `map` | `<map>` element | use a list comprehension `[f(x) for x in xs]` instead of `map(f, xs)` |
| `input` | `<input>` element | `input()` (stdin) is meaningless in a browser component |
| `object` | `<object>` element | `object()` is rarely needed in a component |

That is the **entire** collision set — the common data-builtins **`filter` / `set` / `list` / `dict` / `zip` / `sorted` are *not* element names**, so they keep their normal builtin behavior everywhere. Outside a `@component`/`@psx`, all of `map`/`input`/`object` are the ordinary Python builtins. (Rationale: `<input>`/`<map>`/`<object>` are the overwhelmingly common in-component use; the colliding builtins have clean alternatives.)

### Component signatures — named params are PROP NAMES

`@component` named parameters declare **props by name** and destructure the flat
props object React passes — at every arity. To receive the **whole props
object**, use `**props` (or, as a pragmatic alias, a single no-default param
literally named `props`).

```python
@component
def Frontier(data):        # ONE prop named `data` — `data` is the prop VALUE
    return div(data["points"])

@component
def Card(title, on_close=None):   # props `title` + optional `on_close`
    ...

@component
def Wrapper(**props):      # the WHOLE props object (dict-style access)
    return div(props["label"])
```

| Signature | Compiles to | Meaning |
|---|---|---|
| `def C():` | `function C()` | no props |
| `def C(data):` | `function C({data} = {})` | one prop named `data` (any arity-1 name except literal `props`) |
| `def C(title, on_click):` | `function C({title, on_click} = {})` | props by name |
| `def C(count=0):` | `function C({count = 0} = {})` | optional prop (defaulted params always destructure — even one named `props`) |
| `def C(title, **rest):` | `function C({title, ...rest} = {})` | named props + the rest of the props object |
| `def C(**props):` | `function C(props = {})` | **whole props object** (documented form) |
| `def C(props):` | `function C(props)` | whole props object (alias: single no-default param literally named `props`) |

Do not name a *prop* `props` — an arity-1 `def C(props):` is always the
whole-object alias (use `**props` or rename the prop if that bites).

### Call forms — nested/flat is the default

All forms compile to the same `createElement(tag, props, ...children)`, so the
choice is purely stylistic — and the **default is nested/flat**. Use it everywhere
unless you have a specific reason not to.

```python
# ✅ DEFAULT — nested/flat: props as kwargs, children nested as positional args
div(class_name="card", "Title", span("body"))

# (accepted, NOT default) Direct — a single positional child, no props
div("Just text")

# ❌ AVOID by default — curried: splits props and children into two calls.
# Identical JS, harder to scan. Only for an unusually deep tree where it truly helps.
div(class_name="card")("Title", span("body"))
```

The nested/flat form is the documented default and works for both HTML elements AND capitalized user components (`Link(to="/x", "click me")`, `Suspense(fallback=p("Loading..."), Children(...))`, etc.). It relies on the parser allowing positional args after keyword args — a deliberate relaxation; see the §"PythScribe Extensions (beyond Python)" note above. **Do not reach for the curried form as a default** — it reads as two calls and adds nothing.

### Example

```python
from pyths.react import component, use_state, use_effect

@component
def TodoApp():
    todos, set_todos = use_state([])
    text, set_text = use_state("")

    def add():
        if text:
            set_todos([*todos, {"text": text, "done": False}])
            set_text("")

    def toggle(index):
        updated = [t for t in todos]
        updated[index] = {**updated[index], "done": not updated[index]["done"]}
        set_todos(updated)

    pending = len([t for t in todos if not t["done"]])

    return div(
        h2("Todo List"),
        div(
            input(value=text, on_change=lambda e: set_text(e.target.value),
                placeholder="New todo..."),
            button(on_click=lambda: add(), "Add"),
        ),
        ul(*[li(todo["text"], on_click=lambda: toggle(i))
             for i, todo in enumerate(todos)]),
        p(f"{pending} remaining"),
    )
```

### PSX prop conventions

Snake_case props auto-convert to camelCase:

| PythScribe | JavaScript |
|------------|-----------|
| `class_name` | `className` |
| `on_click` | `onClick` |
| `on_change` | `onChange` |
| `on_submit` | `onSubmit` |
| `on_key_down` | `onKeyDown` |
| `tab_index` | `tabIndex` |
| `html_for` | `htmlFor` |

Hyphenated data/aria attributes are written as snake_case kwargs and auto-kebab:

| PythScribe | JavaScript |
|------------|-----------|
| `data_testid` | `data-testid` |
| `data_agent_id` | `data-agent-id` |
| `aria_label` | `aria-label` |

A conditional attribute that should be omitted resolves its value to `None`
(`data_current="true" if active else None`) — don't reach for a `**{...}` spread.

Plain HTML attributes that need no conversion are written verbatim as kwargs:
`type`, `id`, `name`, `role`, `value`, `href`, `src`, `alt`, `placeholder`, etc.
`type="submit"` is fine even though `type` is a Python builtin — as a keyword-argument
*name* it is just an attribute and passes through unchanged.

### PSX gotchas (two that bite React authors)

**Per-item event handlers: write `on_click=lambda: f(i)`, never `on_click=lambda i=i: f(i)`.**
A comprehension already binds `i` fresh per row (it compiles to `xs.map((i) => …)`), and a
zero-argument handler compiles to `() => f(i)`, which correctly *ignores* the DOM event.
The Python loop-capture idiom `lambda i=i: f(i)` compiles to `(i = i) => f(i)`, and because
JSX/PSX handlers are invoked with the event as the first argument, the event overrides the
default and `i` becomes the event object, not the index — a silent misfire that compiles
cleanly. (For capture in a *non-comprehension* loop, use a factory: `def mk(i): return lambda: f(i)`.)

**A helper that returns elements must be decorated `@component` (or `@psx` for a small
inline fragment).** Only `@component`/`@psx` functions turn PSX call-form (`section(...)`,
`div(...)`) into `createElement(...)`. A bare `def Row(x): return section(...)` leaves
`section(...)` as an undefined call and crashes at runtime (`section is not defined`).

### Token-efficient style (defaults that keep `.ps` lean)

`.ps` should compile to fewer tokens than the equivalent React+TypeScript. A
naive style erodes that to break-even; the defaults below reliably hold a
**~10% cl100k saving on real application code** (data models + validation +
logic + UI), and more after `.psc` compression. Every form was verified by
compiling with `pyths` and reading the emitted JS. Use them by default:

| Topic | ✅ Do | ❌ Avoid | Why |
|---|---|---|---|
| **Styling** | `class_name="card"` + a stylesheet | inline styles when avoidable | 9 tok, parity with TSX; wins on tokens **and** bundle |
| **Inline style** (unavoidable) | quoted dict literal `style={"k": v}` | `style=dict(k=v)` · `style=a_var` | literal → snake→camel at compile time, **no** runtime wrapper. The others emit `pyNormalizeStyle(PyDict({…}))` (~+50% JS) |
| **PSX call form** | flat `tag(prop=v, child)` | curried `tag(props)(child)` | identical JS, fewer source chars |
| **Hyphenated attrs** | `data_testid="x"` kwarg (auto-kebabs) | `**{"data-testid": "x"}` spread | the spread keeps the heavier quoted key |
| **Conditional child** | `cond and El(...)` | `El(...) if cond else None` (when there's no else branch) | emits idiomatic `cond && …`. Beware Python-falsy values (`[]`, `""`, `0`) — `xs and El(...)` renders `El` when `xs == []`; keep the ternary when the guard can be an empty collection |
| **Child lists** | `*[El(x) for x in xs]` | imperative `.append()` loops | comprehension + spread, no temp variable |
| **Text** | f-strings `f"{a}-{b}"` | concatenation / multi-arg text children | native template literal, one node |
| **Data models + validation** | `@dataclass` + `Field(...)` | hand-rolled `__init__` / manual `validate_*` | auto-generated validation; the single biggest structural saving |

Two caveats worth stating plainly:

- **`style=dict(...)` is a trap.** It looks like fewer source tokens than a
  quoted dict literal, but compiles to `pyNormalizeStyle(PyDict({…}))` — heavier
  JS. Prefer `class_name`; if you must inline, use a **quoted dict literal**.
- **The ~10% floor is for real app code.** A *pure-presentational* component
  forced to use inline styles sits near parity even when optimized — the inline
  style dicts dominate and cancel on both tracks. The structural wins
  (`class_name`, comprehensions, `@dataclass`/`Field`) are what carry a real app
  past 10%. Token delta is codebase-dependent: typed / logic-heavy code trends
  to ~−15%, inline-style-heavy presentational code toward parity. Quote the
  range and its driver, not a flat number.

### Calling JS / DOM / library methods — write the real name (usually camelCase)

snake→camel auto-conversion applies in exactly **two** places: React-ecosystem
**import names** (`use_state` → `useState`) and **JSX prop names** (`on_click`
→ `onClick`). It does **not** touch `obj.method(...)` member access — the
compiler can't know `e` is a DOM event. Two cases:

- **Python builtin methods** (str / list / dict / set) are *lowered* to their
  JS equivalent — write them Python-style: `s.strip()`, `xs.append(x)`,
  `name.upper()`, `d.get(k)`.
- **Native JS / DOM / browser / library methods and globals** have no Python analog
  and pass through **verbatim** — write the real API name, camelCase and all. This
  includes browser **globals** (`navigator`, `localStorage`, `window`, `document`)
  and their methods:

```python
def on_submit(e):
    e.preventDefault()          # NOT e.prevent_default() — native DOM, verbatim
    e.stopPropagation()
    value = e.target.value
    el.addEventListener("click", cb)
    navigator.clipboard.writeText(value)           # browser global, verbatim
    localStorage.setItem("draft", value)           # browser global, verbatim
    window.scrollTo(0, 0)                          # browser global, verbatim
    qc.invalidateQueries({"queryKey": ["runs"]})   # library method, verbatim
```

There is **no** snake_case spelling of `preventDefault` that works — writing
`e.prevent_default()` compiles to `e.prevent_default()` and throws at runtime.
Member access on a value is always emitted as-is (after builtin-method
lowering).

### Fragments

Return a tuple for multiple root elements:

```python
@component
def Multi():
    return (
        h1("Title"),
        p("Content"),
    )
```

### All React hooks (snake_case)

```python
use_state(initial)
use_effect(fn, deps)
use_context(ctx)
use_ref(initial)
use_memo(fn, deps)
use_callback(fn, deps)
use_reducer(reducer, init)
use_layout_effect(fn, deps)
use_id()
use_transition()
use_deferred_value(val)
```

### Redux

```python
from reduxjs.toolkit import create_slice, configure_store
from react_redux import use_selector, use_dispatch

counter_slice = create_slice(
    name="counter",
    initial_state={"value": 0},
    reducers={
        "increment": lambda state: state.value + 1,
        "decrement": lambda state: state.value - 1,
    }
)

store = configure_store(reducer={"counter": counter_slice.reducer})

@component
def Counter():
    count = use_selector(lambda state: state.counter.value)
    dispatch = use_dispatch()
    return button(on_click=lambda: dispatch(increment()), f"Count: {count}")
```

### Next.js specifics

```python
"use client"                  # client component directive (first line)

# Server-side functions (snake_case auto-converts)
def get_server_side_props(context):
    return {"props": {"title": "My Page"}}

def get_static_props():
    return {"props": {"data": []}}
```

---

## Imports

### PythScribe standard library

```python
from pyths import math, json, itertools, functools, collections, random, datetime, re
```

### Web modules

```python
from pyths.fetch import get, post, put, patch, delete_
from pyths.storage import local, session, cookies
from pyths.router import route, navigate, start
from pyths.utils.tenacity import retry
from pyths.dom import query, query_all, create_element, add_event_listener
```

### React

```python
from pyths.react import component, use_state, use_effect, use_ref, use_memo
```

### Suppressed imports (no JS output)

These are handled at compile time and produce no `import` statement in JS:

```python
from dataclasses import dataclass, field, Field
from typing import Optional, List, Dict, Union, Any, Callable
from pydantic import BaseModel
```

### NPM module mapping

```python
import react_redux            # → import ... from "react-redux"
import reduxjs.toolkit         # → import ... from "@reduxjs/toolkit"
```

---

## Built-in Functions

| PythScribe | Compiles to | Return type |
|------------|-------------|-------------|
| `print(*args)` | `console.log(...)` | None |
| `len(x)` | `pyLen(x)` | int |
| `range(stop)` / `range(start, stop, step)` | `pyRange(...)` | Iterator |
| `enumerate(x)` | `pyEnumerate(x)` | Iterator |
| `zip(a, b)` | `pyZip(a, b)` | Iterator |
| `sorted(x)` | `pySorted(x)` | list |
| `reversed(x)` | `pyReversed(x)` | Iterator |
| `isinstance(x, T)` | `x instanceof T` | bool |
| `str(x)` | `String(x)` | str |
| `int(x)` | `Math.trunc(Number(x))` | int |
| `float(x)` | `Number(x)` | float |
| `bool(x)` | `pyBool(x)` | bool |
| `list(x)` | `Array.from(x)` | list |
| `abs(x)` | `Math.abs(x)` | number |
| `min(...)` / `max(...)` | `Math.min(...)` / `Math.max(...)` | number |
| `sum(x)` | `x.reduce((a, b) => a + b, 0)` | number |
| `round(x)` | `Math.round(x)` | int |
| `map(f, x)` / `filter(f, x)` | `x.map(f)` / `x.filter(f)` | list |
| `any(x)` / `all(x)` | `x.some(Boolean)` / `x.every(Boolean)` | bool |
| `repr(x)` | `JSON.stringify(x)` | str |
| `format(v, spec)` / `slice(...)` / `ascii(x)` / `vars(obj)` | runtime helpers (CPython semantics) | — |

Unimplemented builtins (`open`, `input`, `eval`, `exec`, `hash`, `id`,
`globals`, `locals`, `memoryview`, …) are a **compile error** naming the
builtin — never a silent runtime `ReferenceError`. Use JS interop
(`window.prompt`, `fetch`, `node:fs`) for the environment-shaped ones.

---

## Operator Semantics

| Operator | PythScribe | JavaScript output | Notes |
|----------|-----------|-------------------|-------|
| `//` | `a // b` | `Math.floor(a / b)` | Floor division |
| `%` | `a % b` | `((a % b) + b) % b` | Python-correct modulo (differs for negatives) |
| `**` | `a ** b` | `a ** b` | Exponentiation |
| `not` | `not x` | `!x` | |
| `and` / `or` | `x and y` | `x && y` | |
| `is` / `is not` | `x is None` | `x === null` | |
| `in` | `x in y` | Uses runtime helper | |
| `??` | `a ?? b` | `a ?? b` | Nullish coalescing |
| `\|>` | `x \|> f` | `f(x)` | Pipeline |

---

## Standard Library Quick Reference

### math
```python
from pyths import math
math.pi, math.e, math.sqrt(x), math.sin(x), math.cos(x), math.floor(x),
math.ceil(x), math.log(x), math.pow(x, y), math.factorial(n), math.gcd(a, b)
```

### json
```python
from pyths import json
text = json.dumps(obj, indent=2)
data = json.loads(text)
```

### itertools
```python
from pyths import itertools
itertools.chain(a, b), itertools.islice(it, n), itertools.zip_longest(a, b),
itertools.product(a, b), itertools.permutations(it), itertools.combinations(it, r),
itertools.groupby(it, key), itertools.accumulate(it), itertools.count(start)
```

### functools
```python
from pyths import functools
functools.reduce(fn, it, init), functools.partial(fn, arg),
functools.lru_cache(maxsize=128), functools.cache(fn)
```

### collections
```python
from pyths import collections
c = collections.Counter(items)
dd = collections.defaultdict(list)
dq = collections.deque([1, 2, 3])
od = collections.OrderedDict()
Point = collections.namedtuple("Point", ["x", "y"])
```

### datetime
```python
from pyths.datetime import date, time, datetime, timedelta
today = date.today()
now = datetime.now()
d = timedelta(days=5)
```

### re
```python
from pyths import re
m = re.search(r"\d+", text)
all_matches = re.findall(r"\w+", text)
result = re.sub(r"\s+", " ", text)
```

### fetch (async)
```python
from pyths.fetch import get, post

async def load():
    resp = await get("https://api.example.com/data")
    resp.raise_for_status()
    return await resp.json()

async def send(payload):
    resp = await post("https://api.example.com/data",
        json=payload, headers={"Authorization": "Bearer TOKEN"})
    return await resp.json()
```

### storage
```python
from pyths.storage import local, session, cookies
local.set("key", {"data": 1})
val = local.get("key")
cookies.set("theme", "dark", days=30)
```

### router
```python
from pyths.router import route, navigate, start
route("/", home_handler)
route("/users/:id", user_handler)
start()
navigate("/users/42")
```

### tenacity (retry)
```python
from pyths.utils.tenacity import retry

@retry(max_attempts=5, delay=1.0, backoff="exponential")
async def fetch_with_retry(url):
    response = await get(url)
    response.raise_for_status()
    return await response.json()
```

---

## DOM API (vanilla, no React)

```python
from pyths.dom import query, query_all, create_element, get_element_by_id
from pyths.dom import set_text, get_text, set_html, add_event_listener
from pyths.dom import set_attribute, add_class, remove_class, toggle_class

el = query("#app")
set_text(el, "Hello")
add_event_listener(el, "click", lambda e: print("clicked"))
```

---

## Type Annotations

PythScribe supports Python type annotations for type checking (`pyths check`) and `.d.ts` generation (`--dts`):

```python
def add(a: int, b: int) -> int:
    return a + b

count: int = 0
items: List[int] = [1, 2, 3]
mapping: Dict[str, int] = {"a": 1}
maybe: Optional[str] = None
callback: Callable[[int], str] = str
```

---

## Key Rules and Gotchas

1. **Uppercase = `new`**: `Dog("Rex")` → `new Dog("Rex")`, `process(data)` → `process(data)` (lowercase, no `new`).

2. **`self` → `this`**: In class methods, `self.x` becomes `this.x` and `__init__` becomes `constructor`.

3. **No angle brackets**: `.ps` files never use `<div>` syntax. PSX uses function calls; the default **flat form** is `tag(prop=v, child1, child2)` — works for HTML elements AND capitalized user components alike. Curried (`tag(prop=v)(children)`) and direct (`tag(child)`) forms are also accepted; all compile identically. The flat form requires PythScribe's positional-after-keyword parser relaxation (see §"PythScribe Extensions").

4. **Suppressed imports**: `dataclasses`, `typing`, `pydantic` imports produce no JS — they're compile-time only.

5. **Snake → camel**: React props (`on_click` → `onClick`), hooks (`use_state` → `useState`), and Next.js exports (`get_static_props` → `getStaticProps`) auto-convert. **But `obj.method()` member access does not** — Python builtin methods are lowered (`s.strip()`), native JS/DOM/library methods are verbatim (`e.preventDefault()`). No snake_case form of `preventDefault` works.

6. **Python-correct modulo**: `-5 % 3` produces `1` (Python semantics), not `-2` (JS default).

7. **Runtime dependency**: Compiled code that uses builtins like `len()`, `range()`, `enumerate()` requires `pyths-runtime` at runtime.

8. **Indentation-based**: Like Python, blocks are defined by indentation. The compiler injects INDENT/DEDENT tokens.

9. **`@component` enables PSX**: HTML element function calls (div, span, h1, etc.) are only transformed inside `@component`-decorated functions.

10. **String literals**: Use `f"..."` for template literals. Regular `"..."` strings compile to JS string literals.

---

## Complete Example: React App with Data Fetching

```python
# UserList.ps
"use client"

from pyths.react import component, use_state, use_effect

@dataclass
class User:
    id: int
    name: str
    email: str

@component
def UserList():
    users, set_users = use_state([])
    loading, set_loading = use_state(True)
    error, set_error = use_state(None)

    async def load_users():
        try:
            response = await fetch("https://jsonplaceholder.typicode.com/users")
            data = await response.json()
            set_users(data)
        except Exception as e:
            set_error(str(e))
        finally:
            set_loading(False)

    use_effect(lambda: load_users(), [])

    if loading:
        return p("Loading...")

    if error:
        return p(f"Error: {error}", class_name="error")

    return div(class_name="user-list",
        h1(f"Users ({len(users)})"),
        ul(*[
            li(key=user["id"],
                strong(user["name"]),
                span(f" - {user['email']}"),
            )
            for user in users
        ]),
    )
```

---

## Testing PythScribe Code

```python
# test_utils.ps
def test_add():
    assert 1 + 1 == 2

def test_greeting():
    result = greet("world")
    assert result == "Hello, world!"

def test_list_ops():
    items = [1, 2, 3]
    assert len(items) == 3
    assert sum(items) == 6
```

```bash
pyths test                     # discover and run test_*.ps files
pyths test tests/ --verbose    # run with verbose output
```

---

## Project Structure Convention

```
my-app/
  app/                         # Next.js app directory
    layout.ps
    page.ps
  components/
    Header.ps
    Counter.ps
    TodoList.ps
  lib/
    utils.ps
    api.ps
  pyths.toml                   # (if using pyths init)
  next.config.mjs              # or vite.config.js
  package.json
```

For Vite projects:

```
my-app/
  src/
    main.ps                    # entry point
    App.ps
    components/
      Counter.ps
  vite.config.js
  package.json
```
