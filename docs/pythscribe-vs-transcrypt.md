# PythScribe vs Transcrypt

Transcrypt is PythScribe's closest peer: a mature, ahead-of-time Python→JavaScript
compiler that ships **no interpreter**. This document compares them honestly, grounded
in Transcrypt's own design article and repository (sources at the bottom).

The short version: the two agree on architecture (AOT, no interpreter, differential
testing, source maps) but sit on **opposite sides of the conformance-versus-performance
axis**. Transcrypt is performance-first with Python semantics opt-in; PythScribe is
conformance-first with performance recovered selectively via WebAssembly.

## Shared ground (be fair)

Both are AOT Python→JS compilers with no interpreter shipped, so the bundle-size and
cold-start advantage over interpreter systems (Pyodide, Brython, RustPython) is **shared
ground, not a differentiator between the two**. Both:

- use Python's own parser (pure-Python syntax);
- validate with **back-to-back differential testing against CPython** — Transcrypt's
  *autotester* (run a testlet under CPython, then under the compiled JS, compare the
  output sequence) is the same methodology as PythScribe's 1,376-program CPython
  differential corpus;
- ship **source maps** for source-level debugging.

## Transcrypt's three demands — and where PythScribe lands

Transcrypt's design article frames the goal as three demands. PythScribe accepts all
three and adds a fourth axis (provable assurance):

| Transcrypt demand | Transcrypt | PythScribe |
|---|---|---|
| **1. User experience** — indistinguishable look/feel, load/startup/sustained speed | AOT JS, no interpreter | AOT JS **+ WASM auto-routing** for compute; edge/Worker-ready runtime |
| **2. Developer experience** — JS interop, debugging, existing skills | JS interop via aliases; source maps; inline-JS escape hatch | React/Next + PSX, snake_case idiom, source maps, `--explain` crash traces, Python-named errors |
| **3. Business continuity** — talent pool, functionality/hours, maintainable | Python talent; isomorphic, readable output | Same, plus a machine-checked trust manifest and per-compilation routing certificate |

## The core divergence: conformance vs performance

This is the defining difference. Transcrypt resolves the conformance/performance tension
by **defaulting to performance and making Python semantics opt-in per scope** through
compiler pragmas:

- **Type unification.** Transcrypt "unifies" the Python and JavaScript type systems —
  *"a Transcrypt dict IS a JavaScript object, in all cases."* Numbers are JS numbers
  (IEEE-754 doubles); there is no arbitrary-precision `int` by default.
- **Operator semantics are opt-in.** Python operator behavior (value equality for
  lists/dicts, container `+`, matrix ops) requires `__pragma__('opov')`; the author notes
  enabling it globally means "even `1 + 2`" becomes two function calls, so it is off by
  default.
- **Truthiness is opt-in.** CPython truthiness (`if []:` is falsy) requires
  `__pragma__('tconv')`, "by default avoided" for inner-loop speed.

PythScribe takes the opposite default: **Python semantics everywhere, by default, with no
pragma** — exact hybrid `Number`/`BigInt` integers, floor `//` and divisor-sign `%`,
banker's rounding, code-point strings, value equality, and fail-loud `KeyError`/
`IndexError`. It treats that fidelity as a *contract* it differential-tests and partially
**proves** against CPython, and it recovers performance not by dropping semantics but by
**auto-routing pure-numeric functions to WebAssembly** (with a per-compilation routing
certificate). Where Transcrypt says "opt in to correctness when you need it," PythScribe
says "correctness is the default; opt in to raw speed via WASM."

## Isomorphism vs semantic fidelity

Transcrypt's headline design property is **isomorphism**: the generated JavaScript
structurally mirrors the Python source, line by line, so a developer can read and debug
it. This is a *readability/debuggability* property — and crucially, **isomorphic is not
the same as semantically identical to CPython** (that gap is exactly what the `opov`/
`tconv` pragmas fill).

PythScribe optimizes for the **end** that isomorphism is a means toward — behaving like
the Python you wrote — and delivers debuggability through source maps and `--explain`
rather than through structural mirroring. The two make a deliberate, opposite trade:
Transcrypt keeps the *shape* close to Python and lets *semantics* diverge unless you opt
in; PythScribe keeps the *semantics* faithful and accepts runtime helpers in the output.

## Static vs dynamic typing

Transcrypt embraces a **hybrid**: dynamic typing inside a module ("everything just falls
into place"), static typing at module boundaries via **mypy** and Python's native
annotations — contracts where modules meet.

PythScribe makes **static typing first-class**, not just a boundary contract: a built-in
inference engine and checker (`pyths check`), bundled `.pyi` stubs for React/Next/Router/
TanStack, tuple-destructure element typing (`count, set_count = use_state(0)` types both),
generic TypeVar inference, and `.d.ts` emission. Both honor Python type annotations; the
difference is that PythScribe ships the checker rather than delegating to mypy.

**TypeScript.** Neither compiler *ingests* TypeScript — both take Python. On the *output*
side they differ: Transcrypt emits **JavaScript only** (readable, Python-mirroring) with no
`.d.ts`; it consumes Python annotations for optional mypy checking but produces no TS
artifacts. PythScribe **emits `.d.ts` declaration files** alongside its JS, so a compiled
`.ps` module presents a typed surface to a TypeScript consumer. If your project's contract
is expressed in TypeScript types, PythScribe interoperates with it; Transcrypt does not
produce that surface.

## State management and complex libraries

Both can, in principle, reach the whole npm ecosystem — but the *manner* differs sharply,
and state management is the sharpest example.

- **Transcrypt** has **no first-class state-management layer**; everything is generic JS
  interop. Redux is *reachable and community-demonstrated* (e.g. the `react-redux-transcrypt`
  and `python-fullstack-transcrypt` example repos) but through **community** React wrappers
  (`reactscrypt`, `pyreact`) that expose `createElement`/`createClass` with **no JSX**, not
  through Transcrypt core. Zustand has no Transcrypt-specific support at all — it works only
  as raw interop (import the npm module via a bundler's `require()`, call its hooks with JS
  naming). The interop mechanism is manual: `__pragma__('js', …)` to inline JS, name-aliases
  for Python-reserved identifiers, no type stubs. Transcrypt's type-unification (a dict *is*
  a JS object) does make plain-object Redux actions/state interop naturally — but that is the
  same unification that relaxes Python semantics.
- **PythScribe** treats these as **native, typed, tested** libraries: it ships a
  `zustand.pyi` type stub (plus React/Next/Router/TanStack), recognizes the state libraries
  in codegen, and carries integration tests and a tutorial. The reference application
  includes real `.ps` components — a Redux Toolkit + react-redux counter and a Zustand
  counter — dual-track-verified against a React oracle. You write them in snake_case/PSX
  idiom with full type inference, not as hand-wrapped JS.

So: Transcrypt *can* drive Redux (community-proven) and *could* drive Zustand (raw interop
only), with no first-class bindings, no type stubs, no JSX, and camelCase JS naming;
PythScribe drives both as native, typed, verified libraries in Python idiom.

## Runtime architecture: compile-to-JS vs ship-the-interpreter

A prior question worth settling explicitly: **what runtime does Python-in-the-browser run
on?** There are two families, and PythScribe and Transcrypt are in the same one.

- **Compile-to-JS (PythScribe, Transcrypt).** Emit JavaScript plus a *small* helper runtime
  (kilobytes: `len`, `range`, dict/list helpers, …). No interpreter ships; your code *is* JS.
- **Ship-the-interpreter (Pyodide / PyScript).** Ship CPython itself compiled to WebAssembly
  (~6–11 MB) and interpret `.py` at runtime.

| Axis (browser/edge) | Compile-to-JS (PythScribe, Transcrypt) | Ship-the-interpreter (Pyodide/PyScript) |
|---|---|---|
| What ships | your code as JS + KB of helpers | full CPython + stdlib in WASM (~6–11 MB) |
| Cold start | milliseconds (edge-viable, PythScribe targets <50 ms) | seconds (interpreter boot + heap) |
| Execution speed | native JS-engine speed (+ WASM for numeric) | interpreted Python-in-WASM (much slower) |
| DOM/JS interop | direct — the emitted code is JS | expensive JS↔WASM crossing per call |
| Semantics | JS by default; CPython fidelity must be *re-created* on JS | true CPython for free (it *is* CPython) |
| Python packages | none as packages (you write the language); npm libs instead | any pure-Python / WASM-wheel package |
| Debug frames | JS, source-mapped back to `.py`/`.ps` | real Python frames |

**For production web / edge / serverless, compile-to-JS wins decisively** — bundle size, cold
start, and speed make shipping a 6–11 MB interpreter a non-starter there. **Ship-the-interpreter
is the right tool only** when you need *real CPython + its package ecosystem* in the browser
(numpy/pandas, notebook-style data apps, teaching demos) and bundle/cold-start don't matter.

The price of compile-to-JS is that CPython semantics must be **re-created on a JS runtime** — and
this is exactly where PythScribe and Transcrypt diverge (the "conformance vs performance" axis
above): **PythScribe pays it in full** with verified/differential-tested helpers (arbitrary-precision
`int`, `KeyError`, code-point strings, banker's rounding → **Python syntax with CPython-faithful semantics**), while
**Transcrypt keeps the runtime thinner** and lets JavaScript semantics show through on the interop
surface unless you opt into a Python idiom. Both keep the runtime in the KB range, not MB.
PythScribe adds a **third gear neither the interpreter nor a pure JS-transpile has**: WASM
auto-routing of pure-numeric hot paths, so the numeric fragment runs at near-native speed while the
DOM/UI code stays lean JS.

## Debuggability and source maps

Both emit **Source Map v3** (`//# sourceMappingURL=…`), so a browser shows console logs, thrown
errors, and stack frames at the **original source line** — Transcrypt at `mapping.py:4`
(`at print_stuff (mapping.py:4)`), PythScribe at `app.ps:4` — clickable straight to the source.
PythScribe embeds **`sourcesContent`** and **preserved-identifier `names`** (so frames resolve
without shipping separate source files), and its console `print` is emitted **inline at the call
site**, so its output is attributed to *your* `.ps` line rather than to a runtime module (Transcrypt
maps `print` into `org.transcrypt.__runtime__.py` because its runtime is transpiled Python).
Transcrypt adds optional **source-line annotations** and an **inline-JS escape hatch**; PythScribe
adds **`pyths run --explain`** (a Python-style explanation above any crash) and **Python-named
runtime errors** with CPython-matching text, source-mapped to `.ps` line:col, plus CDP-verified
**breakpoints and step-over**.

The errors themselves differ, not only their display. Transcrypt surfaces **JS-native** exceptions
(a `TypeError`, a thrown `Error`) mapped back to the `.py` line — but by default it does **not**
synthesize Python's fail-loud errors: a missing dict key or an out-of-range index returns `undefined`
(its performance-first default), whereas PythScribe **raises** `KeyError` / `IndexError` /
`ZeroDivisionError` with CPython-matching message text. Fail-loud collections are already a point
against TypeScript (whose types erase at runtime); by default they separate PythScribe from Transcrypt
too.

The one place Transcrypt was ahead — **step-into** — is closed in **v0.2.2**: because PythScribe's
runtime is JS (not transpiled Python), step-into used to descend into JS helpers; shipping an
identity `.js.map` with `ignoreList` per runtime file (+ `x_google_ignoreList` on the runtime chunk
in the Vite/Next plugins) makes step-into **skip the runtime entirely and stay in your `.ps`** —
cleaner than stepping *through* a Python runtime. (The bundled-build source map lands in the same
change.)

## Interop and naming

Transcrypt resolves Python/JS name clashes with **aliases compiled away at build time**:
`py_split` for Python's `str.split`, `js_split` for the native one, and
`__pragma__('alias', 'S', '$')` to spell identifiers Python forbids (jQuery's `$`). Its
React usage is the raw library: `createElement` + camelCase JS names.

PythScribe converts **snake_case → camelCase** for React import names and JSX props
(`use_state`→`useState`, `on_click`→`onClick`), keeps member access verbatim, and lets you
write components with `@component`/`@psx` and Pythonic JSX (PSX) with props delivered as
named parameters — plus a Next.js toolchain and 30+ recognized libraries. So PythScribe
code reads as Python; Transcrypt code reads as Python-shaped JavaScript.

The difference is concrete. A Transcrypt React component wires JS by hand, spells everything
in **camelCase** to match JS, calls `createElement` with **string-keyed prop dicts**, and
reaches into the event as a **JS object** (`event['target']['value']`):

```python
# Transcrypt — Python-shaped JavaScript
React = require('react')                       # manual JS import
createElement = React.createElement            # manual name mapping
el = createElement
def App():
    newItem, setNewItem = useState("")         # camelCase, JS names
    def handleChange(event):
        target = event['target']               # JS object via subscript
        setNewItem(target['value'])
    return el('form', {'onSubmit': handleSubmit},          # createElement + camelCase string dict
              el('input', {'id': 'editBox',
                           'onChange': handleChange,
                           'value': newItem}))
```

The same component in PythScribe imports natively, is written in **snake_case** (converted to
`useState`/`onChange` at compile), uses **PSX** (elements are calls, props are named kwargs),
and reads the event by **attribute**:

```python
# PythScribe — Python
from pyths.react import component, use_state
@component
def App():
    new_item, set_new_item = use_state("")     # snake_case → useState at compile
    def handle_change(event):
        set_new_item(event.target.value)       # attribute access
    return form(on_submit=handle_submit,                    # PSX: on_submit → onSubmit
                input(id="editBox", on_change=handle_change, value=new_item))
```

Both support Python builtins and stdlib (`len`, `list(...)`, `.append`/`.index`,
comprehensions) — but here too the models diverge: PythScribe binds these to **native
CPython semantics** (arbitrary-precision `int`, `KeyError` on a missing key, code-point
strings, banker's rounding — machine-checked/differential-tested), whereas Transcrypt's
interop surface carries **JavaScript semantics** unless you opt into a Python idiom. Net:
Transcrypt's surface is *JS names and JS objects wearing Python syntax*; PythScribe keeps the
surface **Python syntax** and the behavior **CPython-faithful** — converting names and
implementing semantics for you. In an era where code is generated more than hand-typed, that is
the payoff: the **syntax** stays readable Python for *review* (we review far more than we author),
and the **semantics** stay faithful enough to *trust* what was generated.

## Components, naming, and the Python data layer

PythScribe doesn't blanket-rename identifiers; it applies **one three-way rule** that keeps
Python-shaped code Python while leaving real JS APIs untouched:

1. **React import names + JSX/PSX props** convert snake_case → camelCase (`use_state`→`useState`,
   `on_click`→`onClick`, `class_name`→`className`).
2. **Python builtin methods** lower to their JS equivalent (`s.strip()`→`s.trim()`,
   `xs.append(x)`→`xs.push(x)`).
3. **DOM / third-party-JS methods** are emitted **verbatim** — you write the real API name
   (`e.preventDefault()`, `el.addEventListener(...)`, `query.invalidateQueries(...)`); there is no
   snake_case form for these.

So hooks, props, and builtins read as Python while genuine JS APIs stay exactly themselves — no
guessing, no accidental renames. (Transcrypt handles the same clash with build-time aliases —
`py_split`/`js_split`, `__pragma__('alias', …)` — and otherwise expects the JS name.)

Because PythScribe recognizes **10 stdlib modules natively and CPython-faithfully** — `collections`,
`itertools`, `functools`, `math`, `json`, `random`, `datetime`, `re`, and **exact** `decimal`/
`fractions` — you can do real Python data-wrangling and feed the result straight into PSX, in one
idiom:

```python
from collections import Counter

@component
def TagCloud(posts):
    counts = Counter(tag for p in posts for tag in p.tags)      # Python stdlib
    return div(class_name="cloud",
        *[span(class_name="tag", f"{t} ({n})") for t, n in counts.most_common()])
```

Transcrypt ships a **smaller, partial** stdlib: its Python builtins (`len`, list/dict methods,
comprehensions) are present, but the breadth (`collections`/`functools`) and the fidelity (exact
`Decimal`/`Fraction`) are PythScribe's — heavier data layers in Transcrypt more often fall back to
JavaScript.

Finally, a PythScribe component is a `def` tagged **`@component`** (helper views use `@psx`). This is
idiomatic Python — decorators-as-markers, like `@dataclass`/`@property` — and it separates components
from ordinary functions **both ways**: *visually*, a reader (or a reviewer of AI-generated code) sees
at a glance which functions are components; *in the compiler*, `@component` lowers to `memo(function
…)`, so the marker also **applies React semantics** (memoization + the hooks contract), not just a
label. Transcrypt has no such marker — a component is a plain `def App():` that returns
`createElement(...)`, indistinguishable from any other function until you reach the call site.

## The `.psc` / LOIR layer (PythScribe-only)

PythScribe adds an optional compressed, model-facing source superset — `.psc`, an
LLM-Oriented IR — joined to canonical `.ps` by a deterministic expander whose core rewrite
properties are machine-checked in Lean (the Iron Rule: `canonicalize(expand(x)) ==
canonicalize(x)`). Transcrypt has no compression or model-facing layer.

## Assurance

Transcrypt's assurance is its autotester (back-to-back CPython/JS tests) — solid
engineering, but no formal component. PythScribe adds a **per-compilation routing
certificate**, a 26,483-line **Lean** development (routing safety, selected preservation
fragments, runtime-helper specs, `i64` semantics, naming soundness), a differential corpus
(cross-checked on a second JS engine), and a **trust manifest** enumerating what is proved
vs tested vs trusted.

### Cross-checked against Transcrypt's own autotester

A fair, concrete test of the conformance-by-default claim is to take Transcrypt's *own*
autotester corpus — the back-to-back CPython/JS suite Transcrypt ships to validate itself —
and run it through PythScribe as a PythScribe↔CPython differential. Running it is only
*possible* because Transcrypt built a CPython-differential suite in the first place; the
point below is narrow and additive, not a knock on that engineering.

- **Language surface — 45/45 single-file testlets byte-equal to CPython** (2,328/2,328
  individual checks). These are the features the suite is built to stress: multiple
  inheritance, properties, decorators, generators, operator overloading, comprehensions,
  closures, exceptions, f-strings, slicing, and async/await (`asyncio.run`/`gather`/`sleep`).
- **Full surface — 47/60 ported-clean** across the entire autotester (single-file +
  multi-file + raw-JS), with **13 measured boundaries** (metaclasses, JS proxies,
  `globals()`, multi-file package re-export, and by-design non-byte-equal properties such as
  PRNG streams) and 4 excluded (DOM/AJAX/external-build demos with no CPython-runnable
  differential). Every boundary is *named and measured*, not silently skipped.
- **Differentiation — 3/3.** On the three testlet cases where Transcrypt's output and
  CPython genuinely diverge, PythScribe matches CPython on all three: `-3 % 8` → `5` (Python
  modulo, not JS `-3`); a runtime-branch that prints `Using CPython`; and `x in SomeClass`
  on a non-container → CPython's `TypeError` (Transcrypt's attribute-membership model returns
  a truthy value). This is the conformance-first stance made concrete — where the two peers
  part ways, PythScribe is the one that tracks CPython.

Separately, a hand-authored **multi-file package-import differential corpus** (relative
imports, cross-module inheritance, subpackages, re-export, diamonds/module-singletons,
cross-module dataclasses) runs **13/13 byte-equal to CPython**. Relative-import packages are
the supported multi-file model; the current first-party gap is that `pyths run` executes a
single entry module rather than walking the package graph (`pyths bundle` is the multi-file
path and inlines the graph).

## Summary

| Axis | Transcrypt | PythScribe |
|---|---|---|
| Architecture | AOT Python→JS, no interpreter | AOT Python→JS **+ WASM auto-routing**, no interpreter |
| Default stance | **Performance**; Python semantics opt-in (`opov`/`tconv`) | **Conformance**; Python semantics by default |
| Numbers | JS doubles (unified with JS types) | Exact hybrid `Number`/`BigInt` |
| Guiding property | **Isomorphism** (readable, JS mirrors Python) | **Semantic fidelity** (behaves like CPython) |
| Typing | Dynamic inside, static at boundaries (mypy) | Built-in inference + checker, first-class static |
| TypeScript output | JS only, no `.d.ts` | Emits `.d.ts` declarations alongside JS |
| Debugging | Source maps, line annotations, inline-JS | Source maps, `--explain`, Python-named errors |
| Frontend | React as a JS library (createElement, camelCase) | `@component`/PSX, snake_case, Next.js, 30+ libs |
| State mgmt | Redux via community wrappers (no JSX); Zustand raw interop only; no type stubs | Redux Toolkit + react-redux + Zustand native, `.pyi`-stubbed, dual-track-tested |
| Compression | — | `.psc` LOIR + verified expander |
| Assurance | Autotester (back-to-back) | + routing certificate, Lean proofs, trust manifest |
| Cross-conformance | — | Passes **Transcrypt's own autotester** vs CPython: 45/45 language surface, 47/60 full, **3/3** on the cases where Transcrypt and CPython diverge |
| Maturity | Mature, years in production | Newer |
| License | Apache-2.0 | FSL-1.1-ALv2 |

**Honest trade-off:** Transcrypt is mature and its default JS-native operations are faster;
PythScribe trades that for Python fidelity by default (a runtime-helper cost that WASM
offsets on numeric paths). Pick Transcrypt when isomorphic, fast-by-default output and JS
interop dominate; pick PythScribe when Python semantics, a React/Next idiom, and provable
scope matter — especially for LLM-generated code, where silent semantic drift is a hidden
failure mode.

## Sources

- [PythScribe — *Layered Assurance for an Agent-Written Python-to-JavaScript/WebAssembly Compiler*](https://doi.org/10.5281/zenodo.21875694) (the head-to-head steady-state Livermore benchmark and full protocol behind the conformance-vs-performance claims here — Transcrypt `opov`-global/scoped vs PythScribe's auto-routed JS+WASM — are in its Appendix C).
- [Transcrypt: design requirements and architecture — InfoQ](https://www.infoq.com/articles/transcrypt-python-javascript-compiler/) (the author's design article: the three demands, isomorphism, conformance-vs-performance pragmas, type unification, hybrid typing).
- [TranscryptOrg/Transcrypt — GitHub](https://github.com/TranscryptOrg/Transcrypt) (README: features, source maps, Apache-2.0; layout `transcrypt/modules/org/transcrypt`).
- [Autotesting Transcrypt code — Transcrypt docs](https://www.transcrypt.org/docs/html/autotesting_transcrypt.html) (`development/automated_tests/transcrypt`, `AutoTester.check()`, back-to-back CPython testing).
- [Special facilities — Transcrypt docs](https://www.transcrypt.org/docs/html/special_facilities.html) (`__pragma__('opov')`, `__pragma__('tconv')`, aliases).
