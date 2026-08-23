# Known limitations

Honest record of intentional scope boundaries and residual CPython divergences.

## Semantic deviations from Python (by design)

PythScribe's verified core proves that the compiler preserves Python's semantics on a wide
fragment — integers, floor-division, code-point strings, `round`, `sorted`, bitwise, and more
(see the README's "Correct-by-default semantics" table). The handful of places where PythScribe
**deliberately does not** match CPython are listed here up front, so nothing surprises you at
runtime. These are stances, not bugs.

### D1 — whole-valued floats can display as ints

Python `int` and `float` are both compiled to a single JS `number`, so a **whole-valued float
loses its `.0` when its float-ness can't be tracked statically** — most visibly *inside
containers* and *through untyped function boundaries*:

```python
print([1.0, 2.0])          # PythScribe: [1, 2]        CPython: [1.0, 2.0]
print({0.0: 'x'})          # PythScribe: {0: 'x'}      CPython: {0.0: 'x'}
isinstance(3.0, int)       # PythScribe: True          CPython: False
```

Direct, statically-tracked cases are correct (`x = 2.0; print(x)` → `2.0`; `print(float(2))` →
`2.0`). **Values and lookups are always Python-correct** — `0.0 == 0` share a dict slot, arithmetic
is unaffected; only the *repr* of a whole float differs. Full fidelity would need a float-wrapper
type that taxes every numeric op on the edge target — deliberately rejected. (See the A4/F4 entries
below for the precise tracked/untracked boundary.)

### D2 — `eval` / `exec` / `compile` — and every other unimplemented builtin — are rejected

PythScribe is an **ahead-of-time compiler**, so the dynamic-execution builtins are refused at
compile time with a clear diagnostic ("PythScribe is an ahead-of-time compiler and does not run
arbitrary Python at runtime") rather than a cryptic runtime error. Running arbitrary Python at
runtime would require shipping an interpreter to the browser/edge — the opposite of the design.

This is now a **class-wide gate** (public issue #3): a bare reference to *any* known CPython
builtin with no lowering — `open`, `input`, `hash`, `id`, `globals`, `locals`, `memoryview`,
`help`, `breakpoint`, `aiter`, `anext`, `__import__`, plus the D2 trio — fails **both**
`pyths compile` and `pyths check` with a diagnostic naming the builtin and its deferral target
(`pythscribe-v3.x`), instead of compiling to a bare JS identifier that dies with a runtime
`ReferenceError`. User bindings/imports named like a builtin shadow it and compile normally.
(`format`, `slice`, `ascii`, and 1-arg `vars` gained real implementations in the same change;
zero-arg `vars()` is `locals()` and is rejected with its own message.)

### D3 — `str.encode()` / `bytes` are not (yet) modeled

`str.encode()` and the `bytes` type emit an explicit "not yet supported" diagnostic. Unlike D1/D2
this is a **capability gap, not a permanent stance** — it could be implemented later (TextEncoder-
backed) if a real workload needs it.

### D4 — loop-variable closure capture is per-iteration (early-bound)

A closure that captures a loop variable sees the value **at the iteration it was created**,
because PythScribe compiles a `for`/`while` loop body with a fresh per-iteration `let` binding
(JS block scope). CPython binds the loop variable in the *enclosing function* scope, so a closure
captures the variable itself and reads its **final** value after the loop ends (late binding):

```python
fs = []
for i in range(3):
    fs.append(lambda: i)
print([f() for f in fs])   # PythScribe: [0, 1, 2]   CPython: [2, 2, 2]
```

CPython's late binding is the well-known "closures capture the variable, not the value" gotcha
(the standard CPython workaround is a default argument, `lambda i=i: i`, which PythScribe also
supports and which produces `[0, 1, 2]` on both). PythScribe's per-iteration capture is the more
intuitive result but is nonetheless a **deviation**. Making captured loop variables late-bound
would require hoisting the loop target to a single function-scoped binding and is tracked as a
**v3.x codegen follow-up**; until then this is documented, not changed.

### D5 — `random.seed(n)` is deterministic within PythScribe, not CPython-value-equal

`random.seed(n)` seeds a shared module-level PRNG that every `random.*` function draws from
(the same architecture as CPython's hidden module `Random()` instance), so a seeded program is
**reproducible run-to-run within PythScribe**: same seed → same sequence of `random()` /
`randint()` / `choice()` / `shuffle()` / … results. The generator is **mulberry32, not
CPython's Mersenne Twister**, so the *values* do not match CPython's for the same seed — same
class of deviation as the ≤4-ULP transcendentals: the *property* (determinism, distribution,
range) matches; bit-level output does not. A full Mersenne Twister port is a v3.x candidate if
a real workload ever needs CPython-exact streams. Unseeded use is nondeterministic, as in
CPython. `random.Random(seed)` gives an independent seedable instance.

*(These are the same by-design deviations tracked in `TRUST.md` and surfaced in the differential
suites as pinned cases rather than "fixed". Related mechanics — the float tracked/untracked
boundary, division-by-zero message on whole-float variables — are detailed in the A4/F4 entries.)*

**Positive boundary correction (2026-08-16, full-surface autotester):** async/await + `asyncio`
(`asyncio.run` / `gather` / `sleep`, coroutine interleaving) is **fully supported** and
byte-matches CPython in the differential harness — any older note calling async
"out-of-fragment" or partial is obsolete.

### D6 — default object `repr` reports the module as `__main__`

An instance of a user class with no `__repr__` renders as CPython's default fallback,
`<module.Class object at 0x…>` (previously `{}`). The `module` segment is the one
approximation: PythScribe always emits `__main__`, whereas CPython names the module the class
was *defined* in. So a class imported from another of your own modules reprs as
`<__main__.Widget object at 0x…>` rather than `<mymod.Widget object at 0x…>`. Everything else
is exact — the class name, the `str(x) == repr(x)` fallback identity, and a stable, per-object
synthetic address (distinct objects get distinct addresses). Single-module programs are fully
exact; the deviation is a **cosmetic repr-string difference only** in multi-module programs, and
never affects control flow, equality, or hashing. A faithful `__module__` would require
threading each class's defining-module name through codegen — a v3.x candidate if a workload
ever needs module-accurate default reprs.

### D7 — `import_module` is asynchronous (returns a Promise)

`import_module(spec)` (CPython's `importlib.import_module`, also usable bare as a PythScribe
builtin) lowers to native ES **dynamic** `import(spec)` so `.ps` can express code-splitting /
lazy-load. CPython's `importlib.import_module` is *synchronous* and returns the module object
directly; ES `import()` returns a **Promise**, so the PythScribe form must be `await`ed:

```python
from importlib import import_module        # or just call import_module(...) bare
m = await import_module(f"./{name}.js")     # → m = await import(`./${name}.js`)
Cls = m[name]                               # module-namespace subscript
```

Only `import_module` is provided from `importlib`; any other `from importlib import …` name is
rejected at compile time. The deviation is the **`await`** — everything else (the returned
namespace object, named-export subscript) matches Python semantics.

## HTML sinks — `set_html` / `dangerously_set_html` (A18)

The `pyths.dom` runtime helper `set_html(element, html)` assigns `element.innerHTML = html`
verbatim. It is a legitimate escape hatch (there is no other way to inject a block of markup),
but it is an **unescaped XSS sink**: any attacker-controlled substring in `html` executes.
The runtime performs **no implicit sanitization** — that responsibility is the caller's.

- For untrusted strings, prefer `set_text` (assigns `textContent`, always safe).
- When you must insert markup, run it through a sanitizer (DOMPurify or equivalent) first.
- `dangerously_set_html` is a **clearer-named alias** for the exact same sink, added so the
  danger is unmissable at the call site (mirrors React's `dangerouslySetInnerHTML`). `set_html`
  is retained unchanged for backward compatibility; both behave identically.

The JSX prop form `dangerously_set_inner_html={...}` (→ React's `dangerouslySetInnerHTML`) is
the React-track equivalent and carries the same caveat. See `docs/security.md` §T2.

## Default-argument evaluation in components and methods (F6)

Python evaluates a function's default arguments **once**, at definition time, so
a mutable default (`def f(xs=[])`) is shared across calls. PythScribe now
reproduces this for **plain functions** (and nested functions): each default is
hoisted to a once-evaluated `const` at the definition site and referenced from
the JS default parameter.

Two paths deliberately keep JS-native (per-invocation) default evaluation:

- **`@component` prop defaults.** A component's params compile to destructuring
  defaults on a props object (`function Card({ items = [] } = {})`). React
  re-invokes the component every render, and a default recreated per render is
  the correct, idiomatic React behavior — sharing one mutable default object
  across renders would be a bug, not fidelity. These are left as-is.
- **Class/instance methods.** Method defaults are not hoisted (there is no clean
  once-evaluated slot inside a JS `class` body). A method with a mutable default
  (`def m(self, xs=[])`) therefore gets a fresh value per call rather than a
  shared one. This is an accepted residual; hoisting method defaults to
  module scope would require restructuring method emission and is deferred.

## Float division-by-zero message on whole-valued float *variables* (F4)

`1.0 / 0.0` raises `ZeroDivisionError("float division by zero")` and `1 / 0`
raises `ZeroDivisionError("division by zero")`, matching CPython, because the
compiler tags statically-known float operands. A whole-valued float held in a
*variable* (`x = 2.0; x / 0`) compiles to the same untagged JS number as an
int, so it reports `"division by zero"`. Non-whole float values (`1.5 / 0`) are
detected at runtime and report the float message correctly.

## Whole-float display through untracked channels (pre-existing, A4)

`print(float(2))` renders `2.0`, but a whole-valued float reaching `print` /
`str` / `repr` through an untracked channel (an unannotated variable, a
list/dict element, an unannotated return) may render `2` — PythScribe ints and
whole floats share one untagged JS `number`. Unchanged by this batch.

## `next(gen, default)` (B-011)

The 1-arg `next(gen)` form is supported and raises `StopIteration` when
exhausted. The 2-arg `next(gen, default)` form is honored incidentally by the
runtime but remains officially unsupported and untested.

## Sweep-A residuals (differential infra)

Found by `tests/differential/gen_identifier_cases.mjs` (S1) and the S2/S3
sweeps during the pre-launch differential sweep. The Sweep-A fix batch
(G) fixed and re-enabled every Sweep-A residual (#79-#82, #84-#95) and
every Sweep-B finding (#97-#101) EXCEPT the one below; the S1 skip-list,
the S2 generator restrictions, and the README/docs exclusion lists are
empty again for the fixed set.

**dict non-string keys — FIXED (#83, 2026-07-05), narrow residual below.**
The hybrid shape-dispatch representation landed: dict literals /
comprehensions whose keys are all provably strings (string literals,
f-strings, `str(...)`) stay plain JS objects (full JS interop — React
props, JSON, spread); any other key shape compiles to a Map-backed
`PyDict` with CPython key canonicalization (`True`/`1`/`1.0` fold to one
key, first-inserted key object wins, tuples hash by structure,
lists/dicts/sets raise `TypeError: unhashable type`). All dict operations
(subscript read/write incl. augmented, `in`, `len`, `del`, iteration,
keys/values/items, get/pop/setdefault/popitem/update/clear/copy, `{**a,
**b}`, `|`, `==`, repr, `json.dumps` with CPython key coercion,
`isinstance(d, dict)`) dispatch on receiver shape at runtime.
`dict(map_backed)` with all-string keys returns a plain object — the
documented escape hatch for passing a Map-backed dict to JS APIs that
expect plain objects.

Remaining residuals (each narrow, none new — see issue #83 close-out):

- FIXED for literal keys (#106, 2026-07-10): a dict literal assigned to
  a name that is later subscript-written with a non-string LITERAL key
  (`d = {}` ... `d[1] = x`) now constructs Map-backed (compile-time
  pre-scan). Residual: the poisoning key must be a literal — a write
  through a runtime-computed non-string key (`d[k] = x` where `k`
  happens to be an int at runtime) on a plain-shaped dict still
  stringifies, because the plain shape physically can't hold it and
  computed keys are overwhelmingly strings in real code (flipping those
  would break plain-object interop). Workaround unchanged: seed with a
  non-string key or use a comprehension.
- `d.keys()`/`.values()`/`.items()` return plain arrays, not live
  dict-view objects — `print(d.keys())` shows `[1, 2]`, CPython shows
  `dict_keys([1, 2])` (pre-existing, unchanged by #83; items() pairs ARE
  tuple-marked now, so `sorted(d.items())` reprs correctly).
- Two different NaN objects are distinct CPython dict keys (identity);
  JS Map folds every NaN into one key (SameValueZero) — best-effort.
- A whole-valued float key (`{2.0: 'x'}`) displays as `2` — the
  pre-existing untagged int/float `Number` ambiguity (A4 class), not a
  dict-representation issue.
- FIXED (#106, 2026-07-10): destructuring assignment into subscript
  targets (`d[0], x = a, b`) now evaluates the RHS into a temp and
  assigns element-wise through the shape-dispatching single-target
  path. (Star patterns keep the JS destructuring form.)

GitHub issue: https://github.com/swetmr/pythscribe/issues/83 (closed by
the fix PR); residual tracked separately.

## Pythonic-checks findings (2026-07-06 targeted sweep)

Differential sweep over collections / itertools / zip / f-strings
(+168 corpus entries, 580/580 green). Fixed in the same sweep: lazy
one-shot `zip` (infinite iterators, `strict=True`, tuple rows),
comprehensions over arbitrary iterables (strings/generators/dicts),
nested tuple for-targets (`for i, (x, y) in ...`), itertools tuple
yields + kwargs + `chain.from_iterable`, Counter/defaultdict/deque/
namedtuple/OrderedDict CPython fidelity (`__missing__` protocol,
Counter `+ - & |`, count-descending Counter repr, deque repr/methods),
format-spec `_` grouping / sign-aware zero-pad / CPython `:g`, f-string
`!r`/`!s` conversions and self-documenting `f"{x=}"`, and runtime-method
kwargs forwarding (`d.popitem(last=False)` used to silently drop the
kwarg). Deferred divergences below — each has a tracked issue.

- **Accepted residuals (documented, no issue):** `groupby` buffers each
  group eagerly (a group list stays valid after the parent iterator
  advances; CPython invalidates it) and `tee` materializes its source
  (no lazy shared buffer; infinite sources hang); `defaultdict` repr lacks the
  `<class 'int'>` factory display; Counter keys are SameValueZero (no
  CPython key canonicalization of `True`/`1` or tuple keys — plain
  `dict` non-string keys DO canonicalize via PyDict).

## Integer literal range (B11)

A **source-level integer literal** must fit in a signed 128-bit value
(`|n| ≤ 2^127 − 1`, roughly `1.7 × 10^38`). The lexer stores literals in an `i128` so anything up
to 39 digits (well past `i64`'s ~19) lexes; codegen emits a JS `BigInt` for any literal beyond
`2^53`, so large-but-in-range integers keep full precision. A literal **larger than `i128`** now
produces a dedicated diagnostic —

```
Integer literal '999…' exceeds the supported range (values must fit in 128 bits)
```

— instead of the previous misleading *"Unexpected character '999…'"*. This is a bound on integer
*literals* only; arbitrary-precision integer *arithmetic* at runtime remains a separate, tracked
scope item. Workaround for an out-of-range constant: construct it (`10 ** 40`) or pass it as a
string to `int(...)`.

## `except Exception:` / `except BaseException:` is an unconditional catch-all (B14)

`except Exception:` and `except BaseException:` compile to an **unconditional** JS `catch (e)`
(no type test), because in real Python code these two are the near-universal "catch everything"
handlers and every user-raised PythScribe exception derives from them. A practical consequence:
an internal JS error thrown from *inside* the `try` body by a **compiler defect** — a
`ReferenceError` (undeclared name) or a `TypeError` (bad property access) that PythScribe should
not have emitted — is caught by the user's `except Exception:` just like a genuine Python
exception, rather than surfacing as a crash. This can **mask a compiler bug** as an ordinary
handled exception.

This is a documented residual, not a behavior we change on release-eve: narrowing the catch to
exclude "internal-looking" JS error shapes risks altering the semantics of legitimate Python
programs that legitimately catch broadly (including code that raises the built-in-mapped
`TypeError`/`ValueError`). More specific handlers (`except ValueError:`, `except KeyError:`, …)
already compile to a guarded catch that re-throws non-matching errors, so they are unaffected.
If you are debugging a handler that seems to swallow "impossible" errors, temporarily narrow it or
add `except Exception as e: ...; raise` to see the underlying JS error. Tightening the catch-all
to re-throw obviously-internal errors is a candidate v3.x hardening item.

## Reserved-word *lowercase class* names (F1)

JS reserved words used as identifiers (`let`, `new`, `delete`, `default`, ...)
are sanitized in every binding/reference position. Since all JS reserved words
are lowercase and module-level class instantiation is chosen by capitalization
(`Foo(...)` → `new Foo(...)`), a class *named* with a lowercase reserved word is
not recognized for `new`-insertion — a pre-existing limitation of the
capitalization heuristic, independent of reserved-word handling.

## Generated-output write safety — trust model + platform residuals (v0.2.2 hardening)

The compiler/plugins refuse to overwrite files they cannot prove they created
(the `@generated` text header, the `pythscribe.generated` WASM custom
section), verify that proof on the same file descriptor they truncate, and
pre-flight the complete output graph before writing anything (see
`docs/security.md` §9). `--force` is the deliberate exception: it authorizes
overwriting a file *without* an ownership proof — that is its purpose — but
even then only the exact file the pre-flight inspected (same identity, same
bytes), never a file swapped in afterwards. Honest boundaries of that
scheme:

- **The ownership markers are accident prevention, not a security boundary.**
  A malicious process that can already write your build directory can forge
  either marker — or just write the destination directly. No marker scheme
  can defend against a same-directory writer; that is the OS/filesystem
  permission boundary's job. What the writer does guarantee, stated
  precisely: opens are no-follow on POSIX (`O_NOFOLLOW`); it never truncates
  before the identity + ownership proof passes on the writing fd; it never
  writes a destination that was not pre-flighted; and every overwrite is
  bound to the exact file (dev+inode / volume+file-index) the pre-flight
  inspected, so a different file renamed into place afterwards — even a
  marked or byte-identical one — is refused.
- **Pre-flighting the whole graph reduces, but does not eliminate, partial
  builds.** Writes are sequential: a failure in the middle of the sequence
  (disk full, a destination appearing between writes) leaves the outputs
  already written by that build in place. The guarantee is narrower — any
  refusal the pre-flight can detect aborts the build before the FIRST write,
  and a partially-written artifact is never silently trusted by the next
  run: it either still carries its leading `@generated` marker and is
  rebuilt, or it lost its ownership proof and is refused with an explicit
  error (resolved by rebuilding with `--force` or deleting the partial
  file).
- **Windows has no `O_NOFOLLOW`** — the overwrite path there relies on the
  pre-open `symlink_metadata` plus the on-fd proof; symlink creation on
  Windows is privilege-gated (admin / developer mode). Hard-link refusal IS
  enforced on Windows (`GetFileInformationByHandle`, link count must be
  exactly 1).
- **Alias detection case-folds on Windows and macOS defaults only.** A
  case-insensitive mount on Linux (or a case-sensitive APFS volume) is not
  modeled; the per-destination exclusive-create/fd checks still fail closed
  on the real collision, just later in the write sequence.
- **One-time `--force` for pre-v0.2.2 WASM:** a `.wasm` built by ≤ v0.2.1
  predates the ownership section, so its first rebuild under v0.2.2 needs
  `--force` once (the error message says exactly this); the rebuilt module
  carries the section and all later rebuilds are free.
