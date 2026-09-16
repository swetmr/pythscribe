# #491 — call-time builtin-shadow resolution via a module binding CELL (design note)

Root fix for the **user-binding-shadows-a-builtin** miscompile class. SUPERSEDES two
earlier models (the flat "declared everywhere" authority and the
order-aware "resolve at emit position" authority): both resolved a builtin-named
reference at EMIT time, but CPython resolves a global name at CALL time, so any static
answer is wrong for some program shape (B2: a function defined BEFORE the shadowing
`def` and called after it; B3: a function called both before and after the def).

## 1. The model (ONE authority, both emitters)

CPython module scope: a builtin name `N` resolves to the **builtin** until a
module-level binder of `N` (`def` / `class` / `import` / assignment / for-target /
`global` write / …) EXECUTES, and to the user binding after; the LAST binder to
execute wins; `del N` restores the builtin; function bodies read the binding that is
current **when they run**.

The emitted JS reproduces this with a **mutable module cell**:

```js
export let abs = pyAbs;          // cell, declared at module TOP (hoist section),
                                 // initialized to the builtin's first-class value
…                                // every read (module scope AND function bodies) is
                                 // the bare name `abs` — i.e. the cell
abs = function (x) { … };        // each binder REASSIGNS the cell at its SOURCE
abs = __pyimp_abs_0;             //   position (def / import / assign / class / …),
abs = __wasm$abs;                //   never a hoisted declaration
abs = pyAbs;                     // `del abs` restores the builtin
```

* **Module scope, before the binder**: the cell still holds the builtin → builtin
  (repro 1 forward-ref, repro 14 conditional def that never runs, loops at module
  scope that call before the def on iteration 1 and after on iteration 2).
* **Module scope, after the binder**: the user binding (last-wins; repro 5/10 chains).
* **Function bodies**: read the cell at CALL time → B2 (-35), B3 (5 then -35),
  `global` rebinds (repro 12), `del` (repro 13) all match CPython.
* **`type(x) == T` / `isinstance(x, T)` / the `str`/`repr`/`print` float fast paths**:
  a cell name is "declared" from the module start, so every specially-lowered fast path
  yields to the bare name and the value form (`__pyTypeInt` is the interned first-class
  type object: `type(3) == int` identity holds; integer-valued float literals are boxed
  `__pyF(2)`, so `str(2.0)` through the cell still prints `2.0`).

**Which names get a cell (JS emitter, `JsCodegen::module_cells`):** a name is a cell
iff it is bound at module level by ANY binder (§2) AND either
(a) it is a builtin the emitter lowers specially when unshadowed — one with a
    `builtin_value_mapping` (the init) or a `builtin_func_mapping` whose helper is a
    first-class runtime function, plus `__doc__` (init = the module docstring literal)
    and `breakpoint` (init = `(() => { debugger; })`); or
(b) it is a WASM-routed def (`wasm_skip`) whose name has a SECOND module-level binder
    (S3: `from math import sqrt` + `def sqrt`, `def f` twice) — init `undefined`; the
    cell exists so the hoisted glue import can bind under a hidden alias
    (`import { sqrt as __wasm$sqrt }`) and the def's source position reassigns
    (`sqrt = __wasm$sqrt;`) — no `Identifier already declared` SyntaxError.
Builtins with NO first-class value (`super`, `staticmethod`, `classmethod`,
`import_module`, and the unimplemented ones such as `open`/`input`/`hash`) keep the
pre-existing source-order lowering: a use before the binder is the existing LOUD
compile diagnostic (public #3), never a silent value.

**Mechanics reused, not duplicated:** the cell is emitted in the existing hoist section
(`collect_hoisted_names` → `export let …`) and `declare`d + `mark_hoisted` at module
start. From there every existing binder path already emits the assignment form:
`emit_func_def`/`emit_class_def` (`rebind_declared` → `N = function …`/`N = class …`),
the import Rebind plan (`is_declared(binding)` → `import { X as __pyimp_X_n }` +
`X = __pyimp_X_n;`, namespace imports likewise), `emit_assign` (declared → bare),
`emit_for` (`is_hoisted` → writes the module binding), walrus, `with … as`, `except …
as`, `global` writes. Two new arms: `del N` on a cell restores the init, and the
`wasm_skip` def arm emits `N = __wasm$N;` + `emit_wasm_reexports` imports the glue
export under `__wasm$N` (the `export let` already exports `N`).

## 2. Module-binder enumeration (the S4 authority — `pyths_hir::wasm_analysis::final_module_bindings`)

Every statement form that binds a module-level name, with the index of the TOP-LEVEL
statement it executes under (nested `if`/`for`/`while`/`try`/`with`/`match` bodies at
module level bind module names at that top-level statement's position):

| binder | `ShadowBinding` | notes |
|---|---|---|
| `def N` | `UserDef` | the only kind WASM may route |
| `from math import F [as N]` (F a WASM math fn) | `MathAlias(F)` | canonical bare import stays a math call; an alias demotes callers |
| `from m import X [as N]` (any other) | `Other("import")` | |
| `import N` / `import a.b` (binds `a`) / `import m as N` | `Other("import")` | |
| `class N` | `Other("class")` | |
| `N = …` / `a, (N, *r) = …` / `N: T = …` / `N += …` | `Other("assignment")` | targets recurse through tuple/list/starred |
| walrus `(N := …)` in any module-level expression | `Other("assignment")` | |
| `for N in …` / `with … as N` / `except E as N` | `Other("for-target")`/`("with-target")`/`("except-target")` | |
| `match` capture patterns | `Other("match-capture")` | |
| `del N` | `Other("del")` | restores the builtin at module scope |
| `global N` inside ANY nested def + a binding there | `Other("global write")` | rebinds at an unknowable call time → callers always demote |

Last binder in source order wins (`(index, kind)` per name). The JS emitter uses its
own already-complete binder walker (`collect_module_bound_names` — the shared
`collect_bound_names_in` walk under the MODULE rule — + deep `global` collection) for
the cell set; the WASM side uses this table. The two agree on the NAME SET by
construction (both enumerate every Python binding form under the same module rule —
in particular an annotation-only `x: T` with no value binds NOTHING at module scope,
PEP 526, while the FUNCTION-scope walk `collect_bound_names` keeps treating it as a
static local; SF1 of the independent review aligned the JS module census on this,
pinned by `test_module_annotation_only_is_not_a_binder`); a divergence is a bug in
whichever walker misses a form and shows up as a harness RED, never silent.

A `global N` WRITE inside a class METHOD (instance / `@staticmethod` / `@classmethod`)
is the same `Other("global write")` binder — both censuses descend through class
bodies — and the JS method emitter writes the cell through the ONE shared
function-scope prologue `open_function_scope` (push scope + `global` set + the #199
`global`/`nonlocal` pre-declare) that `emit_func_def` uses, so a method never emits a
method-local `let N` for it (BLOCKER-1 of the independent review; mutation M9).

## 3. WASM side (`wasm_analysis` admission + `pyths_codegen_wasm` emit)

A WASM function body bakes ONE resolution per name, so it may be routed only when
call-time resolution provably equals the FINAL module binding — otherwise it DEMOTES
to the cell-aware JS path (loud in `--verbose`, never silent):

1. **dead-by-name**: a `def N` is eligible only if it IS `N`'s final binder.
2. **Other-binder callee**: an eligible body calling `N` whose final binding is
   `Other(_)` demotes (S4: `abs = len`, `abs = lambda…`, `import math as abs`,
   `class abs`, `for abs in …`, `global abs`, `del abs`).
3. **aliased math callee**: `MathAlias(canon)` with bound name ≠ canon demotes (the
   WASM backend has no aliased-math call); the canonical bare import stays.
4. **B3 early call**: for every shadowed name `N` (any kind) with final binder at
   top-level index `b`, compute `early(N)` = names transitively REFERENCED (any `Name`
   occurrence, over the module def/class graph) by module-level code that executes at
   an index `< b` (for a `def` statement only its decorators/defaults/annotations
   execute at its position; a `class` executes entirely). Every eligible `F` with
   `F ∈ early(N)` whose body calls `N` demotes: it can run while `N` is still the
   builtin. Chains (`g → f → abs`, repro 19) demote via the call-consistency fixpoint
   once `f` is out. A chain called only after the final binder stays WASM (repro 20).
5. **S1 local shadow**: a body that CALLS a name it binds LOCALLY (param, assignment,
   for/with target, walrus, nested def) where that name is one the WASM emitter would
   otherwise lower specially (`WASM_CALL_BUILTINS`, math functions, a `from math`
   alias, or any module-shadowed name) is refused (CPython: `TypeError: 'int' object is
   not callable`; JS raises the same TypeError). A local closure named `f` is untouched.
6. **`for … in NAME(...)`** with a user-def `NAME` refuses (unchanged).

Emit consults the same `shadow_bindings`: `user_def_names` = names whose final binding
is `UserDef` or `Other` (never lower them as the builtin/math alias in `emit_call`, the
math-alias lookup, `is_range`, and the three inference paths); a `ctx` local always
wins over every builtin arm (defense in depth behind rule 5).

## 4. The twin-consistency invariant (fable B1)

The glue's overflow/trap twin `__jsfb.<fn>` is compiled by the JS emitter from a
`twin_module` = the ORIGINAL module's top-level `import`/`from … import` statements +
the WASM-compiled `def`s, in source order (`pyths_codegen_wasm::twin_module`, the ONE
constructor used by `pyths compile` and the tests — previously the CLI built a
defs-only module inline, which dropped every import). A twin body's free callee is
therefore resolved by the SAME binder set the WASM body was admitted under: a builtin
name with no binder → the builtin (`pyAbs`); a user def → the user def (the twin
module's own cell, `let abs = pyAbs; abs = function…` inside the `__jsfb` IIFE, whose
object literal captures the FINAL binding); a math import → the import. Rules 2–5
guarantee no other binder kind reaches a WASM body. Controls: `07_twin_overflow`
(user `abs` × 2**62 → the twin returns 32281802128991715328, not the builtin's
4611686018427387904), the math-import twin CLI test (a `floor` twin that would be a
ReferenceError under the old defs-only construction), and the unit test that a
`twin_module` carries the imports.

## 5. Controls (each mutation-verified; CPython value first)

`tests/differential/shadow_491/shadow_491.py` — a program-level 3-way differential
(CPython 3.12 vs `--target js` vs `--target js+wasm`, exact stdout + admitted-set
expectations) over `cases/*.ps`. Negative control: at the base commit `de345491` it is
**21 RED / 3 green** (`evidence/base-de345491.txt`). Per-repro map:

| # | case | property | RED at base |
|---|---|---|---|
| 1 | `01_forward_ref` | builtin before the def | glue `abs` re-export hoisted |
| 2 | `02_after_def_call_time` | B2 call-time user fn | `pyAbs` baked (5) |
| 3 | `03_type_eq` | user `int` in `type(x)==int`, module + body | body fast path (True) |
| 4 | `04_isinstance` | user `int` as class operand → TypeError | sentinel `"int"` (True) |
| 5 | `05_def_then_import` | later import wins, call-time | `use` bakes def |
| 6 | `06_demoted_caller` | demoted JS caller → re-exported user fn | green at base |
| 7 | `07_twin_overflow` | twin resolves user `abs` | green at base (both defs in twin); pinned by the CLI math-import twin test |
| 8 | `08_early_call_b3` | 5 then -35, `use_abs` demoted | WASM bakes user (-35, -35) |
| 9 | `09_local_shadow` | local param/assign shadow → TypeError | builtin value (2, 5) |
| 10 | `10_import_def_import` | loadable, last-wins chains | `Identifier already declared` / stale |
| 11 | `11a…11e` | assignment / lambda / import-as / class / for-target binders | `use` admitted, bakes builtin |
| 12 | `12_global_rebind` | `global` write rebinds the cell | bakes builtin |
| 13 | `13_del_restores_builtin` | `del` → builtin | `Assignment to constant variable` |
| 14 | `14_conditional_def` | never-executed def → builtin | TDZ/undefined TypeError |
| 15–20 | other builtins, math alias then def, float fast paths, late lambda, chains | as above |
| 21 | `21_method_global_write` | `global abs` write in a METHOD rebinds the cell | green at base by accident (no cell); RED at e937e9a1: method-local `let abs = 7` → stale `pyAbs` printed (silent) |
| 22 | `22_method_global_write_nonbuiltin` | non-builtin twin `global counter; counter = counter + 10` in a method | RED at e937e9a1: `let counter = pyAdd(counter, 10)` TDZ ReferenceError |
| 23 | `23_static_classmethod_global_write` | the `@staticmethod` / `@classmethod` arms | RED at e937e9a1 (same local `let`) |
| 24 | `24_classbody_global_write` | `global abs` write directly in a CLASS BODY rebinds the cell; a class-body read after it; no `C.abs`; `peek` demotes | RED at 4907954e: `__pyClassAttr(C, "abs", 7)` → stale `pyAbs` printed (silent), `peek` admitted |
| 25 | `25_classbody_global_write_nonbuiltin` | non-builtin twins: no-other-binder `g`, `x += 5`, tuple, MIXED tuple (`abs, k`), `del abs` | RED at 4907954e: `g` ReferenceError (loud), `x` stale 1 (silent), `abs` stale (silent) |
| 26 | `26_classbody_global_write_compound` | the write inside an `if`/`try`/`while` block at class-body level; `global` declared inside an `if` | RED at 4907954e: the block emitted RAW inside the JS class → SyntaxError (loud) |
| 27 | `27_classbody_nested_global_write` | the class nested in a FUNCTION (pre-declared global → bare cell write) and in a CLASS | RED at 4907954e: stale `pyAbs` (silent) on both |

Rust-level controls: `pyths_hir` unit tests per admission rule; `pyths_codegen_wasm`
`shadow_builtin_491.rs` (raw-WASM values) + `twin_consistency_491.rs`;
`pyths_codegen_js` `behavioral_differential.rs` (cell shape + behavioral rows);
`pyths_cli` node-backed js+wasm tests (twin overflow, math-import twin, B3). The
standing `wasm_net` ratchet (`shadow*` families) stays green with an unchanged row
count.

## 6. Verification ledger (evidence in `tests/differential/shadow_491/evidence/`)

| artifact | what it proves |
|---|---|
| `base-de345491.txt` | the harness at the base commit: **21 RED / 3 green** of 24 (the negative control of the whole class) |
| `fixed.txt` | the harness at the fix: **24 green / 0 RED** (CPython 3.12 oracle, `--target js` and `--target js+wasm`, admitted-set expectations included) |
| `wasm_net-base-de345491.txt` | the full `wasm_net` ratchet with the BASE binary (built in a throwaway worktree): 1 "regression" (`stmts::try_zero[direct]` RED→REFUSED, #496) + 9 stale-better arms (`arity_int0/float0` js+glue) — the committed `baseline.json` was stale relative to its own base |
| `wasm_net-fixed.txt` | the same net with the fixed binary: the identical verdict (401 rows / 15063 calls; the `shadow*` families green) — 0 regression attributable to #491; `baseline.json` re-pinned surgically for exactly those 3 pre-existing rows (13/16-line diff) |
| `mutation-ledger.txt` | 8 source mutations against the committed fix, each rebuilt + run + restored (`mutations.py`); **every mutation turned its paired control RED** |

Mutation ledger (the paired negative controls):

| mutation | disables | RED (harness cases / cargo control) |
|---|---|---|
| M1 | the JS cell (`builtin_cell_init` → None) | 02, 08, 12, 13, 14, 18 / `test_call_time_shadow_resolution_matches_cpython` |
| M2 | B3 early-call rule (`early_reachable` → ∅) | 08, 19 / `pyths_hir` `b3_*` |
| M3 | S1 at both layers (admission + emitter local check) | 09 / `pyths_hir` `s1_*` |
| M4 | twin imports (defs-only twin, the old CLI construction) | `twin_module_is_imports…`, CLI `twin_math_import_resolves_at_fallback` |
| M5 | S4 binder census (`Other` binders dropped) | 11a–11e, 12, 13 / `pyths_hir` `s4_*` |
| M6 | per-index `wasm_skip` (name-keyed, the base form) | 10 / CLI `import_def_import_chains_*` |
| M7 | class-cell call-time `__pyCall` dispatch | 11d / `test_call_time_shadow_resolution_matches_cpython` |
| M8 | the hidden `__wasm$N` glue alias (plain import beside the `let`) | 01, 02, 03, 06, 07, 08, 10, 15, 16, 19, 20 / `test_wasm_routed_shadow_wins_in_demoted_js_caller` |
| M9 | the method-scope #199 pre-declare (method emitter bypasses `open_function_scope`) | 21, 22, 23 / `test_method_global_write_*` (run by hand at the BLOCKER-1 commit: 3 RED / 24 green, both cargo controls FAILED — see `evidence/m9-method-global-by-hand.txt`) |
| M10 | the class-body global-write dispatch (`is_class_body_global_write` never fires → the installer emits `__pyClassAttr` again; the census still creates the cell) | 24, 25, 26, 27 / `test_class_body_global_write_*` (run by hand at the BLOCKER-2 fix: 4 RED, both cargo controls FAILED, `emit.rs` restored byte-identical — see `evidence/m10-classbody-global-by-hand.txt`) |

Rust controls: `pyths_hir/tests/shadow_491.rs` (9), `pyths_codegen_wasm/tests/{shadow_builtin_491,
twin_consistency_491}.rs` (3 + 2), `pyths_codegen_js/tests/behavioral_differential.rs` (20 incl. the
12-row call-time matrix, the 5-row method-`global` matrix and the 9-row class-body-`global` matrix),
`pyths_cli/tests/shadow_491_cli.rs` (5, node-backed). The impact enumeration covered every binder
form and scope that reaches the same call-time-resolution contract, plus the all-scope
`global N; N = v` emitter enumeration (BLOCKER-2 delta).

### 5a. The class-body `global` arm (#491 BLOCKER-2) — ONE condition, both emitters

CONDITION: *a name declared `global` anywhere in a class block is a MODULE binding for every
value-binding statement of that block (assignment / aug-assign / tuple / mixed-tuple element /
`del` / walrus / a compound block that binds nothing else), wherever the class is nested.*
- JS emitter: `ClassCtx.globals` = `collect_global_declared(body)`; `is_class_body_global_write`
  (statement binds ≥1 name, all `global` — or a compound block binding only such names) → skipped inside
  the JS class by `emit_class_body`, emitted post-class by `emit_class_def` as an ordinary statement of
  the enclosing scope, in class-body order (`abs = 7;` → the cell; `x += 5`, `del abs`, `if c: …` take
  their module lowering). A class nested in a FUNCTION pre-declares the block's globals in that scope
  (#199) so the write is a bare cell write, not a function-local `let`. A mixed tuple target splits per
  element (`global` element → module write, the rest `__pyClassAttr`).
- Censuses (agree by construction): `collect_global_declared_deep` (JS) and `collect_global_writes_deep`
  (HIR) enter a class body with THAT block's value binders as `in_def` (`class_body_value_binders` /
  `class_body_binding_names`: every binder form except the block's own `def`/`class` statements), so
  the `export let N = <builtin>` cell exists and a WASM caller of N demotes (case 24/26/27 `DEMOTE`).
  The B2 import-rebind hoist (`collect_global_declared` in `collect_hoisted_names`) treats a class-body
  `global` like a def's (safe over-approximation).

Residuals (loud or documented, never silent):
- `isinstance(3, <user fn>)` returns False where CPython raises `TypeError: arg 2 must be a type` — the
  RUNTIME half (`__pyIsInstance`), out of scope per the brief; the codegen half passes the user binding
  (case 04 uses `class int`, on which CPython and the runtime agree).
- A builtin with NO first-class value (`super`, `staticmethod`, `classmethod`, `import_module`, the
  unimplemented builtins) gets no cell: a use before its module binder keeps the existing loud compile
  diagnostic.
- `.d.ts` still declares `export function N` for a cell exported as `export let N` (type-level only; the
  pinned-TypeScript d.ts lane is green).
- #136 annotation trust (`-> float` printing a returned int as `2.0`) is shadow-independent and untouched
  (case 10 keeps its contract honest with `* 1.0`).
- #499 (the S3 `from math import X as N` + WASM-eligible `def N` double declaration) closes by
  construction (case 16 green; M8 reopens it).
- (CLOSED — BLOCKER-2, §5a) a `global N` write directly in a CLASS BODY: the builtin-shadow form was a
  SILENT stale value (`class C: global abs; abs = 7; print(abs)` → `<function pyAbs>`), the non-builtin
  twin a loud ReferenceError. Both now write the module cell (cases 24–27, M10).
- (CLOSED — BLOCKER-3, root) a `global N` access (write OR read) in a NESTED scope whose ENCLOSING
  FUNCTION binds `N` locally — `def outer(): abs = 1; def inner(): global abs; abs = 7; inner(); return
  abs` printed `7` / `<function pyAbs>` where CPython prints `1` / `7` (same for a class nested in such
  a function, a read-only `global abs; return abs(-5)`, a colliding PARAM, and every nesting depth —
  each intervening JS local captured in turn). Root: the emitted bare `abs = 7` was captured LEXICALLY by
  the enclosing JS `let abs`. ONE authority: the AST pre-pass `pyths_codegen_js::captured_global_rename`
  at the top of `emit_module` (the single seam every entry point + the glue twin pass through) renames
  the ENCLOSING function's colliding local to `N$l<k>` throughout that function under CPython's symtable
  rules (nested scopes that bind / `global`-declare `N` stop the rename; class bodies that bind or
  `global`-declare `N` stop it for their direct statements only; comprehension targets, lambda params,
  `nonlocal N` in nested scopes handled), so the nested access is a bare module-cell access by
  construction. Export-safe: only function locals are renamed; module cells keep their user names; the
  `.d.ts` is generated from the original module; a renamed PARAM keeps its Python name in `__pyparams__`
  / `__pyKwPop` / the `@component` prop key / the sentinel error message (`python_name`, parsed off the
  `$l<k>` tail — `$` never occurs in a Python identifier). Over-fix guard: no colliding nested `global`
  → no rename (byte-identical emission; case 31). Cases 28–31, `behavioral_differential
  ::test_nested_global_*`, M11 (pass OFF → 28/29/30 RED). The one shape the rename cannot express — a
  function-local DOTTED import with no alias (`import a.b`) whose head `a` a nested scope declares
  `global a` (aliasing it would bind the submodule) — is now REFUSED at compile time (SF2, B1 commit:
  `CapturedGlobalRename::refused` → `emit_import_error`; keyed on the deep `global` set directly, since
  the #438 census does not count a dotted import's head as a local); case 36 (`EXPECT: loud`),
  `captured_global_rename::tests::dotted_import_head_collision_is_refused`, `behavioral_differential::
  test_dotted_import_head_collision_is_refused_loudly`, M14. The pre-existing del-then-read shape
  (`abs = undefined; return abs` returns None instead of UnboundLocalError / NameError) is a different
  class, unchanged.
- **B1 — `global`/`nonlocal` WRITE-TARGET arms (the 6th silent arm; CLOSED as a class).** The #199
  prologue pre-DECLARED a `global`/`nonlocal` name (so `emit_assign` / aug / walrus / with-as / `del`
  wrote it bare) but never marked it HOISTED — and every arm that keys on `is_hoisted` (`emit_for` Name +
  tuple/list elements, `try_emit_range_for`, match `Capture`/`Star`, `except … as N`) emitted a fresh
  block-local: `def f(): global abs; for abs in [11, 22]` left the module `abs` as the builtin (CPython 22),
  the same for a plain name (`0` vs `3`), `nonlocal`, the range fast path, a match capture, a class-body
  `global` for-target nested in a function, and an `except … as e` under `global e` (a module reader saw
  `int` during the handler). ONE authority: `predeclare_outer_binding` = `declare` + `mark_hoisted`, used
  by `open_function_scope` and the class-body `global` path — both facts are set together, always, so an
  arm consulting either one agrees. `emit_for` also handles a PARTIAL pattern (`global c; for c, d in …`
  with `d` fresh) by declaring the fresh elements in a wrapper block and writing the pattern bare (the old
  all-or-nothing rule silently skipped the outer write); `except … as N` on an outer/hoisted name writes
  bare and UNBINDS on handler exit (CPython's `finally: N = None; del N`; builtin alias → restores the
  builtin) through `emit_unbind_name`, shared with `del`. SF1 folded in: a `global N` write with no
  module-level binder hoists `let N = __UNBOUND` at module top (CPython creates the global; a read before
  the write raises NameError via `__pyChkGlobal`; it was a strict-mode ReferenceError). Over-fix guards:
  a plain for-target keeps its per-iteration `const`, a comprehension target stays comprehension-local,
  SF1 never fires for a name with a module binder or a builtin cell. Cases 32–37, `behavioral_differential
  ::test_function_scope_global_target_arms_match_cpython` / `test_global_target_arms_emit_bare_outer_writes`,
  M12 (authority OFF → 32/33/34 RED) / M13 (SF1 OFF → 35 RED).
  **Scope caveat (match capture): "match `Capture`/`Star` CLOSED" above means the UNGUARDED capture — the
  WRITE-TARGET routing.** A GUARDED capture (`case [1, r] if r > 5`) still diverges silently, but that is an
  ORTHOGONAL, PRE-EXISTING `emit_match` bug — the guard is emitted BEFORE the capture bind, so it reads the
  pre-match value; reproduced with a plain LOCAL target (no `global`) at base `733d3fec`, i.e. NOT a #491
  arm and NOT a regression. Tracked as **#500** (guard-before-bind; fix = bind→guard→body). The #491 class
  (write-target routing) is closed; the guarded-capture value is #500's.
- CLOSED (loud): `global abs` + `abs: int = 7` in a function body — a CPython `SyntaxError: annotated name
  'abs' can't be global` — is now a codegen diagnostic (`is_outer_declared`, B1 commit; `nonlocal` too).
- OPEN (loud, documented): a class-body compound block that binds a MIX of `global` and class-attribute
  names (`global abs; for _ in range(1): abs = 7` — `_` is a class attribute) keeps the raw-block
  class-body lowering (a JS SyntaxError at load, never a value); and a `global`-declared `def`/`class`
  NAME in a class body keeps the method / nested-class lowering (exotic; the censuses exclude it too,
  so no cell is created — unchanged from HEAD).
