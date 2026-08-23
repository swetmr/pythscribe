# PythScribe Language Reference

Complete reference for all PythScribe syntax, semantics, and Python-to-JavaScript mappings.

## File Format

- File extension: `.ps`
- Encoding: UTF-8
- Indentation: Spaces (default 4), consistent per block
- Comments: `#` line comments (no block comments)

## Variables and Assignment

```python
x = 42                    # → let x = 42;
x = 100                   # → x = 100;  (reassignment, no let)
a, b = 1, 2              # → let a; let b; ([a, b] = pyTuple(1, 2));
a, b = b, a              # → swap idiom: reassigns, no re-declaration
a = b = 5                # → let a = 5; let b = 5;  (RHS evaluated once)
```

First assignment in a scope emits `let`; subsequent assignments are plain reassignment.

### Augmented Assignment

```python
x += 1      # x += 1
x -= 1      # x -= 1
x *= 2      # x *= 2
x /= 2      # x /= 2
x //= 2     # x = Math.floor(x / 2)
x %= 3      # x = ((x % 3) + 3) % 3
x **= 2     # x **= 2
x &= 0xFF   # x &= 0xFF
x |= 0x01   # x |= 0x01
x ^= mask   # x ^= mask
x <<= 2     # x <<= 2
x >>= 2     # x >>= 2
```

### Annotated Assignment

```python
x: int = 42               # → let x = 42;  (annotation stripped)
name: str = "Alice"        # → let name = "Alice";
```

Annotations are parsed and available for type checking and `.d.ts` generation, but stripped from JS output.

### Walrus Operator

```python
if (n := len(items)) > 0:   # → if ((n = pyLen(items)) > 0) {
    print(n)
```

## Literals

### Numbers

```python
42          # integer
3.14        # float
0xFF        # hex
0b1010      # binary
0o77        # octal
1_000_000   # underscore separators
```

Integers are **arbitrary-precision** (hybrid `Number`/`BigInt` — `2**53 + 1` is `9007199254740993`, exact); floats compile to JS `Number` (IEEE-754 64-bit).

### Strings

```python
"hello"                     # double-quoted
'hello'                     # single-quoted
"""triple                   # multi-line → backtick template
quoted"""
f"value={x}"                # f-string → `value=${x}`
f"total={a + b}"            # expression in f-string
```

Escape sequences are decoded with CPython semantics: `\n`, `\t`, `\\`, `\"`, `\'`, `\r`, `\a`, `\b`, `\f`, `\v`, octal (`\101`), `\xNN`, `\uNNNN`, `\UNNNNNNNN`. Unrecognized escapes keep the literal backslash (like CPython). Raw strings (`r"..."` / `R'...'`) keep their backslashes verbatim.

### Booleans and None

```python
True        # → true
False       # → false
None        # → null
```

### Collections

```python
[1, 2, 3]                   # list → [1, 2, 3]
(1, 2, 3)                   # tuple → [1, 2, 3]  (arrays in JS)
{"key": "value"}             # dict → {"key": "value"}
{1, 2, 3}                   # set → new Set([1, 2, 3])
{1: "a", 2: "b"}            # dict, non-string keys → new PyDict([[1, "a"], [2, "b"]])
```

**Dict representation is hybrid (#83).** A dict literal or comprehension
whose keys are all provably strings (string literals, f-strings,
`str(...)`) compiles to a plain JS object — full JS interop: it can be
spread into React props, passed to `JSON.stringify`, handed to any JS
API. Any other key shape (int, float, bool, tuple, dynamic expression)
compiles to a Map-backed `PyDict` that preserves key type/identity with
CPython semantics: `d[True]`, `d[1]`, and `d[1.0]` are the same key
(first-inserted key object wins, so `{True: 'a'}` reprs as `{True: 'a'}`),
`d[1]` and `d['1']` are different keys, and tuples work as keys by
structural equality. All dict operations dispatch on the shape at
runtime, so both kinds flow through the same code.

**JS boundary:** a Map-backed dict handed directly to a JS API that
expects a plain object (React props, `Object.keys` consumers,
`JSON.stringify` outside PythScribe's own `json.dumps`) will not behave
like an object literal — it is a real JS `Map` subclass. When keys are
strings, convert at the boundary with `dict(d)`, which returns a plain
object whenever every key is a string. (PythScribe's own `json.dumps`
handles Map-backed dicts natively, coercing keys the CPython way.)

### Spread/Unpack

```python
[*a, *b]                    # → [...a, ...b]
{**a, **b}                  # → {...a, ...b}
f(*args, **kwargs)           # → f(...args, ...kwargs)
```

## Operators

### Arithmetic

| Python | JavaScript | Notes |
|--------|-----------|-------|
| `a + b` (numbers/strings) | `a + b` | |
| `a + b` (list/tuple) | `[...a, ...b]` | Spread concat — JS `[] + []` coerces to `""`; this is the Python-correct lowering |
| `a + b` (set) | `new Set([...a, ...b])` | Set union via spread |
| `a - b` | `a - b` | |
| `a * b` | `a * b` | |
| `a / b` | `a / b` | True division |
| `a // b` | `Math.floor(a / b)` | Floor division |
| `a % b` | `((a % b) + b) % b` | Python-correct modulo (sign of divisor) |
| `a ** b` | `a ** b` | Exponentiation |
| `-a` | `-a` | Unary negation |
| `+a` | `+a` | Unary plus |

### Comparison

| Python | JavaScript | Notes |
|--------|-----------|-------|
| `a == b` (primitives) | `a === b` | Matches Python for int/float/bool/str/None |
| `a == b` (list/dict/set/tuple) | `pyEq(a, b)` | Element-wise compare, not JS reference compare |
| `a != b` (primitives) | `a !== b` | |
| `a != b` (list/dict/set/tuple) | `!pyEq(a, b)` | |
| `a < b` | `a < b` | |
| `a <= b` | `a <= b` | |
| `a > b` | `a > b` | |
| `a >= b` | `a >= b` | |

Chained comparisons: `a < b < c` → `a < b && b < c`

PythScribe closes the JS reference-equality footgun: `[1, 2] == [1, 2]` is `True` in Python but `false` under raw JS `===`. The codegen detects collection-typed operands and routes through `pyEq` (element-wise via the runtime helper). Primitive comparisons stay `===` since JS already matches Python for those.

### Logical

| Python | JavaScript |
|--------|-----------|
| `not x` | `!x` |
| `x and y` | `x && y` |
| `x or y` | `x \|\| y` |

### Membership and Identity

| Python | JavaScript | Notes |
|--------|-----------|-------|
| `x in y` | `y.includes(x)` | For arrays/strings |
| `x not in y` | `!y.includes(x)` | |
| `x is y` | `x === y` | Reference equality |
| `x is not y` | `x !== y` | |

### Bitwise

| Python | JavaScript |
|--------|-----------|
| `a & b` | `a & b` |
| `a \| b` | `a \| b` |
| `a ^ b` | `a ^ b` |
| `~a` | `~a` |
| `a << b` | `a << b` |
| `a >> b` | `a >> b` |

### Ternary

```python
x if condition else y       # → condition ? x : y
```

## Functions

### Definition

```python
def greet(name):
    return f"Hello, {name}!"
```

→

```javascript
function greet(name) {
    return `Hello, ${name}!`;
}
```

### Default Arguments

```python
def greet(name, greeting="Hello"):
    return f"{greeting}, {name}!"
```

### `*args` and `**kwargs`

```python
def func(*args, **kwargs):
    print(args, kwargs)
```

→

```javascript
function func(...args) {
    console.log(args, kwargs);
}
```

### Lambda

```python
square = lambda x: x ** 2      # → const square = (x) => x ** 2;
add = lambda a, b: a + b       # → const add = (a, b) => a + b;
```

### Return

```python
def f():
    return 42                   # → return 42;

def g():
    return                      # → return;  (implicit None)
```

### Async Functions

```python
async def fetch_data():
    result = await get_data()
    return result
```

→

```javascript
async function fetchData() {
    const result = await getData();
    return result;
}
```

### Generators

```python
def countdown(n):
    while n > 0:
        yield n
        n -= 1
```

→

```javascript
function* countdown(n) {
    while (n > 0) {
        yield n;
        n -= 1;
    }
}
```

`yield from iterable` is also supported and compiles to `yield* iterable`.

### Decorators

```python
@my_decorator
def func():
    pass

# Equivalent to: func = my_decorator(func)
```

Built-in decorators: `@dataclass`, `@component`, `@staticmethod`, `@property`, `@validator`.

## Classes

### Basic Class

```python
class Animal:
    def __init__(self, name, sound):
        self.name = name
        self.sound = sound

    def speak(self):
        return f"{self.name} says {self.sound}"
```

→

```javascript
class Animal {
    constructor(name, sound) {
        this.name = name;
        this.sound = sound;
    }
    speak() {
        return `${this.name} says ${this.sound}`;
    }
}
```

### Key transformations

- `__init__` → `constructor`
- `self` → `this`
- `self.attr` → `this.attr`
- `__str__` → `toString()`

### Inheritance

```python
class Dog(Animal):
    def __init__(self, name):
        super().__init__(name, "Woof")

    def fetch(self, item):
        return f"{self.name} fetches {item}"
```

→

```javascript
class Dog extends Animal {
    constructor(name) {
        super(name, "Woof");
    }
    fetch(item) {
        return `${this.name} fetches ${item}`;
    }
}
```

### Static Methods and Properties

```python
class MathHelper:
    @staticmethod
    def add(a, b):
        return a + b

    @property
    def value(self):
        return self._value
```

### Instantiation

Names starting with uppercase automatically get `new`:

```python
dog = Dog("Rex")            # → let dog = new Dog("Rex");
result = process(data)      # → let result = process(data);  (lowercase = no new)
```

### Class Components & Error Boundaries (React)

**A3 (2026-07-03), verified with real `react` + `react-dom`.** Subclassing an
imported/native base — `class Boundary(Component)` from `from react import
Component` — does **not** join the cooperative-MRO object model used by
regular PythScribe class hierarchies (the model built for pure-PythScribe
multiple inheritance, B-026). If it did, `__init__` would be emitted as a
mixed-in prototype method instead of the JS `constructor`, and nothing would
ever call it through a native base's own constructor — verified empirically:
`self.state` assigned in `__init__` came back `undefined`/`null` at runtime,
crashing `render()`. The fix: **when a class's first base is not itself a
`class`-defined name in the same file** (i.e. it's external/native — an
import), the class keeps the same native `extends` + native `constructor` +
native `super()` path already used for `Exception` subclasses. Statics
(`@staticmethod`) are unaffected either way — they were never part of the
cooperative wrap.

```python
from react import Component, createElement as h

class Boundary(Component):
    def __init__(self, props):
        super().__init__(props)
        self.state = {"hasError": False}

    @staticmethod
    def getDerivedStateFromError(error):
        return {"hasError": True}

    def componentDidCatch(self, error, info):
        log_error(error, info)

    def render(self):
        if self.state["hasError"]:
            return h("p", None, "Something went wrong.")
        return self.props.children
```

→

```javascript
class Boundary extends Component {
    constructor(props) {
        super(props);
        this.state = { hasError: false };
    }
    static getDerivedStateFromError(error) {
        return { hasError: true };
    }
    componentDidCatch(error, info) {
        logError(error, info);
    }
    render() {
        if (this.state.hasError) {
            return h("p", null, "Something went wrong.");
        }
        return this.props.children;
    }
}
```

Verified at runtime (node-level regression test:
`tests/differential/class_component_test.mjs`, real `react` + `react-dom` +
`jsdom`, run via `node --test`):

- **Plain class component** (`render()` only, no `__init__`) — renders
  correctly via `react-dom/server`'s `renderToStaticMarkup`.
- **`self.state` + `self.setState(...)`** — `__init__` runs, `this.state` is
  set, `render()` reads it correctly.
- **Error boundary** — `static getDerivedStateFromError` + `componentDidCatch`
  both fire and the fallback UI renders when a child throws (tested with a
  real client-side `createRoot` reconciliation against a `jsdom` DOM, inside
  `React.act`).

Regular (non-React, no external base) class hierarchies are unaffected — the
cooperative-MRO model (`extends PyObject`, `__init__` as a dispatched
prototype method, `__pySuper` for cooperative `super()`) still applies, and
so does inheriting from another PythScribe-defined class in the *same file*.
The one known gap: subclassing a PythScribe class defined in a *different*
`.ps` file currently can't be distinguished from an external/native base at
the single-file codegen layer, so it would (incorrectly) take the native
path too. This doesn't come up in practice — Reference-app's own class usage is
exactly the React-base case above — but is called out here rather than left
silent.

### Dataclass

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
```

Auto-generates:
- `constructor(name, age, email, tags)` — with type validation and Field constraints
- `toString()` — `User(name=..., age=..., ...)`
- `__eq__(other)` — deep field comparison
- `toDict()` — serialize to plain object
- `static fromDict(obj)` — deserialize from object
- `@validator` methods called at end of constructor
- `frozen=True` — prevents field mutation after construction

## Control Flow

### if/elif/else

```python
if x > 0:
    print("positive")
elif x == 0:
    print("zero")
else:
    print("negative")
```

**Truthiness is Python's, not JavaScript's.** When the test expression is a known list/dict/set/tuple (literal or scope-tracked), the codegen wraps it in `pyBool()` so `if []:`, `if {}:`, `if set():` are all falsy — matching Python. Primitive tests (`int`, `bool`, `str`, `None`) stay bare since JS truthiness already matches Python for those.

```python
items: list = []
if items:           # → if (pyBool(items)) { ... }   — falsy when empty
    process(items)

count = 0
if count:           # → if (count) { ... }            — bare; JS 0-falsy matches Python
    do_work()
```

### for loop

```python
for item in items:               # → for (const item of items) {
    print(item)

for i, item in enumerate(items): # → for (const [i, item] of pyEnumerate(items)) {
    print(i, item)

for i in range(10):              # → for (const i of pyRange(10)) {
    print(i)
```

### while loop

```python
while condition:
    process()
```

### break / continue

```python
for x in items:
    if x < 0:
        continue
    if x > 100:
        break
    process(x)
```

### pass

```python
def placeholder():
    pass                          # → (empty function body)
```

### try/except/finally

```python
try:
    result = parse(data)
except ValueError as e:
    print(f"Error: {e}")
except Exception:
    print("Unknown error")
else:
    print("Success")
finally:
    cleanup()
```

→

```javascript
try {
    let result = parse(data);
} catch (__err) {
    if (__err instanceof ValueError) {
        const e = __err;
        console.log(`Error: ${e}`);
    } else {
        console.log("Unknown error");
    }
} finally {
    cleanup();
}
```

### raise

```python
raise ValueError("invalid input")   # → throw new ValueError("invalid input");
raise                                # → throw __err;  (re-raise in except block)
```

### assert

```python
assert x > 0, "x must be positive"
# → if (!(x > 0)) { throw Object.assign(new Error("x must be positive"), { name: "AssertionError" }); }
```

The thrown Error has `.name = "AssertionError"`, so Node and DevTools display the trace as `Error [AssertionError]: x must be positive` — matching Python's exception name.

### Python-named runtime errors

PythScribe routes the common silent-failure cases of raw JavaScript through Python-named runtime helpers, so a `.ps` program raises the exception class a Python developer expects:

| Operation | Python raises | PythScribe emits at runtime |
|---|---|---|
| `items[10]` on a 3-item list | `IndexError: list index out of range` | `Error [IndexError]: list index out of range` |
| `d["missing"]` on a dict | `KeyError: 'missing'` | `Error [KeyError]: 'missing'` |
| `t[5]` on a 3-tuple | `IndexError` | `Error [IndexError]: list index out of range` |
| `s[100]` on a short string | `IndexError: string index out of range` | `Error [IndexError]: string index out of range` |
| `a // 0`, `a % 0` | `ZeroDivisionError` | `Error [ZeroDivisionError]: integer division or modulo by zero` |
| `assert False, "msg"` | `AssertionError: msg` | `Error [AssertionError]: msg` |

Implementation: subscript reads (`a[i]`) on typed list/dict/tuple values AND untyped values route through the `pyGetItem` runtime helper, which does the bounds/key check and throws the Python-named error class (#83 extended this to untyped receivers so Map-backed dicts survive unannotated channels; non-plain-prototype objects — DOM wrappers, class instances — pass through natively, preserving interop). Subscript writes (`a[i] = x`) on dict-typed and untyped receivers route through `pySetItem` the same way; list-typed writes stay bare (hot path). Optional chaining (`a?.[i]`) stays bare regardless of type — wrapping would break the short-circuit semantics.

Floor division (`//`) and modulo (`%`) **always** route through `pyFloorDiv` / `pyMod` so a zero divisor raises `ZeroDivisionError` instead of silently producing `Infinity` or `NaN` as raw JS would.

### `pyths run --explain`

For a Python-only developer who doesn't read JavaScript, run with the `--explain` flag for a Python-style explanation paragraph above any crash:

```bash
pyths run app.ps --explain
```

```
─── PythScribe runtime error ──────────────────────────────────
IndexError — your code tried to read past the end of a sequence
(list index out of range).
In Python this raises IndexError; PythScribe follows the same
rule rather than silently returning undefined as raw JS would.

Source location: at crash (app.ps:2:12)
────────────────────────────────────────────────────────────────
```

Recognised classes: `IndexError`, `KeyError`, `ZeroDivisionError`, `AttributeError`, `AssertionError`, `TypeError`, `ValueError`. Successful runs are silent — `--explain` only fires when the program crashes.

### with statement

```python
with open_resource() as r:
    process(r)
```

→

```javascript
const r = openResource();
try {
    process(r);
} finally {
    // cleanup
}
```

### delete

```python
del obj.attr                # → delete obj.attr;
del arr[i]                  # → pyDelItem(arr, i);  (splice — Python semantics, no JS hole)
del d[k]                    # → pyDelItem(d, k);    (KeyError when the key is absent)
```

## Pattern Matching

```python
match subject:
    case pattern:
        body
```

Compiles to:
```javascript
const __match = subject;
if (/* pattern check */) {
    /* bindings */
    body
}
```

### Pattern Types

| Pattern | Example | Check |
|---------|---------|-------|
| Wildcard | `case _:` | Always matches |
| Literal | `case 42:` | `__match === 42` |
| Capture | `case x:` | `let x = __match` |
| OR | `case 1 \| 2:` | `__match === 1 \|\| __match === 2` |
| Sequence | `case [a, b]:` | `Array.isArray(__match) && __match.length === 2` |
| Star | `case [first, *rest]:` | Array check + rest = slice |
| Mapping | `case {"key": val}:` | `"key" in __match` |
| Class | `case Point(x, y):` | `__match instanceof Point` |
| AS | `case pat as name:` | Pattern + `let name = __match` |
| Guard | `case x if x > 0:` | Pattern + condition |
| Value | `case Color.RED:` | `__match === Color.RED` |

## Comprehensions

### List Comprehension

```python
squares = [x ** 2 for x in range(10)]
# → const squares = pyRange(10).map((x) => x ** 2);

evens = [x for x in numbers if x % 2 == 0]
# → const evens = numbers.filter((x) => ((x % 2 + 2) % 2) === 0).map((x) => x);
```

### Dict Comprehension

```python
mapping = {f"k{k}": v * 2 for k, v in items}
# → const mapping = Object.fromEntries(items.map(([k, v]) => [`k${k}`, v * 2]));

squares = {x: x * x for x in range(5)}     # non-string keys (#83)
# → const squares = new PyDict(pyRange(5).map((x) => [x, pyMul(x, x)]));
```

### Generator Expression

```python
total = sum(x ** 2 for x in range(10))
```

## Imports

### Standard Import

```python
import math                       # → import * as math from "pyths-runtime/stdlib/math";
import json as j                  # → import * as j from "pyths-runtime/stdlib/json";
from os import path               # → import { path } from "os";
from utils import helper as h     # → import { helper as h } from "utils";
```

Recognized Python stdlib names (`math`, `json`, `itertools`, `functools`,
`collections`, `random`, `datetime`, `re`, `decimal`, `fractions`) route to
the runtime's stdlib implementations. Anything else non-relative is treated
as an npm bare specifier, with `_` → `-` kebab-casing per path segment
(`from framer_motion import motion` → `"framer-motion"`) — see
[multi-file-apps.md](multi-file-apps.md) for the full resolution chain and
why local files must use relative imports instead.

### PythScribe Standard Library

```python
from pyths.math import sqrt     # → import { sqrt } from "pyths-runtime/stdlib/math";
from pyths.json import dumps    # → import { dumps } from "pyths-runtime/stdlib/json";
from pyths.fetch import get     # → import { get } from "pyths-runtime/web/fetch";
from pyths.storage import local # → import { local } from "pyths-runtime/web/storage";
```

`pyths.<name>` accepts both the stdlib modules (aliases of the bare
`import math` form above) and the web wrappers (`pyths.fetch`,
`pyths.storage`, `pyths.router`, `pyths.dom`).

### Local Project Imports (Relative)

Python-style relative imports are the supported way to import your own
`.ps` files. They emit literal, extensionless relative ESM specifiers —
no kebab-casing, no npm remapping — and the Vite/Next plugins resolve them
back to the `.ps`/`.psc` sources:

```python
from .theme import COLORS           # → import { COLORS } from "./theme";
from .pages.Home import Home       # → import { Home } from "./pages/Home";
from ..lib.store import make_task  # → import { make_task } from "./../lib/store";
x = [COLORS, Home, make_task]
```

Full details — bundler resolution, dual-track sibling precedence, the
absolute-prefix/alias workaround, and a verified multi-file walkthrough —
in [multi-file-apps.md](multi-file-apps.md).

### React Imports

```python
from pyths.react import component, use_state
# → import { memo, useState } from "react";
```

Snake_case hooks auto-convert: `use_state` → `useState`, `use_effect` → `useEffect`.

### Suppressed Imports

These are handled at compile time and produce no JS import:

```python
from dataclasses import dataclass, field     # suppressed
from pydantic import BaseModel               # suppressed
from typing import Optional, List, Dict      # suppressed
```

### Side-Effect Imports (Assets)

**PythScribe extension — not valid Python.** Python has no bare-string-literal
import statement; this is a deliberate `.ps`-only syntax addition, in the same
spirit as the parser's positional-args-after-keyword-args relaxation used by
PSX element calls (see "Element call syntax" under PSX, below).

A module-level `import "<string>"` imports a path purely for its bundler
side effect — no name is bound. Use it for stylesheets and other assets that
a component file owns directly:

```python
import "./styles.css"          # → import "./styles.css";
import "./theme.scss"          # → import "./theme.scss";
import "../assets/logo.png"    # → import "../assets/logo.png";
```

The string is emitted verbatim; the compiler does not inspect or validate
the extension — resolving `.css`/`.scss`/image imports is the bundler's job
(Vite, webpack, Next.js, etc. all support this natively). Because the
statement binds no name, it never triggers the W002 "unused import" lint
warning, and it's JS-only — a module containing it stays on the JS side of
codegen (there's nothing for the WASM path to analyze; only function bodies
are considered for WASM eligibility).

Only the exact bare-string shape is accepted this way — `import "./x.css" as
y` and comma-separated string imports are parse errors, and normal
`import module` / `from module import name` grammar is unaffected.

## Built-in Functions

| Python | JavaScript | Import |
|--------|-----------|--------|
| `print(*args)` | `pyPrint(*args)` — Python-style formatting via `pyStr` | runtime |
| `len(x)` | `pyLen(x)` | runtime |
| `range(stop)` / `range(start, stop, step)` | `pyRange(...)` | runtime |
| `enumerate(x)` | `pyEnumerate(x)` | runtime |
| `zip(a, b)` | `pyZip(a, b)` | runtime |
| `sorted(x)` | `pySorted(x)` | runtime |
| `reversed(x)` | `pyReversed(x)` | runtime |
| `isinstance(x, T)` | `x instanceof T` | — |
| `type(x)` | `x?.constructor ?? typeof x` | — |
| `str(x)` | `pyStr(x)` | runtime |
| `int(x)` | `Math.trunc(Number(x))` | — |
| `float(x)` | `Number(x)` | — |
| `bool(x)` | `pyBool(x)` | runtime |
| `list(x)` | `Array.from(x)` | — |
| `dict(x)` | `pyDict(x)` — plain object if all keys are strings, else Map-backed `PyDict` (#83) | runtime |
| `set(x)` | `PySet(x)` | runtime |
| `tuple(x)` | `PyTuple(x)` | runtime |
| `abs(x)` | `Math.abs(x)` | — |
| `min(*args)` | `Math.min(*args)` | — |
| `max(*args)` | `Math.max(*args)` | — |
| `sum(x)` | `x.reduce((a, b) => a + b, 0)` | — |
| `round(x)` | `Math.round(x)` | — |
| `map(f, x)` | `[...x].map(f)` | — |
| `filter(f, x)` | `[...x].filter(f)` | — |
| `any(x)` | `x.some(Boolean)` | — |
| `all(x)` | `x.every(Boolean)` | — |
| `repr(x)` | `pyRepr(x)` — CPython-style repr | runtime |
| `format(v[, spec])` | `pyFormat(v, spec)` — the f-string/`str.format` engine, `__format__` protocol | runtime |
| `slice(...)` | `pySliceOf(...)` — a real slice object; `xs[slice(1, 3)]` ≡ `xs[1:3]` | runtime |
| `ascii(x)` | `pyAscii(x)` — repr with `\xNN`/`\uNNNN`/`\UNNNNNNNN` escapes | runtime |
| `vars(obj)` | `pyVars(obj)` — the instance `__dict__` (zero-arg `vars()` is `locals()` → compile error) | runtime |

**Unimplemented builtins are a compile error, not a runtime surprise.** A bare
reference to a known CPython builtin that has no lowering — `open`, `input`,
`eval`, `exec`, `compile`, `hash`, `id`, `globals`, `locals`, `memoryview`,
`help`, `breakpoint`, `aiter`, `anext`, `__import__` — fails `pyths compile`
**and** `pyths check` with a named diagnostic
(`builtin 'open' is not supported yet (pythscribe-v3.x) …`) instead of
compiling to a bare JS identifier that crashes with `ReferenceError` at
runtime. User bindings, imports, and star-import rebinds of these names
shadow the builtin and compile normally.

## Type Annotations

### Supported Types

| Python Annotation | TypeScript (.d.ts) | Notes |
|-------------------|-------------------|-------|
| `int`, `float` | `number` | |
| `str` | `string` | |
| `bool` | `boolean` | |
| `None` | `null` (or `void` for returns) | |
| `Any` | `any` | |
| `List[T]` | `T[]` | |
| `Dict[K, V]` | `Record<K, V>` | |
| `Optional[T]` | `T \| null` | |
| `Tuple[A, B]` | `[A, B]` | |
| `Set[T]` | `Set<T>` | |
| `Union[A, B]` | `A \| B` | |
| `Callable[[A, B], R]` | `(arg0: A, arg1: B) => R` | |
| `MyClass` | `MyClass` | Pass-through |

### Usage

```python
# Function annotations
def add(a: int, b: int) -> int:
    return a + b

# Variable annotations
count: int = 0
name: str = "hello"

# Class field annotations (used by @dataclass)
@dataclass
class User:
    name: str
    age: int
```

### Type Checking

```bash
pyths check file.ps
```

Checks:
- Annotated assignment type mismatches
- Function return type mismatches
- Function call argument count
- Variable reassignment type compatibility

### Declaration Files

```bash
pyths compile file.ps -o file.js --dts
```

Generates `file.d.ts` with TypeScript declarations for all exported functions, classes, and variables.

## PSX (Pythonic JSX)

PSX is enabled inside `@component` and `@psx`-decorated functions. **PSX is pure Python syntax** — there are no angle brackets. HTML elements are written as function calls; the codegen rewrites them to `React.createElement(...)`.

PSX is gated on the decorator. Without `@component` or `@psx`, calls to names like `div()` are treated as ordinary function calls and would resolve via the local scope (i.e., they'd error if `div` is not defined).

### Element call syntax — three forms

PythScribe supports four equivalent forms for an HTML element. They all compile to the same `createElement` call.

```python
@psx
def forms():
    # Form 1 — nested (default): tag(prop=val, …, child, …)
    # Props and children in the same call. PythScribe's parser
    # permits positional args after keyword args (CPython doesn't);
    # the codegen separates kwargs → props from positional → children
    # regardless of order. The most Pythonic form.
    form1 = div(class_name="card", h2("Hello"))

    # Form 2 — direct: tag(children…) when there are no props
    form2 = div(h2("Hello"))

    # Form 3 — curried: tag(prop=val, …)(children…)
    # Props in the first call, children in the second. Useful when
    # you want visual separation of props from children, especially
    # for deeply nested trees.
    form3 = div(class_name="card")(h2("Hello"))

    # Form 4 — empty-props curried: tag()(children…)
    # The props-less analog of Form 3, for callsites where you want
    # the same shape as props-bearing elements.
    form4 = div()(h2("Hello"))
    return form1
```

All four compile to:

```js
createElement("div", {className: "card"}, createElement("h2", null, "Hello"))
// (Forms 2 and 4 produce the same call without the props object.)
```

**Why the nested `tag(prop=v, child)` form is the default.** It reads as one call — most Pythonic and most concise for typical UI trees. Reserved cases for the curried form: when the props list is long enough that visual separation from children helps readability, or when you want a uniform shape between props-bearing and props-less elements at adjacent call sites.

**At runtime these are not nested function calls.** The codegen detects the curried `tag(props)(children)` shape and emits a single flat `createElement(tag, props, ...children)`. There's no `tag()` returning a function that's then called — that would be wrong both semantically and for performance. The double parens are pure source-level syntax.

### Props

Props are passed as keyword arguments. Snake_case auto-converts to camelCase:

```python
@psx
def clickable(handler):
    return div(class_name="card", on_click=handler)("Click me")
    # → createElement("div", {className: "card", onClick: handler}, "Click me")
```

| Python | JavaScript |
|---|---|
| `class_name="x"` | `className: "x"` |
| `on_click=fn` | `onClick: fn` |
| `html_for="id"` | `htmlFor: "id"` |
| `tab_index=0` | `tabIndex: 0` |
| `aria_label="x"` | `"aria-label": "x"` (kebab) |
| `data_test_id="x"` | `"data-test-id": "x"` (kebab) |
| `default_value="x"` | `defaultValue: "x"` |
| `auto_focus=True` | `autoFocus: true` |

ARIA and `data_*` attributes use kebab-case (HTML convention). Other DOM attributes use camelCase (React convention). The conversion is automatic.

**`style` props get an extra layer**: when the value is a Dict literal at the call site, every CSS key snake→camel-cases at compile time:

```python
@psx
def styled():
    return div(style={"border_radius": "6px", "font_family": "system-ui"})("Hi")
    # → createElement("div", {style: {borderRadius: "6px", fontFamily: "system-ui"}}, "Hi")
```

When `style` is a variable instead of a literal, the codegen wraps it in `pyNormalizeStyle(...)` so the conversion happens at runtime:

```python
@psx
def styled_var():
    my_styles = {"border_radius": "6px"}
    return div(style=my_styles)("Hi")
    # → createElement("div", {style: pyNormalizeStyle(my_styles)}, "Hi")
```

### Member access is verbatim — calling JS / DOM / library methods

snake→camel conversion is scoped to **prop names** (above) and **React import
names** (`use_state` → `useState`). It does **not** rename `obj.method(...)`
member access. There are two cases for method calls:

- **Python builtin methods** (str / list / dict / set) are *lowered* to their
  JS equivalent via the method-lowering table — write them Python-style:

  ```python
  s.strip()        # → s.replace(/^\s+|\s+$/g, "")
  name.upper()     # → name.toUpperCase()
  xs.append(x)     # → xs.push(x)
  d.get(k)         # → pyDictGet(d, k)
  ```

- **Native JS / DOM / browser / library methods** have no Python analog and
  are emitted **verbatim** — write the real API name (usually camelCase):

  ```python
  def on_submit(e):
      e.preventDefault()        # → e.preventDefault()  (NOT prevent_default)
      e.stopPropagation()       # → e.stopPropagation()
      v = e.target.value        # → e.target.value
      el.addEventListener("click", cb)
      query.invalidateQueries({"queryKey": ["runs"]})
  ```

There is no snake_case spelling of these that works: `e.prevent_default()`
compiles to `e.prevent_default()` and throws `TypeError: ... is not a function`
at runtime. The compiler can't know the receiver's type, so any member it
doesn't recognize as a Python builtin method passes straight through.

### Components (capitalized names)

Capitalized names are React components, not HTML elements. They get the same call syntax:

```python
@component
def Card(title):
    return div(class_name="card")(h2()(title))

@component
def App():
    return Card(title="Hello")()
    # → createElement(Card, {title: "Hello"})
```

Inside a `@component`, a capitalized call routes to `createElement(NameRef, props, children)`. The codegen also disambiguates `@dataclass`-defined classes from React components via a module-level pre-scan — calls to a class name emit `new ClassName(...)` while calls to a non-class capitalized name emit `createElement(...)`.

#### Name resolution inside PSX (`@component` / `@psx`)

A bare `Name(...)` call inside a PSX function resolves in this precedence:

1. **Known classes** — a local `class`, or any **CapWords name imported from a stdlib module** (`Counter`, `OrderedDict`, `ChainMap`, `Decimal`, `Fraction`, …) → `new X(...)`. Stdlib containers construct correctly inside a component.
2. **JS built-in constructors** (`Map`, `URL`, `EventSource`, `Date`, …) → `new X(...)`.
3. **React imports** (capitalized) → `createElement(X, ...)`.
4. **HTML/SVG element name** (or an unbound tag-shaped name) → `createElement("tag", ...)`.

**Design rule — builtin ∩ HTML/SVG-element collision: HTML wins inside a component.** A name that is *both* a Python builtin *and* an HTML/SVG element lowers as the **element**. The entire collision set is **`map` → `<map>`**, **`input` → `<input>`**, **`object` → `<object>`**; use a list comprehension `[f(x) for x in xs]` in place of `map(f, xs)` inside a component (`input()`/`object()` are meaningless in a browser). The common data-builtins `filter` / `set` / `list` / `dict` / `zip` / `sorted` are **not** element names — no collision, normal builtin behavior everywhere. This lowering fires **only** inside `@component`/`@psx`; outside them (module level, plain `def`s, `.py` files) every one of these names is the ordinary Python builtin — React/HTML element lowering is confined to the `@component`/`@psx` boundary.

### Children

Children are positional args after the props call:

```python
@psx
def item_list():
    return ul()(
        li()("Item 1"),
        li()("Item 2"),
        li()("Item 3"),
    )
```

**Spread a list of children with `*`**:

```python
@psx
def spread_list():
    items = ["a", "b", "c"]
    return ul()(*[li()(item) for item in items])
    # → createElement("ul", null, ...items.map(item => createElement("li", null, item)))
```

**Mixed text + dynamic content**: each comma-separated arg becomes a separate child node, matching JSX's `<p>Hello {name}</p>` semantics:

```python
@psx
def greeting(name):
    return p()("Hello, ", name, "!")
    # → createElement("p", null, "Hello, ", name, "!")  // 3 text/expression children
```

If you instead concatenate via an f-string, the result is a single text child (one DOM text node):

```python
@psx
def greeting_fstring(name):
    return p()(f"Hello, {name}!")
    # → createElement("p", null, `Hello, ${name}!`)  // 1 text child
```

This matters for DOM-parity tests against React: JSX's `<p>Hello {name}!</p>` produces multiple text children, so prefer comma-separated children when DOM byte-equality is a goal.

### Fragments

Returning a tuple from a `@component` or `@psx` function emits `<>...</>`:

```python
@component
def Multi():
    return (
        h1()("Title"),
        p()("Content"),
    )
# → createElement(Fragment, null, createElement("h1", ...), createElement("p", ...))
```

### Conditional rendering

Standard Python conditionals — `if`/`else`, ternary, short-circuit — work inside the call tree:

```python
@component
def Page(logged_in, user):
    return div()(
        h1()("Welcome"),
        Profile(user=user) if logged_in else LoginButton()(),
    )
```

Or with intermediate variables for complex conditions:

```python
@component
def Notice(severity, message):
    color = "red" if severity == "error" else "yellow"
    return div(style={"background": color})(message)
```

### List rendering

Standard list comprehension + spread:

```python
@component
def TodoList(items):
    return ul()(
        *[li(key=str(i.id))(i.text) for i in items]
    )
```

Each child element should have a stable `key` prop, same as JSX.

### Render-prop helpers and HOCs (`@psx`)

Use `@psx` for utility functions that build JSX subtrees but aren't full components (no React lifecycle, no props destructuring, no named export):

```python
from pyths.react import component, psx

@psx
def render_row(item):
    return tr()(td()(item.name), td()(item.value))

@component
def DataTable(items):
    return table()(
        thead()(tr()(th()("Name"), th()("Value"))),
        tbody()(*[render_row(i) for i in items])
    )
```

Without `@psx`, `render_row` would treat `tr()` and `td()` as ordinary function calls (looking up `tr`/`td` in scope, which don't exist) — and fail to compile.

### Supported HTML elements

All standard HTML5 elements are recognized as PSX targets when called inside a `@component`/`@psx` function. The list (140+) covers structure (`div`, `section`, `article`, `nav`), text (`p`, `h1`-`h6`, `span`, `a`, `code`, `pre`), forms (`form`, `input`, `button`, `select`, `textarea`, `label`), tables (`table`, `tr`, `td`, `th`, `thead`, `tbody`, `tfoot`), media (`img`, `video`, `audio`, `canvas`), and SVG (`svg`, `path`, `circle`, `rect`, `g`, `text`, etc.). The full list is the source of truth at `crates/pyths_codegen_js/src/react.rs::is_html_element`.

### Component-name vs HTML-name collision

Some React-import names collide with HTML/SVG element names — `use` (React 19 hook vs SVG `<use>`), `input` (form input vs… nothing else), etc. Inside `@component`, names imported from a recognized React module take precedence over the HTML-element fallback:

```python
from pyths.react import use, component

@component
async def Profile(promise):
    user = use(promise)        # React 19 use() hook, NOT createElement("use", ...)
    return div()(h2()(user["name"]))
```

The disambiguation tracks every non-aliased import from a React-recognized module. If you alias the import, the alias wins:

```python
from react_router_dom import use_navigate as goto
goto()  # `goto` is just a function call; no PSX dispatch
```

## React Integration

### Hooks

All React hooks are available with snake_case naming:

| PythScribe | React |
|----------|-------|
| `use_state(init)` | `useState(init)` |
| `use_effect(fn, deps)` | `useEffect(fn, deps)` |
| `use_context(ctx)` | `useContext(ctx)` |
| `use_ref(init)` | `useRef(init)` |
| `use_memo(fn, deps)` | `useMemo(fn, deps)` |
| `use_callback(fn, deps)` | `useCallback(fn, deps)` |
| `use_reducer(reducer, init)` | `useReducer(reducer, init)` |
| `use_layout_effect(fn, deps)` | `useLayoutEffect(fn, deps)` |
| `use_id()` | `useId()` |
| `use_transition()` | `useTransition()` |
| `use_deferred_value(val)` | `useDeferredValue(val)` |

### Next.js

Special function exports:

| PythScribe | Next.js |
|----------|---------|
| `get_static_props` | `getStaticProps` |
| `get_server_side_props` | `getServerSideProps` |
| `get_static_paths` | `getStaticPaths` |
| `generate_metadata` | `generateMetadata` |

Directives (`"use client"`, `"use server"`) are preserved at file top.

## Source Maps

```bash
pyths compile app.ps -o app.js --sourcemap
```

Generates `app.js.map` (Source Map v3) that maps compiled JS back to `.ps` source lines for browser DevTools debugging.

## CLI Reference

```
pyths compile <file> [-o output] [--stdout] [--sourcemap] [--dts] [--timings]
pyths check <file>
pyths run <file>
pyths init [name]
pyths test [path] [--verbose]
pyths fmt [path] [--check] [--indent N]
pyths lint [path]
pyths bundle <entry> [-o output] [--minify]

Global: --quiet, --verbose
```
