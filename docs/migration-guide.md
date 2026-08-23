# Migration Guide: Python to PythScribe

This guide covers what works in PythScribe, what doesn't, and the key differences from standard Python.

## What PythScribe Is

PythScribe (`.ps`) uses Python syntax but compiles to JavaScript. It is **not** a Python interpreter — it's a compiler that maps Python constructs to their JavaScript equivalents. The output is standard JS that runs in browsers, Node.js, or any JS runtime.

## What Works

### Core Language

| Feature | Status | Notes |
|---------|--------|-------|
| Variables and assignment | Full | `let` on first assign, plain reassign after |
| `def` functions | Full | → `function` |
| `class` with `__init__`, methods | Full | `self` → `this`, `__init__` → `constructor` |
| `if/elif/else` | Full | → `if/else if/else` |
| `for` loops | Full | → `for...of` |
| `while` loops | Full | Direct mapping |
| `return`, `break`, `continue` | Full | Direct mapping |
| `try/except/finally` | Full | → `try/catch/finally` |
| `with` statement | Full | → `try/finally` pattern |
| `match/case` | Full | → `if/else` chain |
| List comprehensions | Full | → `.filter().map()` |
| Dict comprehensions | Full | → `Object.fromEntries()` |
| Lambda expressions | Full | → arrow functions |
| f-strings | Full | → template literals |
| Triple-quoted strings | Full | → backtick strings |
| Walrus operator `:=` | Full | → assignment expression |
| Generators (`yield`) | Full | → `function*` |
| `async/await` | Full | Direct mapping |
| Decorators | Full | `@dataclass`, `@component`, custom |
| Type annotations | Full | Parsed, stripped in JS output |
| `*args, **kwargs` | Full | → spread/rest |
| Default arguments | Full | Direct mapping |
| Ternary (`x if c else y`) | Full | → `c ? x : y` |

### Operators

| Python | JavaScript | Notes |
|--------|-----------|-------|
| `+`, `-`, `*`, `/` | Same | Direct mapping |
| `//` (floor div) | `Math.floor(a / b)` | Integer division |
| `%` (modulo) | `((a % b) + b) % b` | Python-correct modulo for negative numbers |
| `**` (power) | `**` | ES2016 exponentiation |
| `==`, `!=` | `===`, `!==` | Strict equality |
| `not`, `and`, `or` | `!`, `&&`, `\|\|` | Direct mapping |
| `in` | `.includes()` or `in` | Context-dependent |
| `is` | `===` | Reference equality |
| `is not` | `!==` | Reference inequality |

### Built-in Functions

| Python | JavaScript | Notes |
|--------|-----------|-------|
| `print(x)` | `console.log(x)` | Supports multiple arguments |
| `len(x)` | `pyLen(x)` | Runtime helper |
| `range(n)` | `pyRange(n)` | Runtime helper, supports 1-3 args |
| `int(x)` | `parseInt(x)` | |
| `float(x)` | `parseFloat(x)` | |
| `str(x)` | `String(x)` | |
| `bool(x)` | `Boolean(x)` | |
| `list(x)` | `Array.from(x)` | |
| `dict(x)` | `Object.fromEntries(x)` | |
| `isinstance(x, T)` | `x instanceof T` | |
| `type(x)` | `typeof x` | |
| `abs(x)` | `Math.abs(x)` | |
| `min(a, b)` | `Math.min(a, b)` | |
| `max(a, b)` | `Math.max(a, b)` | |
| `round(x)` | `Math.round(x)` | |
| `sorted(x)` | `[...x].sort()` | |
| `reversed(x)` | `[...x].reverse()` | |
| `enumerate(x)` | `x.entries()` | |
| `zip(a, b)` | `pyZip(a, b)` | Runtime helper |
| `map(f, x)` | `x.map(f)` | |
| `filter(f, x)` | `x.filter(f)` | |
| `input(prompt)` | *compile error* — not supported yet; call `window.prompt` via JS interop | |
| `sum(x)` | `x.reduce((a, b) => a + b, 0)` | |
| `any(x)` | `x.some(Boolean)` | |
| `all(x)` | `x.every(Boolean)` | |

## What Doesn't Work

### Not Supported

| Feature | Reason |
|---------|--------|
| Multiple inheritance | JS classes only support single inheritance |
| Metaclasses | No JS equivalent |
| `__slots__` | No JS equivalent |
| `global`/`nonlocal` keywords | Scope model is different (uses `let`) |
| Complex `*args` unpacking patterns | Partial support only |
| `exec()` / `eval()` | Security concern; not compiled |
| Arbitrary-precision integers | JS numbers are 64-bit float; use `BigInt` manually |
| Context managers (custom `__enter__`/`__exit__`) | `with` compiles to try/finally; no protocol dispatch |
| `__getattr__`/`__setattr__` magic | No Proxy-based emulation |
| Threading/multiprocessing | Use Web Workers via JS interop instead |
| File I/O (`open()`) | Not applicable in browser; use `fetch` for network I/O |

### Behavioral Differences

**Integer division**: Python `7 / 2 = 3.5` works the same. But Python `int` type doesn't exist — all numbers are JS `Number` (64-bit float). Very large integers (> 2^53) will lose precision.

**Equality** (Python-faithful for collections): primitive `==` compiles to `===` (matches Python for int/float/bool/str/None). Collection equality (`==` on lists, dicts, sets, tuples) routes through the `pyEq` runtime helper for element-wise comparison — `[1, 2] == [1, 2]` is `True` exactly like Python, not `false` as raw JS reference equality would give you.

**Truthiness** (Python-faithful for collections): `if []:`, `if {}:`, `if set():` are all falsy — matching Python — when the test expression is a known collection (literal or annotated). The codegen wraps in `pyBool()` for those cases. Primitive tests (`if 0:`, `if "":`, `if None:`) stay bare since JS truthiness already matches Python for primitives.

**Runtime errors** (Python-flavored): `items[10]` on a typed list raises `IndexError`, `d["missing"]` on a typed dict raises `KeyError`, `a // 0` raises `ZeroDivisionError` — all with CPython-matching message text. See [`language-reference.md`](./language-reference.md#python-named-runtime-errors).

**`None.attribute`**: Python raises `AttributeError`; PythScribe inherits JS's `TypeError: Cannot read properties of null`. The type checker catches most `Optional[T]` cases at compile time. To get a Python-flavored hint paragraph when running locally, use `pyths run app.ps --explain`.

**Import system**: Python imports resolve to:
- Standard library → `pyths-runtime/stdlib/<module>`
- `pyths.*` → `pyths-runtime/[web/]<module>`
- `dataclasses`, `pydantic`, `typing` → suppressed (handled at compile time)
- Everything else → passed through as-is (assumed to be npm packages)

## Key Patterns

### Class instantiation

PythScribe automatically adds `new` for class instantiation when the name starts with an uppercase letter:

```python
# PythScribe
user = User("Alice", 30)

# Compiled JS
let user = new User("Alice", 30);
```

### `self` → `this`

```python
# PythScribe
class Counter:
    def __init__(self, start):
        self.value = start

    def increment(self):
        self.value += 1

# Compiled JS
class Counter {
    constructor(start) {
        this.value = start;
    }
    increment() {
        this.value += 1;
    }
}
```

### Suppressed imports

These Python imports are recognized and suppressed (their features are handled at compile time):

```python
from dataclasses import dataclass, field      # suppressed
from pydantic import BaseModel                # suppressed
from typing import Optional, List, Dict       # suppressed
```

### Type annotations

Type annotations are parsed and used for:
- Type checking via `pyths check`
- `.d.ts` generation via `--dts`

They are **stripped** from the JavaScript output:

```python
# PythScribe
def add(a: int, b: int) -> int:
    return a + b

# Compiled JS (no type info)
function add(a, b) {
    return (a + b);
}
```

## React/Next.js Migration

### Hooks

PythScribe uses snake_case for hooks, which auto-converts to camelCase:

```python
# PythScribe                          # JavaScript
use_state(0)                        # useState(0)
use_effect(fn, [dep])               # useEffect(fn, [dep])
use_ref(None)                       # useRef(null)
use_memo(fn, [dep])                 # useMemo(fn, [dep])
use_callback(fn, [dep])             # useCallback(fn, [dep])
```

### Props

Snake_case props auto-convert (HTML/React names) or kebab-case (ARIA / data):

```python
# PythScribe                # JavaScript
class_name="app"             # className: "app"
on_click=handler             # onClick: handler
tab_index=0                  # tabIndex: 0
default_value="hi"           # defaultValue: "hi"
aria_label="Close"           # "aria-label": "Close"
data_test_id="root"          # "data-test-id": "root"
```

### PSX (Pythonic JSX)

PSX is **pure Python syntax** — no angle brackets. Inside `@component` (or `@psx`) functions, HTML element calls become `React.createElement(...)`:

```python
@component
def App():
    return div(class_name="app",
        h1("Hello"),
        p(f"Count: {count}"),
    )
```

The default form nests props and children in one call: `tag(prop=v, …, child, …)`. PythScribe's parser permits positional args after keyword args (CPython doesn't); the codegen separates kwargs → props from positional → children regardless of order. Two equivalent forms are also accepted:

```python
# All four forms compile to the same createElement tree:
div(class_name="x", h2("Hello"))     # nested (default) — concise, most Pythonic
div(h2("Hello"))                      # direct — single positional arg, no props
div(class_name="x")(h2("Hello"))      # curried — visual separation of props/children
div()(h2("Hello"))                    # empty-props curried — uniform shape
```

JSX-equivalent compiled output:
```jsx
<div className="x"><h2>Hello</h2></div>
```

See [`language-reference.md`](./language-reference.md#psx-pythonic-jsx) for the full PSX section — fragments, conditional rendering, list comprehensions, helper-function `@psx`, and the `<use>`/`use()` disambiguation rule.

### Next.js Directives

`"use client"` and `"use server"` are preserved at the top of compiled files:

```python
"use client"

from pyths.react import component, use_state

@component
def ClientComponent():
    # ...
```

## Tips for Python Developers

1. **Think in terms of JS output** — When in doubt, consider what JS the compiler will generate
2. **Use `@dataclass`** — It generates 6x less code than manual classes with validation, serialization, and equality
3. **Use type annotations** — They enable `pyths check` and `.d.ts` output for TypeScript interop
4. **Use `pyths lint`** — Catches unused variables, unreachable code, and common mistakes
5. **Use `pyths fmt`** — Normalizes formatting for consistent style
6. **Check compiled output** — `pyths compile --stdout app.ps` lets you inspect what JS is generated
