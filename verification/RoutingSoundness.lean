/-
  RoutingSoundness — SEMANTIC admissibility of the credible-compilation
  subscript router (`cert.rs::route` / its Lean twin `routeOf`), with the
  §3b Float/Set-native mis-route as the proved-inadmissible discriminating
  witness.

  WHY THIS FILE EXISTS (the bug the proof catches). the design notes
  §3b (fix commit `3d7a83f2`): the certified router sent `RecvTy.float` and
  `RecvTy.set` plain reads to `Route::Native` (raw JS `3.5[0]` → `undefined`
  → Python `None` instead of CPython's `TypeError` — a C3/C4 violation), and
  the Lean twin `route_read_safety` CERTIFIED that as safe, because its
  precondition `helperTy s.ty = true` was drawn from the ROUTER'S OWN
  classification: when the router (wrongly) excluded float/set from
  `helperTy`, the theorem's coverage silently shrank in lock-step — the
  precondition MOVED WITH THE BUG (the F2 "router-defined hypothesis" /
  Leroy vacuous-precondition trap). `old_statement_blind_to_misroute` below
  reproduces that blindness as a theorem.

  THE FIX SHAPE. Admissibility here has a precondition pinned by CPYTHON
  (`nonSubscriptable` — which receiver types CPython refuses to subscript),
  independent of any router, and a postcondition at the C3/C4 observable
  granularity (an error occurs, AND its kind is `TypeError` — reusing the
  lattice's `Exc`). Against that statement the pre-fix router is PROVABLY
  inadmissible (`buggy_router_inadmissible` — the theorem whose existence
  would have caught §3b), the shipped fixed router is admissible
  (`router_admissible`), and the certificate checker admits ONLY admissible
  routes (`checked_admissible` — internal consistency).

  HONEST BOUNDARIES (stated, not smuggled):
  * The route observables (`nonSubReadObs`) are a MODEL of what each route's
    emitted code does on the witness domain. `helper → TypeError` models the
    runtime `pyGetItem` guard, whose behavior is enforced by the runtime and
    bound EMPIRICALLY by the C4 shipping-binding harness
    (`experiments/pbt-ps/lattice_shipped_binding.py`, witnesses `(3.5)[0]`,
    `({1})[0]`) + the CLI test `test_run_nonsubscriptable_raises_typeerror`
    — the same trust split `route_read_safety` documents. This file proves
    the ROUTING layer; the helper's own behavior is the harness's job.
  * `RecvTy.primitive` is EXCLUDED from `nonSubscriptable` because it is
    MIXED at the router's type granularity (str IS subscriptable; the
    int/bool inside `primitive` are not) — witnessed: `"ab"[0]` succeeds,
    `(5)[0]` is a TypeError, both are `primitive`. A type-level admissibility
    claim over `primitive` would be unfaithful (F4). Int/bool mis-reads are
    covered by the runtime guard + harness, not by this router-level
    theorem; NO unrestricted claim is made (legitimate-restriction triple:
    witnessed / stated / not claimed elsewhere).
  * `pySlice` is unmodeled on this domain (`nonSubReadObs .pySlice = none`):
    plain reads (isSlice = false) can never take it, and admissibility
    conservatively rejects unmodeled routes rather than guessing.

  ROUTE FAMILIES ("all 3 routes"): `RouteK` has four constructors in three
  semantic families — Python-semantics helpers (`pySlice`, `helper` =
  `pyGetItem`), the certified native fast path (`nativeInbounds`, gated by
  in-bounds evidence), and raw native lowering (`native`). The inversion
  lemmas (`route_pySlice_iff` … `route_native_iff`) characterize routeOf on
  ALL four, and `checked_nativeInbounds_evidence` closes the fast path's
  internal consistency (no fast path without checker-verified evidence).

  Lean core only (no mathlib), same as the rest of `verification/`.
-/

import PythExpandVerify

namespace PythExpandVerify
namespace RoutingSoundness

/-! ## The observable model (C3/C4 granularity) -/

/-- Observable outcome of ONE plain subscript read, at exactly the
    granularity the C3 (error-occurrence) / C4 (error-kind) lattice
    components distinguish. `jsUndef` is JS `undefined` (surfacing as Python
    `None`) — NEVER a CPython outcome for a read; it is the §3b failure
    observable. Exception kinds reuse the lattice's `Exc`. -/
inductive ReadObs where
  | pyOk               -- some value, per CPython lookup semantics
  | pyExc (e : Exc)    -- a raised Python exception of class `e`
  | jsUndef            -- silent JS `undefined` → `None` (the §3b observable)
deriving DecidableEq, Repr

/-- Receiver types that are WHOLLY non-subscriptable in CPython, at the
    router's type granularity. CPYTHON-PINNED and ROUTER-INDEPENDENT — this
    is the load-bearing difference from `helperTy`, which is the router's
    own classification and therefore moved with the §3b bug. `primitive` is
    deliberately absent: it is mixed (str subscriptable, int/bool not), so
    no type-level claim is made for it (see header). -/
def nonSubscriptable : RecvTy → Bool
  | .float | .set => true
  | .primitive | .list | .dict | .tuple | .unknown => false

/-- CPython's observable for a plain read on a wholly non-subscriptable
    receiver: `TypeError` ("'float' object is not subscriptable", "'set'
    object is not subscriptable"). Kind only; message text out of scope
    (exception registry, the design notes §7). Empirically bound by
    the §3b harness witnesses `(3.5)[0]` / `({1})[0]`. -/
def refNonSubObs : ReadObs := .pyExc .typeError

/-- What each route's EMITTED code observably does on a plain read of a
    wholly non-subscriptable receiver (the modeled domain — nothing is
    claimed off it). `none` = unmodeled (conservatively inadmissible).
    * `helper`: `pyGetItem` guards non-subscriptable receivers with a Python
      `TypeError` (runtime guard; trust boundary bound by the C4 harness).
    * `native` / `nativeInbounds`: BOTH emit raw JS indexing `x[i]`; on a JS
      Number or Set there is no such property → `undefined` (§3b).
    * `pySlice`: unreachable for plain reads (`isSlice = false`); unmodeled. -/
def nonSubReadObs : RouteK → Option ReadObs
  | .helper => some (.pyExc .typeError)
  | .native => some .jsUndef
  | .nativeInbounds => some .jsUndef
  | .pySlice => none

/-! ## The admissibility statement -/

/-- The modeled domain: a PLAIN READ — not a slice, not an LHS target, not
    optional-chained, no in-bounds evidence — on a wholly non-subscriptable
    receiver. Exactly the §3b site shape. -/
def PlainNonSubRead (s : Site) : Prop :=
  s.isSlice = false ∧ s.isLhs = false ∧ s.isOptional = false ∧
    s.inbounds = false ∧ nonSubscriptable s.ty = true

/-- A route `k` is admissible AT a site `s`: if `s` is in the modeled
    domain, `k`'s modeled observable must be defined AND equal CPython's
    (`TypeError`) — simultaneously C3 (an error occurs, not a silent value)
    and C4 (the error's class is right). Unmodeled routes are inadmissible
    on the domain by construction (conservative gate). -/
def AdmissibleAt (s : Site) (k : RouteK) : Prop :=
  PlainNonSubRead s → nonSubReadObs k = some refNonSubObs

/-- A ROUTING FUNCTION is admissible when every site it routes gets an
    admissible route. This is the soundness condition the §3b router
    violated: it quantifies over ALL sites with a CPython-pinned
    precondition, so a router cannot shrink its own coverage. -/
def Admissible (R : Site → RouteK) : Prop :=
  ∀ s : Site, AdmissibleAt s (R s)

/-! ## The witness sites (the §3b reproducers) -/

/-- `(3.5)[0]` — the §3b float witness site. -/
def floatReadSite : Site := ⟨false, false, false, false, .float⟩

/-- `({1})[0]` — the §3b set witness site. -/
def setReadSite : Site := ⟨false, false, false, false, .set⟩

-- Domain non-vacuity: the witness sites ARE in the modeled domain, and the
-- native observable genuinely DIFFERS from the reference (the witness
-- discriminates — both halves needed for the inadmissibility below to bite).
example : PlainNonSubRead floatReadSite := ⟨rfl, rfl, rfl, rfl, rfl⟩
example : PlainNonSubRead setReadSite := ⟨rfl, rfl, rfl, rfl, rfl⟩
#guard nonSubReadObs .native ≠ some refNonSubObs
#guard nonSubReadObs .nativeInbounds ≠ some refNonSubObs
#guard nonSubReadObs .helper = some refNonSubObs

/-! ## Domain characterization + decidable admissibility checker

`Site` restricted to `PlainNonSubRead` is exactly the two witness sites, so
admissibility of ANY router is decidable — the Move-borrow-checker
"collapse the preconditions into an evaluable checker" move: Lean's own
evaluator certifies the input space is non-empty and checks concrete
routers both ways (`#guard`s below). -/

/-- The modeled domain is EXACTLY the two §3b witness sites. -/
theorem plainNonSubRead_cases (s : Site) (hp : PlainNonSubRead s) :
    s = floatReadSite ∨ s = setReadSite := by
  obtain ⟨hs, hl, ho, hi, hty⟩ := hp
  obtain ⟨a, b, c, d, ty⟩ := s
  cases a <;> cases b <;> cases c <;> cases d <;> cases ty <;>
    simp_all [nonSubscriptable, floatReadSite, setReadSite]

/-- Admissibility of a router ⟺ its decisions at the two witness sites are
    the reference observable. Both directions: the ⟸ shows the finite check
    is COMPLETE (nothing in the domain escapes it); the ⟹ shows it is
    SOUND (an admissible router must pass it). -/
theorem admissible_iff (R : Site → RouteK) :
    Admissible R ↔
      nonSubReadObs (R floatReadSite) = some refNonSubObs ∧
        nonSubReadObs (R setReadSite) = some refNonSubObs := by
  refine ⟨fun h => ⟨h floatReadSite ⟨rfl, rfl, rfl, rfl, rfl⟩,
                    h setReadSite ⟨rfl, rfl, rfl, rfl, rfl⟩⟩,
          fun h s hp => ?_⟩
  cases plainNonSubRead_cases s hp with
  | inl e => subst e; exact h.1
  | inr e => subst e; exact h.2

/-- The evaluable admissibility checker (checker `= true → Admissible`, and
    conversely — `admissibleB_iff`). -/
def admissibleB (R : Site → RouteK) : Bool :=
  decide (nonSubReadObs (R floatReadSite) = some refNonSubObs)
    && decide (nonSubReadObs (R setReadSite) = some refNonSubObs)

theorem admissibleB_iff (R : Site → RouteK) :
    admissibleB R = true ↔ Admissible R := by
  rw [admissible_iff]
  simp [admissibleB]

/-! ## Soundness of the SHIPPED (fixed) router -/

/-- `helperTy` is total post-fix — every receiver type is a helper type for
    plain reads (the §3b root fix, commit `3d7a83f2`). -/
theorem helperTy_total (t : RecvTy) : helperTy t = true := by
  cases t <;> rfl

/-- **Routing soundness of the shipped router.** Every plain read of a
    wholly non-subscriptable receiver is routed so that its modeled
    observable is CPython's `TypeError` — proved THROUGH
    `route_read_safety` (the router picks `helper`) composed with the
    helper's modeled guard. Unlike `route_read_safety`, the precondition
    here is CPython-pinned (`nonSubscriptable`), NOT the router's own
    `helperTy` — this statement CANNOT shrink with a router bug. -/
theorem router_admissible : Admissible routeOf := by
  intro s hp
  obtain ⟨hs, hl, ho, hi, _⟩ := hp
  rw [route_read_safety s (helperTy_total s.ty) hs hl ho hi]
  rfl

/-- Internal consistency, rule level: whatever route the router marks a
    site with, that (site, route) pair is admissible — the router never
    emits a route inadmissible for its site. -/
theorem router_marks_only_admissible (s : Site) :
    AdmissibleAt s (routeOf s) :=
  router_admissible s

-- Evaluator confirmation of the same fact (the checker accepts the shipped
-- router), plus SPOTs deriving the concrete witness decisions THROUGH the
-- theorem (they fail if `Admissible` were vacuous on the witness domain —
-- these are NOT `#guard`-style evaluations of `routeOf`).
#guard admissibleB routeOf
example : nonSubReadObs (routeOf floatReadSite) = some (.pyExc .typeError) :=
  router_admissible floatReadSite ⟨rfl, rfl, rfl, rfl, rfl⟩
example : nonSubReadObs (routeOf setReadSite) = some (.pyExc .typeError) :=
  router_admissible setReadSite ⟨rfl, rfl, rfl, rfl, rfl⟩

/-! ## The DISCRIMINATING WITNESS — the pre-fix router is inadmissible

`buggyRouteOf` is the pre-fix router, reproduced VERBATIM from the parent
of fix commit `3d7a83f2` (`helperTy` excluded `float`/`set`; the routing
rules were otherwise identical). This is the stub-fails companion with the
REAL historical bug as the stub: the theorem below is the one whose
existence would have caught §3b at statement time. -/

/-- The PRE-FIX helper type set (verbatim from history — float/set
    excluded, hence lowered native). -/
def buggyHelperTy : RecvTy → Bool
  | .list | .dict | .tuple | .primitive | .unknown => true
  | .float | .set => false

/-- The PRE-FIX router: identical rules, buggy type set. -/
def buggyRouteOf (s : Site) : RouteK :=
  if s.isSlice then .pySlice
  else if s.inbounds then .nativeInbounds
  else if !s.isLhs && !s.isOptional && buggyHelperTy s.ty then .helper
  else .native

/-- The mis-route, concretely: the pre-fix router sends the §3b float
    witness to the raw native lowering. -/
theorem buggy_misroutes_float : buggyRouteOf floatReadSite = .native := rfl

/-- ... and the set witness likewise. -/
theorem buggy_misroutes_set : buggyRouteOf setReadSite = .native := rfl

/-- **The theorem that catches the §3b bug.** The pre-fix router is NOT
    admissible: at the float witness it emits `native`, whose observable is
    the silent `undefined → None` (`jsUndef`), not CPython's `TypeError` —
    a C3 violation (no error where CPython errors) and a C4 violation (kind
    lost) in one. Had this statement existed pre-fix, `lake build` would
    have failed the moment `helperTy` excluded float/set — unlike
    `route_read_safety`, which stayed green (see next theorem). -/
theorem buggy_router_inadmissible : ¬ Admissible buggyRouteOf := by
  intro h
  have hobs := h floatReadSite ⟨rfl, rfl, rfl, rfl, rfl⟩
  rw [buggy_misroutes_float] at hobs
  exact absurd hobs (by decide)

/-- The same refutation via the evaluable checker (both directions of
    `admissibleB_iff` are exercised across the two routers). -/
theorem buggy_router_fails_checker : admissibleB buggyRouteOf = false := by
  decide

#guard admissibleB buggyRouteOf = false

/-- **Why the OLD statement was blind (the §3b lesson as a theorem).** The
    pre-fix router SATISFIES the pre-fix `route_read_safety` shape verbatim:
    with the precondition drawn from the router's own `buggyHelperTy`, the
    theorem is green over the buggy router — its coverage shrank in
    lock-step with the bug, silently excluding exactly the mis-routed
    sites. (`buggyHelperTy floatReadSite.ty = false`, pinned below — the
    witness never satisfied the old hypothesis.) Together with
    `buggy_router_inadmissible` this is the strict-strengthening
    separation: `Admissible` rejects a router that the old statement-shape
    provably cannot. -/
theorem old_statement_blind_to_misroute (s : Site)
    (hty : buggyHelperTy s.ty = true) (hs : s.isSlice = false)
    (hl : s.isLhs = false) (ho : s.isOptional = false)
    (hi : s.inbounds = false) :
    buggyRouteOf s = .helper := by
  simp [buggyRouteOf, hs, hi, hl, ho, hty]

-- The old hypothesis excluded the witness (coverage moved with the bug):
#guard buggyHelperTy floatReadSite.ty = false
#guard buggyHelperTy setReadSite.ty = false

/-! ## Internal consistency of the VALIDATED artifact (checker level)

Composes the admissibility statement through `checkCert_sound` (Comp₀): a
checker-accepted certificate cannot carry an inadmissible route, and in
particular the checker REJECTS the §3b mis-route record outright. -/

/-- **Checked admissibility.** Every record in a checker-accepted
    certificate is admissible at its recorded input: the recorded route is
    the rule's route (`checkCert_sound`) and the rule is admissible
    (`router_admissible`). No certificate the checker accepts can route a
    non-subscriptable plain read anywhere but the helper. -/
theorem checked_admissible (cert : List SiteRecord)
    (h : checkCert cert = true) (r : SiteRecord) (hr : r ∈ cert) :
    AdmissibleAt r.input r.route := by
  intro hp
  rw [(checkCert_sound cert h r hr).1]
  exact router_admissible r.input hp

/-- The checker rejects the §3b mis-route record concretely: a certificate
    recording `native` for the float witness site fails `checkSite`. (Note
    the honest scope: the PRE-fix checker — whose `route` twin was
    `buggyRouteOf` — would have ACCEPTED this record; rule and checker were
    wrong in lock-step. It is the SEMANTIC statement above, not the
    checker, that catches the bug class.) -/
theorem checker_rejects_float_misroute :
    checkSite ⟨floatReadSite, none, .native⟩ = false := by decide

/-- ... and the set-witness mis-route likewise. -/
theorem checker_rejects_set_misroute :
    checkSite ⟨setReadSite, none, .native⟩ = false := by decide

/-- Emitter-model level (composes with `emitCert_checks`/`emitCert_covers`):
    every record the modeled emitter produces is admissible at its site —
    end to end, rule → emitter → checker all agree on admissibility. -/
theorem emitted_admissible (x : SiteSrc) :
    AdmissibleAt (emitRecord x).input (emitRecord x).route :=
  router_admissible x.toSite

/-! ## Route inversion — internal consistency across ALL route kinds

Complete characterization of `routeOf` per constructor (the three route
families: helper routes `pySlice`/`helper`, the evidence-gated fast path
`nativeInbounds`, raw `native`). These pin exactly WHEN each route fires —
in particular `route_native_iff`: post-fix, raw native lowering happens
ONLY on write targets / optional chains, never on any plain read. -/

theorem route_pySlice_iff (s : Site) :
    routeOf s = .pySlice ↔ s.isSlice = true := by
  obtain ⟨a, b, c, d, ty⟩ := s
  cases a <;> cases b <;> cases c <;> cases d <;> cases ty <;> decide

theorem route_nativeInbounds_iff (s : Site) :
    routeOf s = .nativeInbounds ↔ s.isSlice = false ∧ s.inbounds = true := by
  obtain ⟨a, b, c, d, ty⟩ := s
  cases a <;> cases b <;> cases c <;> cases d <;> cases ty <;> decide

theorem route_helper_iff (s : Site) :
    routeOf s = .helper ↔
      s.isSlice = false ∧ s.inbounds = false ∧ s.isLhs = false
        ∧ s.isOptional = false := by
  obtain ⟨a, b, c, d, ty⟩ := s
  cases a <;> cases b <;> cases c <;> cases d <;> cases ty <;> decide

/-- Post-fix, `native` fires ONLY for writes / optional chains — the
    contrapositive of read safety, now an exact characterization: no plain
    read of ANY type reaches raw native lowering. -/
theorem route_native_iff (s : Site) :
    routeOf s = .native ↔
      s.isSlice = false ∧ s.inbounds = false
        ∧ (s.isLhs = true ∨ s.isOptional = true) := by
  obtain ⟨a, b, c, d, ty⟩ := s
  cases a <;> cases b <;> cases c <;> cases d <;> cases ty <;> decide

/-- Fast-path internal consistency: on a checker-accepted certificate, a
    record routed `nativeInbounds` carries in-bounds EVIDENCE that actually
    justifies it (`justifiedInbounds = true`) — the native fast path cannot
    be taken on trust; the evidence is re-derived by the checker. -/
theorem checked_nativeInbounds_evidence (cert : List SiteRecord)
    (h : checkCert cert = true) (r : SiteRecord) (hr : r ∈ cert)
    (hroute : r.route = .nativeInbounds) :
    justifiedInbounds r.evidence = true := by
  obtain ⟨hrt, hev⟩ := checkCert_sound cert h r hr
  rw [hrt] at hroute
  have hib : r.input.inbounds = true :=
    ((route_nativeInbounds_iff r.input).mp hroute).2
  rw [← hev]
  exact hib

/-! ## Axiom pins (per new theorem — Stage-5 discipline) -/

/-- info: 'PythExpandVerify.RoutingSoundness.router_admissible' depends on axioms: [propext] -/
#guard_msgs in #print axioms router_admissible

/-- info: 'PythExpandVerify.RoutingSoundness.buggy_router_inadmissible' does not depend on any axioms -/
#guard_msgs in #print axioms buggy_router_inadmissible

/-- info: 'PythExpandVerify.RoutingSoundness.admissible_iff' depends on axioms: [propext] -/
#guard_msgs in #print axioms admissible_iff

/-- info: 'PythExpandVerify.RoutingSoundness.admissibleB_iff' depends on axioms: [propext] -/
#guard_msgs in #print axioms admissibleB_iff

/-- info: 'PythExpandVerify.RoutingSoundness.plainNonSubRead_cases' depends on axioms: [propext] -/
#guard_msgs in #print axioms plainNonSubRead_cases

/-- info: 'PythExpandVerify.RoutingSoundness.old_statement_blind_to_misroute' depends on axioms: [propext] -/
#guard_msgs in #print axioms old_statement_blind_to_misroute

/-- info: 'PythExpandVerify.RoutingSoundness.checked_admissible' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms checked_admissible

/-- info: 'PythExpandVerify.RoutingSoundness.emitted_admissible' depends on axioms: [propext] -/
#guard_msgs in #print axioms emitted_admissible

/-- info: 'PythExpandVerify.RoutingSoundness.checker_rejects_float_misroute' does not depend on any axioms -/
#guard_msgs in #print axioms checker_rejects_float_misroute

/-- info: 'PythExpandVerify.RoutingSoundness.route_native_iff' does not depend on any axioms -/
#guard_msgs in #print axioms route_native_iff

/-- info: 'PythExpandVerify.RoutingSoundness.checked_nativeInbounds_evidence' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms checked_nativeInbounds_evidence

end RoutingSoundness
end PythExpandVerify
