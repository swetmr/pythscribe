/- # C8HostInterop — host-interop preservation (C8), the NEW taxonomy axis

**Why C8 exists.** The taxonomy's C1–C7 are all SOURCE-FIDELITY axes — every
predicate references CPython. The int/float regression that forced the
Option-A revert exposed a missing axis: **target-host compatibility**.
Option A (int = bigint UNCONDITIONALLY, `c63d0dd7`) was a *faithful Python*
value model — its C2-side fidelity is PROVED below
(`preservationC2_optionA`, `preservationC2_tag_optionA`) — and it still
broke the JS host: a Python int reaching a native numeric sink arrived as a
`bigint`, and ECMAScript **ToNumber throws on BigInt** (the `slice(0n)`
class: node 2026-08-21, `[10,20,30].slice(1n)` → `TypeError: Cannot convert
a BigInt value to a number`; `Math.trunc(1n)` likewise), while the emitted
`.d.ts` declared that int as `number` (`crates/pyths_codegen_js/src/dts.rs`
L26 @ `6e990502`: `"int" | "float" => TsType::Number`) — a type-lie
(`typeof 1n === "bigint"`). No C1–C7 oracle could catch it, because none of
them mentions the host.

**C8 = host-interop preservation:** the emitted value is a well-formed
citizen of the JS host —
  (a) **native-sink safety + value fidelity**: a value of the numeric-sink
      domain arrives at a native numeric sink as a ToNumber-coercible
      carrier holding the right value (never a throwing `bigint`);
  (b) **`.d.ts` honesty**: the emitted declaration type equals the runtime
      `typeof` of the value AT THE HOST BOUNDARY (after the shipped
      crossing authority).

**What shipped (the target model's referent), minimal Option B `6e990502`:**
  * int stays the native hybrid — a safe-range int IS a plain `number`
    (`typeof "number"`), so native sinks and the `.d.ts` `number` claim
    hold with NO boundary authority at all;
  * an integer-valued float is boxed (`PyFloat extends Number`), and THE
    single unbox authority `__pyJs` (operators.js L485) strips the box at
    native-JS sinks (`typeof __pyJs(__pyF(8)) === "number"`, node
    2026-08-21). Even an UNSTRIPPED box is sink-safe — `PyFloat extends
    Number` coerces via the `[[NumberData]]`/`valueOf` slot
    (`[10,20,30].slice(new PyFloat(1))` = `[20,30]`, node 2026-08-21) —
    but `typeof` of the box is `"object"` (`boxF_typeof_object` below), so
    the `.d.ts` face is stated at the CROSSED value, which is exactly what
    the host receives.

**The discriminating witness = the REAL failed attempt, not a strawman.**
`C8_fails_optionA` refutes the SAME `C8Preserves` predicate at Option A
AS SHIPPED (`c63d0dd7`): `lowInt = bigint` unconditionally and NO
boundary-unbox authority for ints (`crossA = id` — A shipped none; `__pyJs`
did not exist there for ints and still has no bigint arm). The refutation
is at the exact regression witness class: `.pint 0` / `.pint 1` — an
ordinary small index. Both faces fail and both failures are pinned
(`optionA_bigint_at_sink`, `optionA_dts_lie`, the `hostSlice` guards).

**Honesty tier + scope.**
  * MODEL TIER, transliteration-bound: `__pyJs` L485, `PyFloat` L470–478,
    the hybrid int (`__norm` L456–457), dts.rs L26 — all @ `6e990502`;
    Option A's carrier @ `c63d0dd7` (the prior revision of C2TypeRepr.lean,
    now reverted, transliterated it — same provenance).
  * EMPIRICAL (node 2026-08-21, recorded above and in C2TypeRepr.lean):
    the `slice(1n)` throw, the box coercion, the `__pyJs` unbox, the
    `typeof` triple (number / bigint / object).
  * The numeric-sink DOMAIN is safe-range ints + integer-valued floats —
    the values the emit actually feeds to int-valued native sinks (indices,
    counts, lengths, host props). A |int| > 2^53 still crosses as `bigint`
    under B and would throw at a native numeric sink: that is a STATED
    boundary of B (CPython ints are unbounded; JS numbers are not), the
    same shape as Union7's WASM i64 range guard — not a hidden gap. The
    hypothesis is decidable and the domain inhabited (`#guard`s below).
  * A crossing that DEMOTED `bigint → number` at the boundary would satisfy
    this C8 fragment for Option A on safe ints — such an authority was
    never shipped (and for large ints it would be lossy); the refuted world
    is A-as-shipped, not the strongest imaginable A-variant. Stated, not
    hidden.
  * The WASM lane of host-interop ((c) in the taxonomy note — a WASM result
    marshalled to JS is a valid native value) is owned by JsWasmMirror.lean
    and is NOT re-claimed here. String/bool/container sinks and ToNumber on
    strings are out of fragment. `numX` (non-integer float) crosses as a
    native number trivially and carries digits, not an `Int`, so it is out
    of the Int-valued sink OBSERVATION (not of the host's good graces).
  * NOT shipping-bound FROM THIS FILE: the three interop-oracle lanes
    (JS-native sink / WASM-host / TS-`.d.ts`-run differentials) are the
    empirical binding and live with the fix branch; this file pins the
    correspondence by provenance + the recorded node runs. -/

import C2TypeRepr

namespace C8HostInterop

open C2TypeRepr

/-! ## The host observations -/

/-- Runtime `typeof` tags of the JS host (the C8(b) observation). -/
inductive TypeofTag where
  | tnumber | tbigint | tobject | tstring | tboolean
  deriving DecidableEq, Repr

/-- `typeof` of a carrier. A `PyFloat` box is an OBJECT (`typeof new
    PyFloat(8) === "object"`, node 2026-08-21) — which is why C8(b) is
    stated at the crossed value: `__pyJs` strips the box before the host
    sees it. -/
def typeofOf : JsVal → TypeofTag
  | .numI _ => .tnumber
  | .numX _ => .tnumber
  | .big _  => .tbigint
  | .boxF _ => .tobject
  | .str _  => .tstring
  | .bool _ => .tboolean
  | .arr _  => .tobject
  | .tup _  => .tobject
  | .map _  => .tobject

/-- The box is an object pyths-side — stated, not hidden (see header). -/
theorem boxF_typeof_object (n : Int) : typeofOf (.boxF n) = .tobject := rfl

/-- ECMAScript ToNumber at a native numeric sink, on the numeric carriers
    (the only carriers the modeled sinks receive): a plain integer-valued
    `number` passes with its value; a `PyFloat` box coerces via its
    `[[NumberData]]`/`valueOf` slot (`extends Number` — B's whole design
    point); a `bigint` THROWS (`TypeError: Cannot convert a BigInt value
    to a number` — the `slice(0n)` class), modeled as `none`. Carriers
    outside the Int-valued sink fragment are mapped to `none` and never
    fed to this observation by the C8 predicate (header scope note). -/
def toNumberHost : JsVal → Option Int
  | .numI n => some n
  | .boxF n => some n          -- [[NumberData]] coercion
  | .big _  => none            -- ToNumber(BigInt) throws
  | _       => none            -- out of the Int-valued sink fragment

/-- The emitted `.d.ts` type of a Python type, as the host will `typeof` it
    (dts.rs L26 @ `6e990502`: `"int" | "float" => TsType::Number`; str →
    `string`, bool → `boolean`, containers → JS objects). -/
def dtsOf : PTag → TypeofTag
  | .tint => .tnumber
  | .tfloat => .tnumber
  | .tstr => .tstring
  | .tbool => .tboolean
  | .tlist => .tobject
  | .ttuple => .tobject
  | .tdict => .tobject

/-! ## The crossing authorities (the second falsifiable parameter) -/

/-- The SHIPPED native-boundary crossing (minimal B): `__pyJs` — unbox a
    `__pyfloat__` box to its native number; every other value passes
    through untouched (operators.js L485 @ `6e990502`). -/
def pyJsB : JsVal → JsVal
  | .boxF n => .numI n
  | v => v

/-- Option A's crossing: A shipped NO boundary authority — nothing was
    boxed, ints crossed to native sinks as raw `bigint`. That absence IS
    the regression. -/
def crossA : JsVal → JsVal := id

/-! ## Option A as a `NumModel` (the reverted world, `c63d0dd7`) -/

/-- Option A (`c63d0dd7`, REVERTED): a Python int is a JS `bigint`
    UNCONDITIONALLY; a float (integer-valued or not) is a native `number`.
    Source-faithful (proved below) — host-breaking (refuted below). -/
def optionA : NumModel := ⟨fun n => .big n, fun n => .numI n⟩

/-- Option A's `typeName` dispatch (`c63d0dd7` L17–18): every `number` is a
    float, every `bigint` an int — no `Number.isInteger` heuristic, no box
    (the `boxF` arm is unreachable in A's image; tagged float for
    totality). -/
def jsTagOptionA : JsVal → PTag
  | .numI _ => .tfloat
  | .numX _ => .tfloat
  | .big _  => .tint
  | .boxF _ => .tfloat
  | .str _  => .tstr | .bool _ => .tbool
  | .arr _  => .tlist | .tup _ => .ttuple | .map _ => .tdict

/-! ## The C8 predicate (both knobs falsifiable, F10) -/

/-- **C8 host-interop preservation** for a value model `M` with boundary
    crossing `cross`, over the numeric-sink domain (safe-range ints +
    integer-valued floats — header scope):
    (a) the crossed value ToNumber-coerces at a native numeric sink to
        exactly the Python value (no `bigint` throw, no value drift);
    (b) the runtime `typeof` of the crossed value equals the emitted
        `.d.ts` type. -/
def C8Preserves (M : NumModel) (cross : JsVal → JsVal) : Prop :=
  (∀ n : Int, n.natAbs ≤ maxSafe →
      toNumberHost (cross (lowerM M (.pint n))) = some n) ∧
  (∀ n : Int, toNumberHost (cross (lowerM M (.pfloat n))) = some n) ∧
  (∀ n : Int, n.natAbs ≤ maxSafe →
      typeofOf (cross (lowerM M (.pint n))) = dtsOf (pyTag (.pint n))) ∧
  (∀ n : Int, typeofOf (cross (lowerM M (.pfloat n))) = dtsOf (pyTag (.pfloat n)))

/-! ## THE THEOREM — shipped minimal B is host-interop-safe -/

/-- The B-lowering of a safe-range int is a bare native number — the
    reason B needs no boundary authority for ints at all. -/
theorem lowerB_pint_safe (n : Int) (h : n.natAbs ≤ maxSafe) :
    lowerM optionB (.pint n) = .numI n := by
  simp only [lowerM, optionB, hybridInt]
  exact if_pos h

/-- **`preservationC8` — SHIPPED minimal B (`6e990502`) satisfies C8**:
    a safe-range int crosses as a native `number` untouched (sink-safe,
    `.d.ts`-honest, no authority needed); an integer-valued float is
    unboxed by `__pyJs` at the boundary to a native `number` (sink-safe,
    `.d.ts`-honest). -/
theorem preservationC8 : C8Preserves optionB pyJsB := by
  refine ⟨?_, ?_, ?_, ?_⟩
  · intro n h
    rw [lowerB_pint_safe n h]
    rfl
  · intro n
    rfl
  · intro n h
    rw [lowerB_pint_safe n h]
    rfl
  · intro n
    rfl

/-! ## THE REFUTATION — Option A as shipped violates C8 (both faces) -/

/-- Face (a) at the regression witness: under A an ordinary small int
    reaches the native sink as a `bigint` — ToNumber throws
    (`slice(0n)` class; node-recorded TypeError). -/
theorem optionA_bigint_at_sink :
    toNumberHost (crossA (lowerM optionA (.pint 0))) = none := rfl

/-- Face (b) at the regression witness: the emitted `.d.ts` says `number`;
    the runtime says `bigint`. The type-lie, machine-checked. -/
theorem optionA_dts_lie :
    typeofOf (crossA (lowerM optionA (.pint 0))) ≠ dtsOf (pyTag (.pint 0)) := by
  intro h
  exact TypeofTag.noConfusion h

/-- **`C8_fails_optionA` — the machine-checked A-regression**: Option A as
    shipped (`c63d0dd7`, identity crossing) violates C8 at the exact
    witness `.pint 0`. The SAME predicate shipped B satisfies. -/
theorem C8_fails_optionA : ¬ C8Preserves optionA crossA := by
  intro h
  have hsink := h.1 0 (by decide)
  rw [optionA_bigint_at_sink] at hsink
  exact absurd hsink (by decide)

/-! ## Option A is SOURCE-faithful (C2 axes) — the punchline's other half

The reverted world was not sloppy Python semantics: on the C2 fragment its
repr and tag dispatch are FULLY faithful. Together with `C8_fails_optionA`
this makes "we shipped a value model that satisfied source fidelity and
violated host interop" a pair of theorems, not prose. (Union8's `sep_C8`
packages this as the world-level separation.) -/

mutual
theorem preservationC2_optionA_val (v : PyVal) :
    jsReprM floatDisplay (lowerM optionA v) = reprRef v := by
  cases v with
  | pint n => rfl
  | pfloat n => rfl
  | pfloatX d => rfl
  | pstr s => rfl
  | pbool b => rfl
  | plist xs =>
    simp only [lowerM, jsReprM, reprRef]
    rw [preservationC2_optionA_list xs]
  | ptuple xs =>
    simp only [lowerM, jsReprM, reprRef]
    rw [preservationC2_optionA_list xs, lowerMList_length]
  | pdict kvs =>
    simp only [lowerM, jsReprM, reprRef]
    rw [preservationC2_optionA_pairs kvs]

theorem preservationC2_optionA_list (xs : List PyVal) :
    jsReprMList floatDisplay (lowerMList optionA xs) = reprRefList xs := by
  cases xs with
  | nil => rfl
  | cons v vs =>
    simp only [lowerMList, jsReprMList, reprRefList]
    rw [preservationC2_optionA_val v, preservationC2_optionA_listTail vs]

theorem preservationC2_optionA_listTail (xs : List PyVal) :
    jsReprMListTail floatDisplay (lowerMList optionA xs) = reprRefListTail xs := by
  cases xs with
  | nil => rfl
  | cons v vs =>
    simp only [lowerMList, jsReprMListTail, reprRefListTail]
    rw [preservationC2_optionA_val v, preservationC2_optionA_listTail vs]

theorem preservationC2_optionA_pairs (kvs : List (PyVal × PyVal)) :
    jsReprMPairs floatDisplay (lowerMPairs optionA kvs) = reprRefPairs kvs := by
  cases kvs with
  | nil => rfl
  | cons kv ps =>
    obtain ⟨k, v⟩ := kv
    simp only [lowerMPairs, jsReprMPairs, reprRefPairs]
    rw [preservationC2_optionA_val k, preservationC2_optionA_val v,
        preservationC2_optionA_pairsTail ps]

theorem preservationC2_optionA_pairsTail (kvs : List (PyVal × PyVal)) :
    jsReprMPairsTail floatDisplay (lowerMPairs optionA kvs) = reprRefPairsTail kvs := by
  cases kvs with
  | nil => rfl
  | cons kv ps =>
    obtain ⟨k, v⟩ := kv
    simp only [lowerMPairs, jsReprMPairsTail, reprRefPairsTail]
    rw [preservationC2_optionA_val k, preservationC2_optionA_val v,
        preservationC2_optionA_pairsTail ps]
end

/-- **Option A preserves C2 (repr)** — the reverted world was
    source-faithful on the representation axis. -/
theorem preservationC2_optionA :
    C2Preserves (lowerM optionA) (jsReprM floatDisplay) :=
  fun v => preservationC2_optionA_val v

/-- **Option A preserves C2 (tag)** — `type(x)` was right under A too. -/
theorem preservationC2_tag_optionA :
    C2PreservesTag (lowerM optionA) jsTagOptionA := by
  intro v
  cases v <;> rfl

/-- And B does NOT satisfy A's repr model nor vice versa — the two worlds
    are genuinely different lowerings, refutable against each other's
    knobs: B's carrier under A's float-display repr breaks on ints. -/
theorem optionB_under_A_repr_fails :
    ¬ C2Preserves lowerB (jsReprM floatDisplay) := by
  intro h
  exact absurd (h (.pint 8)) (by decide)

/-! ## The `slice(0n)` class, concretely (a modeled native sink) -/

/-- A native numeric sink of the `Array.prototype.slice` class: ToNumber
    the index (throw = `none`), then take the suffix. -/
def hostSlice (xs : List Int) (idx : JsVal) : Option (List Int) :=
  (toNumberHost idx).map (fun s => xs.drop s.toNat)

-- Shipped B at the sink — matches node 2026-08-21 `[10,20,30].slice(1)`:
#guard hostSlice [10, 20, 30] (pyJsB (lowerM optionB (.pint 1))) = some [20, 30]
-- …and a boxed float index EVEN WITHOUT the crossing (extends-Number
-- coercion; node: `[10,20,30].slice(new PyFloat(1))` = `[20,30]`):
#guard hostSlice [10, 20, 30] (lowerM optionB (.pfloat 1)) = some [20, 30]
-- Option A at the sink — the recorded TypeError (`slice(1n)` throws):
#guard hostSlice [10, 20, 30] (crossA (lowerM optionA (.pint 1))) = none

/-- SPOT: shipped-B sink success derived THROUGH `preservationC8`, not by
    evaluating the target side. -/
example : hostSlice [10, 20, 30] (pyJsB (lowerM optionB (.pint 1))) = some [20, 30] := by
  unfold hostSlice
  rw [preservationC8.1 1 (by decide)]
  rfl

/-- SPOT: the float-index crossing too — through the C8 float face. -/
example : hostSlice [10, 20, 30] (pyJsB (lowerM optionB (.pfloat 1))) = some [20, 30] := by
  unfold hostSlice
  rw [preservationC8.2.1 1]
  rfl

/-! ## Non-vacuity — the sink domain is inhabited across its breadth -/

#guard toNumberHost (pyJsB (lowerM optionB (.pint 0))) = some 0
#guard toNumberHost (pyJsB (lowerM optionB (.pint 8))) = some 8
#guard toNumberHost (pyJsB (lowerM optionB (.pint (-3)))) = some (-3)
#guard toNumberHost (pyJsB (lowerM optionB (.pint 9007199254740991))) = some 9007199254740991
#guard toNumberHost (pyJsB (lowerM optionB (.pfloat 8))) = some 8
#guard typeofOf (pyJsB (lowerM optionB (.pint 8))) = .tnumber
#guard typeofOf (pyJsB (lowerM optionB (.pfloat 8))) = .tnumber
-- the boundary of the modeled domain, stated (B's large int is a bigint —
-- outside the sink domain BY the safe-range hypothesis, not silently ok):
#guard toNumberHost (pyJsB (lowerM optionB (.pint 9007199254740993))) = none
#guard typeofOf (pyJsB (lowerM optionB (.pint 9007199254740993))) = .tbigint
-- Option A's failure is TOTAL on ints — even 0 and 1 throw:
#guard toNumberHost (crossA (lowerM optionA (.pint 0))) = none
#guard toNumberHost (crossA (lowerM optionA (.pint 1))) = none
#guard typeofOf (crossA (lowerM optionA (.pint 0))) = .tbigint
-- …while A's floats were fine at the sink (the failure is int-specific):
#guard toNumberHost (crossA (lowerM optionA (.pfloat 8))) = some 8

/-! ## Axiom pins (per-declaration; ⊆ the three standard, no native-decide escape) -/

/-- info: 'C8HostInterop.preservationC8' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationC8

/-- info: 'C8HostInterop.C8_fails_optionA' depends on axioms: [propext] -/
#guard_msgs in
#print axioms C8_fails_optionA

/-- info: 'C8HostInterop.optionA_dts_lie' does not depend on any axioms -/
#guard_msgs in
#print axioms optionA_dts_lie

/-- info: 'C8HostInterop.preservationC2_optionA' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationC2_optionA

/-- info: 'C8HostInterop.preservationC2_tag_optionA' does not depend on any axioms -/
#guard_msgs in
#print axioms preservationC2_tag_optionA

/-- info: 'C8HostInterop.optionB_under_A_repr_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms optionB_under_A_repr_fails

/-- info: 'C8HostInterop.lowerB_pint_safe' does not depend on any axioms -/
#guard_msgs in
#print axioms lowerB_pint_safe

end C8HostInterop
