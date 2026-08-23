/-
# RpcBoundary — C7 (object identity / aliasing) + §8.6 tag disjointness

Formalizes two rows of `rpc-boundary-spec.md` (reference-app), for the taxonomy in
the design notes:

* **§8.4 / taxonomy C7** — object identity / aliasing:
  - WITHIN a side, aliasing is preserved (a call passes object references);
  - ACROSS the typed RPC boundary it is provably NOT preserved — two
    parameters referencing ONE object arrive as TWO objects. C7 is the first
    taxonomy component recorded as a *deviation by construction* (the wire
    form is structural JSON: it has no constructor that can carry an object
    address, so sharing is unrepresentable on the wire).
* **§8.6** — tag disjointness: over the closed tag set of §5, no two
  constructor heads share a wire tag. Finite, `decide`-able (`route()`-style
  row check).

## Statement-quality manifest (lean-spec-quality gates applied)

* **Shape.** ONE predicate (`PreservesIdentity`), instantiated positively by
  the same-side transport (`local_preserves_identity`) and REFUTED by the RPC
  transport (`rpc_not_preserves_identity`). Same-predicate-two-instantiations
  is the accepted C1 pattern (the refutation is the `*_stub_fails`-style
  discriminator, here of a real designed deviation, not of a strawman).
* **Anti-garbage / anti-vacuity companion.** `¬ PreservesIdentity rpcCall`
  alone would also hold of a garbage transport (`fun _ _ => ([], [])`).
  `rpc_preserves_value` pins `rpcCall` as a value-faithful boundary model
  (position-wise denotations preserved — the C1 face of the same crossing),
  so the refutation is IDENTITY-specific. This is the stub-litmus applied to
  a negative theorem.
* **Observational grounding (F4 guard).** Address equality could be a model
  artifact. `alias_iff_mutation_visible` proves `a = b` ↔ mutation through
  `a` is visible at `b`, so `aliased` is the observational notion (Python's
  `is` / mutation visibility), not an encoding accident.
* **Discriminating witness.** `local_mutation_visible` vs
  `rpc_mutation_invisible`: the SAME two-alias program observes `[42, 7]`
  locally and `[42]` across the boundary. The observations DIFFER, so the
  witness discriminates (a witness both transports satisfy would pin
  nothing).
* **Precondition non-vacuity (Move-borrow-checker move).** The only
  hypothesis, `WF`, has a decidable checker `wfB` with soundness
  (`wfB_sound`) and a kernel-checked concrete inhabitant (`#guard wfB …`),
  so the quantified domain is certified non-empty.
* **False-world control.** In the false world "identity IS preserved across
  the boundary", `rpc_not_preserves_identity` is a direct contradiction —
  the claim is disprovable, not unfalsifiable. Conversely
  `local_preserves_identity` shows the predicate is satisfiable, so the
  refutation is not vacuous (the predicate is neither always-true nor
  always-false).

## Binding status (HONEST — read before citing)

`rpc-boundary-spec.md` is a DESIGN document: **no RPC boundary is
implemented** (its own header: "Nothing here is implemented"). These
theorems are therefore MODEL-LEVEL statements about the *designed* boundary
(§4 wire grammar, §5 conversion rules), not shipping-bound claims. The
differential binding of spec §10 (generated-corpus round-trip, cross-side
execution differential, tag-table fixture regenerated from both sides) is
deferred to the boundary's implementation (0.3.x scope). Record in TRUST.md
at the *model* tier; do NOT round up to the preservation waves' tier.

Faithfulness of the model to the spec, point by point:
* `Wire` here is a two-constructor fragment (`num`, `arr`) of the §4 grammar
  — the fragment reachable from `int` and `list[int]`. The load-bearing
  property is shared with the full grammar: NO `Wire` constructor carries an
  address. That is spec §2 ("No identity tracking … Explicitly refused") made
  structural.
* `localCall` models a same-side call: CPython and the emitted JS both pass
  object references unchanged (pass-by-object-reference / JS object
  references).
* `rpcCall` models the designed crossing: encode each parameter structurally
  against the caller heap, decode each into a FRESH callee heap, each
  decoded compound value freshly allocated (JSON.parse-fresh-objects; spec
  §2's refusal of a whole-graph memo pass is what makes per-value fresh
  allocation the only possible decode).
-/

namespace RpcBoundary

/-! ## The reference model

Minimal heap/reference model. This is deliberately NOT the taxonomy's
`PyValue` (PythExpandVerify): `PyValue` is purely structural — two
structurally equal values are indistinguishable in it, which is exactly the
quotient C7 must NOT take (C7 is the reference-level question the value
model abstracts away; the design notes §2 "Why C7 is separate").
A heap is forced, and it is kept minimal: one mutable object kind (a list
of ints — Python's canonical mutable object) plus immutable ints. -/

/-- Object addresses (references). -/
abbrev Addr := Nat

/-- One heap object: a mutable list of integers. -/
abbrev Obj := List Int

/-- The heap: allocation list, `Addr`-indexed; allocation appends. -/
abbrev Heap := List Obj

/-- A parameter value: an immutable scalar or a reference to a heap object. -/
inductive Val where
  | int (n : Int)
  | ref (a : Addr)
deriving DecidableEq, Repr

/-- Well-formedness: every reference in the argument vector is in range.
(The mandatory range precondition; dangling references are unreachable in
real programs.) -/
def WF (h : Heap) (vs : List Val) : Prop :=
  ∀ a, Val.ref a ∈ vs → a < h.length

/-- Decidable checker for `WF` (non-vacuity certificate: see `#guard` below). -/
def wfB (h : Heap) (vs : List Val) : Bool :=
  vs.all fun v => match v with
    | .ref a => a < h.length
    | .int _ => true

theorem wfB_sound (h : Heap) (vs : List Val) (hb : wfB h vs = true) : WF h vs := by
  intro a ha
  have := (List.all_eq_true.mp hb) _ ha
  simpa using this

-- The precondition's domain is non-empty — kernel-checked.
#guard wfB [[42]] [.ref 0, .ref 0]

/-! ## Observation: denotation and mutation -/

/-- What a callee can observe of one parameter, structurally: the scalar, or
the CONTENT of the referenced object. (Identity-blind on purpose — this is
the C1-style observation, used to show `rpcCall` is value-faithful.) -/
inductive Deno where
  | int (n : Int)
  | list (xs : List Int)
deriving DecidableEq, Repr

def denoteVal (h : Heap) : Val → Option Deno
  | .int n => some (.int n)
  | .ref a => (h[a]?).map .list

/-! ### Heap read/write lemmas (self-contained, no Mathlib) -/

theorem read_append_left {h : Heap} (ext : Heap) {a : Addr}
    (ha : a < h.length) : (h ++ ext)[a]? = h[a]? := by
  induction h generalizing a with
  | nil => cases ha
  | cons o t ih =>
    cases a with
    | zero => rfl
    | succ n => exact ih (Nat.lt_of_succ_lt_succ ha)

theorem read_append_length (h : Heap) (x : Obj) (rest : List Obj) :
    (h ++ x :: rest)[h.length]? = some x := by
  induction h with
  | nil => rfl
  | cons o t ih => exact ih

theorem read_set_self {h : Heap} {a : Addr} (ha : a < h.length) (o : Obj) :
    (h.set a o)[a]? = some o := by
  induction h generalizing a with
  | nil => cases ha
  | cons x t ih =>
    cases a with
    | zero => rfl
    | succ n => simpa using ih (Nat.lt_of_succ_lt_succ ha)

theorem read_set_ne {h : Heap} {a b : Addr} (hab : a ≠ b) (o : Obj) :
    (h.set a o)[b]? = h[b]? := by
  induction h generalizing a b with
  | nil => rfl
  | cons x t ih =>
    cases a with
    | zero =>
      cases b with
      | zero => exact absurd rfl hab
      | succ m => rfl
    | succ n =>
      cases b with
      | zero => rfl
      | succ m =>
        simpa using ih (a := n) (b := m) (fun hnm => hab (congrArg Nat.succ hnm))

/-- Address equality is the OBSERVATIONAL notion of aliasing: two in-range
references are equal iff mutation through one is visible through the other.
This grounds the model's `aliased` (below) in mutation observability —
Python's actual notion — so the C7 refutation is not an artifact of the
heap encoding. -/
theorem alias_iff_mutation_visible (h : Heap) (a b : Addr)
    (ha : a < h.length) (hb : b < h.length) :
    a = b ↔ ∀ o : Obj, (h.set a o)[b]? = some o := by
  constructor
  · intro hab o
    subst hab
    exact read_set_self ha o
  · intro hobs
    by_cases hab : a = b
    · exact hab
    · exfalso
      -- pick a value the slot at `b` cannot already hold
      obtain ⟨ob, hob⟩ : ∃ o, h[b]? = some o := ⟨h[b], List.getElem?_eq_getElem hb⟩
      have hvis := hobs (ob ++ [0])
      rw [read_set_ne hab, hob] at hvis
      have heq : ob = ob ++ [0] := by injection hvis
      have hlen := congrArg List.length heq
      simp at hlen

/-! ## Transports: the same-side call and the RPC crossing -/

/-- A transport: how an argument vector moves from caller to callee
(resulting callee-side heap + callee-side argument vector). -/
abbrev Transport := Heap → List Val → Heap × List Val

/-- Same-side call: object references are passed unchanged
(CPython pass-by-object-reference; JS object references in the emitted code). -/
def localCall : Transport := fun h vs => (h, vs)

/-- Wire form — the fragment of the §4 grammar reachable from `int` and
`list[int]`. THE load-bearing structural fact, shared with the full §4
grammar: no constructor carries an `Addr`. Sharing is unrepresentable on
the wire BY CONSTRUCTION (spec §2: identity tracking explicitly refused). -/
inductive Wire where
  | num (n : Int)
  | arr (xs : List Int)
deriving DecidableEq, Repr

def wireDenote : Wire → Deno
  | .num n => .int n
  | .arr xs => .list xs

/-- Encode one parameter against the caller heap (§5 rows for `int` /
`list[int]`: structural, deep — the object's CONTENT crosses, its identity
does not). The `getD []` branch is unreachable on the `WF` domain. -/
def encodeVal (h : Heap) : Val → Wire
  | .int n => .num n
  | .ref a => .arr ((h[a]?).getD [])

/-- Decode one wire value into the callee heap. A compound wire value
allocates a FRESH callee object — with no address on the wire and no
whole-payload memo pass (refused, spec §2), fresh allocation is the only
possible decode. -/
def decodeVal (h : Heap) : Wire → Heap × Val
  | .num n => (h, .int n)
  | .arr xs => (h ++ [xs], .ref h.length)

def decodeAll (h : Heap) : List Wire → Heap × List Val
  | [] => (h, [])
  | w :: ws =>
    let p := decodeVal h w
    let q := decodeAll p.1 ws
    (q.1, p.2 :: q.2)

/-- The RPC crossing: encode every parameter against the caller heap, decode
all of them into a fresh callee heap (`[]` — the callee is a separate
address space). -/
def rpcCall : Transport := fun h vs => decodeAll [] (vs.map (encodeVal h))

/-! ## C7 — the statement pair -/

/-- Argument positions `i` and `j` alias: both hold a reference to the SAME
object. (Grounded observationally by `alias_iff_mutation_visible`.) -/
def aliased (vs : List Val) (i j : Nat) : Prop :=
  ∃ a, vs[i]? = some (.ref a) ∧ vs[j]? = some (.ref a)

/-- Transport `t` preserves object identity: argument positions that alias
before the crossing still alias after it. Stated over address equality of
the OUTPUT vector, so it is robust under uniform address renaming — a
transport that relocates every object coherently still preserves identity;
only genuine unsharing refutes it. -/
def PreservesIdentity (t : Transport) : Prop :=
  ∀ h vs, WF h vs → ∀ i j, aliased vs i j → aliased (t h vs).2 i j

/-- **C7, scoped positive** — WITHIN a side, identity is preserved: a call
passes references, so aliasing arrives intact. (Definitionally direct —
that IS the semantics of a same-side call; its role here is to certify
`PreservesIdentity` is satisfiable, so the boundary refutation below is
about the boundary, not about an unsatisfiable predicate.) -/
theorem local_preserves_identity : PreservesIdentity localCall :=
  fun _ _ _ _ _ hal => hal

/-- **C7, the deviation** — ACROSS the RPC boundary, identity preservation
is REFUTED: two parameters referencing one object (`[.ref 0, .ref 0]` over
the one-object heap `[[42]]`) arrive as two distinct objects. This is
spec §8.4 made a theorem: the taxonomy must record C7 as a deviation at
the boundary, because its preservation is provably false there. -/
theorem rpc_not_preserves_identity : ¬ PreservesIdentity rpcCall := by
  intro hp
  have hwf : WF [[42]] [.ref 0, .ref 0] :=
    wfB_sound _ _ (by decide)
  have hal : aliased [Val.ref 0, Val.ref 0] 0 1 := ⟨0, rfl, rfl⟩
  obtain ⟨a, h0, h1⟩ := hp [[42]] [.ref 0, .ref 0] hwf 0 1 hal
  -- output vector computes to [.ref 0, .ref 1]: positions 0 and 1 differ
  have e0 : (rpcCall [[42]] [Val.ref 0, Val.ref 0]).2[0]? = some (.ref 0) := rfl
  have e1 : (rpcCall [[42]] [Val.ref 0, Val.ref 0]).2[1]? = some (.ref 1) := rfl
  have h01 : (some (Val.ref 0) : Option Val) = some (Val.ref 1) := by
    rw [← e0, ← e1]; exact h0.trans h1.symm
  exact absurd h01 (by decide)

/-! ### The anti-garbage companion: the boundary is VALUE-faithful

Without this, `rpc_not_preserves_identity` would also hold of a transport
that destroys everything. It does not: `rpcCall` preserves every
parameter's structural denotation, position by position. Identity is the
ONLY thing it loses — which is precisely the C7 claim (C1 holds across the
crossing; C7 does not). -/

theorem decodeAll_heap_extends (h : Heap) (ws : List Wire) :
    ∃ ext, (decodeAll h ws).1 = h ++ ext := by
  induction ws generalizing h with
  | nil => exact ⟨[], by simp [decodeAll]⟩
  | cons w ws ih =>
    cases w with
    | num n =>
      simpa [decodeAll, decodeVal] using ih h
    | arr xs =>
      obtain ⟨ext, hext⟩ := ih (h ++ [xs])
      exact ⟨xs :: ext, by simp [decodeAll, decodeVal, hext]⟩

/-- Every decoded value's denotation IN THE FINAL CALLEE HEAP equals its
wire content — later allocations do not disturb earlier ones. -/
theorem decodeAll_denote (h : Heap) (ws : List Wire) :
    ((decodeAll h ws).2).map (denoteVal (decodeAll h ws).1)
      = ws.map (fun w => some (wireDenote w)) := by
  induction ws generalizing h with
  | nil => simp [decodeAll]
  | cons w ws ih =>
    cases w with
    | num n =>
      simpa [decodeAll, decodeVal, denoteVal, wireDenote] using ih h
    | arr xs =>
      obtain ⟨ext, hext⟩ := decodeAll_heap_extends (h ++ [xs]) ws
      have hread : ((decodeAll (h ++ [xs]) ws).1)[h.length]? = some xs := by
        rw [hext, List.append_assoc]
        exact read_append_length h xs ext
      simpa [decodeAll, decodeVal, denoteVal, wireDenote, hread]
        using ih (h ++ [xs])

/-- Encoding is denotation-faithful on the `WF` domain. -/
theorem encode_denote (h : Heap) (v : Val)
    (hv : ∀ a, v = .ref a → a < h.length) :
    some (wireDenote (encodeVal h v)) = denoteVal h v := by
  cases v with
  | int n => rfl
  | ref a =>
    have ha : a < h.length := hv a rfl
    have ho : h[a]? = some h[a] := List.getElem?_eq_getElem ha
    simp [encodeVal, denoteVal, wireDenote, ho]

/-- **The RPC crossing preserves structural VALUE (C1-face), position-wise.**
Together with `rpc_not_preserves_identity`, this is C7 stated exactly: the
boundary is value-faithful and identity-losing. -/
theorem rpc_preserves_value (h : Heap) (vs : List Val) (hwf : WF h vs) :
    ((rpcCall h vs).2).map (denoteVal (rpcCall h vs).1)
      = vs.map (denoteVal h) := by
  show ((decodeAll [] (vs.map (encodeVal h))).2).map
        (denoteVal (decodeAll [] (vs.map (encodeVal h))).1)
      = vs.map (denoteVal h)
  rw [decodeAll_denote, List.map_map]
  apply List.map_congr_left
  intro v hv
  exact encode_denote h v (fun a hva => hwf a (hva ▸ hv))

-- SPOT (through the theorem, NOT by evaluation — fails if
-- `rpc_preserves_value` is ever weakened): the concrete witness program's
-- callee observes exactly the caller's structural values.
example :
    ((rpcCall [[42]] [.ref 0, .ref 0]).2).map
        (denoteVal (rpcCall [[42]] [.ref 0, .ref 0]).1)
      = [some (.list [42]), some (.list [42])] := by
  rw [rpc_preserves_value _ _ (wfB_sound _ _ (by decide))]
  rfl

/-! ### The discriminating witness — mutation visibility

The observable consequence of C7, as one program observed under both
transports. The observations DIFFER (`[42, 7]` vs `[42]`), so the witness
discriminates between identity-preserving and identity-losing transports —
a witness both satisfy would pin nothing. This is the honest content of
"two aliases become two objects": a mutation through one is no longer seen
through the other. -/

/-- The observation program: callee appends `7` through argument 0, then
reads argument 1. -/
def mutateThenObserve (p : Heap × List Val) : Option Obj :=
  match p.2[0]?, p.2[1]? with
  | some (Val.ref a), some (Val.ref b) =>
      (p.1.set a (((p.1[a]?).getD []) ++ [7]))[b]?
  | _, _ => none

/-- Same side: the mutation IS visible through the alias. -/
theorem local_mutation_visible :
    mutateThenObserve (localCall [[42]] [.ref 0, .ref 0]) = some [42, 7] := rfl

/-- Across the boundary: the SAME program does NOT see the mutation — the
aliases were severed into two objects. -/
theorem rpc_mutation_invisible :
    mutateThenObserve (rpcCall [[42]] [.ref 0, .ref 0]) = some [42] := rfl

/-! ## §8.6 — tag disjointness (the finite, `decide`-able row check)

Over the closed tag set of spec §5. `Ty` itself is infinite (recursive),
but the tag assignment depends ONLY on the head constructor, so
disjointness is a finite check over the head enumeration — the
`route()`-style row check §8.6 names. The full encode/decode/round-trip
theorems (§8.1–8.3, 8.5, 8.7) are v0.3 scope and deliberately NOT
attempted here.

Scope note (do not over-read): this proves tag-INJECTIVITY over the closed
set, plus closure. It does NOT claim untagged wire forms are shape-disjoint
— they are not (`int` and `float` both use `num`; `none` and `optional[T]`
both use `null`) and are disambiguated by the TYPE-directed decode
(`decode T w` takes `T`), not by shape. Tagged-vs-untagged confusability is
impossible at THIS model's level because `tagged` is a distinct `Wire`
constructor (§4); whether the JSON realization of `tagged` can collide with
a user `dict[str, …]` object is an encoder-representation decision below
this model, owned by the §10 binding when the boundary ships. -/

namespace Tags

/-- The head constructors of the crossable grammar (§3) — `dict` split per
its two §5 rows (string keys: plain `obj`; non-string keys: tagged pair
array) — plus the exception row (§5: `tagged("e", …)`). -/
inductive TyHead where
  | none | bool | int | float | str | bytes | decimal | datetime
  | list | set | tuple | dictStr | dictNonStr | option | union | record
  | exc
deriving DecidableEq, Repr

/-- The tag a head can emit, per the §5 conversion table (one row per
constructor). `Option.none` = the head only ever emits untagged wire forms. -/
def tagOf : TyHead → Option String
  | .int        => some "i"   -- big-int escape (decision D1)
  | .float      => some "f"   -- NaN / ±Inf / -0.0 escape (decision D2)
  | .bytes      => some "b"
  | .decimal    => some "d"
  | .datetime   => some "t"   -- decision D3
  | .tuple      => some "u"   -- decision D4 (untagged would collide with list)
  | .set        => some "s"
  | .dictNonStr => some "m"
  | .union      => some "v"   -- decision D5
  | .exc        => some "e"
  | _           => Option.none

/-- The closed tag set (§4: "Tag names are drawn from a fixed, closed set"). -/
def closedTagSet : List String :=
  ["i", "f", "b", "d", "t", "u", "s", "m", "v", "e"]

def allHeads : List TyHead :=
  [.none, .bool, .int, .float, .str, .bytes, .decimal, .datetime,
   .list, .set, .tuple, .dictStr, .dictNonStr, .option, .union, .record,
   .exc]

theorem allHeads_complete : ∀ h : TyHead, h ∈ allHeads := by
  intro h; cases h <;> simp [allHeads]

/-- The finite row check, as one computable Boolean: the tag map is
injective on tagged heads AND every emitted tag is in the closed set. -/
def tagsDisjointB : Bool :=
  (allHeads.all fun a => allHeads.all fun b =>
    !((tagOf a).isSome && (tagOf a == tagOf b)) || (a == b))
  && (allHeads.all fun a =>
    match tagOf a with
    | some t => closedTagSet.contains t
    | Option.none => true)

/-- §8.6, decided over the closed set. -/
theorem tags_disjoint_check : tagsDisjointB = true := by decide

/-- §8.6 as a usable Prop: no two constructor heads share a tag. -/
theorem tags_disjoint (a b : TyHead)
    (hs : (tagOf a).isSome) (he : tagOf a = tagOf b) : a = b := by
  cases a <;> cases b <;> simp_all [tagOf]

/-- Every emitted tag lies in the closed set (§4's "a tag outside that set
is a decode error" presupposes exactly this on the encode side). -/
theorem tags_closed (a : TyHead) (t : String) (ht : tagOf a = some t) :
    t ∈ closedTagSet := by
  cases a <;> simp_all [tagOf, closedTagSet]

/-! ### Stub-fails companion (the check CAN fail)

The same Boolean check, run over a deliberately WRONG table (tuple stealing
set's tag `"s"` — the exact collision D4 exists to prevent), computes
`false`. So `tags_disjoint_check` is not a check that passes everything. -/

def tagOfBroken : TyHead → Option String
  | .tuple => some "s"        -- WRONG: collides with set
  | h => tagOf h

def tagsDisjointBrokenB : Bool :=
  allHeads.all fun a => allHeads.all fun b =>
    !((tagOfBroken a).isSome && (tagOfBroken a == tagOfBroken b)) || (a == b)

theorem broken_table_fails : tagsDisjointBrokenB = false := by decide

end Tags

/-! ## Axiom footprint — pinned per declaration (build-gated) -/

/-- info: 'RpcBoundary.local_preserves_identity' depends on axioms: [propext] -/
#guard_msgs in #print axioms local_preserves_identity

/-- info: 'RpcBoundary.rpc_not_preserves_identity' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms rpc_not_preserves_identity

/-- info: 'RpcBoundary.rpc_preserves_value' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms rpc_preserves_value

/-- info: 'RpcBoundary.alias_iff_mutation_visible' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms alias_iff_mutation_visible

/-- info: 'RpcBoundary.local_mutation_visible' depends on axioms: [propext] -/
#guard_msgs in #print axioms local_mutation_visible

/-- info: 'RpcBoundary.rpc_mutation_invisible' depends on axioms: [propext] -/
#guard_msgs in #print axioms rpc_mutation_invisible

/-- info: 'RpcBoundary.Tags.tags_disjoint_check' depends on axioms: [propext] -/
#guard_msgs in #print axioms Tags.tags_disjoint_check

/-- info: 'RpcBoundary.Tags.tags_disjoint' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms Tags.tags_disjoint

/-- info: 'RpcBoundary.Tags.tags_closed' depends on axioms: [propext] -/
#guard_msgs in #print axioms Tags.tags_closed

/-- info: 'RpcBoundary.Tags.broken_table_fails' depends on axioms: [propext] -/
#guard_msgs in #print axioms Tags.broken_table_fails

end RpcBoundary
