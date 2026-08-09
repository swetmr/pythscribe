# Headline claims for verification/formalization.yaml (ten-proofs style).
#
# The ~50 theorems that actually BACK Paper-C sentences — NOT all 429
# theorems/lemmas in PythExpandVerify.lean (most are helper lemmas).
#
# Each entry:
#   decl        : fully-qualified Lean identifier (the FINAL statement),
#                 given WITHOUT the `PythExpandVerify.` prefix (added by tools).
#   name        : one-line informal statement, in English.
#   witness     : the `*_stub_fails` / discriminating-witness theorem that
#                 refutes the WRONG lowering/statement (proves the theorem has
#                 teeth), or None + a `witness_note` if genuinely absent.
#   witness_note: rationale when witness is None (why no refuter is expected).
#   paper_claim : which Paper-C claim / lattice component it backs
#                 (cross-ref reference-app/semantic_preservance.md + TRUST.md).
#
# `file`, `line`, `sorry_count`, and `axioms` are filled in mechanically by
# axiom_footprint.py from the live source + `#print axioms` output.
#
# GROUPS (kept for readability; flattened into HEADLINE at the bottom):

_EXPANDER = [
    dict(decl="expand_order",
         name="Expansion applies tiers in the fixed order E->A->B->hooks->Dict and is left-total (every source expands).",
         witness=None,
         witness_note="Determinism/totality structural theorem, not a preservation claim; no wrong-lowering to refute. Bound to the shipping expander by the diff_harness.py 2,017-case model-vs-implementation differential.",
         paper_claim="Paper C expansion-side; TRUST.md 'Expansion is deterministic' row."),
    dict(decl="expand_length",
         name="Expansion never shortens below the source token count (left-total length monotonicity).",
         witness=None,
         witness_note="Structural totality lemma; no wrong-lowering refuter applies.",
         paper_claim="Paper C expansion-side; TRUST.md determinism row."),
    dict(decl="expand_zone_safety",
         name="No tier rewrites inside string / comment / f-string zones (protected content is emitted verbatim).",
         witness=None,
         witness_note="Zone-safety is a universal 'nothing happens here' property over the executable scanner; the executable-model differential (diff_harness.py) is its shipping binding. Refuted implicitly by the contiguity/verbatim lemmas.",
         paper_claim="Paper C zone-safety; TRUST.md 'Zone-safety' row."),
    dict(decl="expand_characterization",
         name="One-statement characterization of expansion: fixed tier order AND totality AND zone-safety simultaneously.",
         witness=None,
         witness_note="Conjunction of the three structural guarantees; witnessed by the same differential as its conjuncts.",
         paper_claim="Paper C expansion characterization; TRUST.md 'One-statement characterization' row."),
    dict(decl="committed_inverse_consistent",
         name="The SHIPPING 61-entry alias dictionary is inverse-consistent (the decidable hypothesis the round-trip needs holds on the real table).",
         witness=None,
         witness_note="Decided fact about the committed table (gen-dict-data.py --check keeps it in sync with strings.rs); non-vacuity is the point, not a lowering to refute.",
         paper_claim="Paper C alias round-trip; TRUST.md 'Alias round-trip' row."),
    dict(decl="committed_roundtrip",
         name="expand . compress = id on canonical code for the real shipping dictionary (alias round-trip on the committed table).",
         witness=None,
         witness_note="Round-trip identity; the wrong-behavior it rules out (a non-invertible alias) is excluded by committed_inverse_consistent being a proved hypothesis, not assumed.",
         paper_claim="Paper C alias round-trip; TRUST.md 'Alias round-trip' row."),
    dict(decl="roundtrip_tok",
         name="For any inverse-consistent dictionary, expanding a compressed token recovers the original token.",
         witness=None,
         witness_note="Abstract round-trip lemma (the general form of committed_roundtrip); hypothesis-guarded, not vacuous.",
         paper_claim="Paper C alias round-trip (general)."),
    dict(decl="roundtrip_code",
         name="For any inverse-consistent dictionary, expanding a compressed token stream recovers the original stream.",
         witness=None,
         witness_note="Abstract round-trip lemma over token lists.",
         paper_claim="Paper C alias round-trip (general)."),
    dict(decl="expandTok_idem",
         name="Expansion of a single token is idempotent (expanding an already-expanded token is a no-op).",
         witness=None,
         witness_note="Idempotence stability lemma; no lowering to refute.",
         paper_claim="Paper C expansion stability."),
    dict(decl="expandCode_idem",
         name="Expansion of a token stream is idempotent.",
         witness=None,
         witness_note="Idempotence stability over streams.",
         paper_claim="Paper C expansion stability."),
    dict(decl="kwargs_position_discipline",
         name="Tier-B kwarg rewriting fires ONLY in call-argument position (never in statement position, before ==, in a string/comment, or on a non-table identifier).",
         witness=None,
         witness_note="Decided position-discipline predicate over the shipping table; the diff_harness kwargs arm (432 cases) is its shipping binding.",
         paper_claim="Paper C Tier B; TRUST.md 'Tier B is concrete' row."),
    dict(decl="committed_kwargs_exact",
         name="The Tier-B kwarg rewrite is decided to be EXACTLY the shipping 9-entry kwargs.rs table (concrete, not opaque).",
         witness=None,
         witness_note="Exactness of the concrete table; drift-gated by gen-kwarg-data.py --check.",
         paper_claim="Paper C Tier B; TRUST.md 'Tier B is concrete' row."),
]

_ROUTING = [
    dict(decl="route_read_safety",
         name="No Python-typed plain subscript READ lowers to a bare native subscript (every such read routes through a bounds-checked helper).",
         witness=None,
         witness_note="Read-safety over the RouteModel; bound two-sided to cert.rs via route-table.txt (Lean lake exe --check-route-table AND cargo route_table_matches_committed_fixture). Refutation lives at the certificate layer (checkCert_sound / covered_read_safety).",
         paper_claim="Paper C 7.2 credible compilation; semantic_preservance 3b (router soundness)."),
    dict(decl="checkCert_sound",
         name="If the subscript-routing certificate checker accepts a site, its recorded route IS routeOf's route and its in-bounds bit IS what the evidence justifies (Leroy Comp0 property 7).",
         witness="checkSite_sound",
         witness_note=None,
         paper_claim="Paper C 7.2 certificate soundness; TRUST.md 'Certificate-checker soundness' row."),
    dict(decl="emitted_read_safety",
         name="Every emitted Python-typed unsafe read is BOTH present in the certificate AND helper-routed — proved from a model of the emit.rs subscript loop, not an assumed coverage equation.",
         witness=None,
         witness_note="Completeness from the emitCert emitter model (emitCert_covers is the coverage invariant, a theorem). The wrong shape it rules out (an omitted site) is excluded by emitCert_covers; the decidable occurrence-sensitive backstop is covered_read_safety.",
         paper_claim="Paper C 7.2 certificate completeness; TRUST.md 'Certificate-checker soundness' row."),
    dict(decl="covered_read_safety",
         name="On a certificate that passes checkCert AND covers the emitted OCCURRENCES (multiset, not set), every emitted Python-typed unsafe read has >= its emitted multiplicity of helper-routed records.",
         witness=None,
         witness_note="Occurrence-sensitive coverage (siteOccurrences via List.countP). A missing site OR an omitted DUPLICATE makes coversAll FAIL — both pinned by #guard, so the check genuinely constrains the compiler.",
         paper_claim="Paper C 7.2 occurrence-coverage (C2 fix); TRUST.md 'Certificate-checker soundness' row."),
    dict(decl="wasm_admission_sound",
         name="Every boundary type the compiler admits to WASM lowering has a concrete WASM representation (nothing un-lowerable is admitted).",
         witness=None,
         witness_note="Admission soundness (soundness only, NOT completeness/value-equivalence — stated). Bound two-sided to is_wasm_eligible/to_wasm_type via wasm-admission-table.txt; the over-admission it rules out (Optional) is regression-pinned in Rust.",
         paper_claim="Paper C WASM auto-routing; TRUST.md 'WASM auto-routing admission soundness' row."),
    dict(decl="wasmRouteSafe",
         name="On the numeric WASM fragment, the routed/wrapped i64 evaluation equals the Python reference on the in-range region (routing decision is value-safe).",
         witness="wasmRouteSafe_wrapStub_fails",
         witness_note=None,
         paper_claim="Paper C 7b boundary composition (WASM route); semantic_preservance 7b."),
    dict(decl="wasmRouteSafe_jsWasm",
         name="The js+wasm twin path never surfaces a wrong value: a returned .val n equals the CPython reference. The twin is MODELED as the arbitrary-precision reference (evalPyW) — a trust boundary BY CONSTRUCTION, NOT an independent JS/WASM differential; the LOAD-BEARING guard content (the __ovf range guard the wrap-stub refutes) lives in wasmRouteSafe (the edge path). Listed as a corollary of wasmRouteSafe, not a standalone independent claim.",
         witness="wasmRouteSafe_wrapStub_fails",
         witness_note=None,
         paper_claim="Paper C 7b boundary composition (js+wasm twin corollary; trust boundary); semantic_preservance 7b."),
]

_CORE = [
    dict(decl="slice_walk_inbounds",
         name="Every index pySlice/pySetSlice/pyDelSlice reads is a real in-bounds access 0<=i<len, for ANY start/stop, ANY nonzero step (both signs), ANY out-of-range endpoint (the silent-OOB class made impossible by proof).",
         witness=None,
         witness_note="Soundness half of the slice core; its completeness twin slice_walk_complete is what rules out a []-stub / fuel-dropping walk. Together they pin the walk to exactly CPython's index set.",
         paper_claim="Paper C Tier-1 verified core; TRUST.md 'Slice-family clamping totality + completeness' row."),
    dict(decl="slice_walk_complete",
         name="Every on-grid index CPython's slice.indices walk produces IS visited, and the len+1 fuel provably never truncates (completeness — rules out a truncating/empty walk).",
         witness=None,
         witness_note="Completeness is itself the refuter of the []-stub that soundness alone would accept (F7 fix). Model<->CPython bound by sliceIndices #guards.",
         paper_claim="Paper C Tier-1 verified core; TRUST.md 'Slice-family' row."),
    dict(decl="getIndex_inbounds",
         name="Scalar index normalization: pyGetItem admits an index only when it is a real in-bounds access 0<=i<n (the negative-index-OOB class made impossible by proof).",
         witness=None,
         witness_note="Totality/safety theorem; the getIndex_spec companion pins the exact normalized value (two-sidedness). CPython-bound by tier2_spec_validate.py.",
         paper_claim="Paper C Tier-2; TRUST.md 'Scalar index-normalization safety' row."),
    dict(decl="getIndex_spec",
         name="pyGetItem's normalized index is EXACTLY CPython's (two-sided characterization pinning a single result — anti-vacuity for getIndex_inbounds).",
         witness=None,
         witness_note="Characterization (uniqueness) theorem; a too-weak spec fails its SPOTs. CPython-bound.",
         paper_claim="Paper C Tier-2; TRUST.md 'Scalar index-normalization safety' row."),
    dict(decl="setIndex_inbounds",
         name="List-write index normalization is byte-identical to the read path, so writes are in-bounds by the very same theorem (the #278/#279 WRITE side).",
         witness=None,
         witness_note="Proved alias of getIndex_inbounds; axiom-free.",
         paper_claim="Paper C Tier-2; TRUST.md 'List-write index safety' row."),
    dict(decl="pyBool_iff_not_falsy",
         name="The shipping pyBool implements Python's OWN falsy rule over the value representation ({}->False, empty/zero exactly) — truthiness conformance.",
         witness=None,
         witness_note="Conformance iff over an INDEPENDENT falsy rule (pyBoolM vs pyFalsy); CPython-bound by 23 representatives. A wrong truthiness rule breaks the iff.",
         paper_claim="Paper C Tier-2; TRUST.md 'Truthiness conformance' row."),
    dict(decl="pyEqV_trans",
         name="pyEq is an EQUIVALENCE relation (reflexive+symmetric+transitive) on the full modeled domain incl. nested dict/list values, with bool-subset-int chains.",
         witness=None,
         witness_note="Equivalence-relation property with a transitivity SPOT; CPython-bound (595 triples). Non-vacuity is the nested-value faithfulness (C8), witnessed by distinct nested structures.",
         paper_claim="Paper C Tier-2 full equality; TRUST.md 'pyEq is an EQUIVALENCE RELATION' row."),
]

# Tier-3 C1-rollout preservation waves — the INDEPENDENT-target, shipping-bound
# form. Each: (main theorem, its discriminating *_stub_fails refuter).
_WAVES = [
    ("preservation", None,
     "SEED: evaluating the COMPILED arithmetic fragment equals Python semantics incl. the //,% floor-division deviation (two differently-defined evaluators).",
     "Seed theorem predates the independent-target/stub-fails form; guarded by a through-the-theorem SPOT carrying the -7//2=-4 deviation. Later waves add the *_stub_fails refuters.",
     "Paper C Tier-3 seed; TRUST.md 'Tier-3 SEED' row."),
    ("preservationE", "preservationE_stub_fails",
     "C1 wave 1 (expressions): the compiled imperative-expression fragment computes the same value as the Python reference under the shipped floor-correction lowering.",
     "Paper C Tier-3 wave 2 / C1 rollout wave 1; TRUST.md 'statement-language compilation preservation' row."),
    ("preservationS", "preservationS_stub_fails",
     "C1 wave 1 (statements): compiled control flow + state (assignment, if, bounded while) yields the same final environment as Python.",
     "Paper C Tier-3 wave 2 / C1 rollout wave 1; TRUST.md 'statement-language' row."),
    ("preservationFE", "preservationFE_stub_fails",
     "C1 wave 3 (functions, expr side): compiled first-order functions incl. recursion + return short-circuit compute the same result as Python.",
     "Paper C Tier-3 wave 4 / C1 rollout wave 3; TRUST.md 'Tier-3 wave 4' row."),
    ("preservationFS", "preservationFS_stub_fails",
     "C1 wave 3 (functions, stmt side): compiled function bodies compute the same result+environment as Python.",
     "Paper C Tier-3 wave 4 / C1 rollout wave 3; TRUST.md 'Tier-3 wave 4' row."),
    ("preservationC", "preservationC_stub_fails",
     "C1 wave 4 (collections): compiled list values + literals + len + indexing (reusing verified-core getIndex) compute the same value as Python.",
     "Paper C Tier-3 wave 5 / C1 rollout wave 4; TRUST.md 'Tier-3 wave 5' row."),
    ("preservationCs", "preservationCs_stub_fails",
     "C1 wave 4 (collections, list-level): the element-list value itself is preserved (observed at the list level, no indexing).",
     "Paper C Tier-3 wave 5 / C1 rollout wave 4; TRUST.md 'Tier-3 wave 5' row."),
    ("preservationG", "preservationG_stub_fails",
     "C1 wave 5 (comprehensions): compiled list-comprehension fragment preserves the Python value; filter truthiness is full-domain valTruthy (F9 domain-complete).",
     "Paper C Tier-3 wave 6 / C1 rollout wave 5; TRUST.md 'Tier-3 wave 6' row."),
    ("anyG_preservation", "anyG_stub_fails",
     "C1 wave 5 (lazy any): the lazy short-circuiting any(... for ...) — Python's REAL semantics — is preserved (rules out the #348 eager-materialize bug).",
     "Paper C Tier-3 wave 6 / C1 rollout wave 5; TRUST.md 'Tier-3 wave 6' row (laziness non-vacuity)."),
    ("preservationI", "preservationI_stub_fails",
     "C1 wave 6 (itertools): compiled lazy core (count/islice/takewhile/chain) + combinatorial trio (product/permutations/combinations) preserve the Python value.",
     "Paper C Tier-3 wave 7 / C1 rollout wave 6; TRUST.md 'Tier-3 wave 7' row."),
    ("preservationD", "preservationD_stub_fails",
     "C1 wave 7 (dicts as values): compiled int-keyed dict literals (last-write-wins) + key lookup preserve the Python value.",
     "Paper C Tier-3 wave 9 / C1 rollout wave 7; TRUST.md 'Tier-3 wave 9' row."),
    ("preservationCls", "preservationCls_stub_fails",
     "C1 wave 8 (classes): compiled multi-class/multi-method fragment (new, attr get, single-dispatch method call, nested objects) preserves the Python value.",
     "Paper C Tier-3 wave 10 / C1 rollout wave 8; TRUST.md 'Tier-3 wave 10' row."),
    ("preservationS11", "preservationS11_stub_fails",
     "C1 wave 9 (strings): compiled string fragment (code-point values, concat, len, index, slice) preserves the Python value; D3/D5 UTF-16 deviation proved as a witness.",
     "Paper C Tier-3 wave 11 / C1 rollout wave 9; TRUST.md 'Tier-3 wave 11' row."),
    ("preservationBw", "preservationBw_stub_fails",
     "C1 wave 10 (bitwise): compiled &,|,^,<<,>> on non-negative ints preserve the arbitrary-precision Python value (no 32-bit ToInt32 truncation).",
     "Paper C Tier-3 wave 13 / C1 rollout wave 10; TRUST.md 'Tier-3 wave 13' row."),
    ("preservationPow", "preservationPow_stub_fails",
     "C1 wave 11 (power): compiled integer ** preserves Python's EXACT arbitrary-precision bigint (not an IEEE double).",
     "Paper C Tier-3 wave 14 / C1 rollout wave 11; TRUST.md 'Tier-3 wave 14' row."),
    ("preservationSort", "preservationSort_stub_fails",
     "C1 wave 12 (sorted): compiled sorted() preserves Python's numeric order (vs naive JS lexicographic .sort()).",
     "Paper C Tier-3 wave 16 / C1 rollout wave 12; TRUST.md 'Tier-3 wave 16' row."),
    ("preservationSm", "preservationSm_stub_fails",
     "C1 wave 13 (string offsets): compiled .index/.find/len return CODE-POINT offsets (vs naive JS UTF-16 code units).",
     "Paper C Tier-3 wave 18 / C1 rollout wave 13; TRUST.md 'Tier-3 wave 18' row."),
    ("preservationNb", "preservationNb_stub_fails",
     "C1 wave 14 (negative bitwise): compiled ~x and negative-operand &,|,^ preserve Python's infinite two's-complement value — the wave whose shipping binding found the real unary-~ 32-bit miscompile.",
     "Paper C Tier-3 wave 17 / C1 rollout wave 14; TRUST.md 'Tier-3 wave 17' row (the bug-finding payoff)."),
    ("preservationSg", "preservationSg_stub_fails",
     "C1 wave 15 (general string methods): compiled startswith/find/index/count/replace/strip/split/join preserve Python semantics incl. empty needles + astral handling.",
     "Paper C Tier-3 wave 19 / C1 rollout wave 15; TRUST.md 'Tier-3 wave 19' row."),
    ("preservationD2", "preservationD2_stub_fails",
     "C1 wave 16 (dict methods, int|str keys): compiled d[k]/get/in/len/keys/values/items preserve CPython 3.7+ order rules; the FINAL rollout wave (+ excluded unguarded + per F10).",
     "Paper C Tier-3 wave 20 / C1 rollout wave 16; TRUST.md 'Tier-3 wave 20' row."),
    ("preservationWasm", None,
     "Tier-3 wave 8 (WASM numeric): the i64 fast path is a faithful WRAPPING implementation — evalW e = (evalPyW e).map wrapI64 for every expression.",
     "Width-axis deviation (i64 wraparound vs Python bigint); refuter role played by overflow #guards showing the evaluators genuinely differ + preservationWasm_inRange corollary.",
     "Paper C Tier-3 wave 8; TRUST.md 'Tier-3 wave 8' row."),
]

_LATTICE = [
    dict(decl="preservationA",
         name="LATTICE v2 arith fragment, under the tag-faithful IDEALIZED lowering: the compiled evaluator preserves value (C1), type-tag (C2) and error-KIND (C4) in ONE PyResult statement, each INDEPENDENTLY refuted (truncAL/d1CollapseAL/zeroKind+typeKindAL). C3 error-occurrence HOLDS but is a consequence of shared zero-guard structure — NOT independently refutable here (the fable F-1 gap); the fully-knobbed all-six version with an independent C3 (guardZero) refuter is preservationUnion. Shipped D1 deviation carried by preservationA_rho_holds_d1 / preservationA_tagStub_fails.",
         witness="preservationA_valueStub_fails",
         witness_note=None,
         paper_claim="Paper C preservation-lattice v2 core; semantic_preservance 5 (Day-1 PoC C1^C2^C4; C3 as consequence). See preservationUnion for the broad-observational union (DEPENDENT projections, not independent axes; universal over the Lean fragment, bound to shipped pyths by a full-observation differential)."),
    dict(decl="preservationA_zeroKindStub_fails",
         name="C4 error-KIND stub litmus (ZERO site): a lowering that raises ZeroDivisionError where CPython raises a different kind is REFUTED — the error-kind axis has teeth.",
         witness=None,
         witness_note="This decl IS itself the discriminating refuter for preservationA's C4 (zero-site) axis; a refuter needs no refuter. Listed as a headline claim because it establishes C4 non-vacuity.",
         paper_claim="Paper C lattice v2 C4 axis; semantic_preservance 5."),
    dict(decl="preservationA_typeKindStub_fails",
         name="C4 error-KIND stub litmus (TYPE site): a lowering that raises the wrong exception kind on a type error is REFUTED.",
         witness=None,
         witness_note="This decl IS itself the discriminating refuter for preservationA's C4 (type-site) axis; establishes C4 non-vacuity on the type-error site.",
         paper_claim="Paper C lattice v2 C4 axis; semantic_preservance 5."),
    dict(decl="preservationA_tag",
         name="C2 type-representation axis: the shipped lowering preserves the Python type tag of every arithmetic result.",
         witness="preservationA_tagStub_fails",
         witness_note=None,
         paper_claim="Paper C lattice v2 C2 axis; semantic_preservance 5."),
    dict(decl="preservationA_rho_holds_d1",
         name="The D1 exception-registry collapse is rho-faithful: quotienting error KINDS by the documented D1 registry still preserves the observable equivalence.",
         witness="preservationA_rhoStub_fails",
         witness_note=None,
         paper_claim="Paper C lattice v2 D1 collapse; semantic_preservance 7 (exception registry)."),
    dict(decl="evalAtgt_d1_collapse",
         name="The independent arithmetic target under the D1-collapse lowering computes the registry-collapsed reference semantics (binds d1CollapseAL to the reference).",
         witness=None,
         witness_note="Binding lemma for the D1 collapse lowering (exercises d1CollapseAL / valRho / PVal defs).",
         paper_claim="Paper C lattice v2 D1; semantic_preservance 7."),
    dict(decl="typeTag_not_rhoFaithful",
         name="The raw type-tag projection is NOT rho-faithful (bool vs int share observable behavior) — establishing why the D1/rho quotient is needed, not the naive tag.",
         witness=None,
         witness_note="Negative result (impossibility); exercises the typeTag def. It IS the refuter of the naive-tag composition (compositionThm_typeTagStub_fails).",
         paper_claim="Paper C lattice v2 C2/D1; semantic_preservance 7b composition."),
    dict(decl="preservationSub",
         name="C4 on SUBSCRIPT: the compiled subscript fragment preserves value AND error-KIND (IndexError vs KeyError vs TypeError distinguished), over the independent target.",
         witness="preservationSub_valueStub_fails",
         witness_note=None,
         paper_claim="Paper C lattice v2 Day-2 subscript-C4; semantic_preservance 5 (Day 2)."),
    dict(decl="preservationSub_keyKindStub_fails",
         name="Subscript C4 refuter: a lowering that raises KeyError where CPython raises IndexError (or vice-versa) is REFUTED — subscript error-kind has teeth.",
         witness=None,
         witness_note="This decl IS itself the discriminating refuter for preservationSub's error-kind axis; establishes subscript-C4 non-vacuity.",
         paper_claim="Paper C lattice v2 Day-2 subscript-C4; semantic_preservance 5."),
    dict(decl="preservationSc",
         name="C5 EFFECT-ORDER (short-circuit): the compiled short-circuit fragment preserves Python's effect/trace order — a wrong EAGER lowering is refuted even when it agrees on the final value.",
         witness="preservationSc_eagerStub_fails",
         witness_note=None,
         paper_claim="Paper C lattice v2 Day-2 C5 short-circuit; semantic_preservance 5 (Day 2)."),
    dict(decl="preservationSc_trace_fails_on_success",
         name="C5 non-vacuity: the effect-order theorem constrains the TRACE even on successful (non-erroring) runs — an eager lowering with the same value but wrong effect order is refuted.",
         witness="preservationSc_eagerStub_fails",
         witness_note=None,
         paper_claim="Paper C lattice v2 C5 effect-order non-vacuity; semantic_preservance 5."),
    dict(decl="preservationAy",
         name="C5 EFFECT-ORDER (statement reordering): the compiled flatten lowering preserves the effect machine; a REORDER lowering that permutes observable effects is refuted.",
         witness="preservationAy_reorderStub_fails",
         witness_note=None,
         paper_claim="Paper C lattice v2 C5 effect-order; semantic_preservance 5 / 7b."),
]

_COMPOSITION = [
    dict(decl="compositionThm",
         name="COMPOSITION meta-theorem (7b centerpiece): for any rho-faithful observation M, preservation of the per-component semantics COMPOSES to preservation of M across the boundary.",
         witness="compositionThm_typeTagStub_fails",
         witness_note=None,
         paper_claim="Paper C 7b boundary composition centerpiece; semantic_preservance 7b (composition principle)."),
    dict(decl="compositionThm_arith",
         name="Composition instantiated on the arithmetic fragment: rho-faithful observations compose through the arith preservation.",
         witness="compositionThm_typeTagStub_fails",
         witness_note=None,
         paper_claim="Paper C 7b; semantic_preservance 7b."),
    dict(decl="compositionThm_arith_d1",
         name="Composition instantiated on the arith fragment under the D1 exception-registry collapse (the shipped observation is rho-faithful, so it composes).",
         witness="compositionThm_arith_d1_typeTagStub_fails",
         witness_note=None,
         paper_claim="Paper C 7b under D1; semantic_preservance 7b."),
    dict(decl="d1c_rhoFaithful",
         name="The D1 collapse map is rho-faithful (the hypothesis compositionThm needs, discharged for the real shipped observation — not assumed).",
         witness="typeTag_not_rhoFaithful",
         witness_note=None,
         paper_claim="Paper C 7b rho-faithfulness; semantic_preservance 7 / 7b."),
    dict(decl="M_dom_rhoFaithful",
         name="Any domain-quotient observation nodeOf that respects rho is rho-faithful — the general marshalling lemma the boundary composition rests on.",
         witness="M_dom_reorderStub_fails",
         witness_note=None,
         paper_claim="Paper C 7b marshalling lemmas; semantic_preservance 7b."),
]


# The UNION — ONE equation entailing BROAD observational preservation MODULO the
# documented exception set (D1 whole-float tag; message text) on a representative
# combined fragment (arith //,%,zero-guard + subscript + short-circuit + recursion) over
# the PyResult x trace x fuel carrier. The named slices are DEPENDENT congrArg projections
# (tag = fn(value), C2 subset C1; order & termination share the eager knob), NOT
# independent axes; each carries a per-projection refuter (teeth). Bound to the SHIPPED
# pyths by experiments/pbt-ps/union_shipped_binding.py (model==pyths==CPython, DRY).
# See semantic_preservance.md 5/6.
_UNION = [
    dict(decl="preservationUnion",
         name="THE UNION (Paper-C centrepiece): ONE equation evalUjs fuel e = evalUpy fuel e over the combined arith(//,% - the % arm is a genuine ufmod constructor, jsFmod-lowered, proved via jsFmod_eq_fmod)+subscript+short-circuit+recursion fragment, carrier PyResult UVal x trace fuel-indexed. The HONEST composite claim, in two parts stated exactly: (1) preservationUnion is UNIVERSAL over the Lean model fragment (forall UExpr, forall fuel, in Lean) - BROAD observational preservation (value, type mod-D1, error-occurrence, error-KIND, effect-order, termination agree at every fuel); (2) it is BOUND to the shipped compiler by a full-observation pyths<->CPython differential over an N-program INT-typed corpus (union_shipped_binding.py: obs_sig(eval_model)==parse(pyths)==parse(CPython) on all axes at once for SCALAR values - container final values coarsen to their type-name; the D1 whole-float cases are bound SEPARATELY as model-derived tag+repr witnesses, not the full signature - plus a Lean<->Python drift guard via `lake env lean`), modulo the documented exception set {D1 whole-float type TAG - shipped type(2.0)->int, bound THROUGH the model obsTagModD1 (the separate repr-2.0 retention is a runtime fact, unmodeled); message text (unmodeled)}. The finite corpus is NOT claimed exhaustive - universality lives in the Lean proof, the differential binds it to the runtime. NOT six independent axes: the slices are DEPENDENT projections (tag=fn(value), C2 subset C1; order & termination share the eager knob). The Lean theorem is universal over UExpr (incl. ufloat, which stays float-tagged under jsUL = CPython-faithful); the SHIPPING differential is the int-typed corpus + separate partial D1 witnesses; lift to 16 waves is CONJECTURED-mechanical future work. Two documented out-of-fragment divergences (str % -> NotImplementedError vs TypeError; raise-by-name -> NameError) noted in the harness, outside the modeled domain.",
         witness="preservationUnion_occurrenceStub_fails",
         witness_note=None,
         paper_claim="Paper C semantic-preservation UNION (broad observational preservation modulo exceptions, on a representative fragment, bound to shipped pyths); semantic_preservance 5/6."),
    dict(decl="preservationUnion_value",
         name="Value projection of the union (congrArg): compiled value equals CPython value at every fuel. Refuted per-projection by a truncating // lowering (obsValue teeth).",
         witness="preservationUnion_valueStub_fails", witness_note=None,
         paper_claim="Paper C union value slice; semantic_preservance 2 (C1)."),
    dict(decl="preservationUnion_tag",
         name="Type-tag projection MODULO D1: compiled type() (whole-float untagged to int) equals CPython's tag mod-D1 (int/list/dict). A DEPENDENT slice: obsTagModD1_factors proves the tag is a function of the value (C2 subset C1), so it has no separate refuter; the D1 whole-float deviation is the documented EXCEPTION, modeled by evalUtgt_d1_untag and witnessed on pyths.",
         witness="evalUtgt_d1_untag", witness_note="C2 subset C1 (obsTagModD1_factors): the mod-D1 tag is a function of the value, so tag preservation is a COROLLARY of value preservation, not an independently-refutable axis. evalUtgt_d1_untag MODELS + discriminates the D1 whole-float exception (shipped int vs CPython float; agree mod-D1), harness-confirmed on pyths.",
         paper_claim="Paper C union tag slice (mod-D1, C2 subset C1); semantic_preservance 2 (C2)."),
    dict(decl="preservationUnion_occurrence",
         name="Error-occurrence projection of the union: an error occurs iff CPython raises — refuted per-projection by a NO-zero-guard lowering (silent value where CPython raises), the occurrence knob the earlier preservationA lacked (fable F-1 gap CLOSED).",
         witness="preservationUnion_occurrenceStub_fails", witness_note=None,
         paper_claim="Paper C union occurrence slice (the F-1 fix); semantic_preservance 2 (C3)."),
    dict(decl="preservationUnion_kind",
         name="Error-kind projection of the union: the exception CLASS matches CPython (ZeroDivisionError/IndexError/KeyError/TypeError). Refuted per-projection by a wrong-class lowering (obsKind teeth) even where value and occurrence agree.",
         witness="preservationUnion_kindStub_fails", witness_note=None,
         paper_claim="Paper C union kind slice; semantic_preservance 2 (C4)."),
    dict(decl="preservationUnion_trace",
         name="Effect-order projection of the union: the observable print/effect trace matches CPython — refuted per-projection by an EAGER short-circuit lowering (same value, extra effect). Shares the eager knob with termination (dependent, not independent).",
         witness="preservationUnion_orderStub_fails", witness_note=None,
         paper_claim="Paper C union order slice; semantic_preservance 2 (C5)."),
    dict(decl="preservationUnion_terminates",
         name="Termination projection: the compiled program halts (exists a fuel) iff the CPython reference does — refuted by the EAGER lowering on '1 or (while True)', which diverges where the lazy reference halts. NB eager is NOT value-faithful in general (it errors on '1 or raise'); order and termination are two consequences of the one eager choice, not independent axes.",
         witness="preservationUnion_terminationStub_fails", witness_note=None,
         paper_claim="Paper C union termination slice; semantic_preservance 2 (C6)/6."),
    dict(decl="evalUpy_fuel_mono",
         name="Fuel-monotonicity: if the reference halts at some budget with result r, it halts with the SAME r at any larger budget — makes the C6 'exists a halting fuel' predicate well-defined.",
         witness=None,
         witness_note="Structural monotonicity lemma (no wrong-lowering to refute); load-bearing well-definedness for terminatesU. propext-only.",
         paper_claim="Paper C union C6 well-definedness; semantic_preservance 6 (fuel model)."),
    dict(decl="preservationUnion_occurrenceStub_fails",
         name="Occurrence teeth (THE fable F-1 gap): a lowering with NO zero-guard returns a value where CPython raises ZeroDivisionError — REFUTED per-projection (obsErr). Establishes the occurrence slice of the union is load-bearing (guardZero knob).",
         witness=None,
         witness_note="This decl IS the per-projection refuter for the union's occurrence slice; a refuter needs no refuter. Documents the F-1-gap closure.",
         paper_claim="Paper C union occurrence non-vacuity (F-1 fix); semantic_preservance 4/6."),
    dict(decl="preservationUnion_terminationStub_fails",
         name="Termination teeth: the EAGER lowering FAILS to preserve termination on '1 or (while True)' (lazy reference halts, eager diverges at every fuel) — the halting projection is refuted. NB eager is not value-faithful in general; order and termination both stem from the one eager knob.",
         witness=None,
         witness_note="This decl IS the per-projection refuter for the union's termination slice; establishes termination-preservation non-vacuity (a wrong lazy->eager lowering breaks it).",
         paper_claim="Paper C union termination non-vacuity; semantic_preservance 6."),
]


def _flatten():
    out = []
    out += _EXPANDER
    out += _ROUTING
    out += _CORE
    for entry in _WAVES:
        if len(entry) == 5:
            decl, witness, name, wnote, paper = entry
        else:
            decl, witness, name, paper = entry
            wnote = None
        out.append(dict(decl=decl, name=name, witness=witness,
                        witness_note=wnote, paper_claim=paper))
    out += _LATTICE
    out += _COMPOSITION
    out += _UNION
    return out


HEADLINE = _flatten()

# The pinned axiom footprint. Our core is dependency-free (no Mathlib); the
# three-axiom classical trio is the CEILING (inherited from Lean core's
# `String` where `Classical.choice`/`Quot.sound` enter). Many decls are
# strictly tighter (`[propext]`, `[propext, Quot.sound]`, or axiom-free).
PINNED_AXIOMS = {"propext", "Classical.choice", "Quot.sound"}

if __name__ == "__main__":
    print(f"{len(HEADLINE)} headline claims")
    seen = {}
    for h in HEADLINE:
        seen[h["decl"]] = seen.get(h["decl"], 0) + 1
    dupes = {k: v for k, v in seen.items() if v > 1}
    if dupes:
        print("DUPLICATE decls:", dupes)
