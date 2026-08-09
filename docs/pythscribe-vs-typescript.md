# PythScribe vs TypeScript — which JavaScript defects actually survive TypeScript

A design-philosophy essay: of JavaScript's well-known defects, **which persist after TypeScript** — i.e. where PythScribe differentiates *beyond* TS, and where it honestly does not. Framed against the classic *["The Top 10 Things Wrong With JavaScript"](https://medium.com/javascript-non-grata/the-top-10-things-wrong-with-javascript-58f440d6b3d8)*.

## The one principle

**TypeScript is compile-time-only — its types are erased at runtime.** So every defect that is a *runtime* problem survives TypeScript; the *authoring/tooling* defects are largely gone. PythScribe is different in kind: it carries semantics **into the runtime** — real arbitrary-precision `int`, true `i32/i64/f64` on the WASM-routed numeric path, and Python's strict fail-loud model — so it closes runtime classes TS structurally cannot.

In one line: *TypeScript checks, then erases; PythScribe changes the runtime.*

## The table

Legend — ✗ persists · ◐ partially fixed (compile-time / authoring only) · ✓ fixed.

| # | JS complaint | After TypeScript | After PythScribe |
|---|---|---|---|
| 1 | **No integer type** (precision loss past 2⁵³, `NaN`) | ✗ **Persists** — `number` is IEEE-754 float; `bigint` exists but isn't the default | ✓ **Integer type fixed** — real arbitrary-precision `int` (`2**53 + 1` → `9007199254740993`, exact; hybrid `Number`/`BigInt`); numeric code auto-routed to **WASM** runs on true `i32/i64/f64`. *Caveat: float literals stay IEEE-754 — `0.1 + 0.2` is still `0.30000000000000004`. PythScribe fixes the missing **integer** type, not floating-point rounding.* |
| 2 | **Loose typing / coercion** (`[] + {}`) | ◐ **Partly** — caught at compile time, but erased at runtime; `==`, `any`, and JSON/DOM boundaries still coerce | ✓ **Avoided at the language level** — `[] + []` is `[]` (not `""`); `1 + "1"` is a compile-time type error, not `"11"` |
| 3 | **Automatic Semicolon Insertion** | ✗ **Persists** — TS is a JS superset; same ASI rules | ✓ **Gone** — you write Python (no ASI); the compiler emits correct JS |
| 4 | **Abused language / module hacks** | ✓ Fixed — ES/TS modules | ✓ Clean (Python imports) — *not a differentiator (dated complaint)* |
| 5 | **Implied globals / bad scoping** | ✓ Fixed — strict mode + `let`/`const` | ✓ Clean (explicit scope, `NameError`, no `var` hoisting) — *marginal differentiator* |
| 6 | **Silent failures** | ◐ **Partly** — compile-time typos caught; runtime silent `undefined`/`NaN` propagation remains | ✓ **Reduced** — Python fails loud: `items[10]` raises `IndexError`, `d["missing"]` raises `KeyError` (CPython-matching messages) instead of returning `undefined` |
| 7 | **Prototype inheritance / `this`** | ◐ **Authoring fixed** (`class`, generics); runtime prototype + `this`-binding quirks remain | ✓ Cleaner — real classes, explicit `self`, no `this`-rebinding footgun |
| 8 | **Callback hell / async mess** | ✓ Fixed by modern JS (`async/await`) | ✓ Parity — Python `async/await` — *not a differentiator* |
| 9 | **"Not actually Lisp"** (no macros / homoiconicity) | ✗ Not fixed | ✗ **Also not fixed** — Python isn't homoiconic either; *we don't claim this one* |
| 10 | **Framework instability / "transpile from a better language"** | ✗ Doesn't fix ecosystem churn | ◐ **PythScribe's thesis** — churn persists for anyone targeting the JS ecosystem, but PythScribe lets you *author* in a stable language |

> Every behavioral claim above is checked by the differential test suite (CPython semantic corpus + runtime-helper tests) **and, increasingly, by machine-checked Lean theorems** — the verified core proves, *per deviation*, that the compiled output reaches Python's semantics (see [*The verified core*](#the-verified-core-each-deviation-is-a-theorem-not-just-a-test) below).

## Runtime-semantics deltas beyond the classic top-10

The 2015-era top-10 predates the defects that bite modern **text- and data-heavy** code hardest. Three more runtime deltas — each a place TypeScript is powerless (types erase at runtime) and PythScribe carries Python's model *with a machine-checked proof*:

| Delta | JavaScript / TypeScript | PythScribe | Proven |
|---|---|---|---|
| **Strings index by UTF-16 code unit** | `"💩".length === 2`; `"💩"[0]` is `'\uD83D'` — a lone high surrogate, *half a character*; every `.length`, `[i]`, and slice corrupts astral text | code points: `len("💩") == 1`, `"💩"[0] == "💩"`, slices split on characters | ✓ wave 11 `preservationS11` + `utf16_astral_strict` |
| **`[]` and `{}` are truthy** | `if ([]) { … }` **runs** the branch; `if ({})` too — a naively-ported guard inverts | Python falsy: `if []:` / `if {}:` **skip** — empty containers are falsy | ✓ `pyBool_iff_not_falsy` |
| **No floor division; sign-of-dividend `%`** | no `//`; `7/2 === 3.5`; `%` truncates toward zero (`-7 % 2 === -1`) | `7 // 2 == 3`, `-7 // 2 == -4` (floors); `%` takes the divisor's sign (`-7 % 2 == 1`) | ✓ preservation seed + waves (`jsFdiv_eq_fdiv`) |

### Strings are the sharpest one

Python strings are sequences of **code points** (Unicode scalar values): `s[i]` is the *i*-th code point, `len(s)` counts code points. JavaScript strings are sequences of **UTF-16 code units**: `s[i]` is the *i*-th 16-bit unit, `s.length` counts units. For characters in the Basic Multilingual Plane (≤ U+FFFF) the two *happen* to agree — so the bug hides in testing on ASCII. But an **astral** character (> U+FFFF — emoji, non-BMP CJK, math symbols, historic scripts) is stored in UTF-16 as a **surrogate pair: two units**. So one Python code point is two JS units, and:

```
s = "💩x"                # 💩 is U+1F4A9, astral
# Python / PythScribe:  len(s) == 2,  s[1] == "x"
# Native JS:            s.length === 3,  s[1] === "\uD83D"   ← half a character
```

This is not "usually the same, rare edge case." The verified core's **`utf16_astral_strict`** proves that *any* string containing a code point ≥ U+10000 has strictly more UTF-16 units than code points (`cps.length < utf16Len cps`) — so naive JS `s[i]`/`s.length` **provably cannot** implement Python's indexing on that whole class of inputs. It is an *impossibility* result: the reason PythScribe emits code-point helpers isn't stylistic — the obvious native alternative is proven wrong, so the correct one had to be built.

### Truthiness silently flips control flow

The single most surprising cross-language divergence: JavaScript treats an empty array/object as **truthy**, Python as **falsy**.

```
xs = []
if xs:  ...   # PythScribe/Python: SKIPS (empty list is falsy)
              # a naive JS port `if (xs)` RUNS the branch (empty array is truthy)
```

A guard ported by hand inverts its control flow. PythScribe compiles `if xs:` to Python's falsy rule, proven by `pyBool_iff_not_falsy` (`{}`→False included, closing the #211/#272 class).

## The verified core: each deviation is a theorem, not just a test

The runtime-semantics claims above are not only differential-tested against CPython — they are **machine-checked in Lean**. The compiler's `.ps`→JS/WASM behavior is modeled by two evaluators, `evalTgt` (compiled) and `evalPy` (Python reference), and a growing family of **preservation waves** proves `evalTgt e = evalPy e` — that the compiled program computes the Python value — over successively larger fragments:

- **arithmetic seed** — floor-div/mod: the emitted correction reaches Python's `-7 // 2 = -4`, not JS-trunc `-3` (`jsFdiv_eq_fdiv`).
- **WASM i64** (`preservationWasm`) — the `i64` fast path is exactly `wrapI64` of the Python value; **strings** (`preservationS11`) — code-point indexing/slicing/`len`; **dicts** (`preservationD`), **classes** (`preservationCls`), **collections/comprehensions/itertools**, and the statement/function language.

Each wave isolates the *one deviation node* where a naive translation would be wrong and proves the compiler bridges it. Every proof carries only Lean's standard axioms (`propext`, `Classical.choice`, `Quot.sound`), zero `sorry`. This is what "carries semantics into the runtime" means *concretely* — and it is the real differentiator over any transpiler: not merely Python-flavored output, but a **proof, quirk by quirk, that the compiler does not leak the JavaScript behavior underneath**. The flagship is the string impossibility result above: most tests show "our output is right on these inputs"; `utf16_astral_strict` shows "the naive alternative is wrong on an entire input class, no matter what."

## Why this matters most for real-world text and LLM-generated content

These deltas are not corner cases in modern software — they *saturate* the inputs it processes:

- **Astral text is everywhere.** Emoji, mixed scripts, mathematical symbols, and non-BMP CJK are pervasive in user content and — increasingly — in **LLM output**. Any app that slices, truncates, or counts the "length" of that text (chat UIs, content editors, character-limit enforcement, previews) hits the UTF-16 quirk constantly. Truncating an LLM response to *N* characters in JS can cut an emoji in half, emitting a lone surrogate that renders as `�`; in PythScribe it truncates on code points, so a "character" is what the user sees.
- **Exact integers + fail-loud collections** matter wherever IDs, counts, or amounts exceed 2⁵³ or a lookup can miss — JS's silent precision loss and `undefined` are a real bug class that Python's `int` and `KeyError` turn loud.
- **Generation quality.** LLMs are trained overwhelmingly on Python, so an LLM emitting `.ps` is likelier to be correct than one emitting the exotic JS (`Array.from`, `BigInt`, explicit `Map`) you'd otherwise need to get these semantics by hand — a second-order "better by design" for an AI-generation-heavy future.

## What this means (three buckets)

1. **Genuinely persist after TypeScript → the real "even vs TypeScript" wins:** **#1 integer precision** (TS keeps IEEE-754; PythScribe has a real `int`), **#3 ASI** (you never write JS), and the **runtime halves of #2 and #6** — coercion and silent `undefined`/`NaN` at every untyped boundary, because TS erases types at runtime.
2. **Already solved by TS / modern JS → drop from any "vs TS" claim:** #4, #5, #7 (authoring), #8. Leading with these reads as dated.
3. **Don't claim at all:** #9 (Python isn't Lisp either); #10 is a *tailwind for the thesis*, not a defect PythScribe fixes.

## Honesty guardrails

- Frame it as **"the classes TypeScript structurally can't fix,"** never "zero JS errors ever."
- The integer guarantee is the **integer type** + the WASM-routed numeric path. **Floating-point rounding is unchanged** — `0.1 + 0.2` is still imprecise; that is IEEE-754, not a language defect PythScribe claims to fix.
- PythScribe can't police the **FFI boundary** — data from the DOM, `JSON.parse`, or third-party JS is untyped at runtime; the strong-typing claim is for PythScribe-authored code, not the wire. The same holds for **strings**: a JS string crossing the boundary is UTF-16 until it enters PythScribe's code-point operations.
- The semantics are a **correctness win, not a free one.** The runtime helpers that deliver code-point strings, exact `int`, and fail-loud collections cost cycles — tight numeric/collection loops run slower than CPython (tracked, and the hot-helper overhead is an active perf item). We claim *correct by default*, not *fastest*.
- **TypeScript's type system is a genuine TS strength.** This doc is about **runtime semantics** — the classes TS structurally can't fix. TS's mature *structural* type system is real and valuable; PythScribe leans on Python type hints + inference, a different and lighter model. Don't frame runtime-semantics wins as if they beat TS at static typing.
- Claim the **mechanism**, not a flat "we fix JavaScript."

## The defensible framing

> *"TypeScript checks types at compile time, then erases them — so JavaScript's runtime footguns survive it: the missing integer type, coercion, silent `undefined`/`NaN` at every untyped boundary, UTF-16 string indexing that corrupts emoji and astral text, and empty-container truthiness that silently inverts a guard. PythScribe carries semantics into the runtime — a real arbitrary-precision `int`, true `i32/i64/f64` on the WASM path, code-point strings, Python's fail-loud errors and falsy rules — and its **verified core proves, deviation by deviation**, that the compiler reaches Python's semantics rather than leaking JavaScript's. We fix the classes TypeScript can't — and prove we did."*
