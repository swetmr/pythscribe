/-
# InternalConsistency — compiler-internal coherence invariants

Formalizes two INTERNAL-consistency invariants of the PythScribe compiler —
meta-invariants about the compiler being internally coherent, each stated so
that its violation IS a real bug this project shipped. Distinct from (and
complementary to) the language-preservation waves (PythExpandVerify) and the
router-admissibility work on `proof/routing-soundness` — nothing here claims
routing soundness or CPython preservation.

* **Part 1 — Runtime two-copy parity (WB-20 family).** `pyths run/bundle/test`
  embed an INLINE runtime; `pyths compile` imports the PACKAGE runtime
  (`runtime/src/*.js`). Fixes repeatedly landed in ONE copy only. The
  discriminating real bug: commit `1b28bae5` fixed `pyDelItem` error kinds in
  the package copy only — the stale inline copy shipped
  IndexError-for-TypeError and a SILENT tuple splice (bundled apps mutated
  "immutable" tuples). We state "all copies of a helper denote the same
  function", prove the single-source-by-extraction configuration satisfies it
  BY CONSTRUCTION, REFUTE the historical drifted configuration on the exact
  pyDelItem witness, and prove the parity gate's behavioral battery
  (`crates/pyths_codegen_js/tests/inline_runtime_parity.rs`, face B) rejects
  any corpus-witnessed drift — "the proof would catch the drift".

* **Part 2 — Container-method dispatch soundness + determinism (F2 family).**
  The F2 bug (#446): `form.append("file", file)` on a foreign `FormData` was
  lowered as 1-arg Python `list.append`, SILENTLY DROPPING the second
  argument (the method-name-collision / Tier-D interop class; WB-3 `.sort`
  comparator-drop and WB-10 `.replace` regex/callback are the same family).
  The root fix (dev-main `87d4eaaa` + `41a42aa0`) is receiver-context + arity
  dispatch. We transliterate the shipped decision procedure, prove the
  arity-soundness invariant (a container lowering is NEVER kept at an arity
  impossible for the Python method unless the receiver is provably that
  container), refute the PRE-fix name-only dispatch on the exact FormData
  witness, and prove the pre-fix rule system is genuinely AMBIGUOUS (two
  rules fire on one call site with contradictory outputs) while the shipped
  cascade fires exactly one rule per call site.

## Statement-quality manifest (lean-spec-quality gates applied)

* **Shape (C1 pattern).** Each invariant is ONE predicate (`ConsistentAll`,
  `AritySound` / functionality of `Fires`), instantiated positively by the
  shipped configuration and REFUTED by the real historical bug configuration.
  The refutations are the discriminating witnesses — the exact drift/collision
  that shipped — not strawmen.
* **F1 honesty.** `extraction_consistent` / `shipped_single_source_consistent`
  are by-construction (near-`rfl`) ON PURPOSE: single-sourcing makes parity
  DEFINITIONAL — that is the design claim. The theorem content lives in
  `two_copy_drift_refuted`, `battery_catches_witnessed_drift` (a universal:
  ANY corpus-witnessed drift is rejected), and the two false-world controls.
* **False-world controls (both REAL historical failure modes, not strawmen).**
  (1) `happy_path_battery_misses_drift`: the pre-blocker-2 battery covered
  only happy paths, and the drifted pair PASSES it — the gate's
  battery-coverage rule is load-bearing. (2) `kind_blind_misses_kind_drift`:
  a comparator that checks only ok-vs-err (not the error KIND) stays green
  over the real IndexError-vs-TypeError drift — the gate's error-NAME
  serialization is load-bearing. In both false worlds the drift survives;
  under the real gate (`gate_rejects_pyDelItem_drift`) it does not.
* **Discriminating witnesses.** Part 1: `(tuple [1,2,3], 0)` — fixed copy
  TypeError vs stale copy SILENT SPLICE (outcomes differ in ok-vs-err); and
  `(tuple [], 0)` — TypeError vs IndexError (outcomes differ in error KIND;
  the C4 axis of the design notes). Part 2:
  `⟨"append", argc 2, foreign⟩` — 2 positional args are impossible for
  Python `list.append` (arity exactly 1), so the shipped dispatch diverts to
  verbatim (every argument preserved) while the pre-fix dispatch lowers (the
  arg-drop). Positive selections (`append` on a provable list, keyword-only
  `sort`, plain `replace`) show the discriminators SELECT rather than
  blanket-verbatim — the witness discriminates in both directions.
* **Anti-vacuity.** The battery is proven non-trivial in both directions
  (rejects the drifted pair, green on the single-source pair over the SAME
  corpus). The arity table is machine-checked non-degenerate (every rule
  accepts its own `min`; nonempty container list; NO DUPLICATE NAMES — the
  table itself is deterministic). `Decision`/`Outcome` inequalities are by
  constructor, so no refutation is an artifact of a degenerate model.
* **SPOTs (proved THROUGH the universal theorems, not by evaluation).**
  `gate_rejects_pyDelItem_drift` is discharged via
  `battery_catches_witnessed_drift`; `f2_witness_diverted` is discharged via
  `aritySound_dispatch` (contraposition over `Decision`) — if either
  universal were silently weakened, these SPOTs would not close.

## Binding status (HONEST — read before citing)

These are MODEL-LEVEL theorems about transliterated compiler internals:

* Part 1 transliterates `runtime/src/runtime.js::pyDelItem` (pythscribe
  `main` @ `b4ffda22`, post-`1b28bae5` fixed shape) restricted to
  int-keyed list/tuple/dict receivers, and the stale pre-fix inline shape
  (no tuple branch — a tuple IS a JS array, so `Array.isArray` swallowed it).
  Out of modeled domain: string keys and error MESSAGE text. (Live finding,
  recorded for the caller: on the `proof/cat-c-findings` worktree base the
  two literal copies still differ in KeyError message formatting — package
  quotes string keys, inline does not; on `main` the A1 shim has removed the
  second copy, mooting it.)
* Part 2 transliterates `method_table.rs::container_method_arity` and
  `emit.rs::container_dispatch_prefers_verbatim` (`main` @ `b4ffda22`),
  decision order preserved. Receiver inference (`infer_type` /
  `is_foreign_receiver`) is abstracted to a 3-way context
  (provably-container / provably-foreign / unknown); the WB-10 argument
  signal (`RegExp` first arg / function second arg) is abstracted to a Bool.
* UNLIKE `DictData`/`KwargData`, these transliterations are NOT (yet)
  regenerated + CI-diffed against the Rust source — model↔Rust binding is by
  hand-transliteration with the provenance pinned above. That is a
  documented trust boundary (the F6 lesson): recommend a
  `gen-dispatch-data.py`-style drift gate as follow-up before citing these
  as shipping-bound. Record in TRUST.md at the MODEL tier.
-/

namespace InternalConsistency

/-! ## Part 1 — Runtime two-copy parity (WB-20 / the pyDelItem drift) -/

namespace CopyParity

/-- Python error KINDS observable from `pyDelItem` on the modeled domain
(the C4 error-kind axis: the drift changed the KIND, not just a message). -/
inductive ErrKind
  | typeError
  | indexError
  | keyError
deriving DecidableEq, Repr

/-- Modeled receiver domain: int lists, int tuples, int→int dicts. Enough to
express both real drift witnesses (silent tuple splice; TypeError vs
IndexError). String keys / message text are out of modeled domain (header). -/
inductive PyVal
  | list  (xs : List Int)
  | tuple (xs : List Int)
  | dict  (kvs : List (Int × Int))
deriving DecidableEq, Repr

/-- Full observable outcome of one helper invocation: the mutated value or
the raised error kind. Comparing WHOLE outcomes (value AND error kind) is
exactly what the shipped battery's `t()` serialization does. -/
inductive Outcome
  | ok  (v : PyVal)
  | err (k : ErrKind)
deriving DecidableEq, Repr

/-- A copy of the `pyDelItem` helper, as a denotation on the modeled domain. -/
abbrev Impl := PyVal → Int → Outcome

/-- CPython index normalization for a length-`n` sequence: negative indices
count from the end; `none` = out of range. -/
def normIdx (i : Int) (n : Nat) : Option Nat :=
  let j := if i < 0 then i + n else i
  if 0 ≤ j ∧ j < n then some j.toNat else none

/-- Shared list arm (`Array.isArray` branch on a real list): splice in range,
IndexError out of range. Identical in both copies (the drift is tuple-only). -/
def delListArm (xs : List Int) (k : Int) : Outcome :=
  match normIdx k xs.length with
  | some j => .ok (.list (xs.eraseIdx j))
  | none   => .err .indexError

/-- Shared dict arm: delete a present key, KeyError otherwise. Identical in
both copies on the modeled domain. -/
def delDictArm (kvs : List (Int × Int)) (k : Int) : Outcome :=
  if kvs.any (fun p => p.1 == k) then
    .ok (.dict (kvs.filter (fun p => p.1 != k)))
  else
    .err .keyError

/-- The FIXED copy — transliterates `runtime/src/runtime.js::pyDelItem`
(`main` @ `b4ffda22`, the post-`1b28bae5` shape): a tuple is immutable, so
`del t[i]` is a TypeError ("'tuple' object doesn't support item deletion"),
NEVER a splice and NEVER an IndexError. -/
def delItemFixed : Impl
  | .list xs,  k => delListArm xs k
  | .tuple _,  _ => .err .typeError
  | .dict kvs, k => delDictArm kvs k

/-- The STALE inline copy as it actually shipped pre-fix: NO tuple branch.
A PythScribe tuple is a JS array with a `__pytuple__` marker, so
`Array.isArray` is true and the tuple falls into the list arm — in range it
SILENTLY SPLICES (mutating an "immutable" tuple), out of range it raises
IndexError where CPython raises TypeError (wrong error KIND — the C4 axis).
Independent definition: not written in terms of `delItemFixed`. -/
def delItemStale : Impl
  | .list xs,  k => delListArm xs k
  | .tuple xs, k =>
      match normIdx k xs.length with
      | some j => .ok (.tuple (xs.eraseIdx j))
      | none   => .err .indexError
  | .dict kvs, k => delDictArm kvs k

/-- INVARIANT 1a: two copies of one helper are consistent iff they denote the
SAME function on the whole modeled domain (extensional equality — nothing
weaker; agreeing "up to error kind" would have blessed the real drift). -/
def Consistent (f g : Impl) : Prop := ∀ v k, f v k = g v k

/-- INVARIANT 1b: a build configuration (the set of copies of one helper that
a build can embed or import) is consistent iff its copies are PAIRWISE
consistent — "a helper has at most one semantic definition". -/
def ConsistentAll (impls : List Impl) : Prop :=
  ∀ f ∈ impls, ∀ g ∈ impls, Consistent f g

/-- The single-source-by-extraction configuration (the shipped A1 shim +
extraction: every embedded/imported copy IS the canonical definition,
copied verbatim) satisfies the invariant BY CONSTRUCTION — for any number of
build paths. Deliberately near-trivial: single-sourcing makes parity
definitional; that is the design claim, and the refutations below carry the
discriminating content (F1 note in the header manifest). -/
theorem extraction_consistent (n : Nat) :
    ConsistentAll (List.replicate n delItemFixed) := by
  intro f hf g hg v k
  rw [(List.mem_replicate.mp hf).2, (List.mem_replicate.mp hg).2]

/-- The shipped two-path configuration after the A1 shim: `pyths compile`
imports the package runtime; `pyths run/bundle` embed the extraction of the
SAME source. Consistent by construction. -/
theorem shipped_single_source_consistent :
    Consistent delItemFixed delItemFixed := fun _ _ => rfl

/-- DISCRIMINATING WITNESS (silent tuple splice): on `del t[0]` with
`t = (1,2,3)` the fixed copy raises TypeError; the stale copy returns
NORMALLY with the tuple mutated to `(2,3)`. The outcomes differ in
ok-vs-err — the strongest possible disagreement. -/
theorem drift_witness_splice :
    delItemFixed (.tuple [1, 2, 3]) 0 = .err .typeError ∧
    delItemStale (.tuple [1, 2, 3]) 0 = .ok (.tuple [2, 3]) := by
  constructor <;> rfl

/-- DISCRIMINATING WITNESS (error KIND, the C4 axis): on `del t[0]` with
`t = ()` BOTH copies raise — but the fixed copy raises TypeError and the
stale copy IndexError. Any parity notion blind to the error kind cannot see
this drift (see the false-world control below). -/
theorem drift_witness_kind :
    delItemFixed (.tuple []) 0 = .err .typeError ∧
    delItemStale (.tuple []) 0 = .err .indexError := by
  constructor <;> rfl

/-- THE REFUTATION — the historical drifted configuration (package fixed,
inline stale, exactly the post-`1b28bae5` shipping state) VIOLATES the
invariant. This is the theorem whose absence let the drift ship: had the
build been required to satisfy `ConsistentAll`, the `1b28bae5` half-landing
would have been rejected at the tuple witness. -/
theorem two_copy_drift_refuted :
    ¬ ConsistentAll [delItemFixed, delItemStale] := by
  intro h
  have hc := h delItemFixed (by simp) delItemStale (by simp)
  have := hc (.tuple [1, 2, 3]) 0
  rw [drift_witness_splice.1, drift_witness_splice.2] at this
  exact absurd this (by simp)

/-- Localization (by construction — shared arms): the two copies agree on
every LIST and DICT input; the modeled drift is tuple-only, exactly like the
real one. So the witness above discriminates precisely, not accidentally. -/
theorem copies_agree_off_tuples :
    (∀ xs k, delItemFixed (.list xs) k = delItemStale (.list xs) k) ∧
    (∀ kvs k, delItemFixed (.dict kvs) k = delItemStale (.dict kvs) k) :=
  ⟨fun _ _ => rfl, fun _ _ => rfl⟩

/-! ### The parity gate (face B of `inline_runtime_parity.rs`) -/

/-- A battery probe: one concrete invocation. -/
abbrev Probe := PyVal × Int

/-- The behavioral battery: run BOTH copies on every probe and compare WHOLE
outcomes — mutated value AND error kind (the shipped battery serializes
error NAMES, so a wrong-kind regression fails the diff). -/
def battery (f g : Impl) (corpus : List Probe) : Bool :=
  corpus.all fun p => decide (f p.1 p.2 = g p.1 p.2)

/-- Gate soundness (HONEST bounded claim): a green battery certifies
agreement ON THE CORPUS — nothing more. Full extensional parity is delivered
by extraction (`extraction_consistent`), not by the battery. -/
theorem battery_sound (f g : Impl) (corpus : List Probe)
    (h : battery f g corpus = true) :
    ∀ p ∈ corpus, f p.1 p.2 = g p.1 p.2 := by
  intro p hp
  exact of_decide_eq_true (List.all_eq_true.mp h p hp)

/-- Gate completeness on witnessed drift — "the proof would catch the
drift": for ANY two copies and ANY corpus, if the corpus contains one input
where the copies disagree, the battery is RED. The gate cannot pass a drift
its corpus can see; what it can miss is exactly an uncovered input, which is
why the battery-coverage rule (and false world 1 below) matter. -/
theorem battery_catches_witnessed_drift (f g : Impl) (corpus : List Probe)
    (p : Probe) (hp : p ∈ corpus) (hne : f p.1 p.2 ≠ g p.1 p.2) :
    battery f g corpus = false := by
  cases hb : battery f g corpus with
  | false => rfl
  | true  => exact absurd (battery_sound f g corpus hb p hp) hne

/-- A discriminating corpus per the shipped battery-coverage rule: every
receiver-shape branch AND its error paths, tuple probes included. -/
def discriminatingCorpus : List Probe :=
  [ (.list [1, 2, 3], 0), (.list [1, 2, 3], -1), (.list [1], 5),
    (.dict [(1, 2)], 1), (.dict [], 0),
    (.tuple [1, 2, 3], 0), (.tuple [], 0) ]

/-- SPOT (proved THROUGH `battery_catches_witnessed_drift`, not by
evaluation): the gate run on the discriminating corpus REJECTS the
historical drifted pair. If the completeness theorem above were silently
weakened, this would not close. -/
theorem gate_rejects_pyDelItem_drift :
    battery delItemFixed delItemStale discriminatingCorpus = false :=
  battery_catches_witnessed_drift _ _ _ (.tuple [1, 2, 3], 0)
    (by simp [discriminatingCorpus])
    (by rw [drift_witness_splice.1, drift_witness_splice.2]; simp)

/-- Non-vacuity of the gate: it is not always-red — the single-source
configuration is GREEN over the SAME corpus. Together with the rejection
above, the gate genuinely discriminates. -/
theorem gate_green_on_single_source :
    battery delItemFixed delItemFixed discriminatingCorpus = true := by
  decide

/-! ### False-world controls — both are REAL historical failure modes -/

/-- FALSE WORLD 1 — the pre-blocker-2 battery: happy paths only, no tuple or
error-path probes. This is the exact configuration under which the real
drift "sailed through a GREEN battery" (0.2.2 hold blocker 2). -/
def happyPathCorpus : List Probe :=
  [ (.list [1, 2, 3], 0), (.list [1, 2], -1), (.dict [(1, 2)], 1) ]

/-- In false world 1 the drifted pair PASSES the gate: the battery-coverage
rule (probe every branch and its error kinds) is load-bearing, not
decoration. A gate is only as strong as its corpus. -/
theorem happy_path_battery_misses_drift :
    battery delItemFixed delItemStale happyPathCorpus = true := by
  decide

def isErr : Outcome → Bool
  | .err _ => true
  | .ok _  => false

/-- FALSE WORLD 2 — a KIND-BLIND comparator: compares only ok-vs-err,
discarding the error kind (i.e., a battery that does NOT serialize error
names). -/
def kindBlindBattery (f g : Impl) (corpus : List Probe) : Bool :=
  corpus.all fun p => isErr (f p.1 p.2) == isErr (g p.1 p.2)

/-- In false world 2 the REAL error-kind drift (TypeError vs IndexError on
`del ()[0]`) survives: both copies "error", so the kind-blind gate stays
green. The shipped battery's error-NAME serialization is load-bearing. -/
theorem kind_blind_misses_kind_drift :
    kindBlindBattery delItemFixed delItemStale [(.tuple [], 0)] = true := by
  decide

/-- …while the real gate is RED on the very same single-probe corpus. -/
theorem full_gate_catches_kind_drift :
    battery delItemFixed delItemStale [(.tuple [], 0)] = false := by
  decide

end CopyParity

/-! ## Part 2 — Container-method dispatch soundness + determinism (F2/WB-3/WB-10) -/

namespace Dispatch

/-- Container receiver kinds carried by the arity table
(`method_table.rs::ReceiverKind`, container subset). -/
inductive RecvKind
  | list
  | dict
  | set
  | tuple
deriving DecidableEq, Repr

/-- One row of the arity table: legal positional-arg range for the Python
container method of a name, plus the container kinds owning that name
(`method_table.rs::ContainerArity`). -/
structure ArityRule where
  min : Nat
  max : Option Nat
  containers : List RecvKind
deriving DecidableEq, Repr

/-- `ContainerArity::accepts` — is `argc` a legal positional count? -/
def accepts (r : ArityRule) (argc : Nat) : Bool :=
  decide (r.min ≤ argc) &&
    (match r.max with
     | none => true
     | some m => decide (argc ≤ m))

def ca (min : Nat) (max : Option Nat) (containers : List RecvKind) : ArityRule :=
  ⟨min, max, containers⟩

/-- Transliterates `method_table.rs::container_method_arity`
(`main` @ `b4ffda22`) — the source of truth for the F2 root fix. Variadic
set ops are intentionally absent (documented residual there and here). -/
def arityTable : List (String × ArityRule) :=
  [ ("append",     ca 1 (some 1) [.list]),
    ("extend",     ca 1 (some 1) [.list]),
    ("insert",     ca 2 (some 2) [.list]),
    ("reverse",    ca 0 (some 0) [.list]),
    ("remove",     ca 1 (some 1) [.list, .set]),
    ("add",        ca 1 (some 1) [.set]),
    ("discard",    ca 1 (some 1) [.set]),
    ("clear",      ca 0 (some 0) [.list, .dict, .set]),
    ("copy",       ca 0 (some 0) [.list, .dict, .set]),
    ("pop",        ca 0 (some 2) [.list, .dict, .set]),
    ("keys",       ca 0 (some 0) [.dict]),
    ("values",     ca 0 (some 0) [.dict]),
    ("items",      ca 0 (some 0) [.dict]),
    ("popitem",    ca 0 (some 0) [.dict]),
    ("get",        ca 1 (some 2) [.dict]),
    ("setdefault", ca 1 (some 2) [.dict]),
    ("count",      ca 1 (some 3) [.list, .tuple]),
    ("index",      ca 1 (some 3) [.list, .tuple]) ]

def lookupArity (name : String) : Option ArityRule :=
  (arityTable.find? (fun p => p.1 == name)).map (·.2)

/-- TABLE INTERNAL CONSISTENCY (a): no duplicate names — at most ONE arity
rule per method name, so table lookup is deterministic by construction
(two rules for one name would be the ambiguity bug). Decided over the
finite shipped table. -/
theorem arityTable_names_nodup : (arityTable.map (·.1)).Nodup := by decide

/-- TABLE INTERNAL CONSISTENCY (b): every rule is non-degenerate — it
accepts its own `min` (min ≤ max, so no rule is unsatisfiable/never-firing)
and owns at least one container kind. Anti-vacuity for `accepts`. -/
theorem arityTable_nondegenerate :
    arityTable.all
      (fun p => accepts p.2 p.2.min && !p.2.containers.isEmpty) = true := by
  decide

/-- Receiver context at a call site, abstracting `emit.rs` inference:
`infer_type` identified a container kind / `is_foreign_receiver` proved a
JS/DOM constructor result / no information. -/
inductive RecvCtx
  | provably (k : RecvKind)
  | foreign
  | unknown
deriving DecidableEq, Repr

/-- One method-call site as the dispatch gate sees it. `replaceJsArgs`
abstracts the WB-10 argument-shape signal (first arg `RegExp(...)` or
second arg a function — both impossible for Python `str.replace`). -/
structure CallSite where
  attr : String
  argc : Nat
  recv : RecvCtx
  replaceJsArgs : Bool
deriving DecidableEq, Repr

inductive Decision
  | verbatim  -- emit the call verbatim (foreign/JS method wins; all args kept)
  | lower     -- keep the Python container lowering
deriving DecidableEq, Repr

/-- `infer_matches_container`: receiver provably one of the kinds owning
this method name. -/
def provablyMatches : RecvCtx → List RecvKind → Bool
  | .provably k, cs => cs.contains k
  | _,           _  => false

/-- WB-3 guard: positional-arg `.sort` (keyword-only in Python — a
positional arg proves a JS comparator). -/
def g0a (c : CallSite) : Prop := c.attr = "sort" ∧ 1 ≤ c.argc

/-- WB-10 guard: `.replace` whose argument shape proves the JS form. -/
def g0b (c : CallSite) : Prop := c.attr = "replace" ∧ c.replaceJsArgs = true

instance (c : CallSite) : Decidable (g0a c) := by unfold g0a; infer_instance
instance (c : CallSite) : Decidable (g0b c) := by unfold g0b; infer_instance

/-- Transliterates `emit.rs::container_dispatch_prefers_verbatim`
(`main` @ `b4ffda22`), decision order preserved:
(0a) WB-3: positional-arg `.sort` can never be Python `list.sort`
(keyword-only) → verbatim, overriding even a provable list;
(0b) WB-10: `.replace` with a JS-proving arg shape → verbatim;
(1)  receiver provably the owning container → keep the lowering;
(2)  arity impossible for the Python method → verbatim (the F2 backstop);
(3)  provably foreign receiver at valid arity → verbatim;
(4)  otherwise (unknown receiver, valid arity) → keep the lowering
(runtime shape dispatch; backward-compatible for untyped containers).
Names absent from the arity table are not arity-gated (str-only methods,
variadic set ops) and keep their own lowering path. -/
def dispatch (c : CallSite) : Decision :=
  if g0a c then .verbatim
  else if g0b c then .verbatim
  else
    match lookupArity c.attr with
    | none => .lower
    | some rule =>
      if provablyMatches c.recv rule.containers = true then .lower
      else if accepts rule c.argc = true then
        (if c.recv = .foreign then .verbatim else .lower)
      else .verbatim

/-- INVARIANT 2 (arity-soundness — the F2 invariant): whenever an
arity-gated name KEEPS the container lowering, either the receiver is
provably the owning Python container (the name IS the Python method — no
collision possible), or the positional arity is legal for the Python
method. Its violation is exactly the F2 bug class: a container lowering
emitted at an impossible arity silently drops/corrupts arguments. -/
def AritySound (d : CallSite → Decision) : Prop :=
  ∀ c rule, lookupArity c.attr = some rule → d c = .lower →
    provablyMatches c.recv rule.containers = true ∨ accepts rule c.argc = true

/-- The SHIPPED dispatch satisfies the invariant — the F2 arg-drop
configuration is unreachable in the modeled dispatch. -/
theorem aritySound_dispatch : AritySound dispatch := by
  intro c rule hrule hd
  unfold dispatch at hd
  split at hd
  · exact absurd hd (by simp)
  · split at hd
    · exact absurd hd (by simp)
    · rw [hrule] at hd
      simp only at hd
      split at hd
      · exact Or.inl (by assumption)
      · split at hd
        · exact Or.inr (by assumption)
        · exact absurd hd (by simp)

/-- WB-3 invariant: a positional-arg `.sort` is NEVER container-lowered —
it overrides even a provably-list receiver (`[].sort(f)` is a CPython
TypeError; the positional arg PROVES a JS comparator). The violation was
WB-3: `pyListSort` silently dropped the comparator. -/
theorem sort_positional_always_verbatim (c : CallSite)
    (h1 : c.attr = "sort") (h2 : 1 ≤ c.argc) :
    dispatch c = .verbatim := by
  unfold dispatch
  rw [if_pos (show g0a c from ⟨h1, h2⟩)]

/-- WB-10 invariant: a `.replace` whose argument shape proves JS
`String.prototype.replace` (regex pattern / function replacer) is never
lowered to `pyStrReplace` (which would drop capture groups / stringify the
replacer). -/
theorem replace_js_signal_always_verbatim (c : CallSite)
    (h1 : c.attr = "replace") (h2 : c.replaceJsArgs = true) :
    dispatch c = .verbatim := by
  have hn : ¬ g0a c := fun hg => absurd (h1.symm.trans hg.1) (by decide)
  unfold dispatch
  rw [if_neg hn, if_pos (show g0b c from ⟨h1, h2⟩)]

/-! ### Discriminating witnesses (the real bugs, both directions) -/

/-- THE F2 WITNESS, proved as a SPOT THROUGH `aritySound_dispatch` (not by
evaluation): `FormData().append("file", file)` — foreign receiver, 2
positional args. Two args are IMPOSSIBLE for Python `list.append` (arity
exactly 1) and `foreign` is not a provable container, so by contraposition
of the invariant the dispatch cannot keep the lowering — it diverts to
verbatim and every argument is preserved. Pre-fix, this exact call site
compiled to 1-arg `.append("file")`, silently dropping the file. -/
theorem f2_witness_diverted :
    dispatch ⟨"append", 2, .foreign, false⟩ = .verbatim := by
  cases hd : dispatch ⟨"append", 2, .foreign, false⟩ with
  | verbatim => rfl
  | lower =>
      have h := aritySound_dispatch ⟨"append", 2, .foreign, false⟩
        (ca 1 (some 1) [.list]) (by rfl) hd
      rcases h with h | h <;> exact absurd h (by decide)

/-- Positive selection: the same name on a PROVABLE list at legal arity
keeps the container lowering — the discriminator selects, it does not
blanket-verbatim (a blanket-verbatim dispatch would satisfy `AritySound`
vacuously; this witness rules that degenerate reading out). -/
theorem append_on_list_lowered :
    dispatch ⟨"append", 1, .provably .list, false⟩ = .lower := by decide

/-- Positive selection: unknown receiver at legal arity keeps the lowering
(runtime shape dispatch — the backward-compatible arm for untyped lists). -/
theorem append_unknown_valid_arity_lowered :
    dispatch ⟨"append", 1, .unknown, false⟩ = .lower := by decide

/-- Backstop fires even WITHOUT receiver knowledge: unknown receiver at
IMPOSSIBLE arity diverts (this is the load-bearing part of the F2 fix —
it does not depend on typing `FormData`). -/
theorem append_unknown_impossible_arity_diverted :
    dispatch ⟨"append", 2, .unknown, false⟩ = .verbatim := by decide

/-- WB-3 witness: `xs.sort(cmp)` on a PROVABLE list diverts to verbatim
`Array.prototype.sort(cmp)` — the override beats the provable-container
short-circuit. Pre-fix, `pyListSort` dropped `cmp`. -/
theorem wb3_witness_sort_comparator :
    dispatch ⟨"sort", 1, .provably .list, false⟩ = .verbatim :=
  sort_positional_always_verbatim _ rfl (by decide)

/-- …while keyword-only / no-arg `xs.sort()` keeps the `pyListSort`
lowering (Python replace-all… — Python in-place sort semantics). -/
theorem sort_no_positional_lowered :
    dispatch ⟨"sort", 0, .provably .list, false⟩ = .lower := by decide

/-- WB-10 witness pair: with the JS-proving arg signal, `.replace` diverts;
a plain `s.replace(str, str)` keeps the Python lowering. -/
theorem wb10_witness_replace :
    dispatch ⟨"replace", 2, .unknown, true⟩ = .verbatim ∧
    dispatch ⟨"replace", 2, .unknown, false⟩ = .lower := by
  exact ⟨replace_js_signal_always_verbatim _ rfl rfl, by decide⟩

/-! ### The pre-fix false world — name-only dispatch (refuted) -/

/-- The PRE-fix dispatch (before `87d4eaaa`): a catalogued container-method
NAME alone selects the container lowering — no receiver context, no arity
gate. Independent definition (not written via `dispatch`). -/
def dispatchPreF2 (c : CallSite) : Decision :=
  match lookupArity c.attr with
  | some _ => .lower
  | none   => .verbatim

/-- FALSE-WORLD CONTROL: the pre-fix dispatch VIOLATES the arity-soundness
invariant on the exact FormData witness — it keeps the lowering at an
impossible arity on a non-container receiver. This is the configuration
that shipped the F2 arg-drop; the invariant genuinely forbids it (the
statement is falsifiable, and the pre-fix world falsifies it). -/
theorem preF2_not_aritySound : ¬ AritySound dispatchPreF2 := by
  intro h
  have := h ⟨"append", 2, .foreign, false⟩ (ca 1 (some 1) [.list]) (by rfl) (by rfl)
  rcases this with h' | h' <;> exact absurd h' (by decide)

/-! ### Determinism: the shipped cascade fires exactly one rule -/

/-- The dispatch rules as individual guards with the shipped priority
compiled in (each guard conjoins the negation of every earlier one), so the
rule SYSTEM can be studied as an unordered set. -/
inductive Rule
  | sortPos        -- WB-3 override
  | replaceJs      -- WB-10 override
  | notGated       -- name absent from the arity table
  | provCont       -- receiver provably the owning container
  | arityBackstop  -- impossible arity (the F2 backstop)
  | foreignRecv    -- provably foreign at valid arity
  | fallLower      -- unknown receiver, valid arity
deriving DecidableEq, Repr

/-- Guard of each rule, priorities compiled in. -/
def guard (r : Rule) (c : CallSite) : Prop :=
  match r with
  | .sortPos   => g0a c
  | .replaceJs => ¬ g0a c ∧ g0b c
  | .notGated  => ¬ g0a c ∧ ¬ g0b c ∧ lookupArity c.attr = none
  | .provCont  => ¬ g0a c ∧ ¬ g0b c ∧ ∃ rule, lookupArity c.attr = some rule ∧
      provablyMatches c.recv rule.containers = true
  | .arityBackstop => ¬ g0a c ∧ ¬ g0b c ∧ ∃ rule, lookupArity c.attr = some rule ∧
      provablyMatches c.recv rule.containers = false ∧ accepts rule c.argc = false
  | .foreignRecv => ¬ g0a c ∧ ¬ g0b c ∧ (∃ rule, lookupArity c.attr = some rule ∧
      provablyMatches c.recv rule.containers = false ∧ accepts rule c.argc = true) ∧
      c.recv = .foreign
  | .fallLower => ¬ g0a c ∧ ¬ g0b c ∧ (∃ rule, lookupArity c.attr = some rule ∧
      provablyMatches c.recv rule.containers = false ∧ accepts rule c.argc = true) ∧
      c.recv ≠ .foreign

def output : Rule → Decision
  | .sortPos       => .verbatim
  | .replaceJs     => .verbatim
  | .notGated      => .lower
  | .provCont      => .lower
  | .arityBackstop => .verbatim
  | .foreignRecv   => .verbatim
  | .fallLower     => .lower

/-- The dispatch RELATION: rule `r` fires on `c` producing `d`. -/
def Fires (c : CallSite) (d : Decision) : Prop :=
  ∃ r, guard r c ∧ output r = d

/-- EXHAUSTIVENESS + DISJOINTNESS in one statement: the rule system agrees
pointwise with the total function `dispatch` — every call site fires SOME
rule, and every rule that fires produces the function's decision. -/
theorem fires_iff_dispatch (c : CallSite) (d : Decision) :
    Fires c d ↔ dispatch c = d := by
  have step (rule : ArityRule) (h1 : ¬ g0a c) (h2 : ¬ g0b c)
      (h3 : lookupArity c.attr = some rule) :
      dispatch c =
        (if provablyMatches c.recv rule.containers = true then .lower
         else if accepts rule c.argc = true then
           (if c.recv = .foreign then Decision.verbatim else .lower)
         else .verbatim) := by
    unfold dispatch
    rw [if_neg h1, if_neg h2, h3]
  constructor
  · rintro ⟨r, hg, rfl⟩
    cases r with
    | sortPos =>
        have hg' : g0a c := hg
        unfold dispatch; rw [if_pos hg']; rfl
    | replaceJs =>
        obtain ⟨h1, h2⟩ := hg
        unfold dispatch; rw [if_neg h1, if_pos h2]; rfl
    | notGated =>
        obtain ⟨h1, h2, h3⟩ := hg
        unfold dispatch; rw [if_neg h1, if_neg h2, h3]; rfl
    | provCont =>
        obtain ⟨h1, h2, rule, h3, h4⟩ := hg
        rw [step rule h1 h2 h3, if_pos h4]; rfl
    | arityBackstop =>
        obtain ⟨h1, h2, rule, h3, h4, h5⟩ := hg
        rw [step rule h1 h2 h3]
        simp [h4, h5, output]
    | foreignRecv =>
        obtain ⟨h1, h2, ⟨rule, h3, h4, h5⟩, h6⟩ := hg
        rw [step rule h1 h2 h3]
        simp [h5, h6, output, provablyMatches]
    | fallLower =>
        obtain ⟨h1, h2, ⟨rule, h3, h4, h5⟩, h6⟩ := hg
        rw [step rule h1 h2 h3]
        simp [h5, h6, output, provablyMatches]
  · intro hd
    subst hd
    by_cases h1 : g0a c
    · exact ⟨.sortPos, h1, by unfold dispatch; rw [if_pos h1]; rfl⟩
    · by_cases h2 : g0b c
      · exact ⟨.replaceJs, ⟨h1, h2⟩,
          by unfold dispatch; rw [if_neg h1, if_pos h2]; rfl⟩
      · cases h3 : lookupArity c.attr with
        | none =>
            exact ⟨.notGated, ⟨h1, h2, h3⟩,
              by unfold dispatch; rw [if_neg h1, if_neg h2, h3]; rfl⟩
        | some rule =>
            by_cases h4 : provablyMatches c.recv rule.containers = true
            · exact ⟨.provCont, ⟨h1, h2, rule, h3, h4⟩,
                by rw [step rule h1 h2 h3, if_pos h4]; rfl⟩
            · have h4' : provablyMatches c.recv rule.containers = false :=
                Bool.eq_false_iff.mpr h4
              by_cases h5 : accepts rule c.argc = true
              · by_cases h6 : c.recv = .foreign
                · exact ⟨.foreignRecv, ⟨h1, h2, ⟨rule, h3, h4', h5⟩, h6⟩,
                    by rw [step rule h1 h2 h3]; simp [h5, h6, output, provablyMatches]⟩
                · exact ⟨.fallLower, ⟨h1, h2, ⟨rule, h3, h4', h5⟩, h6⟩,
                    by rw [step rule h1 h2 h3]; simp [h5, h6, output, provablyMatches]⟩
              · have h5' : accepts rule c.argc = false := Bool.eq_false_iff.mpr h5
                exact ⟨.arityBackstop, ⟨h1, h2, rule, h3, h4', h5'⟩,
                  by rw [step rule h1 h2 h3]; simp [h4', h5', output]⟩

/-- DETERMINISM (the F2-family non-ambiguity invariant): the shipped rule
system is a FUNCTION — no two rules fire on one call site with different
decisions. Derived from `fires_iff_dispatch`, so it is exactly as strong as
the pointwise agreement, not an artifact of defining dispatch as a
function. -/
theorem fires_functional (c : CallSite) (d₁ d₂ : Decision)
    (h₁ : Fires c d₁) (h₂ : Fires c d₂) : d₁ = d₂ := by
  rw [fires_iff_dispatch] at h₁ h₂
  rw [← h₁, ← h₂]

/-- TOTALITY: every call site fires some rule (no dispatch hole). -/
theorem fires_total (c : CallSite) : ∃ d, Fires c d :=
  ⟨dispatch c, (fires_iff_dispatch c (dispatch c)).mpr rfl⟩

/-! ### The pre-fix rule system is genuinely AMBIGUOUS -/

/-- The PRE-fix system had two applicable interpretations at a collision
site and no discriminator: the container-name rule ("this name is a Python
container method → lower it") and the foreign-interop rule ("a provably
foreign receiver's methods pass through verbatim" — the Tier-D intent).
Modeled as an UNORDERED pair of rules, exactly the ambiguity the F2 fix
resolves with context+arity. -/
inductive PreRule
  | containerName
  | foreignVerbatim
deriving DecidableEq, Repr

def preGuard : PreRule → CallSite → Prop
  | .containerName,   c => (lookupArity c.attr).isSome = true
  | .foreignVerbatim, c => c.recv = .foreign

def preOutput : PreRule → Decision
  | .containerName   => .lower
  | .foreignVerbatim => .verbatim

def PreFires (c : CallSite) (d : Decision) : Prop :=
  ∃ r, preGuard r c ∧ preOutput r = d

/-- AMBIGUITY REFUTATION: on the F2 witness BOTH pre-fix rules fire with
CONTRADICTORY outputs — the pre-fix dispatch relation is NOT a function.
(Shipping resolved the ambiguity the WRONG way: the container-name rule won
and the argument was dropped.) The shipped system on the same witness fires
exactly one rule — the arity backstop (`f2_witness_diverted`). -/
theorem preF2_ambiguous :
    PreFires ⟨"append", 2, .foreign, false⟩ .lower ∧
    PreFires ⟨"append", 2, .foreign, false⟩ .verbatim :=
  ⟨⟨.containerName, by show (lookupArity "append").isSome = true; decide, rfl⟩,
   ⟨.foreignVerbatim, rfl, rfl⟩⟩

theorem preF2_not_functional :
    ¬ (∀ c d₁ d₂, PreFires c d₁ → PreFires c d₂ → d₁ = d₂) := by
  intro h
  have := h _ _ _ preF2_ambiguous.1 preF2_ambiguous.2
  exact absurd this (by decide)

end Dispatch

/-! ## Axiom footprint — pinned per declaration (build-gated) -/

/-- info: 'InternalConsistency.CopyParity.extraction_consistent' depends on axioms: [propext] -/
#guard_msgs in #print axioms CopyParity.extraction_consistent

/-- info: 'InternalConsistency.CopyParity.shipped_single_source_consistent' does not depend on any axioms -/
#guard_msgs in #print axioms CopyParity.shipped_single_source_consistent

/-- info: 'InternalConsistency.CopyParity.drift_witness_splice' does not depend on any axioms -/
#guard_msgs in #print axioms CopyParity.drift_witness_splice

/-- info: 'InternalConsistency.CopyParity.drift_witness_kind' does not depend on any axioms -/
#guard_msgs in #print axioms CopyParity.drift_witness_kind

/-- info: 'InternalConsistency.CopyParity.two_copy_drift_refuted' depends on axioms: [propext] -/
#guard_msgs in #print axioms CopyParity.two_copy_drift_refuted

/-- info: 'InternalConsistency.CopyParity.copies_agree_off_tuples' does not depend on any axioms -/
#guard_msgs in #print axioms CopyParity.copies_agree_off_tuples

/-- info: 'InternalConsistency.CopyParity.battery_sound' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms CopyParity.battery_sound

/-- info: 'InternalConsistency.CopyParity.battery_catches_witnessed_drift' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms CopyParity.battery_catches_witnessed_drift

/-- info: 'InternalConsistency.CopyParity.gate_rejects_pyDelItem_drift' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms CopyParity.gate_rejects_pyDelItem_drift

/-- info: 'InternalConsistency.CopyParity.gate_green_on_single_source' does not depend on any axioms -/
#guard_msgs in #print axioms CopyParity.gate_green_on_single_source

/-- info: 'InternalConsistency.CopyParity.happy_path_battery_misses_drift' does not depend on any axioms -/
#guard_msgs in #print axioms CopyParity.happy_path_battery_misses_drift

/-- info: 'InternalConsistency.CopyParity.kind_blind_misses_kind_drift' does not depend on any axioms -/
#guard_msgs in #print axioms CopyParity.kind_blind_misses_kind_drift

/-- info: 'InternalConsistency.CopyParity.full_gate_catches_kind_drift' does not depend on any axioms -/
#guard_msgs in #print axioms CopyParity.full_gate_catches_kind_drift

/-- info: 'InternalConsistency.Dispatch.arityTable_names_nodup' does not depend on any axioms -/
#guard_msgs in #print axioms Dispatch.arityTable_names_nodup

/-- info: 'InternalConsistency.Dispatch.arityTable_nondegenerate' does not depend on any axioms -/
#guard_msgs in #print axioms Dispatch.arityTable_nondegenerate

/-- info: 'InternalConsistency.Dispatch.aritySound_dispatch' depends on axioms: [propext] -/
#guard_msgs in #print axioms Dispatch.aritySound_dispatch

/-- info: 'InternalConsistency.Dispatch.sort_positional_always_verbatim' depends on axioms: [propext] -/
#guard_msgs in #print axioms Dispatch.sort_positional_always_verbatim

/-- info: 'InternalConsistency.Dispatch.replace_js_signal_always_verbatim' depends on axioms: [propext] -/
#guard_msgs in #print axioms Dispatch.replace_js_signal_always_verbatim

/-- info: 'InternalConsistency.Dispatch.f2_witness_diverted' depends on axioms: [propext] -/
#guard_msgs in #print axioms Dispatch.f2_witness_diverted

/-- info: 'InternalConsistency.Dispatch.append_on_list_lowered' depends on axioms: [propext] -/
#guard_msgs in #print axioms Dispatch.append_on_list_lowered

/-- info: 'InternalConsistency.Dispatch.append_unknown_valid_arity_lowered' depends on axioms: [propext] -/
#guard_msgs in #print axioms Dispatch.append_unknown_valid_arity_lowered

/-- info: 'InternalConsistency.Dispatch.append_unknown_impossible_arity_diverted' depends on axioms: [propext] -/
#guard_msgs in #print axioms Dispatch.append_unknown_impossible_arity_diverted

/-- info: 'InternalConsistency.Dispatch.wb3_witness_sort_comparator' depends on axioms: [propext] -/
#guard_msgs in #print axioms Dispatch.wb3_witness_sort_comparator

/-- info: 'InternalConsistency.Dispatch.sort_no_positional_lowered' depends on axioms: [propext] -/
#guard_msgs in #print axioms Dispatch.sort_no_positional_lowered

/-- info: 'InternalConsistency.Dispatch.wb10_witness_replace' depends on axioms: [propext] -/
#guard_msgs in #print axioms Dispatch.wb10_witness_replace

/-- info: 'InternalConsistency.Dispatch.preF2_not_aritySound' depends on axioms: [propext] -/
#guard_msgs in #print axioms Dispatch.preF2_not_aritySound

/-- info: 'InternalConsistency.Dispatch.fires_iff_dispatch' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms Dispatch.fires_iff_dispatch

/-- info: 'InternalConsistency.Dispatch.fires_functional' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms Dispatch.fires_functional

/-- info: 'InternalConsistency.Dispatch.fires_total' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms Dispatch.fires_total

/-- info: 'InternalConsistency.Dispatch.preF2_ambiguous' does not depend on any axioms -/
#guard_msgs in #print axioms Dispatch.preF2_ambiguous

/-- info: 'InternalConsistency.Dispatch.preF2_not_functional' does not depend on any axioms -/
#guard_msgs in #print axioms Dispatch.preF2_not_functional

end InternalConsistency
