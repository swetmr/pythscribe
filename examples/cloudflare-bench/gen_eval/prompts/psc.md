# Condition: psc (compressed PythScribe)

Solve the task in **compressed PythScribe** (`.psc`) — write the compressed form
directly. `.psc` is PythScribe plus a deterministic expansion layer: compression aliases
expand to canonical `.ps` source before compilation, so everything in the PythScribe
manual below applies unchanged. Use the compression tiers wherever they save output
tokens; anything not covered by an alias is written as ordinary PythScribe.

Output ONLY a fenced code block containing the complete `.psc` program:

```python
# your .psc program
```

No prose before or after the code block.

---

## PythScribe in one paragraph

PythScribe uses strict Python 3 syntax — indentation blocks, no semicolons, no braces.
Write it like Python. `print(...)` behaves like Python's print (Python-style repr for
lists/dicts/tuples/bools/None: `[1, 2, 3]`, `{'a': 1}`, `True`, `None`). Python operator
semantics are preserved: `//` floors toward negative infinity, `%` takes the sign of the
divisor, `**` is exponentiation, f-strings support format specs (`{x:>10}`, `{v:.2f}`,
`{n:05d}`, `{p:.1%}`, `{'hi':*^8}`).

## Core conventions

1. **snake_case everything — except native browser/DOM/JS-library methods.** Your own
   identifiers, React props (`on_click`), and hooks (`use_state`) are snake_case; the
   compiler converts to camelCase where the JS ecosystem needs it. Member-access calls
   into native JS/DOM APIs are written verbatim (`e.preventDefault()`,
   `localStorage.getItem(...)`) — there is no snake_case spelling that works. Python
   builtin methods stay Python-style and are lowered for you (`s.strip()`,
   `xs.append(x)`, `d.get(k)`).
2. **PSX flat call form is the default** (React section below): props as kwargs, children
   as positional args after them — `div(class_name="card", h2("Title"))`. Never use the
   curried `tag(props)(children)` form.

## Language essentials (all Python-standard)

```python
x = 42
a, b = 1, 2                       # destructuring
name: str = "Alice"               # annotations allowed

def greet(name, greeting="Hello"):
    return f"{greeting}, {name}!"

square = lambda x: x ** 2

class Animal:
    def __init__(self, name, sound):
        self.name = name          # self.x -> this.x, __init__ -> constructor
        self.sound = sound
    def speak(self):
        return f"{self.name} says {self.sound}"

class Dog(Animal):                # inheritance works; Dog("Rex") -> new Dog("Rex")
    def __init__(self, name):
        self.name = name
        self.sound = "woof"
```

Control flow (`if`/`elif`/`else`, `for ... in`, `while`, `break`/`continue`),
`try`/`except`/`finally`, `raise`, custom exception classes (`class E(Exception): pass`),
comprehensions (list/dict/set + generator expressions), generators (`yield`), `assert`,
and `match`/`case` (literal, sequence `[x, y]`, and wildcard `_` patterns) all work as in
Python.

## Built-ins and stdlib

Built-ins behave like Python: `print`, `len`, `range`, `enumerate` (with optional start),
`zip`, `sorted` (with `key=`, stable), `reversed`, `sum`/`min`/`max`/`abs`/`round`
(banker's rounding like Python), `any`/`all`, `map`/`filter`, `str`/`int`/`float`/`bool`/
`list`/`dict`/`set`/`tuple`, `isinstance`, `repr`. String/list/dict/set methods are the
Python ones: `.split`, `.join`, `.strip`, `.replace`, `.upper`, `.lower`, `.title`,
`.startswith`, `.count`, `.find`, `.append`, `.extend`, `.insert`, `.pop`, `.items`,
`.keys`, `.values`, `.get`, `.most_common`, set operators `| & - ^`, slicing with
negative indices and steps (`s[::-1]`, `xs[2:7]`, `xs[::3]`).

Standard-library modules are imported with Python `from` imports:

```python
from collections import Counter, deque, OrderedDict
from itertools import chain, islice, product, permutations, combinations, accumulate, groupby
from decimal import Decimal
from fractions import Fraction
import math                        # math.sqrt, math.floor, math.gcd, ...
import json                        # json.dumps / json.loads
import re                          # re.search, re.findall, re.sub
```

`Counter`, `Decimal('0.1') + Decimal('0.2')`, `Fraction(2, 4)` etc. print exactly like
CPython.

## Gotchas (real, will bite)

- **Never pass a bare builtin as a value.** `sorted(ws, key=len)` and
  `defaultdict(list)` fail at runtime (builtins are lowered only in call position).
  Wrap in a lambda: `sorted(ws, key=lambda w: len(w))`, `defaultdict(lambda: [])`.
- **`match` guards on capture patterns** (`case s if s > 100:`) are unreliable — use
  plain `if`/`elif` when you need guards; literal/sequence/wildcard cases are fine.
- **Uppercase call = `new`**: `Dog("Rex")` compiles to `new Dog("Rex")`. Name classes in
  CapWords and functions in snake_case, as in idiomatic Python, and this never bites.
- Standalone scripts run under Node: there is no `input()`/file I/O for these tasks —
  just compute and `print`.

## PythScribe extensions (beyond Python — optional)

```python
value = x ?? default              # nullish coalescing
name = obj?.user?.name            # optional chaining
result = data |> parse |> render  # pipeline: render(parse(data))
merged = {**defaults, "key": v}   # dict spread (also standard Python)
button(on_click=cb, "Click")      # positional args AFTER kwargs (PSX flat form)
```

## React components (for UI tasks)

Inside a `@component`-decorated function, HTML elements are function calls — no JSX, no
angle brackets. Flat form: props as kwargs first, children as positional args.

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

    def toggle(i):
        updated = [t for t in todos]
        updated[i] = {**updated[i], "done": not updated[i]["done"]}
        set_todos(updated)

    pending = len([t for t in todos if not t["done"]])

    return div(class_name="todo-app",
        h2("Todo List"),
        input(value=text, on_change=lambda e: set_text(e.target.value),
              placeholder="New todo..."),
        button(on_click=lambda: add(), "Add"),
        ul(*[li(key=i, on_click=lambda: toggle(i), t["text"])
             for i, t in enumerate(todos)]),
        p(f"{pending} remaining"),
    )
```

Conventions:

- Hooks are snake_case: `use_state`, `use_effect`, `use_ref`, `use_memo`,
  `use_callback`. `count, set_count = use_state(0)`.
- Props are snake_case and auto-camelize: `class_name`, `on_click`, `on_change`,
  `on_submit`, `on_key_down`, `tab_index`, `html_for`. Data/aria attrs auto-kebab:
  `data_testid="x"` → `data-testid`. Plain attrs pass through verbatim: `type`, `id`,
  `value`, `href`, `src`, `alt`, `placeholder`, `disabled`, `key`.
- Conditional child: `cond and p("shown")` (beware falsy `[]`/`""`/`0` — use a ternary
  `El(...) if cond else None` when the guard can be an empty collection).
- Child lists: spread a comprehension — `ul(*[li(key=x["id"], x["name"]) for x in xs])`.
  Always give list children a `key=`.
- **Per-item event handlers: write `on_click=lambda: f(i)`, never `lambda i=i: f(i)`.** The
  comprehension binds `i` per row and a zero-arg handler ignores the DOM event; `lambda i=i:`
  compiles to `(i=i) => f(i)` and React passes the event as arg 1, overriding `i` (silent
  misfire). Non-comprehension capture: a factory `def mk(i): return lambda: f(i)`.
- **A helper that returns elements must be `@component`- (or `@psx`-) decorated.** A bare
  `def Row(x): return section(...)` leaves `section(...)` untransformed and crashes
  (`section is not defined`). Only decorated functions turn `tag(...)` into an element.
- Inline style only when unavoidable, as a quoted dict literal:
  `style={"width": f"{pct}%"}` (keys are CSS property names in snake_case or quoted
  camelCase; prefer `class_name` + class names otherwise).
- Event handlers: DOM event objects are native JS — `e.target.value`,
  `e.preventDefault()` (verbatim camelCase).
- Multiple root elements: return a tuple.
- Put `"use client"` as the literal first line of an interactive component file.

---

# The `.psc` compression layer

Everything below expands deterministically to the canonical PythScribe above; the
expander never substitutes inside string literals, f-strings, or comments. Only the
bundled tiers listed here are available — do NOT use `%NAME` Tier-E idioms (they require
a project `pyths.toml`, which these tasks do not have). Unknown aliases pass through and
break compilation, so use exactly the tables below.

## Tier A — import presets and decorator aliases (whole-line / `@` slot)

| Write | Expands to |
|---|---|
| `R*` | `from pyths.react import component, use_state, use_effect, use_callback, use_memo` |
| `R+` | same plus `forwardRef, memo, Suspense, lazy` |
| `T*` | `from dataclasses import dataclass` |
| `T+` | `from dataclasses import dataclass, Field` |
| `A*` | `from pyths.asyncio import gather, sleep` |
| `D*` | `from pyths.dom import query, query_all, get_element_by_id, set_text, get_text, add_event_listener` |
| `@c` | `@component` |
| `@d` | `@dataclass` (call form works: `@d(coerce=True)`) |
| `@v` | `@validator` |

A preset marker occupies a whole line, e.g. the first line of a component file after
`"use client"` is just `R*`.

## Tier B — kwarg aliases and hook shorthand (call-argument position only)

| Write | Expands to | | Write | Expands to |
|---|---|---|---|---|
| `cn=` | `class_name=` | | `us(` | `use_state(` |
| `st=` | `style=` | | `ue(` | `use_effect(` |
| `oc=` | `on_click=` | | `um(` | `use_memo(` |
| `oh=` | `on_change=` | | `uc(` | `use_callback(` |
| `os=` | `on_submit=` | | `ur(` | `use_ref(` |
| `oa=` | `on_blur=` | | `ux(` | `use_context(` |
| `ph=` | `placeholder=` | | | |
| `dis=` | `disabled=` | | | |

Hook shorthand requires the following `(` — a bare `us` identifier is left alone.

## Tier Dictionary — `$NAME` string-literal aliases (bundled table only)

| Write | Expands to |
|---|---|
| `$c1` | `"#9ca3af"` |
| `$c2` | `"#ffffff"` |
| `$c4` | `"#3b82f6"` |
| `$p1` | `"12px"` |
| `$p4` | `"16px"` |
| `$pad` | `"padding"` |
| `$bg` | `"background"` |
| `$ff` | `"system-ui, sans-serif"` |
| `$gtc` | `"grid_template_columns"` |

Use a `$NAME` anywhere the canonical string literal would appear. Do not invent new
`$NAME`s — unknown aliases pass through verbatim and fail to parse.

## Example — compressed component

```python
"use client"
R*

@c
def Greeting():
    name, set_name = us("world")
    return div(cn="greeting",
        h1(f"Hello, {name}"),
        button(oc=lambda: set_name("PythScribe"), "Switch"),
    )
```

For plain (non-UI) programs most savings come from ordinary tight PythScribe; Tiers A/B/C
and the dictionary mainly pay off in component code. Do not force aliases where none
apply.
