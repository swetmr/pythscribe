/-!
# EffectOrder — C5 slice 6c: SYNCHRONOUS left-to-right observable-effect order

The **effect / observation-order** axis of the 7-component semantic-preservation
lattice (the design notes §2, component **C5**). C5 is the property that
the compiler preserves the ORDER in which observable side-effects (e.g. the
sequence of `print` / output events) happen, between the source (CPython)
semantics and the emitted lowering.

## What C5 already had, and the gap this file fills

`PythExpandVerify.lean` proves two C5 slices:

* **6a** (`preservationSc`) — synchronous SHORT-CIRCUIT: whether the RHS of
  `and`/`or` runs at all. Its own header states the operand order *a-before-b*
  is "common to both 6a strategies" and is **NOT** a falsifiable axis there —
  "an order SWAP is falsified in 6b".
* **6b** (`preservationAy`) — the ASYNC `await` → state-machine desugaring:
  effect order across a *suspension point*.

Neither refutes the PLAIN, SYNCHRONOUS operand/statement evaluation-order swap:
a compiler that (still no `await`, no short-circuit) emits the operands of
`f() + g()` right-to-left, or emits two top-level statements in the wrong order.
That is a real codegen bug class (stack-based right-to-left argument emission;
a reversed statement schedule) and, until this file, **no C5 theorem could see
it**. This is **slice 6c** — the synchronous left-to-right effect-order
obligation — covering the binary-operand / statement-order core of C5.
(The n-ary call-argument evaluation-order clause is NOT modeled by this
fragment — there is no call node — so it is out of scope here, not "closed".)

## The statement (one line)

`preservationEO : ∀ e, evalEOtgt EvalOrder.lr e = evalEO e` — the left-to-right
lowering agrees with the CPython reference on the PAIR `(result, effect trace)`:
every observable effect runs, in the same program order, and the value/error
match too. `evalEOtgt` is an INDEPENDENT recursion parameterised by an
evaluation-order strategy `o : EvalOrder` (F10: the ordering is not hardcoded —
`o = rl` reaches the swap of every binary node). The discriminating witness is
`o = rl` (right-to-left), refuted by `preservationEO_rlStub_fails`.

## Honesty tier — MODEL-tier (stated plainly)

This is a **model-level** slice, exactly like the rest of the preservation
lattice v2 (the design notes §5 "Explicitly deferred: Shipping-binding
the lattice carriers" — the lattice PoC fragments are proved at the MODEL level;
binding the new carriers to the emitted runtime is future work). The reference
`evalEO` is a CPython-FAITHFUL model of Python's guaranteed left-to-right
evaluation order (Python Language Reference §6.16 "Evaluation order": the
left operand of a binary operator is evaluated before the right, in source
order); each MODELED faithfulness fact is pinned by a `#guard` below. (The
n-ary call-argument clause of §6.16 is not modeled by this fragment and so is
not pinned here.) The target `evalEOtgt` is an INDEPENDENT evaluator (a separate
recursion, not a flag on the reference), so `o = rl` is a genuine wrong compiler,
not the reference in disguise. Shipping relevance (NOT a binding claim): the
shipped path is `pyths → JS`, and JS also evaluates left-to-right, so the
shipped compiler naturally preserves this; the refuted `rl` world models the
concrete mis-lowering. A `pyths`-vs-CPython print-order differential is the
future shipping-binding, deferred with the rest of the lattice carriers.

Self-contained: Lean core only, own minimal `EOExc`/`EOResult` carrier
(no import of the 17k-line `PythExpandVerify`), so the build is tiny and the
its axiom footprint is isolated. Purely additive; no existing file is touched.
-/

namespace EffectOrder

/-- Two exception kinds — enough for a raising witness (the order axis is
    visible even without errors, but a raise makes "which effects ran" sharper).
    `DecidableEq` drives the `decide`-based stub refutations and `#guard`s. -/
inductive EOExc where
  | zeroDiv
  | valueError
  deriving DecidableEq, Repr

/-- Result of an evaluation: an int value or a raise. -/
inductive EOResult where
  | ok (v : Int)
  | err (e : EOExc)
  deriving DecidableEq, Repr

/-- The synchronous, effectful expression fragment. Evaluating a leaf EMITS one
    observable effect (a `String` label — one label per evaluated leaf; no DOM
    payload is modelled, deliberately). No `await`, no `and`/`or`
    short-circuit — this is the plain left-to-right core that 6a/6b leave open.

    * `eff l n` — emits `l`, yields int `n`.
    * `raise l e` — emits `l`, then raises `e` (an evaluated operand whose
      effect runs before its raise).
    * `add a b` — `a + b`: evaluate `a`, then `b`, value `va + vb`.
    * `seqS a b` — `a ; b` (two statements): run `a` for its effect, then `b`;
      the statement's value is `b`'s. Models `print(...); print(...)`. -/
inductive Expr where
  | eff (l : String) (n : Int)
  | raise (l : String) (e : EOExc)
  | add (a b : Expr)
  | seqS (a b : Expr)
  deriving Repr

/-- **Reference (CPython) semantics**: `(result, trace of effects that actually
    ran, in order)`. Left-to-right, source order; the leftmost raise wins and
    stops evaluation of everything to its right (so that operand's effect never
    runs). Every clause is pinned against documented CPython behaviour by a
    `#guard` below. -/
def evalEO : Expr → EOResult × List String
  | .eff l n => (.ok n, [l])
  | .raise l e => (.err e, [l])
  | .add a b =>
      match evalEO a with
      | (.err e, ta) => (.err e, ta)                 -- a raised ⇒ b NOT evaluated
      | (.ok va, ta) =>
          match evalEO b with
          | (.err e, tb) => (.err e, ta ++ tb)
          | (.ok vb, tb) => (.ok (va + vb), ta ++ tb)
  | .seqS a b =>
      match evalEO a with
      | (.err e, ta) => (.err e, ta)                 -- 1st statement raised ⇒ 2nd NOT run
      | (.ok _, ta) =>
          match evalEO b with
          | (r, tb) => (r, ta ++ tb)                 -- value = 2nd statement's result

-- F9 faithfulness pins — the REFERENCE matches CPython left-to-right evaluation
-- order (one effect label per evaluated leaf, in source order):
#guard evalEO (.add (.eff "a" 1) (.eff "b" 2)) = (.ok 3, ["a", "b"])            -- a before b
#guard evalEO (.seqS (.eff "a" 1) (.eff "b" 2)) = (.ok 2, ["a", "b"])           -- stmt order; value = 2nd
#guard evalEO (.add (.raise "a" .zeroDiv) (.eff "b" 2)) = (.err .zeroDiv, ["a"]) -- a raises ⇒ b skipped
#guard evalEO (.add (.eff "a" 1) (.raise "b" .zeroDiv)) = (.err .zeroDiv, ["a", "b"]) -- a runs, then b raises
#guard evalEO (.seqS (.raise "a" .valueError) (.eff "b" 2)) = (.err .valueError, ["a"]) -- 1st raises ⇒ 2nd skipped
-- nested, program order a,b,c preserved:
#guard evalEO (.seqS (.add (.eff "a" 1) (.eff "b" 2)) (.eff "c" 9)) = (.ok 9, ["a", "b", "c"])

/-- The evaluation-ORDER strategy — the falsifiable parameter (F10). `lr`
    evaluates children left-to-right (what the compiler emits); `rl` evaluates
    the RIGHT child first (a right-to-left / stack-reversed emission). Two-point,
    exactly like slice 6a's `ScStrategy` — `rl` reaches the child-order swap of
    EVERY binary node, so every left-to-right fact `preservationEO` asserts is
    reachable by the parameter; nothing is hardcoded. -/
inductive EvalOrder where
  | lr
  | rl
  deriving DecidableEq, Repr

/-- **Independent target evaluator**: the compiled semantics under strategy `o`.
    A SEPARATE recursion (not a flag threaded through `evalEO`). With `o = lr` it
    matches the reference; with `o = rl` it evaluates the RIGHT child FIRST —
    running that effect first AND short-circuiting on ITS raise (the reversed
    operand really executed first, so its raise wins and the left child is never
    reached) — while forming the value by the SAME position rule (`va + vb`;
    `seqS` value = 2nd statement), so `rl` diverges purely in effect ORDER (and,
    when a raise is present, in WHICH effects run / which raise wins), never in
    the success value. -/
def evalEOtgt (o : EvalOrder) : Expr → EOResult × List String
  | .eff l n => (.ok n, [l])
  | .raise l e => (.err e, [l])
  | .add a b =>
      match o with
      | .lr =>
          match evalEOtgt o a with
          | (.err e, ta) => (.err e, ta)
          | (.ok va, ta) =>
              match evalEOtgt o b with
              | (.err e, tb) => (.err e, ta ++ tb)
              | (.ok vb, tb) => (.ok (va + vb), ta ++ tb)
      | .rl =>
          match evalEOtgt o b with                   -- RIGHT child first
          | (.err e, tb) => (.err e, tb)             -- b raised first ⇒ a NOT evaluated
          | (.ok vb, tb) =>
              match evalEOtgt o a with
              | (.err e, ta) => (.err e, tb ++ ta)
              | (.ok va, ta) => (.ok (va + vb), tb ++ ta)   -- same value formula, trace tb++ta
  | .seqS a b =>
      match o with
      | .lr =>
          match evalEOtgt o a with
          | (.err e, ta) => (.err e, ta)
          | (.ok _, ta) =>
              match evalEOtgt o b with
              | (r, tb) => (r, ta ++ tb)
      | .rl =>
          match evalEOtgt o b with                   -- 2nd statement first
          | (.err e, tb) => (.err e, tb)             -- b raised ⇒ a NOT run (value=b's err)
          | (.ok vb, tb) =>
              match evalEOtgt o a with
              | (.err e, ta) => (.err e, tb ++ ta)
              | (.ok _, ta) => (.ok vb, tb ++ ta)    -- value = 2nd statement's, trace tb++ta

/-- The compiled fragment semantics: the independent target under the shipped
    (left-to-right) strategy. -/
abbrev evalEOjs : Expr → EOResult × List String := evalEOtgt EvalOrder.lr

/-- Preservation as a predicate OVER the strategy — the SAME predicate is proved
    for the shipped strategy and refuted for the `rl` stub. -/
def EOPreserves (o : EvalOrder) : Prop := ∀ e, evalEOtgt o e = evalEO e

/-- **C5 (synchronous effect order) over (result × trace), in one statement.**
    The compiled left-to-right evaluation agrees with the reference on the PAIR
    `(result, effect trace)`: every effect that runs, runs in the same program
    order; none the reference skips is run; and the value / error-occurrence /
    error-kind match. Real structural induction over the fragment (the IHs
    rewrite the compiled sub-results before the arm-by-arm comparison). -/
theorem preservationEO (e : Expr) : evalEOjs e = evalEO e := by
  induction e with
  | eff l n => rfl
  | raise l ex => rfl
  | add a b iha ihb =>
    show evalEOtgt EvalOrder.lr (.add a b) = evalEO (.add a b)
    -- rewrite the compiled sub-results (IHs), then both sides are the SAME
    -- arm-by-arm dispatch on `evalEO a` / `evalEO b`.
    simp only [evalEOtgt, evalEO, iha, ihb]
  | seqS a b iha ihb =>
    show evalEOtgt EvalOrder.lr (.seqS a b) = evalEO (.seqS a b)
    simp only [evalEOtgt, evalEO, iha, ihb]

/-- The predicate-form instantiation the stub litmus contrasts against. -/
theorem preservationEO_real : EOPreserves EvalOrder.lr := preservationEO

/-- **Stub litmus — the discriminating witness (pure order divergence).**
    Witness `a + b` with effects: the reference runs `["a", "b"]`, the `rl` stub
    runs `["b", "a"]`. Same value (`3`), same (no) error — the ONLY difference is
    the effect ORDER. Exactly the defect a value-only or occurrence-only theorem
    can never see. This is the false world C5 slice 6c REFUTES: a synchronous
    right-to-left operand emission. -/
theorem preservationEO_rlStub_fails : ¬ EOPreserves EvalOrder.rl := by
  intro h
  have hc := h (.add (.eff "a" 1) (.eff "b" 2))
  -- hc reduces to `(.ok 3, ["b", "a"]) = (.ok 3, ["a", "b"])`.
  exact absurd hc (by decide)

-- The 6c witness, concretely — same value, REVERSED trace (the pure C5 axis):
#guard evalEOjs (.add (.eff "a" 1) (.eff "b" 2)) = (.ok 3, ["a", "b"])            -- LR: program order
#guard evalEOtgt EvalOrder.rl (.add (.eff "a" 1) (.eff "b" 2)) = (.ok 3, ["b", "a"]) -- RL: reversed ✗
-- and the value AGREES while the trace differs (order is invisible to value):
#guard (evalEOtgt EvalOrder.rl (.add (.eff "a" 1) (.eff "b" 2))).1
     = (evalEO (.add (.eff "a" 1) (.eff "b" 2))).1                                -- value: agree
#guard (evalEOtgt EvalOrder.rl (.add (.eff "a" 1) (.eff "b" 2))).2
     ≠ (evalEO (.add (.eff "a" 1) (.eff "b" 2))).2                                -- trace: differ

/-! **Sharper witness — order changes WHICH effects run and WHICH raise wins.**
    `raise("a") + eff("b")` (a raising left operand and an effectful right one):
    the reference raises at `a` and NEVER runs `b` (`(.err zeroDiv, ["a"])`); the
    `rl` stub runs `b` first (a SPURIOUS effect), then raises at `a`
    (`(.err zeroDiv, ["b", "a"])`). So the synchronous order bug is not merely
    cosmetic: it makes an effect happen that the reference skips. And when both
    operands raise DIFFERENT kinds, the order bug even flips the observed
    exception CLASS (a C4 divergence downstream of the C5 defect). -/
#guard evalEOjs (.add (.raise "a" .zeroDiv) (.eff "b" 2)) = (.err .zeroDiv, ["a"])
#guard evalEOtgt EvalOrder.rl (.add (.raise "a" .zeroDiv) (.eff "b" 2)) = (.err .zeroDiv, ["b", "a"])
#guard evalEOjs (.add (.raise "a" .valueError) (.raise "b" .zeroDiv)) = (.err .valueError, ["a"])
#guard evalEOtgt EvalOrder.rl (.add (.raise "a" .valueError) (.raise "b" .zeroDiv)) = (.err .zeroDiv, ["b"])

/-! ### Added-strength separation — C5(order) is strictly stronger than value

The mirror of 6a's `preservationSc_value_blind_on_success` / 6b's reorder
separation, one lattice axis up: on the failure-free subclass the `rl` strategy
— refuted above on the full `(result, trace)` predicate — provably PRESERVES the
value projection, so a value-only theorem is structurally BLIND to the order
defect. Together with `preservationEO_trace_fails_on_success` (the trace-carrying
predicate fails on the SAME subclass), this is the theorem pair proving order is
independently falsifiable from value. -/

/-- Value projection: keep the result, FORGET the trace — what a pre-C5
    (value/error-only) statement observes. -/
def eoValue : EOResult × List String → EOResult := Prod.fst

/-- Failure-free subclass: no `raise` leaf anywhere. -/
def eoNoRaise : Expr → Bool
  | .eff _ _ => true
  | .raise _ _ => false
  | .add a b => eoNoRaise a && eoNoRaise b
  | .seqS a b => eoNoRaise a && eoNoRaise b

/-- On a failure-free program the REFERENCE always succeeds. Load-bearing for the
    blindness lemma (it lets the arm-by-arm comparison take the `ok` branch). -/
theorem evalEO_noRaise_isOk (e : Expr) :
    eoNoRaise e = true → ∃ v, (evalEO e).1 = EOResult.ok v := by
  induction e with
  | eff l n => intro _; exact ⟨n, rfl⟩
  | raise l ex => intro h; simp [eoNoRaise] at h
  | add a b iha ihb =>
    intro h
    simp only [eoNoRaise, Bool.and_eq_true] at h
    obtain ⟨va, hva⟩ := iha h.1
    obtain ⟨vb, hvb⟩ := ihb h.2
    refine ⟨va + vb, ?_⟩
    simp only [evalEO]
    cases hA : evalEO a with
    | mk ra ta =>
      cases hB : evalEO b with
      | mk rb tb =>
        rw [hA] at hva; rw [hB] at hvb
        simp only at hva hvb
        subst hva; subst hvb; rfl
  | seqS a b iha ihb =>
    intro h
    simp only [eoNoRaise, Bool.and_eq_true] at h
    obtain ⟨va, hva⟩ := iha h.1
    obtain ⟨vb, hvb⟩ := ihb h.2
    refine ⟨vb, ?_⟩
    simp only [evalEO]
    cases hA : evalEO a with
    | mk ra ta =>
      cases hB : evalEO b with
      | mk rb tb =>
        rw [hA] at hva; rw [hB] at hvb
        simp only at hva hvb
        subst hva; subst hvb; rfl

/-- On a failure-free program the `rl` target always succeeds too. -/
theorem evalEOtgt_rl_noRaise_isOk (e : Expr) :
    eoNoRaise e = true → ∃ v, (evalEOtgt EvalOrder.rl e).1 = EOResult.ok v := by
  induction e with
  | eff l n => intro _; exact ⟨n, rfl⟩
  | raise l ex => intro h; simp [eoNoRaise] at h
  | add a b iha ihb =>
    intro h
    simp only [eoNoRaise, Bool.and_eq_true] at h
    obtain ⟨va, hva⟩ := iha h.1
    obtain ⟨vb, hvb⟩ := ihb h.2
    refine ⟨va + vb, ?_⟩
    simp only [evalEOtgt]
    cases hB : evalEOtgt EvalOrder.rl b with
    | mk rb tb =>
      cases hA : evalEOtgt EvalOrder.rl a with
      | mk ra ta =>
        rw [hA] at hva; rw [hB] at hvb
        simp only at hva hvb
        subst hva; subst hvb; rfl
  | seqS a b iha ihb =>
    intro h
    simp only [eoNoRaise, Bool.and_eq_true] at h
    obtain ⟨va, hva⟩ := iha h.1
    obtain ⟨vb, hvb⟩ := ihb h.2
    refine ⟨vb, ?_⟩
    simp only [evalEOtgt]
    cases hB : evalEOtgt EvalOrder.rl b with
    | mk rb tb =>
      cases hA : evalEOtgt EvalOrder.rl a with
      | mk ra ta =>
        rw [hA] at hva; rw [hB] at hvb
        simp only at hva hvb
        subst hva; subst hvb; rfl

/-- **The C5 added-strength SEPARATION (value-correct, ORDER-wrong).** On the
    failure-free subclass, the `rl` strategy PRESERVES the value projection: the
    result is identical (the value formula is position-based; only the effect
    order changes). So a value-only preservation statement cannot see the order
    defect — the mirror of `eraseKind_blind_to_zeroKind` (C4 over C3) and
    `preservationSc_value_blind_on_success` (6a), one axis up. -/
theorem preservationEO_value_blind_on_success (e : Expr) :
    eoNoRaise e = true → eoValue (evalEOtgt EvalOrder.rl e) = eoValue (evalEO e) := by
  induction e with
  | eff l n => intro _; rfl
  | raise l ex => intro h; simp [eoNoRaise] at h
  | add a b iha ihb =>
    intro h
    simp only [eoNoRaise, Bool.and_eq_true] at h
    obtain ⟨va, hva⟩ := evalEO_noRaise_isOk a h.1
    obtain ⟨vb, hvb⟩ := evalEO_noRaise_isOk b h.2
    obtain ⟨va', hva'⟩ := evalEOtgt_rl_noRaise_isOk a h.1
    obtain ⟨vb', hvb'⟩ := evalEOtgt_rl_noRaise_isOk b h.2
    have hEqA := iha h.1
    have hEqB := ihb h.2
    -- collapse the value equalities to va' = va, vb' = vb
    simp only [eoValue, hva, hva'] at hEqA
    simp only [eoValue, hvb, hvb'] at hEqB
    injection hEqA with hEqA
    injection hEqB with hEqB
    subst hEqA; subst hEqB
    -- destruct all four sub-results, kill the error branches via hva/…
    simp only [eoValue, evalEO, evalEOtgt]
    cases hA : evalEO a with
    | mk ra ta =>
      cases hB : evalEO b with
      | mk rb tb =>
        cases hA' : evalEOtgt EvalOrder.rl a with
        | mk ra' ta' =>
          cases hB' : evalEOtgt EvalOrder.rl b with
          | mk rb' tb' =>
            rw [hA] at hva; rw [hB] at hvb; rw [hA'] at hva'; rw [hB'] at hvb'
            simp only at hva hvb hva' hvb'
            subst hva; subst hvb; subst hva'; subst hvb'
            rfl
  | seqS a b iha ihb =>
    intro h
    simp only [eoNoRaise, Bool.and_eq_true] at h
    obtain ⟨va, hva⟩ := evalEO_noRaise_isOk a h.1
    obtain ⟨vb, hvb⟩ := evalEO_noRaise_isOk b h.2
    obtain ⟨va', hva'⟩ := evalEOtgt_rl_noRaise_isOk a h.1
    obtain ⟨vb', hvb'⟩ := evalEOtgt_rl_noRaise_isOk b h.2
    have hEqB := ihb h.2
    simp only [eoValue, hvb, hvb'] at hEqB
    injection hEqB with hEqB
    subst hEqB
    simp only [eoValue, evalEO, evalEOtgt]
    cases hA : evalEO a with
    | mk ra ta =>
      cases hB : evalEO b with
      | mk rb tb =>
        cases hA' : evalEOtgt EvalOrder.rl a with
        | mk ra' ta' =>
          cases hB' : evalEOtgt EvalOrder.rl b with
          | mk rb' tb' =>
            rw [hA] at hva; rw [hB] at hvb; rw [hA'] at hva'; rw [hB'] at hvb'
            simp only at hva hvb hva' hvb'
            subst hva; subst hvb; subst hva'; subst hvb'
            rfl

/-- The trace-carrying predicate is refuted ON THE SAME failure-free subclass
    where the value projection provably holds: the both-succeed witness `a + b`
    gives the SAME value but a REVERSED trace. So the 6c order defect is INVISIBLE
    to any value-only statement and VISIBLE to the trace-carrying one — the C5
    separation as a theorem pair. -/
theorem preservationEO_trace_fails_on_success :
    ¬ (∀ e, eoNoRaise e = true → evalEOtgt EvalOrder.rl e = evalEO e) := by
  intro h
  have hc := h (.add (.eff "a" 1) (.eff "b" 2)) (by decide)
  -- hc reduces to `(.ok 3, ["b", "a"]) = (.ok 3, ["a", "b"])`.
  exact absurd hc (by decide)

/-- SPOT (through the theorem, not by evaluation): the compiled `a + b` yields
    its effects in program order `["a","b"]`, derived from `preservationEO`;
    fails if the statement is weakened. -/
example : evalEOjs (.add (.eff "a" 1) (.eff "b" 2)) = (.ok 3, ["a", "b"]) := by
  rw [preservationEO]; rfl

/-- SPOT (through the separation lemma): the `rl` strategy's VALUE on the
    both-succeed witness agrees with the reference — derived from
    `preservationEO_value_blind_on_success`, so it fails if that lemma is
    weakened. -/
example : eoValue (evalEOtgt EvalOrder.rl (.add (.eff "a" 1) (.eff "b" 2)))
        = eoValue (evalEO (.add (.eff "a" 1) (.eff "b" 2))) :=
  preservationEO_value_blind_on_success (.add (.eff "a" 1) (.eff "b" 2)) (by decide)

-- Per-declaration axiom pins (Stage-5 gate; captured from a real build). The
-- footprint MUST stay ⊆ {propext, Classical.choice, Quot.sound}.

/-- info: 'EffectOrder.preservationEO' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms preservationEO

/-- info: 'EffectOrder.preservationEO_real' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms preservationEO_real

/-- info: 'EffectOrder.preservationEO_rlStub_fails' does not depend on any axioms -/
#guard_msgs in
#print axioms preservationEO_rlStub_fails

/-- info: 'EffectOrder.evalEO_noRaise_isOk' depends on axioms: [propext] -/
#guard_msgs in
#print axioms evalEO_noRaise_isOk

/-- info: 'EffectOrder.evalEOtgt_rl_noRaise_isOk' depends on axioms: [propext] -/
#guard_msgs in
#print axioms evalEOtgt_rl_noRaise_isOk

/-- info: 'EffectOrder.preservationEO_value_blind_on_success' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationEO_value_blind_on_success

/-- info: 'EffectOrder.preservationEO_trace_fails_on_success' does not depend on any axioms -/
#guard_msgs in
#print axioms preservationEO_trace_fails_on_success

end EffectOrder
