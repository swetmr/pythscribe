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
  output sequence) is the same methodology as PythScribe's 1,318-program CPython
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

## Debuggability and source maps

Both generate source maps for source-level breakpoints and stepping. Transcrypt adds
optional **source-line annotations** in the output and an **inline-JavaScript escape
hatch** (native JS at any point via a directive). PythScribe adds **`pyths run --explain`**
(a Python-style explanation paragraph above any crash) and **Python-named runtime errors**
with CPython-matching message text, source-mapped to `.ps` line:col.

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

## The `.psc` / LOIR layer (PythScribe-only)

PythScribe adds an optional compressed, model-facing source superset — `.psc`, an
LLM-Oriented IR — joined to canonical `.ps` by a deterministic expander whose core rewrite
properties are machine-checked in Lean (the Iron Rule: `canonicalize(expand(x)) ==
canonicalize(x)`). Transcrypt has no compression or model-facing layer.

## Assurance

Transcrypt's assurance is its autotester (back-to-back CPython/JS tests) — solid
engineering, but no formal component. PythScribe adds a **per-compilation routing
certificate**, an 18,627-line **Lean** development (routing safety, selected preservation
fragments, runtime-helper specs, `i64` semantics, naming soundness), a differential corpus
(cross-checked on a second JS engine), and a **trust manifest** enumerating what is proved
vs tested vs trusted.

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
