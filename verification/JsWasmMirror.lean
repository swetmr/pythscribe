import PythExpandVerify

/-! # JsWasmMirror — JS↔WASM mirror equivalence at the marshalling boundary

The MIRROR property: a routed numeric kernel has TWO shipped lowerings — the pure-JS
(arbitrary-precision) path and the WASM i64 fast path behind the value-marshalling
boundary (`crates/pyths_codegen_wasm/src/bridge.rs`: `convert_js_to_wasm` /
`__list_to_wasm` i64 element guard / `__i64Oob` / the sticky `__ovf` flag /
`__i64ToJs`). Mirror equivalence = every value the WASM side SURFACES to JS equals
the value the JS lowering computes; everything else diverts loudly (twin re-run /
RangeError / OverflowError) — never a silently wrapped value.

This file states and proves that mirror at the model level, quantified over the
WHOLE boundary of a call: argument crossing (per-element, whole-list), in-WASM
compute (`evalW`, the emitted i64 lowering — consumed via `preservationWasm_inRange`
so a drift in the emitted-lowering model falsifies the mirror), and return crossing
(`__i64ToJs` normalization).

**Bugs these theorems verify (each with its discriminating witness as a
`*_stub_fails` refutation of the SAME predicate):**
  * the `__list_to_wasm` i64 ELEMENT wrap (fixed `1734f82e`, table row
    `list-elem-i64-oob`; PoC 2026-08-16: `pick([2**63+7])` returned
    `-9223372036854775801`) — `listI64Cross_prefixStub_fails` and
    `mirror_unguardedArgs_fails` refute the guard-deleted world at the EXACT
    recorded PoC value `wrapI64 (2^63+7) = -(2^63)+7 = -9223372036854775801`;
  * the i64 arithmetic silent wrap (#358, fixed by overflow-checked ops + the
    `__ovf` twin fallback / `f429d308`) — `mirror_unguardedOvf_fails` refutes the
    guard-deleted world at `2^62 * 4`: surfaced `0` vs JS `2^64`;
  * the partial-crossing class: a mixed list with one out-of-range element must
    divert the WHOLE call, never compute on a half-wrapped list —
    `listI64Cross_divertsOnOob` (+ the mixed-list `#guard` pins).

**Provenance.** The scalar-crossing model (`JsInt`/`wf`/`i64ArgMarshal`/
`i64RetMarshal`/`BridgeMode`/`ArgOutcome`) is restated VERBATIM from the
MarshalTable section landed on `main` (`feat/0.2.2-marshalling-table`, commit
`548280b0`; element-loop lemma `887c8064`) — that section is not an ancestor of
this proof branch, and the restatement lives in the separate `JsWasmMirror`
namespace so the eventual merge is conflict- and clash-free. The reviewer should
reconcile (keep both: main's section owns the table binding; this file owns the
mirror composition).

**Shipping binding (stated, not re-run here).** The scalar/element crossings and
fault dispositions are shipping-bound on main by
`verification/marshalling_shipped_binding.py` (real `pyths` js+wasm vs CPython,
22 boundary observations incl. the PoC list) and the two-sided table gate
(`cargo test marshalling_table_matches_committed_fixture` ↔
`lake exe expanddiff --check-marshalling-table`); the compute leg is bound by the
B-WASM differential (`wasm_shipped_binding.py`). This file composes those bound
pieces into the whole-call mirror statement; it introduces no new unbound claims.

**Trust boundary (explicit premises).** `JsInt.wf` is the runtime's documented int
representation invariant (ints beyond 2^53 are ALWAYS BigInt), an explicit
hypothesis exercised by the differential. The JS engine's `BigInt`/`Number`
exactness inside stated ranges is host-owned. The linear-memory write loop and
`call_indirect` host mechanics stay the trusted sink — this is the COMPILER's
value boundary, not the WASM host.

**B1 (WASM `break`/`continue` under nesting) — OUT OF MODEL.** The WASM lowering
model on this branch (`WExp`/`evalW`) is the scalar EXPRESSION fragment: no loops,
no `br`/`br_if` depth, so a control-flow-preservation statement is not stateable
here without building a structured-control model first. Noted, not forced. -/

namespace JsWasmMirror

open PythExpandVerify

/-! ## The scalar crossing model (restated from main's MarshalTable — provenance
    in the header). `wrapI64` itself is imported from `PythExpandVerify`. -/

/-- The runtime's JS int representation: a Number inside the safe range, a BigInt
    outside it. `wf` is the documented representation invariant — an explicit
    trust-boundary premise, exercised by the shipped-binding differential. -/
inductive JsInt where
  | num (n : Int) | big (n : Int)
  deriving DecidableEq, Repr

def JsInt.val : JsInt → Int
  | .num n => n
  | .big n => n

def jsSafeMax : Int := 2 ^ 53 - 1

def JsInt.wf : JsInt → Bool
  | .num n => decide (-jsSafeMax ≤ n ∧ n ≤ jsSafeMax)
  | .big _ => true

def i64Max : Int := 2 ^ 63 - 1
def i64Min : Int := -(2 ^ 63)

/-- Whether the bridge embeds exact JS twins (`--target js+wasm`) or not
    (edge targets). Both ship. -/
inductive BridgeMode where
  | twins | notwins
  deriving DecidableEq, Repr

/-- Outcome of marshalling one JS int across the boundary. -/
inductive ArgOutcome where
  | pass (w : Int)  -- crossed into WASM as the i64 value w
  | rerouteTwin     -- diverted to the exact JS twin (js+wasm)
  | throwRange      -- loud RangeError (edge targets)
  deriving DecidableEq, Repr

/-- The SHIPPED i64 argument/element crossing: `__i64Oob` guards BigInts beyond
    i64 (Numbers are inside i64 whenever `wf` holds — 2^53 < 2^63); an in-range
    value crosses EXACTLY. Models BOTH the scalar and the post-fix
    `__list_to_wasm` element path (rows `i64-arg-oob` / `list-elem-i64-oob`). -/
def i64ArgMarshal (m : BridgeMode) : JsInt → ArgOutcome
  | .num n => .pass n
  | .big n =>
    if n > i64Max ∨ n < i64Min then
      match m with
      | .twins => .rerouteTwin
      | .notwins => .throwRange
    else .pass n

/-- The SHIPPED i64 return crossing (`__i64ToJs`): Number inside the safe range,
    BigInt outside — always the same mathematical integer, always `wf`. -/
def i64RetMarshal (w : Int) : JsInt :=
  if -jsSafeMax ≤ w ∧ w ≤ jsSafeMax then .num w else .big w

/-- Scalar crossing exactness (restated): a `pass` is ALWAYS the exact value AND
    inside i64 — the conversion never wraps what it lets through. -/
theorem i64ArgMarshal_exact (m : BridgeMode) (v : JsInt) (w : Int)
    (hwf : v.wf = true) (h : i64ArgMarshal m v = .pass w) :
    w = v.val ∧ i64Min ≤ w ∧ w ≤ i64Max := by
  cases v with
  | num n =>
    simp only [i64ArgMarshal, ArgOutcome.pass.injEq] at h
    simp only [JsInt.wf, decide_eq_true_eq, jsSafeMax] at hwf
    subst h
    refine ⟨rfl, ?_, ?_⟩ <;> simp only [i64Min, i64Max] <;> omega
  | big n =>
    simp only [i64ArgMarshal] at h
    split at h
    · cases m <;> simp at h
    · rename_i hin
      simp only [ArgOutcome.pass.injEq] at h
      subst h
      simp only [not_or] at hin
      exact ⟨rfl, by omega, by omega⟩

/-- The return normalization is value-preserving and re-establishes `wf`. -/
theorem i64RetMarshal_exact (w : Int) :
    (i64RetMarshal w).val = w ∧ (i64RetMarshal w).wf = true := by
  unfold i64RetMarshal
  split
  · rename_i hin
    refine ⟨rfl, ?_⟩
    simp only [JsInt.wf, decide_eq_true_eq]
    exact hin
  · exact ⟨rfl, rfl⟩

/-- Projection of `i64RetMarshal_exact` (used as a rewrite). -/
theorem i64RetMarshal_val (w : Int) : (i64RetMarshal w).val = w :=
  (i64RetMarshal_exact w).1

/-- Decidable out-of-i64-range test on the JS side (`__i64Oob`'s predicate). -/
def JsInt.oob : JsInt → Bool
  | .num _ => false
  | .big n => decide (n > i64Max ∨ n < i64Min)

/-- An in-range value crosses as EXACTLY itself, in every mode. -/
theorem i64ArgMarshal_pass_of_notOob (m : BridgeMode) (v : JsInt)
    (h : v.oob = false) : i64ArgMarshal m v = .pass v.val := by
  cases v with
  | num n => rfl
  | big n =>
    have h' := of_decide_eq_false h
    simp only [i64ArgMarshal, if_neg h']
    rfl

/-- An out-of-range value ALWAYS diverts — twin in `twins`, loud in `notwins`. -/
theorem i64ArgMarshal_divert_of_oob (v : JsInt) (h : v.oob = true) :
    i64ArgMarshal .twins v = .rerouteTwin ∧
    i64ArgMarshal .notwins v = .throwRange := by
  cases v with
  | num n => simp [JsInt.oob] at h
  | big n =>
    have h' := of_decide_eq_true h
    refine ⟨?_, ?_⟩ <;> simp only [i64ArgMarshal, if_pos h']

/-- The twins-mode scalar crossing never throws (the fault ladder reroutes). -/
theorem i64ArgMarshal_twins_ne_throwRange (v : JsInt) :
    i64ArgMarshal .twins v ≠ .throwRange := by
  cases v with
  | num n => simp [i64ArgMarshal]
  | big n =>
    simp only [i64ArgMarshal]
    split
    · simp
    · simp

/-! ## The WHOLE-LIST i64 crossing (`__list_to_wasm(x, "i64")`, post-fix)

The shipped element loop marshals elements in order; the FIRST out-of-range
element raises `RangeError`, which aborts the WHOLE crossing (twins: the
`__isWasmFault` ladder re-runs the call on the JS twin; notwins: the RangeError
propagates). The loop therefore never delivers a partially-wrapped list to WASM —
the all-or-nothing discipline the pre-fix code violated. -/

/-- Outcome of crossing a whole `list[int]`. -/
inductive ListOutcome where
  | pass (ws : List Int)  -- every element crossed; ws are the i64 values, in order
  | rerouteTwin
  | throwRange
  deriving DecidableEq, Repr

/-- The whole-list crossing: element-ordered, first fault aborts the call. -/
def listI64Cross (m : BridgeMode) : List JsInt → ListOutcome
  | [] => .pass []
  | v :: vs =>
    match i64ArgMarshal m v with
    | .pass w =>
      match listI64Cross m vs with
      | .pass ws => .pass (w :: ws)
      | .rerouteTwin => .rerouteTwin
      | .throwRange => .throwRange
    | .rerouteTwin => .rerouteTwin
    | .throwRange => .throwRange

/-- **∀-over-elements exactness, crisply: a passed list IS the source list.**
    When the whole crossing passes, the crossed list EQUALS `xs.map JsInt.val` —
    same length, same order, every element the exact source integer — and every
    element is inside i64. (List equality subsumes the pointwise `Forall₂`
    reading of main's `listI64Marshal_exact`: length + order + per-element value
    in one equation.) -/
theorem listI64Cross_exact (m : BridgeMode) (xs : List JsInt) (ws : List Int)
    (hwf : ∀ v ∈ xs, v.wf = true)
    (h : listI64Cross m xs = .pass ws) :
    ws = xs.map JsInt.val ∧ ∀ w ∈ ws, i64Min ≤ w ∧ w ≤ i64Max := by
  induction xs generalizing ws with
  | nil =>
    simp only [listI64Cross, ListOutcome.pass.injEq] at h
    subst h
    exact ⟨rfl, by intro w hw; cases hw⟩
  | cons a as ih =>
    cases ha : i64ArgMarshal m a with
    | pass w =>
      cases hs : listI64Cross m as with
      | pass ws' =>
        simp only [listI64Cross, ha, hs, ListOutcome.pass.injEq] at h
        subst h
        obtain ⟨hw, hw1, hw2⟩ := i64ArgMarshal_exact m a w (hwf a (.head _)) ha
        obtain ⟨hws', hbnd⟩ := ih ws' (fun v hv => hwf v (.tail _ hv)) hs
        subst hws'
        refine ⟨by rw [hw]; rfl, ?_⟩
        intro x hx
        cases hx with
        | head => exact ⟨hw1, hw2⟩
        | tail _ hx' => exact hbnd x hx'
      | rerouteTwin => simp [listI64Cross, ha, hs] at h
      | throwRange => simp [listI64Cross, ha, hs] at h
    | rerouteTwin => simp [listI64Cross, ha] at h
    | throwRange => simp [listI64Cross, ha] at h

/-- Return-marshalling every element of a crossed list is the identity on the
    carried integers (helper for the round-trip). -/
theorem retVal_map_id (l : List Int) :
    l.map (fun w => (i64RetMarshal w).val) = l := by
  induction l with
  | nil => rfl
  | cons a as ih => rw [List.map_cons, i64RetMarshal_val, ih]

/-- **List boundary round-trip (the mirror on values).** A `list[int]` that
    crosses in and comes back element-by-element is the SAME list of integers —
    and every element returns in a `wf` representation. This is the JS↔WASM
    round-trip whose per-element violation was the `pick([2**63+7])` bug. -/
theorem listI64_boundary_roundtrip (m : BridgeMode) (xs : List JsInt)
    (ws : List Int) (hwf : ∀ v ∈ xs, v.wf = true)
    (h : listI64Cross m xs = .pass ws) :
    ws.map (fun w => (i64RetMarshal w).val) = xs.map JsInt.val
      ∧ ∀ r ∈ ws.map i64RetMarshal, r.wf = true := by
  obtain ⟨hws, _⟩ := listI64Cross_exact m xs ws hwf h
  subst hws
  refine ⟨retVal_map_id _, ?_⟩
  intro r hr
  rw [List.mem_map] at hr
  obtain ⟨w, _, hw⟩ := hr
  rw [← hw]
  exact (i64RetMarshal_exact w).2

/-- **All-or-nothing: ONE out-of-range element diverts the WHOLE call.** The
    crossing never computes on a half-wrapped list — a mixed list with any oob
    element re-runs entirely on the twin (twins) or throws (notwins). This is the
    partial-crossing class the pre-fix element loop violated. -/
theorem listI64Cross_divertsOnOob (xs : List JsInt) (v : JsInt)
    (hv : v ∈ xs) (hoob : v.oob = true) :
    listI64Cross .twins xs = .rerouteTwin ∧
    listI64Cross .notwins xs = .throwRange := by
  induction xs with
  | nil => cases hv
  | cons a as ih =>
    cases hv with
    | head =>
      obtain ⟨h1, h2⟩ := i64ArgMarshal_divert_of_oob v hoob
      exact ⟨by simp only [listI64Cross, h1], by simp only [listI64Cross, h2]⟩
    | tail _ hv' =>
      obtain ⟨ih1, ih2⟩ := ih hv'
      constructor
      · cases ha : i64ArgMarshal .twins a with
        | pass w => simp only [listI64Cross, ha, ih1]
        | rerouteTwin => simp only [listI64Cross, ha]
        | throwRange => exact absurd ha (i64ArgMarshal_twins_ne_throwRange a)
      · cases ha : i64ArgMarshal .notwins a with
        | pass w => simp only [listI64Cross, ha, ih2]
        | rerouteTwin =>
          -- notwins never reroutes: contradiction from the definition
          exfalso
          cases a with
          | num n => simp [i64ArgMarshal] at ha
          | big n =>
            simp only [i64ArgMarshal] at ha
            split at ha
            · simp at ha
            · simp at ha
        | throwRange => simp only [listI64Cross, ha]

/-- The twins-mode whole-list crossing never throws (fault ladder → twin). -/
theorem listI64Cross_twins_ne_throwRange (xs : List JsInt) :
    listI64Cross .twins xs ≠ .throwRange := by
  induction xs with
  | nil => simp [listI64Cross]
  | cons a as ih =>
    cases ha : i64ArgMarshal .twins a with
    | pass w =>
      cases hs : listI64Cross .twins as with
      | pass ws => simp [listI64Cross, ha, hs]
      | rerouteTwin => simp [listI64Cross, ha, hs]
      | throwRange => exact absurd hs ih
    | rerouteTwin => simp [listI64Cross, ha]
    | throwRange => exact absurd ha (i64ArgMarshal_twins_ne_throwRange a)

/-- **STUB — the PRE-FIX element loop (raw per-element `ToBigInt64`, no guard):
    wraps every element mod 2^64 and always "passes".** This is the shipped
    behavior `1734f82e` fixed (PoC: `pick([2**63+7])`). -/
def listI64Cross_prefixStub (xs : List JsInt) : ListOutcome :=
  .pass (xs.map (fun v => wrapI64 v.val))

/-- **The element guard is load-bearing (the `pick([2**63+7])` bug).** The
    pre-fix loop VIOLATES whole-list exactness at the recorded PoC witness: it
    passes `[wrapI64 (2^63+7)] = [-(2^63)+7] = [-9223372036854775801]` — the
    EXACT value the PoC observed — where the source list is `[2^63+7]`. The
    fixed crossing diverts this witness (see the `#guard` pins below). -/
theorem listI64Cross_prefixStub_fails :
    ¬ (∀ (xs : List JsInt) (ws : List Int),
        (∀ v ∈ xs, v.wf = true) →
        listI64Cross_prefixStub xs = .pass ws →
        ws = xs.map JsInt.val ∧ ∀ w ∈ ws, i64Min ≤ w ∧ w ≤ i64Max) := by
  intro h
  have hbad := h [.big (2 ^ 63 + 7)] [wrapI64 (2 ^ 63 + 7)]
      (by intro v hv
          cases hv with
          | head => rfl
          | tail _ hv' => cases hv')
      rfl
  have h1 : wrapI64 (2 ^ 63 + 7) = -(2 ^ 63) + 7 := by decide
  have h2 := hbad.1
  rw [h1] at h2
  simp only [List.map_cons, List.map_nil, List.cons.injEq] at h2
  exact absurd h2.1 (by decide)

/-! ## The END-TO-END routed-call mirror

A routed unary/n-ary numeric kernel `f(params) = e` called from JS with `args`:

  * **JS lowering** (`jsMirrorCall`): arbitrary-precision compute — `evalPyW`
    over the args' integer values (the JS twin IS this path).
  * **WASM lowering** (`wasmMirrorCall`): the whole boundary — cross the args
    (`listI64Cross`), compute on the EMITTED i64 lowering (`evalW`, consumed
    directly: an emitted-lowering drift falsifies the mirror), `__ovf`-guard the
    result (out-of-range true value → twin/OverflowError, mirroring
    `wasmRunEdge`/`wasmRunJsWasm` §7b), and return-marshal (`__i64ToJs`).

MIRROR EQUIVALENCE (`jsWasmMirror_agree`): every value the WASM lowering
SURFACES equals the JS lowering's value, in a `wf` representation. For the
primary `js+wasm` target the mirror is TOTAL (`jsWasm_mirror_total`): counting
the twin re-run, the routed call is observationally EQUAL to the JS lowering. -/

/-- Outcome of a routed call's WASM attempt, as observed at the JS boundary. -/
inductive CallOutcome where
  | ret (r : JsInt)  -- a value surfaced to JS
  | rerouteTwin      -- diverted: re-run on the exact JS twin (js+wasm)
  | throwRange       -- loud RangeError (edge targets, arg out of i64)
  | throwOverflow    -- loud OverflowError (edge targets, result out of i64)
  deriving DecidableEq, Repr

/-- The WASM lowering of a routed call, whole boundary included. `none` = the
    kernel itself is stuck (unbound variable) — not a boundary outcome. -/
def wasmMirrorCall (m : BridgeMode) (ps : List String) (e : WExp)
    (args : List JsInt) : Option CallOutcome :=
  match listI64Cross m args with
  | .rerouteTwin => some .rerouteTwin
  | .throwRange => some .throwRange
  | .pass ws =>
    match evalPyW e (ps.zip ws) with
    | none => none
    | some v =>
      if -(2 ^ 63) ≤ v ∧ v < 2 ^ 63 then
        (evalW e (ps.zip ws)).map (fun w => CallOutcome.ret (i64RetMarshal w))
      else
        some (match m with | .twins => .rerouteTwin | .notwins => .throwOverflow)

/-- The pure-JS lowering of the same routed call (arbitrary precision). -/
def jsMirrorCall (ps : List String) (e : WExp) (args : List JsInt) : Option Int :=
  evalPyW e (ps.zip (args.map JsInt.val))

/-- **THE MIRROR-EQUIVALENCE THEOREM.** Under the runtime representation
    invariant, EVERY value the WASM lowering surfaces to JS equals the JS
    lowering's value — same mathematical integer, `wf` representation. The proof
    composes the three exactness legs: argument-crossing exactness
    (`listI64Cross_exact`), emitted-compute correctness in range
    (`preservationWasm_inRange`, consumed directly on `evalW`), and return
    exactness (`i64RetMarshal_exact`). A silently wrapped surfaced value —
    either bug class below — is forbidden by this statement. -/
theorem jsWasmMirror_agree (m : BridgeMode) (ps : List String) (e : WExp)
    (args : List JsInt) (r : JsInt)
    (hwf : ∀ v ∈ args, v.wf = true)
    (h : wasmMirrorCall m ps e args = some (.ret r)) :
    jsMirrorCall ps e args = some r.val ∧ r.wf = true := by
  cases hc : listI64Cross m args with
  | rerouteTwin => simp [wasmMirrorCall, hc] at h
  | throwRange => simp [wasmMirrorCall, hc] at h
  | pass ws =>
    obtain ⟨hws, _⟩ := listI64Cross_exact m args ws hwf hc
    subst hws
    simp only [wasmMirrorCall, hc] at h
    cases hv : evalPyW e (ps.zip (args.map JsInt.val)) with
    | none => simp [hv] at h
    | some v =>
      simp only [hv] at h
      split at h
      · rename_i hr
        rw [preservationWasm_inRange e (ps.zip (args.map JsInt.val)) v hv hr.1 hr.2,
            hv] at h
        simp only [Option.map_some, Option.some.injEq, CallOutcome.ret.injEq] at h
        subst h
        exact ⟨by simp only [jsMirrorCall, hv, i64RetMarshal_val],
               (i64RetMarshal_exact v).2⟩
      · cases m <;> simp at h

/-- The twins-mode WASM attempt never throws: args out of range and result
    overflow both reroute to the twin (the `__isWasmFault` ladder). -/
theorem wasmMirrorCall_twins_neverThrows (ps : List String) (e : WExp)
    (args : List JsInt) :
    wasmMirrorCall .twins ps e args ≠ some .throwRange ∧
    wasmMirrorCall .twins ps e args ≠ some .throwOverflow := by
  constructor <;>
  · intro h
    cases hc : listI64Cross .twins args with
    | rerouteTwin => simp [wasmMirrorCall, hc] at h
    | throwRange => exact absurd hc (listI64Cross_twins_ne_throwRange args)
    | pass ws =>
      simp only [wasmMirrorCall, hc] at h
      cases hv : evalPyW e (ps.zip ws) with
      | none => simp [hv] at h
      | some v =>
        simp only [hv] at h
        split at h
        · cases hw : evalW e (ps.zip ws) with
          | none => simp [hw] at h
          | some w => simp [hw] at h
        · simp at h

/-- The primary `js+wasm` routed call, twin re-run included: surface the WASM
    value if the attempt passed, else the twin's (= the JS lowering's) value.
    The throw arms are unreachable in twins mode
    (`wasmMirrorCall_twins_neverThrows`). -/
def jsWasmTwinsRun (ps : List String) (e : WExp) (args : List JsInt) :
    Option Int :=
  match wasmMirrorCall .twins ps e args with
  | some (.ret r) => some r.val
  | some .rerouteTwin => jsMirrorCall ps e args  -- twin re-run = the JS lowering
  | some .throwRange => none
  | some .throwOverflow => none
  | none => none

/-- **TOTAL mirror for the shipped primary target: `js+wasm` ≡ the JS lowering,
    observationally, on EVERY routed call.** Fast path or twin, the surfaced
    integer is exactly the JS lowering's. (The reroute arm is the twin by
    construction — same modeling as §7b `wasmRunJsWasm`; the CONTENT is the
    `.ret` arm via `jsWasmMirror_agree` + the never-throws lemma + the stuck
    case, where crossing exactness forces both sides to agree on `none`.) -/
theorem jsWasm_mirror_total (ps : List String) (e : WExp) (args : List JsInt)
    (hwf : ∀ v ∈ args, v.wf = true) :
    jsWasmTwinsRun ps e args = jsMirrorCall ps e args := by
  unfold jsWasmTwinsRun
  cases ho : wasmMirrorCall .twins ps e args with
  | none =>
    cases hc : listI64Cross .twins args with
    | rerouteTwin => simp [wasmMirrorCall, hc] at ho
    | throwRange => exact absurd hc (listI64Cross_twins_ne_throwRange args)
    | pass ws =>
      obtain ⟨hws, _⟩ := listI64Cross_exact _ args ws hwf hc
      subst hws
      simp only [wasmMirrorCall, hc] at ho
      cases hv : evalPyW e (ps.zip (args.map JsInt.val)) with
      | none => simp only [jsMirrorCall, hv]
      | some v =>
        simp only [hv] at ho
        split at ho
        · cases hw : evalW e (ps.zip (args.map JsInt.val)) with
          | none =>
            rw [preservationWasm, hv] at hw
            simp at hw
          | some w => simp [hw] at ho
        · simp at ho
  | some c =>
    cases c with
    | ret r =>
      have hjs := (jsWasmMirror_agree .twins ps e args r hwf ho).1
      rw [hjs]
    | rerouteTwin => rfl
    | throwRange => exact absurd ho (wasmMirrorCall_twins_neverThrows ps e args).1
    | throwOverflow => exact absurd ho (wasmMirrorCall_twins_neverThrows ps e args).2

/-! ## The bug-verifying stub refutations (guard-deleted worlds) -/

/-- **STUB — the mirror with the PRE-FIX (unguarded) argument crossing**: same
    body as `wasmMirrorCall`, only `listI64Cross` replaced by the wrap-everything
    pre-fix loop. This is the shipped world before `1734f82e`. -/
def wasmMirrorCall_unguardedArgs (ps : List String) (e : WExp)
    (args : List JsInt) : Option CallOutcome :=
  match listI64Cross_prefixStub args with
  | .rerouteTwin => some .rerouteTwin
  | .throwRange => some .throwRange
  | .pass ws =>
    match evalPyW e (ps.zip ws) with
    | none => none
    | some v =>
      if -(2 ^ 63) ≤ v ∧ v < 2 ^ 63 then
        (evalW e (ps.zip ws)).map (fun w => CallOutcome.ret (i64RetMarshal w))
      else some .rerouteTwin

/-- **The argument guard is load-bearing end-to-end (the `pick([2**63+7])`
    bug).** In the guard-deleted world, the identity kernel over
    `[2^63+7]` SURFACES `-(2^63)+7 = -9223372036854775801` — the exact recorded
    PoC value — while the JS lowering gives `2^63+7`. The mirror predicate
    (same as `jsWasmMirror_agree`) is refuted at that witness; the FIXED
    mirror diverts it to the twin (see the `#guard` pins). -/
theorem mirror_unguardedArgs_fails :
    ¬ (∀ (ps : List String) (e : WExp) (args : List JsInt) (r : JsInt),
        (∀ v ∈ args, v.wf = true) →
        wasmMirrorCall_unguardedArgs ps e args = some (.ret r) →
        jsMirrorCall ps e args = some r.val ∧ r.wf = true) := by
  intro h
  have hbad := h ["x"] (.var "x") [.big (2 ^ 63 + 7)] (.big (-(2 ^ 63) + 7))
      (by intro v hv
          cases hv with
          | head => rfl
          | tail _ hv' => cases hv')
      (by decide)
  exact absurd hbad.1 (by decide)

/-- **STUB — the mirror with the `__ovf` guard DELETED** (#358's world): the
    compute leg surfaces the raw i64 result unconditionally. -/
def wasmMirrorCall_unguardedOvf (ps : List String) (e : WExp)
    (args : List JsInt) : Option CallOutcome :=
  match listI64Cross .twins args with
  | .rerouteTwin => some .rerouteTwin
  | .throwRange => some .throwRange
  | .pass ws => (evalW e (ps.zip ws)).map (fun w => CallOutcome.ret (i64RetMarshal w))

/-- **The `__ovf` guard is load-bearing end-to-end (#358).** With the guard
    deleted, `2^62 * 4` SURFACES `0` (the wrapped i64) while the JS lowering
    gives `2^64` — the silent-wrap divergence #358 fixed (overflow-checked ops +
    twin fallback). Refuted on the SAME mirror predicate. -/
theorem mirror_unguardedOvf_fails :
    ¬ (∀ (ps : List String) (e : WExp) (args : List JsInt) (r : JsInt),
        (∀ v ∈ args, v.wf = true) →
        wasmMirrorCall_unguardedOvf ps e args = some (.ret r) →
        jsMirrorCall ps e args = some r.val ∧ r.wf = true) := by
  intro h
  have hbad := h [] (.mul (.lit (2 ^ 62)) (.lit 4)) [] (.num 0)
      (by intro v hv; cases hv) (by decide)
  exact absurd hbad.1 (by decide)

/-! ## Non-vacuity + fixed-behavior pins (executable) -/

-- The mirror hypothesis is satisfiable: a genuine full pass, small and at the
-- i64 edge (a BigInt near the boundary survives the round-trip EXACTLY).
#guard wasmMirrorCall .twins ["x"] (.add (.var "x") (.lit 1)) [.num 41]
    = some (.ret (.num 42))
#guard wasmMirrorCall .twins ["x"] (.var "x") [.big (2 ^ 63 - 2)]
    = some (.ret (.big (2 ^ 63 - 2)))

-- FIXED behavior at the PoC witness: the oob list element diverts the WHOLE
-- call (twin / loud), never a wrapped pass.
#guard wasmMirrorCall .twins ["x"] (.var "x") [.big (2 ^ 63 + 7)] = some .rerouteTwin
#guard wasmMirrorCall .notwins ["x"] (.var "x") [.big (2 ^ 63 + 7)] = some .throwRange
#guard listI64Cross .twins [.num 5, .big (2 ^ 63 + 7)] = .rerouteTwin   -- mixed list: all-or-nothing
#guard listI64Cross .notwins [.num 5, .big (2 ^ 63 + 7)] = .throwRange

-- FIXED behavior at the #358 witness: result overflow reroutes (twins) /
-- fails loud (notwins), never a silent 0.
#guard wasmMirrorCall .twins [] (.mul (.lit (2 ^ 62)) (.lit 4)) [] = some .rerouteTwin
#guard wasmMirrorCall .notwins [] (.mul (.lit (2 ^ 62)) (.lit 4)) [] = some .throwOverflow

-- The guard-deleted worlds reproduce the RECORDED bug values (the PoC's
-- -9223372036854775801 and #358's silent 0) — the discriminating witnesses.
#guard wasmMirrorCall_unguardedArgs ["x"] (.var "x") [.big (2 ^ 63 + 7)]
    = some (.ret (.big (-(2 ^ 63) + 7)))
#guard (-(2 ^ 63) + 7 : Int) = -9223372036854775801
#guard wasmMirrorCall_unguardedOvf [] (.mul (.lit (2 ^ 62)) (.lit 4)) []
    = some (.ret (.num 0))
#guard listI64Cross_prefixStub [.num 5, .big (2 ^ 63 + 7)]
    = .pass [5, -(2 ^ 63) + 7]                       -- the half-wrapped list (pre-fix)

-- The total js+wasm mirror at an overflow witness: fast path diverts, twin
-- surfaces the exact bigint — observationally the JS lowering.
#guard jsWasmTwinsRun [] (.mul (.lit (2 ^ 62)) (.lit 4)) [] = some (2 ^ 64)
#guard jsMirrorCall [] (.mul (.lit (2 ^ 62)) (.lit 4)) [] = some (2 ^ 64)

/-! ## SPOTs — client facts proved THROUGH the mirror theorems (not by eval) -/

/-- SPOT (through `jsWasmMirror_agree`): a surfaced value at the in-range witness
    is pinned to EXACTLY the JS lowering's integer — fails if the mirror is
    weakened to not determine the value. -/
example (r : JsInt)
    (h : wasmMirrorCall .twins ["x"] (.add (.var "x") (.lit 1)) [.num 41]
        = some (.ret r)) :
    jsMirrorCall ["x"] (.add (.var "x") (.lit 1)) [.num 41] = some r.val :=
  (jsWasmMirror_agree .twins _ _ _ r
    (by intro v hv
        cases hv with
        | head => rfl
        | tail _ hv' => cases hv') h).1

/-- SPOT (through `jsWasm_mirror_total`): the squaring kernel on `2^40` (whose
    square `2^80` overflows i64, so the fast path MUST divert) is still
    observationally the JS lowering — the twin absorbs the overflow. -/
example : jsWasmTwinsRun ["x"] (.mul (.var "x") (.var "x")) [.big (2 ^ 40)]
    = jsMirrorCall ["x"] (.mul (.var "x") (.var "x")) [.big (2 ^ 40)] :=
  jsWasm_mirror_total _ _ _
    (by intro v hv
        cases hv with
        | head => rfl
        | tail _ hv' => cases hv')

/-- SPOT (through `listI64_boundary_roundtrip`): a crossed list comes back as
    the same integers — pinned through the theorem, not evaluation. -/
example (ws : List Int)
    (h : listI64Cross .twins [.num 5, .num (-3)] = .pass ws) :
    ws.map (fun w => (i64RetMarshal w).val) = [5, -3] :=
  (listI64_boundary_roundtrip .twins _ ws
    (by intro v hv
        cases hv with
        | head => rfl
        | tail _ hv' =>
          cases hv' with
          | head => rfl
          | tail _ hv'' => cases hv'') h).1

/-! ## Axiom pins (per-declaration) -/

/-- info: 'JsWasmMirror.i64ArgMarshal_exact' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms i64ArgMarshal_exact

/-- info: 'JsWasmMirror.listI64Cross_exact' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms listI64Cross_exact

/-- info: 'JsWasmMirror.listI64_boundary_roundtrip' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms listI64_boundary_roundtrip

/-- info: 'JsWasmMirror.listI64Cross_divertsOnOob' depends on axioms: [propext] -/
#guard_msgs in
#print axioms listI64Cross_divertsOnOob

/-- info: 'JsWasmMirror.listI64Cross_prefixStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms listI64Cross_prefixStub_fails

/-- info: 'JsWasmMirror.jsWasmMirror_agree' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms jsWasmMirror_agree

/-- info: 'JsWasmMirror.jsWasm_mirror_total' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms jsWasm_mirror_total

/-- info: 'JsWasmMirror.mirror_unguardedArgs_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms mirror_unguardedArgs_fails

/-- info: 'JsWasmMirror.mirror_unguardedOvf_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms mirror_unguardedOvf_fails

end JsWasmMirror
