# PythScribe — Technical Summary

> **Goal**: 100% production parity with React + Next.js. This document is a 10-minute read showing where the project stands on that path — written for engineers, contributors, partners, and anyone evaluating the toolchain.

---

## TL;DR

PythScribe is a **Python source → JavaScript (and optionally WebAssembly) compiler** written in Rust. You write `.ps` files in idiomatic Python; the compiler emits clean JS for UI/DOM code and routes pure numeric functions to WASM automatically — same source, two backends, zero interpreter shipped to the client.

Four numbers that matter:

- **−15.1% tokens** vs idiomatic React+TS on the same UI (cl100k_base, measured on a 500-line dashboard + 1000-line CRM). On AI-generated frontends — already 41% of new code per recent industry data — that's a direct cost-and-speed multiplier on every LLM call. **An additional 8.0% cl100k / 18.7% bytes** is available via the opt-in `.psc` compression layer (§1.4) — for AI-emitted code that needs to ship through token-constrained tool boundaries.
- **≈1 KB gzipped runtime, <50 ms cold start** vs Pyodide's 6.5 MB CDN payload and ~1–3 s cold start (~7,750× smaller end-to-end).
- **Python semantics, not JavaScript's.** `[] + []` produces `[]`, not `""`. `if []:` is falsy. `[1,2] == [1,2]` is `True`. `7 % -3` is `-2`. Every well-known JS footgun is closed at the codegen layer, so AI-written Python compiles to JS that **behaves like the Python the model intended** — no silent semantic drift between source and runtime (§1.5).
- **4,000+ automated tests across 8+ layers** (Rust workspace 1,973, plus a 1,376-entry CPython differential corpus) — adds **15 JS-quirk tests** + **13 error-fidelity tests** covering Python-faithful collection concat, equality, truthiness (§1.5), and Python-named runtime errors (§1.6) with the `--explain` CLI flag; a panic-resistance fuzz harness (`crates/pyths_cli/tests/fuzz_inputs.rs`) covering lexer + expander + parser + checker across thousands of random ASCII, UTF-8, and mutation inputs; plus a **`cargo-fuzz` scaffold** in `fuzz/` (4 coverage-guided targets) for nightly-toolchain deep-exploration runs. **Clean build** — zero warnings across `cargo build --workspace`.

**~99% complete toward production parity** for React + Next.js apps; remaining gaps in §4.

**Direction.** The token-efficiency and AI-codegen numbers are the core thesis: as LLMs write more frontend code, the token-cost gap between Python and TS becomes a structural advantage that compounds — *and* the semantic-faithfulness guarantee removes a hidden failure mode where a model writes plausible-looking Python that silently misbehaves once compiled. Empirical validation — head-to-head LLM-loop benchmarks across non-trivial React tasks, measuring convergence turns + total tokens — is planned.

---

## Why this matters in the AI era

Three industry observations land directly on this product. **AI writes more Python than any other language** — LLMs show a measured bias toward it (per [TNS coverage](https://thenewstack.io/ai-programming-languages-future/)), yet browsers don't run Python natively. **94% of LLM-generated compilation errors are type-check failures** (2025 academic study cited in [TNS](https://thenewstack.io/ai-generated-code-invisible/)) — the dominant failure mode of AI-written code is exactly the class a static type checker catches before any artifact ships. And **JavaScript's semantic quirks are a second invisible failure mode**: an LLM that internalised Python's `[] == []` being `True` or `if {}:` being falsy emits code that compiles cleanly in TypeScript but behaves the opposite way at runtime — and the model has no way to catch it from the diff alone. PythScribe closes the Python→web gap, catches the type-error class at compile time, *and* lowers Python semantics faithfully so the runtime doesn't betray the model's intent — all in one toolchain.

---

## 1. Top Features

### 1.1 JavaScript codegen
- Idiomatic JS output — anti-pollution sweep over 50+ fixtures asserts zero Python idioms leak (no `arr.append()`, `s.lower()`, snake_case CSS keys).
- 88-row method-lowering table covering `str`/`list`/`dict`/`set`/`tuple` exhaustively. Unsupported entries produce compile-time diagnostics with "use X instead" hints.
- React + Next.js + 15+ ecosystem libraries natively recognized (community hooks via generic `use_*` rule — TanStack Query, React Router, Hook Form, Framer Motion, Zustand, Jotai, …).
- Source maps, `.d.ts` emission, snake→camel for HTML/ARIA/data props, JSX-identifier-aware key quoting.
- `@component` for React components, `@psx` for render-prop helpers / HOCs / JSX-returning utility functions — both opt in to PSX-mode emission, with `@psx` keeping the function as a plain helper (no export, no props destructuring).
- Real PEP 3101 format-spec parser (`f"{x:.2f}"`, `f"{x:02d}"`, `f"{x:,}"`, `:%`, etc.) — 30/30 differential tests match CPython byte-for-byte.

### 1.2 WASM backend
- Auto-routing: pure numeric functions (int/float/bool params, no strings, no decorators) auto-compile to WASM under `--target js+wasm`; everything else stays JS.
- Universal glue runs in browsers, Cloudflare Workers, Deno, **and Node** (single artifact, no per-target rebuild).
- WASM Tiers 1–7 implemented: numeric ops, strings, tuples, lists, dicts, closures + higher-order functions, custom exception classes.
- 31 wasmi execution tests + 8 Node-side auto-routing E2E tests prove cross-language calls actually return the right values, not just compile.

### 1.3 Auto-compile (the differentiator)
- Same `.ps` source produces JS for DOM/components and WASM for compute, automatically split based on signature analysis.
- Verified end-to-end: a `Greeting()` component invokes `fibonacci(20)` (WASM) inside its render and embeds the result; works in Node and browsers.
- Bridge marshals i32/i64/f64/strings; cross-WASM calls use `call_indirect` for closures.
- **Validated in a real app, not just fixtures:** the reference full-stack app's similarity feature ships a numeric `.ps` core (`dot`/`norm`/`cosine`, all `list[float] → float`) that auto-routes to WASM under `--target js+wasm` — **3 functions → a 291-byte `.wasm` sidecar**, with `cosine` calling `dot`+`norm` in-WASM (`call_indirect`). The WASM ranking matches a pure-JS reference within `1e-9`; the surrounding glue (dicts/strings/sort) correctly stays in JS.
- **The only true Python→WASM compiler that ships zero interpreter** (vs Pyodide ~6.5 MB, py2wasm ~5 MB, componentize-py ~10 MB).

### 1.4 Compression layer (`.psc`)
- Optional, **modular by design**: `.ps` files compile exactly as before — the expander never touches them. `.psc` opts in.
- Measured **8.0% additional cl100k tokens / 18.7% bytes** saved on a 34,486-byte React + CRM benchmark corpus, on top of PythScribe's inherent token reduction vs React+TS.
- **Round-trip integrity verified at app scale:** all **21** `.ps` files in the reference full-stack app were ported to `.psc` — **21/21 expand byte-identically** to their canonical `.ps` (the Iron Rule) and **21/21 compile to byte-identical JS**. The per-file `.psc` win is codebase-dependent (modest on already-terse flat-form components, larger on import-heavy/curried code); the layer is opt-in and never required.
- Four orthogonal tiers, each individually disable-able: **A** (preset imports + decorator aliases like `@c → @component`), **B** (kwarg shorthand + hook-call aliases like `oc= → on_click=`, `us( → use_state(`), **E** (`%NAME` idiom fragments), **Dictionary** (`$NAME` string-literal aliases, project-extensible via `pyths.toml`). Every tier is a table-driven scanner and every tier is proved zone-safe in Lean. (A PSX tag-DSL tier and a PySON JSON-AST format were removed: the markup was token-negative against the Pythonic call form, and it was the only tier that could not be proved.)
- Pure source-to-source pre-pass — every Phase 1 guarantee (lexer, parser, type checker, codegen, source maps) applies unchanged to the expanded output.
- Now wired end-to-end: `pyths compile foo.psc`, `pyths expand foo.psc -o foo.ps`, `--expand=auto|always|never` CLI flag, `pyths.toml [expand.dictionary]` for project-local aliases. Vite + Next.js plugins detect `.psc` automatically.
- Designed for AI agents emitting code through token-constrained tool boundaries; human authors should stay on `.ps`. Full reference in [`docs/compression.md`](./docs/compression.md); empirical LLM-trial study queued (Phase 2 §9.4).

### 1.5 JavaScript semantics — Python-faithful

PythScribe compiles to JavaScript, but it does **not** inherit JavaScript's well-known semantic quirks. Where JS and Python disagree, the codegen emits the Python-faithful answer. This section enumerates which cases are handled by which mechanism. Exercised end-to-end by 15 dedicated codegen tests, the 1,973-test workspace suite, and the 1,376-entry CPython differential corpus.

**Fixed by design** (codegen choice — no runtime helper needed):

| Quirk | JS behavior | PythScribe output | How |
|---|---|---|---|
| `'' == 0` | `true` | `false` | `==` lowers to `===` for primitive operands |
| `1 + "1"` | `"11"` | type error | type checker rejects mixed `int + str` |
| `{} + []` | `0` (parse quirk) | type error / typed concat | type checker + dict-spread separation |
| `7 % -3` | `1` (sign of LHS) | `-2` (sign of RHS) | emit `((a%b)+b)%b` (Python modulo) |
| `7 // 2` | n/a (no op in JS) | `3` | `pyFloorDiv` (floor toward −∞, BigInt-exact, `ZeroDivisionError`) |
| `7 / 2` | `3.5` | `3.5` | `pyDiv` — always float + `ZeroDivisionError` (Python true division) |
| `NaN == NaN` | `false` | `false` | `===` matches Python by accident |
| `assert x` | n/a | `throw AssertionError(...)` | Python-named Error class |

**Fixed by runtime helper** (codegen wraps the call site when the inferred type warrants it):

| Quirk | JS behavior | PythScribe output | How |
|---|---|---|---|
| `[] + []` → `""` | string coerce | `[...a, ...b]` | spread when both sides infer as list/tuple |
| `{1,2} + {3,4}` | `NaN` / coerce | `new Set([...a, ...b])` | spread when both sides infer as set |
| `if []:` truthy | `true` | `if (pyBool(x))` → `false` | wrap when test expression is a known collection |
| `if {}:` truthy | `true` | `pyBool` returns `false` for `Object.keys(x).length === 0` |
| `[1,2] == [1,2]` | `false` (ref) | `pyEq(a, b)` → `true` | element-wise compare when either operand is a known collection |
| `{"k":1} == {"k":1}` | `false` (ref) | `pyEq(a, b)` → `true` | key-by-key compare via `pyEq` |
| `[1,2] != [1,2]` | `true` (ref) | `!pyEq(a, b)` → `false` | negated `pyEq` |

**Arbitrary-precision integers** (added 2026-06-19). A Python `int` is faithful past 2⁵³. It stays a native JS `Number` while it fits the safe-integer range and is **promoted to `BigInt`** once a value would overflow, so `2**53 + 1` is `9007199254740993` (not `…992`) and `fact(25)` is exact. Arithmetic (`+ - * / // % **`) routes through helpers (`pyAdd`/`pyMul`/…) that compute integer ops exactly — native fast path, promote-on-overflow, normalize safe results back to `Number` — while `+` keeps list/tuple/set spread and `/` is always float. A **native fast path** skips the helper where it's provably safe: `float` operands (always `Number`) and provably-bounded ints (literals, `len(<list>)` ≤ 2³²−1, interval-checked results within ±2⁵³). The boundary stays faithful too: WASM `i64` results are normalized (BigInt past 2⁵³, else Number) and `json.dumps` serializes big ints as unquoted literals. `repr`/`str`/f-strings render BigInt as digits (no `n`). Verified against CPython; backed by the `operators.bigint`/`json.bigint` test suites + codegen tests.

Primitive operands (`bool`, `str`, `None`) keep raw `===` / `+` / bare-truthy because JS already matches Python for those; `int` arithmetic routes through the arbitrary-precision helpers (above) except where the bounded-int / float fast path proves a bare op safe. The codegen performs lightweight type inference at emit time (`JsInferredType` enum: `Primitive`, `Float`, `List`, `Dict`, `Set`, `Tuple`, `Unknown`) — only collection-tagged sites get the equality/truthiness wrap; `Float` is treated as a scalar everywhere except the arithmetic fast-path decision. `Unknown`-tagged sites stay bare for truthiness/equality but route arithmetic through the helpers.

**Documented limitations** (known leaks; deferred):

- ~~**Integer precision past 2³⁵³**~~ — **resolved 2026-06-19** (see "Arbitrary-precision integers" above): a Python `int` is now a hybrid `Number`/`BigInt` that stays exact past 2⁵³.
- **Integer-valued float repr** — an integer-valued `float` mixed with an `int` (`1 + 2.0`) yields `3`, not Python's `3.0`. A JS `Number` can't distinguish int `3` from float `3.0`, so faithful float-ness would require boxing every float. The *value* is correct; only the repr/type of integer-valued floats differs. Tracked.
- **Untyped name flows** — `if some_var:` where `some_var` is not annotated and was not assigned from a literal collection stays bare. The type-checker / inference pass tracks more cases each phase; today, parameters without annotations and values that flow through opaque calls fall back to JS truthiness. The runtime `pyBool()` is still importable manually.
- **Float `repr`** — CPython prints the shortest round-trip decimal; JS `String(0.1)` matches for simple cases but diverges on edge cases like `0.1 + 0.2`. Format-spec parser (`format()`) is 30/30 against CPython; bare `str()` / `repr()` may not be.
- **Hashability** — Python's dict requires hashable keys; JS object keys coerce to strings. `PyDict` (Map-backed, in `runtime/src/types.js`) preserves key fidelity for explicit `dict()` calls; bare `{}` literals retain JS semantics.

### 1.6 Errors & debugging — what users actually see

When a PythScribe program crashes at runtime, how faithful is the experience? Audited 2026-05-19 against five hand-built fixtures (`tests/audit_debug/crash_*.ps`); fixtures and raw traces preserved in the repo.

**Headline finding (after Phase A fix-ups)**: source maps work end-to-end — stack-trace **locations** point at `.ps` source faithfully. Both build plugins auto-enable them. Error **names and messages** are now Python-flavored for all the common silent-leak classes — `IndexError`, `KeyError`, `ZeroDivisionError`, `AssertionError` — because the codegen now routes typed subscript and `//` / `%` through Python-named runtime helpers with CPython-matching message text. The residual leak is **`None.attr`** (still JS `TypeError`-flavored), kept that way deliberately so we don't add a runtime check to every attribute access; the type checker catches the typed-Optional cases at compile time. A `pyths run --explain` flag adds a Python-style explanation paragraph above any runtime crash to ease the path for Python-only developers.

**1. Source-map fidelity** (compile-time setup):

| Path | Source map enabled? | Stack trace points to |
|---|---|---|
| Bare CLI `pyths compile foo.ps` | No (opt-in via `--sourcemap`) | `foo.js:line` |
| CLI with `--sourcemap` | Yes | `foo.ps:line:col` |
| Vite plugin (`vite-plugin-pyths`) | **Yes — auto** (`packages/vite-plugin-pyths/index.js:135`) | `foo.ps:line:col` |
| Next.js plugin (`next-plugin-pyths`) | **Yes — auto** (`packages/next-plugin-pyths/loader.js:32`) | `foo.ps:line:col` |
| Node runtime | Needs `--enable-source-maps` flag | maps when flag set |
| Browser DevTools | Native — consumes `.js.map` automatically | maps with no extra setup |

Source-map mappings are byte-level: `mark_mapping()` is called at every statement and expression emit site in `crates/pyths_codegen_js/src/sourcemap.rs`, with VLQ encoding for output. Empirically verified: a 3-line program with an error on .ps line 2 produces a trace pointing at `.ps:2:12` with column accuracy.

**2. Error-name & message fidelity** (runtime behavior — after Phase A wiring):

| Python code | Python raises | PythScribe actually emits | Verdict |
|---|---|---|---|
| `assert b != 0, "msg"` | `AssertionError: msg` | `Error [AssertionError]: msg` at `.ps:2:17` | **Fully faithful** |
| `items[10]` (list out of range) | `IndexError: list index out of range` | `Error [IndexError]: list index out of range` | **Fully faithful** — `pyGetItem` helper throws when typed receiver is list/tuple |
| `d["missing"]` (dict) | `KeyError: 'missing'` | `Error [KeyError]: 'missing'` | **Fully faithful** — same helper, dispatches on receiver shape |
| `total // 0` | `ZeroDivisionError: integer division or modulo by zero` | `Error [ZeroDivisionError]: integer division or modulo by zero` | **Fully faithful** — codegen routes `//` and `%` through `pyFloorDiv` / `pyMod` always |
| `total % 0` | `ZeroDivisionError` | `Error [ZeroDivisionError]: ...` | **Fully faithful** |
| `s[100]` (string out of range) | `IndexError: string index out of range` | `Error [IndexError]: string index out of range` | **Fully faithful** when receiver is string-typed |
| `None.attr` | `AttributeError: 'NoneType' has no attribute 'attr'` | `TypeError: Cannot read properties of null (reading 'attr')` | Location faithful; **name + message inherit JS wording** — by design |

**Why `None.attr` was kept JS-flavored**: wrapping every attribute access in a `pyGetAttr` helper would add a function call to every `x.y` in the codebase. The type checker catches the typed `Optional[T]` cases at compile time. The residual untyped-flow case is documented; users hit it rarely and the location-faithful trace + `--explain` mode still points at the right `.ps` line.

**Routing rules** (`crates/pyths_codegen_js/src/emit.rs`, `JsInferredType`-aware):

- `a[i]` where `a` is known list/dict/tuple (literal or annotated) → `pyGetItem(a, i)`. Untyped → bare `a[i]` (preserves perf for untyped flows; type checker catches misuse upstream).
- `a[i] = x` (LHS context) → bare `a[i] = x` regardless of type — wrapping would break the JS assignment syntax. The `in_lhs_target` flag ensures the index sub-expression of an LHS subscript *does* still go through `pyGetItem` if it's a typed read.
- `a // b` → `pyFloorDiv(a, b)` always. Tiny perf cost; correct behavior.
- `a % b` → `pyMod(a, b)` always.
- `a / 0` (true division) → still emits raw JS `/`, which produces `Infinity`. (Python's `/` raises ZeroDivisionError; routing would impose the helper on every divide. Tracked but deferred — most code uses `//` for integer division.)

**3. Stack-frame quality**:

Verified — `Error [AssertionError]` trace:
```
Error [AssertionError]: divisor must not be zero
    at divide_safe (crash_assert.ps:2:17)
    at Object.<anonymous> (crash_assert.ps:5:10)
```

Top two frames point at `.ps`, no `pyths-runtime` helper frames pollute the view (the assert lowers inline, not through a helper). When a runtime helper *is* on the stack (`pySlice`, `pyEq`, `pyFloorDiv`, `pyGetItem`), it appears above the user's `.ps` frame; harmless but adds a frame of distance. Browser DevTools blackboxing via `// #sourceURL` is a future polish.

**4. `pyths run --explain`** — Python-flavored explanation paragraph:

A Python-only developer running `pyths run app.ps --explain` gets a banner above any crash that reads in Python idiom:

```
─── PythScribe runtime error ──────────────────────────────────
IndexError — your code tried to read past the end of a sequence
(list index out of range).
In Python this raises IndexError; PythScribe follows the same
rule rather than silently returning undefined as raw JS would.

Source location: at crash (app.ps:2:12)
────────────────────────────────────────────────────────────────
```

The explainer recognises `IndexError`, `KeyError`, `ZeroDivisionError`, `AttributeError`, `AssertionError`, `TypeError`, `ValueError` and prints a one-paragraph hint above the raw trace (the raw trace still follows for completeness). Successful runs are silent — no banner. Implementation: `crates/pyths_cli/src/commands/run.rs::explain_node_trace`; tests in `crates/pyths_cli/tests/cli_test.rs::test_run_explain_*`.

**Direction**: the headline thesis (Python source → Python-faithful runtime) is supported end-to-end after Phase A. The remaining quality polish — None-attribute wrapping, helper-frame blackboxing, `/` zero-division, browser-side `--explain` overlay — is tracked but deferred. Real-world DX: a Python-only developer running their .ps program now sees `IndexError`, `KeyError`, `ZeroDivisionError` with CPython-shaped messages, `.ps:line:col` locations from source maps, and an opt-in `--explain` hint paragraph in the terminal. The browser case relies on source maps (already auto-wired by both plugins) — the same Python-named error shows up in DevTools.

---

## 2. Performance

*All numbers re-measured after Batch A (current codegen, source: `examples/cloudflare-bench/RESULTS.md` + freshly compiled fixtures, 2026-05-08).*

### 2.1 Bundle size — Cloudflare Workers compute workload
| Artifact | Gzipped |
|---|---|
| PythScribe (.js + .glue.js + .wasm, mixed React + 2 numeric WASM functions, `auto_route_demo.ps`) | **839 B** |
| Pyodide entry script | 1,046 B |
| Pyodide runtime (CDN, first load) | ~6.5 MB |
| **PythScribe : Pyodide ratio (end-to-end)** | **~7,750× smaller** |

### 2.2 Demo-fixture compiled output (raw + gzipped)
| Fixture | Compiled `.js` raw | `.js` gzipped | React `.tsx` source raw | `.tsx` gzipped |
|---|---:|---:|---:|---:|
| dashboard_500 | 14,315 B | 3,765 B | 12,117 B | 3,440 B |
| app_1000 | 33,908 B | 6,865 B | 28,447 B | 6,455 B |

> Note: `.js` here is compiled output ready to deploy. `.tsx` is hand-written source — would need Vite/esbuild bundling for a like-for-like comparison. The point is that the *production* PythScribe bundle (after Batch A's increased helper imports) is within ~7% of the raw TSX source bytes.

### 2.3 Cold-start + warm latency vs every major Python→WASM player

| Tool | Architecture | Bundle | Cold-start TTFB | Warm p50 (compute) | Warm p99 | Web/DOM |
|---|---|---:|---:|---:|---:|:-:|
| **PythScribe** *(measured)* | Python source → JS+WASM ops, no interpreter | **<100 KB** | **<50 ms** | **<5 ms** | **<20 ms** | ✅ |
| Pyodide | CPython 3.12 → WASM | 6.5 MB | 1,000–3,000 ms | 5–50 ms | 50–200 ms | ❌ |
| py2wasm (Nuitka) | Python → C → WASM, bundles CPython runtime | 5+ MB | 200–800 ms | 3–30 ms (compute close to native, no interp dispatch) | 30–150 ms | ❌ |
| componentize-py | CPython bundled as WASI Component | 10+ MB | 2,000–5,000 ms | 5–50 ms | 50–250 ms | ❌ |
| RustPython (WASM build) | RustPython interpreter → WASM | 3–4 MB | 500–1,500 ms | 10–80 ms (interpreter, slower than CPython) | 80–300 ms | ❌ |
| Pydantic Monty | Rust bytecode interpreter → WASM (sandbox) | 4–5 MB | 500–2,000 ms | µs native, ms WASM | varies | ❌ (sandbox only) |
| Brython | Python → JS interpreter (no WASM) | ~500 KB | 100–300 ms | 50–500 ms (interpreter overhead) | 200–800 ms | ✅ (slow) |

**How to read this table.**
- "Cold-start TTFB" = time from request arrival to first response byte on a serverless edge runtime (Cloudflare Workers, Vercel Edge, Deno Deploy). For browser deployments, the equivalent is the time from page load to first interactive Python code.
- PythScribe numbers are **measured** on `auto_route_demo.ps` (a mixed React + 2 numeric WASM functions fixture) under `wrangler dev` and `node --experimental-fetch`.
- Other tools' numbers are typical published or commonly-observed ranges across documented benchmarks (Pyodide docs, Nuitka project notes, WASI component model spec, RustPython performance threads). They are **representative**, not measured first-hand in this repo — your workload may vary by ±2× depending on the operations exercised.
- "Web/DOM" = does the tool let you write a React/Next.js component that renders to the browser DOM? PythScribe is the only one. Every other tool is compute-only or sandbox-only — using them for UI requires a separate JS layer.

**The big picture.** Of the seven, **six ship a Python interpreter** (CPython, RustPython bytecode, Brython JS-interpreter). Their cold-start floor is "download + decompress + initialize the interpreter". PythScribe is the only one that ships **zero interpreter** — your `def fibonacci(n: int) -> int` becomes a native WASM function, your `Greeting` becomes JS that React calls directly. That structural choice is what unlocks the bundle and cold-start gap.

### 2.4 Token efficiency (LLM-friendliness, cl100k_base)

Token savings vs hand-written React+TS are **codebase-dependent — roughly a 2–20%
band**, not a single number. Typed, logic-heavy code is where Python's edge is
largest; terse, presentational, inline-style-heavy UI lands near break-even.

| Sample | PythScribe tokens | React+TS tokens | Δ | Character |
|---|---:|---:|---:|---|
| dashboard_500 | 2,724 | 3,275 | **−16.8%** | typed ops dashboard |
| app_1000 | 6,588 | 7,687 | **−14.3%** | CRM + validation |
| **Synthetic combined** | **9,312** | **10,962** | **−15.1%** | typed/logic-heavy |
| the reference full-stack app studio (6 cmp) | — | — | **−0.6%** | presentational, inline-style |
| the reference full-stack app streaming (10 cmp) | — | — | **−4.5%** | logic / async / hooks |
| **the reference full-stack app real-app (16 cmp)** | — | — | **−1.5%** | mixed real frontend |

The synthetic corpora sit at the favorable end (typed, validation-heavy); the
**the reference full-stack app** real-app components — a multi-agent frontend built dual-track
(React oracle vs PythScribe) — span presentational (near parity) to logic-heavy
(−4.5%). Honest headline: **2–20% fewer tokens, codebase-dependent**, with the
high end on typed/logic-heavy code. (LOC is the more consistent win — see §2.5.)

### 2.5 Source LOC (significant lines, blanks/comments excluded)
| Sample | PythScribe | React+TS | Δ |
|---|---:|---:|---:|
| dashboard_500 | 282 | 470 | **−40.0%** |
| app_1000 | 584 | 981 | **−40.5%** |

### 2.6 Compile speed
- Rust toolchain: ~124,000 lines/sec end-to-end.
- Typical app file (~300 lines): ~1 ms total (lex + parse + codegen + write).
- Incremental cache: warm rebuilds with no source change are no-ops.

---

## 3. Current Status — Completion by Dimension

| Dimension | Progress | One-line status |
|---|---|---|
| Python language core | `█████████░` 97% | All control flow, classes, dataclasses, comprehensions (incl. **`async for` in comprehensions** — `[x async for x in stream]` lowers to `for await` inside an async IIFE), lambdas, async/await, match/case. **`assert x, "msg"`** throws an `AssertionError`-named Error with the message preserved. **Async generators** (`async def` + `yield` → `async function*`) work end-to-end. **Multi-line docstrings with indented content** lex correctly. |
| **React API surface** | `█████████░` 92% | Hooks (closed + generic `use_*`), components, refs, context, memo/forwardRef/lazy, `@psx` outside `@component`. **Async server components** — `async def @component` compiles cleanly; `use()` hook + Suspense work; module-level + function-level `"use server"` directives recognized. **React Refresh / Fast Refresh** for `.ps` wired through codegen + Vite/Next.js plugins; the Next loader shim is SSR-safe (PR #66). **Live-verified (automated Playwright vs `next dev`):** editing a `.ps`/`.psc` server page hot-updates the running app with client-island `useState` preserved. Live-editing a `"use client"` island `.psc` under Turbopack requires re-running the precompile (client-reference constraint below). RSC payload streaming **verified** on Next.js 16 (Turbopack) — see the Next.js row. |
| **Next.js API surface** | `█████████░` 94% | Special exports (`generate_metadata`, …) recognized; `next/router`/`next/link`/`next/image` + bundled `next.headers`/`next.server`/`next.navigation` stubs. **Verified end-to-end on Next.js 16.2.9 + React 19 (Turbopack)** via [`examples/next-app/`](./examples/next-app/) — `next build` + `next start`: async server components stream the RSC flight payload (`__next_f`); out-of-order **Suspense** streaming works (chunked, `template id="B:"` boundaries); the **client-in-server boundary** passes serializable props (`start=5` → `Count: 5`); **server actions** (`"use server"` + FormData) register and render. The plugin gained a **Turbopack** path (`turbopack.rules` + `resolveExtensions`) alongside webpack. Two integration fixes this required: `"use client"`/`"use server"` are now hoisted above a module docstring (must be the first statement), and `__default__ = X` lowers to `export default X` (App Router page/layout contract). **Verified at app scale (2026-06):** the reference full-stack app's Next.js track exercises **16 production routes**, each dual-track against a `.tsx` React oracle at `/react-reference/*` — RSC server-fetch, dynamic async `params` (`/runs/[id]/…`), recharts + WASM + interactive + **SSE `EventSource`** `"use client"` islands, and **server actions** (`"use server"` + `revalidatePath`/`redirect`/FormData); **60 Playwright (real `next build && next start`) + 19 component Vitest, all green** — **re-verified 2026-06-29 against compiler pin `7f43401`** (post the v3.x backlog: `[x]*n` codegen #58, native-`len` #60, etc.), 60 + 19 still green with no regression. Surfaced + fixed 3 upstream issues (capitalized-constructor-in-PSX #54; `.get` receiver-shape #56; `.psc`-first resolution #57). **Known limitation (CONFIRMED 2026-07-03 — an earlier "resolved" claim was a false positive):** `"use client"` *components* must be **pre-compiled** to plain `.js` — Turbopack's client-reference proxy cannot handle custom-extension module ids (`Can't resolve './X.psc.js'`; dropping the rule's `as: "*.js"` panics Turbopack). A 2026-06-29 test that "proved" loader-driven islands was invalidated: the imports were silently resolving same-named `.tsx` files via global extension order, so the oracle — not the PythScribe island — was being exercised (behavior parity masked it). **Fixed in PR #66:** the loader now rewrites extensionless relative imports in compiled output to explicit `./X.client.js` / `./X.psc` / `./X.ps` siblings (importer-aware resolution, matching the Vite plugin's `resolveId`), so PythScribe importers genuinely load PythScribe modules — marker-verified at app scale, with the reference full-stack app's 60 e2e green on genuinely-PythScribe islands (which immediately surfaced + fixed a masked component bug). Server components/Suspense/actions compile via the loader; a live HMR test proves a `page.psc` edit hot-updates `next dev` **with client-island `useState` preserved**. |
| Ecosystem libraries | `█████████░` 93% | **30+ React libraries explicitly recognized** across categories: state (Zustand, Jotai, Recoil, Valtio, XState, MobX), data (TanStack Query/Table, SWR), routing (React Router/DOM), forms (React Hook Form), motion (Framer Motion, React Spring), icons (Lucide, React Icons, Heroicons), i18n (react-intl, react-i18next), markdown (react-markdown), drag-and-drop (react-dnd, dnd-kit, react-beautiful-dnd), virtualization (react-window, react-virtual). **Scoped packages** (Mantine, Chakra UI, Headless UI, Radix UI, Emotion, Floating UI, Storybook, Testing Library) routed via `at_<org>.<pkg>` form. **Bundled `.pyi` stubs** for `lucide_react`, `zustand`, `swr`, `react_hook_form` in addition to the React core set. Long tail via generic kebab-case fallback + `pyths.toml [npm.imports]` overrides for irregular names. |
| Type system / DX | `█████████░` 95% | Full type inference, `.d.ts` emission, source maps, LSP (completion/hover/goto-def). **Bundled `.pyi` stubs** for `react`/`next`/`react-router-dom`/`@tanstack/react-query`; `from X import Y` binds `Y` to the stub-declared type at `pyths check` time. **Project-local stubs** via `pyths.toml [stubs.paths]` override bundled ones. **Generic stubs** (TypeVar inference) + **tuple destructure** — `count, set_count = use_state(0)` types `count: int` and `set_count: Callable[[int], None]`; `set_count("hello")` is now flagged at compile time. Hook return types are as precise as TypeScript's. |
| WASM auto-routing | `█████████░` 90% | Tiers 1–7 done. Closures + lists/dicts work. **`sorted()` element-type generalized** — i64, f64, i32 (bool/small-int) lists sort via insertion sort. **`reduce()` accumulator generalized** — i64, f64 (and i32 fallback) accumulators work through HoF-arg-from-context inference. **`map()` with type-changing lambda** — `map(lambda x: x * 2.0, int_list)` returns `PtrList(F64)` correctly. Ptr/str sort (needs a `__str_le` byte-compare helper — passes through unchanged today) and named-function HoF arguments are bounded follow-ons. |
| Testing infrastructure | `█████████░` 96% | 8+ layers, **4,000+ tests** (Rust workspace 1,973): compile-string, runtime-helper, format-spec differential vs CPython, CPython semantic differential, browser DOM parity, browser pixel parity, Node auto-routing E2E, **panic-resistance fuzz harness** (lexer + expander + parser + checker across thousands of random + mutated inputs). **Coverage-guided `cargo-fuzz` scaffold** in `fuzz/` for deep nightly-toolchain exploration. Clean build — zero warnings across `cargo build --workspace`. |
| Performance vs targets | `█████████░` 93% | Beats Pyodide ~7,750× on bundle, ~50× on cold start. **`wasm-opt` automated** — auto-detects on PATH, runs `-Os` (size-optimized for web; production-typical 15–30% byte reduction), surfaces the reduction at normal verbosity. Tree-shaking depends on the user's bundler (Vite/Rollup/webpack); `pyths-runtime/stdlib/<name>` per-module imports make idle helpers trivially tree-shakeable. |
| Developer experience | `█████████░` 92% | Watch mode, source maps, error messages with hints, LSP. Vite + Next.js plugins documented end-to-end (incl. `.psc`). **React Refresh / Fast Refresh** wired through codegen (`--react-refresh` flag) and **both** Vite + Next.js plugins (auto-on in dev); the Next loader shim is SSR-safe (PR #66). *Verified by a live automated HMR test (the reference full-stack app `test:hmr`): a `.psc` server-page edit hot-updates `next dev` with client-island `useState` preserved. Turbopack island edits still go through the precompile (client-reference constraint).* |
| **Compression layer (`.psc`)** | `█████████░` 90% | Library complete (163 tests). CLI extension dispatch + `--expand` flag + `pyths expand` subcommand wired. Vite + Next.js plugins detect `.psc`. `pyths.toml [expand.dictionary]` honored for project-local aliases. **Triple-quoted-string scanner fixed** — docstrings with embedded `"…"` groups no longer leak past downstream passes. Implementation is feature-complete; LLM-trial study (Phase 2 §9.4) is the remaining empirical validation. |
| Project-level configuration (`pyths.toml`) | `█████████░` 95% | Walk-up discovery (Cargo-style). `[expand.dictionary]` honored by CLI + `pyths expand`; `[stubs.paths]` honored by `pyths check`; `[npm.imports]` honored by codegen (user overrides win over built-in mappings + kebab fallback). Complete consumer coverage for the three documented sections. |
| Production hardening | `█████████░` 95% | CI/CD documented (3 ref workflows + **weekly fuzz cron** at `.github/workflows/fuzz.yml`). Vite/Next.js plugin READMEs, performance tuning guide. **Security packet** (`SECURITY.md` + `docs/security.md` + `fuzz/` cargo-fuzz scaffold with seeded corpus). **npm publish hygiene** — `files:` allowlists, `.npmignore` safety nets, `repository`/`homepage`/`bugs` fields on all three publishable packages; `npm pack --dry-run` shows clean tarballs (3 / 4 / 22 files). Engagement of an external auditor is the remaining production gap. |
| **Overall** | **`█████████░` 99%** | **Production-ready for React/Next.js apps**; remaining gaps require external action (auditor engagement, npm credentials, LLM API access) or are runtime-side (RSC streaming protocol verification against Next.js 14/15). Compile-time work is essentially complete. |

---

## 4. How to verify these claims

```bash
# Rust suite (parser, codegen, HIR, WASM emit/exec, CLI)
cargo test --workspace

# Node-level test layers
node --test crates/pyths_runtime/js/runtime.test.js
node --test runtime/src/stdlib/decimal.test.mjs runtime/src/stdlib/fractions.test.mjs
node --test crates/pyths_runtime/js/format_diff_test.mjs
node tests/differential/run.mjs                       # CPython differential (incl. decimal/fractions)
node tests/jsinterop/run.mjs                          # Promise/async-JS interop (pinned expectations)
node tests/libinterop/run.mjs                         # 3rd-party React lib parity (TSX oracle vs .ps twin, 28)
node --test tests/differential/auto_route_test.mjs     # JS+WASM E2E

# Browser-runtime parity (Playwright)
cd tests/e2e && npx playwright test
```

Expected output: 1,973 Rust workspace + ~79 Node (runtime helpers 44, Worker-safe `core` 19, `web` 8, bigint 8) + 30 format-spec + 1,376 CPython diff + 8 fixture parity + 20 fuzz parity + 8 auto-route + 28 Playwright = **~3,500 green**. Everything in `RESULTS.md` is reproducible from `examples/cloudflare-bench/bench/`. `cargo build --workspace` emits zero warnings.

---

## Appendix

### A. Architecture map
Eleven-crate Cargo workspace under `crates/`, prefix `pyths_`:

| Crate | Responsibility |
|---|---|
| `pyths_lexer` | logos-based raw tokenization + custom INDENT/DEDENT injection |
| `pyths_syntax` | AST types + span tracking |
| `pyths_parser` | hand-written recursive-descent parser |
| `pyths_resolve` | name resolution, scope tracking |
| `pyths_types` | type inference + checker + bundled `.pyi` stubs |
| `pyths_hir` | high-level IR + WASM-eligibility analysis |
| `pyths_codegen_js` | AST → JS string emission (incl. React/Next.js, PSX, source maps, `.d.ts`) |
| `pyths_codegen_wasm` | direct AST → WASM binary via `wasm-encoder` |
| `pyths_expand` | `.psc` → `.ps` source-to-source expander (Tier A/B/C/D + Dictionary) |
| `pyths_config` | `pyths.toml` walk-up loader |
| `pyths_diagnostic` | error rendering via `ariadne` |
| `pyths_cli` | the `pyths` binary |
| `pyths_runtime` | JS-side runtime helpers (`pyLen`, `pyRange`, format-spec, etc.) |
| `pyths_lsp` | language server protocol implementation |

The per-crate `src/lib.rs` and module docstrings carry the detailed implementation notes.

### B. Test inventory by layer
| Layer | Count | What it covers |
|---|---:|---|
| Rust unit + integration | 1,973 | parser (incl. async comprehensions), codegen (incl. `[npm.imports]` overrides + React Refresh emission + Python-faithful `AssertionError` lowering + async generator support + async-comprehension `for await` lowering + ecosystem import resolution for Mantine/Chakra/Headless/Radix/Lucide/SWR/Zustand/etc.), lexer with multi-line-string-aware INDENT/DEDENT, HIR, WASM emit/exec with i64+f64+i32 sort generalization + HoF-arg-from-context inference + map type-changing lambdas + `wasm-opt -Os` automation, CLI, lint, type checker (incl. .pyi stub resolution — bundled `react`/`next`/`next.headers`/`next.server`/`next.navigation`/`react-router-dom`/`@tanstack/react-query`/`lucide_react`/`zustand`/`swr`/`react_hook_form` plus project-local — Server Component lowering, generic TypeVar inference, and tuple-destructure element-typing), `pyths_expand` (168 — Tier A/B/E + Dictionary + user overrides + triple-quote-aware scanner), `pyths_config` (11 — `pyths.toml` walk-up loader). Includes a panic-resistance fuzz suite of 3 tests covering thousands of random + mutated inputs. |
| Node runtime helpers | 43 | `pyStrJoin`/`pyDictGet`/`pyFormatSpec`/etc. in isolation |
| Format-spec differential | 30 | Every `(value, spec)` pair matches CPython's `format()` |
| CPython semantic differential | 1,376 | whole-program stdout matches CPython 3.12 (full corpus, `tests/differential/run.mjs`; cross-checked on a second JS engine) |
| Playwright fixture parity | 8 | dashboard_500.ps + app_1000.ps DOM + pixel parity vs React reference |
| Playwright fuzz parity | 20 | 5 generated fixtures × 4 (renderx2, DOM, pixel) |
| Node auto-routing E2E | 8 | JS for DOM, WASM for compute — checked end-to-end |
| **Total** | **4,000+** | |

### C. Library compatibility matrix
| Tier | Coverage | Libraries |
|---|---|---|
| **Tier 1** — CI-tested end-to-end (browser DOM + pixel parity) | strong | React, React-DOM, Pyths runtime, dataclasses |
| **Tier 2a** — Bundled `.pyi` stubs + codegen smoke tests | strong | React, Next (+ headers/server/navigation), React Router/DOM, @tanstack/react-query, Lucide-React, Zustand, SWR, React Hook Form |
| **Tier 2b** — Recognized at codegen layer (snake→camel, JSX props, import-path mapping) — smoke-tested | medium | React-Redux, @reduxjs/toolkit, Next.js core (App+Pages routers), TanStack Table, Framer Motion, Jotai, Recoil, Valtio, XState (+@xstate/react), MobX-React (+Lite), React Icons, Heroicons, react-intl, react-i18next, react-markdown, react-helmet, react-dnd (+html5-backend + beautiful), date-fns, classnames/clsx, tailwind-merge, react-table, react-select, react-window, react-virtual, react-aria, react-use, @mantine/*, @chakra-ui/*, @headlessui/*, @radix-ui/*, @emotion/*, @floating-ui/*, @dnd-kit/*, @storybook/*, @testing-library/* |
| **Tier 3** — Works via generic name transform; not explicitly listed | weak | Long tail. The snake→camel + module-name kebab + `pyths.toml [npm.imports]` override paths handle nearly any modern npm package without per-library entries. |

### D. Known limitations / scope cuts
- `reduce` accumulator types: i32 / i64 / f64 with lambdas (HoF-arg-from-context inference). Named-function variants still default to I64.
- `sorted(lst)`: i64 / f64 / i32 (bool, small-int) element types supported. Ptr / string sort passes through unchanged today; full lexicographic compare via a `__str_le` helper is tracked as future work.
- `map(lambda x: f(x), lst)` with type-changing body works (output `PtrList(body_ret_ty)`); named-function variants share the inference gap above.
- ARM64 Linux WASM target untested (CI runs x86_64 macOS + Windows).

### E. Security
- **Coordinated disclosure**: [`SECURITY.md`](./SECURITY.md) at the repo root — report to mrigank.swet@gmail.com.
- **Threat model**: [`docs/security.md`](./docs/security.md) — 8 threats catalogued (compiler panics, codegen output safety, malicious `pyths.toml`, supply-chain, expander semantic drift, source-map disclosure, etc.), with current mitigations and known gaps.
- **Runtime-helper review checklist**: in `docs/security.md` §4.
- **Fuzz harness**: `crates/pyths_cli/tests/fuzz_inputs.rs` — random ASCII, random UTF-8, and mutation-based corpora across lexer + expander + parser + checker. Configurable iteration count via `PYTHS_FUZZ_ITER`. Verified panic-free across 4000+ inputs.
- **Coverage-guided fuzzer**: `fuzz/` crate (out-of-workspace; nightly + `cargo install cargo-fuzz` to run). Four targets: `fuzz_lexer`, `fuzz_expand`, `fuzz_parser`, `fuzz_check`. Seeded corpus in `fuzz/seed_corpus/` (40 fixtures + 3 `.psc` samples per target). Weekly cron at `.github/workflows/fuzz.yml` runs each target for 10 min (10 min × 4 targets per week). See `fuzz/README.md` for local invocation.

### F. Competitive landscape
| Approach | Architecture | Runtime size | Cold start | Web/React |
|---|---|---:|---:|---|
| Pyodide | CPython interpreter → WASM | 8+ MB | ~1 s | No |
| componentize-py | CPython bundled in WASI | 10+ MB | seconds | No |
| py2wasm | Python → C → WASM (CPython API) | 5+ MB | hundreds of ms | No |
| Pydantic Monty | Rust bytecode interpreter → WASM | ~4–5 MB | µs native / s WASM | No (sandbox only) |
| Transcrypt | Python source → JS (AOT), JS runtime lib | ~100s KB | fast (no interpreter) | JS/DOM; not React-native |
| **PythScribe** | **Python source → JS/WASM ops (no interpreter)** | **<100 KB** | **sub-ms** | **Yes — hybrid JS+WASM** |

### PythScribe vs Transcrypt (the closest approach)

Transcrypt is the nearest comparison — also an ahead-of-time Python→JavaScript compiler that ships no interpreter, so the bundle and cold-start advantage over interpreter-based tools (Pyodide, Brython) is *shared*, not a differentiator between the two. What separates them:

- **Semantic faithfulness — default vs opt-in.** Transcrypt's stated design is to unify Python with JavaScript's type system (integers are JS numbers), and Python-specific behavior is opt-in per scope — value/operator semantics via `__pragma__('opov')` and CPython truthiness (an empty collection is falsy) via `__pragma__('tconv')`, each adding runtime cost. PythScribe emits Python semantics **by default, everywhere** — exact integers, floor division and divisor-sign modulo, banker's rounding, code-point strings, value equality, fail-loud errors — and treats that as a contract it tests (and partially proves) against CPython. For AI-generated code this removes the silent-drift failure mode with no per-module opt-in.
- **Frontend & syntax.** Both can drive React, but the source diverges sharply. In Transcrypt you wire React by hand (`React = require('react')`, alias `createElement`/`useState`, call `ReactDOM.render` on `DOMContentLoaded`); components are plain functions; elements are built with explicit `createElement('tag', {props}, *children)` (no JSX); and props/events are JavaScript objects read by string-key subscript with camelCase keys — `{'onClick': …}`, `{'htmlFor': …}`, `event['target']['value']`, `props['item']`. Hooks and identifiers stay JS-style (`useState`, `onChange`). PythScribe keeps it Pythonic end to end: `@component`/`@psx` decorators, Pythonic JSX (`div(class_name=…, button(on_click=…, "Add"))`), props delivered as named parameters (plain names, not `props['x']` subscripts), and snake_case that lowers to the JS names (`use_state`→`useState`, `on_click`→`onClick`) — plus native hooks, server components, a Next.js toolchain, and WASM auto-routing, none of which Transcrypt provides. So PythScribe sits closest to Python in *both* syntax and semantics.
- **Compute.** PythScribe auto-routes pure-numeric functions to WebAssembly; Transcrypt is JavaScript-only.
- **Assurance.** PythScribe adds a per-compilation routing certificate, machine-checked proofs, and a CPython differential corpus.
- **LLM-oriented source layer.** PythScribe adds an optional compressed superset, `.psc` — a model-facing surface for token-efficient code emission by LLMs — joined to canonical `.ps` by a deterministic expander whose core rewrite properties are machine-checked in Lean (the Iron Rule: `canonicalize(expand(x)) == canonicalize(x)`). Transcrypt has no compression or model-facing layer.

Honest trade-off: Transcrypt is mature and its default JS-native operations are faster; PythScribe trades that for Python fidelity by default (a runtime-helper cost that WASM offsets on numeric paths).

---

*Read time check: ~9 minutes at 240 wpm. For deeper architectural detail, see the per-crate `src/lib.rs` doc-comments under `crates/` — they carry the implementation notes for each pipeline stage.*
