# PythExpandVerify — Lean 4 verification of the `.psc` expander core

A compact, **machine-checked** Lean 4 model of the PythScribe `.psc` expander's
core rewrite properties, in the style of the Axon verified compiler
(Rinard 2026): a faithful *model* of the tier-rewrite system with the three
core safety/correctness lemmas proved end-to-end — not a full `.ps`→JS
semantic-preservation proof (deliberately out of scope; see the honest gap
statement below).

## Build status (verified)

```
$ cd verification && lake build
✔ [2/3] Built PythExpandVerify (1.3s)
Build completed successfully (3 jobs).
BUILD_EXIT=0
```

- Toolchain: `leanprover/lean4:v4.31.0` (pinned in `lean-toolchain`), installed
  via `elan`. Dependency-free — Lean core only, **no mathlib** — so `lake build`
  needs only the pinned toolchain, no package fetch.
- **0 proof holes, 0 custom axioms, 0 kernel-bypassing evaluation** — enforced
  by the CI trust-base audit.
- Axiom audit (`#print axioms`), pinned as a build gate with `#guard_msgs`:
  - **Segment / tier / route lemmas:** `propext` only.
  - **Classifier theorems over the executable scanner:** `propext`,
    `Classical.choice`, `Quot.sound` — Lean's three standard trusted axioms.
    These are *inherited from Lean core's `String`*, not introduced by our
    proofs: `#print axioms expandDictChars` reports the same three for the
    **unmodified** shipped definition. See the gap statement.

## What is modeled

Ground truth: `crates/pyths_expand/src/lib.rs :: expand_with_config`, a
FIXED-ORDER pipeline of position-aware, pre-parse *text* rewrites:

```
E (%NAME idioms) → A (presets/decorators) → B (kwarg aliases)
  → hooks (hook shorthand) → Dict ($NAME domain dictionary)
```

Each tier scans the source and rewrites recognised forms **only in code
zones**, emitting string / comment / f-string **protected zones** verbatim
(the scanner state machine in `strings.rs`, `idioms.rs`, `kwargs.rs`,
`hooks.rs`).

### Abstraction

| Rust reality | Lean model |
|---|---|
| Byte stream classified by the scanner into code vs string/comment/f-string zones | `Source := List Segment`, `Segment = code String \| prot String` |
| A tier's matcher, applied only outside protected zones | `Tier := String → String`, `applyTier` maps it over `code` segments only, `prot` verbatim |
| Fixed pipeline order (Steps 1–5) | `Config.pipeline = [tierE, tierA, tierB, tierHooks, tierDict]`; `runPipeline` folds head-first |
| `$NAME` dictionary tier internal behaviour (`strings.rs`) | token refinement: `Tok = txt \| ali \| can`, `elookup`/`rlookup`, `expandTok`/`compressTok` |

## The three core lemmas (all proved, 0 `sorry`)

1. **Determinism** — `expand` is a pure total function with a statically-fixed
   tier order.
   - `expand_order`: `expand c src` unfolds to exactly
     `applyTier tierDict (… (applyTier tierE src))` — no nondeterminism in tier
     ordering or selection.
   - `applyTier_length`, `runPipeline_length`, `expand_length`: left-totality —
     every segment is mapped, none dropped or invented (`(expand c src).length =
     src.length`).

2. **Zone-safety** — no tier rewrites inside a protected segment.
   - `zone_safety`: `∀ f src, protPayloads (applyTier f src) = protPayloads src`.
   - `pipeline_zone_safety` / `expand_zone_safety`: lifts it to the whole
     pipeline for **any** config — protected content (string/comment/f-string)
     is byte-identical in the output. This is THE key safety property: a `$`/`%`
     sigil inside a string literal can never expand.

3. **Alias round-trip on the alias domain** — for the `$NAME` dictionary tier,
   `expand ∘ compress = id` on canonical (`.ps`) source.
   - `roundtrip_tok`: single-token round-trip under `InverseConsistent D`.
   - `roundtrip_code`: `expandCode D (compressCode D xs) = xs` for any code
     segment `xs` in the alias domain (no `$alias` sigils present).
   - Non-vacuity witnesses: `nil_inverse_consistent` (trivial), plus
     `example_inverse_consistent` for a concrete 2-entry slice of the committed
     table, plus fully-`decide`d concrete round-trip `example`s.

4. **Classifier zone-safety (x18 closed, 2026-07-14)** — the char-level scan
   state machine itself, not just the tiers above it.
   - `zone_safety_chars`: every character consumed while the scan state is
     protected (single / double / triple-quoted string, `#` comment, escape
     pairs) is emitted **verbatim, in order** by `expandDictChars` — the SAME
     executable function the differential harness runs. Char-generic: covers
     `$`, `%`, and any future sigil.
   - `protChars_sublist_input`: the classifier selects only input characters,
     so "verbatim" means the same bytes, from the input, in the output.
   - `expandDictChars_prot_contiguous`: a zone is copied as an **uninterrupted
     block**, not merely as an interleaved subsequence.
   - Non-vacuity: kernel-checked evaluations pin the classifier in both
     directions (zone contents reported inside zones; nothing reported in code
     position, where the sigil really does expand).

### Bonus (proved, not required)

- `expandTok_idem` / `expandCode_idem`: the dictionary expand pass is
  idempotent — matches the Rust `dict_idempotent_on_canonical_input` test.

## Faithfulness argument (why the model matches the Rust `expand`)

A Lean proof about the *wrong* model proves nothing, so the abstraction is
justified point-by-point:

1. **Zone partition is the real invariant.** Every Rust tier (`strings.rs`,
   `idioms.rs`, `kwargs.rs`, `hooks.rs`) is the same scanner family: a byte
   loop with an `in_string` / comment / triple-quote state that emits protected
   bytes verbatim and only attempts a match in code state. `Segment` +
   `applyTier` encodes exactly this: `prot` payloads are structurally
   untouchable, so `zone_safety` holds for **any** `Tier` function regardless of
   its matcher internals — which is why we model tiers as opaque `String →
   String`. The proof therefore covers all six real tiers *and* any future one.

2. **Fixed order matches Steps 1–6.** `Config.pipeline` lists the tiers in the
   exact order of `expand_with_config` (idioms first … dictionary last), and
   `expand_order` pins the composition. The Rust order is a hard-coded sequence
   of six calls; the model mirrors it literally.

3. **Dictionary round-trip mirrors `strings.rs`.** `expandTok` reproduces the
   real behaviours: `$k` → canonical when `k` is in the table (first-match
   `elookup`), and unknown `$k` left as a literal `"$" ++ k` (the Rust "emit the
   `$` and pass through" branch). `compress` is the inverse-table lookup (we
   define it since only `expand` ships). The `InverseConsistent` hypothesis is
   precisely the committed table's structural invariant (distinct aliases and
   distinct canonicals ⇒ bijection), and the correctness contract we are
   modeling is the Iron Rule `canonicalize(expand(x.psc)) == canonicalize(x.ps)`
   specialised to the dictionary tier.

### Honest gap statement (revised 2026-07-14 — x18 closed)

**What changed.** The classifier is no longer assumed. Previously the segment
model *postulated* a correct zone partition and proved that every tier
respects it; the partition itself was only differentially tested. The
char-level scanner is now **proved zone-safe in Lean**. What survives as a gap
is strictly smaller, and we state it precisely:

- **PROVED (Lean).** The executable char-level scanner `expandDictChars` —
  *the same function* that `expandDictStr`, the `expanddiff` driver, and
  `diff_harness.py` run — emits every character it consumes inside a protected
  zone (single-quoted, double-quoted, triple-quoted, `#`-comment; escapes
  included) **verbatim and in order** (`zone_safety_chars`), and copies each
  zone as an **uninterrupted block** (`expandDictChars_scanProt`). The theorem
  is char-generic, so it covers the `$` dictionary sigil, the `%` idiom sigil,
  and any sigil added later. The classifier is pinned non-vacuously in *both*
  directions by kernel-checked evaluations: it reports the zone contents inside
  each zone kind, and reports nothing in code position — where the sigil really
  does expand.
- **NOT PROVED (the residue of x18).** That the **Rust** byte scanner in
  `strings.rs` *refines* the Lean char scanner. Lean reasons over `List Char`;
  Rust reasons over UTF-8 bytes with a hand-written `utf8_char_len`. The two
  are bound by the model-vs-implementation differential (418 generated cases,
  byte-identical, in CI), by a 300-case seeded property test, and — since
  2026-07-14 — by **bounded model checking of the Rust itself** (see "Kani"
  below), which is exhaustive to a bound but is *not* a refinement proof. A
  Rust refinement proof remains the open verification item. Do not read the
  Lean theorem as a statement about `strings.rs`; read it as a statement about
  the algorithm that `strings.rs` is differentially certified to implement.
- **THE RESIDUE IS NOT HYPOTHETICAL — it bit us (2026-07-14).** The very first
  Kani harness written against the Rust `zones.rs` refuted the in-bounds
  property of `string_step`: on a **truncated** multi-byte lead byte at the end
  of a buffer (`bytes = [0xF0]`, `i = 0`), `utf8_char_len` returned the length
  the lead byte *claims* (4) while 1 byte remained, so `i + len > bytes.len()`.
  Not a live bug — real buffers come from `&str` and `line_start_states` clamps
  with `.min(n)` — but an **unstated invariant that the `&[u8]` signature does
  not carry**. The 2,039-case differential could never have found it: the Lean
  model reasons over `List Char` and so *shares the assumption*. **A
  differential is blind to a bug both of its sides make.** `string_step` now
  clamps unconditionally and the property is pinned by a Kani harness.
- **PROVED for EVERY tier of the shipped expander (2026-07-14).** Tiers E, A, B, hooks and Dict
  are each instantiated as a concrete executable Lean function over a
  CI-pinned table, with a decided exactness theorem and a char-level
  zone-safety theorem — all five against the SAME classifier. See "The concrete tiers" below.
- **Tier A: refuted, then FIXED (2026-07-14).** Instantiating Tier A concretely
  *falsified* the segment model's central assumption for that tier: Tier A was a
  per-LINE rewrite with no zone state, and it rewrote a preset marker or
  decorator alias sitting alone on a line inside a triple-quoted string. The
  refutation was machine-checked against the real compiler by the differential —
  and the compiler was then fixed. Tier A now shares the one zone classifier
  (`zones.rs`), `tierA_zone_safety_chars` proves it zone-safe at the character
  level, and the docstring cases are pinned the correct way round in
  `gates.rs::tier_a_is_zone_aware_inside_docstrings` and in the `--tier tiera`
  corpus. `expand_zone_safety` (the segment-level theorem) is therefore a
  faithful abstraction of **every** tier in the shipped pipeline.
- **NOTHING IS LEFT UNINSTANTIATED.** Every tier the expander runs is a concrete
  executable Lean function over a CI-pinned table. The former Tier C (the PSX
  tag-DSL) was the one opaque slot — a recursive-descent parser with backtracking
  and **no alias table**, so the generate-table → `decide`-exactness → drift-gate
  method used for the other
  five has no object to apply to.
- **OUT OF SCOPE.** Full end-to-end `.ps`→JS semantic preservation (it would
  require modeling the entire Phase-1 compiler and would, per the Axon paper's
  finding, cause proof search to spin). This is deliberately a *rewrite-layer*
  verification only.
- **PERMANENT TRUST BOUNDARY — arbitrary JS interop (§10.8 item 8, decided
  2026-08-02).** Calls that cross into arbitrary host JavaScript — user-supplied
  JS libraries, DOM/browser APIs, `Promise`-returning host functions, React
  component internals — are **not a proof target and never will be**: the host
  side has no formal semantics to preserve against (there is no "CPython
  reference" for what `someNpmLib.frobnicate()` *should* do), and modeling V8
  itself is out of the question. The verified story STOPS at the helper/value
  boundary: what the Lean waves certify is that PythScribe's own emitted
  helpers and value representations are CPython-faithful *up to that boundary*
  (the value-representation relation of waves 8/11–20: bool⊂int, whole-float,
  UTF-16 vs code points, Map-backed dicts). Everything beyond it is covered by
  testing only — the jsinterop suite (43 pinned cases), the libinterop
  behavioral suite (28), and the Playwright E2E layers — and is recorded in
  `TRUST.md` at the *tested* tier, permanently. Do not attempt to promote it.

### Classifier zone-safety — the x18 closure

The theorem the 300-case property test could only *assert*, now proved:

```lean
theorem zone_safety_chars (D : Dict) (fuel : Nat) (st : Option ScanState)
    (cs : List Char) :
    List.Sublist (protChars D fuel st cs) (expandDictChars D fuel st cs)
```

`protChars` is a companion *classifier*: a traversal with the **same**
state-transition structure as `expandDictChars`, returning exactly the
characters consumed while the scan state is protected.
`protChars_sublist_input` proves those characters really come from the input
(the classifier cannot invent one); `zone_safety_chars` proves they all reach
the output, unchanged and in order. Contiguity closes the residual "a
subsequence could be interleaved" loophole:

```lean
theorem expandDictChars_prot_contiguous (D : Dict) (fuel : Nat)
    (st : ScanState) (cs : List Char) :
    ∃ (pre suf : List Char) (f' : Nat),
      cs = pre ++ suf
      ∧ expandDictChars D fuel (some st) cs = pre ++ expandDictChars D f' none suf
      ∧ protChars D fuel (some st) cs = pre ++ protChars D f' none suf
```

Entering a zone, the input splits `pre ++ suf`; the output is `pre` **verbatim**
followed by ordinary code scanning of `suf`; and `pre` is precisely the block
the classifier calls protected.

Crucially, `expandDictChars` was **not modified, re-modelled, or replaced** to
make this provable. The theorems range over the shipped function verbatim, so
the differential binding to `pyths expand` is preserved exactly — the same 418
cases still pass byte-for-byte.

**Trust base of these theorems:** `propext`, `Classical.choice`, `Quot.sound` —
Lean's three standard axioms. They are **inherited from Lean core's `String`**
(`expandDictChars` calls `String.ofList` / `String.toList`), not introduced by
the proofs: `#print axioms expandDictChars` reports the same three for the
*unmodified* definition, before any proof is written. The purely list-level
lemmas (`protChars_sublist_input`) remain `propext`-only. Every `#print axioms`
result is pinned with `#guard_msgs`, so a proof hole or a new axiom changes the
printed list and **breaks `lake build`** — the trust base is checked by the
build, not asserted in prose.

## Tier B — a concrete tier (added 2026-07-14)

Every tier above is an opaque `Tier := String → String`. That is what makes
`zone_safety` / `expand_order` / `expand_length` hold for *any* function — and
it is exactly why a reader may fairly call them near-tautological: they say
nothing about what a tier actually **does**.

Tier B (kwarg-position aliases, `crates/pyths_expand/src/kwargs.rs`) is now
**instantiated concretely** and the previously-assumed properties are proved
of it:

| Previously assumed of an arbitrary `Tier` | Now proved of the concrete `tierB` |
|---|---|
| maps each segment to exactly one segment | `tierB_length` |
| never rewrites inside a protected segment | `tierB_zone_safety` (segment level) **and** `kwarg_zone_safety_chars` (char level — a theorem about the scanner, not an assumption) |
| — (nothing was said about the rewrite itself) | `committed_kwargs_exact`: the rewrite **is** the committed 9-entry `kwargs.rs` table, *decided* on the real table |
| — | `kwargs_position_discipline`: fires only in call-argument position; never in statement position, never before `==`, never inside a string/comment, never on a non-table identifier |

`tierB := expandKwargStr committedKwargs`, where `committedKwargs` is
**generated** from `kwargs.rs::ALIASES` by `gen-kwarg-data.py` — exactly the
pattern `gen-dict-data.py` uses for the dictionary. The table is pinned from
both sides, so it cannot silently diverge:

- **Lean side:** `python verification/gen-kwarg-data.py --check` in CI fails on
  any drift in `KwargData.lean`.
- **Rust side:** `verification/model-manifest.txt` pins `kwarg-entries` +
  `kwarg-fnv1a64`, checked by `cargo test -p pyths_expand --test gates`.
- **Behaviour:** `python verification/diff_harness.py --tier kwargs` diffs the
  Lean tier against the real `pyths expand` — 432 generated cases,
  byte-identical, in CI. The corpus is restricted to constructs where tiers
  E/C/A/hooks/Dict are the identity, so `pyths expand ≡ Tier B` on it.

`committed_kwargs_exact` is the non-vacuity move, and it is the same one used
for the dictionary tier's `committed_inverse_consistent`: rather than assume a
property of the table, **decide** it on the shipping table. If the Lean tier did
not really implement the alias table, the file would not compile.

**What is still assumed for Tier B.** As with the dictionary tier, the Rust
byte scanner in `kwargs.rs` is not proved to *refine* the Lean char scanner —
that binding is the differential (432 cases).

## The concrete tiers — no tier stays abstract (2026-07-14)

The Tier-B recipe above is applied to **every tier of the shipped expander** —
all five are concrete executable Lean functions over CI-pinned tables, each with
a `by decide` exactness theorem and a character-level zone-safety theorem. There
is no abstract slot and no caveat. (`concreteConfig` in `PythExpandVerify.lean`
takes no parameter — that is the machine-checkable form of this claim, together
with `shipped_pipeline_is_five_proved_tiers`.)

| Tier | Lean function | Table (source of truth) | Exactness (`by decide`) | Char-level zone-safety | Differential arm |
|---|---|---|---|---|---|
| **E** — `%NAME` idioms | `tierE` = `expandIdiomStr committedIdioms` | *no compiler-side table* — fixture `idiom-table.toml` | `committed_idioms_exact` | `idiom_zone_safety_chars` ✅ | `--tier idioms` (373) |
| **A** — presets + decorators | `tierA` = `expandTierAStr committedPresets committedDecorators` | `presets.rs::PRESETS`, `decorators.rs::ALIASES` | `committed_tierA_exact` | `tierA_zone_safety_chars` ✅ | `--tier tiera` (416) |
| **B** — kwarg aliases | `tierB` = `expandKwargStr committedKwargs` | `kwargs.rs::ALIASES` | `committed_kwargs_exact` | `kwarg_zone_safety_chars` ✅ | `--tier kwargs` (432) |
| **hooks** — hook shorthand | `tierHooks` = `expandHookStr committedHooks` | `hooks.rs::ALIASES` | `committed_hooks_exact` | `hook_zone_safety_chars` ✅ | `--tier hooks` (400) |
| **Dict** — `$NAME` dictionary | `expandDictStr committedDict` | `strings.rs::ALIASES` | `committed_inverse_consistent` | `zone_safety_chars` ✅ | `--tier dict` (418) |

Every table is pinned from **both** sides — Lean via `gen-*-data.py --check`
in CI, Rust via the FNV block in `model-manifest.txt` checked by
`cargo test -p pyths_expand --test gates` — so neither side can move alone.
The differential (`diff_harness.py`) runs all five arms, **2,039 cases**,
byte-identical against the real `pyths expand`, in CI.

Beyond exactness and zone-safety, each tier's *position discipline* is decided
on the shipping table:

- `hooks_position_discipline` — fires only on a free identifier in **call**
  position; never after `.` (`obj.us(0)`), never inside a longer identifier
  (`x1us(0)`), never on a bare reference (`x = us`), never in a protected zone.
- `idioms_sigil_discipline` — `%` fires only before a name char (so `a % b` is
  modulo, untouched); names are **maximal-munch** (`%EMPTYb` is the unknown
  name `EMPTYb`, not `EMPTY` + `b`); unknown names pass through verbatim;
  expansions are **not** re-scanned.
- `tierA_line_discipline` / `tierA_zone_discipline` — a marker fires only as the
  **whole trimmed line**, at a line start, in a **code** zone; indentation is
  preserved and the marker's trailing whitespace dropped; a decorator alias keeps
  the rest of its line byte-for-byte; `x = R*`, `R* extra`, `R*  # note`, a bare
  `@`, and anything inside a docstring or comment are all untouched.

### One scanner skeleton, five instantiations (dedup refactor, 2026-07-17)

The zone-classifier ladder used to be written 4× — `protChars` (dict tier)
plus per-tier twins `protCharsK` (kwargs), `protCharsH` (hooks), `protCharsE`
(idioms), each with its own hand-mirrored scanner arms and its own chain of
sublist/zone-safety lemmas. It is now ONE parameterized skeleton in
`PythExpandVerify.lean`:

- **`ScanSpec σ`** — a tier, as data: its code-zone step
  (`codeStep : σ → Char → List Char → CodeOut σ`, returning
  emit / next-state / remainder) plus how its extra code-state reacts to
  zone entry/exit. The alias table lives inside `codeStep` and never enters
  any proof.
- **`scanChars sp` / `protScan sp`** — THE scanner and THE companion
  classifier: the zone state machine written once, lockstep by construction
  (not by hand-mirrored arms).
- **The generic ladder, proved once**: `protScan_sublist_input` (the
  classifier cannot invent input, given the tier's `codeStep` doesn't),
  `protScan_sublist_scanChars_R` (zone-safety as a simulation between ANY
  two consumption-lockstep specs), `scanChars_scanProt` /
  `protScan_scanProt` (contiguity), `scanChars_id` (an
  emit-what-you-consume spec is the identity — the empty idiom map).

Every tier-level theorem kept its exact name and statement and became an
instantiation; Tier A's `tierA_zone_safety_chars` reuses the ONE classifier
`protChars` via the simulation with `R := ⊤` plus a one-lemma lockstep fact
(`tierAStep_rest_eq_dictStep`). What a NEW tier costs today:

```lean
def myStep (T : Dict) (s : σ) (c : Char) (rest : List Char) : CodeOut σ := ...
def mySpec (T : Dict) : ScanSpec σ := ⟨enterStr, enterComment, exitComment, myStep T⟩
def expandMyChars (T : Dict) : Nat → Option ScanState → σ → List Char → List Char :=
  fun fuel st s cs => scanChars (mySpec T) fuel st s cs
theorem myStep_rest_sublist ... := by unfold myStep; repeat' split; ...   -- ~8 lines
theorem my_zone_safety_chars ... := protScan_sublist_scanChars (mySpec T) fuel st s cs
```

— the tier's code step plus one ~8-line `codeStep`-never-invents-input
lemma; the whole ladder (classifier, sublist-input, char-level zone-safety,
contiguity) comes for free.

Two disciplines worth naming. **Zone flags:** the skeleton carries a tier's
code-state through zone arms and resets it at zone ENTRY (`enterStr` /
`enterComment` / `exitComment`); the old hand-written scanners reset inside
every zone arm. From the `String` entry points the two are byte-identical —
pinned by the `#guard` fixtures, the decided discipline theorems, and the
2,039-case differential, all byte-identical across the refactor.
**Axiom hygiene:** the dict step is parameterized over its emission
(`dictStep em`), and the classifier's spec instantiates `em := fun _ => []`,
so `protChars`'s constant graph never touches `String.toList` (which carries
`Classical.choice` / `Quot.sound` from Lean core) and
`protChars_sublist_input` keeps its `propext`-only pin — every `#guard_msgs`
axiom pin in the file is byte-identical to before the refactor.

### Tier E — an honest asymmetry

Tier E is concrete but differs from the others in one respect that must not be
glossed: **there is no compiler-side idiom table.**
`idioms::substitute_with_map` takes its `%NAME` map from `[expand.idioms]` in
the user's `pyths.toml`, which is **empty by default**. So:

- The Lean theorems are stated for an **arbitrary** table `M` — strictly
  stronger than a fixed-table statement.
- What the differential pins is the shipped **scanner**, over a committed
  fixture table (`verification/idiom-table.toml`) that is fed to *both* sides:
  `gen-idiom-data.py` compiles it into `IdiomData.lean`, and
  `diff_harness.py --tier idioms` copies it into the scratch dir as
  `pyths.toml` so the real `pyths expand` uses exactly the same entries.
- `expandIdiomStr_nil` proves the **default (empty-map) configuration is the
  identity** — which also proves the Rust early-return (`if map.is_empty() {
  return src }`) is a sound optimization, not just a fast path.
- `idiom_digit_name_intercepts_modulo` machine-checks the hazard that
  `idioms.rs`'s own module doc warns about: an all-digit idiom name silently
  intercepts `x %10` (legal Python for `x % 10`). The model is deliberately
  **bug-compatible**, and the differential proves the Rust scanner agrees.

### Tier A — the line-oriented tier, made zone-aware

Tier A rewrites LINES, not characters: a line whose trimmed body is exactly a
preset marker becomes `indent ++ canonicalImport`; a line whose first
significant character is `@` followed by an alias in the decorator table has
that alias replaced and the rest of the line copied verbatim.

It used to do this with **no string / comment / triple-quote state whatsoever**,
so a marker alone on a line inside a docstring was expanded — corrupting the
docstring's text. That defect was surfaced *by* the concrete instantiation
(while Tier A was an opaque `Tier`, `applyTier` silently granted it a
code-segments-only property it did not have), machine-checked as a refutation,
and then **fixed in the compiler**: `lib.rs` now gates the line rewrite on
`zones::line_start_states`, the shared classifier every tier uses. A line that
begins inside an unterminated string or docstring is emitted byte-for-byte.

```lean
theorem tierA_zone_safety_chars : ∀ (fuel : Nat) (st : Option ScanState)
    (m : TierAMode) (cs : List Char),
    List.Sublist (protChars committedDict fuel st cs)
      (expandTierAChars committedDict committedPresets committedDecorators fuel st m cs)

theorem tierA_zone_safety (s : String) :
    List.Sublist (protCharsStr committedDict s) (tierA s).toList
```

The statement is the positive analogue of `zone_safety_chars` and ranges over
the *same* companion classifier `protChars` — one notion of "protected", shared
by all five instantiated tiers. In Lean, Tier A is modelled as a character
scanner whose zone arms are, arm for arm, the arms of `expandDictChars`, with
three code-zone modes (`lineStart` / `dropMarker` / `dropAlias`) carrying the
line structure; the zone transitions are taken *before* the mode logic, so a
zone opener can never be dropped or reinterpreted, whatever mode Tier A is in.
`tierA` is the function `expanddiff --tiera` runs, so the differential binds the
theorem to the shipping compiler.

What the differential corpus (`--tier tiera`, 416 cases) now certifies
byte-for-byte on the real `pyths expand`: every marker and every alias inside
both flavours of triple-quoted string (indented, with trailing whitespace,
nested and adjacent quotes, escaped quotes) is emitted verbatim — and the same
marker in a real code position, including on the line immediately after a closed
docstring, still expands.

One behaviour changed with the fix: a decorator line keeps everything after the
alias byte-for-byte, *including trailing whitespace* (`@c··` → `@component··`).
It has to: on a line whose args open an unterminated string, that trailing
whitespace is inside the string, i.e. protected. Preset lines still drop the
marker's trailing whitespace, which is zone-safe because a preset line is pure
code (the guard is whole-trimmed-line equality against a table of markers that
contain no quote and no `#`).

### Why there is no longer a PSX tier

A sixth tier — the PSX tag-DSL (`psx.rs`, angle-bracket markup compressed to
`tag(attrs)(children)`) — used to sit at Step 2. It was the **only** tier this
method could not reach, and it has been **removed from the expander** rather than
left as a permanent asterisk. Two independent reasons converged:

1. **It could not be proved.** It was not table-driven: `psx.rs` contained no
   alias table of any kind (the `cl`→`className`, `oc`→`on_click` mappings people
   associate with PSX actually belong to the *later* kwarg/hook/dict tiers). It
   was a flat outer scanner delegating to a *recursive-descent parser* with
   snapshot/restore backtracking, building an AST and re-emitting from it. The
   method used for every other tier — generate the table, `decide` exactness
   against it, FNV-pin it, gate drift — had **no object to apply to**. Worse, a
   faithful port would have had to be bug-compatible with at least three
   accidental Rust behaviours (a `usize` underflow in `read_attr_value`, a
   char-index-vs-byte-length comparison in the paren-strip check, and a
   `len() - 1` underflow on the attribute value `()`): "port the spec" and "port
   the implementation" gave *different* answers.
2. **It did not even pay.** Under the fixed frontier tokenizers `.psc` actually
   targets, the markup form **lost** tokens to the Pythonic call form it was
   supposed to beat — a closing tag repeats the element name (`</div>`), a cost
   `div(...)(...)` never pays. Converting the two idiomatic corpora off PSX made
   them *cheaper*: the o200k `.psc`→`.ps` increment improved from +7.6% to +8.9%.

So the tier bought no tokens and cost the proof its completeness. Removing it is
strictly dominant, and it is why the tier table above has no "—" row: **every tier
of the shipped expander is proved zone-safe.** The `Config` structure carries five
tiers, `concreteConfig` takes no abstract parameter, and
`shipped_pipeline_is_five_proved_tiers` pins the pipeline to exactly those five.

## Effort metrics (Axon-style)

| Metric | Value |
|---|---|
| Total source LoC | 314 |
| Named theorems | 13 |
| `example` proof obligations (concrete witnesses) | 3 |
| `def` / `abbrev` / `structure` / `inductive` | 22 |
| Proof : definition ratio | ~16 obligations over 22 model defs — a small, sharp model |
| Dependencies | none (Lean core only) |
| Axioms used | `propext` (list-level lemmas); `propext` + `Classical.choice` + `Quot.sound` (classifier theorems — inherited from Lean core `String`, see gap statement). Pinned by `#guard_msgs`. |
| `sorry` / `admit` | 0 |
| Proof-attempt iterations to green | 2 `lake build` runs (1st: 15/16 green, only the concrete `example_inverse_consistent` `ite` case-split failed; 2nd: all green after switching `by_cases`+`if_pos` to `split`) |
| Spin incidents | 0 — no theorem was abandoned; the one failure was a tactic-syntax mismatch (`if_pos rfl` vs `split`), fixed in one edit, not a proof-search spin |

The single failure-and-fix (a `subst` on a non-`x=t` `ite` residue) is exactly
the Axon-style "widen/adjust rather than grind" move: swapped the manual
`by_cases`/`if_pos` reduction for the `split` tactic and it went green
immediately.

## Reproduce

```bash
# one-time: install elan (Lean toolchain manager); auto-fetches lean4 v4.31.0
curl -sSf https://elan.lean-lang.org/elan-init.sh | sh -s -- -y
export PATH="$HOME/.elan/bin:$PATH"

cd verification
lake build          # → Build completed successfully, BUILD_EXIT=0
```

## Kani — bounded model checking of the Rust classifier (added 2026-07-14)

A **distinct assurance layer**, sitting between the differential corpus and the
Lean proofs. Full detail in [`../KANI.md`](../KANI.md); the epistemic status,
stated here so nobody has to click through:

> **Bounded model checking (Kani, CBMC-based): exhaustive for all inputs up to
> N bytes.** Strictly *stronger* than the differential corpus (2,039 fixed
> cases — those cases and no others). Strictly *weaker* than the Lean proofs
> (unbounded — but about a *model*). **It does not license the word
> "verified."** A green run licenses exactly one sentence: *no input of at most
> N bytes falsifies this property*.

What it is for: Kani is the only layer that is **both about the shipping Rust
and exhaustive**. Lean is unbounded but proves things about `List Char`; the
differential binds the two but can only ever check the cases in it — and, as the
x18 residue note above records, **it is structurally blind to any bug the model
and the implementation both make**. Kani is where such bugs die. It caught the
`string_step` truncated-lead-byte precondition on the first run.

| | Scope | Exhaustive? | About the shipping Rust? |
|---|---|---|---|
| Differential corpus | Lean model vs `pyths expand` | no (2,039 cases) | yes |
| **Kani** | `zones.rs` + the tier rewrites | **yes, to N bytes** | **yes** |
| Lean | a model of the expander | yes (unbounded) | no (a model) |

Harnesses live in `crates/pyths_expand/src/kani_proofs.rs`, behind `#[cfg(kani)]`
— invisible to `cargo build` / `cargo test`, and the workspace takes **no** Kani
dependency. Nine harnesses exist; **five are gated in CI** and four are not.

**Gated (all observed green, each < 20 s)** — every one over *unconstrained*
`&[u8]`: `string_step` progress + in-bounds; the truncated-lead-byte regression
pin; a proof that the clamp is a **no-op on valid UTF-8** (which is why the
differential stayed byte-identical); escape-pairs-never-close-a-zone; and
`utf8_char_len` vs the real encoding.

**Not gated — they do not converge:** `code_step` progress + in-bounds,
`line_start_states` one-entry-per-line, and the two tier-level zone-safety
harnesses over the shipping `strings::substitute` (`$`) and
`idioms::substitute_with_map` (`%`). The split falls on one line: **these four
take `&str`**, so the harness must `assume(from_utf8(buf).is_ok())`, and CBMC
pays for that UTF-8 validity constraint on every symbolic buffer. Measured:
`line_start_states` did not converge in 59 min at N = 6; `code_step` passed 15 GB
of solver memory at N = 8. They stay in the source and run on demand — a gate
nobody has watched go green is not a gate. Closing them needs a cheaper encoding
of the UTF-8 precondition (build the buffer from symbolic `char`s, making
validity true by construction). **Open work, not a claim.**

Consequence worth being explicit about: **Kani is currently a floor under the
classifier, not under the tiers.** Tier-level zone safety rests on the Lean proofs
plus the 2,039-case differential, as before.

```bash
# CI job "Kani (bounded model checking)" runs the five gated harnesses by name.
cargo kani -p pyths_expand --harness string_step_progress_and_in_bounds
```

Two caveats, kept in the open: results hold **only up to the bound**, and the two
tier-level harnesses restrict the symbolic alphabet to the eight bytes the
classifier branches on (`$ % ' " # \ \n a`) to stay inside the CBMC budget. See
KANI.md § "The two honest caveats".

## Continuous enforcement (added 2026-07-10 — P0 gap-closure)

Two CI gates keep these proofs live instead of a one-time artifact:

1. **`verification` job** (`.github/workflows/ci.yml`): `lake build` on
   every push + PR (elan pinned by `lean-toolchain`, cached), plus a
   trust-base audit step that fails on any `sorry` / `admit` / `axiom` /
   `native_decide` entering the development.
2. **Model drift gate** (`cargo test -p pyths_expand --test gates`):
   `verification/model-manifest.txt` pins the tier order (`expand_order`'s
   subject) and the `$NAME` dictionary domain (FNV-1a 64 over the sorted
   `alias=canonical` table). If `pyths_expand` changes either, the Rust
   test fails with the exact replacement block — forcing the model and
   the manifest to move together. The same test file carries the
   `dict_audit_*` reversibility invariants and the zone-classifier
   property test (the model's assumed-correct partition, checked against
   the real classifier with sigils embedded in every protected-zone kind).

## Bound to the shipping compiler (added 2026-07-10 — §7.8)

The abstract-dictionary hypotheses are now discharged on the real table,
and the model is executable and differentially bound to the compiler:

| Artifact | What it adds |
|---|---|
| `DictData.lean` (generated) | The committed 61-entry `$NAME` table, generated from `strings.rs` by `gen-dict-data.py`; CI regenerates + diffs (stale = fail) |
| `invConsistentB` + `invConsistentB_sound` | Decidable checker for `InverseConsistent` + soundness proof (the Move-borrow-checker recipe) |
| `committed_inverse_consistent` | The round-trip hypothesis DECIDED on the real table — the theorems are demonstrably non-vacuous |
| `committed_roundtrip` | `expand ∘ compress = id` on canonical code over the shipping dictionary, hypothesis-free |
| `expand_characterization` | One composed statement: fixed order ∧ left-totality ∧ zone-safety (§7.3) |
| `expandDictStr` + `lake exe expanddiff` | Executable char-level port of `strings.rs::substitute_with_dict` (zone classifier + `$NAME` lookup), with `#guard` smoke checks |
| `diff_harness.py` | Model-vs-implementation differential: 418 generated cases (every alias × every zone kind, escapes, unknown aliases, edge sigils), byte-compared against real `pyths expand` in CI. Mutation-verified: dropping single-string escape handling in the model is caught with line-level diffs |
| `MessageData.lean` (generated) + `message-table.json` + `message_shipped_binding.py` | **E7 message-layer binding (2026-08-27).** The C1C3C4 exception-message literals are externalized to `message-table.json`; `gen-message-data.py` generates `MessageData.lean` from it (CI `--check` drift gate, the DictData pattern), and `C1C3C4Outcome.lean`'s LIVE model messages are built from `MessageData` (false-world 3.12 wordings stay inline — they encode refuted worlds). `message_shipped_binding.py` (CI) runs all 45 table witnesses through the REAL `pyths` binary AND the pinned CPython oracle and asserts both terminal `Kind: message` lines equal the table — so a runtime message change turns the differential red, and the forced table update re-evaluates the Lean `#guard` pins. Root-fixes the 3.14 oracle-bump transcribed-literal drift (the Lean gate stayed green asserting an obsolete 3.12 message) |

Epistemic note: `expandDictStr` is deliberately NOT proof-covered — the
byte-level classifier is the model's stated trust boundary (our x18).
Its correctness claim is the differential, which is the point: proofs
guarantee properties of the model; only differential testing of an
executable model against the real implementation shows the model *is*
the implementation.

## Credible compilation — subscript routing (added 2026-07-10 — §7.2)

The Axon move applied to one codegen pass: rather than prove
`emit.rs`'s subscript lowering correct forever, every compilation can
carry a certificate that is independently validated.

- `cert.rs::route` is now the SINGLE decision procedure (emit.rs
  matches on its verdict — certificate and artifact cannot disagree by
  construction); `--emit-cert` writes `<out>.js.cert.json` and rejects
  the compilation on any violation.
- `cert.rs::check_certificate` re-applies the rules to every recorded
  site and cross-checks pyGetItem/pySlice call-site counts in the JS.
- The **RouteModel** section here proves `route_read_safety` (the #22
  "correctness > savings" rule as a theorem: no Python-typed plain read
  can lower to a bare native subscript), slice totality, the inbounds
  gate, and route exhaustiveness.
- **Binding**: `verification/route-table.txt` (112 rows — every
  flag/type combination) is checked from BOTH sides: Lean via
  `lake exe expanddiff --check-route-table`, Rust via
  `cargo test route_table_matches_committed_fixture`. The whole
  `examples/` corpus compiles certificate-ACCEPTED in
  `tests/cert_corpus.rs`.

Trust boundary stated plainly: the receiver-type evidence comes from
`emit.rs::infer_type` and is trusted by the checker (validating rule
application + artifact consistency, not re-inferring) — the inference
itself stays covered by the CPython differential and the 1,150+ test
suite.

## Credible compilation — WASM auto-routing admission (added 2026-07-16)

The same Axon move applied PAST subscript lowering, to the compiler's
headline "auto-routes numeric functions to WASM" feature. The question:
does the compiler ever admit to WASM a function it cannot actually lower?

- **The soundness claim (HONEST).** For every function admitted under
  `--target js+wasm` (`pyths_hir::analyze_module`), each boundary type
  passes `is_wasm_eligible` **and** has a concrete WASM representation
  (`crates/pyths_codegen_wasm/src/types.rs::to_wasm_type` returns `Some`).
  This is **soundness only** — NOT completeness (we do not claim we admit
  everything lowerable) and NOT value-equivalence (the JS/WASM lowerings
  computing the same value is the Livermore-WASM differential's job).
- **A real over-admission it caught + fixed.** `is_wasm_eligible` used to
  accept `Type::Optional`, which `to_wasm_type` has no arm for — so
  `def f(x: Optional[int])` was admitted and then PANICKED codegen at
  `emit.rs`'s `to_wasm_type(ty).unwrap()`. The fix aligns admission with the
  lowering (Optional falls back to JS); regression:
  `wasm_analysis.rs::test_optional_param_not_eligible`.
- **The theorem.** The **WasmAdmission** section of `PythExpandVerify.lean`
  models `isWasmEligible` and `toWasmType` arm-for-arm and proves
  `wasm_admission_sound : isWasmEligible t = true → (toWasmType t).isSome`
  (+ `wasm_admission_total`). Trust base `[propext, Quot.sound]`, pinned by
  `#guard_msgs`; 0 `sorry`/`admit`/custom-axiom/`native_decide`.
- **Per-compilation certificate.** `cert.rs::build_certificate` records every
  admitted function + its boundary types; `check_certificate` re-applies
  `is_wasm_eligible`, re-checks `to_wasm_type` is `Some`, and cross-checks the
  emitted `.wasm` export section (admitted names present; rejected names
  absent). `--target js+wasm --emit-cert` writes `<out>.js.wasm.cert.json`
  and rejects the compilation on any violation. Whole-`examples/` corpus:
  `tests/admission_cert.rs`.
- **Binding**: `verification/wasm-admission-table.txt` (232 boundary shapes ×
  `is_wasm_eligible`/`to_wasm_type` bits) is checked from BOTH sides — Lean
  via `lake exe expanddiff --check-wasm-admission-table`, Rust via
  `cargo test wasm_admission_table_matches_committed_fixture`. Every row also
  finitely witnesses the theorem (no row is elig=1, lower=0).

Trust boundary stated plainly: the **body-fragment** evidence
(`analyze_module::check_body`) is trusted by the checker — the machine-checked
half is the *boundary* (types + lowering) + artifact consistency; the body
fragment stays covered by the CPython + Livermore-WASM differentials.
Modeling scope: the Lean twin covers the binary-tuple / unary-callable shapes
the table enumerates (the Rust functions handle arbitrary arity; the
all-inner argument is identical at every arity, and the n-ary residual is
covered by the corpus certificate + `admission_table_is_sound_on_every_row`).

## JS↔WASM value-marshalling boundary table (added 2026-08-16)

The third table-gate: the JS↔WASM VALUE boundary (bridge.rs
`convert_js_to_wasm` / `convert_wasm_to_js` / `list_elem_kind` + the
`__i64Oob` argument guard, the `__list_to_wasm` i64 element guard, the sticky
`__ovf` flag and the #364 fault ladder), enumerated as the finite
`verification/marshalling-table.txt` — 56 rows: 23 shapes × (arg + ret) with
the LITERAL emitted conversion expression + the #364 admission bit, plus 10
explicit failure-disposition rows (`fault <event> <mode> -> <action>`).

- **Theorems** (MarshalTable section of `PythExpandVerify.lean`):
  `marshal_param_admitted_sound` / `marshal_ret_admitted_sound` (the admitted
  boundary marshals ONLY through value-exact numeric converter classes;
  pointer marshalling is formally excluded), `i64ArgMarshal_exact` +
  `i64_boundary_roundtrip` (the i64 crossing passes the EXACT value or
  diverts — never a silent wrap; `[propext, Quot.sound]`), and
  `marshal_exhaustive` (the finite table covers the whole infinite
  representable domain — conversion depends only on head constructor +
  element kind). Stubs: `i64Marshal_unguardedStub_fails` (raw ToBigInt64
  wraps 2⁶³ → −2⁶³; axiom-free) and `listInt_i32KindStub_fails` (a mis-kinded
  int list truncates 2³²+1 → 1; axiom-free).
- **Two-sided binding**: Lean `lake exe expanddiff --check-marshalling-table`,
  Rust `cargo test marshalling_table_matches_committed_fixture` — the Rust
  side DERIVES every row from the shipping code (the fault rows from real
  probe bridges with loud-panic snippet assertions). Plus
  `admitted_arg_rows_use_exact_marshallers`,
  `marshalling_table_checker_rejects_forged_row`, and
  `every_disposition_class_is_witnessed`.
- **Shipping binding**: `verification/marshalling_shipped_binding.py` — real
  `pyths --target js+wasm` vs CPython over boundary-crossing values (22
  observations: exact passes at the i64 edges, oob scalar args, oob list
  elements, in-WASM result overflow, exceptions), with a FALSE-WORLD control
  (guard-deleted glue must diverge, reproducing the pre-fix wrap).
- **A real silent-wrap bug it caught + fixed.** The `__list_to_wasm` i64
  ELEMENT path had no range guard — `DataView.setBigInt64` wraps mod 2⁶⁴, so
  `pick([2**63+7])` crossed as −9223372036854775801 (scalars were already
  `__i64Oob`-guarded). Fixed by the element RangeError guard (twins mode:
  fault ladder → exact JS twin; edge: loud throw), pinned by the
  `list-elem-i64-oob` rows + `list_i64_elements_are_oob_guarded`.
- **Scope**: this is the JS↔WASM boundary only. The client↔server (RPC) and
  JS↔TS boundaries are the same table shape but 0.3.x scope — not built.
