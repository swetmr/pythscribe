/-
# Union7 — THE 7-WAY EXHAUSTIVE PRESERVATION UNION (C1 ∧ C2 ∧ C3 ∧ C4 ∧ C5 ∧ C6 ∧ C7)

The strongest union/join theorem for the semantic-preservation taxonomy
(the design notes §2/§3): all SEVEN components, conjoined EXPLICITLY,
exhaustively over the fragments the 20 preservation waves + the lattice-v2
sections establish (PythExpandVerify.lean), plus a NEW heap-carrying alias
fragment for C7 (within-side object identity — the one component the
value-level `PyValue`/`Val` models structurally CANNOT express, because they
quotient sharing away by design).

## Statement architecture (why FACES, not a bare conjunction of equations)

Each component `Ck` is a conjunction of per-fragment *conditional projections*
("faces") of the wave theorems, evaluated at a `World` — a record holding one
compiled-target function per fragment:

* **C1 (value)**       — both sides succeed ⇒ equal values (mod ρ on the arith
                         fragment: the documented D1 whole-float quotient).
* **C2 (type/repr)**   — both sides succeed ⇒ same Python type tag, MODULO the
                         documented D1 collapse on the arith fragment (`d1Tag`),
                         plus the exact i64-representation equation on WASM.
* **C3 (error-occ.)**  — error iff error (`isErr`/`isSome` agreement).
* **C4 (error-kind)**  — both sides err ⇒ same exception class.
* **C5 (effect order)** — identical effect traces (sync short-circuit + async
                         state-machine lowering; language-owned slice only).
* **C6 (termination)** — fuel-indexed definedness agreement on every fueled
                         fragment (the fuel model's termination face), with the
                         ∃-fuel co-termination corollary.
* **C7 (identity)**    — within-side aliasing preservation on the alias
                         fragment (reference-level, heap-carrying).

The conditional-face form is what makes every conjunct INDEPENDENTLY
FALSIFIABLE (the added-strength gate): for each k there is a separation world
`sepWk` — a `World` differing from `realWorld` in exactly ONE fragment target —
that FAILS `Ck` and SATISFIES the other six (`sep_C1` … `sep_C7` below). Hence
the 7-way union is strictly stronger than every 6-way sub-conjunction
(`sixway_*_not_sufficient`). The decomposition also loses nothing: the no-loss
lemmas (`okAgree_someAgree_iff_eq`, `pyResult_faces_iff_eq`,
`arith_faces_iff_d1_collapse`) prove the faces conjoined recover the strict
wave equations — including the EXACT shipped-D1 characterization on arith.

## Honesty ledger (read before citing this theorem)

* `realWorld` holds the SHIPPED lowerings: `d1CollapseAL` on arith (the real
  whole-float collapse, NOT the idealized tag-faithful `jsAL`), the i64 machine
  `evalW` on WASM, the emitted banker's rounding, the short-circuit strategy,
  the flattening async desugaring, and reference-copy assignment on the alias
  fragment. Nothing here claims tag-faithfulness the compiler does not deliver.
* **C7 is PROVED, not assumed — but at the same model tier as every wave**: a
  new heap-carrying fragment whose target models the emitted JS assignment
  (`y = x` copies the reference). It is NOT derivable from the existing wave
  models (their value domains quotient sharing away — that is WHY the fragment
  exists). The RPC-boundary C7 *deviation* (RpcBoundary.lean, v0.3) is NOT part
  of this compiler-side union; its unshare shape is used here only
  COUNTERFACTUALLY, as the separation world `sepW7`.
* **Exhaustiveness scope** — faces cover ALL 24 independent-target fragments:
  E, S, FE, FS, C, Cs, G, anyG, I, takewhileI, Wasm, D, Ds, Cls, ClsS, S11,
  Bw, Pow, Round (both the Int-level lowering and the expression fragment),
  Sort, Sm, Nb, Sg, D2, D2s, A, Sub, Sc, Ay — plus the new Al fragment.
  Honest exclusions, each stated where it bites:
  - the wave-0 seed `preservation` and the `poc_preservation` design PoC
    (model-level exemplars, superseded by the independent-target waves);
  - C3 on the FUELED fragments: their `Option` carrier conflates error with
    fuel exhaustion, so their occurrence content lives in C6's fuel-indexed
    face until a `PyResult`+fuel carrier exists (taxonomy: "C4 the gap");
  - C4 beyond the `PyResult` fragments (A, Sub, Sc) — inexpressible on the
    `Option` carriers by construction (that is the lattice-v2 point);
  - C5 host-owned scheduling (event loop, DOM, real Promise timing) — trust
    boundary by construction (`release_plan_v2.md` §10.2);
  - C6 genuine co-termination beyond the fuel model (needs step-indexing);
  - C7 on the value-level fragments (sharing is quotiented away there) and
    across the RPC boundary (documented deviation, not a compiler obligation);
  - WASM out-of-range values: C1 carries the range guard; the UNCONDITIONAL
    i64 wrap equation is C2's representation face (nothing hidden);
  - message TEXT of exceptions (kind only), dict iteration-order beyond
    literal order, non-whole floats on arith (stated fragment simplification).
* Every underlying wave theorem retains its own `*_stub_fails` litmus in
  PythExpandVerify.lean; the alias fragment gets its own (`preservationAl` /
  `preservationAl_cloneStub_fails`), and the union-level stubs are the seven
  separation worlds.
-/
import PythExpandVerify

namespace Union7

open PythExpandVerify

/- Decidable syntactic equality for the arith fragment — drives the surgical
   marker-based separation targets below. -/
deriving instance DecidableEq for AExp

/-! ## Generic face combinators (the projection algebra) -/

/-- C1 face on an `Option` carrier: if BOTH sides succeed, the values agree. -/
def okAgree {α : Type} (t r : Option α) : Prop :=
  ∀ x y, t = some x → r = some y → x = y

/-- C3/C6 face on an `Option` carrier: definedness agrees. -/
def someAgree {α : Type} (t r : Option α) : Prop := t.isSome = r.isSome

/-- C1 face on the `PyResult` carrier: if BOTH sides succeed, values agree. -/
def bothOkEq {α : Type} (t r : PyResult α) : Prop :=
  ∀ v w, t = .ok v → r = .ok w → v = w

/-- C3 face on the `PyResult` carrier: an error occurs iff one occurs. -/
def errAgree {α : Type} (t r : PyResult α) : Prop := t.isErr = r.isErr

/-- C4 face on the `PyResult` carrier: if BOTH sides err, the KINDS agree. -/
def bothErrKind {α : Type} (t r : PyResult α) : Prop :=
  ∀ e f, t = .err e → r = .err f → e = f

/-- C2 face on a tagged `Option` carrier: if BOTH sides succeed, the type
    tags agree (`tag` = the fragment's Python-type observation). -/
def okTagAgree {α β : Type} (tag : α → β) (t r : Option α) : Prop :=
  ∀ v w, t = some v → r = some w → tag v = tag w

theorem okAgree_of_eq {α : Type} {t r : Option α} (h : t = r) : okAgree t r := by
  intro x y hx hy
  rw [h] at hx; rw [hx] at hy; exact Option.some.inj hy

theorem someAgree_of_eq {α : Type} {t r : Option α} (h : t = r) : someAgree t r := by
  rw [someAgree, h]

theorem okTagAgree_of_eq {α β : Type} (tag : α → β) {t r : Option α}
    (h : t = r) : okTagAgree tag t r := by
  intro v w hv hw
  rw [h] at hv; rw [hv] at hw; rw [Option.some.inj hw]

theorem bothOkEq_of_eq {α : Type} {t r : PyResult α} (h : t = r) : bothOkEq t r := by
  intro v w hv hw
  rw [h] at hv; rw [hv] at hw; injection hw

theorem errAgree_of_eq {α : Type} {t r : PyResult α} (h : t = r) : errAgree t r := by
  rw [errAgree, h]

theorem bothErrKind_of_eq {α : Type} {t r : PyResult α} (h : t = r) : bothErrKind t r := by
  intro e f he hf
  rw [h] at he; rw [he] at hf; injection hf

/-- **No-loss (Option carriers).** The C1 face and the C3/C6 face conjoined
    recover the strict wave equation — the face decomposition weakens nothing. -/
theorem okAgree_someAgree_iff_eq {α : Type} (t r : Option α) :
    (okAgree t r ∧ someAgree t r) ↔ t = r := by
  constructor
  · rintro ⟨hok, hsome⟩
    cases t with
    | none =>
      cases r with
      | none => rfl
      | some y => simp [someAgree, Option.isSome] at hsome
    | some x =>
      cases r with
      | none => simp [someAgree, Option.isSome] at hsome
      | some y => rw [hok x y rfl rfl]
  · intro h; exact ⟨okAgree_of_eq h, someAgree_of_eq h⟩

/-- **No-loss (`PyResult` carriers).** Value + occurrence + kind faces conjoined
    recover the strict equation. -/
theorem pyResult_faces_iff_eq {α : Type} (t r : PyResult α) :
    (bothOkEq t r ∧ errAgree t r ∧ bothErrKind t r) ↔ t = r := by
  constructor
  · rintro ⟨hok, hocc, hkind⟩
    cases t with
    | ok v =>
      cases r with
      | ok w => rw [hok v w rfl rfl]
      | err f => simp [errAgree, PyResult.isErr] at hocc
    | err e =>
      cases r with
      | ok w => simp [errAgree, PyResult.isErr] at hocc
      | err f => rw [hkind e f rfl rfl]
  · intro h; exact ⟨bothOkEq_of_eq h, errAgree_of_eq h, bothErrKind_of_eq h⟩

/-! ## Arith-fragment faces (D1-honest: the SHIPPED collapse, not the ideal)

The taxonomy's D1 exception (whole float ≡ int) is a value-REPRESENTATION
fact: shipped PythScribe behaves as `d1CollapseAL`. The union therefore
evaluates the arith fragment at the SHIPPED lowering, states C1 mod ρ
(the D1 quotient), and states C2 as "tags preserved modulo EXACTLY the D1
collapse, nothing else" (`d1Tag`). A lowering that changed any other tag —
e.g. boxed every int as a float (`sepW2` below) — falsifies the C2 face. -/

/-- The D1 collapse ON TAGS: what the shipped representation does to a
    Python type tag (whole float → untagged JS number ≡ int). -/
def d1Tag : PTag → PTag
  | .tfloat => .tint
  | t => t

/-- C1 face on arith: both ok ⇒ values ρ-related (the D1 value quotient). -/
def arithOkRho (t r : PyResult PVal) : Prop :=
  ∀ v w, t = .ok v → r = .ok w → valRho v w = true

/-- C2 face on arith: both ok ⇒ the compiled tag is EXACTLY the D1 image of
    the Python tag. -/
def arithOkTagD1 (t r : PyResult PVal) : Prop :=
  ∀ v w, t = .ok v → r = .ok w → typeTag v = d1Tag (typeTag w)

theorem typeTag_d1c (v : PVal) : typeTag (d1c v) = d1Tag (typeTag v) := by
  cases v <;> rfl

/-- `isErr` is blind to `mapOk` (success re-representation never moves an
    error site). -/
theorem isErr_mapOk {α : Type} (f : α → α) (r : PyResult α) :
    (r.mapOk f).isErr = r.isErr := by
  cases r <;> rfl

/-- ρ is reflexive (used to derive ρ-faces from strict value equality). -/
theorem valRho_refl (v : PVal) : valRho v v = true := by
  cases v <;> simp [valRho]

/-- The four shipped-arith face lemmas, once — reused by the real world AND
    by the off-marker branches of the surgical separation stubs. -/
theorem arithC1_shipped :
    ∀ e env, arithOkRho (evalAtgt d1CollapseAL e env) (evalA e env) := by
  intro e env v w hv hw
  have h := preservationA_rho_holds_d1 e env
  rw [hv, hw] at h
  simpa [resultRho] using h

theorem arithC2_shipped :
    ∀ e env, arithOkTagD1 (evalAtgt d1CollapseAL e env) (evalA e env) := by
  intro e env v w hv hw
  rw [evalAtgt_d1_collapse, hw] at hv
  have : d1c w = v := by injection hv
  rw [← this, typeTag_d1c]

theorem arithC3_shipped :
    ∀ e env, errAgree (evalAtgt d1CollapseAL e env) (evalA e env) := by
  intro e env
  rw [errAgree, evalAtgt_d1_collapse, isErr_mapOk]

theorem arithC4_shipped :
    ∀ e env, bothErrKind (evalAtgt d1CollapseAL e env) (evalA e env) := by
  intro e env a b ha hb
  rw [evalAtgt_d1_collapse, hb] at ha
  injection ha with h
  exact h.symm

/-- **No-loss (arith, shipped).** The four faces conjoined recover EXACTLY the
    shipped-D1 characterization `evalAtgt d1CollapseAL = (evalA ·).mapOk d1c`
    (`evalAtgt_d1_collapse`) — pointwise: faces ⟺ `t = r.mapOk d1c`. So the
    D1-honest decomposition is not weaker than the strongest true statement
    about the shipped arith lowering. -/
theorem arith_faces_iff_d1_collapse (t r : PyResult PVal) :
    (arithOkRho t r ∧ arithOkTagD1 t r ∧ errAgree t r ∧ bothErrKind t r)
      ↔ t = r.mapOk d1c := by
  constructor
  · rintro ⟨hrho, htag, hocc, hkind⟩
    cases t with
    | ok v =>
      cases r with
      | ok w =>
        have hρ := hrho v w rfl rfl
        have hτ := htag v w rfl rfl
        cases w with
        | pint n =>
          cases v with
          | pint m => simp [valRho] at hρ; simp [PyResult.mapOk, d1c, hρ]
          | pfloat m => simp [typeTag, d1Tag] at hτ
          | pstr s => simp [typeTag, d1Tag] at hτ
        | pfloat n =>
          cases v with
          | pint m => simp [valRho] at hρ; simp [PyResult.mapOk, d1c, hρ]
          | pfloat m => simp [typeTag, d1Tag] at hτ
          | pstr s => simp [typeTag, d1Tag] at hτ
        | pstr s =>
          cases v with
          | pint m => simp [typeTag, d1Tag] at hτ
          | pfloat m => simp [typeTag, d1Tag] at hτ
          | pstr s' => simp [valRho] at hρ; simp [PyResult.mapOk, d1c, hρ]
      | err f => simp [errAgree, PyResult.isErr] at hocc
    | err e =>
      cases r with
      | ok w => simp [errAgree, PyResult.isErr] at hocc
      | err f => rw [hkind e f rfl rfl]; rfl
  · intro h
    subst h
    refine ⟨?_, ?_, ?_, ?_⟩
    · intro v w hv hw
      rw [hw] at hv
      have : d1c w = v := by injection hv
      rw [← this]
      cases w <;> simp [d1c, valRho]
    · intro v w hv hw
      rw [hw] at hv
      have : d1c w = v := by injection hv
      rw [← this, typeTag_d1c]
    · rw [errAgree, isErr_mapOk]
    · intro e f he hf
      rw [hf] at he
      injection he with h
      exact h.symm

/-! ## Python type-tag observations for the tagged value carriers (C2) -/

/-- Python type of a wave-4/5/6 `Val`. -/
def valPyType : Val → String
  | .vint _ => "int"
  | .vlist _ => "list"

/-- Python type of a wave-9 `DVal`. -/
def dvalPyType : DVal → String
  | .dint _ => "int"
  | .ddict _ => "dict"

/-- Python type of a wave-10 `OVal` — the CLASS NAME for instances, so the
    face pins class identity, not just "object". -/
def ovalPyType : OVal → String
  | .oint _ => "int"
  | .obj cls _ => cls

/-- Python type of a wave-11 `SVal`. -/
def svalPyType : SVal → String
  | .sstr _ => "str"
  | .sint _ => "int"

/-- Python type of a wave-15/19 `SgVal`. -/
def sgPyType : SgVal → String
  | .gbool _ => "bool"
  | .gint _ => "int"
  | .gstr _ => "str"
  | .glist _ => "list"

/-- Python type of a wave-20 `D2Res` (view results carry the CPython view
    types). -/
def d2PyType : D2Res → String
  | .rint _ => "int"
  | .rbool _ => "bool"
  | .rdict _ => "dict"
  | .rkeys _ => "dict_keys"
  | .rvals _ => "dict_values"
  | .ritems _ => "dict_items"

/-- Python type of a lattice `SubVal`. -/
def subPyType : SubVal → String
  | .sint _ => "int"
  | .slist _ => "list"
  | .sdict _ => "dict"

/-- C2 face on the `PyResult SubVal` carrier. -/
def subOkTag (t r : PyResult SubVal) : Prop :=
  ∀ v w, t = .ok v → r = .ok w → subPyType v = subPyType w

theorem subOkTag_of_eq {t r : PyResult SubVal} (h : t = r) : subOkTag t r := by
  intro v w hv hw
  rw [h] at hv; rw [hv] at hw
  have : v = w := by injection hw
  rw [this]

/-! ## The C7 alias fragment — heap, references, mutation (NEW)

C1–C6 live on value-level models that quotient sharing away by design
(the design notes §2, "Why C7 is separate"). Within-side aliasing is
therefore NOT provable from the existing wave models — a heap-carrying
fragment is required. This one is minimal-but-real: mutable list objects
(Python's canonical mutable value), environments binding names to scalars or
references, and the three operations that make aliasing OBSERVABLE:
allocation (`x = [..]`), assignment (`y = x` — CPython pass-by-object-
reference; the emitted JS is the reference-copying `y = x`), and in-place
mutation (`x[i] = v`). Same independent-target + falsifiable-lowering + stub
discipline as every wave: the lowering knob is what the emitted JS does with
the RHS of an assignment — the shipped `shareAL` copies the reference; the
wrong `cloneAL` deep-copies (`y = [...x]`), and is REFUTED. Heap encoding
mirrors RpcBoundary.lean's (whose `alias_iff_mutation_visible` grounds
address-aliasing in mutation observability for exactly this heap shape). -/

/-- Alias-fragment values: immutable ints, or references to heap objects. -/
inductive AlVal where
  | intv (n : Int)
  | ref (a : Nat)
  deriving DecidableEq, Repr

/-- One heap object: a mutable list of ints. Allocation appends. -/
abbrev AlObj := List Int
abbrev AlHeap := List AlObj

/-- Environment: name ↦ value bindings, first-match (cons shadows). -/
abbrev AlEnv := List (String × AlVal)

def AlEnv.get : AlEnv → String → Option AlVal
  | [], _ => none
  | (y, v) :: rest, x => if y = x then some v else AlEnv.get rest x

/-- A program state: environment + heap. -/
structure AlState where
  env : AlEnv
  heap : AlHeap
  deriving DecidableEq, Repr

/-- The alias fragment: allocation, assignment, in-place mutation, sequencing. -/
inductive AlStmt where
  | newList (x : String) (init : List Int)     -- x = [init…]
  | assignVar (y x : String)                   -- y = x
  | setIdx (x : String) (i : Nat) (v : Int)    -- x[i] = v
  | seq (a b : AlStmt)
  deriving Repr

/-- **Reference (CPython) semantics.** Assignment binds the SAME object
    (pass-by-object-reference); mutation writes through the reference; errors
    (`NameError` on an unbound name, `TypeError` on subscripting an int,
    `IndexError` out of range) collapse to `none` — this fragment's carrier is
    the `Option` monolith (kind is the lattice fragments' business). -/
def alRun : AlStmt → AlState → Option AlState
  | .newList x init, s => some ⟨(x, .ref s.heap.length) :: s.env, s.heap ++ [init]⟩
  | .assignVar y x, s => (AlEnv.get s.env x).map (fun v => ⟨(y, v) :: s.env, s.heap⟩)
  | .setIdx x i v, s =>
      match AlEnv.get s.env x with
      | some (.ref a) =>
          match s.heap[a]? with
          | some o => if i < o.length
                      then some ⟨s.env, s.heap.set a (o.set i v)⟩
                      else none
          | none => none
      | _ => none
  | .seq a b, s => (alRun a s).bind (alRun b)

/-- An alias LOWERING — the falsifiable parameter (F10): what the emitted
    code does with the RHS value of `y = x`. -/
structure AlLowering where
  assignRhs : AlHeap → AlVal → AlVal × AlHeap

/-- SHIPPED: the emitted JS `y = x` copies the REFERENCE (JS object
    references), exactly like CPython. -/
def shareAL : AlLowering := ⟨fun h v => (v, h)⟩

/-- WRONG lowering: `y = [...x]` — deep-copies on assignment. Plausible naive
    emission ("defensive copy"); breaks aliasing AND, after later mutation,
    values (see the `#guard` contrast below). -/
def cloneAL : AlLowering :=
  ⟨fun h v => match v with
    | .intv n => (.intv n, h)
    | .ref a => (.ref h.length, h ++ [(h[a]?).getD []])⟩

/-- **Independent target evaluator** — the compiled semantics under lowering
    `L`. A separate recursion; the assignment arm routes through the lowering. -/
def alRunTgt (L : AlLowering) : AlStmt → AlState → Option AlState
  | .newList x init, s => some ⟨(x, .ref s.heap.length) :: s.env, s.heap ++ [init]⟩
  | .assignVar y x, s => (AlEnv.get s.env x).map (fun v =>
      let p := L.assignRhs s.heap v
      ⟨(y, p.1) :: s.env, p.2⟩)
  | .setIdx x i v, s =>
      match AlEnv.get s.env x with
      | some (.ref a) =>
          match s.heap[a]? with
          | some o => if i < o.length
                      then some ⟨s.env, s.heap.set a (o.set i v)⟩
                      else none
          | none => none
      | _ => none
  | .seq a b, s => (alRunTgt L a s).bind (alRunTgt L b)

/-- The compiled alias-fragment semantics under the shipped lowering. -/
abbrev alRunJs : AlStmt → AlState → Option AlState := alRunTgt shareAL

/-- Preservation as a predicate over the lowering (stub-litmus discipline). -/
def AlPreserves (L : AlLowering) : Prop := ∀ p s, alRunTgt L p s = alRun p s

/-- **Alias-fragment preservation (strict).** The compiled reference-copy
    assignment computes the reference semantics on every program and state —
    environments, heaps, aliasing and all. -/
theorem preservationAl (p : AlStmt) (s : AlState) : alRunJs p s = alRun p s := by
  induction p generalizing s with
  | newList x init => rfl
  | assignVar y x => rfl
  | setIdx x i v => rfl
  | seq a b iha ihb =>
    simp only [alRunTgt, alRun, iha]
    cases alRun a s with
    | none => rfl
    | some s' => simp only [Option.bind_some]; exact ihb s'

theorem preservationAl_real : AlPreserves shareAL := preservationAl

/-- **Stub litmus (alias fragment).** The deep-copying lowering is REFUTED on
    `y = x` from a state where `x` holds a reference: the clone binds `y` to a
    FRESH object (address 1) where CPython binds the SAME object (address 0). -/
theorem preservationAl_cloneStub_fails : ¬ AlPreserves cloneAL := by
  intro h
  have hc := h (.assignVar "y" "x") ⟨[("x", .ref 0)], [[0]]⟩
  simp [alRunTgt, alRun, cloneAL, AlEnv.get] at hc

-- The clone stub is not just identity-wrong — after a later mutation it is
-- VALUE-wrong too (aliasing is value-observable through mutation; cf.
-- RpcBoundary.alias_iff_mutation_visible). Program: x=[0]; y=x; x[0]=7:
#guard (alRun (.seq (.newList "x" [0]) (.seq (.assignVar "y" "x") (.setIdx "x" 0 7)))
          ⟨[], []⟩) = some ⟨[("y", .ref 0), ("x", .ref 0)], [[7]]⟩
#guard (alRunTgt cloneAL
          (.seq (.newList "x" [0]) (.seq (.assignVar "y" "x") (.setIdx "x" 0 7)))
          ⟨[], []⟩) = some ⟨[("y", .ref 1), ("x", .ref 0)], [[7], [0]]⟩
-- (y denotes [7] under CPython/shipped, [0] under the clone stub.)

/-! ### C7 observations: denotation, aliasing, well-formedness -/

/-- What a name denotes: its scalar, or the CONTENT of its object (the
    C1-style, identity-blind observation). -/
inductive AlDeno where
  | dint (n : Int)
  | dlist (o : List Int)
  deriving DecidableEq, Repr

def alDenote (s : AlState) (x : String) : Option AlDeno :=
  (AlEnv.get s.env x).bind fun
    | .intv n => some (.dint n)
    | .ref a => (s.heap[a]?).map .dlist

/-- Names `x` and `y` ALIAS in state `s`: both bound to the SAME object.
    (Grounded observationally: for this heap shape, address equality ⟺
    mutation-through-one-is-visible-through-the-other —
    `RpcBoundary.alias_iff_mutation_visible`.) -/
def aliasedIn (s : AlState) (x y : String) : Prop :=
  ∃ a, AlEnv.get s.env x = some (.ref a) ∧ AlEnv.get s.env y = some (.ref a)

/-- Decidable aliasing checker (drives the `decide`-based refutations). -/
def aliasedInB (s : AlState) (x y : String) : Bool :=
  match AlEnv.get s.env x, AlEnv.get s.env y with
  | some (.ref a), some (.ref b) => a == b
  | _, _ => false

theorem aliasedIn_iff_B (s : AlState) (x y : String) :
    aliasedIn s x y ↔ aliasedInB s x y = true := by
  constructor
  · rintro ⟨a, hx, hy⟩
    simp [aliasedInB, hx, hy]
  · intro h
    unfold aliasedInB at h
    cases hx : AlEnv.get s.env x with
    | none => rw [hx] at h; simp at h
    | some v =>
      cases v with
      | intv n => rw [hx] at h; simp at h
      | ref a =>
        cases hy : AlEnv.get s.env y with
        | none => rw [hx, hy] at h; simp at h
        | some w =>
          cases w with
          | intv m => rw [hx, hy] at h; simp at h
          | ref b =>
            rw [hx, hy] at h
            simp at h
            exact ⟨a, hx, by rw [hy, h]⟩

/-- Well-formedness: every reachable reference is in heap range (dangling
    references are unreachable in real programs — same mandatory range
    precondition as RpcBoundary's `WF`; legitimated by the decidable checker
    + non-vacuity `#guard`s below). -/
def AlWF (s : AlState) : Prop :=
  ∀ x a, AlEnv.get s.env x = some (.ref a) → a < s.heap.length

/-- One binding is in `hp`'s range (references only; scalars vacuously). -/
def alBindingWF (hp : AlHeap) (p : String × AlVal) : Bool :=
  match p.2 with
  | .ref a => decide (a < hp.length)
  | .intv _ => true

/-- Decidable `AlWF` checker (checks ALL bindings — a superset of the
    reachable ones, so it is sound). -/
def alWFB (s : AlState) : Bool := s.env.all (alBindingWF s.heap)

theorem alWFB_go (hp : AlHeap) (env : AlEnv) (x : String) (a : Nat) :
    env.all (alBindingWF hp) = true →
    AlEnv.get env x = some (.ref a) → a < hp.length := by
  induction env with
  | nil => intro _ hget; simp [AlEnv.get] at hget
  | cons hd tl ih =>
    intro hall hget
    rw [List.all_cons] at hall
    have h1 := (Bool.and_eq_true _ _).mp hall
    unfold AlEnv.get at hget
    by_cases hxy : hd.1 = x
    · rw [if_pos hxy] at hget
      have hv : hd.2 = .ref a := by injection hget
      have := h1.1
      rw [alBindingWF, hv] at this
      simpa using this
    · rw [if_neg hxy] at hget
      exact ih h1.2 hget

theorem alWFB_sound (s : AlState) (hb : alWFB s = true) : AlWF s :=
  fun x a hx => alWFB_go s.heap s.env x a hb hx

-- Non-vacuity: the WF domain is inhabited (kernel-checked), including a
-- genuinely ALIASED well-formed state:
#guard alWFB ⟨[("x", .ref 0), ("y", .ref 0)], [[42]]⟩ = true
#guard aliasedInB ⟨[("x", .ref 0), ("y", .ref 0)], [[42]]⟩ "x" "y" = true

/-- Runs preserve well-formedness (the heap only grows or is written in
    place; every reference created is in range). -/
theorem alRun_WF : ∀ (p : AlStmt) (s s' : AlState),
    AlWF s → alRun p s = some s' → AlWF s' := by
  intro p
  induction p with
  | newList x init =>
    intro s s' hwf hrun
    have hs' : s' = ⟨(x, .ref s.heap.length) :: s.env, s.heap ++ [init]⟩ := by
      simp [alRun] at hrun; exact hrun.symm
    subst hs'
    intro z a hz
    unfold AlEnv.get at hz
    by_cases hxz : x = z
    · rw [if_pos hxz] at hz
      have : s.heap.length = a := by
        have := Option.some.inj hz; injection this
      simp [← this]
    · rw [if_neg hxz] at hz
      have := hwf z a hz
      simp; omega
  | assignVar y x =>
    intro s s' hwf hrun
    simp only [alRun] at hrun
    cases hg : AlEnv.get s.env x with
    | none => rw [hg] at hrun; simp at hrun
    | some v =>
      rw [hg] at hrun
      simp at hrun
      subst hrun
      intro z a hz
      unfold AlEnv.get at hz
      by_cases hyz : y = z
      · rw [if_pos hyz] at hz
        have hv : v = .ref a := by injection hz
        rw [hv] at hg
        exact hwf x a hg
      · rw [if_neg hyz] at hz
        exact hwf z a hz
  | setIdx x i v =>
    intro s s' hwf hrun
    simp only [alRun] at hrun
    cases hg : AlEnv.get s.env x with
    | none => simp only [hg] at hrun; exact absurd hrun (by simp)
    | some w =>
      cases w with
      | intv n => simp only [hg] at hrun; exact absurd hrun (by simp)
      | ref a =>
        simp only [hg] at hrun
        cases hh : s.heap[a]? with
        | none => simp only [hh] at hrun; exact absurd hrun (by simp)
        | some o =>
          simp only [hh] at hrun
          by_cases hi : i < o.length
          · rw [if_pos hi] at hrun
            have hs' : s' = ⟨s.env, s.heap.set a (o.set i v)⟩ := by
              injection hrun with h; exact h.symm
            subst hs'
            intro z b hz
            have := hwf z b hz
            simpa using this
          · rw [if_neg hi] at hrun; simp at hrun
  | seq a b iha ihb =>
    intro s s' hwf hrun
    simp only [alRun] at hrun
    cases hr : alRun a s with
    | none => rw [hr] at hrun; simp at hrun
    | some sm =>
      rw [hr] at hrun
      simp at hrun
      exact ihb sm s' (iha s sm hwf hr) hrun

/-! ### The unshare transform — the COUNTERFACTUAL C7-breaking world

`unshareState` reallocates every binding's object to a fresh copy: every
DENOTATION survives, every ALIAS is destroyed — the exact shape of the RPC
boundary crossing (`RpcBoundary.rpc_preserves_value` +
`rpc_not_preserves_identity`), used here purely as the separation world
`sepW7`: it satisfies C1–C6's faces and falsifies C7's. This is the formal
proof that C7 ADDS STRENGTH over value preservation. -/

def unshareEnv (h : AlHeap) : AlEnv → AlHeap → AlEnv × AlHeap
  | [], acc => ([], acc)
  | (x, .intv n) :: rest, acc =>
      let p := unshareEnv h rest acc
      ((x, .intv n) :: p.1, p.2)
  | (x, .ref a) :: rest, acc =>
      let p := unshareEnv h rest acc
      ((x, .ref p.2.length) :: p.1, p.2 ++ [(h[a]?).getD []])

def unshareState (s : AlState) : AlState :=
  ⟨(unshareEnv s.heap s.env s.heap).1, (unshareEnv s.heap s.env s.heap).2⟩

/-- The unshare world's alias-fragment target: run correctly, then deep-copy
    the final state (the RPC-crossing shape as a counterfactual lowering). -/
def unshareTgt : AlStmt → AlState → Option AlState :=
  fun p s => (alRun p s).map unshareState

/-- Reading past an append is stable (local mirror of
    `RpcBoundary.read_append_left`, over the same heap encoding). -/
theorem alRead_append_left {h : AlHeap} (ext : AlHeap) {a : Nat}
    (ha : a < h.length) : (h ++ ext)[a]? = h[a]? := by
  induction h generalizing a with
  | nil => cases ha
  | cons o t ih =>
    cases a with
    | zero => rfl
    | succ n => exact ih (Nat.lt_of_succ_lt_succ ha)

/-- Reading the just-appended cell (mirror of
    `RpcBoundary.read_append_length`). -/
theorem alRead_append_length (h : AlHeap) (o : AlObj) :
    (h ++ [o])[h.length]? = some o := by
  induction h with
  | nil => rfl
  | cons x t ih => exact ih

/-- `unshareEnv` transforms lookups pointwise: misses stay misses, scalars
    stay themselves, and a reference is rebound to a fresh IN-RANGE cell
    holding the ORIGINAL object's content. -/
theorem unshareEnv_get (h : AlHeap) (env : AlEnv) (x : String) :
    ∀ acc : AlHeap,
      (AlEnv.get env x = none → AlEnv.get (unshareEnv h env acc).1 x = none) ∧
      (∀ n, AlEnv.get env x = some (.intv n) →
        AlEnv.get (unshareEnv h env acc).1 x = some (.intv n)) ∧
      (∀ a, AlEnv.get env x = some (.ref a) →
        ∃ b, AlEnv.get (unshareEnv h env acc).1 x = some (.ref b) ∧
             b < (unshareEnv h env acc).2.length ∧
             ((unshareEnv h env acc).2)[b]? = some ((h[a]?).getD [])) := by
  induction env with
  | nil =>
    intro acc
    refine ⟨fun _ => rfl, fun n hn => by simp [AlEnv.get] at hn,
            fun a ha => by simp [AlEnv.get] at ha⟩
  | cons hd rest ih =>
    intro acc
    obtain ⟨y, v⟩ := hd
    cases v with
    | intv m =>
      by_cases hxy : y = x
      · refine ⟨fun hn => ?_, fun n hn => ?_, fun a ha => ?_⟩
        · unfold AlEnv.get at hn; rw [if_pos hxy] at hn; exact absurd hn (by simp)
        · unfold AlEnv.get at hn ⊢
          rw [if_pos hxy] at hn
          simp only [unshareEnv]
          rw [if_pos hxy]
          exact hn
        · unfold AlEnv.get at ha; rw [if_pos hxy] at ha
          exact absurd ha (by simp)
      · refine ⟨fun hn => ?_, fun n hn => ?_, fun a ha => ?_⟩
        · unfold AlEnv.get at hn; rw [if_neg hxy] at hn
          simp only [unshareEnv]
          unfold AlEnv.get; rw [if_neg hxy]
          exact (ih acc).1 hn
        · unfold AlEnv.get at hn; rw [if_neg hxy] at hn
          simp only [unshareEnv]
          unfold AlEnv.get; rw [if_neg hxy]
          exact (ih acc).2.1 n hn
        · unfold AlEnv.get at ha; rw [if_neg hxy] at ha
          simp only [unshareEnv]
          unfold AlEnv.get; rw [if_neg hxy]
          exact (ih acc).2.2 a ha
    | ref c =>
      by_cases hxy : y = x
      · refine ⟨fun hn => ?_, fun n hn => ?_, fun a ha => ?_⟩
        · unfold AlEnv.get at hn; rw [if_pos hxy] at hn; exact absurd hn (by simp)
        · unfold AlEnv.get at hn; rw [if_pos hxy] at hn; exact absurd hn (by simp)
        · unfold AlEnv.get at ha; rw [if_pos hxy] at ha
          have hca : c = a := by
            have := Option.some.inj ha; injection this
          subst hca
          refine ⟨(unshareEnv h rest acc).2.length, ?_, ?_, ?_⟩
          · simp only [unshareEnv]
            unfold AlEnv.get; rw [if_pos hxy]
          · simp only [unshareEnv]
            simp
          · simp only [unshareEnv]
            exact alRead_append_length _ _
      · refine ⟨fun hn => ?_, fun n hn => ?_, fun a ha => ?_⟩
        · unfold AlEnv.get at hn; rw [if_neg hxy] at hn
          simp only [unshareEnv]
          unfold AlEnv.get; rw [if_neg hxy]
          exact (ih acc).1 hn
        · unfold AlEnv.get at hn; rw [if_neg hxy] at hn
          simp only [unshareEnv]
          unfold AlEnv.get; rw [if_neg hxy]
          exact (ih acc).2.1 n hn
        · unfold AlEnv.get at ha; rw [if_neg hxy] at ha
          obtain ⟨b, hb1, hb2, hb3⟩ := (ih acc).2.2 a ha
          refine ⟨b, ?_, ?_, ?_⟩
          · simp only [unshareEnv]
            unfold AlEnv.get; rw [if_neg hxy]
            exact hb1
          · simp only [unshareEnv]
            simp; omega
          · simp only [unshareEnv]
            rw [alRead_append_left _ hb2]
            exact hb3

/-- **Unsharing preserves every denotation** (on well-formed states): the
    exact "value-faithful crossing" half of the RPC pair, proved for the
    counterfactual world. -/
theorem unshare_denote (s : AlState) (hwf : AlWF s) (x : String) :
    alDenote (unshareState s) x = alDenote s x := by
  unfold alDenote unshareState
  cases hg : AlEnv.get s.env x with
  | none =>
    rw [(unshareEnv_get s.heap s.env x s.heap).1 hg]
    rfl
  | some v =>
    cases v with
    | intv n =>
      rw [(unshareEnv_get s.heap s.env x s.heap).2.1 n hg]
      rfl
    | ref a =>
      obtain ⟨b, hb1, _, hb3⟩ := (unshareEnv_get s.heap s.env x s.heap).2.2 a hg
      rw [hb1]
      simp only [Option.bind_some, hb3]
      have ha := hwf x a hg
      have : s.heap[a]? = some s.heap[a] := List.getElem?_eq_getElem ha
      rw [this]
      rfl

/-! ## The World — one compiled target per fragment -/

/-- A world: an assignment of a compiled-target semantics to EVERY fragment
    the union ranges over. `realWorld` holds the shipped targets; the
    separation worlds perturb exactly one field. -/
structure World where
  tE     : Exp → Env → Option Int
  tS     : Stmt → Env → Option Env
  tFE    : FEnv → Nat → FExp → Env → Option Int
  tFS    : FEnv → Nat → FStmt → Env → Option FRes
  tC     : CExp → VEnv → Option Val
  tCs    : List CExp → VEnv → Option (List Val)
  tG     : Nat → GExp → VEnv → Option Val
  tAnyG  : Nat → GExp → String → List Val → VEnv → Option Bool
  tI     : Nat → IExp → VEnv → Option (List Val)
  tTakeI : Nat → GExp → String → List Val → VEnv → Option (List Val)
  tW     : WExp → Env → Option Int
  tD     : DExp → DEnv → Option DVal
  tDs    : List (Int × DExp) → DEnv → Option (List (Int × DVal))
  tCls   : ClsTable → Nat → CExp10 → OEnv → Option OVal
  tClsS  : ClsTable → Nat → CStmt10 → OEnv → Option ORes10
  tS11   : SExp → SEnv → Option SVal
  tBw    : BwExp → Env → Option Int
  tPow   : PowExp → Env → Option Int
  tRound : Int → Int
  tRoundE : RoundExp → Env → Option Int
  tSort  : SortExp → Option (List Int)
  tSm    : SmExp → SEnv → Option Int
  tNb    : NbExp → Env → Option Int
  tSg    : SgExp → SEnv → Option SgVal
  tD2    : D2Exp → D2Env → Option D2Res
  tD2s   : List (DKey × D2Exp) → D2Env → Option (List (DKey × Int))
  tA     : AExp → Env → PyResult PVal
  tSub   : SubExp → PyResult SubVal
  tSc    : ScExp → PyResult ScVal × List String
  tAy    : AyStmt → List String
  tAl    : AlStmt → AlState → Option AlState

/-- **The real world: the SHIPPED targets** — every wave's independent
    compiled evaluator under the shipped lowering, incl. the honest
    D1-collapsing arith lowering and the i64 WASM machine. -/
def realWorld : World where
  tE := evalEjs
  tS := evalSjs
  tFE := evalFEjs
  tFS := evalFSjs
  tC := evalCjs
  tCs := evalCsjs
  tG := evalGjs
  tAnyG := anyGjs
  tI := evalIjs
  tTakeI := takewhileIjs
  tW := evalW
  tD := evalDjs
  tDs := evalDsjs
  tCls := evalCls10js
  tClsS := evalClsS10js
  tS11 := evalS11js
  tBw := evalBwjs
  tPow := evalPowjs
  tRound := bankersTwin
  tRoundE := evalRound true
  tSort := evalSortjs
  tSm := evalSmjs
  tNb := evalNbjs
  tSg := evalSgjs
  tD2 := evalD2js
  tD2s := evalD2sjs
  tA := evalAtgt d1CollapseAL
  tSub := evalSubjs
  tSc := evalScjs
  tAy := ayLower flattenL
  tAl := alRunJs

/-! ## The seven components (face order within each: the perturbable
    fragments — A, FE, Ay, Al — come first, so separation proofs reuse the
    real proofs' tails by definitional equality) -/

/-- **C1 — value preservation**, exhaustively over every fragment: both sides
    succeed ⇒ same value (mod ρ on arith — the documented D1 quotient; guarded
    by i64-representability on WASM, whose unconditional representation
    equation is C2's face; denotation equality on the alias fragment). -/
def C1 (w : World) : Prop :=
  -- arith (PyResult carrier, D1-honest mod-ρ)
  (∀ e env, arithOkRho (w.tA e env) (evalA e env)) ∧
  -- functions, expressions (fueled)
  (∀ fenv fuel e env, okAgree (w.tFE fenv fuel e env) (evalFE fenv false fuel e env)) ∧
  -- alias fragment: denotations of the final state agree
  (∀ p s s₁ s₂, AlWF s → alRun p s = some s₁ → w.tAl p s = some s₂ →
      ∀ x, alDenote s₂ x = alDenote s₁ x) ∧
  -- the remaining fragments
  (∀ e env, okAgree (w.tE e env) (evalE false e env)) ∧
  (∀ s env, okAgree (w.tS s env) (evalS false s env)) ∧
  (∀ fenv fuel s env, okAgree (w.tFS fenv fuel s env) (evalFS fenv false fuel s env)) ∧
  (∀ e env, okAgree (w.tC e env) (evalC false e env)) ∧
  (∀ es env, okAgree (w.tCs es env) (evalCs false es env)) ∧
  (∀ fuel e env, okAgree (w.tG fuel e env) (evalG false fuel e env)) ∧
  (∀ fuel body v xs env, okAgree (w.tAnyG fuel body v xs env) (anyG false fuel body v xs env)) ∧
  (∀ fuel e env, okAgree (w.tI fuel e env) (evalI false fuel e env)) ∧
  (∀ fuel pred v xs env, okAgree (w.tTakeI fuel pred v xs env) (takewhileI false fuel pred v xs env)) ∧
  -- WASM: value equality whenever the Python value is i64-representable
  (∀ e env v, evalPyW e env = some v → -(2 ^ 63) ≤ v → v < 2 ^ 63 →
      w.tW e env = evalPyW e env) ∧
  (∀ e env, okAgree (w.tD e env) (evalD false e env)) ∧
  (∀ es env, okAgree (w.tDs es env) (evalDs false es env)) ∧
  (∀ ct fuel e env, okAgree (w.tCls ct fuel e env) (evalCls10 ct false fuel e env)) ∧
  (∀ ct fuel s env, okAgree (w.tClsS ct fuel s env) (evalClsS10 ct false fuel s env)) ∧
  (∀ e env, okAgree (w.tS11 e env) (evalS11 false e env)) ∧
  (∀ e env, okAgree (w.tBw e env) (evalBw false e env)) ∧
  (∀ e env, okAgree (w.tPow e env) (evalPow false e env)) ∧
  (∀ a, w.tRound a = pyRound a) ∧
  (∀ e env, okAgree (w.tRoundE e env) (evalRound false e env)) ∧
  (∀ e, okAgree (w.tSort e) (evalSort false e)) ∧
  (∀ e env, okAgree (w.tSm e env) (evalSm false e env)) ∧
  (∀ e env, okAgree (w.tNb e env) (evalNb false e env)) ∧
  (∀ e env, okAgree (w.tSg e env) (evalSg false e env)) ∧
  (∀ e env, okAgree (w.tD2 e env) (evalD2 false e env)) ∧
  (∀ es env, okAgree (w.tD2s es env) (evalD2s false es env)) ∧
  (∀ e, bothOkEq (w.tSub e) (evalSub e)) ∧
  (∀ e, bothOkEq (w.tSc e).1 (evalSc e).1)
  -- (Ay carries no values — effect-only fragment; its faces are C5's.)

/-- **C2 — type/representation preservation**, explicit even where C1
    subsumes it (representation matters): tags on every multi-tag PLAIN value
    carrier, the D1-collapsed tag law on arith (shipped-honest), and the EXACT
    unconditional i64-representation equation on WASM. Single-tag carriers
    (Int, Bool, trace) have no separate C2 content — subsumed, stated. The
    list/pair-LIFTED carriers (Cs, Ds, D2s, I, takeI, FS, ClsS) carry no
    separate face either: their elements are the plain carriers above, and
    their C1 faces are strict on the lifted values (tags included) — stated,
    not hidden. -/
def C2 (w : World) : Prop :=
  -- arith: the compiled tag is EXACTLY the D1 image of the Python tag
  (∀ e env, arithOkTagD1 (w.tA e env) (evalA e env)) ∧
  -- WASM: the compiled value IS the i64 representation of the Python value
  (∀ e env, w.tW e env = (evalPyW e env).map wrapI64) ∧
  (∀ e env, okTagAgree valPyType (w.tC e env) (evalC false e env)) ∧
  (∀ fuel e env, okTagAgree valPyType (w.tG fuel e env) (evalG false fuel e env)) ∧
  (∀ e env, okTagAgree dvalPyType (w.tD e env) (evalD false e env)) ∧
  (∀ ct fuel e env, okTagAgree ovalPyType (w.tCls ct fuel e env) (evalCls10 ct false fuel e env)) ∧
  (∀ e env, okTagAgree svalPyType (w.tS11 e env) (evalS11 false e env)) ∧
  (∀ e env, okTagAgree sgPyType (w.tSg e env) (evalSg false e env)) ∧
  (∀ e env, okTagAgree d2PyType (w.tD2 e env) (evalD2 false e env)) ∧
  (∀ e, subOkTag (w.tSub e) (evalSub e))

/-- **C3 — error-occurrence preservation** on every fragment whose `none`/
    `.err` means ERROR (the non-fueled carriers; on the FUELED fragments
    `none` conflates error with fuel exhaustion, so their occurrence content
    lives in C6's fuel-indexed face — stated conflation, see the header). -/
def C3 (w : World) : Prop :=
  (∀ e env, errAgree (w.tA e env) (evalA e env)) ∧
  (∀ p s, AlWF s → (w.tAl p s).isSome = (alRun p s).isSome) ∧
  (∀ e env, someAgree (w.tE e env) (evalE false e env)) ∧
  (∀ s env, someAgree (w.tS s env) (evalS false s env)) ∧
  (∀ e env, someAgree (w.tW e env) (evalPyW e env)) ∧
  (∀ e env, someAgree (w.tC e env) (evalC false e env)) ∧
  (∀ es env, someAgree (w.tCs es env) (evalCs false es env)) ∧
  (∀ e env, someAgree (w.tD e env) (evalD false e env)) ∧
  (∀ es env, someAgree (w.tDs es env) (evalDs false es env)) ∧
  (∀ e env, someAgree (w.tS11 e env) (evalS11 false e env)) ∧
  (∀ e env, someAgree (w.tBw e env) (evalBw false e env)) ∧
  (∀ e env, someAgree (w.tPow e env) (evalPow false e env)) ∧
  (∀ e env, someAgree (w.tRoundE e env) (evalRound false e env)) ∧
  (∀ e, someAgree (w.tSort e) (evalSort false e)) ∧
  (∀ e env, someAgree (w.tSm e env) (evalSm false e env)) ∧
  (∀ e env, someAgree (w.tNb e env) (evalNb false e env)) ∧
  (∀ e env, someAgree (w.tSg e env) (evalSg false e env)) ∧
  (∀ e env, someAgree (w.tD2 e env) (evalD2 false e env)) ∧
  (∀ es env, someAgree (w.tD2s es env) (evalD2s false es env)) ∧
  (∀ e, errAgree (w.tSub e) (evalSub e)) ∧
  (∀ e, errAgree (w.tSc e).1 (evalSc e).1)

/-- **C4 — error-KIND preservation** on every kind-carrying fragment (the
    `PyResult` carriers: arith, subscript, short-circuit). Inexpressible on
    the `Option` carriers BY CONSTRUCTION — closing that gap fragment-by-
    fragment is the lattice-v2 program; the union states C4 exactly where the
    carrier can state it, and nowhere pretends more. -/
def C4 (w : World) : Prop :=
  (∀ e env, bothErrKind (w.tA e env) (evalA e env)) ∧
  (∀ e, bothErrKind (w.tSub e) (evalSub e)) ∧
  (∀ e, bothErrKind (w.tSc e).1 (evalSc e).1)

/-- **C5 — effect/observation-order preservation** (the language-owned slice:
    synchronous short-circuit effect traces + the async → state-machine
    desugaring; host scheduling is a trust boundary by construction). -/
def C5 (w : World) : Prop :=
  (∀ s, w.tAy s = ayRun s) ∧
  (∀ e, (w.tSc e).2 = (evalSc e).2)

/-- **C6 — termination preservation** in the fuel model, on every fueled
    fragment: at EVERY fuel, definedness agrees (`someAgree`). Strictly
    stronger than ∃-fuel co-termination (derived below as
    `C6_coterminates_FE`); genuine co-termination beyond the fuel model
    (step-indexing) is stated future work, not claimed. -/
def C6 (w : World) : Prop :=
  (∀ fenv fuel e env, someAgree (w.tFE fenv fuel e env) (evalFE fenv false fuel e env)) ∧
  (∀ fenv fuel s env, someAgree (w.tFS fenv fuel s env) (evalFS fenv false fuel s env)) ∧
  (∀ fuel e env, someAgree (w.tG fuel e env) (evalG false fuel e env)) ∧
  (∀ fuel body v xs env, someAgree (w.tAnyG fuel body v xs env) (anyG false fuel body v xs env)) ∧
  (∀ fuel e env, someAgree (w.tI fuel e env) (evalI false fuel e env)) ∧
  (∀ fuel pred v xs env, someAgree (w.tTakeI fuel pred v xs env) (takewhileI false fuel pred v xs env)) ∧
  (∀ ct fuel e env, someAgree (w.tCls ct fuel e env) (evalCls10 ct false fuel e env)) ∧
  (∀ ct fuel s env, someAgree (w.tClsS ct fuel s env) (evalClsS10 ct false fuel s env))

/-- **C7 — object-identity / aliasing preservation, WITHIN a side**: on the
    heap-carrying alias fragment, two names alias in the compiled final state
    IFF they alias in the reference final state. Reference-level — the
    component the value models cannot state. (The RPC-boundary deviation is
    v0.3 scope and deliberately NOT part of this compiler-side union.) -/
def C7 (w : World) : Prop :=
  ∀ p s s₁ s₂, AlWF s → alRun p s = some s₁ → w.tAl p s = some s₂ →
    ∀ x y, (aliasedIn s₂ x y ↔ aliasedIn s₁ x y)

/-! ## The real world satisfies every component -/

theorem C1_real : C1 realWorld :=
  ⟨arithC1_shipped,
   fun fenv fuel e env => okAgree_of_eq (preservationFE fenv fuel e env),
   fun p s s₁ s₂ hwf hr ht x => by
     have : some s₂ = some s₁ := by rw [← ht, ← hr]; exact preservationAl p s
     rw [Option.some.inj this],
   fun e env => okAgree_of_eq (preservationE e env),
   fun s env => okAgree_of_eq (preservationS s env),
   fun fenv fuel s env => okAgree_of_eq (preservationFS fenv fuel s env),
   fun e env => okAgree_of_eq (preservationC e env),
   fun es env => okAgree_of_eq (preservationCs es env),
   fun fuel e env => okAgree_of_eq (preservationG fuel e env),
   fun fuel body v xs env => okAgree_of_eq (anyG_preservation fuel body v xs env),
   fun fuel e env => okAgree_of_eq (preservationI fuel e env),
   fun fuel pred v xs env => okAgree_of_eq (takewhileI_preservation fuel pred v xs env),
   fun e env v hv h1 h2 => preservationWasm_inRange e env v hv h1 h2,
   fun e env => okAgree_of_eq (preservationD e env),
   fun es env => okAgree_of_eq (preservationDs es env),
   fun ct fuel e env => okAgree_of_eq (preservationCls ct fuel e env),
   fun ct fuel s env => okAgree_of_eq (preservationClsS ct fuel s env),
   fun e env => okAgree_of_eq (preservationS11 e env),
   fun e env => okAgree_of_eq (preservationBw e env),
   fun e env => okAgree_of_eq (preservationPow e env),
   fun a => roundReal_preserves a,
   fun e env => okAgree_of_eq (preservationRound e env),
   fun e => okAgree_of_eq (preservationSort e),
   fun e env => okAgree_of_eq (preservationSm e env),
   fun e env => okAgree_of_eq (preservationNb e env),
   fun e env => okAgree_of_eq (preservationSg e env),
   fun e env => okAgree_of_eq (preservationD2 e env),
   fun es env => okAgree_of_eq (preservationD2s es env),
   fun e => bothOkEq_of_eq (preservationSub e),
   fun e => bothOkEq_of_eq (congrArg Prod.fst (preservationSc e))⟩

theorem C2_real : C2 realWorld :=
  ⟨arithC2_shipped,
   fun e env => preservationWasm e env,
   fun e env => okTagAgree_of_eq valPyType (preservationC e env),
   fun fuel e env => okTagAgree_of_eq valPyType (preservationG fuel e env),
   fun e env => okTagAgree_of_eq dvalPyType (preservationD e env),
   fun ct fuel e env => okTagAgree_of_eq ovalPyType (preservationCls ct fuel e env),
   fun e env => okTagAgree_of_eq svalPyType (preservationS11 e env),
   fun e env => okTagAgree_of_eq sgPyType (preservationSg e env),
   fun e env => okTagAgree_of_eq d2PyType (preservationD2 e env),
   fun e => subOkTag_of_eq (preservationSub e)⟩

theorem C3_real : C3 realWorld :=
  ⟨arithC3_shipped,
   fun p s _ => by rw [show realWorld.tAl p s = alRun p s from preservationAl p s],
   fun e env => someAgree_of_eq (preservationE e env),
   fun s env => someAgree_of_eq (preservationS s env),
   fun e env => by
     rw [someAgree, show realWorld.tW e env = (evalPyW e env).map wrapI64 from preservationWasm e env]
     cases evalPyW e env <;> rfl,
   fun e env => someAgree_of_eq (preservationC e env),
   fun es env => someAgree_of_eq (preservationCs es env),
   fun e env => someAgree_of_eq (preservationD e env),
   fun es env => someAgree_of_eq (preservationDs es env),
   fun e env => someAgree_of_eq (preservationS11 e env),
   fun e env => someAgree_of_eq (preservationBw e env),
   fun e env => someAgree_of_eq (preservationPow e env),
   fun e env => someAgree_of_eq (preservationRound e env),
   fun e => someAgree_of_eq (preservationSort e),
   fun e env => someAgree_of_eq (preservationSm e env),
   fun e env => someAgree_of_eq (preservationNb e env),
   fun e env => someAgree_of_eq (preservationSg e env),
   fun e env => someAgree_of_eq (preservationD2 e env),
   fun es env => someAgree_of_eq (preservationD2s es env),
   fun e => errAgree_of_eq (preservationSub e),
   fun e => errAgree_of_eq (congrArg Prod.fst (preservationSc e))⟩

theorem C4_real : C4 realWorld :=
  ⟨arithC4_shipped,
   fun e => bothErrKind_of_eq (preservationSub e),
   fun e => bothErrKind_of_eq (congrArg Prod.fst (preservationSc e))⟩

theorem C5_real : C5 realWorld :=
  ⟨fun s => preservationAy s,
   fun e => congrArg Prod.snd (preservationSc e)⟩

theorem C6_real : C6 realWorld :=
  ⟨fun fenv fuel e env => someAgree_of_eq (preservationFE fenv fuel e env),
   fun fenv fuel s env => someAgree_of_eq (preservationFS fenv fuel s env),
   fun fuel e env => someAgree_of_eq (preservationG fuel e env),
   fun fuel body v xs env => someAgree_of_eq (anyG_preservation fuel body v xs env),
   fun fuel e env => someAgree_of_eq (preservationI fuel e env),
   fun fuel pred v xs env => someAgree_of_eq (takewhileI_preservation fuel pred v xs env),
   fun ct fuel e env => someAgree_of_eq (preservationCls ct fuel e env),
   fun ct fuel s env => someAgree_of_eq (preservationClsS ct fuel s env)⟩

theorem C7_real : C7 realWorld := by
  intro p s s₁ s₂ hwf hr ht x y
  have : some s₂ = some s₁ := by
    rw [← ht, ← hr]; exact preservationAl p s
  rw [Option.some.inj this]

/-! ## THE 7-WAY EXHAUSTIVE UNION -/

/-- **THE UNION: C1 ∧ C2 ∧ C3 ∧ C4 ∧ C5 ∧ C6 ∧ C7, conjoined explicitly, at
    the shipped world, exhaustively over the wave/lattice fragments + the
    alias fragment.** Every conjunct is load-bearing: `sep_C1` … `sep_C7`
    below exhibit, for each component, a world satisfying the OTHER six and
    failing it — so this statement is strictly stronger than every 6-way
    sub-conjunction (`sixway_*_not_sufficient`). The face decomposition loses
    nothing (`okAgree_someAgree_iff_eq`, `pyResult_faces_iff_eq`,
    `arith_faces_iff_d1_collapse` + the recovery SPOTs below). Honesty ledger
    — including exactly what is NOT claimed — in the file header. -/
theorem union7 :
    C1 realWorld ∧ C2 realWorld ∧ C3 realWorld ∧ C4 realWorld ∧
    C5 realWorld ∧ C6 realWorld ∧ C7 realWorld :=
  ⟨C1_real, C2_real, C3_real, C4_real, C5_real, C6_real, C7_real⟩

/-- C6 corollary — ∃-fuel co-termination on the function fragment (the
    taxonomy's literal C6 face: `c` terminates iff `r` terminates, in the
    fuel model), derived from the stronger fuel-indexed face. -/
theorem C6_coterminates_FE (fenv : FEnv) (e : FExp) (env : Env) :
    (∃ n, (realWorld.tFE fenv n e env).isSome) ↔
    (∃ n, (evalFE fenv false n e env).isSome) := by
  constructor
  · rintro ⟨n, hn⟩
    exact ⟨n, by rw [← C6_real.1 fenv n e env]; exact hn⟩
  · rintro ⟨n, hn⟩
    exact ⟨n, by rw [C6_real.1 fenv n e env]; exact hn⟩

/-! ## Non-vacuity: every component's face domain is inhabited
    (kernel-checked witnesses — no face is satisfied for want of inputs) -/

-- C1: a both-ok arith instance with a REAL value (floor, not trunc):
#guard realWorld.tA (.fdiv (.lit (.pint (-7))) (.lit (.pint 2))) [] = .ok (.pint (-4))
-- C2: the D1 collapse is REAL on the shipped world (tag moves float→int),
-- and the C2 face's `d1Tag` law is exactly it:
#guard resultTag (realWorld.tA (.lit (.pfloat 2)) []) = some .tint
#guard resultTag (evalA (.lit (.pfloat 2)) []) = some .tfloat
-- C3/C4: a both-err instance, with the KIND carried:
#guard (realWorld.tA (.fdiv (.lit (.pint 1)) (.lit (.pint 0))) []).isErr = true
#guard realWorld.tA (.fdiv (.lit (.pint 1)) (.lit (.pint 0))) [] = .err .zeroDiv
-- C5: a nonempty trace in program order across a suspension point:
#guard realWorld.tAy (.seqA (.emit "a") (.seqA (.awaitV 0) (.emit "b"))) = ["a", "b"]
-- C6: fuel-sensitivity is real (none at 0, some at 1):
#guard (realWorld.tFE [] 0 (.lit 0) []).isSome = false
#guard (realWorld.tFE [] 1 (.lit 0) []).isSome = true
-- C7: a program whose FINAL state genuinely aliases (x=[0]; y=x):
#guard (alRun (.seq (.newList "x" [0]) (.assignVar "y" "x")) ⟨[], []⟩).map
        (fun s => aliasedInB s "x" "y") = some true

/-! ## SPOTs — client facts THROUGH the union (fail if a face is weakened) -/

/-- SPOT (C3 through the union): the compiled `"a" // 1` errs exactly because
    Python errs — derived from the C3 arith face, not by evaluating the
    target. -/
example : (realWorld.tA (.fdiv (.lit (.pstr "a")) (.lit (.pint 1))) []).isErr = true := by
  have h := union7.2.2.1.1 (.fdiv (.lit (.pstr "a")) (.lit (.pint 1))) []
  rw [errAgree] at h
  rw [h]; rfl

/-- SPOT (C7 through the union): aliasing survives compilation — derived from
    the C7 face + the SOURCE run, not by evaluating the compiled target. -/
example :
    ∀ s₂, realWorld.tAl (.seq (.newList "x" [0]) (.assignVar "y" "x")) ⟨[], []⟩ = some s₂ →
      aliasedIn s₂ "x" "y" := by
  intro s₂ ht
  have h := union7.2.2.2.2.2.2
      (.seq (.newList "x" [0]) (.assignVar "y" "x")) ⟨[], []⟩
      ⟨[("y", .ref 0), ("x", .ref 0)], [[0]]⟩ s₂
      (alWFB_sound _ (by decide)) (by decide) ht "x" "y"
  exact h.mpr ((aliasedIn_iff_B _ _ _).mpr (by decide))

/-- SPOT (no-loss recovery): the C1+C3 faces at the real world RECOVER the
    strict wave-1 equation — the decomposition did not weaken it. -/
example (e : Exp) (env : Env) : realWorld.tE e env = evalE false e env :=
  (okAgree_someAgree_iff_eq _ _).mp
    ⟨C1_real.2.2.2.1 e env, C3_real.2.2.1 e env⟩

/-- SPOT (no-loss recovery, arith): the four arith faces at the real world
    RECOVER the exact shipped-D1 characterization. -/
example (e : AExp) (env : Env) :
    realWorld.tA e env = (evalA e env).mapOk d1c :=
  (arith_faces_iff_d1_collapse _ _).mp
    ⟨C1_real.1 e env, C2_real.1 e env, C3_real.1 e env, C4_real.1 e env⟩

/-! ## THE SEPARATION WORLDS — each conjunct is load-bearing

For each component `Ck`, a world `sepWk` differing from `realWorld` in
exactly ONE fragment target that FAILS `Ck` and SATISFIES the other six.
The C1/C3/C4 stubs are surgical: they perturb the shipped arith target on a
single marker input, in a way that touches only the targeted projection.
The C2/C5/C7 worlds are recognizable wrong-compiler shapes (int-boxing;
state-machine reordering; unshare-on-return). Every claim is proved against
the SAME face predicates the union asserts. -/

/-! ### sep-world targets (surgical: perturb ONE projection at ONE marker) -/

/-- Marker for the C1 separation: a literal whose value the target corrupts
    (same tag). -/
def mC1 : AExp := .lit (.pint 999983)

/-- Marker for the C3 separation: a literal on which the target raises where
    Python succeeds. -/
def mC3 : AExp := .lit (.pint 271828)

/-- Marker for the C4 separation: `1 // 0` — an error site whose KIND the
    target corrupts (the site itself is untouched). -/
def mC4 : AExp := .fdiv (.lit (.pint 1)) (.lit (.pint 0))

/-- C1-separation target: on the marker return the SAME TAG, WRONG VALUE;
    elsewhere the shipped target. -/
def sepC1tgt (e : AExp) (env : Env) : PyResult PVal :=
  if e = mC1 then .ok (.pint 0) else evalAtgt d1CollapseAL e env

/-- The C2-separation representation: box EVERY int as a float —
    value-preserving mod ρ (the D1 clause is symmetric), error-preserving,
    tag-WRONG in the direction `d1Tag` forbids (int must stay int). A
    plausible naive "everything is a JS Number, so tag it float" emission. -/
def floatize : PVal → PVal
  | .pint n => .pfloat n
  | v => v

def sepC2tgt (e : AExp) (env : Env) : PyResult PVal :=
  (evalA e env).mapOk floatize

/-- C3-separation target: on the marker raise where Python succeeds
    (spurious error); elsewhere the shipped target. -/
def sepC3tgt (e : AExp) (env : Env) : PyResult PVal :=
  if e = mC3 then .err .typeError else evalAtgt d1CollapseAL e env

/-- C4-separation target: on the marker raise the WRONG KIND (`ValueError`
    where Python raises `ZeroDivisionError`); elsewhere the shipped target. -/
def sepC4tgt (e : AExp) (env : Env) : PyResult PVal :=
  if e = mC4 then .err .valueError else evalAtgt d1CollapseAL e env

/-- C6-separation marker (as a decidable recognizer — `FExp` has no derived
    `DecidableEq`, and none is needed: the positive faces only require that
    the then-branch is `none`). -/
def isM6 (fenv : FEnv) (e : FExp) (env : Env) : Bool :=
  fenv.isEmpty && env.isEmpty &&
    (match e with | .lit n => n == 0 | _ => false)

/-- C6-separation target: DIVERGE (`none` at EVERY fuel) on the marker input;
    elsewhere the shipped fueled target. Values wherever both sides terminate
    are untouched — only termination breaks. -/
def sepC6tgt (fenv : FEnv) (fuel : Nat) (e : FExp) (env : Env) : Option Int :=
  if isM6 fenv e env then none else evalFEjs fenv fuel e env

/-! ### the arith sep targets satisfy the NON-targeted arith faces -/

-- sepC1tgt: fails ρ at the marker; keeps tags, occurrence, kinds everywhere.
theorem sepC1tgt_tagD1 : ∀ e env, arithOkTagD1 (sepC1tgt e env) (evalA e env) := by
  intro e env v w hv hw
  unfold sepC1tgt at hv
  by_cases h : e = mC1
  · rw [if_pos h] at hv
    subst h
    have hv' : PVal.pint 0 = v := by injection hv
    have hw' : PVal.pint 999983 = w := by
      have : evalA mC1 env = .ok (.pint 999983) := rfl
      rw [this] at hw; injection hw
    rw [← hv', ← hw']; rfl
  · rw [if_neg h] at hv
    exact arithC2_shipped e env v w hv hw

theorem sepC1tgt_errAgree : ∀ e env, errAgree (sepC1tgt e env) (evalA e env) := by
  intro e env
  unfold sepC1tgt
  by_cases h : e = mC1
  · rw [if_pos h]
    subst h
    rfl
  · rw [if_neg h]
    exact arithC3_shipped e env

theorem sepC1tgt_errKind : ∀ e env, bothErrKind (sepC1tgt e env) (evalA e env) := by
  intro e env a b ha hb
  unfold sepC1tgt at ha
  by_cases h : e = mC1
  · rw [if_pos h] at ha; exact absurd ha (by simp)
  · rw [if_neg h] at ha
    exact arithC4_shipped e env a b ha hb

-- sepC2tgt: fails the tag law at `.lit (.pint 0)`; keeps ρ, occurrence, kinds.
theorem valRho_floatize (v : PVal) : valRho (floatize v) v = true := by
  cases v <;> simp [floatize, valRho]

theorem sepC2tgt_rho : ∀ e env, arithOkRho (sepC2tgt e env) (evalA e env) := by
  intro e env v w hv hw
  unfold sepC2tgt at hv
  rw [hw] at hv
  have : floatize w = v := by injection hv
  rw [← this]
  exact valRho_floatize w

theorem sepC2tgt_errAgree : ∀ e env, errAgree (sepC2tgt e env) (evalA e env) := by
  intro e env
  unfold sepC2tgt
  rw [errAgree, isErr_mapOk]

theorem sepC2tgt_errKind : ∀ e env, bothErrKind (sepC2tgt e env) (evalA e env) := by
  intro e env a b ha hb
  unfold sepC2tgt at ha
  rw [hb] at ha
  injection ha with h
  exact h.symm

-- sepC3tgt: fails occurrence at the marker; ρ/tag/kind faces hold (the marker
-- makes them VACUOUS there — both-ok / both-err never fires — and shipped
-- elsewhere).
theorem sepC3tgt_rho : ∀ e env, arithOkRho (sepC3tgt e env) (evalA e env) := by
  intro e env v w hv hw
  unfold sepC3tgt at hv
  by_cases h : e = mC3
  · rw [if_pos h] at hv; exact absurd hv (by simp)
  · rw [if_neg h] at hv
    exact arithC1_shipped e env v w hv hw

theorem sepC3tgt_tagD1 : ∀ e env, arithOkTagD1 (sepC3tgt e env) (evalA e env) := by
  intro e env v w hv hw
  unfold sepC3tgt at hv
  by_cases h : e = mC3
  · rw [if_pos h] at hv; exact absurd hv (by simp)
  · rw [if_neg h] at hv
    exact arithC2_shipped e env v w hv hw

theorem sepC3tgt_errKind : ∀ e env, bothErrKind (sepC3tgt e env) (evalA e env) := by
  intro e env a b ha hb
  by_cases h : e = mC3
  · subst h
    have : evalA mC3 env = .ok (.pint 271828) := rfl
    rw [this] at hb
    exact absurd hb (by simp)
  · unfold sepC3tgt at ha
    rw [if_neg h] at ha
    exact arithC4_shipped e env a b ha hb

-- sepC4tgt: fails the KIND at the marker; ρ/tag vacuous there, occurrence
-- untouched (both sides err at the marker).
theorem sepC4tgt_rho : ∀ e env, arithOkRho (sepC4tgt e env) (evalA e env) := by
  intro e env v w hv hw
  unfold sepC4tgt at hv
  by_cases h : e = mC4
  · rw [if_pos h] at hv; exact absurd hv (by simp)
  · rw [if_neg h] at hv
    exact arithC1_shipped e env v w hv hw

theorem sepC4tgt_tagD1 : ∀ e env, arithOkTagD1 (sepC4tgt e env) (evalA e env) := by
  intro e env v w hv hw
  unfold sepC4tgt at hv
  by_cases h : e = mC4
  · rw [if_pos h] at hv; exact absurd hv (by simp)
  · rw [if_neg h] at hv
    exact arithC2_shipped e env v w hv hw

theorem sepC4tgt_errAgree : ∀ e env, errAgree (sepC4tgt e env) (evalA e env) := by
  intro e env
  unfold sepC4tgt
  by_cases h : e = mC4
  · rw [if_pos h]
    subst h
    rfl
  · rw [if_neg h]
    exact arithC3_shipped e env

/-! ### the C6 sep target keeps values wherever it terminates -/

theorem sepC6tgt_okAgree :
    ∀ fenv fuel e env, okAgree (sepC6tgt fenv fuel e env) (evalFE fenv false fuel e env) := by
  intro fenv fuel e env x y hx hy
  unfold sepC6tgt at hx
  by_cases h : isM6 fenv e env = true
  · rw [if_pos h] at hx; exact absurd hx (by simp)
  · rw [if_neg h] at hx
    rw [preservationFE fenv fuel e env] at hx
    rw [hx] at hy
    injection hy

/-! ### the seven separation worlds -/

def sepW1 : World := { realWorld with tA := sepC1tgt }
def sepW2 : World := { realWorld with tA := sepC2tgt }
def sepW3 : World := { realWorld with tA := sepC3tgt }
def sepW4 : World := { realWorld with tA := sepC4tgt }
def sepW5 : World := { realWorld with tAy := ayLower reorderL }
def sepW6 : World := { realWorld with tFE := sepC6tgt }
def sepW7 : World := { realWorld with tAl := unshareTgt }

/-! ### the seven separations — each component is load-bearing -/

/-- **C1 adds strength**: a world that keeps every tag, every error site,
    every kind, every trace, every termination behavior and every alias —
    and still computes a WRONG VALUE. Only C1 sees it. -/
theorem sep_C1 :
    ¬ C1 sepW1 ∧ C2 sepW1 ∧ C3 sepW1 ∧ C4 sepW1 ∧ C5 sepW1 ∧ C6 sepW1 ∧ C7 sepW1 := by
  refine ⟨?_, ⟨sepC1tgt_tagD1, C2_real.2⟩, ⟨sepC1tgt_errAgree, C3_real.2⟩,
          ⟨sepC1tgt_errKind, C4_real.2⟩, C5_real, C6_real, C7_real⟩
  intro h
  have hc := h.1 mC1 [] (.pint 0) (.pint 999983)
      (by simp [sepW1, sepC1tgt]) rfl
  exact absurd hc (by decide)

/-- **C2 adds strength**: a world that is value-correct mod ρ (the honest D1
    quotient), error-correct, kind-correct — and re-tags every int as a
    float. Only C2 (the `d1Tag` law) sees it: this is exactly the projection
    that separates "right value on the wire" from "right Python TYPE". -/
theorem sep_C2 :
    C1 sepW2 ∧ ¬ C2 sepW2 ∧ C3 sepW2 ∧ C4 sepW2 ∧ C5 sepW2 ∧ C6 sepW2 ∧ C7 sepW2 := by
  refine ⟨⟨sepC2tgt_rho, C1_real.2⟩, ?_, ⟨sepC2tgt_errAgree, C3_real.2⟩,
          ⟨sepC2tgt_errKind, C4_real.2⟩, C5_real, C6_real, C7_real⟩
  intro h
  have hc := h.1 (.lit (.pint 0)) [] (.pfloat 0) (.pint 0) rfl rfl
  exact absurd hc (by decide)

/-- **C3 adds strength**: a world that is value/tag/kind-correct wherever the
    projections fire — and raises where Python succeeds. Only C3 sees the
    spurious error (the conditional C1/C2/C4 faces are vacuous at it). -/
theorem sep_C3 :
    C1 sepW3 ∧ C2 sepW3 ∧ ¬ C3 sepW3 ∧ C4 sepW3 ∧ C5 sepW3 ∧ C6 sepW3 ∧ C7 sepW3 := by
  refine ⟨⟨sepC3tgt_rho, C1_real.2⟩, ⟨sepC3tgt_tagD1, C2_real.2⟩, ?_,
          ⟨sepC3tgt_errKind, C4_real.2⟩, C5_real, C6_real, C7_real⟩
  intro h
  have hc := h.1 mC3 []
  rw [errAgree, show sepW3.tA mC3 [] = .err .typeError from by simp [sepW3, sepC3tgt],
      show evalA mC3 [] = .ok (.pint 271828) from rfl] at hc
  exact absurd hc (by decide)

/-- **C4 adds strength**: a world that errs EXACTLY where Python errs, with
    every value and tag right — and raises the wrong CLASS at one site. Only
    C4 sees it (this is the projection the `Option` monolith provably cannot
    express — `eraseKind_blind_to_zeroKind`). -/
theorem sep_C4 :
    C1 sepW4 ∧ C2 sepW4 ∧ C3 sepW4 ∧ ¬ C4 sepW4 ∧ C5 sepW4 ∧ C6 sepW4 ∧ C7 sepW4 := by
  refine ⟨⟨sepC4tgt_rho, C1_real.2⟩, ⟨sepC4tgt_tagD1, C2_real.2⟩,
          ⟨sepC4tgt_errAgree, C3_real.2⟩, ?_, C5_real, C6_real, C7_real⟩
  intro h
  have hc := h.1 mC4 [] .valueError .zeroDiv
      (by simp [sepW4, sepC4tgt]) rfl
  exact absurd hc (by decide)

/-- **C5 adds strength**: a world that computes every value, error, kind,
    tag, termination and alias correctly — and emits this body's effects in
    the WRONG ORDER across a suspension point (the reordering desugaring).
    Only C5 sees a pure order divergence. -/
theorem sep_C5 :
    C1 sepW5 ∧ C2 sepW5 ∧ C3 sepW5 ∧ C4 sepW5 ∧ ¬ C5 sepW5 ∧ C6 sepW5 ∧ C7 sepW5 := by
  refine ⟨C1_real, C2_real, C3_real, C4_real, ?_, C6_real, C7_real⟩
  intro h
  exact preservationAy_reorderStub_fails h.1

/-- **C6 adds strength**: a world that agrees on every value WHENEVER both
    sides terminate (the conditional C1 face is blind to divergence by
    construction) — and diverges on an input where Python terminates. Only
    C6's fuel-indexed face sees it. -/
theorem sep_C6 :
    C1 sepW6 ∧ C2 sepW6 ∧ C3 sepW6 ∧ C4 sepW6 ∧ C5 sepW6 ∧ ¬ C6 sepW6 ∧ C7 sepW6 := by
  refine ⟨⟨C1_real.1, sepC6tgt_okAgree, C1_real.2.2⟩, C2_real, C3_real, C4_real,
          C5_real, ?_, C7_real⟩
  intro h
  have hc := h.1 [] 1 (.lit 0) []
  rw [someAgree, show sepW6.tFE [] 1 (.lit 0) [] = none from by simp [sepW6, sepC6tgt, isM6]] at hc
  have href : (evalFE [] false 1 (.lit 0) []).isSome = true := by
    simp [evalFE]
  rw [href] at hc
  exact absurd hc (by decide)

/-- **C7 adds strength — the crown separation**: a world that preserves EVERY
    denotation (proved: `unshare_denote`), every error occurrence, and every
    other component's faces — and destroys ALL sharing in the final state
    (the unshare/RPC-crossing shape as a counterfactual within-side
    lowering). Value preservation provably does NOT imply identity
    preservation; only C7 sees unsharing. -/
theorem sep_C7 :
    C1 sepW7 ∧ C2 sepW7 ∧ C3 sepW7 ∧ C4 sepW7 ∧ C5 sepW7 ∧ C6 sepW7 ∧ ¬ C7 sepW7 := by
  refine ⟨⟨C1_real.1, C1_real.2.1, ?_, C1_real.2.2.2⟩, C2_real,
          ⟨C3_real.1, ?_, C3_real.2.2⟩, C4_real, C5_real, C6_real, ?_⟩
  · -- C1 on the alias fragment: unsharing preserves every denotation
    intro p s s₁ s₂ hwf hr ht x
    have ht' : (alRun p s).map unshareState = some s₂ := ht
    rw [hr] at ht'
    have hs₂ : unshareState s₁ = s₂ := by
      simpa using ht'
    rw [← hs₂]
    exact unshare_denote s₁ (alRun_WF p s s₁ hwf hr) x
  · -- C3 on the alias fragment: mapping preserves definedness
    intro p s _
    show ((alRun p s).map unshareState).isSome = (alRun p s).isSome
    cases alRun p s <;> rfl
  · -- ¬C7: the final states of x=[0]; y=x alias in the reference and are
    -- UNSHARED by the world
    intro h
    have hs := h (.seq (.newList "x" [0]) (.assignVar "y" "x")) ⟨[], []⟩
        ⟨[("y", .ref 0), ("x", .ref 0)], [[0]]⟩
        ⟨[("y", .ref 2), ("x", .ref 1)], [[0], [0], [0]]⟩
        (alWFB_sound _ (by decide)) (by decide) (by decide) "x" "y"
    have h1 : aliasedIn ⟨[("y", .ref 0), ("x", .ref 0)], [[0]]⟩ "x" "y" :=
      (aliasedIn_iff_B _ _ _).mpr (by decide)
    have h2 := (aliasedIn_iff_B
        ⟨[("y", .ref 2), ("x", .ref 1)], [[0], [0], [0]]⟩ "x" "y").mp (hs.mpr h1)
    exact absurd h2 (by decide)

/-! ### strictly stronger than every 6-way sub-conjunction -/

theorem sixway_no_C1_not_sufficient :
    ¬ (∀ w : World, C2 w ∧ C3 w ∧ C4 w ∧ C5 w ∧ C6 w ∧ C7 w → C1 w) :=
  fun h => sep_C1.1 (h sepW1
    ⟨sep_C1.2.1, sep_C1.2.2.1, sep_C1.2.2.2.1, sep_C1.2.2.2.2.1,
     sep_C1.2.2.2.2.2.1, sep_C1.2.2.2.2.2.2⟩)

theorem sixway_no_C2_not_sufficient :
    ¬ (∀ w : World, C1 w ∧ C3 w ∧ C4 w ∧ C5 w ∧ C6 w ∧ C7 w → C2 w) :=
  fun h => sep_C2.2.1 (h sepW2
    ⟨sep_C2.1, sep_C2.2.2.1, sep_C2.2.2.2.1, sep_C2.2.2.2.2.1,
     sep_C2.2.2.2.2.2.1, sep_C2.2.2.2.2.2.2⟩)

theorem sixway_no_C3_not_sufficient :
    ¬ (∀ w : World, C1 w ∧ C2 w ∧ C4 w ∧ C5 w ∧ C6 w ∧ C7 w → C3 w) :=
  fun h => sep_C3.2.2.1 (h sepW3
    ⟨sep_C3.1, sep_C3.2.1, sep_C3.2.2.2.1, sep_C3.2.2.2.2.1,
     sep_C3.2.2.2.2.2.1, sep_C3.2.2.2.2.2.2⟩)

theorem sixway_no_C4_not_sufficient :
    ¬ (∀ w : World, C1 w ∧ C2 w ∧ C3 w ∧ C5 w ∧ C6 w ∧ C7 w → C4 w) :=
  fun h => sep_C4.2.2.2.1 (h sepW4
    ⟨sep_C4.1, sep_C4.2.1, sep_C4.2.2.1, sep_C4.2.2.2.2.1,
     sep_C4.2.2.2.2.2.1, sep_C4.2.2.2.2.2.2⟩)

theorem sixway_no_C5_not_sufficient :
    ¬ (∀ w : World, C1 w ∧ C2 w ∧ C3 w ∧ C4 w ∧ C6 w ∧ C7 w → C5 w) :=
  fun h => sep_C5.2.2.2.2.1 (h sepW5
    ⟨sep_C5.1, sep_C5.2.1, sep_C5.2.2.1, sep_C5.2.2.2.1,
     sep_C5.2.2.2.2.2.1, sep_C5.2.2.2.2.2.2⟩)

theorem sixway_no_C6_not_sufficient :
    ¬ (∀ w : World, C1 w ∧ C2 w ∧ C3 w ∧ C4 w ∧ C5 w ∧ C7 w → C6 w) :=
  fun h => sep_C6.2.2.2.2.2.1 (h sepW6
    ⟨sep_C6.1, sep_C6.2.1, sep_C6.2.2.1, sep_C6.2.2.2.1,
     sep_C6.2.2.2.2.1, sep_C6.2.2.2.2.2.2⟩)

theorem sixway_no_C7_not_sufficient :
    ¬ (∀ w : World, C1 w ∧ C2 w ∧ C3 w ∧ C4 w ∧ C5 w ∧ C6 w → C7 w) :=
  fun h => sep_C7.2.2.2.2.2.2 (h sepW7
    ⟨sep_C7.1, sep_C7.2.1, sep_C7.2.2.1, sep_C7.2.2.2.1,
     sep_C7.2.2.2.2.1, sep_C7.2.2.2.2.2.1⟩)

/-! ## Axiom footprint — per-declaration pins (Stage-5 gate; captured from a
    real build; the build FAILS if any footprint grows) -/

/-- info: 'Union7.union7' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms union7

/-- info: 'Union7.sep_C1' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms sep_C1

/-- info: 'Union7.sep_C2' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms sep_C2

/-- info: 'Union7.sep_C3' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms sep_C3

/-- info: 'Union7.sep_C4' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms sep_C4

/-- info: 'Union7.sep_C5' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms sep_C5

/-- info: 'Union7.sep_C6' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms sep_C6

/-- info: 'Union7.sep_C7' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms sep_C7

/-- info: 'Union7.preservationAl' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationAl

/-- info: 'Union7.preservationAl_cloneStub_fails' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms preservationAl_cloneStub_fails

/-- info: 'Union7.unshare_denote' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms unshare_denote

/-- info: 'Union7.alRun_WF' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms alRun_WF

/-- info: 'Union7.C6_coterminates_FE' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms C6_coterminates_FE

/-- info: 'Union7.arith_faces_iff_d1_collapse' depends on axioms: [propext] -/
#guard_msgs in
#print axioms arith_faces_iff_d1_collapse

/-- info: 'Union7.sixway_no_C1_not_sufficient' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms sixway_no_C1_not_sufficient

/-- info: 'Union7.sixway_no_C7_not_sufficient' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms sixway_no_C7_not_sufficient

end Union7
