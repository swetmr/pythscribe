/-
# Union8 — THE 8-WAY EXHAUSTIVE PRESERVATION UNION (C1 ∧ … ∧ C7 ∧ C8)

Union7 (C1–C7) conjoined with the NEW host-interop axis C8
(`C8HostInterop.lean`, the design notes C8), at the SHIPPED
minimal-Option-B world (`6e990502`).

## What C8 adds — and why the separation world is special

C1–C7 are all SOURCE-fidelity axes: every face references CPython. C8 is
the first TARGET-HOST axis: the emitted value must be a well-formed citizen
of the JS host (native-sink safety + `.d.ts` honesty). The separation world
`sepW8` is therefore not a synthetic stub — it is **Option A as shipped**
(`c63d0dd7`, int = bigint unconditionally, no boundary authority), the REAL
reverted attempt: a world that satisfies every source-fidelity component
(C1–C7 hold — its base world is the shipped `realWorld`; and its OWN
scalar value model's C2-side fidelity is PROVED and carried as explicit
conjuncts of `sep_C8`) yet violates C8 at an ordinary small int (the
`slice(0n)` class + the `.d.ts` type-lie). **"We shipped a value model that
satisfied every source-fidelity axis and still broke the host" is a
theorem here** (`sep_C8`), and the 8-way union is strictly stronger than
the 7-way sub-conjunction that omits C8
(`eightway_no_C8_not_sufficient`).

## Statement architecture

`World8` = a Union7 `World` (the 30 per-fragment compiled targets) + the
two host-boundary knobs the C8 axis owns: the numeric value model
(`hNum : NumModel`) and the native-boundary crossing (`hCross`). C1–C7 read
ONLY the base world; C8 reads ONLY the two host knobs — so each separation
world perturbs exactly the fields its component owns:
* `sepW8` keeps the base world SHIPPED and swaps the host knobs to
  Option A + identity crossing (the axis-owned pair);
* the seven Union7 separation worlds lift with the SHIPPED host knobs
  (`liftB`), so all EIGHT conjuncts are load-bearing
  (`eightway_no_C1 … no_C8`).

## Honesty ledger (additive to Union7's — read that one too)

* C8's face covers the host-boundary NUMERIC fragment (safe-range ints +
  integer-valued floats crossing to native numeric sinks / the `.d.ts`
  surface) — the same per-axis fragment scoping as C4 (PyResult carriers
  only), C5 (effect fragments only), C7 (alias fragment only). Large ints
  cross as `bigint` under B and are OUTSIDE the modeled sink domain by the
  stated safe-range hypothesis (C8HostInterop header); the WASM lane is
  owned by JsWasmMirror.
* `sep_C8`'s base world is `realWorld` itself: the C1–C7 conjuncts at
  `sepW8` are inherited, NOT re-proved against an A-lowered arith/wave
  model — the A-ness of `sepW8` lives entirely in the host knobs, plus the
  two explicit `preservationC2_optionA*` conjuncts proving A's OWN scalar
  model source-faithful. A full A-instantiated 30-fragment world is not
  claimed (and not needed: the separation only requires SOME world with
  all of C1–C7 and ¬C8).
* A boundary crossing that DEMOTED `bigint → number` would satisfy this C8
  fragment for A on safe ints; none was shipped — the refuted world is
  A-as-shipped (C8HostInterop header states this; no strawman).
* Provenance: minimal B `6e990502` (PyFloat/__pyF/__pyJs, hybrid int,
  dts.rs L26); Option A `c63d0dd7`. Empirical pins: node + CPython runs
  2026-08-21 recorded in C2TypeRepr.lean / C8HostInterop.lean.
-/
import Union7
import C8HostInterop

namespace Union8

open C2TypeRepr C8HostInterop

/-! ## The world: Union7's fragment targets + the C8 host knobs -/

/-- An 8-way world: the 7-way union's per-fragment compiled targets plus
    the two host-boundary knobs C8 owns (value model + crossing). -/
structure World8 where
  base   : Union7.World
  hNum   : NumModel
  hCross : JsVal → JsVal

/-- **The real world: everything SHIPPED** — Union7's shipped targets and
    the minimal-B host knobs (`optionB`, `__pyJs`) @ `6e990502`. -/
def realWorld8 : World8 := ⟨Union7.realWorld, optionB, pyJsB⟩

/-- Lift a Union7 world with the SHIPPED host knobs. -/
def liftB (w : Union7.World) : World8 := ⟨w, optionB, pyJsB⟩

/-! ## The eight components -/

def C1 (w : World8) : Prop := Union7.C1 w.base
def C2 (w : World8) : Prop := Union7.C2 w.base
def C3 (w : World8) : Prop := Union7.C3 w.base
def C4 (w : World8) : Prop := Union7.C4 w.base
def C5 (w : World8) : Prop := Union7.C5 w.base
def C6 (w : World8) : Prop := Union7.C6 w.base
def C7 (w : World8) : Prop := Union7.C7 w.base
/-- **C8 — host-interop preservation** (the new axis): the world's host
    knobs deliver sink-safe, `.d.ts`-honest values at the native boundary
    (`C8HostInterop.C8Preserves`). -/
def C8 (w : World8) : Prop := C8Preserves w.hNum w.hCross

/-! ## THE 8-WAY EXHAUSTIVE UNION -/

/-- **THE UNION: C1 ∧ C2 ∧ C3 ∧ C4 ∧ C5 ∧ C6 ∧ C7 ∧ C8, conjoined
    explicitly, at the shipped world** — Union7's exhaustive
    source-fidelity union PLUS host-interop, i.e. the shipped compiler is
    simultaneously CPython-faithful over every wave/lattice fragment AND a
    well-formed citizen of its JS host at the numeric boundary. Every
    conjunct is load-bearing (`eightway_no_C1 … no_C8` below). Honesty
    ledgers: this header + Union7's + C8HostInterop's. -/
theorem union8 :
    C1 realWorld8 ∧ C2 realWorld8 ∧ C3 realWorld8 ∧ C4 realWorld8 ∧
    C5 realWorld8 ∧ C6 realWorld8 ∧ C7 realWorld8 ∧ C8 realWorld8 :=
  ⟨Union7.C1_real, Union7.C2_real, Union7.C3_real, Union7.C4_real,
   Union7.C5_real, Union7.C6_real, Union7.C7_real, preservationC8⟩

/-! ## THE C8 SEPARATION — the real reverted world, as a theorem

`sepW8` = Option A as shipped, riding on the shipped base world: all seven
source-fidelity components hold, A's own scalar model is source-faithful
(repr AND tag — the two explicit conjuncts), and C8 FAILS at the exact
regression witness. -/

/-- The Option-A world: shipped fragment targets, A host knobs
    (int = bigint, identity crossing — `c63d0dd7`). -/
def sepW8 : World8 := ⟨Union7.realWorld, optionA, crossA⟩

/-- **`sep_C8` — the crown separation of the 8-way union**: a world
    satisfying EVERY source-fidelity component C1–C7, whose scalar value
    model is itself C2-faithful (repr + tag, proved — not just inherited),
    and which VIOLATES host-interop. The machine-checked form of "the
    Option-A regression satisfied every axis our oracles measured and
    still broke the host" — the reason C8 exists. -/
theorem sep_C8 :
    (C1 sepW8 ∧ C2 sepW8 ∧ C3 sepW8 ∧ C4 sepW8 ∧ C5 sepW8 ∧ C6 sepW8 ∧ C7 sepW8)
    ∧ C2TypeRepr.C2Preserves (lowerM optionA) (jsReprM floatDisplay)
    ∧ C2TypeRepr.C2PreservesTag (lowerM optionA) jsTagOptionA
    ∧ ¬ C8 sepW8 :=
  ⟨⟨Union7.C1_real, Union7.C2_real, Union7.C3_real, Union7.C4_real,
    Union7.C5_real, Union7.C6_real, Union7.C7_real⟩,
   preservationC2_optionA, preservationC2_tag_optionA, C8_fails_optionA⟩

/-- **The 8-way union is strictly stronger than the 7-way sub-conjunction
    omitting C8**: source fidelity (all of C1–C7) does NOT imply
    host-interop. THE theorem this file exists for. -/
theorem eightway_no_C8_not_sufficient :
    ¬ (∀ w : World8, C1 w ∧ C2 w ∧ C3 w ∧ C4 w ∧ C5 w ∧ C6 w ∧ C7 w → C8 w) :=
  fun h => sep_C8.2.2.2 (h sepW8 sep_C8.1)

/-! ## The seven lifted separations — every OTHER conjunct stays
    load-bearing in the 8-way union (Union7's worlds + shipped host knobs,
    which satisfy C8 — so each `Ck` is still the only component that sees
    its perturbation) -/

theorem sep_C1 :
    ¬ C1 (liftB Union7.sepW1) ∧ C2 (liftB Union7.sepW1) ∧ C3 (liftB Union7.sepW1) ∧
    C4 (liftB Union7.sepW1) ∧ C5 (liftB Union7.sepW1) ∧ C6 (liftB Union7.sepW1) ∧
    C7 (liftB Union7.sepW1) ∧ C8 (liftB Union7.sepW1) :=
  ⟨Union7.sep_C1.1, Union7.sep_C1.2.1, Union7.sep_C1.2.2.1, Union7.sep_C1.2.2.2.1,
   Union7.sep_C1.2.2.2.2.1, Union7.sep_C1.2.2.2.2.2.1, Union7.sep_C1.2.2.2.2.2.2,
   preservationC8⟩

theorem sep_C2 :
    C1 (liftB Union7.sepW2) ∧ ¬ C2 (liftB Union7.sepW2) ∧ C3 (liftB Union7.sepW2) ∧
    C4 (liftB Union7.sepW2) ∧ C5 (liftB Union7.sepW2) ∧ C6 (liftB Union7.sepW2) ∧
    C7 (liftB Union7.sepW2) ∧ C8 (liftB Union7.sepW2) :=
  ⟨Union7.sep_C2.1, Union7.sep_C2.2.1, Union7.sep_C2.2.2.1, Union7.sep_C2.2.2.2.1,
   Union7.sep_C2.2.2.2.2.1, Union7.sep_C2.2.2.2.2.2.1, Union7.sep_C2.2.2.2.2.2.2,
   preservationC8⟩

theorem sep_C3 :
    C1 (liftB Union7.sepW3) ∧ C2 (liftB Union7.sepW3) ∧ ¬ C3 (liftB Union7.sepW3) ∧
    C4 (liftB Union7.sepW3) ∧ C5 (liftB Union7.sepW3) ∧ C6 (liftB Union7.sepW3) ∧
    C7 (liftB Union7.sepW3) ∧ C8 (liftB Union7.sepW3) :=
  ⟨Union7.sep_C3.1, Union7.sep_C3.2.1, Union7.sep_C3.2.2.1, Union7.sep_C3.2.2.2.1,
   Union7.sep_C3.2.2.2.2.1, Union7.sep_C3.2.2.2.2.2.1, Union7.sep_C3.2.2.2.2.2.2,
   preservationC8⟩

theorem sep_C4 :
    C1 (liftB Union7.sepW4) ∧ C2 (liftB Union7.sepW4) ∧ C3 (liftB Union7.sepW4) ∧
    ¬ C4 (liftB Union7.sepW4) ∧ C5 (liftB Union7.sepW4) ∧ C6 (liftB Union7.sepW4) ∧
    C7 (liftB Union7.sepW4) ∧ C8 (liftB Union7.sepW4) :=
  ⟨Union7.sep_C4.1, Union7.sep_C4.2.1, Union7.sep_C4.2.2.1, Union7.sep_C4.2.2.2.1,
   Union7.sep_C4.2.2.2.2.1, Union7.sep_C4.2.2.2.2.2.1, Union7.sep_C4.2.2.2.2.2.2,
   preservationC8⟩

theorem sep_C5 :
    C1 (liftB Union7.sepW5) ∧ C2 (liftB Union7.sepW5) ∧ C3 (liftB Union7.sepW5) ∧
    C4 (liftB Union7.sepW5) ∧ ¬ C5 (liftB Union7.sepW5) ∧ C6 (liftB Union7.sepW5) ∧
    C7 (liftB Union7.sepW5) ∧ C8 (liftB Union7.sepW5) :=
  ⟨Union7.sep_C5.1, Union7.sep_C5.2.1, Union7.sep_C5.2.2.1, Union7.sep_C5.2.2.2.1,
   Union7.sep_C5.2.2.2.2.1, Union7.sep_C5.2.2.2.2.2.1, Union7.sep_C5.2.2.2.2.2.2,
   preservationC8⟩

theorem sep_C6 :
    C1 (liftB Union7.sepW6) ∧ C2 (liftB Union7.sepW6) ∧ C3 (liftB Union7.sepW6) ∧
    C4 (liftB Union7.sepW6) ∧ C5 (liftB Union7.sepW6) ∧ ¬ C6 (liftB Union7.sepW6) ∧
    C7 (liftB Union7.sepW6) ∧ C8 (liftB Union7.sepW6) :=
  ⟨Union7.sep_C6.1, Union7.sep_C6.2.1, Union7.sep_C6.2.2.1, Union7.sep_C6.2.2.2.1,
   Union7.sep_C6.2.2.2.2.1, Union7.sep_C6.2.2.2.2.2.1, Union7.sep_C6.2.2.2.2.2.2,
   preservationC8⟩

theorem sep_C7 :
    C1 (liftB Union7.sepW7) ∧ C2 (liftB Union7.sepW7) ∧ C3 (liftB Union7.sepW7) ∧
    C4 (liftB Union7.sepW7) ∧ C5 (liftB Union7.sepW7) ∧ C6 (liftB Union7.sepW7) ∧
    ¬ C7 (liftB Union7.sepW7) ∧ C8 (liftB Union7.sepW7) :=
  ⟨Union7.sep_C7.1, Union7.sep_C7.2.1, Union7.sep_C7.2.2.1, Union7.sep_C7.2.2.2.1,
   Union7.sep_C7.2.2.2.2.1, Union7.sep_C7.2.2.2.2.2.1, Union7.sep_C7.2.2.2.2.2.2,
   preservationC8⟩

/-! ## Strictly stronger than every 7-way sub-conjunction (all eight) -/

theorem eightway_no_C1_not_sufficient :
    ¬ (∀ w : World8, C2 w ∧ C3 w ∧ C4 w ∧ C5 w ∧ C6 w ∧ C7 w ∧ C8 w → C1 w) :=
  fun h => sep_C1.1 (h (liftB Union7.sepW1)
    ⟨sep_C1.2.1, sep_C1.2.2.1, sep_C1.2.2.2.1, sep_C1.2.2.2.2.1,
     sep_C1.2.2.2.2.2.1, sep_C1.2.2.2.2.2.2.1, sep_C1.2.2.2.2.2.2.2⟩)

theorem eightway_no_C2_not_sufficient :
    ¬ (∀ w : World8, C1 w ∧ C3 w ∧ C4 w ∧ C5 w ∧ C6 w ∧ C7 w ∧ C8 w → C2 w) :=
  fun h => sep_C2.2.1 (h (liftB Union7.sepW2)
    ⟨sep_C2.1, sep_C2.2.2.1, sep_C2.2.2.2.1, sep_C2.2.2.2.2.1,
     sep_C2.2.2.2.2.2.1, sep_C2.2.2.2.2.2.2.1, sep_C2.2.2.2.2.2.2.2⟩)

theorem eightway_no_C3_not_sufficient :
    ¬ (∀ w : World8, C1 w ∧ C2 w ∧ C4 w ∧ C5 w ∧ C6 w ∧ C7 w ∧ C8 w → C3 w) :=
  fun h => sep_C3.2.2.1 (h (liftB Union7.sepW3)
    ⟨sep_C3.1, sep_C3.2.1, sep_C3.2.2.2.1, sep_C3.2.2.2.2.1,
     sep_C3.2.2.2.2.2.1, sep_C3.2.2.2.2.2.2.1, sep_C3.2.2.2.2.2.2.2⟩)

theorem eightway_no_C4_not_sufficient :
    ¬ (∀ w : World8, C1 w ∧ C2 w ∧ C3 w ∧ C5 w ∧ C6 w ∧ C7 w ∧ C8 w → C4 w) :=
  fun h => sep_C4.2.2.2.1 (h (liftB Union7.sepW4)
    ⟨sep_C4.1, sep_C4.2.1, sep_C4.2.2.1, sep_C4.2.2.2.2.1,
     sep_C4.2.2.2.2.2.1, sep_C4.2.2.2.2.2.2.1, sep_C4.2.2.2.2.2.2.2⟩)

theorem eightway_no_C5_not_sufficient :
    ¬ (∀ w : World8, C1 w ∧ C2 w ∧ C3 w ∧ C4 w ∧ C6 w ∧ C7 w ∧ C8 w → C5 w) :=
  fun h => sep_C5.2.2.2.2.1 (h (liftB Union7.sepW5)
    ⟨sep_C5.1, sep_C5.2.1, sep_C5.2.2.1, sep_C5.2.2.2.1,
     sep_C5.2.2.2.2.2.1, sep_C5.2.2.2.2.2.2.1, sep_C5.2.2.2.2.2.2.2⟩)

theorem eightway_no_C6_not_sufficient :
    ¬ (∀ w : World8, C1 w ∧ C2 w ∧ C3 w ∧ C4 w ∧ C5 w ∧ C7 w ∧ C8 w → C6 w) :=
  fun h => sep_C6.2.2.2.2.2.1 (h (liftB Union7.sepW6)
    ⟨sep_C6.1, sep_C6.2.1, sep_C6.2.2.1, sep_C6.2.2.2.1,
     sep_C6.2.2.2.2.1, sep_C6.2.2.2.2.2.2.1, sep_C6.2.2.2.2.2.2.2⟩)

theorem eightway_no_C7_not_sufficient :
    ¬ (∀ w : World8, C1 w ∧ C2 w ∧ C3 w ∧ C4 w ∧ C5 w ∧ C6 w ∧ C8 w → C7 w) :=
  fun h => sep_C7.2.2.2.2.2.2.1 (h (liftB Union7.sepW7)
    ⟨sep_C7.1, sep_C7.2.1, sep_C7.2.2.1, sep_C7.2.2.2.1,
     sep_C7.2.2.2.2.1, sep_C7.2.2.2.2.2.1, sep_C7.2.2.2.2.2.2.2⟩)

/-! ## Non-vacuity — the C8 face fires at the shipped world
    (C1–C7 non-vacuity is Union7's; inherited unchanged) -/

#guard toNumberHost (realWorld8.hCross (lowerM realWorld8.hNum (.pint 1))) = some 1
#guard toNumberHost (realWorld8.hCross (lowerM realWorld8.hNum (.pfloat 8))) = some 8
#guard typeofOf (realWorld8.hCross (lowerM realWorld8.hNum (.pfloat 8))) = .tnumber
-- the A-world's C8 failure fires at the same witnesses:
#guard toNumberHost (sepW8.hCross (lowerM sepW8.hNum (.pint 1))) = none

/-! ## SPOTs — client facts THROUGH the union -/

/-- SPOT (C8 through the union): the compiled index `1` survives the
    native `slice` sink — derived from union8's C8 face, not by evaluating
    the target side. -/
example :
    hostSlice [10, 20, 30] (realWorld8.hCross (lowerM realWorld8.hNum (.pint 1)))
      = some [20, 30] := by
  unfold hostSlice
  rw [union8.2.2.2.2.2.2.2.1 1 (by decide)]
  rfl

/-- SPOT (C3 through the union, lifted): the compiled `"a" // 1` errs
    exactly because Python errs — Union7's face, reached through the 8-way
    statement. -/
example :
    (realWorld8.base.tA (.fdiv (.lit (.pstr "a")) (.lit (.pint 1))) []).isErr = true := by
  have h := union8.2.2.1.1 (.fdiv (.lit (.pstr "a")) (.lit (.pint 1))) []
  rw [Union7.errAgree] at h
  rw [h]; rfl

/-! ## Axiom pins (per-declaration; ⊆ the three standard, no native-decide escape) -/

/-- info: 'Union8.union8' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms union8

/-- info: 'Union8.sep_C8' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms sep_C8

/-- info: 'Union8.eightway_no_C8_not_sufficient' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms eightway_no_C8_not_sufficient

/-- info: 'Union8.eightway_no_C1_not_sufficient' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms eightway_no_C1_not_sufficient

/-- info: 'Union8.eightway_no_C7_not_sufficient' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms eightway_no_C7_not_sufficient

end Union8
