/-
  PythExpandVerify — a Lean 4 model of the PythScribe `.psc` expander's
  core rewrite properties, in the style of the Axon verified compiler
  (Rinard 2026): a compact, faithful MODEL of the tier-rewrite system,
  with the three core safety/correctness lemmas machine-checked.

  Ground truth modeled: `crates/pyths_expand/src/lib.rs :: expand_with_config`,
  a FIXED-ORDER pipeline  E → C → A → B → hooks → Dict  of position-aware
  text rewrites. Each tier rewrites recognised forms ONLY in "code" zones,
  emitting string / docstring / comment "protected zones" verbatim. All tiers
  share ONE zone classifier (`crates/pyths_expand/src/zones.rs`); the tiers
  differ only in what they do in a code zone (see `strings.rs`, `idioms.rs`,
  `kwargs.rs`, `hooks.rs`, and — line-oriented, gated by
  `zones::line_start_states` — `lib.rs::expand_line`).

  This file is deliberately dependency-free (Lean core only, no mathlib) so
  `lake build` needs only the pinned toolchain.

  Faithfulness argument: see README.md. In brief — the real scanner
  partitions the byte stream into Code / Protected segments and applies each
  tier's matcher only within Code segments; our `Segment` + `applyTier`
  abstraction captures exactly that invariant, so any tier (whatever its
  internal matcher) is a Code-only rewrite. The dictionary tier's inner
  behaviour is refined to a token model to state the alias round-trip.
-/

import DictData
import KwargData
import TierAData
import HookData
import IdiomData

namespace PythExpandVerify

/-! ## Segment representation

A `.psc` source, after the scanner's zone classification, is a list of
segments. Each segment is either `code` text (eligible for rewriting) or
`prot` text (a string / comment / f-string zone, preserved verbatim). -/

inductive Segment where
  | code (s : String)
  | prot (s : String)
deriving DecidableEq, Repr

abbrev Source := List Segment

/-- A tier is any code-text rewrite. We treat it as an opaque `String → String`:
    zone-safety and determinism hold for ANY such function, so the model does
    not depend on the internals of E / C / A / B / hooks / Dict. -/
abbrev Tier := String → String

/-- Apply a tier: rewrite `code` segments through `f`, leave `prot` verbatim.
    This is THE abstraction of "rewrite only outside protected zones". -/
def applyTier (f : Tier) : Source → Source
  | [] => []
  | .code s :: rest => .code (f s) :: applyTier f rest
  | .prot s :: rest => .prot s :: applyTier f rest

/-- Extract the protected-zone payloads, in order. -/
def protPayloads : Source → List String
  | [] => []
  | .code _ :: rest => protPayloads rest
  | .prot s :: rest => s :: protPayloads rest

/-- Run a pipeline of tiers left-to-right (head applied first).
    Explicit recursion (not `foldl`) so it reduces definitionally. -/
def runPipeline : List Tier → Source → Source
  | [], src => src
  | f :: fs, src => runPipeline fs (applyTier f src)

/-- The expander configuration: the five tiers, in their FIXED pipeline order
    matching `expand_with_config` Steps 1–5. -/
structure Config where
  tierE : Tier      -- Step 1: %NAME idioms
  tierA : Tier      -- Step 2: presets + decorator aliases
  tierB : Tier      -- Step 3: kwarg-position aliases
  tierHooks : Tier  -- Step 4: hook-call shorthand
  tierDict : Tier   -- Step 5: $NAME domain dictionary

/-- The pipeline order is a fixed list — this is the "tier order is fixed"
    content of determinism. -/
def Config.pipeline (c : Config) : List Tier :=
  [c.tierE, c.tierA, c.tierB, c.tierHooks, c.tierDict]

/-- The top-level expander. -/
def expand (c : Config) (src : Source) : Source :=
  runPipeline c.pipeline src

/-! ## Lemma 1 — Determinism (fixed order + left-totality)

`expand` is a pure total function whose tier order is statically fixed.
We witness this two ways:
  * `expand_order` pins the exact left-to-right composition (E first … Dict
    last), so there is no nondeterminism in tier ordering or selection.
  * `applyTier_length` / `expand_length` show every segment is mapped
    (nothing dropped or duplicated) — the pipeline is left-total on every
    input. -/

theorem applyTier_length (f : Tier) (src : Source) :
    (applyTier f src).length = src.length := by
  induction src with
  | nil => rfl
  | cons s rest ih => cases s <;> simp [applyTier, ih]

/-- Determinism, part A: the pipeline is exactly the fixed left-to-right
    composition `Dict ∘ hooks ∘ B ∘ A ∘ E`. -/
theorem expand_order (c : Config) (src : Source) :
    expand c src
      = applyTier c.tierDict (applyTier c.tierHooks (applyTier c.tierB
          (applyTier c.tierA (applyTier c.tierE src)))) := by
  simp [expand, Config.pipeline, runPipeline]

theorem runPipeline_length (tiers : List Tier) (src : Source) :
    (runPipeline tiers src).length = src.length := by
  induction tiers generalizing src with
  | nil => rfl
  | cons f fs ih =>
    show (runPipeline fs (applyTier f src)).length = src.length
    rw [ih (applyTier f src), applyTier_length]

/-- Determinism, part B: left-totality — `expand` maps every input to an
    output of identical segment count (no segment dropped or invented). -/
theorem expand_length (c : Config) (src : Source) :
    (expand c src).length = src.length := by
  simp [expand, runPipeline_length]

/-! ## Lemma 2 — Zone-safety

No tier rewrites inside a protected segment: the protected payloads are
preserved verbatim by every tier, hence by the whole pipeline. This is THE
key safety property — a dictionary/idiom sigil inside a string literal must
never expand. -/

/-- Zone-safety for a single tier. -/
theorem zone_safety (f : Tier) (src : Source) :
    protPayloads (applyTier f src) = protPayloads src := by
  induction src with
  | nil => rfl
  | cons s rest ih =>
    cases s with
    | code a => simp [applyTier, protPayloads, ih]
    | prot a => simp [applyTier, protPayloads, ih]

/-- Zone-safety for an arbitrary pipeline (hence the real 6-tier one). -/
theorem pipeline_zone_safety (tiers : List Tier) (src : Source) :
    protPayloads (runPipeline tiers src) = protPayloads src := by
  induction tiers generalizing src with
  | nil => rfl
  | cons f fs ih =>
    show protPayloads (runPipeline fs (applyTier f src)) = protPayloads src
    rw [ih (applyTier f src), zone_safety]

/-- Zone-safety for the full expander: protected content is byte-identical
    in the output. -/
theorem expand_zone_safety (c : Config) (src : Source) :
    protPayloads (expand c src) = protPayloads src := by
  simp [expand, pipeline_zone_safety]

/-! ## Lemma 3 — Dictionary alias round-trip

Refinement of the `$NAME` dictionary tier (`strings.rs`) to a token model.
A code segment's content is a list of tokens: ordinary text, a `$alias`
sigil (compressed form), or a canonical string literal (expanded form).

`compress` replaces an in-table canonical value `v` with its alias `$k`;
`expand` replaces `$k` back with `v`. We prove `expand ∘ compress = id` on
the ALIAS DOMAIN — canonical (`.ps`) source, i.e. token lists that contain
no `$alias` sigils (those only exist in the compressed `.psc`). -/

abbrev Alias := String
abbrev Canon := String
/-- The committed alias table: `(alias, canonicalValue)` pairs. -/
abbrev Dict := List (Alias × Canon)

/-- Forward lookup: alias → canonical (first match wins, as in `strings.rs`). -/
def elookup : Dict → Alias → Option Canon
  | [], _ => none
  | (a, c) :: t, k => if a = k then some c else elookup t k

/-- Reverse lookup: canonical → alias (used by `compress`, which we define
    since only `expand` ships). -/
def rlookup : Dict → Canon → Option Alias
  | [], _ => none
  | (a, c) :: t, v => if c = v then some a else rlookup t v

inductive Tok where
  | txt (s : String)   -- ordinary code text
  | ali (a : Alias)    -- `$a` sigil — exists only in compressed `.psc`
  | can (c : Canon)    -- a canonical string literal — the expanded form
deriving DecidableEq, Repr

/-- Is this token a `$alias` sigil? Sigils are outside the alias domain. -/
def Tok.isAli : Tok → Bool
  | .ali _ => true
  | _ => false

/-- Expand one token: `$a` → its canonical (or a literal `$a` if unknown,
    matching the Rust "unknown alias left verbatim" behaviour). -/
def expandTok (D : Dict) : Tok → Tok
  | .ali a => match elookup D a with
              | some c => .can c
              | none => .txt ("$" ++ a)
  | t => t

/-- Compress one token: an in-range canonical → its alias; else unchanged. -/
def compressTok (D : Dict) : Tok → Tok
  | .can c => match rlookup D c with
              | some a => .ali a
              | none => .can c
  | t => t

/-- The table is inverse-consistent: whenever the reverse lookup yields `a`
    for `c`, the forward lookup yields `c` for `a`. Holds for any table whose
    aliases and canonicals are each distinct (a bijection) — the committed
    table's invariant. -/
def InverseConsistent (D : Dict) : Prop :=
  ∀ a c, rlookup D c = some a → elookup D a = some c

/-- Round-trip on a single token in the alias domain (not a `$alias`). -/
theorem roundtrip_tok (D : Dict) (hinv : InverseConsistent D)
    (t : Tok) (hdom : t.isAli = false) :
    expandTok D (compressTok D t) = t := by
  cases t with
  | txt s => rfl
  | ali a => simp [Tok.isAli] at hdom
  | can c =>
    simp only [compressTok]
    cases hr : rlookup D c with
    | none => simp [expandTok]
    | some a =>
      simp only [expandTok, hinv a c hr]

/-- Code-segment expand / compress: map the token rewrite across the segment. -/
def expandCode (D : Dict) : List Tok → List Tok := List.map (expandTok D)
def compressCode (D : Dict) : List Tok → List Tok := List.map (compressTok D)

/-- Round-trip on a whole code segment: `expandCode ∘ compressCode = id`
    on the alias domain (no `$alias` sigils present). This is deliverable #3. -/
theorem roundtrip_code (D : Dict) (hinv : InverseConsistent D)
    (xs : List Tok) (hdom : ∀ t ∈ xs, t.isAli = false) :
    expandCode D (compressCode D xs) = xs := by
  induction xs with
  | nil => rfl
  | cons t ts ih =>
    simp only [expandCode, compressCode, List.map_cons, List.cons.injEq]
    refine ⟨roundtrip_tok D hinv t (hdom t (by simp)), ?_⟩
    have hts : ∀ x ∈ ts, x.isAli = false := fun x hx => hdom x (by simp [hx])
    have := ih hts
    simpa [expandCode, compressCode] using this

/-! ### Optional: idempotence of the dictionary expand pass

`expand` is a no-op on already-expanded source — matches the Rust
`dict_idempotent_on_canonical_input` test. -/

theorem expandTok_idem (D : Dict) (t : Tok) :
    expandTok D (expandTok D t) = expandTok D t := by
  cases t with
  | txt s => rfl
  | can c => rfl
  | ali a =>
    cases h : elookup D a with
    | none => simp [expandTok, h]
    | some c => simp [expandTok, h]

theorem expandCode_idem (D : Dict) (xs : List Tok) :
    expandCode D (expandCode D xs) = expandCode D xs := by
  induction xs with
  | nil => rfl
  | cons t ts ih =>
    simp only [expandCode, List.map_cons, List.cons.injEq]
    exact ⟨expandTok_idem D t, ih⟩

/-! ## Non-vacuity witnesses (concrete, fully decidable)

Demonstrate the round-trip on a concrete table so the general theorems are
not vacuous. -/

/-- The empty table is trivially inverse-consistent (reverse lookup never
    succeeds), so the hypothesis of `roundtrip_code` is satisfiable — the
    theorem is not vacuous. -/
theorem nil_inverse_consistent : InverseConsistent [] := by
  intro a c h; simp [rlookup] at h

/-- A concrete two-entry slice of the committed table (`strings.rs`). -/
def exampleDict : Dict := [("c1", "\"#9ca3af\""), ("p4", "\"16px\"")]

example : compressTok exampleDict (Tok.can "\"#9ca3af\"") = Tok.ali "c1" := by decide
example : expandTok exampleDict (Tok.ali "c1") = Tok.can "\"#9ca3af\"" := by decide

/-- Concrete end-to-end round-trip on a small canonical code segment. -/
example :
    expandCode exampleDict
      (compressCode exampleDict
        [Tok.txt "color = ", Tok.can "\"#9ca3af\"", Tok.txt "; pad = ", Tok.can "\"16px\""])
      = [Tok.txt "color = ", Tok.can "\"#9ca3af\"", Tok.txt "; pad = ", Tok.can "\"16px\""] := by
  decide

/-- The example table really is inverse-consistent, so the hypothesis of
    `roundtrip_code` is satisfiable (not vacuously true). Proved by evaluating
    the finite reverse lookups. -/
theorem example_inverse_consistent : InverseConsistent exampleDict := by
  intro a c h
  -- `rlookup` on the concrete 2-entry table decides on `c`; case-split the
  -- nested `ite`s with `split`.
  simp only [exampleDict, rlookup] at h
  split at h
  · rename_i hc
    subst hc
    simp only [Option.some.injEq] at h
    subst h
    decide
  · split at h
    · rename_i hc
      subst hc
      simp only [Option.some.injEq] at h
      subst h
      decide
    · simp at h


/-! ## Binding the model to the shipping compiler (gap-closure 2026-07-10)

Everything above quantifies over an abstract dictionary. This section
instantiates it with `committedDict` — GENERATED from
`crates/pyths_expand/src/strings.rs` (see `gen-dict-data.py`; CI fails on
drift) — and (a) DECIDES the round-trip hypothesis on the real table
(the Move-borrow-checker guard against vacuous theorems), (b) composes
the lemmas into one characterization theorem, and (c) provides an
executable char-level expander used by the model-vs-implementation
differential harness (`diff_harness.py`). -/

/-- Decidable checker for `InverseConsistent` — one total Boolean
    function over the finite table (the Move recipe: decide the
    hypothesis, prove the decision procedure sound). -/
def invConsistentB (D : Dict) : Bool :=
  D.all fun (ac : Alias × Canon) =>
    match rlookup D ac.2 with
    | some a' => elookup D a' == some ac.2
    | none => true

/-- A successful reverse lookup returns an entry of the table. -/
theorem rlookup_mem (D : Dict) (a : Alias) (c : Canon)
    (h : rlookup D c = some a) : (a, c) ∈ D := by
  induction D with
  | nil => simp [rlookup] at h
  | cons hd t ih =>
    obtain ⟨a0, c0⟩ := hd
    simp only [rlookup] at h
    split at h
    · rename_i hc
      cases h
      exact hc ▸ List.mem_cons_self ..
    · exact List.mem_cons_of_mem _ (ih h)

/-- Soundness: if the Boolean checker accepts, the table is
    inverse-consistent. -/
theorem invConsistentB_sound (D : Dict) (h : invConsistentB D = true) :
    InverseConsistent D := by
  intro a c hr
  have hmem : (a, c) ∈ D := rlookup_mem D a c hr
  have hall := List.all_eq_true.mp h (a, c) hmem
  simp only at hall
  rw [hr] at hall
  exact eq_of_beq hall

/-- **The committed table satisfies the round-trip hypothesis** — decided
    on the real 61-entry dictionary, so `roundtrip_code` is non-vacuous
    for the shipping compiler. -/
theorem committed_inverse_consistent : InverseConsistent committedDict :=
  invConsistentB_sound _ (by decide)

/-- Round-trip on the SHIPPING dictionary: `expand ∘ compress = id` on
    canonical (sigil-free) code. No abstract hypothesis remains. -/
theorem committed_roundtrip (xs : List Tok)
    (hdom : ∀ t ∈ xs, t.isAli = false) :
    expandCode committedDict (compressCode committedDict xs) = xs :=
  roundtrip_code _ committed_inverse_consistent xs hdom

/-! ## Characterization theorem (§7.3)

One statement a reader can take away: for ANY tier configuration and any
zone-classified source, expansion (a) is exactly the fixed
E→A→B→hooks→Dict composition, (b) maps every segment — nothing dropped
or invented, and (c) preserves every protected payload byte-for-byte;
and on the committed dictionary, compression round-trips on canonical
code. -/

theorem expand_characterization (c : Config) (src : Source) :
    expand c src
      = applyTier c.tierDict (applyTier c.tierHooks (applyTier c.tierB
          (applyTier c.tierA (applyTier c.tierE src))))
    ∧ (expand c src).length = src.length
    ∧ protPayloads (expand c src) = protPayloads src :=
  ⟨expand_order c src, expand_length c src, expand_zone_safety c src⟩

/-! ## Executable char-level expander (the differential-harness subject)

A faithful port of `strings.rs::substitute_with_dict` (empty user dict):
the zone classifier (single/triple-string + comment state machine, with
escapes) fused with `$NAME` lookup.

EPISTEMIC STATUS (updated 2026-07-14 — x18 closed). This function is now
**proof-covered**: see "CLASSIFIER ZONE-SAFETY" below, where
`zone_safety_chars` and `expandDictChars_prot_contiguous` are proved about
THIS function, unmodified. What remains outside the proofs is only the
*refinement* claim — that the Rust byte scanner in `strings.rs` implements
this same algorithm over UTF-8 bytes. That is established by the
model-vs-implementation differential (`diff_harness.py`, run in CI), which
compares this function byte-for-byte against the real `pyths expand` on a
generated corpus. Consequently **do not change this definition** without
re-running the differential: the proofs and the binding both range over it.

Fuel-based recursion: fuel ≥ steps always holds for the `length + 1` budget
`expandDictStr` supplies; on exhaustion the remainder is emitted verbatim
(never reached in practice — and the differential would catch it if it were).
The zone-safety theorems hold at EVERY fuel value, so they carry no fuel
side-condition: at exhaustion the remainder is emitted verbatim, which is
exactly what a protected zone requires. -/

inductive ScanState where
  | single (q : Char)
  | triple (q : Char)
  | comment

def isIdentChar (c : Char) : Bool := c.isAlphanum || c = '_'

def takeIdent : List Char → List Char × List Char
  | [] => ([], [])
  | c :: rest =>
    if isIdentChar c then
      let (id, rest') := takeIdent rest
      (c :: id, rest')
    else ([], c :: rest)

/-! ## THE SCANNER SKELETON — one zone state machine, five instantiations
    (dedup refactor, 2026-07-17)

Every tier of the shipped expander is the SAME scanner: the zone state
machine of `zones.rs` (single-quoted / double-quoted / triple-quoted /
`#`-comment, with escapes), differing ONLY in what it does to an ordinary
code character. This section states that fact as code, once:

  * `ScanSpec σ` — a tier, as data: its code-zone step (`codeStep`, the
    "rewrite function shape") plus how its extra code-state `σ` reacts to
    zone entry/exit. The alias TABLE lives inside `codeStep` and never
    enters any proof.
  * `scanChars sp` — THE generic scanner: the zone arms, written once.
  * `protScan sp` — THE generic companion classifier: the same traversal,
    recording exactly the characters consumed in a protected state.

The ladder of per-tier lemmas that used to be duplicated 4× (classifier
`sublist_input` → char-level `zone_safety` → contiguity) is proved ONCE,
generically, below; the per-tier theorems (`protChars_sublist_input`,
`zone_safety_chars`, `kwarg_zone_safety_chars`, `hook_zone_safety_chars`,
`idiom_zone_safety_chars`, `tierA_zone_safety_chars`, …) keep their exact
names and statements and become instantiations. A tier's proof obligations
reduce to two facts about its `codeStep` alone: it never invents input
(`*_rest_sublist`), and — for a tier that reuses another tier's classifier,
as Tier A reuses the dict's — it consumes input in lockstep with it
(`tierAStep_rest_eq_dictStep`).

Zone-state discipline of the skeleton: the code-state `σ` is CARRIED
through zone arms and reset at zone ENTRY (`enterStr` / `enterComment`,
plus `exitComment` for the one tier whose comment-exit state differs —
Tier A returns to `lineStart` after a comment's `\n`). The previous
hand-written scanners reset the flags inside every zone arm instead;
from the `String` entry points (code state, initial flags) the two
disciplines are byte-identical — the per-tier `#guard` fixtures, the
decided discipline theorems and the 2,039-case differential all pin
this — and the kwargs scanner already carried its flag through zones,
which is the semantics the skeleton adopts uniformly. -/

/-- The result of one code-zone step of a tier scanner: the characters to
    emit, the tier's next code-state, and the input remainder to continue
    on. -/
structure CodeOut (σ : Type) where
  emit  : List Char
  state : σ
  rest  : List Char

/-- A tier scanner, as data. The zone arms are shared (they live in
    `scanChars` / `protScan`, once); a tier contributes only its code-zone
    step and how its extra code-state reacts to zone entry/exit. -/
structure ScanSpec (σ : Type) where
  enterStr     : σ → σ
  enterComment : σ → σ
  exitComment  : σ → σ
  codeStep     : σ → Char → List Char → CodeOut σ

/-- THE generic scanner: one copy of the zone state machine. Fuel-based, as
    before: on exhaustion the remainder is emitted verbatim (which is what a
    protected zone requires, so the theorems carry no fuel side-condition). -/
def scanChars {σ : Type} (sp : ScanSpec σ) :
    Nat → Option ScanState → σ → List Char → List Char
  | 0, _, _, rest => rest
  | _ + 1, _, _, [] => []
  | fuel + 1, some (.single q), s, c :: rest =>
    if c = '\\' then
      match rest with
      | c2 :: rest2 => c :: c2 :: scanChars sp fuel (some (.single q)) s rest2
      | [] => [c]
    else if c = q then
      c :: scanChars sp fuel none s rest
    else
      c :: scanChars sp fuel (some (.single q)) s rest
  | fuel + 1, some (.triple q), s, c :: rest =>
    if c = '\\' then
      match rest with
      | c2 :: rest2 => c :: c2 :: scanChars sp fuel (some (.triple q)) s rest2
      | [] => [c]
    else if c = q then
      match rest with
      | c2 :: c3 :: rest3 =>
        if c2 = q && c3 = q then
          c :: c2 :: c3 :: scanChars sp fuel none s rest3
        else
          c :: scanChars sp fuel (some (.triple q)) s rest
      | _ => c :: scanChars sp fuel (some (.triple q)) s rest
    else
      c :: scanChars sp fuel (some (.triple q)) s rest
  | fuel + 1, some .comment, s, c :: rest =>
    if c = '\n' then
      c :: scanChars sp fuel none (sp.exitComment s) rest
    else
      c :: scanChars sp fuel (some .comment) s rest
  | fuel + 1, none, s, c :: rest =>
    if c = '#' then
      c :: scanChars sp fuel (some .comment) (sp.enterComment s) rest
    else if c = '\'' || c = '"' then
      match rest with
      | c2 :: c3 :: rest3 =>
        if c2 = c && c3 = c then
          c :: c2 :: c3 :: scanChars sp fuel (some (.triple c)) (sp.enterStr s) rest3
        else
          c :: scanChars sp fuel (some (.single c)) (sp.enterStr s) rest
      | _ => c :: scanChars sp fuel (some (.single c)) (sp.enterStr s) rest
    else
      (sp.codeStep s c rest).emit
        ++ scanChars sp fuel none (sp.codeStep s c rest).state (sp.codeStep s c rest).rest

/-- THE generic classifier: the SAME traversal, arm for arm — same guards,
    same state transitions, same consumption — recording the characters
    consumed in a protected state and nothing else. At fuel exhaustion in a
    protected state the whole remainder is (soundly) classified protected;
    in code state nothing is claimed. -/
def protScan {σ : Type} (sp : ScanSpec σ) :
    Nat → Option ScanState → σ → List Char → List Char
  | 0, st, _, rest => if st.isSome then rest else []
  | _ + 1, _, _, [] => []
  | fuel + 1, some (.single q), s, c :: rest =>
    if c = '\\' then
      match rest with
      | c2 :: rest2 => c :: c2 :: protScan sp fuel (some (.single q)) s rest2
      | [] => [c]
    else if c = q then
      c :: protScan sp fuel none s rest
    else
      c :: protScan sp fuel (some (.single q)) s rest
  | fuel + 1, some (.triple q), s, c :: rest =>
    if c = '\\' then
      match rest with
      | c2 :: rest2 => c :: c2 :: protScan sp fuel (some (.triple q)) s rest2
      | [] => [c]
    else if c = q then
      match rest with
      | c2 :: c3 :: rest3 =>
        if c2 = q && c3 = q then
          c :: c2 :: c3 :: protScan sp fuel none s rest3
        else
          c :: protScan sp fuel (some (.triple q)) s rest
      | _ => c :: protScan sp fuel (some (.triple q)) s rest
    else
      c :: protScan sp fuel (some (.triple q)) s rest
  | fuel + 1, some .comment, s, c :: rest =>
    if c = '\n' then
      c :: protScan sp fuel none (sp.exitComment s) rest
    else
      c :: protScan sp fuel (some .comment) s rest
  | fuel + 1, none, s, c :: rest =>
    if c = '#' then
      protScan sp fuel (some .comment) (sp.enterComment s) rest
    else if c = '\'' || c = '"' then
      match rest with
      | c2 :: c3 :: rest3 =>
        if c2 = c && c3 = c then
          protScan sp fuel (some (.triple c)) (sp.enterStr s) rest3
        else
          protScan sp fuel (some (.single c)) (sp.enterStr s) rest
      | _ => protScan sp fuel (some (.single c)) (sp.enterStr s) rest
    else
      protScan sp fuel none (sp.codeStep s c rest).state (sp.codeStep s c rest).rest

/-- The scanner emits nothing on empty input, at every fuel. -/
theorem scanChars_nil {σ : Type} (sp : ScanSpec σ) (fuel : Nat)
    (st : Option ScanState) (s : σ) : scanChars sp fuel st s [] = [] := by
  cases fuel <;> rfl

/-- The classifier reports nothing on empty input, at every fuel. -/
theorem protScan_nil {σ : Type} (sp : ScanSpec σ) (fuel : Nat)
    (st : Option ScanState) (s : σ) : protScan sp fuel st s [] = [] := by
  cases fuel with
  | zero => simp [protScan]
  | succ n => rfl

/-- Sublist through a `cons` on the right. -/
theorem sub_cons {l₁ l₂ : List Char} (a : Char) (h : List.Sublist l₁ l₂) :
    List.Sublist l₁ (a :: l₂) := h.cons a

/-- Sublist through an append on the left (a tier emits a canonical
    expansion in front of the recursive result). -/
theorem sub_app {l₁ l₂ : List Char} (pre : List Char) (h : List.Sublist l₁ l₂) :
    List.Sublist l₁ (pre ++ l₂) :=
  h.trans (List.sublist_append_right _ _)

/-! ### The generic ladder, proved ONCE -/

/-- **Generic classifier soundness**: the classifier only ever selects
    characters of the input, in order — it cannot invent a character and
    call it "protected" — provided the tier's code step never invents
    input (`hsub`, discharged per tier on its `codeStep` alone). -/
theorem protScan_sublist_input {σ : Type} (sp : ScanSpec σ)
    (hsub : ∀ s c rest, List.Sublist (sp.codeStep s c rest).rest (c :: rest)) :
    ∀ (fuel : Nat) (st : Option ScanState) (s : σ) (cs : List Char),
    List.Sublist (protScan sp fuel st s cs) cs := by
  intro fuel
  induction fuel with
  | zero =>
    intro st s cs
    simp only [protScan]
    split <;> simp
  | succ fuel ih =>
    intro st s cs
    match st, cs with
    | _, [] => simp [protScan_nil]
    | none, c :: rest
    | some (.single _), c :: rest
    | some (.triple _), c :: rest
    | some .comment, c :: rest =>
      simp only [protScan]
      repeat' split
      all_goals
        repeat (first
          | exact (ih _ _ _).trans (hsub _ _ _)
          | exact List.Sublist.refl _
          | exact List.nil_sublist _
          | apply List.Sublist.cons_cons
          | exact ih _ _ _
          | apply List.Sublist.cons
          | assumption)

/-- **Generic zone-safety, as a simulation.** The classifier of `sp₁` is a
    sublist of the scanner of `sp₂`, for ANY pair of specs whose code steps
    consume the input in lockstep (code-states related by `R`, preserved by
    every zone transition and by the code step, with equal consumption).

    Instantiated two ways: `sp₁ = sp₂, R = Eq` gives each tier's zone-safety
    against its own classifier; `sp₁ = dict, sp₂ = tierA, R = ⊤` gives
    Tier A's zone-safety against the ONE shared classifier `protChars`. -/
theorem protScan_sublist_scanChars_R {σ₁ σ₂ : Type}
    (sp₁ : ScanSpec σ₁) (sp₂ : ScanSpec σ₂) (R : σ₁ → σ₂ → Prop)
    (henterStr : ∀ {s1 s2}, R s1 s2 → R (sp₁.enterStr s1) (sp₂.enterStr s2))
    (henterC : ∀ {s1 s2}, R s1 s2 → R (sp₁.enterComment s1) (sp₂.enterComment s2))
    (hexitC : ∀ {s1 s2}, R s1 s2 → R (sp₁.exitComment s1) (sp₂.exitComment s2))
    (hcode : ∀ {s1 s2} (c : Char) (rest : List Char), R s1 s2 →
        (sp₁.codeStep s1 c rest).rest = (sp₂.codeStep s2 c rest).rest
        ∧ R (sp₁.codeStep s1 c rest).state (sp₂.codeStep s2 c rest).state) :
    ∀ (fuel : Nat) (st : Option ScanState) (s1 : σ₁) (s2 : σ₂) (cs : List Char),
      R s1 s2 →
      List.Sublist (protScan sp₁ fuel st s1 cs) (scanChars sp₂ fuel st s2 cs) := by
  intro fuel
  induction fuel with
  | zero =>
    intro st s1 s2 cs _
    simp only [protScan, scanChars]
    split <;> simp
  | succ fuel ih =>
    intro st s1 s2 cs hR
    match st, cs with
    | _, [] => simp [protScan_nil, scanChars_nil]
    | none, c :: rest
    | some (.single _), c :: rest
    | some (.triple _), c :: rest
    | some .comment, c :: rest =>
      simp only [protScan, scanChars]
      repeat' split
      all_goals
        (try (obtain ⟨heq, hR'⟩ := hcode c rest hR
              rw [heq]
              exact sub_app _ (ih _ _ _ _ hR')))
      all_goals
        repeat (first
          | exact List.Sublist.refl _
          | exact List.nil_sublist _
          | apply List.Sublist.cons_cons
          | exact ih _ _ _ _ hR
          | exact ih _ _ _ _ (henterC hR)
          | exact ih _ _ _ _ (henterStr hR)
          | exact ih _ _ _ _ (hexitC hR)
          | apply List.Sublist.cons
          | assumption)

/-- **Generic zone-safety** of one spec against itself: every character the
    scanner consumes in a protected state is emitted verbatim, in order. -/
theorem protScan_sublist_scanChars {σ : Type} (sp : ScanSpec σ)
    (fuel : Nat) (st : Option ScanState) (s : σ) (cs : List Char) :
    List.Sublist (protScan sp fuel st s cs) (scanChars sp fuel st s cs) :=
  protScan_sublist_scanChars_R sp sp Eq
    (fun h => by rw [h]) (fun h => by rw [h]) (fun h => by rw [h])
    (fun _ _ h => by rw [h]; exact ⟨rfl, rfl⟩)
    fuel st s s cs rfl

set_option linter.unusedSimpArgs false in  -- `hid` IS used, in the codeStep arm
/-- **Generic identity**: a spec whose code step re-emits exactly what it
    consumes makes the scanner the identity (used for the empty idiom
    table — the zero-config Tier E). -/
theorem scanChars_id {σ : Type} (sp : ScanSpec σ)
    (hid : ∀ s c rest,
      (sp.codeStep s c rest).emit ++ (sp.codeStep s c rest).rest = c :: rest) :
    ∀ (fuel : Nat) (st : Option ScanState) (s : σ) (cs : List Char),
    scanChars sp fuel st s cs = cs := by
  intro fuel
  induction fuel with
  | zero => intro st s cs; rfl
  | succ fuel ih =>
    intro st s cs
    match st, cs with
    | _, [] => simp [scanChars_nil]
    | none, c :: rest
    | some (.single _), c :: rest
    | some (.triple _), c :: rest
    | some .comment, c :: rest =>
      simp only [scanChars]
      repeat' split
      all_goals simp_all [hid]

/-! ### Generic contiguity: a protected zone is copied as one block

`scanProt` runs the (shared) protected arms of the scanner to the end of
the zone, returning `(zoneChars, remainder, leftoverFuel)`. It has no `σ`
and no spec parameter — zone traversal is tier-independent, which is the
whole point of the skeleton. -/

/-- Run the scanner's protected arms to the end of the zone.
    Returns (characters consumed in-zone, input remainder, fuel left). -/
def scanProt : Nat → ScanState → List Char → List Char × List Char × Nat
  | 0, _, rest => (rest, [], 0)
  | _ + 1, _, [] => ([], [], 0)
  | fuel + 1, .single q, c :: rest =>
    if c = '\\' then
      match rest with
      | c2 :: rest2 =>
        let r := scanProt fuel (.single q) rest2
        (c :: c2 :: r.1, r.2)
      | [] => ([c], [], 0)
    else if c = q then ([c], rest, fuel)
    else
      let r := scanProt fuel (.single q) rest
      (c :: r.1, r.2)
  | fuel + 1, .triple q, c :: rest =>
    if c = '\\' then
      match rest with
      | c2 :: rest2 =>
        let r := scanProt fuel (.triple q) rest2
        (c :: c2 :: r.1, r.2)
      | [] => ([c], [], 0)
    else if c = q then
      match rest with
      | c2 :: c3 :: rest3 =>
        if c2 = q && c3 = q then ([c, c2, c3], rest3, fuel)
        else
          let r := scanProt fuel (.triple q) rest
          (c :: r.1, r.2)
      | _ =>
        let r := scanProt fuel (.triple q) rest
        (c :: r.1, r.2)
    else
      let r := scanProt fuel (.triple q) rest
      (c :: r.1, r.2)
  | fuel + 1, .comment, c :: rest =>
    if c = '\n' then ([c], rest, fuel)
    else
      let r := scanProt fuel .comment rest
      (c :: r.1, r.2)

/-- The zone split is a split: in-zone characters ++ remainder = input. -/
theorem scanProt_split (fuel : Nat) (st : ScanState) (cs : List Char) :
    (scanProt fuel st cs).1 ++ (scanProt fuel st cs).2.1 = cs := by
  induction fuel, st, cs using scanProt.induct <;>
    (try simp_all [scanProt]) <;>
    (try split) <;>
    (try simp_all)

/-- The tier code-state at the end of a protected zone: a comment hands the
    tier its `exitComment` state, a string zone hands back the entry state. -/
def exitState {σ : Type} (sp : ScanSpec σ) : ScanState → σ → σ
  | .comment, s => sp.exitComment s
  | _, s => s

/-- **Generic contiguity, scanner side**: started inside a protected zone,
    the scanner emits the zone's characters as an uninterrupted,
    byte-identical block, and only then resumes code scanning. -/
theorem scanChars_scanProt {σ : Type} (sp : ScanSpec σ) (fuel : Nat)
    (st : ScanState) (s : σ) (cs : List Char) :
    scanChars sp fuel (some st) s cs
      = (scanProt fuel st cs).1
        ++ scanChars sp (scanProt fuel st cs).2.2 none (exitState sp st s)
             (scanProt fuel st cs).2.1 := by
  induction fuel, st, cs using scanProt.induct <;>
    (try simp_all [scanProt, scanChars, exitState]) <;>
    (try split) <;>
    (try simp_all)

/-- **Generic contiguity, classifier side**: the classifier splits the same
    way, so the block `scanProt` returns IS exactly the characters
    classified protected in that zone. -/
theorem protScan_scanProt {σ : Type} (sp : ScanSpec σ) (fuel : Nat)
    (st : ScanState) (s : σ) (cs : List Char) :
    protScan sp fuel (some st) s cs
      = (scanProt fuel st cs).1
        ++ protScan sp (scanProt fuel st cs).2.2 none (exitState sp st s)
             (scanProt fuel st cs).2.1 := by
  induction fuel, st, cs using scanProt.induct <;>
    (try simp_all [scanProt, protScan, exitState]) <;>
    (try split) <;>
    (try simp_all)

/-- Dict-tier code step: the `$NAME` lookup — the ONLY thing the dict tier
    adds to the shared zone machine. Known alias → `em canon` emitted, the
    identifier consumed; unknown or bare `$` → emitted verbatim.

    Parameterized over the emission `em` so that the CLASSIFIER's spec
    (`em := fun _ => []` — it never emits) does not reference
    `String.toList` and `protChars` keeps its `propext`-only trust base
    (`String.toList` carries `Classical.choice`/`Quot.sound` from Lean
    core; the scanner's spec pays that, the classifier's must not).
    Consumption is `em`-independent (`dictStep_rest_eq`), which is what
    keeps the two specs in lockstep. -/
def dictStep (em : Canon → List Char) (D : Dict) (_ : Unit) (c : Char)
    (rest : List Char) : CodeOut Unit :=
  if c = '$' then
    match rest with
    | c2 :: _ =>
      if isIdentChar c2 then
        match elookup D (String.ofList (takeIdent rest).1) with
        | some canon => ⟨em canon, (), (takeIdent rest).2⟩
        | none => ⟨[c], (), rest⟩
      else ⟨[c], (), rest⟩
    | [] => ⟨[c], (), []⟩
  else ⟨[c], (), rest⟩

/-- The dict tier as a `ScanSpec`: no extra code-state. -/
def dictSpec (D : Dict) : ScanSpec Unit :=
  ⟨fun s => s, fun s => s, fun s => s, dictStep String.toList D⟩

/-- The dict CLASSIFIER's spec: same consumption, no emission — so its
    constant graph is axiom-free (see `dictStep`'s docstring). -/
def dictProtSpec (D : Dict) : ScanSpec Unit :=
  ⟨fun s => s, fun s => s, fun s => s, dictStep (fun _ => []) D⟩

/-- The dict code step consumes the input the same way whatever it emits. -/
theorem dictStep_rest_eq (em em' : Canon → List Char) (D : Dict) (s s' : Unit)
    (c : Char) (rest : List Char) :
    (dictStep em D s c rest).rest = (dictStep em' D s' c rest).rest := by
  unfold dictStep
  repeat' split
  all_goals rfl

def expandDictChars (D : Dict) : Nat → Option ScanState → List Char → List Char :=
  fun fuel st cs => scanChars (dictSpec D) fuel st () cs

/-- Expand `$NAME` aliases in a source string, zone-aware — the
    executable model of the dictionary tier. -/
def expandDictStr (D : Dict) (s : String) : String :=
  String.ofList (expandDictChars D (s.length + 1) none s.toList)

-- Executable smoke checks (kernel-reduced; also serve as documentation).
#guard expandDictStr committedDict "x = $pad" = "x = \"padding\""
#guard expandDictStr committedDict "s = \"$pad\"" = "s = \"$pad\""
#guard expandDictStr committedDict "# $pad" = "# $pad"
#guard expandDictStr committedDict "t = '''$c1'''" = "t = '''$c1'''"
#guard expandDictStr committedDict "$unknown + $p1" = "$unknown + \"12px\""


/-! ## CLASSIFIER ZONE-SAFETY — the proof that closes x18 (2026-07-14)

Everything above proves zone-safety **at the segment level**: given a
zone classification (`Source = List Segment`), no tier touches a `prot`
payload. That left the classifier ITSELF outside the proof: the segment
model *assumes* the scanner correctly identifies string / comment /
triple-quote zones. That assumption was previously discharged only by
the differential harness — the stated gap x18.

This section discharges it by proof, **over the very same executable
function** (`expandDictChars`) that `expandDictStr` — and hence the
`expanddiff` driver and `diff_harness.py` — runs. `expandDictChars` is
NOT modified, redefined, or re-modelled here; the theorems below range
over it verbatim, so the differential binding is preserved unchanged.

The device is a companion *classifier* `protChars`: a second traversal
with the **same** state-transition structure, which returns exactly the
characters the scanner consumes while its state is protected
(`some (.single q)` / `some (.triple q)` / `some .comment`). We then
prove that this list of characters is emitted verbatim.

Two theorems, of increasing strength:

* `zone_safety_chars` — **global**: `List.Sublist (protChars …) (expandDictChars …)`.
  Every character consumed in a protected zone appears in the output,
  in order, unchanged. Char-generic, so it covers `$`, `%`, and any
  future sigil.
* `expandDictChars_prot_contiguous` — **local/contiguous**: entering a
  protected zone, the output *begins* with a byte-exact copy of the
  input prefix consumed inside that zone, and only then resumes code
  scanning. So the zone is not merely preserved as a subsequence — it
  is copied as an uninterrupted block.

What this does NOT prove: that the Rust `strings.rs` byte scanner
refines `expandDictChars`. That remains a differential-testing claim
(`diff_harness.py`, CI). See README §"Honest gap statement". -/

/-- Characters consumed while the scan state is PROTECTED (single-quoted,
    triple-quoted, or comment). A faithful companion of `expandDictChars`:
    every constructor arm below mirrors the corresponding arm of
    `expandDictChars` — same guards, same state transitions, same number of
    characters consumed — and simply records the characters consumed in a
    protected state while recording none of those consumed in code state.

    At fuel exhaustion `expandDictChars` emits the remainder verbatim, so
    when the state is protected the whole remainder is (soundly) classified
    protected; in code state nothing is claimed.

    Since the 2026-07-17 dedup refactor this is `protScan` over
    `dictProtSpec` — the same `dictStep` consumption the scanner's
    `dictSpec` runs (only the emission differs, and consumption is
    `em`-independent: `dictStep_rest_eq`) — so lockstep is by construction,
    not by hand-mirrored arms. -/
def protChars (D : Dict) : Nat → Option ScanState → List Char → List Char :=
  fun fuel st cs => protScan (dictProtSpec D) fuel st () cs

/-- `takeIdent` splits its input: identifier prefix ++ remainder. -/
theorem takeIdent_append (cs : List Char) :
    (takeIdent cs).1 ++ (takeIdent cs).2 = cs := by
  induction cs with
  | nil => rfl
  | cons c rest ih =>
    simp only [takeIdent]
    split
    · simp only [List.cons_append, List.cons.injEq, true_and]
      exact ih
    · rfl

/-- Past an identifier character, `takeIdent`'s remainder is the remainder of
    the tail — the scanner strictly advances over `$NAME`. -/
theorem takeIdent_cons_snd (c : Char) (rest : List Char) (h : isIdentChar c = true) :
    (takeIdent (c :: rest)).2 = (takeIdent rest).2 := by
  simp [takeIdent, h]

/-- Hence the remainder is a sublist (indeed a suffix) of the input. -/
theorem takeIdent_snd_sublist (cs : List Char) :
    List.Sublist (takeIdent cs).2 cs := by
  have h : List.Sublist (takeIdent cs).2 ((takeIdent cs).1 ++ (takeIdent cs).2) :=
    List.sublist_append_right _ _
  rw [takeIdent_append cs] at h
  exact h

/-- Sublist through a cons on the right (list-level convenience). -/
theorem sublist_cons_self' (a : Char) (l : List Char) : List.Sublist l (a :: l) := by
  simp

/-- `takeIdent`, given as an equation, exposes its remainder as a sublist. -/
theorem takeIdent_snd_eq_sublist {cs id rest' : List Char}
    (h : takeIdent cs = (id, rest')) : List.Sublist rest' cs := by
  have hs := takeIdent_snd_sublist cs
  rw [h] at hs
  exact hs

/-- `takeIdent`, given as an equation, reassembles its input. -/
theorem takeIdent_eq_append {cs id rest' : List Char}
    (h : takeIdent cs = (id, rest')) : id ++ rest' = cs := by
  have ha := takeIdent_append cs
  rw [h] at ha
  exact ha

/-- The dict code step never invents input (its side of the generic
    `hsub` obligation — the whole per-tier proof burden of the skeleton). -/
theorem dictStep_rest_sublist (em : Canon → List Char) (D : Dict) (s : Unit)
    (c : Char) (rest : List Char) :
    List.Sublist (dictStep em D s c rest).rest (c :: rest) := by
  unfold dictStep
  repeat' split
  all_goals first
    | exact List.nil_sublist _
    | exact (List.Sublist.refl _).cons _
    | exact (takeIdent_snd_sublist _).cons _
    | exact List.Sublist.refl _

/-- The classifier only ever selects characters of the input, in order — it
    cannot invent a character and call it "protected". Together with
    `zone_safety_chars`, this is what makes "verbatim" meaningful: the SAME
    characters, taken from the input, appear in the output. -/
theorem protChars_sublist_input (D : Dict) (fuel : Nat) (st : Option ScanState)
    (cs : List Char) : List.Sublist (protChars D fuel st cs) cs :=
  protScan_sublist_input (dictProtSpec D) (dictStep_rest_sublist (fun _ => []) D)
    fuel st () cs

/-- **THE CLASSIFIER ZONE-SAFETY THEOREM (x18).**

    Every character the scanner consumes while its state is protected —
    inside `'…'`, `"…"`, `'''…'''`, `"""…"""`, or a `#` comment — is
    emitted by `expandDictChars` verbatim, in order.

    The statement ranges over `expandDictChars` ITSELF: the same function
    `expandDictStr` calls, the same one the `expanddiff` driver runs against
    `pyths expand` in `diff_harness.py`. It is char-generic, so it covers the
    `$` dictionary sigil, the `%` idiom sigil, and any sigil added later. -/
theorem zone_safety_chars (D : Dict) (fuel : Nat) (st : Option ScanState)
    (cs : List Char) :
    List.Sublist (protChars D fuel st cs) (expandDictChars D fuel st cs) :=
  protScan_sublist_scanChars_R (dictProtSpec D) (dictSpec D) (fun _ _ => True)
    (fun _ => trivial) (fun _ => trivial) (fun _ => trivial)
    (fun c rest _ => ⟨dictStep_rest_eq _ _ D () () c rest, trivial⟩)
    fuel st () () cs trivial

/-! ### Contiguity: a protected zone is copied as an uninterrupted block

`zone_safety_chars` says the protected characters survive as a *subsequence*.
That already implies "the sigil appears in the output unexpanded", but it
leaves a pedantic loophole: a subsequence could in principle be interleaved
with rewritten material. We close it.

`scanProt` (defined with the skeleton above) runs the protected arms of the
scanner to the end of the zone, returning `(zoneChars, remainder,
leftoverFuel)`. The generic `scanChars_scanProt` / `protScan_scanProt` prove
the split once; here they are instantiated at the dict spec. -/

/-- **Contiguous verbatim emission.** Started inside a protected zone,
    `expandDictChars` emits the zone's characters as an uninterrupted,
    byte-identical block, and only then resumes scanning in code state. -/
theorem expandDictChars_scanProt (D : Dict) (fuel : Nat) (st : ScanState)
    (cs : List Char) :
    expandDictChars D fuel (some st) cs
      = (scanProt fuel st cs).1
        ++ expandDictChars D (scanProt fuel st cs).2.2 none (scanProt fuel st cs).2.1 :=
  scanChars_scanProt (dictSpec D) fuel st () cs

/-- …and the classifier splits identically, so the block `scanProt` returns
    IS exactly the characters classified protected in that zone. -/
theorem protChars_scanProt (D : Dict) (fuel : Nat) (st : ScanState)
    (cs : List Char) :
    protChars D fuel (some st) cs
      = (scanProt fuel st cs).1
        ++ protChars D (scanProt fuel st cs).2.2 none (scanProt fuel st cs).2.1 :=
  protScan_scanProt (dictProtSpec D) fuel st () cs

/-- The headline packaging of the two above: entering a protected zone, the
    input splits as `pre ++ suf`, the output is `pre` **verbatim** followed by
    ordinary code scanning of `suf`, and `pre` is precisely what the classifier
    calls protected. -/
theorem expandDictChars_prot_contiguous (D : Dict) (fuel : Nat) (st : ScanState)
    (cs : List Char) :
    ∃ (pre suf : List Char) (f' : Nat),
      cs = pre ++ suf
      ∧ expandDictChars D fuel (some st) cs = pre ++ expandDictChars D f' none suf
      ∧ protChars D fuel (some st) cs = pre ++ protChars D f' none suf :=
  ⟨(scanProt fuel st cs).1, (scanProt fuel st cs).2.1, (scanProt fuel st cs).2.2,
    (scanProt_split fuel st cs).symm,
    expandDictChars_scanProt D fuel st cs,
    protChars_scanProt D fuel st cs⟩

/-! ### Sigil corollaries — the statement the 300-case property test asserts -/

/-- Any character consumed inside a protected zone occurs in the output. -/
theorem protected_char_in_output (D : Dict) (fuel : Nat) (st : Option ScanState)
    (cs : List Char) (c : Char) (h : c ∈ protChars D fuel st cs) :
    c ∈ expandDictChars D fuel st cs :=
  (zone_safety_chars D fuel st cs).mem h

/-- **The `$` sigil in a protected zone is emitted verbatim.** -/
theorem dollar_in_zone_verbatim (D : Dict) (fuel : Nat) (st : Option ScanState)
    (cs : List Char) (h : '$' ∈ protChars D fuel st cs) :
    '$' ∈ expandDictChars D fuel st cs :=
  protected_char_in_output D fuel st cs '$' h

/-- **The `%` sigil in a protected zone is emitted verbatim.** In the
    dictionary scanner `%` is an ordinary character; the theorem is
    char-generic, so it holds of every sigil the scanner does not
    special-case — which is exactly what must hold for a Tier-E `%NAME`
    idiom sigil inside a string literal to survive the dictionary pass. -/
theorem percent_in_zone_verbatim (D : Dict) (fuel : Nat) (st : Option ScanState)
    (cs : List Char) (h : '%' ∈ protChars D fuel st cs) :
    '%' ∈ expandDictChars D fuel st cs :=
  protected_char_in_output D fuel st cs '%' h

/-- Multiplicity, not just occurrence: EVERY protected `$` survives — the
    output has at least as many `$` as the zones contained. -/
theorem protected_dollars_count (D : Dict) (fuel : Nat) (st : Option ScanState)
    (cs : List Char) :
    (protChars D fuel st cs).count '$' ≤ (expandDictChars D fuel st cs).count '$' :=
  (zone_safety_chars D fuel st cs).count_le '$'

/-! ### String level — the SHIPPED entry point

`expandDictStr` is the function `Main.lean` exposes as `expanddiff`, i.e. the
one `diff_harness.py` diffs against `pyths expand`. The corollary therefore
speaks about the shipping model, not a paraphrase of it. -/

/-- The protected characters of a whole source string. -/
def protCharsStr (D : Dict) (s : String) : List Char :=
  protChars D (s.length + 1) none s.toList

/-- **Zone-safety of the shipped expander**: every character inside a string
    literal, triple-quoted string, or comment appears, in order and unchanged,
    in `expandDictStr`'s output. -/
theorem expandDictStr_zone_safety (D : Dict) (s : String) :
    List.Sublist (protCharsStr D s) (expandDictStr D s).toList := by
  simpa [protCharsStr, expandDictStr, String.toList_ofList] using
    zone_safety_chars D (s.length + 1) none s.toList

/-- A `$` inside a protected zone of the source survives into the output of
    the SHIPPED expander. -/
theorem expandDictStr_dollar_verbatim (D : Dict) (s : String)
    (h : '$' ∈ protCharsStr D s) : '$' ∈ (expandDictStr D s).toList :=
  (expandDictStr_zone_safety D s).mem h

/-! ### Non-vacuity of the classifier

`protChars` would satisfy `zone_safety_chars` trivially if it always returned
`[]`. These kernel-checked evaluations pin it down in BOTH directions on the
committed dictionary: it reports the zone contents (including the sigil) inside
each zone kind, and reports nothing in code position — where the sigil really
does expand. -/

-- Inside a double-quoted string: `$pad` is classified protected, and survives.
#guard protCharsStr committedDict "s = \"$pad\"" = "$pad\"".toList
#guard expandDictStr committedDict "s = \"$pad\"" = "s = \"$pad\""
example : '$' ∈ protCharsStr committedDict "s = \"$pad\"" := by decide

-- Single-quoted, comment, triple-quoted, and escape-bearing zones.
#guard protCharsStr committedDict "b = '$c1 x'" = "$c1 x'".toList
#guard protCharsStr committedDict "# $pad here" = " $pad here".toList
#guard protCharsStr committedDict "t = '''$c1'''" = "$c1'''".toList
#guard protCharsStr committedDict "e = \"a \\\" $pad\"" = "a \\\" $pad\"".toList
example : '$' ∈ protCharsStr committedDict "e = \"a \\\" $pad\"" := by decide

-- In CODE position nothing is protected — and the sigil really expands, so the
-- classifier is not merely tagging everything.
#guard protCharsStr committedDict "x = $pad" = []
#guard expandDictStr committedDict "x = $pad" = "x = \"padding\""

-- `%` is not a dictionary sigil, but inside a zone the theorem still applies.
#guard protCharsStr committedDict "m = \"%row\"" = "%row\"".toList
example : '%' ∈ protCharsStr committedDict "m = \"%row\"" := by decide

/-! ### Trust base of the new theorems — pinned as a BUILD GATE

Note the honest axiom accounting. The segment-level lemmas above are
propext-only. The classifier theorems are NOT, and cannot be: the shipped
`expandDictChars` uses Lean core's `String` (`String.ofList` for the alias
key, `String.toList` for the canonical value), and in Lean 4.31 `String`
itself depends on `Classical.choice` and `Quot.sound` — `#print axioms
expandDictChars` reports them for the UNMODIFIED definition, before any
proof is written. So the classifier theorems rest on exactly Lean's three
standard axioms (the base mathlib runs on): no proof holes, no
kernel-bypassing evaluation, no axiom of our own.

`#guard_msgs` turns that into a build-time gate: if anyone introduces a
proof hole or a new axiom, the printed axiom list changes and `lake build`
FAILS. The trust base is now enforced, not merely asserted. -/

/-- info: 'PythExpandVerify.expandDictChars' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms expandDictChars

/-- info: 'PythExpandVerify.zone_safety_chars' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms zone_safety_chars

/-- info: 'PythExpandVerify.expandDictChars_prot_contiguous' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms expandDictChars_prot_contiguous

/-- info: 'PythExpandVerify.expandDictStr_zone_safety' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms expandDictStr_zone_safety

/-- info: 'PythExpandVerify.protChars_sublist_input' depends on axioms: [propext] -/
#guard_msgs in #print axioms protChars_sublist_input

/-- info: 'PythExpandVerify.expand_zone_safety' depends on axioms: [propext] -/
#guard_msgs in #print axioms expand_zone_safety

/-- info: 'PythExpandVerify.committed_roundtrip' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms committed_roundtrip


/-! ## TIER B — a CONCRETE tier instantiation (2026-07-14)

Until now every tier was an opaque `Tier := String → String`. That makes
`zone_safety` / `expand_order` / `expand_length` hold for *any* function —
which is exactly why a reader may call them near-tautological: nothing is
said about what a tier actually DOES.

This section removes that objection for Tier B (kwarg-position aliases,
`crates/pyths_expand/src/kwargs.rs`). Tier B is instantiated as a concrete
executable Lean function over the SHIPPING alias table (`committedKwargs`,
generated from `kwargs.rs` by `gen-kwarg-data.py`, CI-gated against drift),
and the properties previously *assumed* of a tier are proved of it:

  * it is a total `String → String`, so `applyTier` maps each segment to
    exactly one segment (`tierB_length`);
  * it rewrites ONLY in code zones — now a char-level THEOREM about the
    scanner (`kwarg_zone_safety_chars`), not an assumption of the segment
    model;
  * its rewrite is EXACTLY the committed alias table — decided on the real
    9-entry table (`committed_kwargs_exact`), so the instantiation is
    non-vacuous, the same move as `committed_inverse_consistent` for the
    dictionary tier;
  * it fires ONLY in call-argument position, and never on `==`
    (`kwargs_position_discipline`, decided).

Faithfulness: `kwargs.rs` uses the same scanner family as `strings.rs` plus
one extra bit of state, `at_arg_start`, set by `(` / `,`, preserved across
whitespace (so a kwarg may sit on a continuation line), and cleared by a
comment, a string, or any other token. `expandKwargChars` threads exactly
that bit. As with the dictionary tier, the binding to the Rust code is the
model-vs-implementation differential (`diff_harness.py --tier kwargs`). -/

/-- Rust `is_identifier_start`: ASCII alphabetic or `_` (NOT a digit). -/
def isIdentStart (c : Char) : Bool := c.isAlpha || c = '_'

/-- Tier-B code step: the kwarg-alias rewrite, threading Rust's
    `at_arg_start` bit — whitespace PRESERVES it (multi-line argument
    lists), `(` / `,` set it, everything else clears it; an alias fires
    only at arg start, before a `=` that is not `==`. -/
def kwargStep (K : Dict) (a : Bool) (c : Char) (rest : List Char) : CodeOut Bool :=
  if c = ' ' || c = '\t' || c = '\r' || c = '\n' then
    ⟨[c], a, rest⟩
  else if c = '(' || c = ',' then
    ⟨[c], true, rest⟩
  else if a && isIdentStart c then
    let (idTail, rest') := takeIdent rest
    let ident := c :: idTail
    match rest' with
    | '=' :: rest2 =>
      -- `=` that is not part of `==`
      if rest2.head? = some '=' then
        ⟨ident, false, rest'⟩
      else
        match elookup K (String.ofList ident) with
        | some canon => ⟨canon.toList ++ ['='], false, rest2⟩
        | none => ⟨ident, false, rest'⟩
    | _ => ⟨ident, false, rest'⟩
  else ⟨[c], false, rest⟩

/-- Tier B as a `ScanSpec`: a comment or a string clears `at_arg_start`
    (as in `kwargs.rs`); the bit survives a zone (it is code-state). -/
def kwargSpec (K : Dict) : ScanSpec Bool :=
  ⟨fun _ => false, fun _ => false, fun s => s, kwargStep K⟩

def expandKwargChars (K : Dict) : Nat → Option ScanState → Bool → List Char → List Char :=
  fun fuel st a cs => scanChars (kwargSpec K) fuel st a cs

/-- Tier B on a source string — the concrete `Tier` we plug into `Config`. -/
def expandKwargStr (K : Dict) (s : String) : String :=
  String.ofList (expandKwargChars K (s.length + 1) none false s.toList)

/-- **THE concrete Tier B**: the shipping kwarg table, as a `Tier`. -/
def tierB : Tier := expandKwargStr committedKwargs

/-! ### Tier-B classifier and its zone-safety theorem

Same technique as the dictionary classifier: `protCharsK` mirrors
`expandKwargChars` arm-for-arm and returns exactly the characters consumed
while the scan state is protected. -/

def protCharsK (K : Dict) : Nat → Option ScanState → Bool → List Char → List Char :=
  fun fuel st a cs => protScan (kwargSpec K) fuel st a cs

/-- If `takeIdent`'s remainder is known to be `e :: tail`, that remainder is
    still a sublist of `c :: input`. -/
theorem takeIdent_snd_cons_sublist {rest tail : List Char} {e : Char}
    (h : (takeIdent rest).snd = e :: tail) (c : Char) :
    List.Sublist (e :: tail) (c :: rest) :=
  (h ▸ takeIdent_snd_sublist rest).cons c

/-- …and so is its tail (the scanner consumed one more char, e.g. the `=`). -/
theorem takeIdent_snd_cons_tail_sublist {rest tail : List Char} {e : Char}
    (h : (takeIdent rest).snd = e :: tail) (c : Char) :
    List.Sublist tail (c :: rest) :=
  ((sublist_cons_self' e tail).trans (h ▸ takeIdent_snd_sublist rest)).cons c

/-- The kwarg code step never invents input. -/
theorem kwargStep_rest_sublist (K : Dict) (s : Bool) (c : Char) (rest : List Char) :
    List.Sublist (kwargStep K s c rest).rest (c :: rest) := by
  simp only [kwargStep]
  repeat' split
  all_goals first
    | exact List.nil_sublist _
    | exact (List.Sublist.refl _).cons _
    | exact (takeIdent_snd_sublist _).cons _
    | exact takeIdent_snd_cons_sublist ‹_› _
    | exact takeIdent_snd_cons_tail_sublist ‹_› _
    | exact List.Sublist.refl _

/-- The Tier-B classifier selects only input characters, in order. -/
theorem protCharsK_sublist_input (K : Dict) (fuel : Nat) (st : Option ScanState)
    (a : Bool) (cs : List Char) :
    List.Sublist (protCharsK K fuel st a cs) cs :=
  protScan_sublist_input (kwargSpec K) (kwargStep_rest_sublist K) fuel st a cs

/-- **TIER-B ZONE-SAFETY.** Every character `expandKwargChars` consumes while
    inside a string / triple-string / comment zone is emitted verbatim, in
    order. Tier B's "rewrites only in code zones" is now a theorem about the
    executable scanner, not an assumption of the segment model. -/
theorem kwarg_zone_safety_chars (K : Dict) (fuel : Nat) (st : Option ScanState)
    (a : Bool) (cs : List Char) :
    List.Sublist (protCharsK K fuel st a cs) (expandKwargChars K fuel st a cs) :=
  protScan_sublist_scanChars (kwargSpec K) fuel st a cs

/-- The protected characters of a whole source string, Tier-B scanner. -/
def protCharsKStr (K : Dict) (s : String) : List Char :=
  protCharsK K (s.length + 1) none false s.toList

/-- Zone-safety of Tier B at the shipped `String` entry point. -/
theorem expandKwargStr_zone_safety (K : Dict) (s : String) :
    List.Sublist (protCharsKStr K s) (expandKwargStr K s).toList := by
  simpa [protCharsKStr, expandKwargStr, String.toList_ofList] using
    kwarg_zone_safety_chars K (s.length + 1) none false s.toList

/-! ### Tier B as a `Tier`: the segment-level obligations, now discharged
    for a CONCRETE function rather than an arbitrary one. -/

/-- Tier B maps each segment to exactly one segment (left-totality of the
    pipeline, instantiated). -/
theorem tierB_length (src : Source) : (applyTier tierB src).length = src.length :=
  applyTier_length tierB src

/-- Tier B never touches a protected segment (segment-level zone-safety,
    instantiated at the concrete tier). -/
theorem tierB_zone_safety (src : Source) :
    protPayloads (applyTier tierB src) = protPayloads src :=
  zone_safety tierB src

/-- The whole pipeline, with Tier B concrete in the Tier-B slot, is still
    zone-safe and left-total. -/
theorem expand_zone_safety_tierB (e aT h d : Tier) (src : Source) :
    protPayloads (expand ⟨e, aT, tierB, h, d⟩ src) = protPayloads src :=
  expand_zone_safety _ src

/-! ### Non-vacuity: the rewrite IS the committed table

The decisive move (same as `committed_inverse_consistent` for the
dictionary): DECIDE the property on the real, shipping 9-entry table. If the
Lean tier did not actually implement the alias table, this would not
compile. -/

/-- For every `(alias, canonical)` in the table, Tier B rewrites
    `f(alias=1)` to `f(canonical=1)` — and nothing else changes. -/
def kwargExactB (K : Dict) : Bool :=
  K.all fun e =>
    expandKwargStr K ("f(" ++ e.1 ++ "=1)") == "f(" ++ e.2 ++ "=1)"

/-- **The committed table is exactly what Tier B implements** — decided on
    the real 9-entry `kwargs.rs` table. -/
theorem committed_kwargs_exact : kwargExactB committedKwargs = true := by decide

/-- Position discipline, decided: an alias is rewritten ONLY in
    call-argument position, never in a statement, never before `==`, never
    inside a protected zone, and a non-table identifier is never touched. -/
def kwargPositionB : Bool :=
  -- fires in argument position, including after `,` and across a newline
  (tierB "f(cn=1)" == "f(class_name=1)") &&
  (tierB "f(x, oc=h)" == "f(x, on_click=h)") &&
  (tierB "f(\n  st=s,\n  dis=True)" == "f(\n  style=s,\n  disabled=True)") &&
  -- NOT in statement position (no preceding `(` or `,`)
  (tierB "cn = 1" == "cn = 1") &&
  -- NOT before `==` (comparison, not a kwarg)
  (tierB "f(cn==1)" == "f(cn==1)") &&
  -- NOT inside string / comment zones
  (tierB "s = \"f(cn=1)\"" == "s = \"f(cn=1)\"") &&
  (tierB "# f(cn=1)" == "# f(cn=1)") &&
  -- NOT a table entry => untouched
  (tierB "f(foo=1)" == "f(foo=1)") &&
  -- alias as a bare argument (no `=`) => untouched
  (tierB "f(cn)" == "f(cn)")

theorem kwargs_position_discipline : kwargPositionB = true := by decide

-- Executable smoke checks (kernel-reduced; also serve as documentation).
#guard tierB "f(cn=1, oc=go)" = "f(class_name=1, on_click=go)"
#guard tierB "d = {'cn': 1}" = "d = {'cn': 1}"
#guard protCharsKStr committedKwargs "s = \"f(cn=1)\"" = "f(cn=1)\"".toList
#guard protCharsKStr committedKwargs "f(cn=1)" = []

/-- info: 'PythExpandVerify.kwarg_zone_safety_chars' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms kwarg_zone_safety_chars

/-- info: 'PythExpandVerify.committed_kwargs_exact' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms committed_kwargs_exact

/-- info: 'PythExpandVerify.tierB_zone_safety' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms tierB_zone_safety

/-- info: 'PythExpandVerify.protCharsK_sublist_input' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms protCharsK_sublist_input


/-! ## HOOKS TIER — a CONCRETE tier instantiation (2026-07-14)

Step 5 of `expand_with_config` (`hooks` in `TIER_ORDER`), from
`crates/pyths_expand/src/hooks.rs`. A hook shorthand is rewritten ONLY when
it is a free identifier in CALL position: `us(` → `use_state(`. It must not
fire on an attribute access (`obj.us(…)`), not inside a longer identifier
(`x1us(`), not on a bare reference (`us` with no `(`), and not inside a
protected zone.

Faithfulness: `hooks.rs` carries the shared zone state machine plus two
extra bits — `prev_ident_cont` (was the previously emitted byte an
identifier-continuation char?) and `prev_was_dot` (was the last
non-space/tab byte a `.`?). `expandHookChars` threads exactly those two. -/

/-- `hooks.rs::prev_was_dot` update: `.` sets it; space/tab PRESERVE it (so
    `obj . us(…)` still inhibits); anything else clears it. -/
def nextDot (pd : Bool) (c : Char) : Bool :=
  if c = '.' then true
  else if c = ' ' || c = '\t' then pd
  else false

/-- Hooks-tier code step. State: `(prev_ident_cont, prev_was_dot)` — an
    alias fires only on a free identifier (`!pic && !pd`) in CALL position
    (immediately followed by `(`). -/
def hookStep (H : Dict) (s : Bool × Bool) (c : Char) (rest : List Char) :
    CodeOut (Bool × Bool) :=
  if !s.1 && !s.2 && isIdentStart c then
    let (idTail, rest') := takeIdent rest
    let ident := c :: idTail
    match rest' with
    | '(' :: _ =>
      match elookup H (String.ofList ident) with
      | some canon => ⟨canon.toList, (false, false), rest'⟩
      | none => ⟨ident, (true, false), rest'⟩
    | _ => ⟨ident, (true, false), rest'⟩
  else ⟨[c], (isIdentChar c, nextDot s.2 c), rest⟩

/-- The hooks tier as a `ScanSpec`: both flags are cleared at every zone
    entry (verbatim passthrough inside; a string/comment is not an
    identifier and not a dot). -/
def hookSpec (H : Dict) : ScanSpec (Bool × Bool) :=
  ⟨fun _ => (false, false), fun _ => (false, false), fun s => s, hookStep H⟩

/-- Executable model of `hooks.rs::substitute`.
    State: zone, `prev_ident_cont`, `prev_was_dot`. -/
def expandHookChars (H : Dict) : Nat → Option ScanState → Bool → Bool → List Char → List Char :=
  fun fuel st pic pd cs => scanChars (hookSpec H) fuel st (pic, pd) cs

/-- The hooks tier on a source string. -/
def expandHookStr (H : Dict) (s : String) : String :=
  String.ofList (expandHookChars H (s.length + 1) none false false s.toList)

/-- **THE concrete hooks tier**: the shipping hook table, as a `Tier`. -/
def tierHooks : Tier := expandHookStr committedHooks

/-- Characters consumed while the hooks scanner is in a PROTECTED state. -/
def protCharsH (H : Dict) : Nat → Option ScanState → Bool → Bool → List Char → List Char :=
  fun fuel st pic pd cs => protScan (hookSpec H) fuel st (pic, pd) cs

/-- The hook code step never invents input. -/
theorem hookStep_rest_sublist (H : Dict) (s : Bool × Bool) (c : Char) (rest : List Char) :
    List.Sublist (hookStep H s c rest).rest (c :: rest) := by
  simp only [hookStep]
  repeat' split
  all_goals first
    | exact List.nil_sublist _
    | exact (List.Sublist.refl _).cons _
    | exact (takeIdent_snd_sublist _).cons _
    | exact List.Sublist.refl _

/-- The hooks classifier selects only input characters, in order. -/
theorem protCharsH_sublist_input (H : Dict) (fuel : Nat) (st : Option ScanState)
    (pic pd : Bool) (cs : List Char) :
    List.Sublist (protCharsH H fuel st pic pd cs) cs :=
  protScan_sublist_input (hookSpec H) (hookStep_rest_sublist H) fuel st (pic, pd) cs

/-- **HOOKS ZONE-SAFETY.** Every character consumed inside a string /
    triple-string / comment zone is emitted verbatim, in order. -/
theorem hook_zone_safety_chars (H : Dict) (fuel : Nat) (st : Option ScanState)
    (pic pd : Bool) (cs : List Char) :
    List.Sublist (protCharsH H fuel st pic pd cs) (expandHookChars H fuel st pic pd cs) :=
  protScan_sublist_scanChars (hookSpec H) fuel st (pic, pd) cs

def protCharsHStr (H : Dict) (s : String) : List Char :=
  protCharsH H (s.length + 1) none false false s.toList

/-- Zone-safety of the hooks tier at the shipped `String` entry point. -/
theorem expandHookStr_zone_safety (H : Dict) (s : String) :
    List.Sublist (protCharsHStr H s) (expandHookStr H s).toList := by
  simpa [protCharsHStr, expandHookStr, String.toList_ofList] using
    hook_zone_safety_chars H (s.length + 1) none false false s.toList

/-- Segment-count preservation, instantiated at the concrete hooks tier. -/
theorem tierHooks_length (src : Source) :
    (applyTier tierHooks src).length = src.length :=
  applyTier_length tierHooks src

/-- Segment-level zone-safety, instantiated at the concrete hooks tier. -/
theorem tierHooks_zone_safety (src : Source) :
    protPayloads (applyTier tierHooks src) = protPayloads src :=
  zone_safety tierHooks src

/-! ### Non-vacuity: the rewrite IS the committed hook table -/

/-- Every `(alias, canonical)` in the table rewrites in call position — and
    the alias in NON-call position (a bare reference) does not. -/
def hookExactB (H : Dict) : Bool :=
  H.all fun e =>
    (expandHookStr H (e.1 ++ "(0)") == e.2 ++ "(0)")
    && (expandHookStr H ("x = " ++ e.1) == "x = " ++ e.1)

/-- **The committed table is exactly what the hooks tier implements** —
    decided on the real 6-entry `hooks.rs` table. -/
theorem committed_hooks_exact : hookExactB committedHooks = true := by decide

/-- Position discipline, decided: call position only; never after `.`;
    never inside a longer identifier; never in a protected zone. -/
def hookPositionB : Bool :=
  -- fires in call position
  (tierHooks "s, ss = us(0)" == "s, ss = use_state(0)") &&
  (tierHooks "ue(f, [])" == "use_effect(f, [])") &&
  -- NOT on attribute access (`prev_was_dot`), even with spaces around the dot
  (tierHooks "obj.us(0)" == "obj.us(0)") &&
  (tierHooks "obj . us(0)" == "obj . us(0)") &&
  -- NOT inside a longer identifier (`prev_ident_cont`)
  (tierHooks "x1us(0)" == "x1us(0)") &&
  (tierHooks "myus(0)" == "myus(0)") &&
  -- NOT a bare reference (no `(` follows)
  (tierHooks "x = us" == "x = us") &&
  (tierHooks "us = 1" == "us = 1") &&
  -- NOT inside string / comment zones
  (tierHooks "s = \"us(0)\"" == "s = \"us(0)\"") &&
  (tierHooks "# us(0)" == "# us(0)") &&
  -- non-table identifier in call position => untouched
  (tierHooks "foo(0)" == "foo(0)")

theorem hooks_position_discipline : hookPositionB = true := by decide

#guard tierHooks "v, sv = us(1)" = "v, sv = use_state(1)"
#guard protCharsHStr committedHooks "s = \"us(0)\"" = "us(0)\"".toList
#guard protCharsHStr committedHooks "us(0)" = []

/-- info: 'PythExpandVerify.hook_zone_safety_chars' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms hook_zone_safety_chars

/-- info: 'PythExpandVerify.committed_hooks_exact' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms committed_hooks_exact

/-- info: 'PythExpandVerify.tierHooks_zone_safety' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms tierHooks_zone_safety


/-! ## TIER E — a CONCRETE tier instantiation (2026-07-14)

Step 1 of `expand_with_config` (`E` in `TIER_ORDER`), from
`crates/pyths_expand/src/idioms.rs`. `%NAME` expands to a canonical code
FRAGMENT (possibly multi-line), unlike `$NAME`, which yields a string
literal.

HONEST SCOPING — Tier E differs from A / B / hooks / Dict in one respect
that must not be glossed: **there is no compiler-side idiom table.**
`substitute_with_map` takes its map from `[expand.idioms]` in the user's
`pyths.toml`, which is EMPTY by default. So what is instantiated and proved
here is the SHIPPED SCANNER, quantified over an ARBITRARY table `M`; and the
differential (`diff_harness.py --tier idioms`) pins that scanner against the
Rust one over a committed fixture table (`verification/idiom-table.toml`,
generated into `IdiomData.lean`). There is no shipped table to be exact
about — so `committed_idioms_exact` below is exactness w.r.t. the FIXTURE,
and `expandIdiom_nil` is the theorem about the default (empty-map) config. -/

/-- Tier-E code step: the `%NAME` idiom lookup. Unlike `$NAME`, an unknown
    `%NAME` is emitted verbatim with the scanner advancing PAST the name
    (maximal munch — sigils are not prefix-matched). -/
def idiomStep (M : Dict) (_ : Unit) (c : Char) (rest : List Char) : CodeOut Unit :=
  if c = '%' then
    match rest with
    | c2 :: _ =>
      if isIdentChar c2 then
        let (name, rest') := takeIdent rest
        match elookup M (String.ofList name) with
        | some frag => ⟨frag.toList, (), rest'⟩
        -- unknown idiom: `%NAME` emitted verbatim, scanner advances past it
        | none => ⟨c :: name, (), rest'⟩
      else ⟨[c], (), rest⟩
    | [] => ⟨[c], (), []⟩
  else ⟨[c], (), rest⟩

/-- Tier E as a `ScanSpec`: no extra code-state. -/
def idiomSpec (M : Dict) : ScanSpec Unit :=
  ⟨fun s => s, fun s => s, fun s => s, idiomStep M⟩

/-- Executable model of `idioms.rs::substitute_with_map`. -/
def expandIdiomChars (M : Dict) : Nat → Option ScanState → List Char → List Char :=
  fun fuel st cs => scanChars (idiomSpec M) fuel st () cs

def expandIdiomStr (M : Dict) (s : String) : String :=
  String.ofList (expandIdiomChars M (s.length + 1) none s.toList)

/-- **THE concrete Tier E**, over the committed fixture table. -/
def tierE : Tier := expandIdiomStr committedIdioms

/-- Characters consumed while the Tier-E scanner is in a PROTECTED state. -/
def protCharsE (M : Dict) : Nat → Option ScanState → List Char → List Char :=
  fun fuel st cs => protScan (idiomSpec M) fuel st () cs

/-- The idiom code step never invents input. -/
theorem idiomStep_rest_sublist (M : Dict) (s : Unit) (c : Char) (rest : List Char) :
    List.Sublist (idiomStep M s c rest).rest (c :: rest) := by
  simp only [idiomStep]
  repeat' split
  all_goals first
    | exact List.nil_sublist _
    | exact (List.Sublist.refl _).cons _
    | exact (takeIdent_snd_sublist _).cons _
    | exact List.Sublist.refl _

/-- The Tier-E classifier selects only input characters, in order. -/
theorem protCharsE_sublist_input (M : Dict) (fuel : Nat) (st : Option ScanState)
    (cs : List Char) : List.Sublist (protCharsE M fuel st cs) cs :=
  protScan_sublist_input (idiomSpec M) (idiomStep_rest_sublist M) fuel st () cs

/-- **TIER-E ZONE-SAFETY.** Every character consumed inside a string /
    triple-string / comment zone is emitted verbatim, in order — so a `%NAME`
    sigil inside a docstring or a comment never expands. Char-generic and
    quantified over an ARBITRARY idiom table `M`. -/
theorem idiom_zone_safety_chars (M : Dict) (fuel : Nat) (st : Option ScanState)
    (cs : List Char) :
    List.Sublist (protCharsE M fuel st cs) (expandIdiomChars M fuel st cs) :=
  protScan_sublist_scanChars (idiomSpec M) fuel st () cs

def protCharsEStr (M : Dict) (s : String) : List Char :=
  protCharsE M (s.length + 1) none s.toList

theorem expandIdiomStr_zone_safety (M : Dict) (s : String) :
    List.Sublist (protCharsEStr M s) (expandIdiomStr M s).toList := by
  simpa [protCharsEStr, expandIdiomStr, String.toList_ofList] using
    idiom_zone_safety_chars M (s.length + 1) none s.toList

/-- Segment-count preservation, instantiated at the concrete Tier E. -/
theorem tierE_length (src : Source) : (applyTier tierE src).length = src.length :=
  applyTier_length tierE src

/-- Segment-level zone-safety, instantiated at the concrete Tier E. -/
theorem tierE_zone_safety (src : Source) :
    protPayloads (applyTier tierE src) = protPayloads src :=
  zone_safety tierE src

/-! ### The default configuration: an EMPTY idiom map is the identity

`idioms.rs` short-circuits on an empty map (`if map.is_empty() { return
src.to_string() }`). That early return is an optimization; this theorem says
it is a SOUND one — the scanner itself is the identity on an empty table, so
the fast path cannot change behaviour. Since the map is empty by default
(`expand_idioms_empty_by_default`), this is the theorem that covers the
zero-config shipping path. -/

/-- The zero-table idiom step re-emits exactly what it consumes (the `hid`
    obligation of the generic `scanChars_id`). -/
theorem idiomStep_nil_id (s : Unit) (c : Char) (rest : List Char) :
    (idiomStep [] s c rest).emit ++ (idiomStep [] s c rest).rest = c :: rest := by
  simp only [idiomStep]
  repeat' split
  all_goals first
    | rfl
    | exact takeIdent_append _
    | (simp_all [elookup, takeIdent_append])
    | exact congrArg (c :: ·) (takeIdent_append _)

theorem expandIdiom_nil (fuel : Nat) (st : Option ScanState) (cs : List Char) :
    expandIdiomChars [] fuel st cs = cs :=
  scanChars_id (idiomSpec []) idiomStep_nil_id fuel st () cs

/-- The zero-config Tier E is the identity on every source. -/
theorem expandIdiomStr_nil (s : String) : expandIdiomStr [] s = s := by
  simp [expandIdiomStr, expandIdiom_nil]

/-! ### Non-vacuity: the rewrite IS the committed fixture table -/

/-- Every `(name, fragment)` in the table expands in code position; and the
    same sigil inside a double-quoted string does NOT. -/
def idiomExactB (M : Dict) : Bool :=
  M.all fun e =>
    (expandIdiomStr M ("%" ++ e.1) == e.2)
    && (expandIdiomStr M ("s = \"%" ++ e.1 ++ "\"") == "s = \"%" ++ e.1 ++ "\"")

/-- **The fixture table is exactly what Tier E implements** — decided on the
    real 9-entry committed fixture. -/
theorem committed_idioms_exact : idiomExactB committedIdioms = true := by decide

/-- Sigil discipline, decided: `%` fires only before an identifier char;
    unknown names pass through verbatim; a bare `%` (modulo) is untouched. -/
def idiomPositionB : Bool :=
  -- fires in code position
  (tierE "%PASS" == "pass") &&
  (tierE "x = 1\n%LOG\n" == "x = 1\nprint(\"[debug] tier-e\")\n") &&
  -- unknown name: verbatim
  (tierE "%NOPE" == "%NOPE") &&
  -- bare `%` (modulo) is not a sigil: not followed by an identifier char
  (tierE "x = a % b" == "x = a % b") &&
  -- MAXIMAL MUNCH on the name run: `%EMPTYb` is the (unknown) name `EMPTYb`,
  -- NOT the known name `EMPTY` followed by `b`. Sigils are not prefix-matched.
  (tierE "a%EMPTYb" == "a%EMPTYb") &&
  -- protected zones
  (tierE "s = \"%PASS\"" == "s = \"%PASS\"") &&
  (tierE "# %PASS" == "# %PASS") &&
  (tierE "d = '''%PASS'''" == "d = '''%PASS'''") &&
  -- the fragment is NOT re-scanned: a `%` inside an expansion survives
  (tierE "%MODEXPR" == "rem = total % 7") &&
  -- empty fragment deletes the sigil entirely
  (tierE "x = %EMPTY" == "x = ")

theorem idioms_sigil_discipline : idiomPositionB = true := by decide

/-- **DOCUMENTED HAZARD, machine-checked** (`idioms.rs` module doc): an
    all-digit idiom name is accepted by the scanner, so a table entry `"10"`
    silently intercepts the Python modulo expression `x %10` — which is legal
    Python, and means `x % 10`. The sigil grammar requires only that a name
    char FOLLOW the `%`, so whitespace is what separates a modulo from a
    sigil. The model is bug-compatible with the implementation, and the
    differential proves the Rust scanner does the same. A property of the
    SIGIL GRAMMAR, not a defect of the model. -/
theorem idiom_digit_name_intercepts_modulo :
    tierE "x = 5 %10" = "x = 5 TEN_INTERCEPTED" := by decide

/-- …and, for contrast, a SPACED modulo is untouched: whitespace after `%` is
    not a name char, so this is the disambiguator users actually rely on. -/
theorem idiom_spaced_modulo_untouched :
    tierE "x = 5 % 10" = "x = 5 % 10" := by decide

#guard tierE "%LOG" = "print(\"[debug] tier-e\")"
#guard protCharsEStr committedIdioms "s = \"%PASS\"" = "%PASS\"".toList
#guard protCharsEStr committedIdioms "%PASS" = []

/-- info: 'PythExpandVerify.idiom_zone_safety_chars' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms idiom_zone_safety_chars

/-- info: 'PythExpandVerify.committed_idioms_exact' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms committed_idioms_exact

/-- info: 'PythExpandVerify.expandIdiomStr_nil' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms expandIdiomStr_nil


/-! ## TIER A — a CONCRETE tier instantiation, ZONE-AWARE (2026-07-14)

Step 3 of `expand_with_config` (`A` in `TIER_ORDER`), from
`crates/pyths_expand/src/{presets,decorators}.rs`, `lib.rs::expand_line`, and
the shared classifier `zones.rs`.

Tier A rewrites LINES, not characters: a line whose trimmed body is exactly a
preset marker (`R*`) becomes `indent ++ canonicalImport`; a line whose first
significant character is `@` followed by an alias in the decorator table has
that alias replaced, the rest of the line copied verbatim; every other line is
emitted unchanged. What makes it SAFE is that it asks the shared zone
classifier (`zones::line_start_states`) which lines begin in a code zone, and
rewrites only those: a marker alone on a line inside a `"""…"""` docstring is
docstring text.

The model below is that same rewrite expressed as a CHARACTER scanner whose
zone arms are, arm for arm, the arms of `expandDictChars` / `protChars` — the
one classifier every tier shares. Three code-zone modes carry the line
structure:

  * `lineStart` — at the start of a line or inside its leading indent: a
    preset marker or decorator alias may fire here, and only here;
  * `dropMarker` — discarding the matched marker and the whitespace after it
    (the canonical import has already been emitted), keeping the `\r` of a
    CRLF terminator, exactly as `expand_line` does;
  * `dropAlias` — discarding the matched decorator alias's name characters
    (the canonical decorator name has already been emitted); everything after
    the alias is then scanned as ordinary code, hence copied verbatim.

Expressing Tier A this way is what lets `tierA_zone_safety_chars` be proved in
the SAME shape, against the SAME classifier, as `zone_safety_chars` /
`kwarg_zone_safety_chars` / `hook_zone_safety_chars` / `idiom_zone_safety_chars`
— and `tierA` is the function `Main.lean` exposes as `expanddiff --tiera`, so
`diff_harness.py --tier tiera` binds it byte-for-byte to the real compiler on
every one of these cases (markers in docstrings, markers after a closed
docstring, escapes, CRLF, indents). -/

/-- Rust `char::is_whitespace` = the Unicode `White_Space` property. Spelled
    out so the model's trimming agrees with `str::trim_start` / `trim_end`
    EXACTLY, including on NBSP and the U+2000 block — not merely on ASCII. -/
def isRustWs (c : Char) : Bool :=
  let v := c.val
  v == 0x09 || v == 0x0A || v == 0x0B || v == 0x0C || v == 0x0D || v == 0x20
    || v == 0x85 || v == 0xA0 || v == 0x1680
    || (0x2000 ≤ v && v ≤ 0x200A)
    || v == 0x2028 || v == 0x2029 || v == 0x202F || v == 0x205F || v == 0x3000

def trimEndWs (cs : List Char) : List Char :=
  (cs.reverse.dropWhile isRustWs).reverse

/-- Tier A's code-zone modes (see the section header). -/
inductive TierAMode where
  | lineStart
  | inLine
  | dropMarker
  | dropAlias
deriving DecidableEq

/-- The body of the line starting at `cs`: characters up to (not including)
    the newline. `expand_line` peels `\r\n` / `\n` and then trims, so the key
    is this body with trailing whitespace removed — the `\r` of a CRLF falls
    to the trim, exactly as in Rust. -/
def tierABody (cs : List Char) : List Char := cs.takeWhile (fun c => c != '\n')

/-- The key Tier A looks up in the preset table: the trimmed line body. -/
def tierAKey (cs : List Char) : List Char := trimEndWs (tierABody cs)

/-- The alias run after an `@`: `lib.rs::expand_decorator_line` takes the
    ASCII-alphanumeric run. -/
def tierAAlias (cs : List Char) : List Char := cs.takeWhile Char.isAlphanum

/-- Tier-A code step: the per-line rewrite. The zone openers (`#`, `'`,
    `"`) are handled by the shared skeleton BEFORE this step is consulted —
    a zone opener is never dropped and never reinterpreted, whatever mode
    Tier A is in. That is what makes the drop modes safe by construction:
    they can only ever discard code. The `$` arm takes the SAME step the
    shared classifier takes (it jumps the identifier) and emits it verbatim
    in both branches, so `D` cannot influence the output — it keeps this
    step in lockstep with `dictStep` (`tierAStep_rest_eq_dictStep`), which
    is what lets Tier A reuse the ONE classifier `protChars`. -/
def tierAStep (D P Dec : Dict) (m : TierAMode) (c : Char) (rest : List Char) :
    CodeOut TierAMode :=
  if c = '$' then
    match rest with
    | c2 :: _ =>
      if isIdentChar c2 then
        match elookup D (String.ofList (takeIdent rest).1) with
        | some _ => ⟨c :: (takeIdent rest).1, .inLine, (takeIdent rest).2⟩
        | none => ⟨[c], .inLine, rest⟩
      else ⟨[c], .inLine, rest⟩
    | [] => ⟨[c], .inLine, []⟩
  else if c = '\n' then
    -- a newline in code always starts a new line
    ⟨[c], .lineStart, rest⟩
  else if m = .dropMarker then
    -- erasing the matched marker and the whitespace after it; the `\r` of a
    -- CRLF terminator is part of the newline, not trailing whitespace
    -- (`split_newline` peels `\r\n` before the trim), so it is kept
    if c = '\r' && rest.head? = some '\n' then
      ⟨[c], .dropMarker, rest⟩
    else
      ⟨[], .dropMarker, rest⟩
  else if m = .dropAlias && c.isAlphanum then
    -- erasing the matched decorator alias's name characters
    ⟨[], .dropAlias, rest⟩
  else if m = .lineStart && isRustWs c then
    -- the leading indent: still at the line start
    ⟨[c], .lineStart, rest⟩
  else if m = .lineStart then
    -- the first significant character of a code line: a preset marker…
    match elookup P (String.ofList (tierAKey (c :: rest))) with
    | some exp => ⟨exp.toList, .dropMarker, rest⟩
    | none =>
      -- …or a decorator alias?
      if c = '@' && !(tierAAlias rest).isEmpty then
        match elookup Dec (String.ofList ('@' :: tierAAlias rest)) with
        | some canon => ⟨canon.toList, .dropAlias, rest⟩
        | none => ⟨[c], .inLine, rest⟩
      else ⟨[c], .inLine, rest⟩
  else ⟨[c], .inLine, rest⟩

/-- Tier A as a `ScanSpec`: a zone entered mid-line returns to mid-line
    (`inLine`); a comment always ends with its line, so its `\n` exits to
    `lineStart`. -/
def tierASpec (D P Dec : Dict) : ScanSpec TierAMode :=
  ⟨fun _ => .inLine, fun _ => .inLine, fun _ => .lineStart, tierAStep D P Dec⟩

def expandTierAChars (D P Dec : Dict) :
    Nat → Option ScanState → TierAMode → List Char → List Char :=
  fun fuel st m cs => scanChars (tierASpec D P Dec) fuel st m cs

/-- Tier A on a source string — the concrete `Tier`. -/
def expandTierAStr (P Dec : Dict) (s : String) : String :=
  String.ofList
    (expandTierAChars committedDict P Dec (s.length + 1) none .lineStart s.toList)

/-- **THE concrete Tier A**: the shipping preset + decorator tables. This is
    the function `expanddiff --tiera` runs. -/
def tierA : Tier := expandTierAStr committedPresets committedDecorators

-- Executable smoke checks (kernel-reduced; also serve as documentation).
#guard tierA "R*\n" = "from pyths.react import component, use_state, use_effect, use_callback, use_memo\n"
#guard tierA "  @c\n" = "  @component\n"
#guard tierA "  R*  \n" = "  from pyths.react import component, use_state, use_effect, use_callback, use_memo\n"
#guard tierA "x = R*\n" = "x = R*\n"
#guard tierA "doc = \"\"\"\nR*\n\"\"\"\ny = 1\n" = "doc = \"\"\"\nR*\n\"\"\"\ny = 1\n"
#guard tierA "d = '''\n@c\n'''\n" = "d = '''\n@c\n'''\n"
#guard tierA "# R*\n" = "# R*\n"

/-! ### Tier A's line discipline -/

/-- A marker fires only when it is the WHOLE (trimmed) line, at a line start,
    in a CODE zone; indentation is preserved; a decorator alias keeps the rest
    of its line byte-for-byte; anything else is untouched. -/
def tierAPositionB : Bool :=
  -- whole-line marker fires; indentation preserved
  (tierA "R*\n" == "from pyths.react import component, use_state, use_effect, use_callback, use_memo\n") &&
  (tierA "  @c\n" == "  @component\n") &&
  -- marker NOT alone on the line => untouched
  (tierA "x = R*\n" == "x = R*\n") &&
  (tierA "R* extra\n" == "R* extra\n") &&
  (tierA "R*  # note\n" == "R*  # note\n") &&
  -- unknown marker / unknown decorator => untouched
  (tierA "Z*\n" == "Z*\n") &&
  (tierA "@zzz\n" == "@zzz\n") &&
  -- a bare `@` is not a decorator alias (alias_len == 0)
  (tierA "@\n" == "@\n") &&
  -- blank / whitespace-only lines are returned verbatim
  (tierA "   \n" == "   \n") &&
  -- final line without a trailing newline still expands
  (tierA "@c" == "@component") &&
  -- CRLF is preserved
  (tierA "@c\r\n" == "@component\r\n") &&
  (tierA "R*\r\n" == "from pyths.react import component, use_state, use_effect, use_callback, use_memo\r\n") &&
  -- a preset marker's trailing whitespace is dropped with the marker
  (tierA "  R*  \n" == "  from pyths.react import component, use_state, use_effect, use_callback, use_memo\n") &&
  -- a decorator alias keeps everything after it, verbatim
  (tierA "@d(coerce=True)\n" == "@dataclass(coerce=True)\n") &&
  (tierA "@c  \n" == "@component  \n")

theorem tierA_line_discipline : tierAPositionB = true := by decide

/-! ### Zone discipline, decided on the witnesses that used to break -/

/-- The old defect, now decided the other way: a preset marker or decorator
    alias alone on a line inside a triple-quoted string is TEXT. -/
def tierAZoneB : Bool :=
  -- markers inside docstrings: verbatim (both quote flavours, indented too)
  (tierA "doc = \"\"\"\nR*\n\"\"\"\ny = 1\n" == "doc = \"\"\"\nR*\n\"\"\"\ny = 1\n") &&
  (tierA "d = '''\n@c\n'''\n" == "d = '''\n@c\n'''\n") &&
  (tierA "d = \"\"\"\n  R+  \n@d\n\"\"\"\n" == "d = \"\"\"\n  R+  \n@d\n\"\"\"\n") &&
  -- an unterminated string protects the lines that follow
  (tierA "s = \"oops\nR*\n" == "s = \"oops\nR*\n") &&
  -- an escaped quote does not close the zone
  (tierA "s = \"a\\\"b\nR*\n" == "s = \"a\\\"b\nR*\n") &&
  -- a comment line is not a marker line
  (tierA "# R*\n" == "# R*\n") &&
  -- CONTROL: the same marker in a code position right after the closed
  -- docstring still expands, and so does one after a closed inline string
  (tierA "doc = \"\"\"\nR*\n\"\"\"\n@c\n" == "doc = \"\"\"\nR*\n\"\"\"\n@component\n") &&
  (tierA "s = \"R*\"\n@c\n" == "s = \"R*\"\n@component\n") &&
  (tierA "a = \"\"\"x\"\"\"\n@d\n" == "a = \"\"\"x\"\"\"\n@dataclass\n")

theorem tierA_zone_discipline : tierAZoneB = true := by decide

/-! ### Non-vacuity: the rewrite IS the committed preset/decorator tables -/

/-- Every preset marker expands to its canonical import (bare and indented,
    with trailing whitespace dropped, as `expand_line` does); every decorator
    alias expands, both bare and with call-args. -/
def tierAExactB : Bool :=
  committedPresets.all (fun e =>
      (tierA (e.1 ++ "\n") == e.2 ++ "\n")
      && (tierA ("    " ++ e.1 ++ "  \n") == "    " ++ e.2 ++ "\n"))
    && committedDecorators.all (fun e =>
      (tierA (e.1 ++ "\n") == e.2 ++ "\n")
      && (tierA (e.1 ++ "(coerce=True)\n") == e.2 ++ "(coerce=True)\n"))

/-- **The committed tables are exactly what Tier A implements** — decided on
    the real 8 presets + 5 decorators. -/
theorem committed_tierA_exact : tierAExactB = true := by decide

/-! ### THE ZONE-SAFETY THEOREM FOR TIER A

The positive analogue of `zone_safety_chars` (Dict), `kwarg_zone_safety_chars`
(B), `hook_zone_safety_chars` (hooks) and `idiom_zone_safety_chars` (E), over
the SAME companion classifier `protChars`: every character the classifier
consumes in a protected state — inside `'…'`, `"…"`, `'''…'''`, `"""…"""`, or
after a `#` — is emitted by Tier A verbatim, in order.

Char-generic, hence it covers the preset markers, the decorator sigil `@`, and
any marker added later: nothing inside a protected zone is rewritten, whatever
it spells. -/

/-- Tier A's code step consumes the input EXACTLY as the dict code step
    does, whatever mode it is in — the lockstep fact that lets Tier A's
    zone-safety range over the ONE shared classifier `protChars`. -/
theorem tierAStep_rest_eq_dictStep (em : Canon → List Char) (D P Dec : Dict)
    (m : TierAMode) (c : Char) (rest : List Char) :
    (dictStep em D () c rest).rest = (tierAStep D P Dec m c rest).rest := by
  by_cases hc : c = '$'
  · subst hc
    simp only [dictStep, tierAStep, if_pos]
    cases rest with
    | nil => rfl
    | cons c2 rest2 =>
      by_cases h2 : isIdentChar c2
      · simp only [h2, if_pos]
        repeat' split
        all_goals simp_all
      · simp [h2]
  · simp only [dictStep, tierAStep, if_neg hc]
    repeat' split
    all_goals rfl

/-- **TIER A IS ZONE-SAFE.**

    The instantiation of the generic simulation `protScan_sublist_scanChars_R`
    with `sp₁` the dict spec (the ONE shared classifier), `sp₂` the Tier-A
    spec, and `R := ⊤`: the two code steps consume input in lockstep at every
    mode (`tierAStep_rest_eq_dictStep`), which is all the simulation needs. -/
theorem tierA_zone_safety_chars : ∀ (fuel : Nat) (st : Option ScanState)
    (m : TierAMode) (cs : List Char),
    List.Sublist (protChars committedDict fuel st cs)
      (expandTierAChars committedDict committedPresets committedDecorators fuel st m cs) := by
  intro fuel st m cs
  exact protScan_sublist_scanChars_R (dictProtSpec committedDict)
    (tierASpec committedDict committedPresets committedDecorators)
    (fun _ _ => True)
    (fun _ => trivial) (fun _ => trivial) (fun _ => trivial)
    (fun c rest _ => ⟨tierAStep_rest_eq_dictStep _ _ _ _ _ c rest, trivial⟩)
    fuel st () m cs trivial

/-- Zone-safety of Tier A at the shipped `String` entry point — the function
    `expanddiff --tiera` runs, hence the one bound to `pyths expand` by the
    differential. -/
theorem tierA_zone_safety (s : String) :
    List.Sublist (protCharsStr committedDict s) (tierA s).toList := by
  simpa [protCharsStr, tierA, expandTierAStr, String.toList_ofList] using
    tierA_zone_safety_chars (s.length + 1) none .lineStart s.toList

/-- A preset marker sitting in a protected zone survives into Tier A's output
    — the concrete refutation-turned-theorem: the `*` of `R*` inside a
    docstring is still there afterwards. -/
theorem tierA_protected_char_verbatim (s : String) (c : Char)
    (h : c ∈ protCharsStr committedDict s) : c ∈ (tierA s).toList :=
  (tierA_zone_safety s).mem h

/-- info: 'PythExpandVerify.committed_tierA_exact' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms committed_tierA_exact

/-- info: 'PythExpandVerify.tierA_line_discipline' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms tierA_line_discipline

/-- info: 'PythExpandVerify.tierA_zone_discipline' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms tierA_zone_discipline

/-- info: 'PythExpandVerify.tierA_zone_safety_chars' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms tierA_zone_safety_chars

/-- info: 'PythExpandVerify.tierA_zone_safety' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms tierA_zone_safety


/-! ## The pipeline, with EVERY tier CONCRETE

`Config` takes five tiers. ALL FIVE are concrete, executable Lean functions
over CI-pinned tables — there is no abstract slot left:

  | slot     | concrete function | table                        |
  |----------|-------------------|------------------------------|
  | tierE    | `tierE`           | fixture (config-supplied)    |
  | tierA    | `tierA`           | presets.rs + decorators.rs   |
  | tierB    | `tierB`           | kwargs.rs                    |
  | tierHooks| `tierHooks`       | hooks.rs                     |
  | tierDict | `expandDictStr …` | strings.rs                   |

Each carries BOTH a `by decide` table-exactness theorem (`committed_*_exact`
— the Lean table IS the shipping Rust table) AND a character-level
zone-safety theorem against the ONE shared classifier:

  | slot     | exactness                  | char-level zone safety     |
  |----------|----------------------------|----------------------------|
  | tierE    | `committed_idioms_exact`   | `idiom_zone_safety_chars`  |
  | tierA    | `committed_tierA_exact`    | `tierA_zone_safety_chars`  |
  | tierB    | `committed_kwargs_exact`   | `kwarg_zone_safety_chars`  |
  | tierHooks| `committed_hooks_exact`    | `hook_zone_safety_chars`   |
  | tierDict | `committed_dict_exact`     | `zone_safety_chars`        |

HISTORICAL NOTE. A sixth tier once sat at Step 2 — the PSX tag-DSL
(`psx.rs`), a recursive-descent parser with snapshot/restore backtracking
and no alias table. It was the one slot this method could not reach: the
`generate table → decide exactness → drift-gate` recipe has no object to
apply to a grammar. It has been REMOVED from the expander rather than left
abstract, so the caveat it forced ("five of six tiers") is gone. Every tier
the shipped expander runs is now proved. -/

/-- The shipped pipeline. Every tier is CONCRETE — this definition takes no
    abstract parameter, which is the whole point: nothing in it is taken on
    trust. Zone-safety and left-totality hold. The segment-level
    `expand_zone_safety` assumes every tier rewrites code segments only; that
    assumption is discharged at the character level, against ONE shared
    classifier, for EVERY tier in this config — E (`idiom_zone_safety_chars`),
    A (`tierA_zone_safety_chars`), B (`kwarg_zone_safety_chars`), hooks
    (`hook_zone_safety_chars`) and Dict (`zone_safety_chars`). -/
def concreteConfig : Config :=
  ⟨tierE, tierA, tierB, tierHooks, expandDictStr committedDict⟩

/-- The fixed tier order, at the concrete instantiation. -/
theorem concrete_expand_order (src : Source) :
    expand concreteConfig src
      = applyTier (expandDictStr committedDict) (applyTier tierHooks (applyTier tierB
          (applyTier tierA (applyTier tierE src)))) :=
  expand_order _ src

/-- Left-totality at the concrete instantiation. -/
theorem concrete_expand_length (src : Source) :
    (expand concreteConfig src).length = src.length :=
  expand_length _ src

/-- **The goal state, as a proposition.** The shipped pipeline is EXACTLY the
    five concrete tiers, in order — no abstract slot, no phantom tier. Read
    together with the exactness + char-level zone-safety theorems named in the
    table above, this says: every tier the shipped expander runs is proved
    zone-safe against the shared classifier, and is table-exact w.r.t. the
    Rust it models. -/
theorem shipped_pipeline_is_five_proved_tiers :
    concreteConfig.pipeline
      = [tierE, tierA, tierB, tierHooks, expandDictStr committedDict] := rfl

/-- The shipped tier order agrees with `TIER_ORDER` in `crates/pyths_expand/src/lib.rs`
    and `tier-order:` in `verification/model-manifest.txt` (both `E,A,B,hooks,Dict`),
    which `tests/gates.rs` cross-checks. Length is the machine-checkable half. -/
theorem shipped_pipeline_length : concreteConfig.pipeline.length = 5 := rfl

/-! ## RouteModel — the subscript-routing decision procedure (§7.2)

Lean twin of `crates/pyths_codegen_js/src/cert.rs::route` — the single
decision procedure `emit.rs` consults for every subscript lowering.
Credible compilation: codegen records each decision into a certificate;
an independent Rust checker re-applies these rules and cross-checks the
emitted JS. HERE the rules themselves are pinned and their safety
invariant proved. The two implementations are bound by the enumerated
decision table (`verification/route-table.txt`): the Rust side compares
it in `tests/cert_corpus.rs`, this side via
`lake exe expanddiff --check-route-table` — either side changing its
rules breaks its own gate. -/

inductive RecvTy where
  | primitive | float | list | dict | set | tuple | unknown
deriving DecidableEq, Repr

inductive RouteK where
  | pySlice | nativeInbounds | helper | native
deriving DecidableEq, Repr

structure Site where
  isSlice : Bool
  isLhs : Bool
  isOptional : Bool
  inbounds : Bool
  ty : RecvTy
deriving DecidableEq, Repr

/-- The helper type set — Python-semantics reads. Pinned here and in
    `cert.rs`; the decision table binds them. EVERY receiver type routes
    through `pyGetItem` for a plain read: the container types get Python
    lookup semantics (IndexError/KeyError), and the NON-subscriptable types
    (`float`, `set`, and the `int`/`bool` inside `primitive`) get a Python
    `TypeError` from the helper instead of a silent JS `undefined`. Previously
    `float`/`set` were EXCLUDED and lowered native (`3.5[0]` → `undefined`), a
    read-safety gap the lattice C4 shipping-binding surfaced; now closed. -/
def helperTy : RecvTy → Bool
  | .list | .dict | .tuple | .primitive | .unknown | .float | .set => true

/-- THE routing rules, in `emit.rs` order. -/
def routeOf (s : Site) : RouteK :=
  if s.isSlice then .pySlice
  else if s.inbounds then .nativeInbounds
  else if !s.isLhs && !s.isOptional && helperTy s.ty then .helper
  else .native

/-- **Read-safety invariant** (Issue #22's "correctness > savings" rule,
    now a theorem): a plain read of ANY receiver type — not a
    slice, not an LHS target, not optional-chained, not provably
    in-bounds — ALWAYS routes through the Python-semantics helper.
    No such input can silently lower to a bare native `x[i]`
    (the silent-`undefined` failure mode). Since `helperTy` now holds of
    every `RecvTy` (incl. `float`/`set`), this covers the whole read domain:
    non-subscriptable receivers are ROUTED to `pyGetItem`. (This theorem proves
    only the ROUTING — that such reads reach the helper. The helper's own
    `TypeError` behavior for non-subscriptable receivers is enforced by the
    runtime guard and bound by the C4 shipping-binding harness + the CLI
    `test_run_nonsubscriptable_raises_typeerror`, NOT by this proof.) -/
theorem route_read_safety (s : Site)
    (hty : helperTy s.ty = true) (hs : s.isSlice = false)
    (hl : s.isLhs = false) (ho : s.isOptional = false)
    (hi : s.inbounds = false) :
    routeOf s = .helper := by
  simp [routeOf, hs, hi, hl, ho, hty]

/-- Slices always take the slice helper — never a native lowering. -/
theorem route_slice_total (s : Site) (hs : s.isSlice = true) :
    routeOf s = .pySlice := by
  simp [routeOf, hs]

/-- The native-inbounds fast path fires only off the in-bounds proof
    obligation (and never intercepts a slice). -/
theorem route_inbounds (s : Site) (hs : s.isSlice = false)
    (hi : s.inbounds = true) : routeOf s = .nativeInbounds := by
  simp [routeOf, hs, hi]

/-- Exhaustiveness: every site takes exactly one of the four routes
    (totality/determinism of the decision procedure — `routeOf` is a
    total function, so determinism is by construction; this states the
    codomain is covered). -/
theorem route_cases (s : Site) :
    routeOf s = .pySlice ∨ routeOf s = .nativeInbounds
      ∨ routeOf s = .helper ∨ routeOf s = .native := by
  unfold routeOf
  split
  · exact Or.inl rfl
  · split
    · exact Or.inr (Or.inl rfl)
    · split
      · exact Or.inr (Or.inr (Or.inl rfl))
      · exact Or.inr (Or.inr (Or.inr rfl))

/-! ### Decision-table generation (the Rust↔Lean binding surface) -/

def RecvTy.all : List RecvTy :=
  [.primitive, .float, .list, .dict, .set, .tuple, .unknown]

def RecvTy.tname : RecvTy → String
  | .primitive => "primitive"
  | .float => "float"
  | .list => "list"
  | .dict => "dict"
  | .set => "set"
  | .tuple => "tuple"
  | .unknown => "unknown"

def RouteK.rname : RouteK → String
  | .pySlice => "pySlice"
  | .nativeInbounds => "native-inbounds"
  | .helper => "pyGetItem"
  | .native => "native"

def boolBit (b : Bool) : String := if b then "1" else "0"

/-- Must byte-match `cert.rs::decision_table()` (same loop order, same
    names). -/
def routeTable : String := Id.run do
  let mut out := ""
  for isSlice in [false, true] do
    for isLhs in [false, true] do
      for isOptional in [false, true] do
        for inbounds in [false, true] do
          for ty in RecvTy.all do
            let s : Site := ⟨isSlice, isLhs, isOptional, inbounds, ty⟩
            out := out ++ boolBit isSlice ++ " " ++ boolBit isLhs ++ " "
              ++ boolBit isOptional ++ " " ++ boolBit inbounds ++ " "
              ++ ty.tname ++ " -> " ++ (routeOf s).rname ++ "\n"
  return out

/-- info: 'PythExpandVerify.route_read_safety' depends on axioms: [propext] -/
#guard_msgs in #print axioms route_read_safety

/-! ### Comp₀ — the certificate CHECKER is sound (Leroy CACM §2.2, property (7))

The theorems above pin the routing *rules*. `cert.rs::check_certificate`
is the independent checker that codegen's output must pass: per site it
re-applies `routeOf` AND (since the Tier-0 in-bounds side condition,
PR #368) re-derives the `inbounds` bit from the recorded evidence rather
than trusting it. Here we model that checker and prove Leroy's property
(7): **if the checker accepts a site, the recorded route IS the rule's
route and the in-bounds bit IS what its evidence justifies** — so a
checker-accepted certificate inherits `route_read_safety`. This is the
"validate a posteriori" half — the rules are proved above; this says the
checker actually enforces them, closing the gap that a `Comp₀`
construction needs a *verified* validator (release_plan_v2 §8.1). The
artifact-consistency half (helper-call counts, WASM exports) stays
trusted, as stated in `cert.rs`. -/

/-- Static evidence for the in-bounds fast path (Lean twin of
    `cert.rs::InboundsEvidence`). `index : Int` models the Rust `i128`;
    a value outside `[0, listLen)` is simply not in bounds. -/
structure InboundsEvidence where
  listLen : Nat
  index : Int
deriving DecidableEq, Repr

/-- The decidable side condition `0 ≤ index < listLen`. -/
def InboundsEvidence.isInbounds (e : InboundsEvidence) : Bool :=
  decide (0 ≤ e.index ∧ e.index < (e.listLen : Int))

/-- The evidence-justified in-bounds bit (absent evidence ⇒ `false`),
    twin of `check_certificate`'s `derived_inbounds`. -/
def justifiedInbounds (e : Option InboundsEvidence) : Bool :=
  (e.map InboundsEvidence.isInbounds).getD false

/-- A recorded certificate entry, projected to the fields the checker
    validates (Lean twin of `cert.rs::SiteRecord`). -/
structure SiteRecord where
  input : Site
  evidence : Option InboundsEvidence
  route : RouteK
deriving Repr

/-- The checker's per-site acceptance test: rule check ∧ in-bounds side
    condition (twin of the two per-site checks in `check_certificate`). -/
def checkSite (r : SiteRecord) : Bool :=
  decide (routeOf r.input = r.route)
    && decide (r.input.inbounds = justifiedInbounds r.evidence)

/-- The whole-certificate checker (twin of the per-site loop). -/
def checkCert (cert : List SiteRecord) : Bool :=
  cert.all checkSite

/-- **Comp₀, per site (Leroy property (7)).** If the checker accepts a
    recorded site, the recorded route is exactly the rule's route and the
    in-bounds bit is exactly what its evidence justifies — both
    re-derived, nothing trusted. -/
theorem checkSite_sound (r : SiteRecord) (h : checkSite r = true) :
    r.route = routeOf r.input
      ∧ r.input.inbounds = justifiedInbounds r.evidence := by
  simp only [checkSite, Bool.and_eq_true, decide_eq_true_eq] at h
  exact ⟨h.1.symm, h.2⟩

/-- **Comp₀, whole certificate.** A checker-accepted certificate has, at
    every recorded site, the rule's route and a justified in-bounds bit. -/
theorem checkCert_sound (cert : List SiteRecord)
    (h : checkCert cert = true) (r : SiteRecord) (hr : r ∈ cert) :
    r.route = routeOf r.input
      ∧ r.input.inbounds = justifiedInbounds r.evidence := by
  have hsite : checkSite r = true := by
    rw [checkCert, List.all_eq_true] at h
    exact h r hr
  exact checkSite_sound r hsite

/-- **The payoff — read-safety transported to the validated artifact.**
    On a checker-accepted certificate, every Python-typed plain read that
    is not provably in bounds has RECORDED route `helper`: the
    `route_read_safety` invariant now holds of what the checker accepts,
    not merely of the abstract rule. This is the end-to-end credible-
    compilation guarantee for the subscript pass. -/
theorem checked_read_safety (cert : List SiteRecord)
    (h : checkCert cert = true) (r : SiteRecord) (hr : r ∈ cert)
    (hty : helperTy r.input.ty = true) (hs : r.input.isSlice = false)
    (hl : r.input.isLhs = false) (ho : r.input.isOptional = false)
    (hi : r.input.inbounds = false) :
    r.route = .helper := by
  rw [(checkCert_sound cert h r hr).1, route_read_safety r.input hty hs hl ho hi]

/-- info: 'PythExpandVerify.checked_read_safety' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms checked_read_safety

/-! ### Certificate COMPLETENESS — proved from a MODEL of the emitter, not assumed

`checkCert = cert.all checkSite`, so `checkCert [] = true` and `checkCert_sound`
/ `checked_read_safety` quantify only over records that are PRESENT (`r ∈ cert`).
On their own they say nothing about a subscript site the compiler EMITTED but
failed to record. Completeness is the missing half. Rather than ASSUME coverage
as a hypothesis (which would smuggle the conclusion), we MODEL the emitter's
subscript loop and PROVE that the certificate it produces covers every emitted
site. `emitCert` mirrors `emit.rs`'s single `Subscript` path (emit.rs:9556-9608):
for each emitted site it derives the in-bounds bit from the recorded evidence
(9577-9579) and records `route(site)`'s decision (9593-9608) — exactly one
record per emitted subscript, in order. From that MODEL we prove: the emitted
certificate always passes the checker (`emitCert_checks`), its recorded inputs
ARE the emitted sites (`emitCert_covers` — the coverage invariant, PROVED), and
therefore every emitted Python-typed unsafe read is both PRESENT and routed to
`helper` (`emitted_read_safety`). The remaining trust obligation is only that the
Lean `emitCert` model matches the Rust `emit.rs` loop (audited, like every model
here), NOT an assumed coverage equation. -/

/-- The recorded inputs, in order — the certificate's view of the emitted sites. -/
def certInputs (cert : List SiteRecord) : List Site := cert.map (·.input)

/-- Per-site source data the emitter has BEFORE it records (the `SiteInput`
    fields + the static in-bounds evidence), Lean twin of emit.rs's locals at
    the `Subscript` arm. The `inbounds` bit is DERIVED from the evidence, not a
    free field — mirroring `provably_inbounds = evidence.map(is_inbounds)…`. -/
structure SiteSrc where
  isSlice : Bool
  isLhs : Bool
  isOptional : Bool
  ty : RecvTy
  evidence : Option InboundsEvidence
deriving Repr

/-- The site the emitter builds from the source (inbounds derived from evidence). -/
def SiteSrc.toSite (x : SiteSrc) : Site :=
  ⟨x.isSlice, x.isLhs, x.isOptional, justifiedInbounds x.evidence, x.ty⟩

/-- The record the emitter pushes for one site: `route(site)`'s decision +
    the site + its evidence (emit.rs:9593-9608). One per emitted subscript. -/
def emitRecord (x : SiteSrc) : SiteRecord :=
  ⟨x.toSite, x.evidence, routeOf x.toSite⟩

/-- The emitter's whole certificate: one record per emitted site, in order
    (the model of emit.rs's `sites.push` loop). -/
def emitCert (srcs : List SiteSrc) : List SiteRecord := srcs.map emitRecord

/-- Each emitted record passes the per-site checker (route = `routeOf`, in-bounds
    bit = evidence-justified), both by construction. -/
theorem emitRecord_checks (x : SiteSrc) : checkSite (emitRecord x) = true := by
  simp [checkSite, emitRecord, SiteSrc.toSite]

/-- **The emitted certificate always passes its own checker.** -/
theorem emitCert_checks (srcs : List SiteSrc) : checkCert (emitCert srcs) = true := by
  simp only [checkCert, emitCert, List.all_eq_true]
  intro r hr
  obtain ⟨x, _, rfl⟩ := List.mem_map.mp hr
  exact emitRecord_checks x

/-- **Coverage, PROVED (not assumed).** The certificate the emitter produces has,
    as its recorded inputs, EXACTLY the emitted sites — in order. This is the
    invariant that was smuggled as a hypothesis before; here it is a theorem
    about the emitter model. -/
theorem emitCert_covers (srcs : List SiteSrc) :
    certInputs (emitCert srcs) = srcs.map SiteSrc.toSite := by
  simp only [certInputs, emitCert, List.map_map]
  rfl

/-- **Completeness payoff — every EMITTED unsafe read is present AND helper-routed.**
    For a Python-typed plain read (helper type, not slice/lhs/optional, not
    provably in bounds) that the emitter produced, the emitted certificate
    contains its record and that record's route is `helper`. No emitted unsafe
    read can be omitted (it is in `emitCert srcs`) or mis-routed (route = helper),
    proved from the emitter model — the "omitted site" gap closed at the root. -/
theorem emitted_read_safety (srcs : List SiteSrc) (x : SiteSrc) (hx : x ∈ srcs)
    (hty : helperTy x.ty = true) (hsl : x.isSlice = false)
    (hlhs : x.isLhs = false) (hopt : x.isOptional = false)
    (hib : justifiedInbounds x.evidence = false) :
    emitRecord x ∈ emitCert srcs ∧ (emitRecord x).route = .helper := by
  refine ⟨List.mem_map.mpr ⟨x, hx, rfl⟩, ?_⟩
  simp only [emitRecord]
  exact route_read_safety x.toSite hty hsl hlhs hopt hib

-- Non-vacuity: a concrete emitted unsafe read (plain dict subscript) — its
-- emitted record passes the checker, is present, and is helper-routed.
#guard (let x : SiteSrc := ⟨false, false, false, .dict, none⟩
        checkCert (emitCert [x]) && decide ((emitRecord x).route = RouteK.helper)
          && decide (certInputs (emitCert [x]) = [x.toSite]))
-- checkCert [] = true stays true, but it now describes the EMPTY emission only
-- (emitCert [] = []); a nonempty emission yields a nonempty covering cert.
#guard checkCert [] = true
#guard emitCert [] = []

/-- info: 'PythExpandVerify.emitCert_checks' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms emitCert_checks

/-- info: 'PythExpandVerify.emitCert_covers' depends on axioms: [propext] -/
#guard_msgs in #print axioms emitCert_covers

/-- info: 'PythExpandVerify.emitted_read_safety' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms emitted_read_safety

/-! ### Coverage as a DECIDABLE CHECKED property (C2) — OCCURRENCE-sensitive, not set-only

`emitCert_covers` proves coverage for the MODELED emitter, but that is a fact
about a Lean model of `emit.rs`. A reviewer of the SHIPPED artifact wants
coverage CHECKED on the actual emitted-site list, without trusting that the Lean
`emitCert` model reproduces `emit.rs`. So we also expose a DECIDABLE checker
`coversAll emitted cert` and prove the real conditional theorem
`checkCert_complete`: IF the checker accepts THEN coverage holds — no hypothesis
`certInputs cert = emitted`, no smuggled invariant.

**C2 (occurrence coverage, not set membership).** `Site` has no source
identity/offset, so a set-membership check (`s ∈ certInputs cert`) would call the
DUPLICATED emission `[s, s]` "covered" by a SINGLE record `[⟨s, …⟩]` — an omitted
duplicate occurrence would slip through. This mirrors exactly the count-vs-swap
gap the JS subscript cert closed (see `TRUST.md`, "Subscript certificate
artifact-consistency is positional, not count-only"). Here the coverage axis is
OMISSION (not route-swap), whose faithful notion is MULTISET containment: every
emitted OCCURRENCE of a site value must be matched by its OWN cert record. So
`coversAll` checks, per emitted site value `s`, that its multiplicity among the
emitted list is ≤ its multiplicity among the recorded inputs
(`siteOccurrences s emitted ≤ siteOccurrences s (certInputs cert)`) — decidable
via `List.countP` (`Site` has `DecidableEq`). A missing site AND an omitted
duplicate both make it FAIL (pinned by `#guard`). Combined with `checkCert_sound`
this gives `covered_read_safety`: on a cert that both PASSES `checkCert` and
covers the emitted OCCURRENCES, every emitted Python-typed unsafe read has at
least as many recorded matching records as it has occurrences, and EVERY recorded
record for it is `helper`-routed. The residual trust obligation shrinks to naming
the true emitted-site list to the checker — an audited input, not an assumed
equation. -/

/-- Occurrence count of a site VALUE in a site list — the multiplicity used for
    multiset (occurrence-sensitive) coverage. `Site` has `DecidableEq`, so the
    predicate `x = s` is decidable and `countP` evaluates. -/
def siteOccurrences (s : Site) (l : List Site) : Nat :=
  l.countP (fun x => decide (x = s))

/-- Decidable OCCURRENCE coverage (C2): every emitted site VALUE is recorded in
    the certificate AT LEAST AS MANY TIMES as it is emitted — multiset
    containment, not set membership. A single record can no longer "cover" two
    emitted occurrences of the same site. -/
def coversAll (emitted : List Site) (cert : List SiteRecord) : Bool :=
  emitted.all (fun s => decide (siteOccurrences s emitted ≤ siteOccurrences s (certInputs cert)))

/-- **Coverage completeness, CHECKED and OCCURRENCE-sensitive (not assumed, not
    set-only).** If the decidable coverage checker accepts, then for every emitted
    site value its multiplicity among the recorded inputs is at least its
    multiplicity among the emitted occurrences — so no emitted occurrence (not
    even a DUPLICATE) is unrecorded. The old set-membership form would have
    accepted an omitted duplicate; this multiset form does not. -/
theorem checkCert_complete (emitted : List Site) (cert : List SiteRecord)
    (h : coversAll emitted cert = true) (s : Site) (hs : s ∈ emitted) :
    siteOccurrences s emitted ≤ siteOccurrences s (certInputs cert) := by
  rw [coversAll, List.all_eq_true] at h
  exact of_decide_eq_true (h s hs)

/-- Occurrences of a site value among the recorded inputs count exactly the cert
    RECORDS whose input is that site — so the multiplicity bound below is a bound
    on matching records. -/
theorem siteOccurrences_certInputs (s : Site) (cert : List SiteRecord) :
    siteOccurrences s (certInputs cert) = cert.countP (fun r => decide (r.input = s)) := by
  simp only [siteOccurrences, certInputs, List.countP_map, Function.comp_def]

/-- info: 'PythExpandVerify.siteOccurrences_certInputs' depends on axioms: [propext] -/
#guard_msgs in #print axioms siteOccurrences_certInputs

/-- **End-to-end OCCURRENCE-coverage payoff.** On a certificate that both PASSES
    the checker (`checkCert`) and covers the emitted OCCURRENCES (`coversAll`),
    every emitted Python-typed unsafe read (1) is matched by at least as many cert
    records as it has emitted occurrences — so an omitted duplicate is impossible —
    AND (2) EVERY cert record recording it is `helper`-routed. Soundness of the
    validated artifact, coverage checked (occurrence-sensitive) not assumed. -/
theorem covered_read_safety (emitted : List Site) (cert : List SiteRecord)
    (hchk : checkCert cert = true) (hcov : coversAll emitted cert = true)
    (s : Site) (hs : s ∈ emitted)
    (hty : helperTy s.ty = true) (hsl : s.isSlice = false)
    (hlhs : s.isLhs = false) (hopt : s.isOptional = false)
    (hib : s.inbounds = false) :
    siteOccurrences s emitted ≤ cert.countP (fun r => decide (r.input = s))
      ∧ ∀ r ∈ cert, r.input = s → r.route = .helper := by
  refine ⟨?_, ?_⟩
  · rw [← siteOccurrences_certInputs]
    exact checkCert_complete emitted cert hcov s hs
  · intro r hr hrs
    rw [(checkCert_sound cert hchk r hr).1, hrs]
    exact route_read_safety s hty hsl hlhs hopt hib

-- Non-vacuity + OCCURRENCE-sensitivity witnesses:
-- (1) a covering cert PASSES and its record checks out;
#guard (let s : Site := ⟨false, false, false, false, .dict⟩
        coversAll [s] [⟨s, none, routeOf s⟩] && checkCert [⟨s, none, routeOf s⟩])
-- (2) a MISSING emitted site makes coverage FAIL (as before);
#guard (let s : Site := ⟨false, false, false, false, .dict⟩
        let missing : Site := ⟨true, false, false, false, .list⟩
        coversAll [s, missing] [⟨s, none, routeOf s⟩] = false)
-- (3) THE C2 FIX — an omitted DUPLICATE occurrence now FAILS coverage: two
--     emitted `s` occurrences are NOT covered by a single record (set membership
--     would have wrongly PASSED this);
#guard (let s : Site := ⟨false, false, false, false, .dict⟩
        coversAll [s, s] [⟨s, none, routeOf s⟩] = false)
-- (4) ...and two records DO cover the two occurrences (so the check is not
--     vacuously false — the duplicate is coverable, just not by one record).
#guard (let s : Site := ⟨false, false, false, false, .dict⟩
        coversAll [s, s] [⟨s, none, routeOf s⟩, ⟨s, none, routeOf s⟩] = true)

/-- info: 'PythExpandVerify.checkCert_complete' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms checkCert_complete

/-- info: 'PythExpandVerify.covered_read_safety' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms covered_read_safety

/-! ## WasmAdmission — soundness of WASM auto-routing admission

Lean twin of the WASM admission boundary in
`crates/pyths_hir/src/wasm_analysis.rs::is_wasm_eligible` (the gate deciding
which functions the compiler lowers to WASM under `--target js+wasm`) and its
lowering `crates/pyths_codegen_wasm/src/types.rs::to_wasm_type`.

**The soundness claim (HONEST — not completeness, not value-equivalence).**
Every boundary type the compiler ADMITS has a concrete WASM representation:
`isWasmEligible t = true → (toWasmType t).isSome`. This is exactly the
invariant a real over-admission violated — `is_wasm_eligible` used to accept
`Optional`, which `to_wasm_type` cannot lower, so codegen `unwrap()`ed a
`None` and PANICKED (`emit.rs`'s `to_wasm_type(ty).unwrap()`). We do NOT
claim the reverse (completeness: that everything lowerable is accepted), nor
that the JS and WASM lowerings compute the same value (that is the
differential / Livermore-WASM oracle layer's job). The point is narrow:
nothing lacking a WASM boundary representation is admitted.

**Binding to the shipping compiler.** `isWasmEligible` mirrors the Rust match
arm-for-arm and `toWasmType` mirrors `to_wasm_type` arm-for-arm (int→i64,
bool→i32, float→f64, str/collections→pointer; `Optional` falls through to
`none`). Both are bound to the shipping functions by the enumerated table
`verification/wasm-admission-table.txt`: Rust prints it via
`cert::admission_table()` (checked by `wasm_admission_table_matches_committed_fixture`),
this side via `expanddiff --check-wasm-admission-table` — either side changing
a rule breaks its own gate. Every row also witnesses the theorem finitely (no
row has elig=1, lower=0).

**Modeling scope (stated plainly).** The Rust `Tuple`/`Callable` take
arbitrary arity (`.all(is_wasm_eligible_inner)`); the table exercises — and
this model covers — the binary-tuple / unary-callable shapes the table
enumerates. The soundness argument is identical at every arity (all-inner);
the n-ary residual is covered by the Rust `admission_table_is_sound_on_every_row`
test + the examples corpus certificate, not by this theorem. -/

inductive WasmTy where
  | int | float | bool | str | wnone | any | void | named
  | list (inner : WasmTy)
  | set (inner : WasmTy)
  | opt (inner : WasmTy)
  | dict (k v : WasmTy)
  | tuple (a b : WasmTy)
  | callable (p r : WasmTy)
deriving DecidableEq, Repr

/-- The WASM value-type lattice at the boundary — mirror of
    `crates/pyths_codegen_wasm/src/types.rs::WasmType`. -/
inductive WasmRepr where
  | i32 | i64 | f64 | ptr
  | ptrList (elem : WasmRepr)
  | ptrDict (k v : WasmRepr)
  | ptrTuple (a b : WasmRepr)
  | ptrClosure (p : WasmRepr) (r : Option WasmRepr)
deriving Repr

def WasmTy.isAny : WasmTy → Bool
  | .any => true
  | _ => false

def WasmTy.isNoneOrVoid : WasmTy → Bool
  | .wnone | .void => true
  | _ => false

/-- THE admission gate — `is_wasm_eligible`, arm-for-arm. Element positions
    use the `_inner` variant (`t == any ∨ isWasmEligible t`), inlined as
    `isWasmEligible t || t.isAny` so the recursion is purely structural.
    `opt` is NOT admitted (falls to `_`), matching the fix that removed the
    unsound `Type::Optional` arm. -/
def isWasmEligible : WasmTy → Bool
  | .int | .float | .bool | .str => true
  | .list i | .set i => isWasmEligible i || i.isAny
  | .dict k v => (isWasmEligible k || k.isAny) && (isWasmEligible v || v.isAny)
  | .tuple a b => (isWasmEligible a || a.isAny) && (isWasmEligible b || b.isAny)
  | .callable p r =>
      (isWasmEligible p || p.isAny)
        && (r.isNoneOrVoid || isWasmEligible r || r.isAny)
  | _ => false

/-- THE lowering — `to_wasm_type`, arm-for-arm, returning the actual
    representation (so `.isSome` is a genuine consequence, not a restatement
    of `isWasmEligible`). Element positions inline `to_wasm_inner`
    (`t == any → ptr, else to_wasm_type t`). `opt` has NO arm → `none`. -/
def toWasmType : WasmTy → Option WasmRepr
  | .int => some .i64
  | .float => some .f64
  | .bool => some .i32
  | .str => some .ptr
  | .list i | .set i =>
      (if i.isAny then some .ptr else toWasmType i).map (WasmRepr.ptrList ·)
  | .dict k v =>
      match (if k.isAny then some .ptr else toWasmType k),
            (if v.isAny then some .ptr else toWasmType v) with
      | some kt, some vt => some (.ptrDict kt vt)
      | _, _ => none
  | .tuple a b =>
      match (if a.isAny then some .ptr else toWasmType a),
            (if b.isAny then some .ptr else toWasmType b) with
      | some at2, some bt => some (.ptrTuple at2 bt)
      | _, _ => none
  | .callable p r =>
      match (if p.isAny then some .ptr else toWasmType p) with
      | some pt =>
          if r.isNoneOrVoid then some (.ptrClosure pt none)
          else (if r.isAny then some .ptr else toWasmType r).map
                 (fun rt => .ptrClosure pt (some rt))
      | none => none
  | _ => none

/-- Element-position helper lemma: if the inner-eligibility flag
    (`isWasmEligible i || i.isAny`) holds, the inner lowering
    (`if i.isAny then some ptr else toWasmType i`) succeeds. -/
theorem inner_sound (i : WasmTy)
    (ih : isWasmEligible i = true → (toWasmType i).isSome = true)
    (h : (isWasmEligible i || i.isAny) = true) :
    (if i.isAny then (some WasmRepr.ptr) else toWasmType i).isSome = true := by
  by_cases ha : i.isAny = true
  · simp [ha]
  · simp only [Bool.or_eq_true] at h
    have hi : isWasmEligible i = true := by
      cases h with
      | inl h => exact h
      | inr h => exact absurd h ha
    simp [ha, ih hi]

/-- **WASM-admission soundness** (the over-admission failure mode, now a
    theorem): every boundary type the admission gate accepts has a concrete
    WASM lowering. No admitted type can reach codegen's
    `to_wasm_type(ty).unwrap()` as a `None`. -/
theorem wasm_admission_sound (t : WasmTy) :
    isWasmEligible t = true → (toWasmType t).isSome = true := by
  induction t with
  | int | float | bool | str => intro _; rfl
  | wnone | any | void | named => intro h; simp [isWasmEligible] at h
  | list i ih | set i ih =>
      intro h
      simp only [isWasmEligible] at h
      have hi := inner_sound i ih h
      rw [Option.isSome_iff_exists] at hi
      obtain ⟨it, hit⟩ := hi
      simp [toWasmType, hit]
  | opt i _ => intro h; simp [isWasmEligible] at h
  | dict k v ihk ihv =>
      intro h
      simp only [isWasmEligible, Bool.and_eq_true] at h
      have hk := inner_sound k ihk h.1
      have hv := inner_sound v ihv h.2
      rw [Option.isSome_iff_exists] at hk hv
      obtain ⟨kt, hkt⟩ := hk
      obtain ⟨vt, hvt⟩ := hv
      simp [toWasmType, hkt, hvt]
  | tuple a b iha ihb =>
      intro h
      simp only [isWasmEligible, Bool.and_eq_true] at h
      have ha := inner_sound a iha h.1
      have hb := inner_sound b ihb h.2
      rw [Option.isSome_iff_exists] at ha hb
      obtain ⟨at2, hat⟩ := ha
      obtain ⟨bt, hbt⟩ := hb
      simp [toWasmType, hat, hbt]
  | callable p r ihp ihr =>
      intro h
      simp only [isWasmEligible, Bool.and_eq_true] at h
      have hp := inner_sound p ihp h.1
      rw [Option.isSome_iff_exists] at hp
      obtain ⟨pt, hpt⟩ := hp
      by_cases hr : r.isNoneOrVoid = true
      · -- no-value return: the closure lowers with `ret = none`.
        simp [toWasmType, hpt, hr]
      · -- r is a real value type; from h.2 its inner lowering succeeds.
        have hrf : r.isNoneOrVoid = false := by
          cases hh : r.isNoneOrVoid with
          | true => exact absurd hh hr
          | false => rfl
        have hr2 : (isWasmEligible r || r.isAny) = true := by
          have h2 := h.2
          rw [hrf, Bool.false_or] at h2
          exact h2
        have hrl := inner_sound r ihr hr2
        rw [Option.isSome_iff_exists] at hrl
        obtain ⟨rt, hrt⟩ := hrl
        simp [toWasmType, hpt, hr, hrt]

/-- Totality/exhaustiveness: `isWasmEligible` is a total decision procedure
    (it is a total Lean function, so every type gets exactly one verdict).
    Stated as decidability of the admission predicate. -/
theorem wasm_admission_total (t : WasmTy) :
    isWasmEligible t = true ∨ isWasmEligible t = false := by
  cases h : isWasmEligible t
  · exact Or.inr rfl
  · exact Or.inl rfl

/-! ### Admission-table generation (the Rust↔Lean binding surface) -/

def wasmBit (b : Bool) : String := if b then "1" else "0"

/-- Must byte-match `cert::admission_table()` (same base alphabet, same
    constructor block order, same shape names). Each row:
    `<shape> -> <isWasmEligible-bit> <toWasmType-isSome-bit>`. -/
def wasmBaseAlpha : List (String × WasmTy) :=
  [("int", .int), ("float", .float), ("bool", .bool), ("str", .str),
   ("none", .wnone), ("any", .any), ("void", .void), ("named", .named)]

def wasmRow (name : String) (t : WasmTy) : String :=
  name ++ " -> " ++ wasmBit (isWasmEligible t) ++ " "
    ++ wasmBit (toWasmType t).isSome ++ "\n"

def wasmAdmissionTable : String := Id.run do
  let mut out := ""
  -- 1. leaves
  for (an, aTy) in wasmBaseAlpha do
    out := out ++ wasmRow an aTy
  -- 2. list<a>
  for (an, aTy) in wasmBaseAlpha do
    out := out ++ wasmRow ("list<" ++ an ++ ">") (.list aTy)
  -- 3. set<a>
  for (an, aTy) in wasmBaseAlpha do
    out := out ++ wasmRow ("set<" ++ an ++ ">") (.set aTy)
  -- 4. opt<a>
  for (an, aTy) in wasmBaseAlpha do
    out := out ++ wasmRow ("opt<" ++ an ++ ">") (.opt aTy)
  -- 5. dict<a,b>
  for (an, aTy) in wasmBaseAlpha do
    for (bn, bTy) in wasmBaseAlpha do
      out := out ++ wasmRow ("dict<" ++ an ++ "," ++ bn ++ ">") (.dict aTy bTy)
  -- 6. tuple<a,b>
  for (an, aTy) in wasmBaseAlpha do
    for (bn, bTy) in wasmBaseAlpha do
      out := out ++ wasmRow ("tuple<" ++ an ++ "," ++ bn ++ ">") (.tuple aTy bTy)
  -- 7. callable<a,b>
  for (an, aTy) in wasmBaseAlpha do
    for (bn, bTy) in wasmBaseAlpha do
      out := out ++ wasmRow ("callable<" ++ an ++ "," ++ bn ++ ">") (.callable aTy bTy)
  -- 8. list<list<a>>
  for (an, aTy) in wasmBaseAlpha do
    out := out ++ wasmRow ("list<list<" ++ an ++ ">>") (.list (.list aTy))
  return out

-- `Quot.sound` enters via the `Option`/`Bool` `simp` normal forms over the
-- executable `toWasmType` (the same standard base the classifier theorems in
-- TRUST.md rest on). It is one of Lean core's three standard axioms — not a
-- proof hole and not a custom one — and is pinned here so it cannot drift.
/-- info: 'PythExpandVerify.wasm_admission_sound' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms wasm_admission_sound

/-! ## PySlice — the slice-family verified core (Tier 1, release_plan_v2 §9)

Lean model of the SHIPPING runtime helpers `pySlice` / `pySetSlice` /
`pyDelSlice` (runtime/src/runtime.js) — the CPython `slice.indices`
normalization + walk they share. The flagship theorem `slice_walk_inbounds`
is **clamping totality**: for ANY start/stop, ANY nonzero step (both signs),
ANY out-of-range endpoint, every index the walk visits is a real in-bounds
access `0 ≤ i < len`. Its violation IS the highest-bug-density class in the
compiler — PBT-1 (silent `None`-padding on `['a','b'][7::-1]`), #281
(read/assign clamp), #321 (delete path), #278/#279 (negative-index write).

Spec validated Stage-3 (lean-spec-quality) against CPython over 25,200
boundary cases (`experiments/pbt-ps/slice_spec_validate.py`, DRY). The
`#guard`s below bind THIS model to concrete CPython-verified cases at build
time (an executable model↔reality check, incl. the PBT-1 regression case). -/

/-- slice.indices lower walk bound (step-sign dependent). -/
def sliceLo (step : Int) : Int := if step < 0 then -1 else 0

/-- slice.indices upper walk bound. -/
def sliceHi (len : Nat) (step : Int) : Int :=
  if step < 0 then (len : Int) - 1 else (len : Int)

/-- Normalize the START endpoint exactly as `pySlice` does (runtime.js:416-418). -/
def normStart (len : Nat) (step : Int) : Option Int → Int
  | none => if step < 0 then sliceHi len step else sliceLo step
  | some s => if s < 0 then max (s + (len : Int)) (sliceLo step)
              else min s (sliceHi len step)

/-- Normalize the STOP endpoint (differs from start only in the `none` case;
    runtime.js:420-422). -/
def normStop (len : Nat) (step : Int) : Option Int → Int
  | none => if step < 0 then sliceLo step else sliceHi len step
  | some s => if s < 0 then max (s + (len : Int)) (sliceLo step)
              else min s (sliceHi len step)

/-- Indices visited walking from `i` toward `stop` by `step`, fuel-bounded
    (`len+1` suffices since |step| ≥ 1). Fuel truncation can only DROP indices,
    never add an out-of-range one, so `slice_walk_inbounds` is robust to it. -/
def walkAux (step stop : Int) : Nat → Int → List Int
  | 0, _ => []
  | fuel + 1, i =>
      if (if step > 0 then i < stop else i > stop) then
        i :: walkAux step stop fuel (i + step)
      else []

/-- Indices visited by `pySlice(len, start, stop, step)` for `step ≠ 0`. -/
def sliceWalk (len : Nat) (start stop : Option Int) (step : Int) : List Int :=
  walkAux step (normStop len step stop) (len + 1) (normStart len step start)

-- Executable model↔reality binding (each RHS is `list(range(len))[start:stop:step]`
-- under CPython). PBT-1 regression case first.
/-- info: [1, 0] -/
#guard_msgs in #eval sliceWalk 2 (some 7) none (-1)   -- ['a','b'][7::-1]
#guard sliceWalk 2 (some 7) none (-1) = [1, 0]
#guard sliceWalk 5 (some 1) (some 4) 1 = [1, 2, 3]
#guard sliceWalk 5 none none 2 = [0, 2, 4]
#guard sliceWalk 3 none none (-1) = [2, 1, 0]
#guard sliceWalk 0 none none 1 = []
#guard sliceWalk 3 (some 9) (some 100) 1 = []
#guard sliceWalk 4 (some (-1)) (some (-9)) (-2) = [3, 1]  -- range(4)[-1:-9:-2]

/-- The normalized `(start', stop', step)` triple — the Lean twin of CPython's
    `slice(start, stop, step).indices(len)`. Stronger binding than the walk
    (pins the triple, not just the visited indices). -/
def sliceIndices (len : Nat) (start stop : Option Int) (step : Int) : Int × Int × Int :=
  (normStart len step start, normStop len step stop, step)

-- CPython reference-suite conformance (Perry §13): the 9 authored vectors from
-- CPython `Lib/test/test_slice.py :: test_indices()`, pinned here so the model
-- is bound to CPython's OWN regression cases, not only our generated sweep.
-- Cross-validated on 48,672 cases in experiments/pbt-ps/slice_cpython_conformance.py.
#guard sliceIndices 10 none none 1 = (0, 10, 1)              -- slice(None).indices(10)
#guard sliceIndices 10 none none 2 = (0, 10, 2)              -- [::2]
#guard sliceIndices 10 (some 1) none 2 = (1, 10, 2)          -- [1::2]
#guard sliceIndices 10 none none (-1) = (9, -1, -1)          -- [::-1]
#guard sliceIndices 10 none (some (-9)) 1 = (0, 1, 1)        -- [:-9]
#guard sliceIndices 10 none (some (-10)) 1 = (0, 0, 1)       -- [:-10]
#guard sliceIndices 10 none (some (-10)) (-1) = (9, 0, -1)   -- [:-10:-1]
#guard sliceIndices 10 (some (-100)) (some 100) 1 = (0, 10, 1)     -- [-100:100]
#guard sliceIndices 10 (some 100) (some (-100)) (-1) = (9, -1, -1) -- [100:-100:-1]

/-- Endpoint normalization stays within the step-sign walk band (lower bound). -/
theorem sliceLo_le_normStart (len : Nat) (step : Int) (o : Option Int) :
    sliceLo step ≤ normStart len step o := by
  rcases o with _ | s <;> simp only [normStart, sliceLo, sliceHi] <;>
    (repeat' split) <;> omega

/-- … and the upper bound for the start endpoint. -/
theorem normStart_le_sliceHi (len : Nat) (step : Int) (o : Option Int) :
    normStart len step o ≤ sliceHi len step := by
  rcases o with _ | s <;> simp only [normStart, sliceLo, sliceHi] <;>
    (repeat' split) <;> omega

/-- Lower bound for the stop endpoint. -/
theorem sliceLo_le_normStop (len : Nat) (step : Int) (o : Option Int) :
    sliceLo step ≤ normStop len step o := by
  rcases o with _ | s <;> simp only [normStop, sliceLo, sliceHi] <;>
    (repeat' split) <;> omega

/-- Upper bound for the stop endpoint. -/
theorem normStop_le_sliceHi (len : Nat) (step : Int) (o : Option Int) :
    normStop len step o ≤ sliceHi len step := by
  rcases o with _ | s <;> simp only [normStop, sliceLo, sliceHi] <;>
    (repeat' split) <;> omega

/-- Walk-band invariant for `walkAux`: every visited index lies strictly on the
    step-sign side of `stop` and on the start side of `i0`. Fuel-agnostic. -/
private theorem walkAux_mem (step stop : Int) :
    ∀ (fuel : Nat) (i0 j : Int), j ∈ walkAux step stop fuel i0 →
      (step > 0 → i0 ≤ j ∧ j < stop) ∧ (step < 0 → stop < j ∧ j ≤ i0) := by
  intro fuel
  induction fuel with
  | zero => intro i0 j h; simp [walkAux] at h
  | succ n ih =>
    intro i0 j h
    by_cases hs : step > 0
    · simp [walkAux, hs] at h
      obtain ⟨hlt, hj⟩ := h
      rcases hj with rfl | htail
      · constructor <;> intro _ <;> refine ⟨?_, ?_⟩ <;> omega
      · have hih := ih (i0 + step) j htail
        exact ⟨fun h' => by have := hih.1 h'; refine ⟨?_, ?_⟩ <;> omega,
               fun h' => by have := hih.2 h'; refine ⟨?_, ?_⟩ <;> omega⟩
    · simp [walkAux, hs] at h
      obtain ⟨hgt, hj⟩ := h
      rcases hj with rfl | htail
      · constructor <;> intro _ <;> refine ⟨?_, ?_⟩ <;> omega
      · have hih := ih (i0 + step) j htail
        exact ⟨fun h' => by have := hih.1 h'; refine ⟨?_, ?_⟩ <;> omega,
               fun h' => by have := hih.2 h'; refine ⟨?_, ?_⟩ <;> omega⟩

/-- **Clamping totality (the flagship).** Every index the slice walk visits is
    a real in-bounds access, for ANY start/stop, ANY nonzero step (both signs),
    ANY out-of-range endpoint. The property PBT-1/#281/#321/#278/#279 violated. -/
theorem slice_walk_inbounds (len : Nat) (start stop : Option Int) (step : Int)
    (hstep : step ≠ 0) (i : Int) (hmem : i ∈ sliceWalk len start stop step) :
    0 ≤ i ∧ i < (len : Int) := by
  unfold sliceWalk at hmem
  have hw := walkAux_mem step (normStop len step stop) (len + 1)
    (normStart len step start) i hmem
  have h1 := sliceLo_le_normStart len step start
  have h2 := normStart_le_sliceHi len step start
  have h3 := sliceLo_le_normStop len step stop
  have h4 := normStop_le_sliceHi len step stop
  have e1 : sliceLo step = if step < 0 then -1 else 0 := rfl
  have e2 : sliceHi len step = if step < 0 then (len : Int) - 1 else (len : Int) := rfl
  rcases Int.lt_or_lt_of_ne hstep with hneg | hpos
  · have h := hw.2 hneg
    rw [if_pos hneg] at e1 e2
    refine ⟨?_, ?_⟩ <;> omega
  · have h := hw.1 hpos
    rw [if_neg (Int.not_lt.mpr (Int.le_of_lt hpos))] at e1 e2
    refine ⟨?_, ?_⟩ <;> omega

/-- info: 'PythExpandVerify.slice_walk_inbounds' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms slice_walk_inbounds

/-! ### Slice-walk COMPLETENESS — the walk visits EVERY on-grid index (no drop)

`slice_walk_inbounds` is SOUNDNESS only: every visited index is in bounds. A
`[]`-returning stub satisfies it vacuously, and fuel truncation could silently
DROP indices without tripping it. Completeness is the missing half: every index
CPython's `slice.indices` walk would produce — `normStart + n·step` while still
strictly on the step-side of `normStop` — is actually visited, and the `len+1`
fuel is provably sufficient (no truncation). Together with soundness this pins
the walk to EXACTLY CPython's index set, so no gap-hiding stub survives. -/

/-- `n` forward steps from `i0` (i.e. `i0 + n·step`), by iteration — no
    multiplication, so the arithmetic stays in `omega`'s linear fragment. -/
def stepN (i0 step : Int) : Nat → Int
  | 0 => i0
  | n + 1 => stepN (i0 + step) step n

/-- For `step > 0`, iterating only increases: `i0 + n ≤ stepN i0 step n`
    (each of the `n` steps adds `step ≥ 1`). -/
private theorem stepN_lb_pos (step : Int) (hpos : step > 0) :
    ∀ (n : Nat) (i0 : Int), i0 + (n : Int) ≤ stepN i0 step n := by
  intro n
  induction n with
  | zero => intro i0; simp [stepN]
  | succ m ih => intro i0; have := ih (i0 + step); simp only [stepN]; omega

/-- For `step < 0`, iterating only decreases: `stepN i0 step n ≤ i0 - n`. -/
private theorem stepN_ub_neg (step : Int) (hneg : step < 0) :
    ∀ (n : Nat) (i0 : Int), stepN i0 step n ≤ i0 - (n : Int) := by
  intro n
  induction n with
  | zero => intro i0; simp [stepN]
  | succ m ih => intro i0; have := ih (i0 + step); simp only [stepN]; omega

/-- **Walk completeness (fuel-parametric).** If the `n`-th on-grid index is
    still strictly on the step-side of `stop` and there is fuel to reach it,
    the walk visits it. -/
private theorem walkAux_complete (step stop : Int) (hstep : step ≠ 0) :
    ∀ (fuel n : Nat) (i0 : Int), n < fuel →
      (if step > 0 then stepN i0 step n < stop else stepN i0 step n > stop) →
      stepN i0 step n ∈ walkAux step stop fuel i0 := by
  intro fuel
  induction fuel with
  | zero => intro n i0 hn _; omega
  | succ f ih =>
    intro n i0 hn hside
    have hcond : (if step > 0 then i0 < stop else i0 > stop) := by
      rcases Int.lt_or_lt_of_ne hstep with hneg | hpos
      · rw [if_neg (by omega : ¬ step > 0)] at hside ⊢
        cases n with
        | zero => simpa [stepN] using hside
        | succ m =>
          have := stepN_ub_neg step hneg (m + 1) i0
          omega
      · rw [if_pos hpos] at hside ⊢
        cases n with
        | zero => simpa [stepN] using hside
        | succ m =>
          have := stepN_lb_pos step hpos (m + 1) i0
          omega
    have hexpand : walkAux step stop (f + 1) i0
        = (if (if step > 0 then i0 < stop else i0 > stop)
           then i0 :: walkAux step stop f (i0 + step) else []) := rfl
    rw [hexpand, if_pos hcond]
    cases n with
    | zero => simp only [stepN]; exact List.mem_cons_self
    | succ m =>
      simp only [stepN] at hside ⊢
      exact List.mem_cons_of_mem _ (ih m (i0 + step) (by omega) hside)

/-- **Slice-walk completeness (the flagship's missing half).** Every on-grid
    index `stepN normStart step n` still strictly on the step-side of
    `normStop` is visited by `sliceWalk` — the `len+1` fuel never truncates.
    A `[]`-stub or a fuel-dropping walk provably FAILS this. -/
theorem slice_walk_complete (len : Nat) (start stop : Option Int) (step : Int)
    (hstep : step ≠ 0) (n : Nat)
    (hside : if step > 0
             then stepN (normStart len step start) step n < normStop len step stop
             else stepN (normStart len step start) step n > normStop len step stop) :
    stepN (normStart len step start) step n ∈ sliceWalk len start stop step := by
  unfold sliceWalk
  refine walkAux_complete step (normStop len step stop) hstep (len + 1) n
    (normStart len step start) ?_ hside
  -- fuel sufficiency: `n < len + 1`, from the endpoint band + monotone stepping.
  have h1 := sliceLo_le_normStart len step start
  have h2 := normStart_le_sliceHi len step start
  have h3 := sliceLo_le_normStop len step stop
  have h4 := normStop_le_sliceHi len step stop
  have e1 : sliceLo step = if step < 0 then -1 else 0 := rfl
  have e2 : sliceHi len step = if step < 0 then (len : Int) - 1 else (len : Int) := rfl
  rcases Int.lt_or_lt_of_ne hstep with hneg | hpos
  · rw [if_neg (by omega : ¬ step > 0)] at hside
    rw [if_pos hneg] at e1 e2
    have := stepN_ub_neg step hneg n (normStart len step start)
    omega
  · rw [if_pos hpos] at hside
    rw [if_neg (Int.not_lt.mpr (Int.le_of_lt hpos))] at e1 e2
    have := stepN_lb_pos step hpos n (normStart len step start)
    omega

-- Completeness bindings: the PBT-1 case visits BOTH its indices (not a `[]`
-- stub), and each is the on-grid point CPython produces.
#guard stepN 7 (-1) 0 = 7      -- the walk clamps start 7→1; grid point n=6 is index 1
#guard sliceWalk 2 (some 7) none (-1) = [1, 0]
-- n=0 grid point of range(5)[1:4:1] is index 1, and it is visited.
#guard (1 : Int) ∈ sliceWalk 5 (some 1) (some 4) 1

/-- info: 'PythExpandVerify.slice_walk_complete' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms slice_walk_complete

/-! ### PyGetItem — scalar index normalization (Tier 2, §9)

Lean model of the shipping `pyGetItem` integer-key path (runtime.js:830-835):
normalize a negative key by `+n`, then require `0 ≤ i < n` (else IndexError).
`getIndex_inbounds` is the scalar sibling of `slice_walk_inbounds` — the
#278/#279 negative-index-OOB class made impossible. Spec validated Stage-3 vs
CPython on 150 cases (experiments/pbt-ps/tier2_spec_validate.py, DRY). -/

/-- The normalized in-bounds index of `key` into a length-`n` sequence, or
    `none` (IndexError). -/
def getIndex (n : Nat) (key : Int) : Option Int :=
  let i := if key < 0 then key + (n : Int) else key
  if 0 ≤ i ∧ i < (n : Int) then some i else none

-- executable model↔CPython binding (RHS = the index list(range(n))[key] reads).
#guard getIndex 3 0 = some 0
#guard getIndex 3 2 = some 2
#guard getIndex 3 (-1) = some 2      -- xs[-1] → last element
#guard getIndex 3 (-3) = some 0
#guard getIndex 3 3 = none           -- IndexError
#guard getIndex 3 (-4) = none        -- IndexError
#guard getIndex 0 0 = none

/-- Helper: the guarded-`some` shape of `getIndex` admits `i` exactly when `i`
    IS the (already-normalized) candidate `e` and `e` is in bounds. -/
private theorem ite_index_eq_some (e n i : Int) :
    (if 0 ≤ e ∧ e < n then some e else none) = some i ↔
      i = e ∧ 0 ≤ i ∧ i < n := by
  split
  · rename_i hb
    constructor
    · intro h
      injection h with h
      subst h
      exact ⟨rfl, hb⟩
    · intro h
      rw [h.1]
  · rename_i hb
    constructor
    · intro h
      cases h
    · intro h
      exact absurd (h.1 ▸ h.2) hb

/-- **Index-normalization safety (Tier-2 flagship #1).** Whenever `pyGetItem`
    admits an index it is a real in-bounds access `0 ≤ i < n` — never OOB. The
    #278/#279 negative-index-OOB class made impossible by proof. -/
theorem getIndex_inbounds (n : Nat) (key i : Int) (h : getIndex n key = some i) :
    0 ≤ i ∧ i < (n : Int) := by
  simp only [getIndex] at h
  exact ((ite_index_eq_some _ _ _).mp h).2

/-- info: 'PythExpandVerify.getIndex_inbounds' does not depend on any axioms -/
#guard_msgs in #print axioms getIndex_inbounds

/-- Characterization / uniqueness: the admitted index is exactly the normalized
    key, and admission is exactly in-boundedness (spec pins a single result). -/
theorem getIndex_spec (n : Nat) (key i : Int) :
    getIndex n key = some i ↔
      (i = if key < 0 then key + (n : Int) else key) ∧ 0 ≤ i ∧ i < (n : Int) := by
  simp only [getIndex]
  exact ite_index_eq_some _ _ _

/-- **`pySetItem` write-index safety (#278/#279 WRITE side).** The list-write
    index normalization (runtime.js:1810-1819) is byte-identical to the read
    path's `getIndex` (:830-835), confirmed differentially in
    `experiments/pbt-ps/tier2b_eq_validate.py` — so writes are in-bounds by the
    very same theorem. A proved alias, not a re-derivation. -/
theorem setIndex_inbounds (n : Nat) (key i : Int) (h : getIndex n key = some i) :
    0 ≤ i ∧ i < (n : Int) :=
  getIndex_inbounds n key i h

/-! ### PyBool — Python truthiness over the value representation (Tier 2, §9)

Lean model of the shipping `pyBool` (types.js:8-22). `PyValue` is the value-
representation relation (§10.3 spine) carrying just enough structure to state
truthiness. `pyBool_iff_not_falsy` binds the shipped helper to PYTHON's own
falsy rule — the JS quirks (`{}` truthy in JS but falsy in Python, #211; #272)
are corrected. Spec validated Stage-3 vs CPython `bool()` (DRY). -/

/-- A modeled Python value. `num 0` models numeric zero (int/float alike for
    truthiness). `dict`/`set` carry their CONTENTS (so nested dict/list VALUES
    can be compared structurally — C8): a dict is a list of `(key, value)`
    entries (Python keys are hashable ⇒ SCALARS in the modeled domain; values are
    arbitrary modeled values, possibly nested dicts/lists), a set is a list of
    (scalar, hashable) elements. `lenObj` stays a size-only ABSTRACT object: an
    arbitrary Python object exposing `__len__` — its `==` is identity, which the
    model cannot decide, so it is deliberately OUTSIDE the equality-faithful
    domain (only its truthiness, which depends on `len`, is modeled). -/
inductive PyValue where
  | none
  | bool (b : Bool)
  | num (n : Int)
  | str (s : String)
  | list (xs : List PyValue)
  | dict (entries : List (PyValue × PyValue))
  | set (elems : List PyValue)
  | lenObj (size : Nat)
deriving Repr

/-- Truthiness as the shipping `pyBool` computes it (types.js:8-22). -/
def pyBoolM : PyValue → Bool
  | .none => false
  | .bool b => b
  | .num n => n ≠ 0
  | .str s => 0 < s.length
  | .list xs => 0 < xs.length
  | .dict es => 0 < es.length
  | .set es => 0 < es.length
  | .lenObj k => 0 < k

/-- Python's OWN falsy rule (the language reference: None, False, numeric zero,
    and every empty sequence/mapping/set/`__len__`), stated over the abstract
    value — INDEPENDENT of the JS type-dispatch in `pyBoolM`. -/
def pyFalsy : PyValue → Bool
  | .none => true
  | .bool b => !b
  | .num n => decide (n = 0)
  | .str s => decide (s.length = 0)
  | .list xs => xs.isEmpty
  | .dict es => es.isEmpty
  | .set es => es.isEmpty
  | .lenObj k => decide (k = 0)

#guard pyBoolM (.dict []) = false    -- {} is FALSY in Python (#211), truthy in raw JS
#guard pyBoolM (.list []) = false
#guard pyBoolM (.num 0) = false
#guard pyBoolM (.str "") = false
#guard pyBoolM (.bool true) = true
#guard pyBoolM (.lenObj 0) = false

/-- Helper: `0 < k` and `¬(k = 0)` decide identically over `Nat` — the bridge
    between the shipping `> 0` checks and Python's `= empty` falsy rule. -/
private theorem decide_pos_eq_not_decide_eq_zero (k : Nat) :
    decide (0 < k) = !decide (k = 0) := by
  cases k <;> rfl

/-- Helper: axiom-free `decide_not` (the library lemma pulls `Classical.choice`
    through `simp`; case-splitting the instance keeps the axiom base minimal). -/
private theorem decide_not_eq_not_decide (p : Prop) [inst : Decidable p] :
    decide (¬p) = !decide p := by
  cases inst <;> rfl

/-- **Truthiness characterization (Tier-2 flagship #2).** The shipping `pyBool`
    is truthy on EXACTLY the non-falsy Python values — it implements Python's
    documented truthiness rule, quirks corrected. -/
theorem pyBool_iff_not_falsy (v : PyValue) : pyBoolM v = !(pyFalsy v) := by
  cases v with
  | none => rfl
  | bool b => cases b <;> rfl
  | num n => exact decide_not_eq_not_decide (n = 0)
  | str s => exact decide_pos_eq_not_decide_eq_zero s.length
  | list xs => cases xs <;> rfl
  | dict es => cases es <;> rfl
  | set es => cases es <;> rfl
  | lenObj k => exact decide_pos_eq_not_decide_eq_zero k

/- Axiom note: `Classical.choice`/`Quot.sound` enter through core's `String.length`
   (UTF-8 machinery) referenced by `pyBoolM`/`pyFalsy` themselves — even the `rfl`
   cases inherit them. Irreducible without changing the defs. -/
/--
info: 'PythExpandVerify.pyBool_iff_not_falsy' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in #print axioms pyBool_iff_not_falsy

/-! ### PyEq — scalar equality and the bool⊂int value representation (Tier 2, §9)

Lean model of the shipping `pyEq` SCALAR path (operators.js:273-283), the
fragment where the `bool ⊂ int` deviation lives (#241/#258 — `True == 1`).
Numeric cross-representation compares by value with `bool` as its int (0/1);
`none`/`str` compare directly. Container equality (list/dict/set, element-wise)
is future work. Spec validated Stage-3 vs CPython `==` on the scalar reprs
(`experiments/pbt-ps/tier2b_eq_validate.py`, DRY: faithful + reflexive + symmetric). -/

/-- Scalar Python equality as `pyEq` computes it, over the scalar fragment. -/
def pyEqScalar : PyValue → PyValue → Bool
  | .none, .none => true
  | .num a, .num b => a == b
  | .bool a, .bool b => a == b
  | .str a, .str b => a == b
  | .bool a, .num b => (if a then (1 : Int) else 0) == b   -- #258 bool ⊂ int
  | .num a, .bool b => a == (if b then (1 : Int) else 0)
  | _, _ => false

/-- The scalar fragment `pyEqScalar` is total-and-faithful over. -/
def PyValue.isScalar : PyValue → Bool
  | .none | .num _ | .bool _ | .str _ => true
  | _ => false

#guard pyEqScalar (.bool true) (.num 1) = true     -- True == 1  (#258)
#guard pyEqScalar (.bool false) (.num 0) = true    -- False == 0
#guard pyEqScalar (.bool true) (.num 2) = false    -- True == 2 is False
#guard pyEqScalar (.num 5) (.num 5) = true
#guard pyEqScalar (.str "a") (.str "a") = true
#guard pyEqScalar .none .none = true

/-- **bool⊂int identity (the deviation, #241/#258).** `True`/`False` equal
    their integer values 1/0 — the value-representation clause that was the bug. -/
theorem pyEq_bool_int (b : Bool) :
    pyEqScalar (.bool b) (.num (if b then 1 else 0)) = true := by
  cases b <;> rfl

/-- Reflexivity on the scalar fragment. -/
theorem pyEqScalar_refl (v : PyValue) (h : v.isScalar = true) :
    pyEqScalar v v = true := by
  cases v <;> simp_all [pyEqScalar, PyValue.isScalar]

/-- `==` is symmetric for any lawful `BEq` (Int/Bool/String here). -/
private theorem beq_comm' {α} [BEq α] [LawfulBEq α] (a b : α) :
    (a == b) = (b == a) := by
  by_cases h : a = b
  · subst h; rfl
  · rw [beq_eq_false_iff_ne.mpr h, beq_eq_false_iff_ne.mpr (fun e => h e.symm)]

/-- Symmetry (total — holds on all `PyValue`). -/
theorem pyEqScalar_symm (a b : PyValue) :
    pyEqScalar a b = pyEqScalar b a := by
  cases a <;> cases b <;> simp only [pyEqScalar] <;>
    first
      | rfl
      | exact beq_comm' ..

/--
info: 'PythExpandVerify.pyEqScalar_symm' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in #print axioms pyEqScalar_symm

/-! ### PyEq container equality — element-wise list equality (Tier 2, §9)

Extends `pyEqScalar` to element-wise, length-aware LIST equality
(operators.js:286-293) — recursively, so `bool ⊂ int` works INSIDE lists
(`[1, True] == [1, 1]`). Set/dict equality (order-independent, canonicalized
keys) is future work. Spec validated Stage-3 vs CPython `==` on nested lists
(`experiments/pbt-ps/tier2c_listeq_validate.py`, DRY: faithful + refl + symm). -/

mutual
/-- Full Python `==` over scalars, lists, and (recursively) nested dict/set
    VALUES (C8). Dict/set KEYS/ELEMENTS are hashable ⇒ scalars in the modeled
    domain; VALUES are arbitrary modeled values, compared RECURSIVELY — so
    `{1: {}} == {1: {}}` is TRUE (a dict-of-dicts equals itself), the C8 fix that
    the old size-only model got wrong. The dict arm is a **mutual LAST-WRITE
    subset** (C8, recursive form): `dSubM` skips entries shadowed by a later
    `pyEqV`-equal key and matches each surviving (last-write) entry against the
    OTHER side's last write for that key — i.e. both sides are compared as their
    CPython canonical (first-position/last-value) forms, at EVERY nesting level,
    without materializing the dedup (which would break the structural
    termination of the mutual recursion). So `{2:10, 2:20} == {2:20}` is TRUE as
    in CPython, even NESTED as a dict value. `lenObj` (an opaque object whose
    `==` is identity) stays outside the faithful domain (`_, _ => false`). -/
def pyEqV : PyValue → PyValue → Bool
  | .none, .none => true
  | .num a, .num b => a == b
  | .bool a, .bool b => a == b
  | .str a, .str b => a == b
  | .bool a, .num b => (if a then (1 : Int) else 0) == b
  | .num a, .bool b => a == (if b then (1 : Int) else 0)
  | .list a, .list b => pyEqL a b
  | .dict a, .dict b => dSubM a b && dSubM b a
  | .set a, .set b => sSubM a b && sSubM b a
  | _, _ => false
termination_by a b => sizeOf a + sizeOf b
decreasing_by all_goals (simp_wf <;> omega)
/-- Element-wise, length-aware list equality. -/
def pyEqL : List PyValue → List PyValue → Bool
  | [], [] => true
  | x :: xs, y :: ys => pyEqV x y && pyEqL xs ys
  | _, _ => false
termination_by a b => sizeOf a + sizeOf b
decreasing_by all_goals (simp_wf <;> omega)
/-- Does some entry of `l` have a key `pyEqV`-equal to `pk`? (The "shadow" scan:
    inside `dSubM`, an entry is IGNORED when a later entry rewrites its key —
    CPython last-write-wins; inside `dHasM` it detects the LAST match.) -/
def dKeyMem (pk : PyValue) : List (PyValue × PyValue) → Bool
  | [] => false
  | (qk, _) :: qs => pyEqV pk qk || dKeyMem pk qs
termination_by l => sizeOf pk + sizeOf l
decreasing_by all_goals (simp_wf <;> omega)
/-- Dict one-sided LAST-WRITE subset (C8, recursive): every entry of `a` that is
    the LAST write for its key class in `a` (no later `pyEqV`-equal key —
    `dKeyMem pk ps` false) must match `b`'s LAST write for that key (`dHasM`);
    shadowed entries are skipped, exactly as CPython canonicalization discards
    them. Keys by `pyEqV` (scalar keys), values RECURSIVELY by `pyEqV`. -/
def dSubM : List (PyValue × PyValue) → List (PyValue × PyValue) → Bool
  | [], _ => true
  | (pk, pv) :: ps, b => (dKeyMem pk ps || dHasM pk pv b) && dSubM ps b
termination_by a b => sizeOf a + sizeOf b
decreasing_by all_goals (simp_wf <;> omega)
/-- Does `b`'s LAST entry whose key `pyEqV`-matches `pk` carry a value
    `pyEqV`-equal to `pv`? (Last-write-wins: an entry only counts when no LATER
    entry of `b` rewrites the key — `dKeyMem pk qs` false.) -/
def dHasM (pk pv : PyValue) : List (PyValue × PyValue) → Bool
  | [] => false
  | (qk, qv) :: qs => (pyEqV pk qk && !dKeyMem pk qs && pyEqV pv qv) || dHasM pk pv qs
termination_by l => sizeOf pk + sizeOf pv + sizeOf l
decreasing_by all_goals (simp_wf <;> omega)
/-- Set one-sided mutual-subset: every element of `a` matches some element of `b`. -/
def sSubM : List PyValue → List PyValue → Bool
  | [], _ => true
  | x :: xs, b => sHasM x b && sSubM xs b
termination_by a b => sizeOf a + sizeOf b
decreasing_by all_goals (simp_wf <;> omega)
/-- Does element `x` match some element of `b`? -/
def sHasM (x : PyValue) : List PyValue → Bool
  | [] => false
  | y :: ys => pyEqV x y || sHasM x ys
termination_by l => sizeOf x + sizeOf l
decreasing_by all_goals (simp_wf <;> omega)
end

/-- WF-preprocessing bridge for `List.all` (absent from core 4.31's
    `Init.Data.List.Attach` set; modeled on `List.filter_wfParam`). Lets
    `eqModeled`'s nested `xs.all PyValue.eqModeled` recursion carry the
    `x ∈ xs` membership fact into its termination goal. -/
@[wf_preprocess] private theorem list_all_wfParam {α} {xs : List α} {f : α → Bool} :
    (wfParam xs).all f = xs.attach.unattach.all f := by
  simp [wfParam]

/-- Companion to `list_all_wfParam` (the `filter_unattach` analogue). -/
@[wf_preprocess] private theorem list_all_unattach {α} {P : α → Prop}
    {xs : List (Subtype P)} {f : α → Bool} :
    xs.unattach.all f = xs.all fun ⟨x, h⟩ =>
      binderNameHint x f <| binderNameHint h () <| f (wfParam x) := by
  simp [wfParam]

/-! The domain `pyEqV` is total-and-faithful over: scalars, (recursively) lists
thereof, DICTS (scalar keys, modeled values — possibly nested dicts/lists), and
SETS (scalar, hashable elements). `lenObj` is excluded (opaque object). The dict
arm uses the structural helper `dictModeled` (so the `p.2.eqModeled` recursion
under a projection is syntactically decreasing); `dictModeled_eq_all` below
re-expresses it as `.all` for the client proofs. -/
mutual
/-- Modeled-value domain predicate (see the section note above). -/
def PyValue.eqModeled : PyValue → Bool
  | .none | .num _ | .bool _ | .str _ => true
  | .list xs => xs.all PyValue.eqModeled
  | .dict es => PyValue.dictModeled es
  | .set es => es.all PyValue.isScalar
  | .lenObj _ => false
/-- Structural modeled-check for dict entries (scalar key, modeled value). -/
def PyValue.dictModeled : List (PyValue × PyValue) → Bool
  | [] => true
  | (k, v) :: rest => (k.isScalar && v.eqModeled) && PyValue.dictModeled rest
end

/-- `dictModeled` unfolds to the pointwise `.all` predicate. -/
theorem dictModeled_eq_all (es : List (PyValue × PyValue)) :
    PyValue.dictModeled es = es.all (fun p => p.1.isScalar && p.2.eqModeled) := by
  induction es with
  | nil => simp [PyValue.dictModeled]
  | cons p ps ih => obtain ⟨k, v⟩ := p; simp [PyValue.dictModeled, List.all_cons, ih]

/-- info: 'PythExpandVerify.dictModeled_eq_all' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms dictModeled_eq_all

#guard pyEqV (.list [.num 1, .bool true]) (.list [.num 1, .num 1]) = true  -- [1,True]==[1,1]
#guard pyEqV (.list [.num 1]) (.list [.num 1, .num 2]) = false             -- length-aware
#guard pyEqV (.list []) (.list []) = true
#guard pyEqV (.list [.num 1]) (.num 1) = false                             -- list vs scalar
#guard pyEqV (.list [.list [.num 1]]) (.list [.list [.num 1]]) = true      -- nested
-- C8: nested dict/set VALUES are compared STRUCTURALLY (the reviewer's case):
#guard pyEqV (.dict [(.num 1, .dict [])]) (.dict [(.num 1, .dict [])]) = true    -- {1:{}}=={1:{}}
#guard pyEqV (.dict []) (.dict []) = true                                         -- {}=={}
#guard pyEqV (.set [.num 1, .num 2]) (.set [.num 2, .num 1]) = true               -- {1,2}=={2,1}
#guard pyEqV (.dict [(.num 1, .list [.num 2])]) (.dict [(.num 1, .list [.num 2])]) = true
-- non-vacuity: DISTINCT nested dicts are NOT equal
#guard pyEqV (.dict [(.num 1, .dict [])]) (.dict [(.num 1, .dict [(.num 2, .num 3)])]) = false
#guard pyEqV (.dict [(.num 1, .dict [])]) (.dict [(.num 2, .dict [])]) = false    -- different key
#guard pyEqV (.dict [(.num 1, .list [.num 2])]) (.dict [(.num 1, .list [.num 3])]) = false
#guard pyEqV (.set [.num 1]) (.set [.num 1, .num 2]) = false
-- C8 (recursive): DUPLICATE KEYS are last-write-wins at EVERY level — the dict
-- arm compares canonical (first-position/last-value) forms, so a dup-key entry
-- list equals its dedup, INCLUDING as a nested VALUE (the iter-5 counterexample):
#guard pyEqV (.dict [(.num 2, .num 10), (.num 2, .num 20)])
             (.dict [(.num 2, .num 20)]) = true                      -- {2:10,2:20}=={2:20}
#guard pyEqV (.dict [(.num 1, .dict [(.num 2, .num 10), (.num 2, .num 20)])])
             (.dict [(.num 1, .dict [(.num 2, .num 20)])]) = true    -- {1:{2:10,2:20}}=={1:{2:20}}
-- non-vacuity: the FIRST write does NOT win, nested included
#guard pyEqV (.dict [(.num 2, .num 10), (.num 2, .num 20)])
             (.dict [(.num 2, .num 10)]) = false                     -- {2:10,2:20}≠{2:10}
#guard pyEqV (.dict [(.num 1, .dict [(.num 2, .num 10), (.num 2, .num 20)])])
             (.dict [(.num 1, .dict [(.num 2, .num 10)])]) = false

/-- A scalar is in the modeled equality domain. -/
private theorem isScalar_eqModeled {v : PyValue} (h : v.isScalar = true) :
    v.eqModeled = true := by
  cases v <;> simp_all [PyValue.isScalar, PyValue.eqModeled]

/-- The LAST value written to key `k` in an entry list (`none` if `k` absent) —
    CPython's "last write wins". Defined HERE (before the equality algebra)
    because the C8 last-write dict arm is characterized by it: `dHasM`/`dSubM`
    are proved below to decide agreement of `pyLastVal` lookups. The
    canonicalization section further down builds `dictDedup` on it. -/
def pyLastVal (k : PyValue) : List (PyValue × PyValue) → Option PyValue
  | [] => none
  | (k', v) :: rest =>
      match pyLastVal k rest with
      | some v' => some v'
      | none => if pyEqV k k' then some v else none

/-- A successful lookup exhibits a matching entry (`pyEqV`-equal key, that value). -/
private theorem pyLastVal_some (k : PyValue) :
    ∀ l v, pyLastVal k l = some v → ∃ p, p ∈ l ∧ pyEqV k p.1 = true ∧ p.2 = v
  | [], v, h => by simp [pyLastVal] at h
  | (k', v') :: rest, v, h => by
      rw [pyLastVal] at h
      cases hr : pyLastVal k rest with
      | some v'' =>
          simp only [hr, Option.some.injEq] at h    -- h : v'' = v
          obtain ⟨p, hp, hk, hv⟩ := pyLastVal_some k rest v'' hr
          exact ⟨p, List.mem_cons.mpr (Or.inr hp), hk, hv.trans h⟩
      | none =>
          cases hk : pyEqV k k' with
          | true =>
              simp only [hr] at h
              simp only [hk, if_true, Option.some.injEq] at h    -- h : v' = v
              exact ⟨(k', v'), List.mem_cons.mpr (Or.inl rfl), hk, h⟩
          | false =>
              simp only [hr] at h
              simp [hk] at h

/-- A looked-up value is a component of a list entry, so it is smaller than the
    list — the size bound the mutual trans workers feed to the termination
    checker for their `pyEqV_trans'` calls on lookup witnesses. -/
private theorem pyLastVal_sizeOf_lt (k : PyValue) (l : List (PyValue × PyValue))
    (v : PyValue) (h : pyLastVal k l = some v) : sizeOf v < sizeOf l := by
  obtain ⟨p, hp, -, hpv⟩ := pyLastVal_some k l v h
  have hs := List.sizeOf_lt_of_mem hp
  obtain ⟨p1, p2⟩ := p
  simp only [Prod.mk.sizeOf_spec] at hs
  subst hpv
  show sizeOf p2 < sizeOf l
  omega

/-- The shadow scan is exactly lookup-presence: no `pyEqV`-matching key ↔ the
    last-write lookup is absent. -/
private theorem dKeyMem_eq_false_iff (pk : PyValue) :
    ∀ l, dKeyMem pk l = false ↔ pyLastVal pk l = none
  | [] => by simp [dKeyMem, pyLastVal]
  | (qk, qv) :: qs => by
      simp only [dKeyMem, pyLastVal, Bool.or_eq_false_iff]
      cases hr : pyLastVal pk qs with
      | some w =>
          have : dKeyMem pk qs ≠ false := fun hf =>
            by rw [(dKeyMem_eq_false_iff pk qs).mp hf] at hr; cases hr
          simp [this]
      | none =>
          have hqs : dKeyMem pk qs = false := (dKeyMem_eq_false_iff pk qs).mpr hr
          cases hk : pyEqV pk qk with
          | true => simp
          | false => simp [hqs]

/-- **The C8 last-write characterization of `dHasM`**: the scan succeeds exactly
    when `b`'s last-write lookup for `pk` yields a value `pyEqV`-equal to `pv`. -/
private theorem dHasM_iff_lastVal (pk pv : PyValue) :
    ∀ b, dHasM pk pv b = true ↔ ∃ w, pyLastVal pk b = some w ∧ pyEqV pv w = true
  | [] => by simp [dHasM, pyLastVal]
  | (qk, qv) :: qs => by
      simp only [dHasM, Bool.or_eq_true, Bool.and_eq_true, Bool.not_eq_true']
      rw [dHasM_iff_lastVal pk pv qs]
      show _ ↔ ∃ w, pyLastVal pk ((qk, qv) :: qs) = some w ∧ _
      rw [pyLastVal]
      cases hr : pyLastVal pk qs with
      | some w0 =>
          have hkm : dKeyMem pk qs ≠ false := fun hf =>
            by rw [(dKeyMem_eq_false_iff pk qs).mp hf] at hr; cases hr
          simp only [Bool.not_eq_false] at hkm
          simp [hkm]
      | none =>
          have hkm : dKeyMem pk qs = false := (dKeyMem_eq_false_iff pk qs).mpr hr
          cases hk : pyEqV pk qk with
          | true => simp [hkm]
          | false => simp [hkm]

/-- On a SUFFIX of `a`, a successful last-write lookup is the global one (later
    entries win, and a suffix contains exactly the later entries). -/
private theorem pyLastVal_suffix_cons (k v : PyValue) (p : PyValue × PyValue)
    (ps : List (PyValue × PyValue)) (h : pyLastVal k ps = some v) :
    pyLastVal k (p :: ps) = some v := by
  obtain ⟨k', v'⟩ := p
  simp only [pyLastVal, h]

/-- **`dSubM` from denotational transport (C8).** If every successful last-write
    lookup on `a` transports to a `pyEqV`-equal lookup on `b`, and `a`'s keys
    self-equal, then `a` is a last-write subset of `b` — proved for every suffix
    `sub` of `a` (the suffix invariant: a suffix lookup that succeeds IS the
    global lookup). Standalone: all `pyEqV` facts arrive as hypotheses, so the
    mutual refl/trans workers can use it without recursion issues. -/
private theorem dSubM_of_denote_aux (a b : List (PyValue × PyValue))
    (hd : ∀ k v, pyLastVal k a = some v →
            ∃ u, pyLastVal k b = some u ∧ pyEqV v u = true)
    (hka : ∀ p, p ∈ a → pyEqV p.1 p.1 = true) :
    ∀ sub, (∀ p, p ∈ sub → p ∈ a) →
      (∀ k v, pyLastVal k sub = some v → pyLastVal k a = some v) →
      dSubM sub b = true
  | [], _, _ => by simp [dSubM]
  | (pk, pv) :: ps, hmem, hsuf => by
    simp only [dSubM, Bool.and_eq_true, Bool.or_eq_true]
    refine ⟨?_, dSubM_of_denote_aux a b hd hka ps
              (fun p hp => hmem p (List.mem_cons.mpr (Or.inr hp)))
              (fun k v hv => hsuf k v (pyLastVal_suffix_cons k v _ ps hv))⟩
    cases hsh : dKeyMem pk ps with
    | true => exact Or.inl rfl
    | false =>
      right
      have hnone : pyLastVal pk ps = none := (dKeyMem_eq_false_iff pk ps).mp hsh
      have hpk : pyEqV pk pk = true :=
        hka _ (hmem _ (List.mem_cons.mpr (Or.inl rfl)))
      have hloc : pyLastVal pk ((pk, pv) :: ps) = some pv := by
        simp [pyLastVal, hnone, hpk]
      obtain ⟨u, hu, hvu⟩ := hd pk pv (hsuf pk pv hloc)
      exact (dHasM_iff_lastVal pk pv b).mpr ⟨u, hu, hvu⟩

/-- If `x` occurs in `b` and self-equals, `sHasM` finds it. -/
private theorem sHasM_mem' (x : PyValue) (hx : pyEqV x x = true) :
    ∀ b, x ∈ b → sHasM x b = true
  | [], hmem => by simp at hmem
  | y :: ys, hmem => by
    rcases List.mem_cons.mp hmem with heq | htl
    · subst heq; simp [sHasM, hx]
    · simp only [sHasM, sHasM_mem' x hx ys htl, Bool.or_true]

/-- `sSubM sub es = true` whenever every element of `sub` occurs in `es` and
    every element of `es` self-equals (structural on `sub`). -/
private theorem sSubM_of_subset (es : List PyValue)
    (hself : ∀ x, x ∈ es → pyEqV x x = true) :
    ∀ sub, (∀ x, x ∈ sub → x ∈ es) → sSubM sub es = true
  | [], _ => by simp [sSubM]
  | x :: xs, hsub => by
    have hmem : x ∈ es := hsub _ (List.mem_cons.mpr (Or.inl rfl))
    simp only [sSubM, Bool.and_eq_true]
    exact ⟨sHasM_mem' x (hself x hmem) es hmem,
           sSubM_of_subset es hself xs (fun y hy => hsub y (List.mem_cons.mpr (Or.inr hy)))⟩

mutual
/-- Value-level reflexivity worker (mutual with the list/entry/element workers). -/
private theorem pyEqV_refl' (v : PyValue) (h : v.eqModeled = true) :
    pyEqV v v = true := by
  match v, h with
  | .none, _ => simp [pyEqV]
  | .bool b, _ => simp [pyEqV]
  | .num n, _ => simp [pyEqV]
  | .str s, _ => simp [pyEqV]
  | .list xs, h =>
    simp only [PyValue.eqModeled] at h
    simpa only [pyEqV] using pyEqL_refl' xs h
  | .dict es, h =>
    simp only [PyValue.eqModeled] at h
    have hself := pyEqEntriesRefl' es h
    have hs : dSubM es es = true :=
      dSubM_of_denote_aux es es
        (fun k v hv => ⟨v, hv, by
          obtain ⟨p, hp, -, hpv⟩ := pyLastVal_some k es v hv
          exact hpv ▸ (hself p hp).2⟩)
        (fun p hp => (hself p hp).1)
        es (fun _ hp => hp) (fun _ _ hv => hv)
    simp only [pyEqV, hs, Bool.and_self]
  | .set es, h =>
    simp only [PyValue.eqModeled] at h
    have hself := pyEqElemsRefl' es h
    have hs : sSubM es es = true := sSubM_of_subset es hself es (fun _ hx => hx)
    simp only [pyEqV, hs, Bool.and_self]
  | .lenObj k, h => simp [PyValue.eqModeled] at h
termination_by sizeOf v
decreasing_by all_goals (simp_wf <;> omega)

/-- List-level reflexivity worker (mutual with `pyEqV_refl'`). -/
private theorem pyEqL_refl' (xs : List PyValue) (h : xs.all PyValue.eqModeled = true) :
    pyEqL xs xs = true := by
  match xs, h with
  | [], _ => simp [pyEqL]
  | x :: rest, h =>
    simp only [List.all_cons, Bool.and_eq_true] at h
    simp only [pyEqL, Bool.and_eq_true]
    exact ⟨pyEqV_refl' x h.1, pyEqL_refl' rest h.2⟩
termination_by sizeOf xs
decreasing_by all_goals (simp_wf <;> omega)

/-- Every entry of a modeled dict self-equals (keys + values, values recursively). -/
private theorem pyEqEntriesRefl' :
    ∀ (es : List (PyValue × PyValue)), PyValue.dictModeled es = true →
      ∀ p, p ∈ es → pyEqV p.1 p.1 = true ∧ pyEqV p.2 p.2 = true
  | [], _, p, hp => by simp at hp
  | (pk, pv) :: rest, h, p, hp => by
    simp only [PyValue.dictModeled, Bool.and_eq_true] at h
    rcases List.mem_cons.mp hp with heq | htl
    · rw [heq]
      exact ⟨pyEqV_refl' pk (isScalar_eqModeled h.1.1), pyEqV_refl' pv h.1.2⟩
    · exact pyEqEntriesRefl' rest h.2 p htl
termination_by es => sizeOf es
decreasing_by all_goals (simp_wf <;> omega)

/-- Every element of a modeled set self-equals. -/
private theorem pyEqElemsRefl' :
    ∀ (es : List PyValue), es.all PyValue.isScalar = true → ∀ x, x ∈ es → pyEqV x x = true
  | [], _, x, hx => by simp at hx
  | y :: rest, h, x, hx => by
    simp only [List.all_cons, Bool.and_eq_true] at h
    rcases List.mem_cons.mp hx with heq | htl
    · rw [heq]; exact pyEqV_refl' y (isScalar_eqModeled h.1)
    · exact pyEqElemsRefl' rest h.2 x htl
termination_by es => sizeOf es
decreasing_by all_goals (simp_wf <;> omega)
end

/-- Reflexivity on the modeled domain (scalars, lists, dicts, sets thereof). -/
theorem pyEqV_refl (v : PyValue) (h : v.eqModeled = true) : pyEqV v v = true :=
  pyEqV_refl' v h

/-- info: 'PythExpandVerify.pyEqV_refl' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms pyEqV_refl

mutual
/-- Value-level symmetry worker (mutual with `pyEqL_symm'`). -/
private theorem pyEqV_symm' (a b : PyValue) : pyEqV a b = pyEqV b a := by
  cases a <;> cases b <;> simp only [pyEqV] <;>
    first
      | rfl
      | exact beq_comm' ..
      | exact pyEqL_symm' ..
      | exact Bool.and_comm ..    -- dict/dict and set/set: (X a b && X b a) = (X b a && X a b)
termination_by sizeOf a
decreasing_by all_goals (simp_wf <;> omega)

/-- List-level symmetry worker (mutual with `pyEqV_symm'`). -/
private theorem pyEqL_symm' (a b : List PyValue) : pyEqL a b = pyEqL b a := by
  match a, b with
  | [], [] => rfl
  | [], _ :: _ => simp [pyEqL]
  | _ :: _, [] => simp [pyEqL]
  | x :: xs, y :: ys =>
    simp only [pyEqL, pyEqV_symm' x y, pyEqL_symm' xs ys]
termination_by sizeOf a
decreasing_by all_goals (simp_wf <;> omega)
end

/-- Symmetry (total — holds on all `PyValue`). -/
theorem pyEqV_symm (a b : PyValue) : pyEqV a b = pyEqV b a :=
  pyEqV_symm' a b

/--
info: 'PythExpandVerify.pyEqV_symm' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in #print axioms pyEqV_symm

/-! ### PyEq set/dict equality — order-independent (Tier 2, §9; §15 SPOTs)

Extends `pyEq` to SET equality (operators.js:294-301) and DICT equality
(302-321), both ORDER-INDEPENDENT and bool⊂int-canonicalized (`{True}=={1}`,
`{True:x}=={1:x}`). Modeled as **mutual subset** under `pyEqV` — so equality is
**symmetric by construction** (no nodup/pigeonhole reasoning needed). Spec
validated Stage-3 vs CPython set/dict `==` (`experiments/pbt-ps/
tier2d_setdicteq_validate.py`, DRY: faithful + reflexive + symmetric).

This section is also the codebase's FIRST use of **SPOTs** (release_plan_v2 §15):
the `example`s below prove concrete client facts THROUGH the theorems (not by
evaluation), demonstrating the specs are two-sided/usable. -/

/-- `a ⊆ b` under Python equality: every element of `a` is `pyEqV` some element of `b`. -/
def pySubset (a b : List PyValue) : Bool := a.all (fun x => b.any (fun y => pyEqV x y))

/-- Set equality: mutual subset (order-independent; symmetric by construction). -/
def setEq (a b : List PyValue) : Bool := pySubset a b && pySubset b a

/-- Dict entry-subset: every `(k,v)` of `a` matches some entry of `b`. -/
def pyDSubset (a b : List (PyValue × PyValue)) : Bool :=
  a.all (fun p => b.any (fun q => pyEqV p.1 q.1 && pyEqV p.2 q.2))

/-- Dict equality: mutual entry-subset (key-order-independent). -/
def dictEq (a b : List (PyValue × PyValue)) : Bool := pyDSubset a b && pyDSubset b a

#guard setEq [.num 1, .num 2] [.num 2, .num 1] = true        -- {1,2}=={2,1} (order-indep)
#guard setEq [.bool true] [.num 1] = true                     -- {True}=={1} (bool⊂int)
#guard setEq [.num 1] [.num 1, .num 2] = false
#guard dictEq [(.num 1, .num 10), (.num 2, .num 20)]
              [(.num 2, .num 20), (.num 1, .num 10)] = true   -- key-order-indep
#guard dictEq [(.bool true, .num 10)] [(.num 1, .num 10)] = true  -- {True:10}=={1:10}

/-- **Set-equality symmetry (order-independence, by construction).** -/
theorem setEq_symm (a b : List PyValue) : setEq a b = setEq b a := by
  simp only [setEq]; exact Bool.and_comm _ _

/-- **Dict-equality symmetry (key-order-independence, by construction).** -/
theorem dictEq_symm (a b : List (PyValue × PyValue)) : dictEq a b = dictEq b a := by
  simp only [dictEq]; exact Bool.and_comm _ _

/-- Every modeled list is a `pySubset` of itself (each element is its own witness). -/
private theorem pySubset_self (a : List PyValue) (h : a.all PyValue.eqModeled = true) :
    pySubset a a = true := by
  simp only [pySubset, List.all_eq_true] at h ⊢
  intro x hx
  simp only [List.any_eq_true]
  exact ⟨x, hx, pyEqV_refl x (h x hx)⟩

/-- Every modeled entry list is a `pyDSubset` of itself. -/
private theorem pyDSubset_self (a : List (PyValue × PyValue))
    (h : a.all (fun p => p.1.eqModeled && p.2.eqModeled) = true) :
    pyDSubset a a = true := by
  simp only [pyDSubset, List.all_eq_true] at h ⊢
  intro p hp
  simp only [List.any_eq_true, Bool.and_eq_true]
  have hm := h p hp
  simp only [Bool.and_eq_true] at hm
  exact ⟨p, hp, pyEqV_refl _ hm.1, pyEqV_refl _ hm.2⟩

/-- Set reflexivity on the modeled fragment (scalars + lists thereof). -/
theorem setEq_refl (a : List PyValue) (h : a.all PyValue.eqModeled = true) :
    setEq a a = true := by
  simp only [setEq, pySubset_self a h, Bool.and_self]

/-- Dict reflexivity on the modeled fragment (keys + values). -/
theorem dictEq_refl (a : List (PyValue × PyValue))
    (h : a.all (fun p => p.1.eqModeled && p.2.eqModeled) = true) :
    dictEq a a = true := by
  simp only [dictEq, pyDSubset_self a h, Bool.and_self]

/-! #### Canonical entries — why raw `dictEq` needs a nodup-keys invariant

`dictEq` compares RAW entry lists by mutual entry-subset. That is faithful to
Python dict equality only when each side has no duplicate KEYS: `{1: 10, 1: 20}`
IS the dict `{1: 20}` (last write wins), but the raw entry lists
`[(1,10),(1,20)]` and `[(1,20)]` are NOT mutual-entry-subsets (the `(1,10)` entry
matches nothing), so raw `dictEq` returns `false` on two equal dicts. The
`refl`/`symm`/`trans` algebra above is a genuine equivalence on the RAW-entry
carrier, but it only MEANS Python dict equality on canonical (nodup-key) lists.
(`pyEqV`'s own dict ARM does not have this problem: it is last-write-wins by
construction — `dSubM` skips shadowed entries — which is what makes NESTED
dict values with duplicate keys faithful, C8 recursive.) Below: the canonical
invariant, a CPython-faithful (first-insertion position, last-write value)
canonicalizer `dictDedup` built on the `pyLastVal` lookup defined above, and
the proof it establishes the invariant. -/

/-- No two entries share a (Python-)equal key — the canonical-dict invariant. -/
def dictNoDupKeys : List (PyValue × PyValue) → Bool
  | [] => true
  | (k, _) :: rest => !rest.any (fun q => pyEqV k q.1) && dictNoDupKeys rest

/-- CPython dict-literal canonicalization: FIRST-insertion position, LAST-write
    value. The head key keeps its position and takes its last value in the whole
    list (`pyLastVal k rest`, or its own `v` if unique); all LATER occurrences of
    that key are removed from the tail. Structural recursion on the tail. -/
def dictDedup : List (PyValue × PyValue) → List (PyValue × PyValue)
  | [] => []
  | (k, v) :: rest =>
      (k, (pyLastVal k rest).getD v) :: (dictDedup rest).filter (fun q => !pyEqV k q.1)

/-- Faithful dict equality on the modeled value fragment: canonicalize both
    sides (CPython first-position/last-value), then compare. -/
def dictEqCanon (a b : List (PyValue × PyValue)) : Bool :=
  dictEq (dictDedup a) (dictDedup b)

-- Raw `dictEq` is UNFAITHFUL on non-canonical input; `dictEqCanon` fixes it,
-- with CPython order (first position) AND value (last write):
#guard dictEq [(.num 1, .num 10), (.num 1, .num 20)] [(.num 1, .num 20)] = false
#guard dictEqCanon [(.num 1, .num 10), (.num 1, .num 20)] [(.num 1, .num 20)] = true
-- C8: dictEqCanon on NESTED dict/list values (the reviewer's counterexample):
#guard dictEqCanon [(.num 1, .dict [])] [(.num 1, .dict [])] = true            -- {1:{}}=={1:{}}
#guard dictEqCanon [(.num 1, .dict [])] [(.num 1, .dict [(.num 2, .num 3)])] = false
#guard dictEqCanon [(.num 1, .list [.num 2])] [(.num 1, .list [.num 2])] = true
-- {1:10, 2:20, 1:30} canonicalizes to [(1,30),(2,20)] — key 1 keeps its FIRST
-- position but its LAST value 30 (the CPython semantics codex flagged). Pattern
-- match (PyValue has no DecidableEq, so no raw list `=`):
#guard (match dictDedup [(.num 1, .num 10), (.num 2, .num 20), (.num 1, .num 30)] with
        | [(.num 1, .num 30), (.num 2, .num 20)] => true
        | _ => false)
#guard dictNoDupKeys [(.num 1, .num 10), (.num 1, .num 20)] = false
#guard dictNoDupKeys [(.num 1, .num 20)] = true

/-- Filtering by `!P` leaves nothing satisfying `P`. -/
private theorem filter_not_any {α} (P : α → Bool) :
    ∀ l : List α, (l.filter (fun x => !P x)).any P = false
  | [] => rfl
  | x :: xs => by
    rw [List.filter_cons]
    cases hP : P x with
    | true =>
      rw [if_neg (by simp)]
      exact filter_not_any P xs
    | false =>
      rw [if_pos (by simp), List.any_cons, hP, filter_not_any P xs, Bool.or_self]

/-- A witness surviving `filter p` also satisfies it in the original list. -/
private theorem any_filter_imp {α} (P p : α → Bool) (l : List α)
    (h : (l.filter p).any P = true) : l.any P = true := by
  rw [List.any_eq_true] at h ⊢
  obtain ⟨x, hx, hPx⟩ := h
  exact ⟨x, (List.mem_filter.mp hx).1, hPx⟩

/-- Filtering preserves the nodup-keys invariant. -/
private theorem dictNoDupKeys_filter (p : (PyValue × PyValue) → Bool) :
    ∀ l, dictNoDupKeys l = true → dictNoDupKeys (l.filter p) = true
  | [], _ => rfl
  | (k, v) :: rest, h => by
    simp only [dictNoDupKeys, Bool.and_eq_true] at h
    simp only [List.filter]
    cases hp : p (k, v) with
    | false => simpa [hp] using dictNoDupKeys_filter p rest h.2
    | true =>
      simp only [dictNoDupKeys, Bool.and_eq_true]
      refine ⟨?_, dictNoDupKeys_filter p rest h.2⟩
      have h1 : rest.any (fun q => pyEqV k q.1) = false := by
        cases hh : rest.any (fun q => pyEqV k q.1) with
        | false => rfl
        | true => rw [hh] at h; exact absurd h.1 (by simp)
      cases hh : (rest.filter p).any (fun q => pyEqV k q.1) with
      | false => rfl
      | true => rw [any_filter_imp _ p rest hh] at h1; exact absurd h1 (by simp)

/-- **Canonicalization establishes the invariant.** `dictDedup` always yields a
    nodup-key (canonical) entry list — so `dictEqCanon` compares two canonical
    lists, on which mutual entry-subset IS Python dict equality (over the modeled
    value fragment). -/
theorem dictDedup_nodup (a : List (PyValue × PyValue)) :
    dictNoDupKeys (dictDedup a) = true := by
  induction a with
  | nil => rfl
  | cons hd rest ih =>
    obtain ⟨k, v⟩ := hd
    simp only [dictDedup, dictNoDupKeys, Bool.and_eq_true]
    refine ⟨?_, dictNoDupKeys_filter _ (dictDedup rest) ih⟩
    simp only [filter_not_any, Bool.not_false]

/-- **`dictEqCanon` is symmetric** (order-independence, inherited from
    `dictEq_symm` under canonicalization). -/
theorem dictEqCanon_symm (a b : List (PyValue × PyValue)) :
    dictEqCanon a b = dictEqCanon b a := by
  simp only [dictEqCanon]; exact dictEq_symm _ _

/-- **General nested-value reflexivity.** For a dict whose keys are scalars and
    whose values are modeled (INCLUDING nested dicts/lists), `pyEqV (.dict a)
    (.dict a) = true` — the equality is a real reflexive relation over nested
    values, via the `dictModeled` domain and `pyEqV_refl`. -/
theorem dictEqV_nested_refl (a : List (PyValue × PyValue))
    (h : PyValue.dictModeled a = true) : pyEqV (.dict a) (.dict a) = true :=
  pyEqV_refl (.dict a) (by simpa [PyValue.eqModeled] using h)

/-- info: 'PythExpandVerify.dictEqV_nested_refl' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms dictEqV_nested_refl

/-- **Nested-dict/list VALUE faithfulness — concrete SPOTs (C8).** These three
    concrete `pyEqV` facts demonstrate the nested-value fix, but they are NOT the
    fidelity statement: the GENERAL universal theorem is `dictEqCanon_faithful`
    (below, `dictEqCanon` decides the independent last-write-wins dict-equality
    `dictDenoteEq`). Kept here as SPOTs beneath it: `pyEqV` compares nested
    dict/list VALUES structurally, so a dict-of-dicts equals itself as in CPython
    (`{1: {}} == {1: {}}` is TRUE — `false` in the old size-only model);
    NON-VACUOUS, since two dicts differing only in a nested value are NOT equal. -/
theorem dictEqCanon_nested_values_spot :
    pyEqV (.dict [(.num 1, .dict [])]) (.dict [(.num 1, .dict [])]) = true
      ∧ pyEqV (.dict [(.num 1, .dict [])]) (.dict [(.num 1, .dict [(.num 2, .num 3)])]) = false
      ∧ pyEqV (.dict [(.num 1, .list [.num 2])]) (.dict [(.num 1, .list [.num 3])]) = false := by
  refine ⟨dictEqV_nested_refl _ (by simp [PyValue.dictModeled, PyValue.isScalar, PyValue.eqModeled]),
          ?_, ?_⟩
  · simp [pyEqV, dSubM, dHasM, dKeyMem]
  · simp [pyEqV, dSubM, dHasM, dKeyMem, pyEqL]

/-- **`dictDedup` keeps the FIRST-occurrence position (with the LAST-written
    value).** The head entry of the canonical form is the head key `k` (its
    FIRST insertion position — CPython semantics, contrary to a last-position
    canonicalizer) carrying its last written value `(pyLastVal k rest).getD v`.
    So the docstring on `dictDedup` is TRUE, and #363/CPython order is honored. -/
theorem dictDedup_keeps_first (k v : PyValue) (rest : List (PyValue × PyValue)) :
    (dictDedup ((k, v) :: rest)).head? = some (k, (pyLastVal k rest).getD v) := by
  simp [dictDedup]

/-- info: 'PythExpandVerify.dictDedup_nodup' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms dictDedup_nodup

/-- info: 'PythExpandVerify.dictEqCanon_symm' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms dictEqCanon_symm

/-- info: 'PythExpandVerify.dictEqCanon_nested_values_spot' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms dictEqCanon_nested_values_spot

/-- info: 'PythExpandVerify.dictDedup_keeps_first' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms dictDedup_keeps_first

-- SPOTs (§15) — concrete client facts proven THROUGH the theorems, not by
-- evaluation. A too-weak/vacuous spec would fail these even though the `#guard`s
-- above (which merely evaluate the model) pass.
/-- SPOT: order-independence is client-usable — comparing `{1,2}` to `{2,1}` in
    either direction yields the same verdict, proven via `setEq_symm`. -/
example : setEq [.num 1, .num 2] [.num 2, .num 1]
        = setEq [.num 2, .num 1] [.num 1, .num 2] := setEq_symm _ _
/-- SPOT: dict key-order-independence, proven via `dictEq_symm`. -/
example : dictEq [(.num 1, .num 10), (.num 2, .num 20)]
                 [(.num 2, .num 20), (.num 1, .num 10)]
        = dictEq [(.num 2, .num 20), (.num 1, .num 10)]
                 [(.num 1, .num 10), (.num 2, .num 20)] := dictEq_symm _ _
/-- SPOT: reflexivity is client-usable on a concrete set, proven via `setEq_refl`. -/
example : setEq [.num 1, .num 2] [.num 1, .num 2] = true :=
  setEq_refl _ (by simp [PyValue.eqModeled])

/--
info: 'PythExpandVerify.setEq_refl' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in #print axioms setEq_refl

/-! ### PyEq is an equivalence relation — transitivity (Tier 2, full equality)

The final piece of the equality algebra: `pyEq` (and set/dict equality) is
TRANSITIVE, so with `refl` and `symm` it is an equivalence relation on the
modeled fragment — including the bool⊂int chains (`True==1` ∧ `1==1.0` ⟹
`True==1.0`). The mutual-subset model of `setEq`/`dictEq` makes their
transitivity follow from `pyEqV_trans` without any nodup/pigeonhole argument.
Spec validated Stage-3 vs CPython `==` (`experiments/pbt-ps/
tier2e_eqtrans_validate.py`, DRY: 595 pyEq triples + set/dict transitivity). -/

/-- The bool⊂int encoding `if · then 1 else 0` is injective: equal `Int`
    encodings force equal `Bool`s (the `True==1 ∧ 1==True'` ⟹ `True==True'`
    cross-case closer). -/
private theorem boolIntEnc_inj (x z : Bool)
    (h : (if x then (1 : Int) else 0) = (if z then (1 : Int) else 0)) : x = z := by
  cases x <;> cases z <;> simp_all

-- Characterizations / monotonicity helpers (standalone, no `pyEqV_trans`; the
-- `pyEqV` facts they need arrive as hypotheses, discharged by mutual calls at
-- the trans use sites).

/-- Shadow-scan monotonicity under a pointwise key-implication hypothesis:
    if every `k`-match in `l` is also a `j`-match, then `j`-absence forces
    `k`-absence. (The hypothesis is discharged with `pyEqV_trans'` at the
    call site inside the mutual trans workers.) -/
private theorem dKeyMem_mono (j k : PyValue) :
    ∀ l, (∀ q, q ∈ l → pyEqV k q.1 = true → pyEqV j q.1 = true) →
      dKeyMem j l = false → dKeyMem k l = false
  | [], _, _ => by simp [dKeyMem]
  | (qk, qv) :: qs, himp, hj => by
      simp only [dKeyMem, Bool.or_eq_false_iff] at hj ⊢
      refine ⟨?_, dKeyMem_mono j k qs
        (fun q hq => himp q (List.mem_cons.mpr (Or.inr hq))) hj.2⟩
      cases hk : pyEqV k qk with
      | false => rfl
      | true =>
          rw [himp (qk, qv) (List.mem_cons.mpr (Or.inl rfl)) hk] at hj
          exact absurd hj.1 (by simp)

/-- Lookups agree at probe keys whose matching behaviour agrees pointwise on
    the list. (Again: the pointwise hypothesis is discharged with the mutual
    `pyEqV_trans'`/`pyEqV_symm` at the call site.) -/
private theorem pyLastVal_congr_hyp (j k : PyValue) :
    ∀ l, (∀ q, q ∈ l → pyEqV j q.1 = pyEqV k q.1) →
      pyLastVal j l = pyLastVal k l
  | [], _ => rfl
  | (qk, qv) :: qs, hkey => by
      have ih := pyLastVal_congr_hyp j k qs
        (fun q hq => hkey q (List.mem_cons.mpr (Or.inr hq)))
      simp only [pyLastVal, ih, hkey (qk, qv) (List.mem_cons.mpr (Or.inl rfl))]

private theorem sHasM_iff (x : PyValue) (b : List PyValue) :
    sHasM x b = true ↔ ∃ y, y ∈ b ∧ pyEqV x y = true := by
  induction b with
  | nil => simp [sHasM]
  | cons y ys ih =>
    simp only [sHasM, Bool.or_eq_true, ih, List.mem_cons]
    constructor
    · rintro (h | ⟨z, hz, h⟩)
      · exact ⟨y, Or.inl rfl, h⟩
      · exact ⟨z, Or.inr hz, h⟩
    · rintro ⟨z, (rfl | hz), h⟩
      · exact Or.inl h
      · exact Or.inr ⟨z, hz, h⟩

private theorem sSubM_iff (a b : List PyValue) :
    sSubM a b = true ↔ ∀ x, x ∈ a → ∃ y, y ∈ b ∧ pyEqV x y = true := by
  induction a with
  | nil => simp [sSubM]
  | cons x xs ih =>
    simp only [sSubM, Bool.and_eq_true, sHasM_iff, ih, List.mem_cons]
    constructor
    · rintro ⟨hhead, htail⟩ p (rfl | hp)
      · exact hhead
      · exact htail p hp
    · intro h
      exact ⟨h x (Or.inl rfl), fun p hp => h p (Or.inr hp)⟩

mutual
/-- Value-level transitivity worker (mutual with `pyEqL_trans'`/`dSubM_trans'`/
    `sSubM_trans'`). Now covers nested dict/set VALUES (C8). -/
private theorem pyEqV_trans' (a b c : PyValue)
    (hab : pyEqV a b = true) (hbc : pyEqV b c = true) : pyEqV a c = true := by
  match a, b, c, hab, hbc with
  | .none, y, z, hab, hbc =>
    cases y <;> cases z <;> simp_all [pyEqV]
  | .str s, y, z, hab, hbc =>
    cases y <;> cases z <;> simp_all [pyEqV]
  | .num x, y, z, hab, hbc =>
    cases y <;> cases z <;>
      simp only [pyEqV, beq_iff_eq, Bool.false_eq_true] at hab hbc ⊢ <;>
      first
        | exact hab.trans hbc          -- shared middle representation
        | (subst hbc; exact hab)       -- bool middle, bool right end
  | .bool x, y, z, hab, hbc =>
    cases y <;> cases z <;>
      simp only [pyEqV, beq_iff_eq, Bool.false_eq_true] at hab hbc ⊢ <;>
      first
        | exact hab.trans hbc                        -- shared middle representation
        | exact boolIntEnc_inj _ _ (hab.trans hbc)   -- bool / num / bool
        | (subst hab; exact hbc)                     -- bool middle, bool left end
  | .list xs, .list ys, .list zs, hab, hbc =>
    simp only [pyEqV] at hab hbc ⊢
    exact pyEqL_trans' xs ys zs hab hbc             -- list / list / list
  | .list _, .list _, .none, _, hbc | .list _, .list _, .bool _, _, hbc
  | .list _, .list _, .num _, _, hbc | .list _, .list _, .str _, _, hbc
  | .list _, .list _, .dict _, _, hbc | .list _, .list _, .set _, _, hbc
  | .list _, .list _, .lenObj _, _, hbc => simp [pyEqV] at hbc
  | .list _, .none, _, hab, _ | .list _, .bool _, _, hab, _
  | .list _, .num _, _, hab, _ | .list _, .str _, _, hab, _
  | .list _, .dict _, _, hab, _ | .list _, .set _, _, hab, _
  | .list _, .lenObj _, _, hab, _ => simp [pyEqV] at hab
  | .dict la, .dict lb, .dict lc, hab, hbc =>
    simp only [pyEqV, Bool.and_eq_true] at hab hbc ⊢
    exact ⟨dSubM_trans' la lb lc hab.1 hbc.1, dSubM_trans' lc lb la hbc.2 hab.2⟩
  | .dict _, .dict _, .none, _, hbc | .dict _, .dict _, .bool _, _, hbc
  | .dict _, .dict _, .num _, _, hbc | .dict _, .dict _, .str _, _, hbc
  | .dict _, .dict _, .list _, _, hbc | .dict _, .dict _, .set _, _, hbc
  | .dict _, .dict _, .lenObj _, _, hbc => simp [pyEqV] at hbc
  | .dict _, .none, _, hab, _ | .dict _, .bool _, _, hab, _
  | .dict _, .num _, _, hab, _ | .dict _, .str _, _, hab, _
  | .dict _, .list _, _, hab, _ | .dict _, .set _, _, hab, _
  | .dict _, .lenObj _, _, hab, _ => simp [pyEqV] at hab
  | .set la, .set lb, .set lc, hab, hbc =>
    simp only [pyEqV, Bool.and_eq_true] at hab hbc ⊢
    exact ⟨sSubM_trans' la lb lc hab.1 hbc.1, sSubM_trans' lc lb la hbc.2 hab.2⟩
  | .set _, .set _, .none, _, hbc | .set _, .set _, .bool _, _, hbc
  | .set _, .set _, .num _, _, hbc | .set _, .set _, .str _, _, hbc
  | .set _, .set _, .list _, _, hbc | .set _, .set _, .dict _, _, hbc
  | .set _, .set _, .lenObj _, _, hbc => simp [pyEqV] at hbc
  | .set _, .none, _, hab, _ | .set _, .bool _, _, hab, _
  | .set _, .num _, _, hab, _ | .set _, .str _, _, hab, _
  | .set _, .list _, _, hab, _ | .set _, .dict _, _, hab, _
  | .set _, .lenObj _, _, hab, _ => simp [pyEqV] at hab
  | .lenObj n, y, _, hab, _ => cases y <;> simp [pyEqV] at hab
termination_by sizeOf a + sizeOf b + sizeOf c
decreasing_by all_goals (simp_wf <;> omega)

/-- List-level transitivity worker (mutual with `pyEqV_trans'`). -/
private theorem pyEqL_trans' (a b c : List PyValue)
    (hab : pyEqL a b = true) (hbc : pyEqL b c = true) : pyEqL a c = true := by
  match a, b, c, hab, hbc with
  | [], [], [], _, _ => simp [pyEqL]
  | [], [], _ :: _, _, hbc => simp [pyEqL] at hbc
  | [], _ :: _, _, hab, _ => simp [pyEqL] at hab
  | _ :: _, [], _, hab, _ => simp [pyEqL] at hab
  | _ :: _, _ :: _, [], _, hbc => simp [pyEqL] at hbc
  | x :: xs, y :: ys, z :: zs, hab, hbc =>
    simp only [pyEqL, Bool.and_eq_true] at hab hbc ⊢
    exact ⟨pyEqV_trans' x y z hab.1 hbc.1, pyEqL_trans' xs ys zs hab.2 hbc.2⟩
termination_by sizeOf a + sizeOf b + sizeOf c
decreasing_by all_goals (simp_wf <;> omega)

/-- **Lookup transport (the C8 trans worker).** A successful last-write lookup
    on `b` transports along `dSubM b c` to a `pyEqV`-equal lookup on `c`: the
    last `pk`-matching entry of `b` is unshadowed for its OWN key too (keys
    chain via the mutual `pyEqV_trans'`), so `dSubM b c` guarantees it
    `dHasM`-matches `c`'s last write, and the probe transfers back to `pk` by
    `pyLastVal_congr_hyp` (keys chained again). Structural on `b`. -/
private theorem dLookup_trans' (pk w : PyValue) (b c : List (PyValue × PyValue))
    (h2 : dSubM b c = true) (hw : pyLastVal pk b = some w) :
    ∃ u, pyLastVal pk c = some u ∧ pyEqV w u = true := by
  match b, h2, hw with
  | [], _, hw => simp [pyLastVal] at hw
  | (qk, qv) :: qs, h2, hw =>
    simp only [dSubM, Bool.and_eq_true, Bool.or_eq_true] at h2
    obtain ⟨hhead, htail⟩ := h2
    rw [pyLastVal] at hw
    cases hqs : pyLastVal pk qs with
    | some w' =>
      rw [hqs] at hw
      simp only [Option.some.injEq] at hw    -- hw : w' = w
      exact hw ▸ dLookup_trans' pk w' qs c htail hqs
    | none =>
      rw [hqs] at hw
      cases hk : pyEqV pk qk with
      | false => rw [hk] at hw; simp at hw
      | true =>
        rw [hk] at hw
        simp only [if_true, Option.some.injEq] at hw    -- hw : qv = w
        have hkmpk : dKeyMem pk qs = false := (dKeyMem_eq_false_iff pk qs).mpr hqs
        have hkmqk : dKeyMem qk qs = false := by
          refine dKeyMem_mono pk qk qs (fun q hq hqk => ?_) hkmpk
          obtain ⟨q1, q2⟩ := q
          have hszq : sizeOf q1 < sizeOf qs := by
            have h := List.sizeOf_lt_of_mem hq
            simp only [Prod.mk.sizeOf_spec] at h
            omega
          exact pyEqV_trans' pk qk q1 hk hqk
        rcases hhead with hsh | hhas
        · rw [hkmqk] at hsh; exact absurd hsh (by simp)
        · rw [dHasM_iff_lastVal] at hhas
          obtain ⟨u, hu, hqvu⟩ := hhas
          have hcongr : pyLastVal pk c = pyLastVal qk c := by
            refine pyLastVal_congr_hyp pk qk c (fun q hq => ?_)
            obtain ⟨q1, q2⟩ := q
            have hszq : sizeOf q1 < sizeOf c := by
              have h := List.sizeOf_lt_of_mem hq
              simp only [Prod.mk.sizeOf_spec] at h
              omega
            show pyEqV pk q1 = pyEqV qk q1
            cases h1 : pyEqV pk q1 with
            | true => rw [pyEqV_trans' qk pk q1 (pyEqV_symm pk qk ▸ hk) h1]
            | false =>
              cases h2' : pyEqV qk q1 with
              | false => rfl
              | true =>
                rw [pyEqV_trans' pk qk q1 hk h2'] at h1
                exact absurd h1 (by simp)
          exact ⟨u, hcongr.trans hu, hw ▸ hqvu⟩
termination_by sizeOf pk + sizeOf b + sizeOf c
decreasing_by all_goals (simp_wf <;> omega)

/-- Dict LAST-WRITE subset transitivity (structural on `a`): an unshadowed
    entry of `a` last-matches into `b` (`dHasM_iff_lastVal`), the lookup
    transports along `dSubM b c` (`dLookup_trans'`), and the values chain via
    `pyEqV_trans'` (the witnesses' sizes bounded by `pyLastVal_sizeOf_lt`). -/
private theorem dSubM_trans' (a b c : List (PyValue × PyValue))
    (h1 : dSubM a b = true) (h2 : dSubM b c = true) : dSubM a c = true := by
  match a, h1 with
  | [], _ => simp [dSubM]
  | (pk, pv) :: ps, h1 =>
    simp only [dSubM, Bool.and_eq_true, Bool.or_eq_true] at h1 ⊢
    obtain ⟨hhead, htail⟩ := h1
    refine ⟨?_, dSubM_trans' ps b c htail h2⟩
    rcases hhead with hsh | hhas
    · exact Or.inl hsh
    · refine Or.inr ?_
      rw [dHasM_iff_lastVal] at hhas
      obtain ⟨w, hw, hvw⟩ := hhas
      obtain ⟨u, hu, hwu⟩ := dLookup_trans' pk w b c h2 hw
      have hszw : sizeOf w < sizeOf b := pyLastVal_sizeOf_lt pk b w hw
      have hszu : sizeOf u < sizeOf c := pyLastVal_sizeOf_lt pk c u hu
      exact (dHasM_iff_lastVal pk pv c).mpr ⟨u, hu, pyEqV_trans' pv w u hvw hwu⟩
termination_by sizeOf a + sizeOf b + sizeOf c
decreasing_by all_goals (simp_wf <;> omega)

/-- Set subset transitivity (structural on `a`; elements chained via `pyEqV_trans'`). -/
private theorem sSubM_trans' (a b c : List PyValue)
    (h1 : sSubM a b = true) (h2 : sSubM b c = true) : sSubM a c = true := by
  match a, h1 with
  | [], _ => simp [sSubM]
  | x :: xs, h1 =>
    simp only [sSubM, Bool.and_eq_true] at h1
    obtain ⟨hhead, htail⟩ := h1
    simp only [sSubM, Bool.and_eq_true]
    refine ⟨?_, sSubM_trans' xs b c htail h2⟩
    rw [sHasM_iff] at hhead
    obtain ⟨y, hy, hxy⟩ := hhead
    rw [sSubM_iff] at h2
    obtain ⟨z, hz, hyz⟩ := h2 y hy
    rw [sHasM_iff]
    have hszy : sizeOf y < sizeOf b := List.sizeOf_lt_of_mem hy
    have hszz : sizeOf z < sizeOf c := List.sizeOf_lt_of_mem hz
    exact ⟨z, hz, pyEqV_trans' x y z hxy hyz⟩
termination_by sizeOf a + sizeOf b + sizeOf c
decreasing_by all_goals (simp_wf <;> omega)
end

/-- **Transitivity of `pyEqV`** over the FULL modeled domain — scalars, lists,
    and (recursively, C8) nested dict/set VALUES. With `pyEqV_refl` and
    `pyEqV_symm`, `pyEqV` is an equivalence relation on the modeled domain,
    including nested dict-of-dict values. -/
theorem pyEqV_trans (a b c : PyValue)
    (hab : pyEqV a b = true) (hbc : pyEqV b c = true) : pyEqV a c = true :=
  pyEqV_trans' a b c hab hbc

/-- Subset-transitivity: chain each element's witness through `pyEqV_trans`. -/
private theorem pySubset_trans (a b c : List PyValue)
    (h1 : pySubset a b = true) (h2 : pySubset b c = true) : pySubset a c = true := by
  simp only [pySubset, List.all_eq_true, List.any_eq_true] at h1 h2 ⊢
  intro x hx
  obtain ⟨y, hy, hxy⟩ := h1 x hx
  obtain ⟨z, hz, hyz⟩ := h2 y hy
  exact ⟨z, hz, pyEqV_trans x y z hxy hyz⟩

/-- **Set-equality transitivity** (follows from `pyEqV_trans`, no nodup needed). -/
theorem setEq_trans (a b c : List PyValue)
    (hab : setEq a b = true) (hbc : setEq b c = true) : setEq a c = true := by
  simp only [setEq, Bool.and_eq_true] at hab hbc ⊢
  exact ⟨pySubset_trans a b c hab.1 hbc.1, pySubset_trans c b a hbc.2 hab.2⟩

/-- Dict entry-subset transitivity: chain key- and value-witnesses through
    `pyEqV_trans`. -/
private theorem pyDSubset_trans (a b c : List (PyValue × PyValue))
    (h1 : pyDSubset a b = true) (h2 : pyDSubset b c = true) :
    pyDSubset a c = true := by
  simp only [pyDSubset, List.all_eq_true, List.any_eq_true, Bool.and_eq_true]
    at h1 h2 ⊢
  intro p hp
  obtain ⟨q, hq, hk1, hv1⟩ := h1 p hp
  obtain ⟨r, hr, hk2, hv2⟩ := h2 q hq
  exact ⟨r, hr, pyEqV_trans _ _ _ hk1 hk2, pyEqV_trans _ _ _ hv1 hv2⟩

/-- **Dict-equality transitivity.** -/
theorem dictEq_trans (a b c : List (PyValue × PyValue))
    (hab : dictEq a b = true) (hbc : dictEq b c = true) : dictEq a c = true := by
  simp only [dictEq, Bool.and_eq_true] at hab hbc ⊢
  exact ⟨pyDSubset_trans a b c hab.1 hbc.1, pyDSubset_trans c b a hbc.2 hab.2⟩

/-- SPOT: transitivity is client-usable — a concrete 3-set chain closes via
    `setEq_trans` (not by evaluation). -/
example (h1 : setEq [.num 1, .num 2] [.num 2, .num 1] = true)
        (h2 : setEq [.num 2, .num 1] [.num 1, .num 2] = true) :
    setEq [.num 1, .num 2] [.num 1, .num 2] = true := setEq_trans _ _ _ h1 h2

/--
info: 'PythExpandVerify.pyEqV_trans' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in #print axioms pyEqV_trans

/-! #### C8 — a GENERAL `dictEqCanon` fidelity theorem (universal, not three evals)

The three concrete `pyEqV` facts in `dictEqCanon_nested_values_spot` demonstrate the
nested-value fix but are NOT a general statement. The universal one, below, is
that **`dictEqCanon` DECIDES an INDEPENDENT Python-dict-equality semantics**: two
entry lists are Python-equal iff, at EVERY probe key, their last-write-wins
lookups agree. That semantics (`dictDenoteEq`) is built purely from `pyLastVal`
(CPython "last write wins") and value-equality `pyEqV` — it mentions NEITHER
`dictEq` NOR `dSubM`, so the theorem is a genuine correctness statement for the
decision procedure, not a model-vs-model tautology.

`dictEqCanon_faithful` holds for ALL modeled entry lists, INCLUDING top-level
DUPLICATE keys (the reviewer's `[(2,10),(2,20)]` vs `[(2,20)]`), because
`dictEqCanon` canonicalizes first and canonicalization is proved
DENOTATION-PRESERVING (`dictDedup_denote`: `pyLastVal k (dictDedup a) =
pyLastVal k a` for every key — the canonical form denotes the SAME mapping).

Faithfulness is RECURSIVE (C8 iter-5 — the former nested-nodup boundary is
REMOVED, option (a)): `pyEqV`'s own dict arm is last-write-wins by construction
(`dSubM` skips shadowed entries; `dHasM` matches the target's LAST write), so
the reference `dictDenoteEq` — whose VALUE comparison is `pyEqV` — is faithful
at EVERY nesting level: `{1: {2:10, 2:20}} == {1: {2:20}}` is TRUE, as in
CPython. That is itself a THEOREM, not a design remark: `pyEqV_dict_iff_denote`
below proves the dict arm decides `dictDenoteEq` (under key-self-equality
hypotheses, satisfied by all modeled dicts). The `dictNoDupKeys` hypotheses on
`dictEq_iff_denote` are NOT a domain restriction of the fidelity claims — they
are the internal contract of RAW `dictEq` (a match-SOME subset, faithful only
on canonical lists), and `dictEqCanon_faithful` DISCHARGES them via
`dictDedup_nodup`. Only `.lenObj` (opaque object identity) stays outside the
faithful domain. -/

/-- Option-level value equality: both absent, or both present and `pyEqV`-equal. -/
def optPyEqV : Option PyValue → Option PyValue → Bool
  | none, none => true
  | some x, some y => pyEqV x y
  | _, _ => false

/-- INDEPENDENT Python-dict-equality semantics: last-write-wins lookups agree at
    every probe key (bool⊂int handled inside `pyLastVal` via `pyEqV` key
    matching). Defined WITHOUT reference to `dictEq`/`dSubM` — the reference the
    decision procedure `dictEqCanon` is proved faithful to. -/
def dictDenoteEq (a b : List (PyValue × PyValue)) : Prop :=
  ∀ k, optPyEqV (pyLastVal k a) (pyLastVal k b) = true

/-- **Recursive-level fidelity (C8 iter-5): the `pyEqV` dict ARM decides the
    independent last-write-wins semantics.** This is the theorem that makes the
    reference `dictDenoteEq` faithful at EVERY nesting level: nested dict VALUES
    are compared by exactly this arm, so a nested `{2:10, 2:20}` equals `{2:20}`
    as in CPython, and `dictEqCanon_faithful` does not bottom out in an
    unfaithful comparison one level down. The key-self-equality hypotheses
    (satisfied by every modeled dict — scalar keys) are load-bearing for the ⟸
    direction: a key that is not `pyEqV`-reflexive (e.g. `.lenObj`) cannot even
    be looked up by `pyLastVal`. The ⟹ direction uses neither hypothesis. -/
theorem pyEqV_dict_iff_denote (a b : List (PyValue × PyValue))
    (hka : ∀ p, p ∈ a → pyEqV p.1 p.1 = true)
    (hkb : ∀ p, p ∈ b → pyEqV p.1 p.1 = true) :
    pyEqV (.dict a) (.dict b) = true ↔ dictDenoteEq a b := by
  simp only [pyEqV, Bool.and_eq_true]
  constructor
  · rintro ⟨hab, hba⟩ k
    cases hxa : pyLastVal k a with
    | some v =>
        obtain ⟨u, hu, hvu⟩ := dLookup_trans' k v a b hab hxa
        simp [optPyEqV, hu, hvu]
    | none =>
        cases hxb : pyLastVal k b with
        | none => simp [optPyEqV]
        | some u =>
            obtain ⟨v, hv, -⟩ := dLookup_trans' k u b a hba hxb
            rw [hxa] at hv
            simp at hv
  · intro hd
    refine ⟨?_, ?_⟩
    · refine dSubM_of_denote_aux a b (fun k v hv => ?_) hka a
        (fun _ hp => hp) (fun _ _ h => h)
      have h := hd k
      rw [hv] at h
      cases hu : pyLastVal k b with
      | none => rw [hu] at h; simp [optPyEqV] at h
      | some u => rw [hu] at h; exact ⟨u, rfl, by simpa [optPyEqV] using h⟩
    · refine dSubM_of_denote_aux b a (fun k v hv => ?_) hkb b
        (fun _ hp => hp) (fun _ _ h => h)
      have h := hd k
      rw [hv] at h
      cases hu : pyLastVal k a with
      | none => rw [hu] at h; simp [optPyEqV] at h
      | some u =>
          rw [hu] at h
          simp only [optPyEqV] at h
          exact ⟨u, rfl, pyEqV_symm u v ▸ h⟩

/-- info: 'PythExpandVerify.pyEqV_dict_iff_denote' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms pyEqV_dict_iff_denote

/-- No matching key ⇒ the lookup is absent. -/
private theorem pyLastVal_none_of_noMatch (k : PyValue) :
    ∀ l, l.any (fun q => pyEqV k q.1) = false → pyLastVal k l = none
  | [], _ => rfl
  | (k', v') :: rest, h => by
      simp only [List.any_cons, Bool.or_eq_false_iff] at h
      have hkk : pyEqV k k' = false := by simpa using h.1
      simp only [pyLastVal, pyLastVal_none_of_noMatch k rest h.2]
      simp [hkk]

/-- Lookups agree at `pyEqV`-equal probe keys. -/
private theorem pyLastVal_congr (j k : PyValue) (h : pyEqV j k = true) :
    ∀ l, pyLastVal j l = pyLastVal k l
  | [] => rfl
  | (k', v') :: rest => by
      have hkey : pyEqV j k' = pyEqV k k' := by
        cases hj : pyEqV j k' with
        | true =>
            rw [pyEqV_trans k j k' (pyEqV_symm k j ▸ h) hj]
        | false =>
            cases hk : pyEqV k k' with
            | true => rw [pyEqV_trans j k k' h hk] at hj; exact absurd hj (by simp)
            | false => rfl
      simp only [pyLastVal, pyLastVal_congr j k h rest, hkey]

/-- On a nodup-key list, a present entry is exactly what its key looks up to. -/
private theorem pyLastVal_nodup_mem (k v : PyValue) :
    ∀ l, dictNoDupKeys l = true → pyEqV k k = true → (k, v) ∈ l →
      pyLastVal k l = some v
  | [], _, _, hmem => by simp at hmem
  | (k0, v0) :: rest, hnd, hrefl, hmem => by
      simp only [dictNoDupKeys, Bool.and_eq_true] at hnd
      rcases List.mem_cons.mp hmem with heq | htl
      · injection heq with h1 h2; subst h1; subst h2
        have hno : pyLastVal k rest = none :=
          pyLastVal_none_of_noMatch k rest (by simpa using hnd.1)
        simp only [pyLastVal, hno, hrefl, if_true]
      · have ih := pyLastVal_nodup_mem k v rest hnd.2 hrefl htl
        simp only [pyLastVal, ih]

/-- Generalization of `pyLastVal_nodup_mem` to a `pyEqV`-equal probe key: on a
    nodup list, an entry `(k0, v)` is found by ANY probe `k` with `pyEqV k k0`. -/
private theorem pyLastVal_nodup_match (k : PyValue) :
    ∀ l k0 v, dictNoDupKeys l = true → pyEqV k k0 = true → (k0, v) ∈ l →
      pyLastVal k l = some v
  | [], k0, v, _, _, hmem => by simp at hmem
  | (k1, v1) :: rest, k0, v, hnd, hmatch, hmem => by
      simp only [dictNoDupKeys, Bool.and_eq_true] at hnd
      rcases List.mem_cons.mp hmem with heq | htl
      · injection heq with h1 h2; subst h1; subst h2   -- k0 := k1, v := v1
        have hrest : rest.any (fun q => pyEqV k q.1) = false := by
          have hk1 : rest.any (fun q => pyEqV k0 q.1) = false := by simpa using hnd.1
          -- pyEqV k q.1 ⇒ pyEqV k0 q.1 (k0 ~ k ~ q), contrapositive
          clear hnd hmem
          induction rest with
          | nil => rfl
          | cons q qs ih =>
              simp only [List.any_cons, Bool.or_eq_false_iff] at hk1 ⊢
              refine ⟨?_, ih hk1.2⟩
              cases hq : pyEqV k q.1 with
              | false => rfl
              | true =>
                  have : pyEqV k0 q.1 = true :=
                    pyEqV_trans k0 k q.1 (pyEqV_symm k0 k ▸ hmatch) hq
                  rw [this] at hk1; exact absurd hk1.1 (by simp)
        have hno : pyLastVal k rest = none := pyLastVal_none_of_noMatch k rest hrest
        simp only [pyLastVal, hno, hmatch, if_true]
      · have ih := pyLastVal_nodup_match k rest k0 v hnd.2 hmatch htl
        simp only [pyLastVal, ih]

/-- Filtering out entries whose key `pyEqV`-matches `k0` moves the lookup: a probe
    equal to `k0` finds nothing; any other probe is unaffected. The core lemma for
    canonicalization preserving denotation. -/
private theorem pyLastVal_filter (k k0 : PyValue) (l : List (PyValue × PyValue)) :
    pyLastVal k (l.filter (fun q => !pyEqV k0 q.1))
       = if pyEqV k k0 then none else pyLastVal k l := by
  cases hc : pyEqV k k0 with
  | true =>
      show pyLastVal k (l.filter (fun q => !pyEqV k0 q.1)) = none
      apply pyLastVal_none_of_noMatch
      induction l with
      | nil => rfl
      | cons q qs ih =>
          obtain ⟨qk, qv⟩ := q
          simp only [List.filter_cons]
          cases hq0 : pyEqV k0 qk with
          | true => simpa [hq0] using ih
          | false =>
              simp only [Bool.not_false, if_true, List.any_cons, Bool.or_eq_false_iff]
              refine ⟨?_, ih⟩
              cases hkq : pyEqV k qk with
              | false => rfl
              | true =>
                  have : pyEqV k0 qk = true := pyEqV_trans k0 k qk (pyEqV_symm k0 k ▸ hc) hkq
                  rw [hq0] at this; exact absurd this (by simp)
  | false =>
      show pyLastVal k (l.filter (fun q => !pyEqV k0 q.1)) = pyLastVal k l
      induction l with
      | nil => rfl
      | cons q qs ih =>
          obtain ⟨qk, qv⟩ := q
          simp only [List.filter_cons]
          cases hq0 : pyEqV k0 qk with
          | true =>
              have hkq : pyEqV k qk = false := by
                cases h : pyEqV k qk with
                | false => rfl
                | true =>
                    have : pyEqV k k0 = true :=
                      pyEqV_trans k qk k0 h (pyEqV_symm k0 qk ▸ hq0)
                    rw [hc] at this; exact absurd this (by simp)
              show pyLastVal k (qs.filter (fun q => !pyEqV k0 q.1)) = pyLastVal k ((qk, qv) :: qs)
              rw [ih, pyLastVal]
              cases hlk : pyLastVal k qs with
              | none => simp [hkq]
              | some w => rfl
          | false =>
              show pyLastVal k ((qk, qv) :: qs.filter (fun q => !pyEqV k0 q.1))
                 = pyLastVal k ((qk, qv) :: qs)
              simp only [pyLastVal, ih]

/-- **Canonicalization is DENOTATION-PRESERVING (the heart of C8 fidelity).** For
    EVERY probe key, the last-write-wins lookup on `dictDedup a` equals the lookup
    on `a`: the canonical form denotes exactly the same Python mapping. This is a
    universal fidelity statement about `dictDedup`, referencing only `pyLastVal`
    (CPython "last write wins"), not `dictEq`. -/
theorem dictDedup_denote (k : PyValue) :
    ∀ a, pyLastVal k (dictDedup a) = pyLastVal k a
  | [] => rfl
  | (k0, v0) :: rest => by
      have ih := dictDedup_denote k rest
      simp only [dictDedup, pyLastVal]
      rw [pyLastVal_filter k k0 (dictDedup rest), ih]
      cases hc : pyEqV k k0 with
      | true =>
          simp only [if_true]
          rw [pyLastVal_congr k k0 hc rest]
          cases hlk : pyLastVal k0 rest with
          | none => simp
          | some w => simp
      | false =>
          cases hlk : pyLastVal k rest with
          | none => simp
          | some w => simp

/-- Every key surviving canonicalization is scalar (so it is `pyEqV`-reflexive) —
    `dictDedup` selects keys already present in the modeled input. -/
private theorem dictDedup_keys_scalar :
    ∀ a, PyValue.dictModeled a = true → ∀ p, p ∈ dictDedup a → p.1.isScalar = true
  | [], _, p, hp => by simp [dictDedup] at hp
  | (k0, v0) :: rest, h, p, hp => by
      simp only [PyValue.dictModeled, Bool.and_eq_true] at h
      simp only [dictDedup, List.mem_cons] at hp
      rcases hp with rfl | htl
      · exact h.1.1
      · exact dictDedup_keys_scalar rest h.2 p (List.mem_filter.mp htl).1

/-- **RAW-`dictEq` nodup contract (internal lemma; NOT a domain restriction on
    any fidelity claim).** This is the internal correctness contract of the RAW
    match-SOME `dictEq`, which is faithful only on CANONICAL (nodup-key) lists;
    the C8 fidelity theorem `dictEqCanon_faithful` discharges this contract via
    `dictDedup_nodup` (C8 was delivered as option (a) — recursive last-write
    canonicalization — so this nodup hypothesis restricts nothing user-facing).
    On NODUP-key entry lists with (`pyEqV`-reflexive) scalar keys, `dictEq`
    decides the INDEPENDENT last-write-wins dict-equality semantics
    `dictDenoteEq`: `dictEq a b = true ↔ (∀ k, the lookups agree)`. Both hypotheses
    are load-bearing — the reverse direction reconstructs mutual subset from
    lookup agreement using nodup uniqueness, and the forward direction uses nodup
    to pin each probe to a unique record. -/
theorem dictEq_iff_denote (a b : List (PyValue × PyValue))
    (ha : dictNoDupKeys a = true) (hb : dictNoDupKeys b = true)
    (hka : ∀ p ∈ a, pyEqV p.1 p.1 = true) (hkb : ∀ p ∈ b, pyEqV p.1 p.1 = true) :
    dictEq a b = true ↔ dictDenoteEq a b := by
  constructor
  · -- dictEq → denote
    intro hde k
    simp only [dictEq, Bool.and_eq_true, pyDSubset, List.all_eq_true, List.any_eq_true,
      Bool.and_eq_true] at hde
    obtain ⟨hab, hba⟩ := hde
    -- one-sided transport: pyLastVal k a = some va ⇒ pyLastVal k b = some vb ∧ pyEqV va vb
    have trans_ab : ∀ (x y : List (PyValue × PyValue)),
        dictNoDupKeys y = true →
        (∀ p ∈ x, ∃ q ∈ y, pyEqV p.1 q.1 = true ∧ pyEqV p.2 q.2 = true) →
        ∀ va, pyLastVal k x = some va → ∃ vb, pyLastVal k y = some vb ∧ pyEqV va vb = true := by
      intro x y hy hsub va hxa
      obtain ⟨p, hp, hkp, hvp⟩ := pyLastVal_some k x va hxa
      obtain ⟨q, hq, hkq, hvq⟩ := hsub p hp
      have hkkq : pyEqV k q.1 = true := pyEqV_trans k p.1 q.1 hkp hkq
      refine ⟨q.2, pyLastVal_nodup_match k y q.1 q.2 hy hkkq (by cases q; exact hq), ?_⟩
      rw [← hvp]; exact hvq
    cases hxa : pyLastVal k a with
    | some va =>
        obtain ⟨vb, hyb, hvab⟩ := trans_ab a b hb hab va hxa
        simp only [optPyEqV, hyb, hvab]
    | none =>
        cases hyb : pyLastVal k b with
        | none => simp [optPyEqV]
        | some vb =>
            obtain ⟨va, hxa', _⟩ := trans_ab b a ha hba vb hyb
            rw [hxa] at hxa'; exact absurd hxa' (by simp)
  · -- denote → dictEq
    intro hde
    simp only [dictEq, Bool.and_eq_true, pyDSubset, List.all_eq_true, List.any_eq_true,
      Bool.and_eq_true]
    refine ⟨?_, ?_⟩
    · intro p hp
      have hpa : pyLastVal p.1 a = some p.2 :=
        pyLastVal_nodup_mem p.1 p.2 a ha (hka p hp) (by cases p; exact hp)
      have := hde p.1
      rw [hpa] at this
      cases hpb : pyLastVal p.1 b with
      | none => rw [hpb] at this; exact absurd this (by simp [optPyEqV])
      | some vb =>
          rw [hpb] at this
          obtain ⟨q, hq, hkq, hvq⟩ := pyLastVal_some p.1 b vb hpb
          exact ⟨q, hq, hkq, hvq ▸ (by simpa [optPyEqV] using this)⟩
    · intro p hp
      have hpb : pyLastVal p.1 b = some p.2 :=
        pyLastVal_nodup_mem p.1 p.2 b hb (hkb p hp) (by cases p; exact hp)
      have := hde p.1
      rw [hpb] at this
      cases hpa : pyLastVal p.1 a with
      | none => rw [hpa] at this; exact absurd this (by simp [optPyEqV])
      | some va =>
          rw [hpa] at this
          obtain ⟨q, hq, hkq, hvq⟩ := pyLastVal_some p.1 a va hpa
          refine ⟨q, hq, hkq, hvq ▸ ?_⟩
          have hsymm := this
          simp only [optPyEqV] at hsymm
          exact pyEqV_symm p.2 va ▸ hsymm

/-- **THE general `dictEqCanon` fidelity theorem (C8 — replaces the three concrete
    evals).** For ALL modeled entry lists — INCLUDING top-level DUPLICATE keys —
    `dictEqCanon` decides the independent last-write-wins Python-dict-equality
    semantics `dictDenoteEq`. Universal, non-vacuous, and CPython-faithful over the
    modeled value fragment: `dictEqCanon` canonicalizes (denotation-preserving,
    `dictDedup_denote`) then compares two nodup lists faithfully
    (`dictEq_iff_denote`). The reviewer's `[(2,10),(2,20)]` vs `[(2,20)]` is the
    top-level-duplicate case, now covered generally — and because the VALUE
    comparison on both sides of the iff is the last-write-wins `pyEqV`
    (`pyEqV_dict_iff_denote`), the statement is faithful RECURSIVELY: nested
    duplicate keys are covered too (`{1:{2:10,2:20}} == {1:{2:20}}`, SPOTs
    below). -/
theorem dictEqCanon_faithful (a b : List (PyValue × PyValue))
    (ha : PyValue.dictModeled a = true) (hb : PyValue.dictModeled b = true) :
    dictEqCanon a b = true ↔ dictDenoteEq a b := by
  have hkey : ∀ (c : List (PyValue × PyValue)), PyValue.dictModeled c = true →
      ∀ p ∈ dictDedup c, pyEqV p.1 p.1 = true := by
    intro c hc p hp
    exact pyEqV_refl p.1 (isScalar_eqModeled (dictDedup_keys_scalar c hc p hp))
  rw [dictEqCanon,
      dictEq_iff_denote (dictDedup a) (dictDedup b)
        (dictDedup_nodup a) (dictDedup_nodup b) (hkey a ha) (hkey b hb)]
  constructor
  · intro h k; have := h k; rwa [dictDedup_denote k a, dictDedup_denote k b] at this
  · intro h k; have := h k; rwa [dictDedup_denote k a, dictDedup_denote k b]

-- SPOTs beneath the general theorem `dictEqCanon_faithful` (concrete client
-- facts derived THROUGH the universal statement, not standing in for it).
/-- SPOT (through the theorem, `.mpr`): the reviewer's TOP-LEVEL-DUPLICATE
    counterexample — `[(2,10),(2,20)]` and `[(2,20)]` both denote `{2:20}`, so once
    their last-write-wins denotations are known to agree they are `dictEqCanon`-
    equal. A too-weak/vacuous general statement could not close this. -/
example (h : dictDenoteEq [(.num 2, .num 10), (.num 2, .num 20)] [(.num 2, .num 20)]) :
    dictEqCanon [(.num 2, .num 10), (.num 2, .num 20)] [(.num 2, .num 20)] = true :=
  (dictEqCanon_faithful _ _
     (by simp [PyValue.dictModeled, PyValue.isScalar, PyValue.eqModeled])
     (by simp [PyValue.dictModeled, PyValue.isScalar, PyValue.eqModeled])).mpr h
/-- SPOT (through the theorem, `.mp`): `dictEqCanon`-equality delivers the
    independent last-write-wins semantics as a client-usable consequence. -/
example (h : dictEqCanon [(.num 2, .num 10), (.num 2, .num 20)] [(.num 2, .num 20)] = true) :
    dictDenoteEq [(.num 2, .num 10), (.num 2, .num 20)] [(.num 2, .num 20)] :=
  (dictEqCanon_faithful _ _
     (by simp [PyValue.dictModeled, PyValue.isScalar, PyValue.eqModeled])
     (by simp [PyValue.dictModeled, PyValue.isScalar, PyValue.eqModeled])).mp h
-- concrete evaluations (the evaluation mechanism = `#guard`, matching CPython):
-- both `{2:10, 2:20}` and `{2:20}` denote `{2:20}`; distinct values stay distinct.
#guard dictEqCanon [(.num 2, .num 10), (.num 2, .num 20)] [(.num 2, .num 20)] = true
#guard dictEqCanon [(.num 1, .num 10)] [(.num 1, .num 20)] = false

-- C8 iter-5 SPOTs — the NESTED-duplicate-key counterexample, routed THROUGH the
-- theorems at BOTH levels (never by bare evaluation): CPython says
-- `{1: {2:10, 2:20}} == {1: {2:20}}` is True (both denote `{1: {2:20}}`).
/-- SPOT (outright, `.mpr`, TWO-LEVEL): the nested-dup pair IS `dictEqCanon`-
    equal, derived by proving the last-write-wins denotations agree — the NESTED
    value equality obtained THROUGH `pyEqV_dict_iff_denote` (the recursive-level
    fidelity theorem), the top level THROUGH `dictEqCanon_faithful`. A reference
    still unfaithful one level down (the iter-4 defect) could not close this. -/
example : dictEqCanon [(.num 1, .dict [(.num 2, .num 10), (.num 2, .num 20)])]
                      [(.num 1, .dict [(.num 2, .num 20)])] = true := by
  apply (dictEqCanon_faithful _ _
      (by simp [PyValue.dictModeled, PyValue.isScalar, PyValue.eqModeled])
      (by simp [PyValue.dictModeled, PyValue.isScalar, PyValue.eqModeled])).mpr
  intro k
  have hkey2 : pyEqV (.num 2) (.num 2) = true := by simp [pyEqV]
  have hnested : pyEqV (.dict [(.num 2, .num 10), (.num 2, .num 20)])
                       (.dict [(.num 2, .num 20)]) = true := by
    apply (pyEqV_dict_iff_denote _ _
        (fun p hp => by
          rcases List.mem_cons.mp hp with rfl | hp'
          · exact hkey2
          · rcases List.mem_cons.mp hp' with rfl | hp''
            · exact hkey2
            · simp at hp'')
        (fun p hp => by
          rcases List.mem_cons.mp hp with rfl | hp'
          · exact hkey2
          · simp at hp')).mpr
    intro j
    simp only [pyLastVal]
    cases hj : pyEqV j (.num 2) with
    | true => simp [optPyEqV, pyEqV]
    | false => simp [optPyEqV]
  simp only [pyLastVal]
  cases hk : pyEqV k (.num 1) with
  | true => simpa [hk, optPyEqV] using hnested
  | false => simp [optPyEqV]
/-- SPOT (through the theorem, `.mp`): nested-dup `dictEqCanon`-equality
    delivers the recursive last-write-wins semantics as a client-usable
    universal consequence (a `∀`-statement no evaluation could produce). -/
example (h : dictEqCanon [(.num 1, .dict [(.num 2, .num 10), (.num 2, .num 20)])]
                         [(.num 1, .dict [(.num 2, .num 20)])] = true) :
    dictDenoteEq [(.num 1, .dict [(.num 2, .num 10), (.num 2, .num 20)])]
                 [(.num 1, .dict [(.num 2, .num 20)])] :=
  (dictEqCanon_faithful _ _
     (by simp [PyValue.dictModeled, PyValue.isScalar, PyValue.eqModeled])
     (by simp [PyValue.dictModeled, PyValue.isScalar, PyValue.eqModeled])).mp h
-- concrete evaluations beneath the SPOTs (matching CPython):
#guard dictEqCanon [(.num 1, .dict [(.num 2, .num 10), (.num 2, .num 20)])]
                   [(.num 1, .dict [(.num 2, .num 20)])] = true
-- non-vacuity: the FIRST write must NOT win, nested included
#guard dictEqCanon [(.num 1, .dict [(.num 2, .num 10), (.num 2, .num 20)])]
                   [(.num 1, .dict [(.num 2, .num 10)])] = false

/-- info: 'PythExpandVerify.dictDedup_denote' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms dictDedup_denote

/-- info: 'PythExpandVerify.dictEq_iff_denote' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms dictEq_iff_denote

/-- info: 'PythExpandVerify.dictEqCanon_faithful' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms dictEqCanon_faithful

/-! ## Tier-3 SEED — arithmetic-fragment compilation preservation (§9 Tier 3; §15)

A SMALL but real preservation result: a source arithmetic fragment (int
literals, neg, `+`, `−`, `×`, and the two Python-vs-JS DEVIATION operators `//`
and `%`) compiles to a target model of the emitted JS, and evaluating the
compiled program yields the Python value. The content is the deviation: Python
`//`/`%` FLOOR (mod takes the divisor's sign); JS truncates toward 0, so codegen
emits a floor-correction — `preservation` proves that correction reaches Python
semantics, not the wrong JS-trunc value. This is the SEED of full numeric-
fragment preservation; the full language semantics + a differential binding to
the shipped compiler stay future work (priced §8.1 at 2–4k lines). Spec
validated Stage-3 (`experiments/pbt-ps/tier3_fdiv_validate.py`, 1640 pairs DRY:
the correction reproduces CPython `//`,`%`). SPOT-led (§15): the deviation is
proven PRESERVED through `preservation`. -/

inductive PyE where
  | lit (n : Int)
  | neg (a : PyE)
  | add (a b : PyE)
  | sub (a b : PyE)
  | mul (a b : PyE)
  | fdiv (a b : PyE)   -- Python floor division  a // b
  | fmod (a b : PyE)   -- Python floor modulo    a % b
deriving Repr

/-- Reference (Python) semantics; `none` on division by zero. -/
def evalPy : PyE → Option Int
  | .lit n => some n
  | .neg a => (evalPy a).map (fun x => -x)
  | .add a b => match evalPy a, evalPy b with | some x, some y => some (x + y) | _, _ => none
  | .sub a b => match evalPy a, evalPy b with | some x, some y => some (x - y) | _, _ => none
  | .mul a b => match evalPy a, evalPy b with | some x, some y => some (x * y) | _, _ => none
  | .fdiv a b => match evalPy a, evalPy b with
      | some x, some y => if y = 0 then none else some (Int.fdiv x y)
      | _, _ => none
  | .fmod a b => match evalPy a, evalPy b with
      | some x, some y => if y = 0 then none else some (Int.fmod x y)
      | _, _ => none

/-- The floor-correction over truncating division that codegen emits for `//`. -/
def jsFdiv (x y : Int) : Int :=
  let q := Int.tdiv x y
  let r := x - q * y
  if r = 0 then q
  else if decide (r < 0) = decide (y < 0) then q
  else q - 1

/-- The emitted `%` (Python mod = x − ⌊x/y⌋·y). -/
def jsFmod (x y : Int) : Int := x - jsFdiv x y * y

/-- Target semantics — the COMPILED program (structure identical; `//` and `%`
    use the emitted correction formulas). -/
def evalTgt : PyE → Option Int
  | .lit n => some n
  | .neg a => (evalTgt a).map (fun x => -x)
  | .add a b => match evalTgt a, evalTgt b with | some x, some y => some (x + y) | _, _ => none
  | .sub a b => match evalTgt a, evalTgt b with | some x, some y => some (x - y) | _, _ => none
  | .mul a b => match evalTgt a, evalTgt b with | some x, some y => some (x * y) | _, _ => none
  | .fdiv a b => match evalTgt a, evalTgt b with
      | some x, some y => if y = 0 then none else some (jsFdiv x y)
      | _, _ => none
  | .fmod a b => match evalTgt a, evalTgt b with
      | some x, some y => if y = 0 then none else some (jsFmod x y)
      | _, _ => none

-- Executable model bindings — the deviation values (Python, not JS-trunc).
#guard evalTgt (.fdiv (.lit (-7)) (.lit 2)) = some (-4)   -- Python floor, not -3
#guard evalTgt (.fmod (.lit (-7)) (.lit 2)) = some 1      -- sign of divisor
#guard evalPy (.fdiv (.lit (-7)) (.lit 2)) = some (-4)
#guard evalTgt (.add (.mul (.lit 3) (.lit 4)) (.lit 2)) = some 14

/-- Helper: truncating remainder of a nonpositive dividend is nonpositive
    (mirror of `Int.tmod_nonneg` through `Int.neg_tmod`). -/
private theorem tmod_nonpos_of_nonpos {a : Int} (b : Int) (h : a ≤ 0) :
    Int.tmod a b ≤ 0 := by
  have h' : 0 ≤ Int.tmod (-a) b := Int.tmod_nonneg b (by omega)
  rw [Int.neg_tmod] at h'
  omega

/-- The emitted `//` correction computes Python floor division (y ≠ 0). -/
theorem jsFdiv_eq_fdiv (x y : Int) (hy : y ≠ 0) : jsFdiv x y = Int.fdiv x y := by
  -- `r` in `jsFdiv` is exactly `Int.tmod x y`; the correction cases then match
  -- `Int.fdiv_eq_tdiv`'s sign/divisibility analysis (tmod carries the dividend's
  -- sign, so `r < 0 ↔ y < 0` decides whether truncation already floored).
  have hnn : 0 ≤ x → 0 ≤ Int.tmod x y := fun h => Int.tmod_nonneg y h
  have hnp : x ≤ 0 → Int.tmod x y ≤ 0 := fun h => tmod_nonpos_of_nonpos y h
  rw [Int.fdiv_eq_tdiv]
  simp only [jsFdiv]
  rw [Int.mul_comm (Int.tdiv x y) y, ← Int.tmod_def]
  simp only [Int.dvd_iff_tmod_eq_zero]
  rcases (by omega : y < 0 ∨ 0 < y) with hneg | hpos
  · rw [Int.sign_eq_neg_one_of_neg hneg]
    by_cases h0 : Int.tmod x y = 0
    · simp [h0]
    · rw [if_neg h0, if_neg h0]
      by_cases hx : 0 ≤ x
      · have := hnn hx
        rw [if_neg (by simp only [decide_eq_decide]; omega),
            if_pos hx, if_neg (by omega : ¬ (0:Int) ≤ y)]
      · have := hnp (by omega)
        rw [if_pos (by simp only [decide_eq_decide]; omega),
            if_neg hx, if_neg (by omega : ¬ (0:Int) ≤ y)]
        omega
  · rw [Int.sign_eq_one_of_pos hpos]
    by_cases h0 : Int.tmod x y = 0
    · simp [h0]
    · rw [if_neg h0, if_neg h0]
      by_cases hx : 0 ≤ x
      · have := hnn hx
        rw [if_pos (by simp only [decide_eq_decide]; omega),
            if_pos hx, if_pos (by omega : (0:Int) ≤ y)]
        omega
      · have := hnp (by omega)
        rw [if_neg (by simp only [decide_eq_decide]; omega),
            if_neg hx, if_pos (by omega : (0:Int) ≤ y)]

/-- The emitted `%` computes Python floor modulo (y ≠ 0). -/
theorem jsFmod_eq_fmod (x y : Int) (hy : y ≠ 0) : jsFmod x y = Int.fmod x y := by
  unfold jsFmod
  rw [jsFdiv_eq_fdiv x y hy, Int.fmod_def, Int.mul_comm (Int.fdiv x y) y]

/-- **Preservation (Tier-3 seed).** Evaluating the COMPILED program yields the
    Python value, for every expression in the fragment — including the `//`/`%`
    deviation, where the emitted floor-correction reaches Python semantics. -/
theorem preservation (e : PyE) : evalTgt e = evalPy e := by
  induction e with
  | lit n => rfl
  | neg a ih => simp [evalTgt, evalPy, ih]
  | add a b iha ihb => simp [evalTgt, evalPy, iha, ihb]
  | sub a b iha ihb => simp [evalTgt, evalPy, iha, ihb]
  | mul a b iha ihb => simp [evalTgt, evalPy, iha, ihb]
  | fdiv a b iha ihb =>
    simp only [evalTgt, evalPy, iha, ihb]
    cases evalPy a with
    | none => rfl
    | some x =>
      cases evalPy b with
      | none => rfl
      | some y =>
        by_cases hy : y = 0
        · simp [hy]
        · simp [hy, jsFdiv_eq_fdiv x y hy]
  | fmod a b iha ihb =>
    simp only [evalTgt, evalPy, iha, ihb]
    cases evalPy a with
    | none => rfl
    | some x =>
      cases evalPy b with
      | none => rfl
      | some y =>
        by_cases hy : y = 0
        · simp [hy]
        · simp [hy, jsFmod_eq_fmod x y hy]

/-- SPOT: `-7 // 2` compiles to Python's `-4` (floor), not JS-trunc `-3`, via `preservation`. -/
example : evalTgt (.fdiv (.lit (-7)) (.lit 2)) = some (-4) := by rw [preservation]; rfl

/-- SPOT: `-7 % 2` is preserved as `1` (sign of divisor), via `preservation`. -/
example : evalTgt (.fmod (.lit (-7)) (.lit 2)) = some 1 := by rw [preservation]; rfl

/-- info: 'PythExpandVerify.preservation' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservation

/-! ## Tier-3 wave 2 — statement-language preservation (variables, control flow, state)

Extends the arithmetic seed to a small IMPERATIVE fragment: variables over an
environment, expressions (incl. the `//`/`%` deviation, reusing `jsFdiv`/`jsFmod`
+ their `_eq_` lemmas), and statements (assignment, sequencing, `if`, bounded
`while`). Preservation now covers **control flow and state**, not just expression
values — a step toward full numeric-fragment preservation.

Design (re-architected in the C1 rollout, waves 1–2): `evalE false`/`evalS false`
is the Python REFERENCE semantics. The compiled target is the INDEPENDENT
evaluator pair `evalEtgt`/`evalStgt`, parameterized by the integer-division
lowering the emitted JS uses (wave-1/wave-2 sections below). The `tgt : Bool`
flag on `evalE`/`evalS` is retained only as the historical definition shape —
its `true` branch is LEGACY and carries NO theorem; every preservation statement
binds the independent target to `eval* false`. `if`/`while` branch on Python
truthiness (nonzero). Still a SEED over a model; the shipped-compiler binding is
the Stage-3 differential (`tier3_shipped_binding.py`). -/

abbrev Env := List (String × Int)

def Env.get (env : Env) (n : String) : Option Int :=
  (env.find? (fun p => p.1 == n)).map (·.2)

def Env.set (env : Env) (n : String) (v : Int) : Env := (n, v) :: env

inductive Exp where
  | lit (n : Int)
  | var (s : String)
  | add (a b : Exp)
  | sub (a b : Exp)
  | mul (a b : Exp)
  | fdiv (a b : Exp)
  | fmod (a b : Exp)
  | lt (a b : Exp)        -- returns 1/0 (Python bool ⊂ int)
deriving Repr

/-- Expression eval. `tgt = false` = Python REFERENCE semantics (the only branch
    any theorem uses). The `tgt = true` branch is LEGACY (the former F1 model-vs-
    model flag) and is NOT the compiled target — the genuine compiled target is
    the INDEPENDENT `evalEtgt (L : IntDivLowering)` below; no theorem references
    `evalE true`. `none` = error (unbound var / div-by-zero). -/
def evalE (tgt : Bool) : Exp → Env → Option Int
  | .lit n, _ => some n
  | .var s, env => env.get s
  | .add a b, env => match evalE tgt a env, evalE tgt b env with
      | some x, some y => some (x + y) | _, _ => none
  | .sub a b, env => match evalE tgt a env, evalE tgt b env with
      | some x, some y => some (x - y) | _, _ => none
  | .mul a b, env => match evalE tgt a env, evalE tgt b env with
      | some x, some y => some (x * y) | _, _ => none
  | .fdiv a b, env => match evalE tgt a env, evalE tgt b env with
      | some x, some y => if y = 0 then none else some (if tgt then jsFdiv x y else Int.fdiv x y)
      | _, _ => none
  | .fmod a b, env => match evalE tgt a env, evalE tgt b env with
      | some x, some y => if y = 0 then none else some (if tgt then jsFmod x y else Int.fmod x y)
      | _, _ => none
  | .lt a b, env => match evalE tgt a env, evalE tgt b env with
      | some x, some y => some (if x < y then 1 else 0) | _, _ => none

inductive Stmt where
  | skip
  | assign (s : String) (e : Exp)
  | seq (a b : Stmt)
  | ite (c : Exp) (t e : Stmt)
  | whileF (fuel : Nat) (c : Exp) (body : Stmt)   -- fuel-bounded
deriving Repr

/-- Statement eval → updated environment (`none` = error / fuel exhausted).
    `if`/`while` branch on Python truthiness (`≠ 0`). -/
def evalS (tgt : Bool) : Stmt → Env → Option Env
  | .skip, env => some env
  | .assign s e, env => (evalE tgt e env).map (fun v => env.set s v)
  | .seq a b, env => (evalS tgt a env).bind (fun env' => evalS tgt b env')
  | .ite c t e, env => (evalE tgt c env).bind
      (fun v => if v ≠ 0 then evalS tgt t env else evalS tgt e env)
  | .whileF 0 _ _, _ => none
  | .whileF (f + 1) c body, env => (evalE tgt c env).bind
      (fun v => if v ≠ 0 then (evalS tgt body env).bind (fun env' => evalS tgt (.whileF f c body) env')
                else some env)

-- executable binding: the // deviation is preserved inside control flow + state.
-- x = -7 // 2 (= -4, Python floor); y = x * 2 (= -8).
def _tier3w2_demo : Stmt :=
  .seq (.assign "x" (.fdiv (.lit (-7)) (.lit 2))) (.assign "y" (.mul (.var "x") (.lit 2)))
-- F9 reference pin (CPython: x = -7//2; y = x*2 → y == -8); the compiled-side
-- guard lives with the independent `evalSjs` in the wave-2 section below.
#guard ((evalS false _tier3w2_demo []).bind (fun e => e.get "y")) = some (-8)

-- SHIPPED-COMPILER BINDING (the `expanddiff` pattern for the numeric fragment):
-- these model values are also produced by the SHIPPED `pyths` and by CPython on
-- the same programs, verified over the fragment corpus in
-- `experiments/pbt-ps/tier3_shipped_binding.py` (pyths-run == CPython, DRY). So
-- the proved `preservation`/`preservationS` (evalTgt = evalPy over the model)
-- corresponds to the real compiler on this fragment: model == pyths == CPython.
-- Loop program: `s=0; i=0; while i<6: s += (i-3)//2; i+=1` → s = -3 (Python floor).
def _tier3w2_loop : Stmt :=
  .seq (.assign "s" (.lit 0))
    (.seq (.assign "i" (.lit 0))
      (.whileF 10 (.lt (.var "i") (.lit 6))
        (.seq (.assign "s" (.add (.var "s") (.fdiv (.sub (.var "i") (.lit 3)) (.lit 2))))
              (.assign "i" (.add (.var "i") (.lit 1))))))
-- F9 reference pin (CPython gives -3); compiled-side guard with `evalSjs` below.
#guard ((evalS false _tier3w2_loop []).bind (fun e => e.get "s")) = some (-3)

/-! ### Wave 1 (C1 rollout) — INDEPENDENT-target expression preservation

The previous `preservationE : evalE true e env = evalE false e env` was an F1
model-vs-model tautology: both sides are the SAME evaluator with a `Bool` flag
that only flips the `//`/`%` arm, so stubbing the shipping lowering could not
break it. Re-architected on the codex-accepted C1 pattern
(`poc_preservation_real`/`poc_preservation_stub_fails`): the target evaluator
`evalEtgt` is a SEPARATE definition, parameterized by the integer-division
LOWERING the emitted JS uses — ranging the preservation predicate over the
lowering is what makes the statement falsifiable. It is TRUE for the shipped
floor-correction (`jsFdiv`/`jsFmod`, `preservationE`) and provably FALSE for
the naive truncating lowering a wrong compiler would emit
(`preservationE_stub_fails`). -/

/-- An integer-division LOWERING: the pair of operations the emitted JS uses
    for `//` and `%`. The preservation predicate ranges over this, so a wrong
    lowering FALSIFIES it (the PoC-wave-20 `DictLookup` move, reused for the
    expression fragment). -/
structure IntDivLowering where
  fdiv : Int → Int → Int
  fmod : Int → Int → Int

/-- The lowering codegen ACTUALLY emits for `//`/`%`: the floor-correction
    over JS truncating division (the seed's `jsFdiv`/`jsFmod`). -/
def jsELowering : IntDivLowering := ⟨jsFdiv, jsFmod⟩

/-- A deliberately WRONG lowering a naive compiler WOULD emit: raw JS
    `Math.trunc(x / y)` and `x % y` — both truncate toward zero
    (`Int.tdiv`/`Int.tmod`), diverging from Python on mixed signs. -/
def truncELowering : IntDivLowering := ⟨Int.tdiv, Int.tmod⟩

/-- **Independent target evaluator** for the `Exp` fragment: the compiled
    program's semantics under lowering `L`. A SEPARATE definition (not a
    `Bool` flag on `evalE`); the `//`/`%` arms call the lowering's operations,
    mirroring the emitted JS expression code. -/
def evalEtgt (L : IntDivLowering) : Exp → Env → Option Int
  | .lit n, _ => some n
  | .var s, env => env.get s
  | .add a b, env => match evalEtgt L a env, evalEtgt L b env with
      | some x, some y => some (x + y) | _, _ => none
  | .sub a b, env => match evalEtgt L a env, evalEtgt L b env with
      | some x, some y => some (x - y) | _, _ => none
  | .mul a b, env => match evalEtgt L a env, evalEtgt L b env with
      | some x, some y => some (x * y) | _, _ => none
  | .fdiv a b, env => match evalEtgt L a env, evalEtgt L b env with
      | some x, some y => if y = 0 then none else some (L.fdiv x y)
      | _, _ => none
  | .fmod a b, env => match evalEtgt L a env, evalEtgt L b env with
      | some x, some y => if y = 0 then none else some (L.fmod x y)
      | _, _ => none
  | .lt a b, env => match evalEtgt L a env, evalEtgt L b env with
      | some x, some y => some (if x < y then 1 else 0) | _, _ => none

/-- The compiled expression semantics: the independent target under the
    SHIPPED lowering. -/
abbrev evalEjs : Exp → Env → Option Int := evalEtgt jsELowering

/-- Expression preservation as a predicate OVER the lowering — the SAME
    predicate is proved for the shipped lowering and REFUTED for the stub. -/
def EPreserves (L : IntDivLowering) : Prop :=
  ∀ e env, evalEtgt L e env = evalE false e env

-- F9 pins: the REFERENCE `evalE false` matches CPython on the deviation
-- (CPython: -7//2 = -4, -7%2 = 1, 7//-2 = -4, 7%-2 = -1).
#guard evalE false (.fdiv (.lit (-7)) (.lit 2)) [] = some (-4)
#guard evalE false (.fmod (.lit (-7)) (.lit 2)) [] = some 1
#guard evalE false (.fdiv (.lit 7) (.lit (-2))) [] = some (-4)
#guard evalE false (.fmod (.lit 7) (.lit (-2))) [] = some (-1)

/-- **Expression preservation (wave 1, re-architected).** The INDEPENDENT
    compiled target under the shipped lowering computes the Python reference
    on every expression and environment — including the `//`/`%` deviation,
    where the emitted floor-correction reaches Python floor semantics
    (`jsFdiv_eq_fdiv`/`jsFmod_eq_fmod`). Real structural induction, not `rfl`:
    the deviation arms need the arithmetic binding lemmas. -/
theorem preservationE (e : Exp) (env : Env) : evalEjs e env = evalE false e env := by
  induction e generalizing env with
  | lit n => rfl
  | var s => rfl
  | add a b iha ihb => simp only [evalEtgt, evalE, iha, ihb]
  | sub a b iha ihb => simp only [evalEtgt, evalE, iha, ihb]
  | mul a b iha ihb => simp only [evalEtgt, evalE, iha, ihb]
  | lt a b iha ihb => simp only [evalEtgt, evalE, iha, ihb]
  | fdiv a b iha ihb =>
    simp only [evalEtgt, evalE, iha, ihb]
    cases evalE false a env with
    | none => rfl
    | some x =>
      cases evalE false b env with
      | none => rfl
      | some y =>
        by_cases hy : y = 0
        · simp [hy]
        · simp [hy, jsELowering, jsFdiv_eq_fdiv x y hy]
  | fmod a b iha ihb =>
    simp only [evalEtgt, evalE, iha, ihb]
    cases evalE false a env with
    | none => rfl
    | some x =>
      cases evalE false b env with
      | none => rfl
      | some y =>
        by_cases hy : y = 0
        · simp [hy]
        · simp [hy, jsELowering, jsFmod_eq_fmod x y hy]

/-- The re-architected statement, in predicate form: the shipped lowering
    preserves. Same content as `preservationE`; this is the instantiation the
    stub litmus contrasts against. -/
theorem preservationE_real : EPreserves jsELowering := preservationE

/-- **Stub litmus (wave 1).** The SAME preservation predicate is FALSE for the
    naive truncating lowering: on the deviating program `-7 // 2` the stub
    computes JS-trunc `-3` where Python floors to `-4` — a concrete
    contradiction. This is what the old `evalE true = evalE false` statement
    could not express (both sides hardcoded the same arm). -/
theorem preservationE_stub_fails : ¬ EPreserves truncELowering := by
  intro h
  have hc := h (.fdiv (.lit (-7)) (.lit 2)) []
  -- hc reduces to `some (-3) = some (-4)`:
  -- LHS `Int.tdiv (-7) 2 = -3` (truncation), RHS `Int.fdiv (-7) 2 = -4` (floor).
  exact absurd hc (by decide)

-- The contrast, concretely (stub is a plausible naive emission, and it diverges):
#guard evalEjs (.fdiv (.lit (-7)) (.lit 2)) [] = some (-4)                 -- real: Python floor
#guard evalEtgt truncELowering (.fdiv (.lit (-7)) (.lit 2)) [] = some (-3) -- stub: JS trunc ✗
#guard evalEtgt truncELowering (.fmod (.lit (-7)) (.lit 2)) [] = some (-1) -- stub `%` also ✗ (CPython: 1)

/-- SPOT (through the theorem, not by evaluation): the deviating `x // 2` at
    `x = -7` — the INDEPENDENT compiled value is Python's floor `-4`, not
    JS-trunc `-3`, derived via `preservationE` (fails if the statement is
    weakened back to model-vs-model). Environment lookup exercised too. -/
example : evalEjs (.fdiv (.var "x") (.lit 2)) [("x", -7)] = some (-4) := by
  rw [preservationE]; rfl

/-- info: 'PythExpandVerify.preservationE' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationE

/-- info: 'PythExpandVerify.preservationE_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationE_real

/-- info: 'PythExpandVerify.preservationE_stub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationE_stub_fails

/-! ### Wave 2 (C1 rollout) — INDEPENDENT-target statement preservation

The previous `preservationS : evalS true s env = evalS false s env` (and its
`whileF` helper `preservationW`) was the same F1 model-vs-model tautology as
the old `preservationE`: a `Bool`-flagged copy of ONE evaluator, deviating
only in the `//`/`%` arm — stubbing the shipping lowering could not break it.
Re-architected on the same wave-1 recipe: `evalStgt` is a SEPARATE recursion,
parameterized by the integer-division lowering (sub-expressions via
`evalEtgt L`), TRUE for the shipped floor-correction (`preservationS`) and
provably FALSE for the naive truncating lowering
(`preservationS_stub_fails`). The wave-1 TEMPORARY glue
(`evalEtgt_js_eq_flag`/`evalE_flag_bridge`) had `preservationS`/
`preservationW` as its only consumers and is now DELETED — no theorem routes
through the legacy `Bool` flag anymore. -/

/-- **Independent target evaluator** for the `Stmt` fragment: the compiled
    program's statement semantics under lowering `L`. A SEPARATE recursion
    (not a `Bool` flag on `evalS`); sub-expressions are evaluated by the
    independent `evalEtgt L`, so the `//`/`%` lowering threads through
    assignments, `if` conditions, and `while` conditions/bodies exactly as in
    the emitted JS. -/
def evalStgt (L : IntDivLowering) : Stmt → Env → Option Env
  | .skip, env => some env
  | .assign s e, env => (evalEtgt L e env).map (fun v => env.set s v)
  | .seq a b, env => (evalStgt L a env).bind (fun env' => evalStgt L b env')
  | .ite c t e, env => (evalEtgt L c env).bind
      (fun v => if v ≠ 0 then evalStgt L t env else evalStgt L e env)
  | .whileF 0 _ _, _ => none
  | .whileF (f + 1) c body, env => (evalEtgt L c env).bind
      (fun v => if v ≠ 0 then (evalStgt L body env).bind
                  (fun env' => evalStgt L (.whileF f c body) env')
                else some env)

/-- The compiled statement semantics: the independent target under the
    SHIPPED lowering. -/
abbrev evalSjs : Stmt → Env → Option Env := evalStgt jsELowering

/-- Statement preservation as a predicate OVER the lowering — the SAME
    predicate is proved for the shipped lowering (`preservationS_real`) and
    REFUTED for the stub (`preservationS_stub_fails`). -/
def SPreserves (L : IntDivLowering) : Prop :=
  ∀ s env, evalStgt L s env = evalS false s env

-- F9 pin: the REFERENCE `evalS false` matches CPython on a deviating
-- STATEMENT (CPython: x = -7 // 2 → x == -4); the `-8`/`-3` demo/loop
-- reference pins are with `_tier3w2_demo`/`_tier3w2_loop` above.
#guard (evalS false (.assign "x" (.fdiv (.lit (-7)) (.lit 2))) []).bind
        (fun e => e.get "x") = some (-4)

/-- Fuel-indexed while-loop preservation for the INDEPENDENT target: given
    body preservation, the bounded loop preserves — induction on fuel (the
    loop's own recursion); the condition is handled by the genuine
    `preservationE`. -/
private theorem preservationW (f : Nat) (c : Exp) (body : Stmt)
    (hbody : ∀ env, evalSjs body env = evalS false body env) :
    ∀ env, evalSjs (.whileF f c body) env = evalS false (.whileF f c body) env := by
  induction f with
  | zero => intro env; simp only [evalStgt, evalS]
  | succ f ih =>
    intro env
    simp only [evalStgt, evalS, preservationE, hbody, ih]

/-- info: '_private.PythExpandVerify.0.PythExpandVerify.preservationW' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationW

/-- **Statement preservation (wave 2, re-architected).** The INDEPENDENT
    compiled statement semantics under the shipped lowering computes the same
    final environment as the Python reference — control flow and state
    preserved, the `//`/`%` deviation preserved throughout (assignment, `if`,
    `while`). Real structural induction (fuel induction for the loop), binding
    the independent target to `evalS false` — NOT a flag-vs-flag identity. -/
theorem preservationS (s : Stmt) (env : Env) : evalSjs s env = evalS false s env := by
  induction s generalizing env with
  | skip => simp only [evalStgt, evalS]
  | assign n e => simp only [evalStgt, evalS, preservationE]
  | seq a b iha ihb => simp only [evalStgt, evalS, iha, ihb]
  | ite c t e iht ihe => simp only [evalStgt, evalS, preservationE, iht, ihe]
  | whileF f c body ihbody => exact preservationW f c body ihbody env

/-- The re-architected statement, in predicate form: the shipped lowering
    preserves statements. Same content as `preservationS`; this is the
    instantiation the stub litmus contrasts against. -/
theorem preservationS_real : SPreserves jsELowering := preservationS

/-- **Stub litmus (wave 2).** The SAME preservation predicate is FALSE for
    the naive truncating lowering, on a deviating STATEMENT: after
    `x = -7 // 2` the stub's final environment holds JS-trunc `-3` where
    Python floors to `-4` — a concrete contradiction the old
    `evalS true = evalS false` statement could not express. -/
theorem preservationS_stub_fails : ¬ SPreserves truncELowering := by
  intro h
  have hc := h (.assign "x" (.fdiv (.lit (-7)) (.lit 2))) []
  -- after one evaluator step, hc reduces to `some [("x", -3)] = some [("x", -4)]`:
  -- LHS `Int.tdiv (-7) 2 = -3` (truncation), RHS `Int.fdiv (-7) 2 = -4` (floor).
  simp only [evalStgt, evalS] at hc
  exact absurd hc (by decide)

-- The contrast, concretely (the deviation lands in statement STATE):
#guard (evalSjs (.assign "x" (.fdiv (.lit (-7)) (.lit 2))) []).bind
        (fun e => e.get "x") = some (-4)                       -- real: Python floor
#guard (evalStgt truncELowering (.assign "x" (.fdiv (.lit (-7)) (.lit 2))) []).bind
        (fun e => e.get "x") = some (-3)                       -- stub: JS trunc ✗

/-- SPOT (through the theorem, not by evaluation): the `//` deviation
    preserved through assignment + sequencing — `x = -7 // 2; y = x * 2` ends
    with `y = -8` in the INDEPENDENT compiled semantics, derived via
    `preservationS` (fails if the statement is weakened back to
    model-vs-model). -/
example : (evalSjs _tier3w2_demo []).bind (fun e => e.get "y") = some (-8) := by
  rw [preservationS]; simp only [_tier3w2_demo, evalS, evalE]; rfl

/-- SPOT: control flow + state through `while` — the loop accumulating
    `(i-3)//2` ends with `s = -3` (Python floor throughout) in the
    INDEPENDENT compiled semantics, derived via `preservationS` (exercises
    the `preservationW` fuel induction). -/
example : (evalSjs _tier3w2_loop []).bind (fun e => e.get "s") = some (-3) := by
  rw [preservationS]; simp only [_tier3w2_loop, evalS, evalE]; rfl

/-- info: 'PythExpandVerify.preservationS' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationS

/-- info: 'PythExpandVerify.preservationS_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationS_real

/-- info: 'PythExpandVerify.preservationS_stub_fails' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms preservationS_stub_fails

/-! ## Tier-3 wave 4 — functions (definitions, calls, recursion, `return`)

Extends the fragment to first-order FUNCTIONS: a function table, calls, `return`
(short-circuiting), and recursion. A **uniform-fuel definitional interpreter**
(the Move-borrow-checker `run(fuel)` pattern) makes termination trivial: every
recursive call — arithmetic, statement, call, and loop unrolling — decrements one
fuel, so a single `termination_by fuel` decreases everywhere; "enough fuel" bounds
recursion/iteration depth. Reuses `Env`/`jsFdiv`.

Design (re-architected in the C1 rollout, wave 3): `evalFE false`/`evalFS false`
is the Python REFERENCE semantics. The compiled target is the INDEPENDENT mutual
evaluator triple `evalFEtgt`/`evalArgstgt`/`evalFStgt`, parameterized by the
integer-division lowering (wave-3 section below). The `tgt : Bool` flag on
`evalFE`/`evalArgs`/`evalFS` is retained only as the historical definition shape
— its `true` branch is LEGACY and carries NO theorem; every preservation
statement binds the independent target to `evalF* false`. -/

inductive FExp where
  | lit (n : Int)
  | var (s : String)
  | add (a b : FExp) | sub (a b : FExp) | mul (a b : FExp)
  | fdiv (a b : FExp) | fmod (a b : FExp) | lt (a b : FExp)
  | call (fn : String) (args : List FExp)
deriving Repr

inductive FStmt where
  | skip
  | assign (s : String) (e : FExp)
  | seq (a b : FStmt)
  | ite (c : FExp) (t e : FStmt)
  | whileB (c : FExp) (body : FStmt)
  | ret (e : FExp)
deriving Repr

structure Func where
  params : List String
  body : FStmt

abbrev FEnv := List (String × Func)

def FEnv.lookup (fenv : FEnv) (n : String) : Option Func :=
  (fenv.find? (fun p => p.1 == n)).map (·.2)

/-- Bind parameters to argument values. -/
def bindParams (params : List String) (vals : List Int) : Env := params.zip vals

/-- A statement result: updated environment + `some rv` if a `return` fired
    (short-circuiting the rest of the function body). -/
abbrev FRes := Env × Option Int

mutual
/-- Fuel-bounded expression eval. `tgt = false` = Python REFERENCE semantics
    (the only branch any theorem uses). The `tgt = true` branch is LEGACY (the
    former F1 model-vs-model flag) and is NOT the compiled target — the genuine
    compiled target is the INDEPENDENT `evalFEtgt (L : IntDivLowering)` below;
    no theorem references `evalFE true`. `fuel` decrements on every recursive
    call (so termination is by `fuel`); `none` on error / fuel-exhaustion. -/
def evalFE (fenv : FEnv) (tgt : Bool) : Nat → FExp → Env → Option Int
  | 0, _, _ => none
  | _ + 1, .lit n, _ => some n
  | _ + 1, .var s, env => env.get s
  | fuel + 1, .add a b, env => match evalFE fenv tgt fuel a env, evalFE fenv tgt fuel b env with
      | some x, some y => some (x + y) | _, _ => none
  | fuel + 1, .sub a b, env => match evalFE fenv tgt fuel a env, evalFE fenv tgt fuel b env with
      | some x, some y => some (x - y) | _, _ => none
  | fuel + 1, .mul a b, env => match evalFE fenv tgt fuel a env, evalFE fenv tgt fuel b env with
      | some x, some y => some (x * y) | _, _ => none
  | fuel + 1, .lt a b, env => match evalFE fenv tgt fuel a env, evalFE fenv tgt fuel b env with
      | some x, some y => some (if x < y then 1 else 0) | _, _ => none
  | fuel + 1, .fdiv a b, env => match evalFE fenv tgt fuel a env, evalFE fenv tgt fuel b env with
      | some x, some y => if y = 0 then none else some (if tgt then jsFdiv x y else Int.fdiv x y)
      | _, _ => none
  | fuel + 1, .fmod a b, env => match evalFE fenv tgt fuel a env, evalFE fenv tgt fuel b env with
      | some x, some y => if y = 0 then none else some (if tgt then jsFmod x y else Int.fmod x y)
      | _, _ => none
  | fuel + 1, .call fn args, env => match fenv.lookup fn with
      | none => none
      | some f => (evalArgs fenv tgt fuel args env).bind
          (fun vals => (evalFS fenv tgt fuel f.body (bindParams f.params vals)).bind (·.2))
termination_by n _ _ => n
/-- Eval an argument list (left to right). -/
def evalArgs (fenv : FEnv) (tgt : Bool) : Nat → List FExp → Env → Option (List Int)
  | 0, _, _ => none
  | _ + 1, [], _ => some []
  | fuel + 1, a :: as, env => match evalFE fenv tgt fuel a env, evalArgs fenv tgt fuel as env with
      | some v, some vs => some (v :: vs) | _, _ => none
termination_by n _ _ => n
/-- Fuel-bounded statement eval → `(env, return?)`. `return` short-circuits
    through `seq`/`while`; `if`/`while` branch on Python truthiness (`≠ 0`). -/
def evalFS (fenv : FEnv) (tgt : Bool) : Nat → FStmt → Env → Option FRes
  | 0, _, _ => none
  | _ + 1, .skip, env => some (env, none)
  | fuel + 1, .assign s e, env => (evalFE fenv tgt fuel e env).map (fun v => (env.set s v, none))
  | fuel + 1, .ret e, env => (evalFE fenv tgt fuel e env).map (fun v => (env, some v))
  | fuel + 1, .seq a b, env => (evalFS fenv tgt fuel a env).bind
      (fun r => match r.2 with | some rv => some (r.1, some rv) | none => evalFS fenv tgt fuel b r.1)
  | fuel + 1, .ite c t e, env => (evalFE fenv tgt fuel c env).bind
      (fun v => if v ≠ 0 then evalFS fenv tgt fuel t env else evalFS fenv tgt fuel e env)
  | fuel + 1, .whileB c body, env => (evalFE fenv tgt fuel c env).bind
      (fun v => if v ≠ 0 then (evalFS fenv tgt fuel body env).bind
          (fun r => match r.2 with
            | some rv => some (r.1, some rv)
            | none => evalFS fenv tgt fuel (.whileB c body) r.1)
        else some (env, none))
termination_by fuel _ _ => fuel
end

-- executable bindings: functions, calls, recursion, the deviation preserved.
-- `dbl(x) = x*2`;  `dbl(-7//2) = dbl(-4) = -8`.
def _tier3w4_dbl : FEnv := [("dbl", ⟨["x"], .ret (.mul (.var "x") (.lit 2))⟩)]
-- F9 reference pin (CPython: dbl(-7//2) == dbl(-4) == -8); the compiled-side
-- guard lives with the independent `evalFEjs` in the wave-3 section below.
#guard (evalFE _tier3w4_dbl false 50 (.call "dbl" [.fdiv (.lit (-7)) (.lit 2)]) []) = some (-8)
-- RECURSION: `sumto(n) = if n<1 then 0 else n + sumto(n-1)`;  sumto(5) = 15.
def _tier3w4_rec : FEnv :=
  [("sumto", ⟨["n"], .ite (.lt (.var "n") (.lit 1)) (.ret (.lit 0))
      (.ret (.add (.var "n") (.call "sumto" [.sub (.var "n") (.lit 1)])))⟩)]
-- F9 reference pin (CPython: sumto(5) == 15); compiled-side guard with `evalFEjs` below.
#guard (evalFE _tier3w4_rec false 100 (.call "sumto" [.lit 5]) []) = some 15

/-! ### Wave 3 (C1 rollout) — INDEPENDENT-target function preservation

The previous `preservationFE`/`preservationFS` (`evalFE true = evalFE false` /
`evalFS true = evalFS false`, via the mutual `preservationF'`) were the same F1
model-vs-model tautology as the old `preservationE`: a `Bool`-flagged copy of
ONE mutual evaluator triple, deviating only in the `//`/`%` arms — stubbing the
shipping lowering could not break them. Re-architected on the wave-1/2 recipe:
`evalFEtgt`/`evalArgstgt`/`evalFStgt` are a SEPARATE mutual recursion,
parameterized by the integer-division lowering, so the `//`/`%` lowering
threads through ARGUMENTS, function BODIES, RECURSION, and `return` exactly as
in the emitted JS. The same predicates (`FEPreserves`/`FSPreserves`) are TRUE
for the shipped floor-correction (`preservationFE`/`preservationFS`) and
provably FALSE for the naive truncating lowering
(`preservationFE_stub_fails`/`preservationFS_stub_fails`). -/

mutual
/-- **Independent target evaluator** (function-fragment expressions): the
    compiled program's semantics under lowering `L`. A SEPARATE mutual
    recursion (not the `Bool` flag on `evalFE`); the `//`/`%` arms call the
    lowering's operations, and a `.call` routes its arguments AND the callee
    body through the same lowering, exactly as in the emitted JS. -/
def evalFEtgt (fenv : FEnv) (L : IntDivLowering) : Nat → FExp → Env → Option Int
  | 0, _, _ => none
  | _ + 1, .lit n, _ => some n
  | _ + 1, .var s, env => env.get s
  | fuel + 1, .add a b, env => match evalFEtgt fenv L fuel a env, evalFEtgt fenv L fuel b env with
      | some x, some y => some (x + y) | _, _ => none
  | fuel + 1, .sub a b, env => match evalFEtgt fenv L fuel a env, evalFEtgt fenv L fuel b env with
      | some x, some y => some (x - y) | _, _ => none
  | fuel + 1, .mul a b, env => match evalFEtgt fenv L fuel a env, evalFEtgt fenv L fuel b env with
      | some x, some y => some (x * y) | _, _ => none
  | fuel + 1, .lt a b, env => match evalFEtgt fenv L fuel a env, evalFEtgt fenv L fuel b env with
      | some x, some y => some (if x < y then 1 else 0) | _, _ => none
  | fuel + 1, .fdiv a b, env => match evalFEtgt fenv L fuel a env, evalFEtgt fenv L fuel b env with
      | some x, some y => if y = 0 then none else some (L.fdiv x y)
      | _, _ => none
  | fuel + 1, .fmod a b, env => match evalFEtgt fenv L fuel a env, evalFEtgt fenv L fuel b env with
      | some x, some y => if y = 0 then none else some (L.fmod x y)
      | _, _ => none
  | fuel + 1, .call fn args, env => match fenv.lookup fn with
      | none => none
      | some f => (evalArgstgt fenv L fuel args env).bind
          (fun vals => (evalFStgt fenv L fuel f.body (bindParams f.params vals)).bind (·.2))
termination_by n _ _ => n
/-- Independent-target argument-list eval (left to right). -/
def evalArgstgt (fenv : FEnv) (L : IntDivLowering) : Nat → List FExp → Env → Option (List Int)
  | 0, _, _ => none
  | _ + 1, [], _ => some []
  | fuel + 1, a :: as, env => match evalFEtgt fenv L fuel a env, evalArgstgt fenv L fuel as env with
      | some v, some vs => some (v :: vs) | _, _ => none
termination_by n _ _ => n
/-- Independent-target statement eval → `(env, return?)`. `return`
    short-circuits through `seq`/`while`; `if`/`while` branch on Python
    truthiness (`≠ 0`); sub-expressions via `evalFEtgt L`. -/
def evalFStgt (fenv : FEnv) (L : IntDivLowering) : Nat → FStmt → Env → Option FRes
  | 0, _, _ => none
  | _ + 1, .skip, env => some (env, none)
  | fuel + 1, .assign s e, env => (evalFEtgt fenv L fuel e env).map (fun v => (env.set s v, none))
  | fuel + 1, .ret e, env => (evalFEtgt fenv L fuel e env).map (fun v => (env, some v))
  | fuel + 1, .seq a b, env => (evalFStgt fenv L fuel a env).bind
      (fun r => match r.2 with | some rv => some (r.1, some rv) | none => evalFStgt fenv L fuel b r.1)
  | fuel + 1, .ite c t e, env => (evalFEtgt fenv L fuel c env).bind
      (fun v => if v ≠ 0 then evalFStgt fenv L fuel t env else evalFStgt fenv L fuel e env)
  | fuel + 1, .whileB c body, env => (evalFEtgt fenv L fuel c env).bind
      (fun v => if v ≠ 0 then (evalFStgt fenv L fuel body env).bind
          (fun r => match r.2 with
            | some rv => some (r.1, some rv)
            | none => evalFStgt fenv L fuel (.whileB c body) r.1)
        else some (env, none))
termination_by fuel _ _ => fuel
end

/-- The compiled function-fragment expression semantics: the independent
    target under the SHIPPED lowering. -/
abbrev evalFEjs (fenv : FEnv) : Nat → FExp → Env → Option Int :=
  evalFEtgt fenv jsELowering

/-- The compiled function-fragment statement semantics: the independent
    target under the SHIPPED lowering. -/
abbrev evalFSjs (fenv : FEnv) : Nat → FStmt → Env → Option FRes :=
  evalFStgt fenv jsELowering

/-- Function-fragment EXPRESSION preservation as a predicate OVER the lowering
    — the SAME predicate is proved for the shipped lowering
    (`preservationFE_real`) and REFUTED for the stub
    (`preservationFE_stub_fails`). Quantifies over the function table and the
    fuel too: a lowering preserves only if it does so under EVERY function
    table at EVERY recursion depth. -/
def FEPreserves (L : IntDivLowering) : Prop :=
  ∀ fenv fuel e env, evalFEtgt fenv L fuel e env = evalFE fenv false fuel e env

/-- Function-fragment STATEMENT preservation as a predicate OVER the lowering
    (`preservationFS_real` vs `preservationFS_stub_fails`). -/
def FSPreserves (L : IntDivLowering) : Prop :=
  ∀ fenv fuel s env, evalFStgt fenv L fuel s env = evalFS fenv false fuel s env

-- Compiled-side guards (the retired `evalFE true` guards, now on the genuine
-- independent target): call + deviation, and recursion.
#guard (evalFEjs _tier3w4_dbl 50 (.call "dbl" [.fdiv (.lit (-7)) (.lit 2)]) []) = some (-8)
#guard (evalFEjs _tier3w4_rec 100 (.call "sumto" [.lit 5]) []) = some 15

/-- Combined preservation worker for the three mutual evaluators: ONE induction
    on `fuel` carries all three statements together (they are mutually dependent
    exactly as the evaluators are — uniform fuel means every recursive call,
    including the `while` unrolling and the `.call` body, sits at the
    predecessor, so the three IHs at `fuel` discharge every recursion site).
    Binds the INDEPENDENT target under the shipped lowering to the Python
    reference `evalF* false`; the `.fdiv`/`.fmod` arms are closed by
    `jsFdiv_eq_fdiv`/`jsFmod_eq_fmod` on the `y ≠ 0` branch — real induction,
    not a flag-vs-flag identity. -/
private theorem preservationF' (fenv : FEnv) (fuel : Nat) :
    (∀ e env, evalFEtgt fenv jsELowering fuel e env = evalFE fenv false fuel e env) ∧
    (∀ args env, evalArgstgt fenv jsELowering fuel args env = evalArgs fenv false fuel args env) ∧
    (∀ s env, evalFStgt fenv jsELowering fuel s env = evalFS fenv false fuel s env) := by
  induction fuel with
  | zero =>
    exact ⟨fun e env => by simp only [evalFEtgt, evalFE],
           fun args env => by simp only [evalArgstgt, evalArgs],
           fun s env => by simp only [evalFStgt, evalFS]⟩
  | succ fuel ih =>
    obtain ⟨ihE, ihA, ihS⟩ := ih
    refine ⟨fun e env => ?_, fun args env => ?_, fun s env => ?_⟩
    · cases e with
      | lit n => simp only [evalFEtgt, evalFE]
      | var s => simp only [evalFEtgt, evalFE]
      | add a b => simp only [evalFEtgt, evalFE, ihE]
      | sub a b => simp only [evalFEtgt, evalFE, ihE]
      | mul a b => simp only [evalFEtgt, evalFE, ihE]
      | lt a b => simp only [evalFEtgt, evalFE, ihE]
      | fdiv a b =>
        simp only [evalFEtgt, evalFE, ihE]
        cases evalFE fenv false fuel a env with
        | none => rfl
        | some x =>
          cases evalFE fenv false fuel b env with
          | none => rfl
          | some y =>
            by_cases hy : y = 0
            · simp [hy]
            · simp [hy, jsELowering, jsFdiv_eq_fdiv x y hy]
      | fmod a b =>
        simp only [evalFEtgt, evalFE, ihE]
        cases evalFE fenv false fuel a env with
        | none => rfl
        | some x =>
          cases evalFE fenv false fuel b env with
          | none => rfl
          | some y =>
            by_cases hy : y = 0
            · simp [hy]
            · simp [hy, jsELowering, jsFmod_eq_fmod x y hy]
      | call fn args => simp only [evalFEtgt, evalFE, ihA, ihS]
    · cases args with
      | nil => simp only [evalArgstgt, evalArgs]
      | cons a as => simp only [evalArgstgt, evalArgs, ihE, ihA]
    · cases s with
      | skip => simp only [evalFStgt, evalFS]
      | assign n e => simp only [evalFStgt, evalFS, ihE]
      | seq a b => simp only [evalFStgt, evalFS, ihS]
      | ite c t e => simp only [evalFStgt, evalFS, ihE, ihS]
      | whileB c body => simp only [evalFStgt, evalFS, ihE, ihS]
      | ret e => simp only [evalFStgt, evalFS, ihE]

/-- **Expression + call preservation (functions, wave 3, re-architected).**
    Under any function table, at any fuel, the INDEPENDENT compiled target
    under the shipped lowering computes the Python reference on every
    function-fragment expression — the `//`/`%` deviation preserved through
    arguments, callee bodies, and recursion. -/
theorem preservationFE (fenv : FEnv) (fuel : Nat) (e : FExp) (env : Env) :
    evalFEjs fenv fuel e env = evalFE fenv false fuel e env :=
  (preservationF' fenv fuel).1 e env

/-- **Statement preservation (functions, Tier-3 wave 4 / C1-rollout wave 3).**
    Under any function table, the INDEPENDENT compiled program (with calls,
    recursion, `return`) computes the same result + environment as the Python
    reference. -/
theorem preservationFS (fenv : FEnv) (fuel : Nat) (s : FStmt) (env : Env) :
    evalFSjs fenv fuel s env = evalFS fenv false fuel s env :=
  (preservationF' fenv fuel).2.2 s env

/-- The re-architected statement, in predicate form: the shipped lowering
    preserves function-fragment expressions. Same content as `preservationFE`;
    this is the instantiation the stub litmus contrasts against. -/
theorem preservationFE_real : FEPreserves jsELowering :=
  fun fenv fuel e env => preservationFE fenv fuel e env

/-- Predicate form for statements: the instantiation `preservationFS_stub_fails`
    contrasts against. -/
theorem preservationFS_real : FSPreserves jsELowering :=
  fun fenv fuel s env => preservationFS fenv fuel s env

-- Deviating function table for the stub litmus: `half(n) = return n // 2` —
-- the deviation sits INSIDE a function body, reached only through a call.
def _tier3w4_half : FEnv := [("half", ⟨["n"], .ret (.fdiv (.var "n") (.lit 2))⟩)]
-- F9 reference pins: `evalFE false`/`evalFS false` match CPython
-- (half(-7) == -7 // 2 == -4, Python floor), incl. through a statement.
#guard (evalFE _tier3w4_half false 10 (.call "half" [.lit (-7)]) []) = some (-4)
#guard (evalFS _tier3w4_half false 10 (.ret (.call "half" [.lit (-7)])) []) = some ([], some (-4))

/-- **Stub litmus (wave 3, expression side).** The SAME preservation predicate
    is FALSE for the naive truncating lowering, on a deviating program whose
    `//` sits inside a CALLED function body: `half(-7)` computes JS-trunc `-3`
    under the stub where Python floors to `-4` — a concrete contradiction the
    old `evalFE true = evalFE false` statement could not express. -/
theorem preservationFE_stub_fails : ¬ FEPreserves truncELowering := by
  intro h
  have hc := h _tier3w4_half 10 (.call "half" [.lit (-7)]) []
  -- The evaluators are fuel-recursive (not kernel-reducible): step them via
  -- their equation lemmas; the residue is ground and `decide` closes it.
  simp [_tier3w4_half, evalFEtgt, evalArgstgt, evalFStgt, evalFE, evalArgs, evalFS,
        FEnv.lookup, bindParams, truncELowering] at hc
  exact absurd hc (by decide)

/-- **Stub litmus (wave 3, statement side).** The SAME statement-preservation
    predicate is FALSE for the truncating lowering, on a deviating function
    STATEMENT: `return half(-7)` returns stub JS-trunc `-3` where Python
    floors to `-4`. -/
theorem preservationFS_stub_fails : ¬ FSPreserves truncELowering := by
  intro h
  have hc := h _tier3w4_half 10 (.ret (.call "half" [.lit (-7)])) []
  simp [_tier3w4_half, evalFEtgt, evalArgstgt, evalFStgt, evalFE, evalArgs, evalFS,
        FEnv.lookup, bindParams, truncELowering] at hc
  exact absurd hc (by decide)

-- The contrast, concretely (the deviation lands in a CALL result):
#guard (evalFEjs _tier3w4_half 10 (.call "half" [.lit (-7)]) []) = some (-4)                 -- real: floor
#guard (evalFEtgt _tier3w4_half truncELowering 10 (.call "half" [.lit (-7)]) []) = some (-3) -- stub ✗
#guard (evalFStgt _tier3w4_half truncELowering 10 (.ret (.call "half" [.lit (-7)])) [])
        = some ([], some (-3))                                                               -- stub stmt ✗

/-- SPOT (through the theorem, not by evaluation): the `//` deviation flows
    through a function CALL and is preserved — `dbl(-7 // 2) = dbl(-4) = -8`
    in the INDEPENDENT compiled semantics, derived via `preservationFE` (fails
    if the statement is weakened back to model-vs-model). -/
example : evalFEjs _tier3w4_dbl 50 (.call "dbl" [.fdiv (.lit (-7)) (.lit 2)]) [] = some (-8) := by
  rw [preservationFE]
  simp [_tier3w4_dbl, evalFE, evalArgs, evalFS, FEnv.lookup, bindParams]
  rfl

/-- SPOT: call + deviation through a STATEMENT — `y = dbl(-7 // 2)` ends with
    `y = -8` in the INDEPENDENT compiled semantics, derived via
    `preservationFS` (exercises assignment-from-call + the argument lowering). -/
example : (evalFSjs _tier3w4_dbl 50 (.assign "y" (.call "dbl" [.fdiv (.lit (-7)) (.lit 2)])) []).bind
    (fun r => r.1.get "y") = some (-8) := by
  rw [preservationFS]
  simp [_tier3w4_dbl, evalFE, evalArgs, evalFS, FEnv.lookup, bindParams]
  rfl

/-- info: '_private.PythExpandVerify.0.PythExpandVerify.preservationF'' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationF'

/-- info: 'PythExpandVerify.preservationFE' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationFE

/-- info: 'PythExpandVerify.preservationFS' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationFS

/-- info: 'PythExpandVerify.preservationFE_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationFE_real

/-- info: 'PythExpandVerify.preservationFS_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationFS_real

/-- info: 'PythExpandVerify.preservationFE_stub_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationFE_stub_fails

/-- info: 'PythExpandVerify.preservationFS_stub_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationFS_stub_fails

/-! ## Tier-3 wave 5 — collections (list values, indexing, len): unifies with the verified core

The last major language gap: values become `Val` (`vint` | `vlist`), and
expressions build lists, take their length, and INDEX them. Indexing reuses the
Tier-2 `getIndex` (negative-index normalization, `xs[-1]` → last), so the
verified core's `getIndex_inbounds` is the safety lemma that certifies every
compiled index is in-bounds — this wave literally ties the Tier-3 preservation
to the Tier-1/2 value model.

Design (re-architected in the C1 rollout, wave 4): `evalC false`/`evalCs false`
is the Python REFERENCE semantics. The compiled target is the INDEPENDENT
mutual evaluator pair `evalCtgt`/`evalCstgt`, parameterized by the integer-
division lowering the emitted JS uses (wave-4 section below). The `tgt : Bool`
flag on `evalC`/`evalCs` is retained only as the historical definition shape —
its `true` branch is LEGACY and carries NO theorem; every preservation
statement binds the independent target to `evalC*/evalCs* false`. Indexing
reuses the verified-core `getIndex` on BOTH sides (list/index/len compile
structurally); the deviation threads through list ELEMENTS. -/

inductive Val where
  | vint (n : Int)
  | vlist (xs : List Val)
deriving Repr

/-- Project a `Val` to its int (for `#guard`s; avoids a nested `DecidableEq`). -/
def Val.asInt : Val → Option Int
  | .vint n => some n
  | .vlist _ => none

abbrev VEnv := List (String × Val)

def VEnv.get (env : VEnv) (n : String) : Option Val :=
  (env.find? (fun p => p.1 == n)).map (·.2)

inductive CExp where
  | lit (n : Int)
  | var (s : String)
  | add (a b : CExp) | sub (a b : CExp) | mul (a b : CExp) | fdiv (a b : CExp)
  | listE (elts : List CExp)
  | index (lst idx : CExp)   -- Python indexing (getIndex normalization)
  | len (e : CExp)
deriving Repr

mutual
/-- Collection-valued expression eval. `tgt = false` = Python REFERENCE
    semantics (the only branch any theorem uses). The `tgt = true` branch is
    LEGACY (the former F1 model-vs-model flag) and is NOT the compiled target —
    the genuine compiled target is the INDEPENDENT `evalCtgt (L : IntDivLowering)`
    below; no theorem references `evalC true`. `index` reuses the verified-core
    `getIndex` (so `xs[-1]` normalizes and OOB → `none`). -/
def evalC (tgt : Bool) : CExp → VEnv → Option Val
  | .lit n, _ => some (.vint n)
  | .var s, env => env.get s
  | .add a b, env => match evalC tgt a env, evalC tgt b env with
      | some (.vint x), some (.vint y) => some (.vint (x + y)) | _, _ => none
  | .sub a b, env => match evalC tgt a env, evalC tgt b env with
      | some (.vint x), some (.vint y) => some (.vint (x - y)) | _, _ => none
  | .mul a b, env => match evalC tgt a env, evalC tgt b env with
      | some (.vint x), some (.vint y) => some (.vint (x * y)) | _, _ => none
  | .fdiv a b, env => match evalC tgt a env, evalC tgt b env with
      | some (.vint x), some (.vint y) =>
          if y = 0 then none else some (.vint (if tgt then jsFdiv x y else Int.fdiv x y))
      | _, _ => none
  | .listE elts, env => (evalCs tgt elts env).map .vlist
  | .index lst idx, env => match evalC tgt lst env, evalC tgt idx env with
      | some (.vlist xs), some (.vint i) => (getIndex xs.length i).bind (fun j => xs[j.toNat]?)
      | _, _ => none
  | .len e, env => match evalC tgt e env with
      | some (.vlist xs) => some (.vint xs.length) | _ => none
termination_by e _ => sizeOf e
/-- Eval a list of element expressions (for `listE`). -/
def evalCs (tgt : Bool) : List CExp → VEnv → Option (List Val)
  | [], _ => some []
  | e :: es, env => match evalC tgt e env, evalCs tgt es env with
      | some v, some vs => some (v :: vs) | _, _ => none
termination_by es _ => sizeOf es
end

-- F9 reference pins (via Val.asInt): the REFERENCE `evalC false` matches
-- CPython on list values, negative-index normalization, OOB, `len`, and the
-- deviation through a list ELEMENT
-- (CPython: [1,2,3][-1]==3; len([1,2,3])==3; [1,2][5]→IndexError; [-7//2,5][0]==-4).
-- The compiled-side guards (the retired `evalC true` guards) live with the
-- independent `evalCjs` in the wave-4 section below.
#guard ((evalC false (.index (.listE [.lit 1, .lit 2, .lit 3]) (.lit (-1))) []).bind Val.asInt) = some 3
#guard ((evalC false (.index (.listE [.lit 10, .lit 20]) (.lit 0)) []).bind Val.asInt) = some 10
#guard ((evalC false (.len (.listE [.lit 1, .lit 2, .lit 3])) []).bind Val.asInt) = some 3
#guard ((evalC false (.index (.listE [.lit 1, .lit 2]) (.lit 5)) []).bind Val.asInt) = none        -- OOB
#guard ((evalC false (.index (.listE [.fdiv (.lit (-7)) (.lit 2), .lit 5]) (.lit 0)) []).bind Val.asInt) = some (-4)

/-! ### Wave 4 (C1 rollout) — INDEPENDENT-target collection preservation

The previous `preservationC : evalC true e env = evalC false e env` (with its
mutual workers `preservationC'`/`preservationCs'`) was the same F1
model-vs-model tautology as the old `preservationE`: a `Bool`-flagged copy of
ONE mutual evaluator pair, deviating only in the `//` arm — stubbing the
shipping lowering could not break it. Re-architected on the wave-1/2/3 recipe:
`evalCtgt`/`evalCstgt` are a SEPARATE mutual recursion, parameterized by the
integer-division lowering, so `//` threads through list ELEMENTS, index
operands, and `len` arguments exactly as in the emitted JS; `index` still
reuses the verified-core `getIndex` on both sides. The same predicates
(`CPreserves`/`CsPreserves`) are TRUE for the shipped floor-correction
(`preservationC`/`preservationCs`) and provably FALSE for the naive
truncating lowering (`preservationC_stub_fails`/`preservationCs_stub_fails`). -/

mutual
/-- **Independent target evaluator** (collection expressions): the compiled
    program's semantics under lowering `L`. A SEPARATE mutual recursion (not
    the `Bool` flag on `evalC`); the `//` arm calls the lowering's operation,
    and a list literal routes every ELEMENT through the same lowering, exactly
    as in the emitted JS. `index` uses the verified-core `getIndex`. -/
def evalCtgt (L : IntDivLowering) : CExp → VEnv → Option Val
  | .lit n, _ => some (.vint n)
  | .var s, env => env.get s
  | .add a b, env => match evalCtgt L a env, evalCtgt L b env with
      | some (.vint x), some (.vint y) => some (.vint (x + y)) | _, _ => none
  | .sub a b, env => match evalCtgt L a env, evalCtgt L b env with
      | some (.vint x), some (.vint y) => some (.vint (x - y)) | _, _ => none
  | .mul a b, env => match evalCtgt L a env, evalCtgt L b env with
      | some (.vint x), some (.vint y) => some (.vint (x * y)) | _, _ => none
  | .fdiv a b, env => match evalCtgt L a env, evalCtgt L b env with
      | some (.vint x), some (.vint y) =>
          if y = 0 then none else some (.vint (L.fdiv x y))
      | _, _ => none
  | .listE elts, env => (evalCstgt L elts env).map .vlist
  | .index lst idx, env => match evalCtgt L lst env, evalCtgt L idx env with
      | some (.vlist xs), some (.vint i) => (getIndex xs.length i).bind (fun j => xs[j.toNat]?)
      | _, _ => none
  | .len e, env => match evalCtgt L e env with
      | some (.vlist xs) => some (.vint xs.length) | _ => none
termination_by e _ => sizeOf e
/-- Independent-target element-list eval (for `listE`): every element routes
    through the same lowering `L`. -/
def evalCstgt (L : IntDivLowering) : List CExp → VEnv → Option (List Val)
  | [], _ => some []
  | e :: es, env => match evalCtgt L e env, evalCstgt L es env with
      | some v, some vs => some (v :: vs) | _, _ => none
termination_by es _ => sizeOf es
end

/-- The compiled collection-expression semantics: the independent target under
    the SHIPPED lowering. -/
abbrev evalCjs : CExp → VEnv → Option Val := evalCtgt jsELowering

/-- The compiled element-list semantics: the independent target under the
    SHIPPED lowering. -/
abbrev evalCsjs : List CExp → VEnv → Option (List Val) := evalCstgt jsELowering

/-- Collection-expression preservation as a predicate OVER the lowering — the
    SAME predicate is proved for the shipped lowering (`preservationC_real`)
    and REFUTED for the stub (`preservationC_stub_fails`). -/
def CPreserves (L : IntDivLowering) : Prop :=
  ∀ e env, evalCtgt L e env = evalC false e env

/-- Element-list preservation as a predicate OVER the lowering
    (`preservationCs_real` vs `preservationCs_stub_fails`). -/
def CsPreserves (L : IntDivLowering) : Prop :=
  ∀ es env, evalCstgt L es env = evalCs false es env

-- Compiled-side guards (the retired `evalC true` guards, now on the genuine
-- independent target): indexing, and the deviation through a list ELEMENT.
#guard ((evalCjs (.index (.listE [.lit 10, .lit 20]) (.lit 0)) []).bind Val.asInt) = some 10
#guard ((evalCjs (.index (.listE [.fdiv (.lit (-7)) (.lit 2), .lit 5]) (.lit 0)) []).bind Val.asInt) = some (-4)

mutual
/-- Expression-side collection-preservation worker (mutual with
    `preservationCs'tgt`): binds the INDEPENDENT target under the shipped
    lowering to the Python reference `evalC false` — real mutual structural
    induction, not a flag-vs-flag identity. The `.fdiv` arm is closed by
    `jsFdiv_eq_fdiv` on the `y ≠ 0` branch. -/
private theorem preservationC'tgt (e : CExp) (env : VEnv) :
    evalCtgt jsELowering e env = evalC false e env := by
  match e with
  | .lit n => simp only [evalCtgt, evalC]
  | .var s => simp only [evalCtgt, evalC]
  | .add a b => simp only [evalCtgt, evalC, preservationC'tgt a env, preservationC'tgt b env]
  | .sub a b => simp only [evalCtgt, evalC, preservationC'tgt a env, preservationC'tgt b env]
  | .mul a b => simp only [evalCtgt, evalC, preservationC'tgt a env, preservationC'tgt b env]
  | .fdiv a b =>
    simp only [evalCtgt, evalC, preservationC'tgt a env, preservationC'tgt b env]
    cases evalC false a env with
    | none => rfl
    | some va =>
      cases evalC false b env with
      | none => cases va <;> rfl
      | some vb =>
        cases va with
        | vlist _ => rfl
        | vint x =>
          cases vb with
          | vlist _ => rfl
          | vint y =>
            by_cases hy : y = 0
            · simp [hy]
            · simp [hy, jsELowering, jsFdiv_eq_fdiv x y hy]
  | .listE elts => simp only [evalCtgt, evalC, preservationCs'tgt elts env]
  | .index lst idx =>
    simp only [evalCtgt, evalC, preservationC'tgt lst env, preservationC'tgt idx env]
  | .len e1 => simp only [evalCtgt, evalC, preservationC'tgt e1 env]
termination_by sizeOf e
decreasing_by all_goals (simp_wf <;> omega)

/-- Element-list preservation worker (mutual with `preservationC'tgt`). -/
private theorem preservationCs'tgt (es : List CExp) (env : VEnv) :
    evalCstgt jsELowering es env = evalCs false es env := by
  match es with
  | [] => simp only [evalCstgt, evalCs]
  | e :: rest => simp only [evalCstgt, evalCs, preservationC'tgt e env, preservationCs'tgt rest env]
termination_by sizeOf es
decreasing_by all_goals (simp_wf <;> omega)
end

/-- **Collection preservation (Tier-3 wave 5 / C1-rollout wave 4,
    re-architected).** The INDEPENDENT compiled target under the shipped
    lowering computes the Python reference on every collection expression and
    environment — list values first-class, `len`, verified-core `getIndex`
    indexing, and the `//` deviation threaded through list ELEMENTS. Real
    mutual structural induction binding the independent target to
    `evalC false` — NOT a flag-vs-flag identity. -/
theorem preservationC (e : CExp) (env : VEnv) : evalCjs e env = evalC false e env :=
  preservationC'tgt e env

/-- Element-list analogue: the compiled evaluation of every element list
    matches the Python reference (the `evalCs` side of the mutual pair). -/
theorem preservationCs (es : List CExp) (env : VEnv) :
    evalCsjs es env = evalCs false es env :=
  preservationCs'tgt es env

/-- The re-architected statement, in predicate form: the shipped lowering
    preserves collection expressions. Same content as `preservationC`; this
    is the instantiation the stub litmus contrasts against. -/
theorem preservationC_real : CPreserves jsELowering := preservationC

/-- Predicate form for element lists: the instantiation
    `preservationCs_stub_fails` contrasts against. -/
theorem preservationCs_real : CsPreserves jsELowering := preservationCs

/-- **Stub litmus (wave 4, expression side).** The SAME preservation predicate
    is FALSE for the naive truncating lowering, on a deviating COLLECTION
    program: `[-7 // 2, 5][0]` builds a list whose first element the stub
    computes as JS-trunc `-3` and reads it back out by indexing, where Python
    floors to `-4` — a concrete contradiction the old
    `evalC true = evalC false` statement could not express. -/
theorem preservationC_stub_fails : ¬ CPreserves truncELowering := by
  intro h
  have hc := h (.index (.listE [.fdiv (.lit (-7)) (.lit 2), .lit 5]) (.lit 0)) []
  -- The evaluators are wf-recursive (not kernel-reducible): step them via
  -- their equation lemmas; hc reduces through `some (.vint (-3)) = some (.vint (-4))`
  -- (stub `Int.tdiv (-7) 2 = -3` vs Python `Int.fdiv (-7) 2 = -4`) to `False`.
  simp [evalCtgt, evalCstgt, evalC, evalCs, truncELowering, getIndex] at hc

/-- **Stub litmus (wave 4, element-list side).** The SAME element-list
    predicate is FALSE for the truncating lowering: evaluating the element
    list `[-7 // 2]` yields stub `[-3]` where Python yields `[-4]`. -/
theorem preservationCs_stub_fails : ¬ CsPreserves truncELowering := by
  intro h
  have hc := h [.fdiv (.lit (-7)) (.lit 2)] []
  -- hc reduces through `some [.vint (-3)] = some [.vint (-4)]` to `False`.
  simp [evalCtgt, evalCstgt, evalC, evalCs, truncELowering] at hc

-- The contrast, concretely (the deviation lands in a list ELEMENT and is
-- read back out through `getIndex` indexing):
#guard ((evalCjs (.index (.listE [.fdiv (.lit (-7)) (.lit 2), .lit 5]) (.lit 0)) []).bind
        Val.asInt) = some (-4)                                  -- real: Python floor
#guard ((evalCtgt truncELowering (.index (.listE [.fdiv (.lit (-7)) (.lit 2), .lit 5]) (.lit 0)) []).bind
        Val.asInt) = some (-3)                                  -- stub: JS trunc ✗

/-- SPOT (through the theorem, not by evaluation): a concrete list-indexing
    program (`[1,2,3][-1]`) — the INDEPENDENT compiled result is rewritten to
    the Python reference via `preservationC` (fails if the statement is
    weakened back to model-vs-model), then evaluated to the Python answer 3. -/
example :
    (evalCjs (.index (.listE [.lit 1, .lit 2, .lit 3]) (.lit (-1))) []).bind Val.asInt
      = some 3 := by
  rw [preservationC]
  simp [evalC, evalCs, getIndex, Val.asInt]

/-- SPOT: a DEVIATING collection — `[-7 // 2, 5][0]` — the INDEPENDENT
    compiled value is Python's floor `-4` (not JS-trunc `-3`), derived via
    `preservationC` (exercises the deviation through a list element + the
    verified-core `getIndex` read-back). -/
example :
    (evalCjs (.index (.listE [.fdiv (.lit (-7)) (.lit 2), .lit 5]) (.lit 0)) []).bind Val.asInt
      = some (-4) := by
  rw [preservationC]
  simp [evalC, evalCs, getIndex, Val.asInt]

/-- info: '_private.PythExpandVerify.0.PythExpandVerify.preservationC'tgt' depends on axioms: [propext,
 Classical.choice,
 Quot.sound] -/
#guard_msgs in
#print axioms preservationC'tgt

/-- info: '_private.PythExpandVerify.0.PythExpandVerify.preservationCs'tgt' depends on axioms: [propext,
 Classical.choice,
 Quot.sound] -/
#guard_msgs in
#print axioms preservationCs'tgt

/-- info: 'PythExpandVerify.preservationC' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationC

/-- info: 'PythExpandVerify.preservationCs' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationCs

/-- info: 'PythExpandVerify.preservationC_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationC_real

/-- info: 'PythExpandVerify.preservationCs_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationCs_real

/-- info: 'PythExpandVerify.preservationC_stub_fails' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms preservationC_stub_fails

/-- info: 'PythExpandVerify.preservationCs_stub_fails' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms preservationCs_stub_fails

/-! ## Tier-3 wave 6 — comprehensions and lazy generators (the laziness depth)

Extends the collection fragment with list COMPREHENSIONS `[body for v in iter if filt]`
and a LAZY `any(body for v in iter)`. Comprehensions iterate a runtime value (not a
syntactic subterm), so — as with functions — this uses the uniform-fuel interpreter
(reusing `Val`/`getIndex`/`jsFdiv`). Two results: (1) `preservationG` — the compiled
comprehension preserves the Python value; (2) `anyG_preservation` — the lazy,
short-circuiting `any` (Python's REAL semantics, which the #348/#155 eager-materialize
bug violated) is preserved; it deliberately does NOT equal an eager fold — they
diverge exactly when a later element errors but an earlier is truthy (§16), and the
wave-5 rollout makes that non-vacuous with an EAGER stub refutation
(`anyG_eager_stub_fails`). The compiled side is the INDEPENDENT
`evalGtgt (L : IntDivLowering)` family below (C1-rollout wave 5), NOT the legacy
`tgt` flag. -/

inductive GExp where
  | lit (n : Int)
  | var (s : String)
  | add (a b : GExp) | sub (a b : GExp) | mul (a b : GExp) | fdiv (a b : GExp) | lt (a b : GExp)
  | listE (elts : List GExp)
  | index (lst idx : GExp)
  | len (e : GExp)
  | comp (body : GExp) (v : String) (iter : GExp) (filt : Option GExp)  -- [body for v in iter if filt]
deriving Repr

/-- **Python truthiness over the FULL fragment value domain** (`bool(v)`): a
    nonzero int is truthy, `0` falsy; a NONEMPTY list is truthy, `[]` falsy
    (CPython: `bool([1]) == True`, `bool([]) == False`). Used by the
    comprehension FILTER and by `any` on BOTH source and target — shared
    verified-core like `getIndex`, because the deviation under test is the `//`
    LOWERING, not truthiness. What wave-5 iter1 got wrong (F9,
    domain-completeness) was not the sharing but the DOMAIN: only `.vint` was
    recognized, so a list-valued filter/`any`-body was modeled as an ERROR
    (`none`) where CPython evaluates its truthiness — with the unhandled
    `.vlist` case closed shared-wrong by `rfl` on both sides. -/
def valTruthy : Val → Bool
  | .vint k => k ≠ 0
  | .vlist [] => false
  | .vlist (_ :: _) => true

-- CPython `bool()` pins for the helper itself.
#guard valTruthy (.vint (-1)) = true
#guard valTruthy (.vint 0) = false
#guard valTruthy (.vlist [.vint 0]) = true   -- nonempty is truthy even if the element is falsy
#guard valTruthy (.vlist []) = false

mutual
/-- Comprehension-fragment eval. `tgt = false` = Python REFERENCE semantics
    (the only branch any theorem uses). The `tgt = true` branch is LEGACY (the
    former F1 model-vs-model flag) and is NOT the compiled target — the genuine
    compiled target is the INDEPENDENT `evalGtgt (L : IntDivLowering)` below;
    NO theorem references `evalG true` (the wave-6 itertools migration deleted
    the last consumer, the TEMPORARY `evalG_flag_bridge` glue). -/
def evalG (tgt : Bool) : Nat → GExp → VEnv → Option Val
  | 0, _, _ => none
  | _ + 1, .lit n, _ => some (.vint n)
  | _ + 1, .var s, env => env.get s
  | fuel + 1, .add a b, env => match evalG tgt fuel a env, evalG tgt fuel b env with
      | some (.vint x), some (.vint y) => some (.vint (x + y)) | _, _ => none
  | fuel + 1, .sub a b, env => match evalG tgt fuel a env, evalG tgt fuel b env with
      | some (.vint x), some (.vint y) => some (.vint (x - y)) | _, _ => none
  | fuel + 1, .mul a b, env => match evalG tgt fuel a env, evalG tgt fuel b env with
      | some (.vint x), some (.vint y) => some (.vint (x * y)) | _, _ => none
  | fuel + 1, .fdiv a b, env => match evalG tgt fuel a env, evalG tgt fuel b env with
      | some (.vint x), some (.vint y) =>
          if y = 0 then none else some (.vint (if tgt then jsFdiv x y else Int.fdiv x y))
      | _, _ => none
  | fuel + 1, .lt a b, env => match evalG tgt fuel a env, evalG tgt fuel b env with
      | some (.vint x), some (.vint y) => some (.vint (if x < y then 1 else 0)) | _, _ => none
  | fuel + 1, .listE elts, env => (evalGs tgt fuel elts env).map .vlist
  | fuel + 1, .index lst idx, env => match evalG tgt fuel lst env, evalG tgt fuel idx env with
      | some (.vlist xs), some (.vint i) => (getIndex xs.length i).bind (fun j => xs[j.toNat]?)
      | _, _ => none
  | fuel + 1, .len e, env => match evalG tgt fuel e env with
      | some (.vlist xs) => some (.vint xs.length) | _ => none
  | fuel + 1, .comp body v iter filt, env => match evalG tgt fuel iter env with
      | some (.vlist xs) => (evalComp tgt fuel body v filt xs env).map .vlist
      | _ => none
termination_by n _ => n
def evalGs (tgt : Bool) : Nat → List GExp → VEnv → Option (List Val)
  | 0, _, _ => none
  | _ + 1, [], _ => some []
  | fuel + 1, e :: es, env => match evalG tgt fuel e env, evalGs tgt fuel es env with
      | some v, some vs => some (v :: vs) | _, _ => none
termination_by n _ => n
/-- The comprehension worker: map `body` over the source values, binding `v`,
    keeping those whose (optional) filter value is Python-truthy (`valTruthy`:
    nonzero int OR nonempty list — the FULL value domain, wave-5 iter2). -/
def evalComp (tgt : Bool) : Nat → GExp → String → Option GExp → List Val → VEnv → Option (List Val)
  | 0, _, _, _, _, _ => none
  | _ + 1, _, _, _, [], _ => some []
  | fuel + 1, body, v, filt, x :: xs, env =>
      let env' := (v, x) :: env
      let keep : Option Bool := match filt with
        | none => some true
        | some f => (evalG tgt fuel f env').map valTruthy
      keep.bind (fun k =>
        if k then match evalG tgt fuel body env', evalComp tgt fuel body v filt xs env with
                  | some bv, some rest => some (bv :: rest) | _, _ => none
        else evalComp tgt fuel body v filt xs env)
termination_by n _ => n
end

-- F9 reference pins (CPython): comprehensions, filter, deviation-in-body,
-- index-of-comp — the REFERENCE `evalG false` matches CPython. The compiled-side
-- guards (the retired `evalG true` guards) live with the independent `evalGjs`
-- in the wave-5 section below.
#guard (evalG false 50 (.comp (.mul (.var "x") (.lit 2)) "x" (.listE [.lit 1, .lit 2, .lit 3]) none) []
        |>.bind (fun v => match v with | .vlist xs => (xs[0]?.bind Val.asInt) | _ => none)) = some 2  -- [x*2 …][0]
#guard (evalG false 50 (.len (.comp (.var "x") "x" (.listE [.lit 1, .lit 2, .lit 3, .lit 4])
          (some (.lt (.var "x") (.lit 3))))) []).bind Val.asInt = some 2   -- [x for x in 1..4 if x<3] = [1,2], len 2
#guard (evalG false 50 (.index (.comp (.mul (.var "x") (.var "x")) "x" (.listE [.lit 1, .lit 2, .lit 3]) none)
          (.lit (-1))) []).bind Val.asInt = some 9   -- [x*x for x in 1..3][-1] = 9
-- CPython: [x//2 for x in [-7]][0] == -4 (floor, the deviation INSIDE a
-- comprehension body); and the DISCRIMINATING deviation-in-FILTER pin
-- (wave-5 iter2; the retired `x = -7`/`x//2 < 0` guard was NON-discriminating:
-- floor `-4` AND trunc `-3` are both `< 0`, so it pinned nothing):
-- len([x for x in [-1] if x // 2]) == 1 — the filter value is floor(-1/2) = -1,
-- TRUTHY (kept), where truncation gives 0, FALSY (dropped) — kept-ness itself
-- differs between the lowerings, pinning floor routing through the FILTER.
#guard (evalG false 50 (.index (.comp (.fdiv (.var "x") (.lit 2)) "x" (.listE [.lit (-7)]) none)
          (.lit 0)) []).bind Val.asInt = some (-4)
#guard (evalG false 50 (.len (.comp (.var "x") "x" (.listE [.lit (-1)])
          (some (.fdiv (.var "x") (.lit 2))))) []).bind Val.asInt = some 1
-- CPython LIST truthiness in the FILTER (wave-5 iter2, F9 domain-completeness):
-- [x for x in [[1]] if x] == [[1]] (nonempty list truthy, kept) and
-- [x for x in [[]] if x] == [] (empty list falsy, dropped) — NOT an error.
#guard (evalG false 50 (.len (.comp (.var "x") "x" (.listE [.listE [.lit 1]])
          (some (.var "x")))) []).bind Val.asInt = some 1
#guard (evalG false 50 (.len (.comp (.var "x") "x" (.listE [.listE []])
          (some (.var "x")))) []).bind Val.asInt = some 0

/-! ### Wave 5 (C1 rollout) — INDEPENDENT-target comprehension/lazy preservation

The previous `preservationG : evalG true fuel e env = evalG false fuel e env`
(with its mutual flag worker `preservationG'`) and
`anyG_preservation : anyG true … = anyG false …` were the same F1 model-vs-model
tautology as the old `preservationE`: `Bool`-flagged copies of ONE evaluator,
deviating only in the `//` arm — stubbing the shipping lowering could not break
them. Re-architected on the wave-1/2/3/4 recipe: `evalGtgt`/`evalGstgt`/
`evalComptgt` are a SEPARATE fuel-indexed mutual recursion, parameterized by the
integer-division lowering, so `//` threads through comprehension BODIES, ITER
sources, and FILTERS exactly as in the emitted JS; `index` still reuses the
verified-core `getIndex` on both sides. The lazy `any` target `anyGtgt` is
likewise independent AND still genuinely lazy (short-circuits at the first
truthy body value). The predicates are proved for the shipped floor-correction
(`preservationG_real`, `anyG_preservation_real`) and REFUTED both for the naive
truncating lowering (`preservationG_stub_fails`, `anyG_stub_fails` — the
deviation FLIPS truthiness) and, on the `any` side, for the WRONG EAGER
materialize-then-fold shape of the #348/#155 bug (`anyG_eager_stub_fails`) —
so the laziness content is non-vacuous, not just the arithmetic.

Iter2 (F9 domain-completeness): the comprehension FILTER and `any` decide
truthiness by the shared verified-core `valTruthy` over the FULL `Val` domain
(nonzero int / NONEMPTY list) on BOTH source and target — iter1 recognized
only `.vint`, modeling `any([1] for x in [0])` (CPython `True`) and
`[x for x in [[1]] if x]` (CPython `[[1]]`) as ERRORS, with the unhandled
`.vlist` case closed shared-wrong by `rfl`; and the deviation-in-filter pin
is now DISCRIMINATING (`[x for x in [-1] if x // 2]`: floor keeps, trunc
drops — the retired `x = -7` witness was satisfied by both lowerings). -/

mutual
/-- **Independent target evaluator** (comprehension fragment): the compiled
    program's semantics under lowering `L`. A SEPARATE fuel-indexed mutual
    recursion (not the `Bool` flag on `evalG`); the `//` arm calls the
    lowering's operation, and a comprehension routes its BODY, ITER source,
    and FILTER through the same lowering, exactly as in the emitted JS.
    `index` uses the verified-core `getIndex`. -/
def evalGtgt (L : IntDivLowering) : Nat → GExp → VEnv → Option Val
  | 0, _, _ => none
  | _ + 1, .lit n, _ => some (.vint n)
  | _ + 1, .var s, env => env.get s
  | fuel + 1, .add a b, env => match evalGtgt L fuel a env, evalGtgt L fuel b env with
      | some (.vint x), some (.vint y) => some (.vint (x + y)) | _, _ => none
  | fuel + 1, .sub a b, env => match evalGtgt L fuel a env, evalGtgt L fuel b env with
      | some (.vint x), some (.vint y) => some (.vint (x - y)) | _, _ => none
  | fuel + 1, .mul a b, env => match evalGtgt L fuel a env, evalGtgt L fuel b env with
      | some (.vint x), some (.vint y) => some (.vint (x * y)) | _, _ => none
  | fuel + 1, .fdiv a b, env => match evalGtgt L fuel a env, evalGtgt L fuel b env with
      | some (.vint x), some (.vint y) =>
          if y = 0 then none else some (.vint (L.fdiv x y))
      | _, _ => none
  | fuel + 1, .lt a b, env => match evalGtgt L fuel a env, evalGtgt L fuel b env with
      | some (.vint x), some (.vint y) => some (.vint (if x < y then 1 else 0)) | _, _ => none
  | fuel + 1, .listE elts, env => (evalGstgt L fuel elts env).map .vlist
  | fuel + 1, .index lst idx, env => match evalGtgt L fuel lst env, evalGtgt L fuel idx env with
      | some (.vlist xs), some (.vint i) => (getIndex xs.length i).bind (fun j => xs[j.toNat]?)
      | _, _ => none
  | fuel + 1, .len e, env => match evalGtgt L fuel e env with
      | some (.vlist xs) => some (.vint xs.length) | _ => none
  | fuel + 1, .comp body v iter filt, env => match evalGtgt L fuel iter env with
      | some (.vlist xs) => (evalComptgt L fuel body v filt xs env).map .vlist
      | _ => none
termination_by n _ => n
/-- Independent-target element-list eval (for `listE`): every element routes
    through the same lowering `L`. -/
def evalGstgt (L : IntDivLowering) : Nat → List GExp → VEnv → Option (List Val)
  | 0, _, _ => none
  | _ + 1, [], _ => some []
  | fuel + 1, e :: es, env => match evalGtgt L fuel e env, evalGstgt L fuel es env with
      | some v, some vs => some (v :: vs) | _, _ => none
termination_by n _ => n
/-- Independent-target comprehension worker: map `body` over the source values
    binding `v`, keeping those whose (optional) filter value is Python-truthy
    (`valTruthy`: nonzero int OR nonempty list — the FULL value domain; the
    truthiness helper is shared verified-core like `getIndex`, the LOWERING is
    what varies) — body AND filter evals route through the lowering `L`. -/
def evalComptgt (L : IntDivLowering) : Nat → GExp → String → Option GExp → List Val → VEnv → Option (List Val)
  | 0, _, _, _, _, _ => none
  | _ + 1, _, _, _, [], _ => some []
  | fuel + 1, body, v, filt, x :: xs, env =>
      let env' := (v, x) :: env
      let keep : Option Bool := match filt with
        | none => some true
        | some f => (evalGtgt L fuel f env').map valTruthy
      keep.bind (fun k =>
        if k then match evalGtgt L fuel body env', evalComptgt L fuel body v filt xs env with
                  | some bv, some rest => some (bv :: rest) | _, _ => none
        else evalComptgt L fuel body v filt xs env)
termination_by n _ => n
end

/-- The compiled comprehension-fragment semantics: the independent target under
    the SHIPPED lowering. -/
abbrev evalGjs : Nat → GExp → VEnv → Option Val := evalGtgt jsELowering

/-- Comprehension-fragment preservation as a predicate OVER the lowering — the
    SAME predicate is proved for the shipped lowering (`preservationG_real`)
    and REFUTED for the stub (`preservationG_stub_fails`). Quantifies over the
    fuel too: a lowering preserves only if it does so at EVERY depth. -/
def GPreserves (L : IntDivLowering) : Prop :=
  ∀ fuel e env, evalGtgt L fuel e env = evalG false fuel e env

-- Compiled-side guards (the retired `evalG true` guards, now on the genuine
-- independent target): index-of-comp, and the deviation inside a BODY.
#guard (evalGjs 50 (.index (.comp (.mul (.var "x") (.var "x")) "x" (.listE [.lit 1, .lit 2, .lit 3]) none)
          (.lit (-1))) []).bind Val.asInt = some 9   -- [x*x for x in 1..3][-1] = 9
#guard (evalGjs 50 (.index (.comp (.fdiv (.var "x") (.lit 2)) "x" (.listE [.lit (-7)]) none)
          (.lit 0)) []).bind Val.asInt = some (-4)   -- [x//2 for x in [-7]][0] = -4 (Python floor)
-- DISCRIMINATING deviation-in-FILTER contrast (wave-5 iter2) on the
-- independent targets: [x for x in [-1] if x // 2] — the shipped floor
-- lowering KEEPS the element (filter value -1//2 = -1, truthy → len 1); the
-- truncating stub DROPS it (trunc 0, falsy → len 0). Kept-ness DIFFERS
-- between the lowerings, so this pins `//` routing through the FILTER (the
-- retired x = -7 witness could not: floor -4 and trunc -3 are both < 0).
#guard (evalGjs 50 (.len (.comp (.var "x") "x" (.listE [.lit (-1)])
          (some (.fdiv (.var "x") (.lit 2))))) []).bind Val.asInt = some 1   -- floor: kept ✓
#guard ((evalGtgt truncELowering 50 (.len (.comp (.var "x") "x" (.listE [.lit (-1)])
          (some (.fdiv (.var "x") (.lit 2))))) []).bind Val.asInt) = some 0  -- trunc stub: dropped ✗
-- LIST truthiness in the filter on the independent target:
-- [x for x in [[1]] if x] == [[1]] (was an ERROR on the iter1 model).
#guard (evalGjs 50 (.len (.comp (.var "x") "x" (.listE [.listE [.lit 1]])
          (some (.var "x")))) []).bind Val.asInt = some 1

/-- Combined preservation worker for the three mutual INDEPENDENT evaluators:
    ONE induction on `fuel` carries all three statements together (uniform fuel
    — every recursive call, including the comprehension worker's body/filter
    evals and its own recursion, sits at the predecessor, so the three IHs at
    `fuel` discharge every recursion site). Binds the INDEPENDENT target under
    the shipped lowering to the Python reference `evalG false`; the `.fdiv` arm
    is closed by `jsFdiv_eq_fdiv` on the `y ≠ 0` branch — real induction, not a
    flag-vs-flag identity. -/
private theorem preservationG'tgt (fuel : Nat) :
    (∀ e env, evalGtgt jsELowering fuel e env = evalG false fuel e env) ∧
    (∀ es env, evalGstgt jsELowering fuel es env = evalGs false fuel es env) ∧
    (∀ body v filt xs env,
      evalComptgt jsELowering fuel body v filt xs env = evalComp false fuel body v filt xs env) := by
  induction fuel with
  | zero =>
    exact ⟨fun e env => by simp only [evalGtgt, evalG],
           fun es env => by simp only [evalGstgt, evalGs],
           fun body v filt xs env => by simp only [evalComptgt, evalComp]⟩
  | succ fuel ih =>
    obtain ⟨ihG, ihGs, ihComp⟩ := ih
    refine ⟨fun e env => ?_, fun es env => ?_, fun body v filt xs env => ?_⟩
    · cases e with
      | lit n => simp only [evalGtgt, evalG]
      | var s => simp only [evalGtgt, evalG]
      | add a b => simp only [evalGtgt, evalG, ihG]
      | sub a b => simp only [evalGtgt, evalG, ihG]
      | mul a b => simp only [evalGtgt, evalG, ihG]
      | lt a b => simp only [evalGtgt, evalG, ihG]
      | fdiv a b =>
        simp only [evalGtgt, evalG, ihG]
        cases evalG false fuel a env with
        | none => rfl
        | some va =>
          cases evalG false fuel b env with
          | none => cases va <;> rfl
          | some vb =>
            cases va with
            | vlist _ => rfl
            | vint x =>
              cases vb with
              | vlist _ => rfl
              | vint y =>
                by_cases hy : y = 0
                · simp [hy]
                · simp [hy, jsELowering, jsFdiv_eq_fdiv x y hy]
      | listE elts => simp only [evalGtgt, evalG, ihGs]
      | index lst idx => simp only [evalGtgt, evalG, ihG]
      | len e1 => simp only [evalGtgt, evalG, ihG]
      | comp body v iter filt => simp only [evalGtgt, evalG, ihG, ihComp]
    · cases es with
      | nil => simp only [evalGstgt, evalGs]
      | cons e es => simp only [evalGstgt, evalGs, ihG, ihGs]
    · cases xs with
      | nil => simp only [evalComptgt, evalComp]
      | cons x xs =>
        -- the equation lemmas for the cons arm split on `filt` (the nested
        -- `keep` match), so case on it first; each branch then rewrites the
        -- filter/body evals (ihG) and the recursion (ihComp) at `fuel`.
        cases filt with
        | none => simp only [evalComptgt, evalComp, ihG, ihComp]
        | some f => simp only [evalComptgt, evalComp, ihG, ihComp]

/-- **Comprehension preservation (Tier-3 wave 6 / C1-rollout wave 5,
    re-architected).** The INDEPENDENT compiled target under the shipped
    lowering computes the Python reference on every comprehension-fragment
    expression, at every fuel — filters, the `//` deviation in bodies/ITER
    sources/filters, and indexing the result all preserved. -/
theorem preservationG (fuel : Nat) (e : GExp) (env : VEnv) :
    evalGjs fuel e env = evalG false fuel e env :=
  (preservationG'tgt fuel).1 e env

/-- The re-architected statement, in predicate form: the shipped lowering
    preserves comprehensions. Same content as `preservationG`; this is the
    instantiation the stub litmus contrasts against. -/
theorem preservationG_real : GPreserves jsELowering := preservationG

/-- **Stub litmus (wave 5, comprehension side).** The SAME preservation
    predicate is FALSE for the naive truncating lowering, on a deviating
    program whose `//` sits INSIDE a comprehension body:
    `[x // 2 for x in [-7]][0]` computes JS-trunc `-3` under the stub where
    Python floors to `-4` — a concrete contradiction the old
    `evalG true = evalG false` statement could not express. -/
theorem preservationG_stub_fails : ¬ GPreserves truncELowering := by
  intro h
  have hc := h 10 (.index (.comp (.fdiv (.var "x") (.lit 2)) "x" (.listE [.lit (-7)]) none)
      (.lit 0)) []
  -- The evaluators are fuel-recursive (not kernel-reducible): step them via
  -- their equation lemmas; hc reduces through `some (.vint (-3)) = some (.vint (-4))`
  -- (stub `Int.tdiv (-7) 2 = -3` vs Python `Int.fdiv (-7) 2 = -4`) to `False`.
  simp [evalGtgt, evalGstgt, evalComptgt, evalG, evalGs, evalComp, truncELowering,
        getIndex, VEnv.get] at hc

-- The contrast, concretely (the deviation lands in a comprehension BODY and is
-- read back out through `getIndex` indexing):
#guard ((evalGtgt truncELowering 10 (.index (.comp (.fdiv (.var "x") (.lit 2)) "x"
        (.listE [.lit (-7)]) none) (.lit 0)) []).bind Val.asInt) = some (-3)  -- stub: JS trunc ✗

/-- SPOT (through the theorem, not by evaluation): a concrete comprehension in
    the INDEPENDENT compiled semantics — `[x*x for x in [1,2,3]][-1]` — routed
    THROUGH `preservationG` to the Python reference and evaluated there to 9
    (negative index normalized by the verified-core `getIndex`). Fails if the
    statement is weakened back to model-vs-model. -/
example :
    (evalGjs 50 (.index (.comp (.mul (.var "x") (.var "x")) "x"
        (.listE [.lit 1, .lit 2, .lit 3]) none) (.lit (-1))) []).bind Val.asInt
      = some 9 := by
  rw [preservationG]
  simp [evalG, evalGs, evalComp, VEnv.get, getIndex, Val.asInt]

/-- SPOT: a DEVIATING comprehension — `[x // 2 for x in [-7]][0]` — the
    INDEPENDENT compiled value is Python's floor `-4` (not JS-trunc `-3`),
    derived via `preservationG` (exercises the deviation through a
    comprehension body + the verified-core `getIndex` read-back). -/
example :
    (evalGjs 50 (.index (.comp (.fdiv (.var "x") (.lit 2)) "x"
        (.listE [.lit (-7)]) none) (.lit 0)) []).bind Val.asInt
      = some (-4) := by
  rw [preservationG]
  simp [evalG, evalGs, evalComp, VEnv.get, getIndex, Val.asInt]

/-- SPOT (wave-5 iter2, LIST truthiness through the theorem):
    `[x for x in [[1]] if x]` — the filter value is a NONEMPTY LIST, truthy,
    so the element is KEPT — computes `[[1]]` in the INDEPENDENT compiled
    semantics, derived via `preservationG`. Fails on the iter1 model, where a
    list-valued filter was modeled as an ERROR (`none`), and fails if the
    statement is weakened back to model-vs-model. -/
example :
    evalGjs 50 (.comp (.var "x") "x" (.listE [.listE [.lit 1]]) (some (.var "x"))) []
      = some (.vlist [.vlist [.vint 1]]) := by
  rw [preservationG]
  simp [evalG, evalGs, evalComp, valTruthy, VEnv.get]

/-- A lazy, short-circuiting `any(body for v in iter)`. `tgt = false` = Python
    REFERENCE semantics (the only branch any theorem uses): stops at the first
    Python-truthy body value (`valTruthy` — nonzero int OR nonempty list, the
    FULL value domain; wave-5 iter2) and does NOT evaluate the rest — exactly
    the behavior the #348/#155 eager-materialize bug broke. The `tgt = true` branch is LEGACY
    (the former F1 flag) and is NOT the compiled target — the genuine compiled
    target is the INDEPENDENT `anyGtgt` below; no theorem references
    `anyG true`. -/
def anyG (tgt : Bool) : Nat → GExp → String → List Val → VEnv → Option Bool
  | 0, _, _, _, _ => none
  | _ + 1, _, _, [], _ => some false
  | fuel + 1, body, v, x :: xs, env =>
      match evalG tgt fuel body ((v, x) :: env) with
      | some val => if valTruthy val then some true else anyG tgt fuel body v xs env
      | none => none
termination_by n _ => n

/-- **Independent target** for the lazy `any`: the compiled combinator under
    lowering `L`. STILL genuinely lazy — same short-circuit shape as the
    reference (stops at the first truthy body value, never evaluates the rest)
    — but body evals route through the INDEPENDENT `evalGtgt L`, not the flag. -/
def anyGtgt (L : IntDivLowering) : Nat → GExp → String → List Val → VEnv → Option Bool
  | 0, _, _, _, _ => none
  | _ + 1, _, _, [], _ => some false
  | fuel + 1, body, v, x :: xs, env =>
      match evalGtgt L fuel body ((v, x) :: env) with
      | some val => if valTruthy val then some true else anyGtgt L fuel body v xs env
      | none => none
termination_by n _ => n

/-- The compiled lazy-`any` semantics: the independent target under the
    SHIPPED lowering. -/
abbrev anyGjs : Nat → GExp → String → List Val → VEnv → Option Bool := anyGtgt jsELowering

/-- The WRONG EAGER `any` lowering — the exact #348/#155 bug shape: materialize
    the FULL comprehension first, then fold (`List.any valTruthy` over the
    materialized values — the SAME total truthiness as the reference, so the
    ONLY difference is EAGERNESS). It evaluates the body on EVERY element even
    after a truthy one, so a later ERRORING element poisons the whole call
    where Python's lazy `any` has already returned `True`. Used ONLY to REFUTE
    the preservation predicate (`anyG_eager_stub_fails`) — this is what makes
    the laziness content of `anyG_preservation` non-vacuous. -/
def anyGeagerTgt (L : IntDivLowering) : Nat → GExp → String → List Val → VEnv → Option Bool
  | 0, _, _, _, _ => none
  | fuel + 1, body, v, xs, env =>
      (evalComptgt L fuel body v none xs env).map (fun vs => vs.any valTruthy)

/-- Lazy-`any` preservation as a predicate OVER the compiled combinator
    implementation — the SAME predicate is proved for the shipped lazy target
    (`anyG_preservation_real`) and REFUTED on BOTH failure axes: the wrong
    ARITHMETIC lowering (`anyG_stub_fails`, truthiness flipped by trunc-`//`)
    and the wrong EAGER shape (`anyG_eager_stub_fails`, the #348 bug). -/
def AnyGPreserves (impl : Nat → GExp → String → List Val → VEnv → Option Bool) : Prop :=
  ∀ fuel body v xs env, impl fuel body v xs env = anyG false fuel body v xs env

-- F9 pins (CPython): `any(1//x for x in [1, 0])` is `True` — the LAZY genexp
-- short-circuits at x=1 and NEVER evaluates 1//0 (no ZeroDivisionError) —
-- while the EAGER `any([1//x for x in [1, 0]])` RAISES (materializes first).
-- And `any(x//2 for x in [-1])` is `True` (floor: -1//2 == -1, truthy).
#guard anyG false 10 (.fdiv (.lit 1) (.var "x")) "x" [.vint 1, .vint 0] [] = some true
#guard anyG false 10 (.fdiv (.var "x") (.lit 2)) "x" [.vint (-1)] [] = some true
-- LIST truthiness in `any` (wave-5 iter2, F9 domain-completeness — CPython):
-- any([1] for x in [0]) is True (the body value is a NONEMPTY LIST, truthy —
-- NOT an error, which is what the iter1 int-only model made it) and
-- any([] for x in [0]) is False (empty list falsy, exhausted → False).
#guard anyG false 10 (.listE [.lit 1]) "x" [.vint 0] [] = some true
#guard anyG false 10 (.listE []) "x" [.vint 0] [] = some false
-- Compiled-side pins on the independent targets: the shipped lazy target
-- short-circuits identically; the eager shape errors; the trunc stub flips
-- truthiness (JS-trunc -1//2 == 0, falsy — CPython floor gives -1, truthy).
#guard anyGjs 10 (.fdiv (.lit 1) (.var "x")) "x" [.vint 1, .vint 0] [] = some true
#guard anyGeagerTgt jsELowering 10 (.fdiv (.lit 1) (.var "x")) "x" [.vint 1, .vint 0] [] = none   -- eager ✗
#guard anyGtgt truncELowering 10 (.fdiv (.var "x") (.lit 2)) "x" [.vint (-1)] [] = some false     -- stub ✗
#guard anyGjs 10 (.listE [.lit 1]) "x" [.vint 0] [] = some true   -- list-truthy body on the target ✓

/-- **Lazy-`any` preservation (#348 laziness / C1-rollout wave 5,
    re-architected).** The INDEPENDENT compiled lazy `any` under the shipped
    lowering computes the Python reference at every fuel — the short-circuit
    preserved (a program where a LATER element errors but an EARLIER is truthy
    returns `True` on both sides; see the `anyGjs`/`anyGeagerTgt` pins and
    `anyG_eager_stub_fails`). Body evals route through `preservationG`; the
    recursion through the fuel IH. It deliberately does NOT equal a full eager
    fold: they diverge exactly when a later element errors but an earlier is
    truthy — the observable reason materialization was wrong (§16). -/
theorem anyG_preservation (fuel : Nat) (body : GExp) (v : String)
    (xs : List Val) (env : VEnv) :
    anyGjs fuel body v xs env = anyG false fuel body v xs env := by
  induction fuel generalizing xs env with
  | zero => simp only [anyGtgt, anyG]
  | succ fuel ih =>
    cases xs with
    | nil => simp only [anyGtgt, anyG]
    | cons x xs =>
      simp only [anyGtgt, anyG, preservationG fuel body ((v, x) :: env)]
      -- after routing the body eval through `preservationG`, BOTH sides
      -- branch on `valTruthy val` over the FULL `Val` domain (no per-
      -- constructor split, hence no shared-wrong `rfl` on `.vlist` — the
      -- iter1 F9 defect): the truthy branch is `some true` on both, the
      -- falsy branch recurses via the fuel IH.
      cases evalG false fuel body ((v, x) :: env) with
      | none => rfl
      | some val => simp only [ih]

/-- The re-architected statement, in predicate form: the shipped LAZY compiled
    combinator preserves. The instantiation the two stub litmuses contrast
    against. -/
theorem anyG_preservation_real : AnyGPreserves anyGjs := anyG_preservation

/-- **Stub litmus (wave 5, `any` arithmetic axis).** The SAME predicate is
    FALSE for the lazy combinator under the TRUNCATING lowering: on
    `any(x // 2 for x in [-1])` the deviation FLIPS TRUTHINESS — stub
    JS-trunc `-1 // 2 = 0` (falsy → `False`) where Python floors to `-1`
    (truthy → `True`). -/
theorem anyG_stub_fails : ¬ AnyGPreserves (anyGtgt truncELowering) := by
  intro h
  have hc := h 10 (.fdiv (.var "x") (.lit 2)) "x" [.vint (-1)] []
  -- hc reduces through `some false = some true` to `False`.
  simp [anyGtgt, anyG, evalGtgt, evalG, valTruthy, truncELowering, VEnv.get] at hc

/-- **Stub litmus (wave 5, LAZINESS axis).** The SAME predicate is FALSE for
    the EAGER materialize-then-fold shape even under the SHIPPED (arithmetically
    correct) lowering: on `any(1 // x for x in [1, 0])` the eager stub
    evaluates `1 // 0` (error → `none`) where the lazy reference has already
    short-circuited to `True` at `x = 1`. This is the #348/#155 bug refuted as
    a wrong lowering — the laziness content of `anyG_preservation` is
    non-vacuous, not just its arithmetic. -/
theorem anyG_eager_stub_fails : ¬ AnyGPreserves (anyGeagerTgt jsELowering) := by
  intro h
  have hc := h 10 (.fdiv (.lit 1) (.var "x")) "x" [.vint 1, .vint 0] []
  -- hc reduces through `none = some true` to `False` (the second element's
  -- div-by-zero poisons the eager materialization; the value of `1 // 1` is
  -- irrelevant — the `some`/`none` match shape already decides it).
  simp [anyGeagerTgt, anyG, evalComptgt, evalGtgt, evalG, valTruthy, jsELowering, VEnv.get] at hc

/-- SPOT (through the theorem, not by evaluation): the error-after-truthy
    program `any(1 // x for x in [1, 0])` in the INDEPENDENT compiled
    semantics — derived via `anyG_preservation`, so it fails if the statement
    is weakened back to model-vs-model OR if the compiled target stops being
    lazy (the eager shape yields `none` here, not `some true`). -/
example : anyGjs 10 (.fdiv (.lit 1) (.var "x")) "x" [.vint 1, .vint 0] [] = some true := by
  rw [anyG_preservation]
  simp [anyG, evalG, valTruthy, VEnv.get]

/-- SPOT (wave-5 iter2, LIST truthiness through `anyG_preservation`):
    `any([1] for x in [0])` is `True` — the body value is a NONEMPTY LIST,
    truthy — in the INDEPENDENT compiled semantics, derived via the theorem.
    Fails on the iter1 model, where a `.vlist` body value made `any` an ERROR
    (`none`), and fails if the statement is weakened back to model-vs-model. -/
example : anyGjs 10 (.listE [.lit 1]) "x" [.vint 0] [] = some true := by
  rw [anyG_preservation]
  simp [anyG, evalG, evalGs, valTruthy]

/-- info: '_private.PythExpandVerify.0.PythExpandVerify.preservationG'tgt' depends on axioms: [propext,
 Classical.choice,
 Quot.sound] -/
#guard_msgs in
#print axioms preservationG'tgt

/-- info: 'PythExpandVerify.preservationG' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationG

/-- info: 'PythExpandVerify.preservationG_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationG_real

/-- info: 'PythExpandVerify.preservationG_stub_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationG_stub_fails

/-- info: 'PythExpandVerify.anyG_preservation' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms anyG_preservation

/-- info: 'PythExpandVerify.anyG_preservation_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms anyG_preservation_real

/-- info: 'PythExpandVerify.anyG_stub_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms anyG_stub_fails

/-- info: 'PythExpandVerify.anyG_eager_stub_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms anyG_eager_stub_fails


/-! ## Tier-3 wave 7 — itertools (lazy core + combinatorial)

Models the itertools surface the compiler lowers: the lazy core
(`count`/`islice`/`takewhile`/`chain`) and the combinatorial trio
(`product`/`permutations`/`combinations`). An `IExp` evaluates to a
MATERIALIZED `List Val`; laziness is modeled the wave-6 way — `takewhile`
short-circuits like `anyG` (stops at the first non-truthy predicate value and
never evaluates the predicate on the rest), and the infinite `count(start,
step)` is bounded by a term count and composed via `islice`/`takewhile`, so
every program materializes to a finite list and fuel bounds termination. The
`//` deviation enters ONLY through `GExp` sub-expressions — a `fromList`
source or a `takewhile` predicate. The compiled side is the INDEPENDENT
`evalItgt`/`evalIstgt`/`takewhileItgt (L : IntDivLowering)` family below
(C1-rollout wave 6): embedded `GExp` sub-terms evaluate via the independent
`evalGtgt L`, and the preservation proofs inherit the GENUINE `preservationG`
(the wave-5 TEMPORARY `evalG_flag_bridge` glue is DELETED). `takewhile`
predicate truthiness is decided by the shared verified-core `valTruthy` over
the FULL `Val` domain (nonzero int / NONEMPTY list) on BOTH source and target
— CPython `takewhile(lambda x: x, [[1], []])` is `[[1]]`, not an error.
The combinators themselves are structural over already-materialized values. -/

/-- First `n` terms of `itertools.count(start, step)`. -/
def countList (start step : Int) : Nat → List Val
  | 0 => []
  | n + 1 => .vint start :: countList (start + step) step n

/-- `itertools.islice(xs, start, stop, step)` over a materialized source: the
    elements at indices `start, start + step, …` that are `< stop` (CPython
    semantics; `step = 0` is rejected at the `evalI` level, as CPython raises
    `ValueError`). -/
def isliceList (xs : List Val) (start stop step : Nat) : List Val :=
  (List.range xs.length).filterMap (fun i =>
    if start ≤ i ∧ i < stop ∧ (i - start) % step = 0 then xs[i]? else none)

/-- `itertools.product(xs, ys)` in CPython order (`x` outer, `y` inner), each
    pair a `vlist`. -/
def productList (xs ys : List Val) : List Val :=
  xs.flatMap (fun x => ys.map (fun y => .vlist [x, y]))

/-- `r`-permutations of `xs` in CPython order: pick each position in source
    order, then recurse on the list with that position removed. -/
def permutationsList : Nat → List Val → List (List Val)
  | 0, _ => [[]]
  | r + 1, xs =>
      (List.range xs.length).flatMap (fun i =>
        match xs[i]? with
        | some x => (permutationsList r (xs.eraseIdx i)).map (x :: ·)
        | none => [])

/-- `r`-combinations of `xs` in CPython order (lexicographic by position):
    those containing the head, then those from the tail. -/
def combinationsList : Nat → List Val → List (List Val)
  | 0, _ => [[]]
  | _ + 1, [] => []
  | r + 1, x :: xs => (combinationsList r xs).map (x :: ·) ++ combinationsList (r + 1) xs

/-- Project a materialized run of ints (for `#guard`s). -/
def valsAsInts : List Val → Option (List Int)
  | [] => some []
  | v :: vs => match v.asInt, valsAsInts vs with
      | some n, some ns => some (n :: ns)
      | _, _ => none

/-- Project a run of int-tuple `vlist`s (product/permutations/combinations
    results) for `#guard`s. -/
def valsAsIntLists : List Val → Option (List (List Int))
  | [] => some []
  | .vlist ys :: vs => match valsAsInts ys, valsAsIntLists vs with
      | some ns, some rest => some (ns :: rest)
      | _, _ => none
  | .vint _ :: _ => none

inductive IExp where
  | count (start step : Int) (n : Nat)          -- islice(count(start, step), n) → first n terms
  | fromList (g : GExp)                          -- lift a list/comprehension GExp as a source
  | islice (src : IExp) (start stop step : Nat)  -- itertools.islice over a materialized source
  | takewhile (pred : GExp) (v : String) (src : IExp)  -- LAZY short-circuit; pred may deviate
  | chain (parts : List IExp)                    -- itertools.chain(*parts) → flatten
  | product (a b : IExp)                         -- itertools.product(a, b) → vlist pairs
  | permutations (src : IExp) (r : Nat)          -- r-permutations, each a vlist
  | combinations (src : IExp) (r : Nat)          -- r-combinations, each a vlist
deriving Repr

/-- The lazy `takewhile` worker — the short-circuit shape of `anyG`, but
    COLLECTING the truthy prefix: stops at the first non-truthy predicate
    value and never evaluates the predicate on the rest. Predicate truthiness
    is the shared verified-core `valTruthy` over the FULL `Val` domain
    (nonzero int / NONEMPTY list — F9 domain-completeness; the earlier
    int-only match modeled a list-valued predicate as an ERROR where CPython
    evaluates its truthiness). `tgt = false` = Python REFERENCE semantics (the
    only branch any theorem uses); `tgt = true` is LEGACY (the former F1
    model-vs-model flag) — the genuine compiled target is the INDEPENDENT
    `takewhileItgt (L : IntDivLowering)` below, and NO theorem references
    `takewhileI true`. -/
def takewhileI (tgt : Bool) : Nat → GExp → String → List Val → VEnv → Option (List Val)
  | 0, _, _, _, _ => none
  | _ + 1, _, _, [], _ => some []
  | fuel + 1, pred, v, x :: xs, env =>
      match evalG tgt fuel pred ((v, x) :: env) with
      | some val =>
          if valTruthy val then (takewhileI tgt fuel pred v xs env).map (x :: ·)
          else some []
      | none => none
termination_by n _ => n

mutual
/-- Itertools evaluator: materializes an `IExp` to a finite `List Val`. `tgt`
    reaches only the `GExp` sub-evaluations (`fromList` sources and
    `takewhile` predicates); every combinator itself is target-independent.
    `tgt = false` = Python REFERENCE semantics (the only branch any theorem
    uses); `tgt = true` is LEGACY (the former F1 model-vs-model flag) — the
    genuine compiled target is the INDEPENDENT
    `evalItgt (L : IntDivLowering)` below, and NO theorem references
    `evalI true`. -/
def evalI (tgt : Bool) : Nat → IExp → VEnv → Option (List Val)
  | 0, _, _ => none
  | _ + 1, .count start step n, _ => some (countList start step n)
  | fuel + 1, .fromList g, env =>
      match evalG tgt fuel g env with
      | some (.vlist xs) => some xs
      | _ => none
  | fuel + 1, .islice src a b s, env =>
      if s = 0 then none
      else (evalI tgt fuel src env).map (fun xs => isliceList xs a b s)
  | fuel + 1, .takewhile pred v src, env =>
      (evalI tgt fuel src env).bind (fun xs => takewhileI tgt fuel pred v xs env)
  | fuel + 1, .chain parts, env => (evalIs tgt fuel parts env).map List.flatten
  | fuel + 1, .product a b, env =>
      match evalI tgt fuel a env, evalI tgt fuel b env with
      | some xs, some ys => some (productList xs ys)
      | _, _ => none
  | fuel + 1, .permutations src r, env =>
      (evalI tgt fuel src env).map (fun xs => (permutationsList r xs).map .vlist)
  | fuel + 1, .combinations src r, env =>
      (evalI tgt fuel src env).map (fun xs => (combinationsList r xs).map .vlist)
termination_by n _ => n
/-- Eval a list of itertools sources (for `chain`). -/
def evalIs (tgt : Bool) : Nat → List IExp → VEnv → Option (List (List Val))
  | 0, _, _ => none
  | _ + 1, [], _ => some []
  | fuel + 1, e :: es, env =>
      match evalI tgt fuel e env, evalIs tgt fuel es env with
      | some xs, some xss => some (xs :: xss)
      | _, _ => none
termination_by n _ => n
end

-- executable bindings pinned to CPython (F9 pins on the REFERENCE
-- `evalI false`): count, takewhile∘count (the lazy compose), islice, chain,
-- product, permutations, combinations, a comprehension source, and a
-- takewhile whose PREDICATE carries the `//` deviation. The compiled-side
-- guards (the retired `evalI true` guard) live with the independent
-- `evalIjs` in the wave-6 section below.
#guard ((evalI false 50 (.count 1 7 5) []).bind valsAsInts) = some [1, 8, 15, 22, 29]
#guard ((evalI false 50 (.takewhile (.lt (.var "x") (.lit 100)) "x" (.count 1 7 20)) []).bind valsAsInts) = some [1, 8, 15, 22, 29, 36, 43, 50, 57, 64, 71, 78, 85, 92, 99]
#guard ((evalI false 50 (.islice (.count 0 1 10) 2 9 3) []).bind valsAsInts) = some [2, 5, 8]
#guard ((evalI false 50 (.chain [.count 1 1 2, .count 3 1 1, .count 4 1 2]) []).bind valsAsInts) = some [1, 2, 3, 4, 5]
#guard ((evalI false 50 (.product (.count 0 1 2) (.count 2 1 2)) []).bind valsAsIntLists) = some [[0, 2], [0, 3], [1, 2], [1, 3]]
#guard ((evalI false 50 (.permutations (.count 1 1 3) 2) []).bind valsAsIntLists) = some [[1, 2], [1, 3], [2, 1], [2, 3], [3, 1], [3, 2]]
#guard ((evalI false 50 (.combinations (.count 1 1 4) 2) []).bind valsAsIntLists) = some [[1, 2], [1, 3], [1, 4], [2, 3], [2, 4], [3, 4]]
#guard ((evalI false 50 (.fromList (.comp (.mul (.var "x") (.lit 2)) "x" (.listE [.lit 1, .lit 2, .lit 3]) none)) []).bind valsAsInts) = some [2, 4, 6]
#guard ((evalI false 50 (.takewhile (.lt (.fdiv (.sub (.var "x") (.lit 3)) (.lit 2)) (.lit 1)) "x" (.count 0 1 6)) []).bind valsAsInts) = some [0, 1, 2, 3, 4]
-- F9 CPython pin, DISCRIMINATING deviation-in-PREDICATE (kept-ness itself
-- differs between the lowerings): takewhile(x // 2, [-1]) keeps the element —
-- the predicate value is floor(-1/2) = -1, TRUTHY — where truncation gives 0,
-- FALSY (stops). (The `(x-3)//2 < 1` pin above does NOT discriminate
-- kept-ness: floor and trunc land on the same side of `< 1` for every term.)
#guard ((evalI false 10 (.takewhile (.fdiv (.var "x") (.lit 2)) "x"
        (.fromList (.listE [.lit (-1)]))) []).bind valsAsInts) = some [-1]
-- F9 CPython LAZINESS pin: takewhile(1 // x, [2, 0]) == [] — the predicate at
-- x = 2 is 1 // 2 == 0, FALSY, so takewhile STOPS and never evaluates 1 // 0
-- (no ZeroDivisionError); an eager materialize-then-take RAISES.
#guard ((evalI false 10 (.takewhile (.fdiv (.lit 1) (.var "x")) "x"
        (.fromList (.listE [.lit 2, .lit 0]))) []).bind valsAsInts) = some []
#guard (takewhileI false 10 (.fdiv (.lit 1) (.var "x")) "x" [.vint 2, .vint 0] []).bind valsAsInts = some []
-- F9 CPython LIST-truthiness pin (domain-completeness in the takewhile
-- PREDICATE): takewhile(lambda x: x, [[1], []]) == [[1]] — a nonempty-list
-- predicate value is TRUTHY (kept), the empty list FALSY (stop) — NOT an
-- error (which is what the retired int-only truthiness modeled).
#guard (takewhileI false 10 (.var "x") "x" [.vlist [.vint 1], .vlist []] []).bind valsAsIntLists
        = some [[1]]

/-! ### Wave 6 (C1 rollout) — INDEPENDENT-target itertools preservation

The previous `preservationI : evalI true fuel e env = evalI false fuel e env`
(with its mutual flag worker `preservationI'`) and
`takewhileI_preservation : takewhileI true … = takewhileI false …` were the
same F1 model-vs-model tautology as the old `preservationE`: `Bool`-flagged
copies of ONE evaluator, whose proofs moreover routed the embedded `GExp`
sub-terms through the wave-5 TEMPORARY `evalG_flag_bridge` glue.
Re-architected on the wave-1..5 recipe: `evalItgt`/`evalIstgt` are a SEPARATE
fuel-indexed mutual recursion parameterized by the integer-division lowering,
and every embedded `GExp` sub-term (a `fromList` source, a `takewhile`
predicate) evaluates via the INDEPENDENT `evalGtgt L` — so the preservation
proofs discharge those obligations by the GENUINE `preservationG`, and the
wave-5 glue is DELETED (this wave was its only consumer; grep-confirmed). The
lazy `takewhile` target `takewhileItgt` is likewise independent AND still
genuinely lazy (collects the truthy prefix, stops at the first non-truthy
predicate value, never evaluates the predicate past it); truthiness on BOTH
source and target is the shared verified-core `valTruthy` over the FULL `Val`
domain (F9 domain-completeness — no int-only match, no shared-wrong `rfl` on
`.vlist`). The predicates are proved for the shipped floor-correction
(`preservationI_real`, `takewhileI_preservation_real`) and REFUTED for the
truncating lowering on a DISCRIMINATING witness where the deviation flips the
takewhile predicate's TRUTHINESS — kept-ness itself differs
(`preservationI_stub_fails`, `takewhileI_stub_fails`) — and, on the laziness
axis, for the WRONG EAGER materialize-then-take shape even under the CORRECT
arithmetic (`takewhileI_eager_stub_fails`), so the laziness content is
non-vacuous, not just the arithmetic. -/

/-- **Independent target** for the lazy `takewhile` worker: the compiled
    combinator under lowering `L`. STILL genuinely lazy — same short-circuit
    shape as the reference (stops at the first non-truthy predicate value and
    never evaluates the predicate on the rest) — but predicate evals route
    through the INDEPENDENT `evalGtgt L`, not the flag. Truthiness is the
    shared verified-core `valTruthy` over the FULL `Val` domain, exactly as on
    the source (the LOWERING is what varies, not truthiness). -/
def takewhileItgt (L : IntDivLowering) : Nat → GExp → String → List Val → VEnv → Option (List Val)
  | 0, _, _, _, _ => none
  | _ + 1, _, _, [], _ => some []
  | fuel + 1, pred, v, x :: xs, env =>
      match evalGtgt L fuel pred ((v, x) :: env) with
      | some val =>
          if valTruthy val then (takewhileItgt L fuel pred v xs env).map (x :: ·)
          else some []
      | none => none
termination_by n _ => n

mutual
/-- **Independent target evaluator** (itertools fragment): the compiled
    program's semantics under lowering `L`. A SEPARATE fuel-indexed mutual
    recursion (not the `Bool` flag on `evalI`); embedded `GExp` sub-terms —
    `fromList` sources and (via `takewhileItgt`) `takewhile` predicates —
    evaluate via the INDEPENDENT `evalGtgt L`, exactly as in the emitted JS;
    every combinator itself is structural over materialized values. -/
def evalItgt (L : IntDivLowering) : Nat → IExp → VEnv → Option (List Val)
  | 0, _, _ => none
  | _ + 1, .count start step n, _ => some (countList start step n)
  | fuel + 1, .fromList g, env =>
      match evalGtgt L fuel g env with
      | some (.vlist xs) => some xs
      | _ => none
  | fuel + 1, .islice src a b s, env =>
      if s = 0 then none
      else (evalItgt L fuel src env).map (fun xs => isliceList xs a b s)
  | fuel + 1, .takewhile pred v src, env =>
      (evalItgt L fuel src env).bind (fun xs => takewhileItgt L fuel pred v xs env)
  | fuel + 1, .chain parts, env => (evalIstgt L fuel parts env).map List.flatten
  | fuel + 1, .product a b, env =>
      match evalItgt L fuel a env, evalItgt L fuel b env with
      | some xs, some ys => some (productList xs ys)
      | _, _ => none
  | fuel + 1, .permutations src r, env =>
      (evalItgt L fuel src env).map (fun xs => (permutationsList r xs).map .vlist)
  | fuel + 1, .combinations src r, env =>
      (evalItgt L fuel src env).map (fun xs => (combinationsList r xs).map .vlist)
termination_by n _ => n
/-- Independent-target eval of a list of itertools sources (for `chain`). -/
def evalIstgt (L : IntDivLowering) : Nat → List IExp → VEnv → Option (List (List Val))
  | 0, _, _ => none
  | _ + 1, [], _ => some []
  | fuel + 1, e :: es, env =>
      match evalItgt L fuel e env, evalIstgt L fuel es env with
      | some xs, some xss => some (xs :: xss)
      | _, _ => none
termination_by n _ => n
end

/-- The compiled itertools semantics: the independent target under the SHIPPED
    lowering. -/
abbrev evalIjs : Nat → IExp → VEnv → Option (List Val) := evalItgt jsELowering

/-- The compiled lazy-`takewhile` semantics: the independent target under the
    SHIPPED lowering. -/
abbrev takewhileIjs : Nat → GExp → String → List Val → VEnv → Option (List Val) :=
  takewhileItgt jsELowering

/-- Itertools preservation as a predicate OVER the lowering — the SAME
    predicate is proved for the shipped lowering (`preservationI_real`) and
    REFUTED for the stub (`preservationI_stub_fails`). Quantifies over the
    fuel too: a lowering preserves only if it does so at EVERY depth. -/
def IPreserves (L : IntDivLowering) : Prop :=
  ∀ fuel e env, evalItgt L fuel e env = evalI false fuel e env

/-- Lazy-`takewhile` preservation as a predicate OVER the compiled combinator
    implementation — the SAME predicate is proved for the shipped lazy target
    (`takewhileI_preservation_real`) and REFUTED on BOTH failure axes: the
    wrong ARITHMETIC lowering (`takewhileI_stub_fails`, kept-ness flipped by
    trunc-`//`) and the wrong EAGER shape (`takewhileI_eager_stub_fails`, the
    #348/#155 materialize-first bug shape on `takewhile`). -/
def TakewhileIPreserves (impl : Nat → GExp → String → List Val → VEnv → Option (List Val)) : Prop :=
  ∀ fuel pred v xs env, impl fuel pred v xs env = takewhileI false fuel pred v xs env

/-- The WRONG EAGER `takewhile` lowering — the #348/#155 materialize-then-fold
    bug shape transplanted to `takewhile`: evaluate the predicate on EVERY
    element first (`evalComptgt` with the predicate as body and no filter),
    then take the truthy prefix of the zipped pairs. Truthiness is the SAME
    total `valTruthy` as the reference, so the ONLY difference is EAGERNESS: a
    later element whose predicate ERRORS poisons the whole call where Python's
    lazy `takewhile` has already stopped. Used ONLY to REFUTE the preservation
    predicate (`takewhileI_eager_stub_fails`) — this is what makes the
    laziness content of `takewhileI_preservation` non-vacuous. -/
def takewhileIeagerTgt (L : IntDivLowering) : Nat → GExp → String → List Val → VEnv → Option (List Val)
  | 0, _, _, _, _ => none
  | fuel + 1, pred, v, xs, env =>
      (evalComptgt L fuel pred v none xs env).map (fun ks =>
        ((xs.zip ks).takeWhile (fun p => valTruthy p.2)).map Prod.fst)

-- Compiled-side guards (the retired `evalI true` guard, now on the genuine
-- independent target), plus the discriminating stub / laziness contrasts.
#guard ((evalIjs 50 (.takewhile (.lt (.fdiv (.sub (.var "x") (.lit 3)) (.lit 2)) (.lit 1)) "x"
        (.count 0 1 6)) []).bind valsAsInts) = some [0, 1, 2, 3, 4]
-- DISCRIMINATING deviation-in-PREDICATE contrast on the independent targets:
-- takewhile(x // 2, [-1]) — the shipped floor lowering KEEPS the element
-- (predicate value -1 // 2 = -1, truthy → [-1]); the truncating stub STOPS
-- (trunc 0, falsy → []). KEPT-NESS itself differs between the lowerings.
#guard ((evalIjs 10 (.takewhile (.fdiv (.var "x") (.lit 2)) "x"
        (.fromList (.listE [.lit (-1)]))) []).bind valsAsInts) = some [-1]    -- floor: kept ✓
#guard ((evalItgt truncELowering 10 (.takewhile (.fdiv (.var "x") (.lit 2)) "x"
        (.fromList (.listE [.lit (-1)]))) []).bind valsAsInts) = some []      -- trunc stub: dropped ✗
-- LAZINESS contrast: the shipped lazy target stops at the first falsy
-- predicate and never evaluates 1 // 0; the eager shape errors.
#guard (takewhileIjs 10 (.fdiv (.lit 1) (.var "x")) "x" [.vint 2, .vint 0] []).bind valsAsInts
        = some []                                                             -- lazy ✓
#guard (takewhileIeagerTgt jsELowering 10 (.fdiv (.lit 1) (.var "x")) "x" [.vint 2, .vint 0] []).bind
        valsAsInts = none                                                     -- eager ✗
-- LIST truthiness in the takewhile predicate on the independent target:
-- takewhile(lambda x: x, [[1], []]) == [[1]] (was an ERROR on the retired
-- int-only truthiness).
#guard (takewhileIjs 10 (.var "x") "x" [.vlist [.vint 1], .vlist []] []).bind valsAsIntLists
        = some [[1]]

/-- `takewhile` worker preservation (C1-rollout wave 6, re-architected): the
    INDEPENDENT compiled lazy prefix-collection under the shipped lowering
    agrees with the Python reference — the predicate evals route through the
    GENUINE `preservationG` (the wave-5 glue is deleted), the recursion
    through the fuel IH (the `anyG_preservation` shape, collecting instead of
    folding). After routing the predicate eval, BOTH sides branch on
    `valTruthy val` over the FULL `Val` domain — no per-constructor split,
    hence no shared-wrong `rfl` on `.vlist`. -/
theorem takewhileI_preservation (fuel : Nat) (pred : GExp) (v : String)
    (xs : List Val) (env : VEnv) :
    takewhileIjs fuel pred v xs env = takewhileI false fuel pred v xs env := by
  induction fuel generalizing xs env with
  | zero => simp only [takewhileItgt, takewhileI]
  | succ fuel ih =>
    cases xs with
    | nil => simp only [takewhileItgt, takewhileI]
    | cons x xs =>
      simp only [takewhileItgt, takewhileI, preservationG fuel pred ((v, x) :: env)]
      cases evalG false fuel pred ((v, x) :: env) with
      | none => rfl
      | some val => simp only [ih]

/-- Combined preservation worker for the two mutual INDEPENDENT itertools
    evaluators: ONE induction on `fuel` carries both statements (uniform
    fuel). Binds the INDEPENDENT target under the shipped lowering to the
    Python reference `evalI false`: only the embedded `GExp` sub-evaluations
    are lowering-sensitive — the GENUINE `preservationG` closes the `fromList`
    source, `takewhileI_preservation` the lazy predicate — and every
    combinator arm is structural over materialized values. -/
private theorem preservationI'tgt (fuel : Nat) :
    (∀ e env, evalItgt jsELowering fuel e env = evalI false fuel e env) ∧
    (∀ es env, evalIstgt jsELowering fuel es env = evalIs false fuel es env) := by
  induction fuel with
  | zero =>
    exact ⟨fun e env => by simp only [evalItgt, evalI],
           fun es env => by simp only [evalIstgt, evalIs]⟩
  | succ fuel ih =>
    obtain ⟨ihI, ihIs⟩ := ih
    refine ⟨fun e env => ?_, fun es env => ?_⟩
    · cases e with
      | count start step n => simp only [evalItgt, evalI]
      | fromList g => simp only [evalItgt, evalI, preservationG fuel g env]
      | islice src a b s => simp only [evalItgt, evalI, ihI]
      | takewhile pred v src => simp only [evalItgt, evalI, ihI, takewhileI_preservation]
      | chain parts => simp only [evalItgt, evalI, ihIs]
      | product a b => simp only [evalItgt, evalI, ihI]
      | permutations src r => simp only [evalItgt, evalI, ihI]
      | combinations src r => simp only [evalItgt, evalI, ihI]
    · cases es with
      | nil => simp only [evalIstgt, evalIs]
      | cons e es => simp only [evalIstgt, evalIs, ihI, ihIs]

/-- **Itertools preservation (Tier-3 wave 7 / C1-rollout wave 6,
    re-architected).** The INDEPENDENT compiled target under the shipped
    lowering computes the Python reference on every itertools program — the
    lazy core `count`/`islice`/`takewhile`/`chain` and the combinatorial
    `product`/`permutations`/`combinations`, with `//`-carrying `GExp`
    sub-expressions as sources and lazy predicates — at every fuel. -/
theorem preservationI (fuel : Nat) (e : IExp) (env : VEnv) :
    evalIjs fuel e env = evalI false fuel e env :=
  (preservationI'tgt fuel).1 e env

/-- The re-architected statement, in predicate form: the shipped lowering
    preserves itertools programs. The instantiation the stub litmus contrasts
    against. -/
theorem preservationI_real : IPreserves jsELowering := preservationI

/-- **Stub litmus (wave 6, itertools side).** The SAME preservation predicate
    is FALSE for the naive truncating lowering, on a DISCRIMINATING deviating
    itertools program whose `//` sits inside a LAZY `takewhile` predicate:
    `takewhile(x // 2, [-1])` — the deviation FLIPS the predicate's
    TRUTHINESS, so KEPT-NESS itself differs: stub JS-trunc `-1 // 2 = 0`
    (falsy → `[]`, element dropped) where Python floors to `-1` (truthy →
    `[-1]`, element kept). Exercises BOTH the arithmetic path (through the
    `fromList`-sourced `GExp` machinery) and the truthiness path. -/
theorem preservationI_stub_fails : ¬ IPreserves truncELowering := by
  intro h
  have hc := h 10 (.takewhile (.fdiv (.var "x") (.lit 2)) "x"
      (.fromList (.listE [.lit (-1)]))) []
  -- hc reduces through `some [] = some [.vint (-1)]` to `False`.
  simp [evalItgt, takewhileItgt, evalI, takewhileI,
        evalGtgt, evalGstgt, evalG, evalGs, valTruthy, truncELowering, VEnv.get] at hc

/-- The re-architected lazy statement, in predicate form: the shipped LAZY
    compiled `takewhile` combinator preserves. The instantiation the two stub
    litmuses contrast against. -/
theorem takewhileI_preservation_real : TakewhileIPreserves takewhileIjs :=
  takewhileI_preservation

/-- **Stub litmus (wave 6, `takewhile` arithmetic axis).** The SAME predicate
    is FALSE for the lazy combinator under the TRUNCATING lowering: on
    `takewhile(x // 2, [-1])` the deviation FLIPS TRUTHINESS — stub JS-trunc
    `-1 // 2 = 0` (falsy → stop, `[]`) where Python floors to `-1` (truthy →
    keep, `[-1]`). -/
theorem takewhileI_stub_fails : ¬ TakewhileIPreserves (takewhileItgt truncELowering) := by
  intro h
  have hc := h 10 (.fdiv (.var "x") (.lit 2)) "x" [.vint (-1)] []
  -- hc reduces through `some [] = some [.vint (-1)]` to `False`.
  simp [takewhileItgt, takewhileI, evalGtgt, evalG, valTruthy, truncELowering, VEnv.get] at hc

/-- **Stub litmus (wave 6, LAZINESS axis).** The SAME predicate is FALSE for
    the EAGER materialize-then-take shape even under the SHIPPED
    (arithmetically correct) lowering: on `takewhile(1 // x, [2, 0])` the
    eager stub evaluates `1 // 0` (error → `none`) where the lazy reference
    has already STOPPED at `x = 2` (predicate `1 // 2 = 0` falsy → `[]`) and
    never touches the rest. The #348/#155 bug shape refuted on `takewhile` —
    the laziness content of `takewhileI_preservation` is non-vacuous, not just
    its arithmetic. -/
theorem takewhileI_eager_stub_fails : ¬ TakewhileIPreserves (takewhileIeagerTgt jsELowering) := by
  intro h
  have hc := h 10 (.fdiv (.lit 1) (.var "x")) "x" [.vint 2, .vint 0] []
  -- hc reduces through `none = some []` to `False` (the second element's
  -- div-by-zero poisons the eager materialization; the lazy reference stopped
  -- at the first falsy predicate value and never evaluated it).
  simp [takewhileIeagerTgt, takewhileI, evalComptgt, evalGtgt, evalG, valTruthy,
        jsELowering, VEnv.get] at hc

/-- SPOT (through the theorem, not by evaluation): the `compose_lazy` program
    `takewhile(x < 100, count(1, 7))` (bounded at 20 source terms) in the
    INDEPENDENT compiled semantics, routed THROUGH `preservationI` to the
    Python reference and evaluated there to CPython's 15-element answer.
    Fails if the statement is weakened back to model-vs-model. -/
example :
    (evalIjs 50 (.takewhile (.lt (.var "x") (.lit 100)) "x" (.count 1 7 20)) []).bind valsAsInts
      = some [1, 8, 15, 22, 29, 36, 43, 50, 57, 64, 71, 78, 85, 92, 99] := by
  rw [preservationI]
  simp [evalI, takewhileI, countList, evalG, valTruthy, VEnv.get, valsAsInts, Val.asInt]

/-- SPOT: a DEVIATING itertools program — `takewhile(x // 2, [-1])`, the `//`
    inside the LAZY predicate — the INDEPENDENT compiled value keeps the
    element (Python floor `-1 // 2 = -1`, truthy), derived via
    `preservationI` (exercises the deviation through the takewhile predicate
    + the `fromList` source; the truncating stub drops it, see
    `preservationI_stub_fails`). -/
example :
    (evalIjs 10 (.takewhile (.fdiv (.var "x") (.lit 2)) "x"
        (.fromList (.listE [.lit (-1)]))) []).bind valsAsInts
      = some [-1] := by
  rw [preservationI]
  simp [evalI, takewhileI, evalG, evalGs, valTruthy, VEnv.get, valsAsInts, Val.asInt]

/-- SPOT (LAZINESS observable, through `takewhileI_preservation`):
    `takewhile(1 // x, [2, 0])` in the INDEPENDENT compiled semantics — the
    predicate at `x = 2` is `1 // 2 = 0`, falsy, so the compiled combinator
    STOPS with `[]` and never evaluates `1 // 0` — derived via the theorem,
    so it fails if the statement is weakened back to model-vs-model OR if the
    compiled target stops being lazy (the eager shape yields `none` here,
    not `some []`). -/
example : takewhileIjs 10 (.fdiv (.lit 1) (.var "x")) "x" [.vint 2, .vint 0] [] = some [] := by
  rw [takewhileI_preservation]
  simp [takewhileI, evalG, valTruthy, VEnv.get]

/-- SPOT (LIST truthiness through `takewhileI_preservation`):
    `takewhile(lambda x: x, [[1], []])` — the first predicate value is a
    NONEMPTY LIST, truthy (kept); the second is the EMPTY list, falsy (stop)
    — computes `[[1]]` in the INDEPENDENT compiled semantics, derived via the
    theorem. Fails on an int-only-truthiness model, where a list-valued
    predicate was an ERROR (`none`). -/
example :
    takewhileIjs 10 (.var "x") "x" [.vlist [.vint 1], .vlist []] []
      = some [.vlist [.vint 1]] := by
  rw [takewhileI_preservation]
  simp [takewhileI, evalG, valTruthy, VEnv.get]

/-- info: 'PythExpandVerify.takewhileI_preservation' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms takewhileI_preservation

/-- info: '_private.PythExpandVerify.0.PythExpandVerify.preservationI'tgt' depends on axioms: [propext,
 Classical.choice,
 Quot.sound] -/
#guard_msgs in
#print axioms preservationI'tgt

/-- info: 'PythExpandVerify.preservationI' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationI

/-- info: 'PythExpandVerify.preservationI_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationI_real

/-- info: 'PythExpandVerify.preservationI_stub_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationI_stub_fails

/-- info: 'PythExpandVerify.takewhileI_preservation_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms takewhileI_preservation_real

/-- info: 'PythExpandVerify.takewhileI_stub_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms takewhileI_stub_fails

/-- info: 'PythExpandVerify.takewhileI_eager_stub_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms takewhileI_eager_stub_fails

/-! ## Tier-3 wave 8 — WASM numeric preservation (the i64 width deviation)

First VALUE-preservation result on the WASM backend, which so far carries only
the admission certificate (routing correctness, no value theorem). The
deviation is the value-WIDTH axis, exactly analogous to the seed's `//`
deviation on the operator axis: Python ints are arbitrary-precision, but the
WASM numeric fast path computes in fixed-width `i64` — `i64.add`/`i64.sub`/
`i64.mul` are two's-complement and wrap at 2^63. `wrapI64` is the signed wrap
into `[-2^63, 2^63)` (balanced mod, `Int.bmod _ (2^64)`), with its explicit
two's-complement characterization, range, congruence, and in-range-identity
proved. `evalPyW` is the Python reference; `evalW` is the i64 machine —
literals emitted as truncated `i64.const`s, environment loads reading the i64
image of the stored int, every arithmetic result wrapped. The STRONG form
holds with NO precondition: `evalW e env = (evalPyW e env).map wrapI64` — the
WASM result is EXACTLY the wrap of the Python value, i.e. the i64 fast path
is a faithful implementation of wrapping semantics. The proof content is that
`wrapI64` commutes with `+`/`-`/`×`/negation (`Int.add_bmod`/`sub_bmod`/
`mul_bmod`); the value-EQUALITY corollary `preservationWasm_inRange` then
recovers `evalW = evalPyW` whenever the program's Python VALUE is i64-
representable — intermediates may overflow freely, since wrapping is a
mod-2^64 homomorphism. `//` is deliberately NOT in this fragment: `i64.div_s`
truncates and division does not commute with wrapping, so the WASM div path
is a separate (guarded) obligation, not a silent extension of this theorem.
NAMING NOTE: the headline theorem is `preservationWasm`, not `preservationW` —
wave 2's private fuel-indexed while-loop lemma already occupies the name
`preservationW` in this namespace and Lean forbids re-declaring it. -/

/-- Signed two's-complement wrap into `[-2^63, 2^63)`: the value an `i64`
    register holds for the Python int `n`. Balanced mod: `Int.bmod n (2^64)`. -/
def wrapI64 (n : Int) : Int := Int.bmod n (2 ^ 64)

-- executable pins of the wrap itself: the positive boundary (2^63 → -2^63),
-- full-width wrap (2^64 → 0), identity inside the range, negative boundary.
#guard wrapI64 (2 ^ 63) = -(2 ^ 63)
#guard wrapI64 (2 ^ 63 - 1) = 2 ^ 63 - 1
#guard wrapI64 (2 ^ 62 * 4) = 0
#guard wrapI64 (-(2 ^ 63) - 1) = 2 ^ 63 - 1
#guard wrapI64 (-7) = -7

/-- Characterization: `wrapI64` IS the explicit two's-complement formula
    `((n + 2^63) mod 2^64) - 2^63`. -/
theorem wrapI64_def_emod (n : Int) : wrapI64 n = (n + 2 ^ 63) % 2 ^ 64 - 2 ^ 63 := by
  simp only [wrapI64, Int.bmod_def]
  omega

/-- Characterization: the wrap lands in the signed-64-bit range … -/
theorem wrapI64_range (n : Int) : -(2 ^ 63) ≤ wrapI64 n ∧ wrapI64 n < 2 ^ 63 := by
  simp only [wrapI64, Int.bmod_def]
  omega

/-- … is congruent to `n` mod 2^64 (same residue class — the wrap forgets only
    the high bits) … -/
theorem wrapI64_congr (n : Int) : (2 ^ 64 : Int) ∣ wrapI64 n - n := by
  simp only [wrapI64, Int.bmod_def]
  omega

/-- … and is the identity on in-range values (no spurious wrapping). -/
theorem wrapI64_eq_self {n : Int} (h1 : -(2 ^ 63) ≤ n) (h2 : n < 2 ^ 63) :
    wrapI64 n = n := by
  simp only [wrapI64, Int.bmod_def]
  omega

/-- The key commuting lemma: wrapping the sum of WRAPPED operands equals
    wrapping the exact sum — `i64.add` on i64 images implements wrapping
    semantics of the true addition. -/
theorem wrapI64_add (a b : Int) : wrapI64 (wrapI64 a + wrapI64 b) = wrapI64 (a + b) :=
  (Int.add_bmod a b (2 ^ 64)).symm

/-- `wrapI64` commutes with subtraction (`i64.sub`). -/
theorem wrapI64_sub (a b : Int) : wrapI64 (wrapI64 a - wrapI64 b) = wrapI64 (a - b) :=
  (Int.sub_bmod a b (2 ^ 64)).symm

/-- `wrapI64` commutes with multiplication (`i64.mul`). -/
theorem wrapI64_mul (a b : Int) : wrapI64 (wrapI64 a * wrapI64 b) = wrapI64 (a * b) :=
  (Int.mul_bmod a b (2 ^ 64)).symm

/-- `wrapI64` commutes with negation (`i64.sub` from 0). -/
theorem wrapI64_neg (a : Int) : wrapI64 (-(wrapI64 a)) = wrapI64 (-a) := by
  have h := Int.sub_bmod 0 a (2 ^ 64)
  simp only [Int.zero_sub, Int.zero_bmod] at h
  exact h.symm

inductive WExp where
  | lit (n : Int)
  | var (s : String)
  | neg (a : WExp)
  | add (a b : WExp)
  | sub (a b : WExp)
  | mul (a b : WExp)
deriving Repr

/-- Python reference semantics: arbitrary-precision ints (`none` = unbound
    variable). Structural recursion — no fuel needed on a finite tree. -/
def evalPyW : WExp → Env → Option Int
  | .lit n, _ => some n
  | .var s, env => env.get s
  | .neg a, env => (evalPyW a env).map (fun x => -x)
  | .add a b, env => match evalPyW a env, evalPyW b env with
      | some x, some y => some (x + y) | _, _ => none
  | .sub a b, env => match evalPyW a env, evalPyW b env with
      | some x, some y => some (x - y) | _, _ => none
  | .mul a b, env => match evalPyW a env, evalPyW b env with
      | some x, some y => some (x * y) | _, _ => none

/-- WASM i64 fast-path semantics: literals are emitted as truncated
    `i64.const`s, environment loads read the i64 image of the stored int, and
    every arithmetic instruction wraps its result to signed 64-bit. -/
def evalW : WExp → Env → Option Int
  | .lit n, _ => some (wrapI64 n)
  | .var s, env => (env.get s).map wrapI64
  | .neg a, env => (evalW a env).map (fun x => wrapI64 (-x))
  | .add a b, env => match evalW a env, evalW b env with
      | some x, some y => some (wrapI64 (x + y)) | _, _ => none
  | .sub a b, env => match evalW a env, evalW b env with
      | some x, some y => some (wrapI64 (x - y)) | _, _ => none
  | .mul a b, env => match evalW a env, evalW b env with
      | some x, some y => some (wrapI64 (x * y)) | _, _ => none

-- the deviation exercised: at and past the i64 boundary the two evaluators
-- genuinely DIFFER — and they differ by exactly wrapI64; in range they agree.
#guard evalPyW (.lit (2 ^ 63)) [] = some (2 ^ 63)
#guard evalW (.lit (2 ^ 63)) [] = some (-(2 ^ 63))                 -- 2^63 wraps to -2^63
#guard evalPyW (.add (.lit (2 ^ 62)) (.lit (2 ^ 62))) [] = some (2 ^ 63)
#guard evalW (.add (.lit (2 ^ 62)) (.lit (2 ^ 62))) [] = some (-(2 ^ 63))  -- overflow at the add
#guard evalPyW (.mul (.lit (2 ^ 62)) (.lit 4)) [] = some (2 ^ 64)
#guard evalW (.mul (.lit (2 ^ 62)) (.lit 4)) [] = some 0           -- 2^62·4 = 2^64 wraps to 0
#guard evalW (.mul (.lit (2 ^ 62)) (.lit 4)) [] = (evalPyW (.mul (.lit (2 ^ 62)) (.lit 4)) []).map wrapI64
#guard evalW (.var "x") [("x", 2 ^ 64 + 7)] = some 7               -- i64 load truncates the stored int
#guard evalPyW (.var "x") [("x", 2 ^ 64 + 7)] = some (2 ^ 64 + 7)
#guard evalW (.sub (.mul (.lit 3) (.lit 4)) (.lit 20)) [] = some (-8)   -- in-range: both agree
#guard evalPyW (.sub (.mul (.lit 3) (.lit 4)) (.lit 20)) [] = some (-8)

/-- **WASM numeric preservation (Tier-3 wave 8), strong form.** For EVERY
    expression and environment, the i64 result is exactly the Python result
    wrapped to signed 64 bits — no in-range precondition. The WASM numeric
    fast path is a faithful implementation of wrapping semantics; the content
    is that `wrapI64` commutes with each arithmetic operator. -/
theorem preservationWasm (e : WExp) (env : Env) :
    evalW e env = (evalPyW e env).map wrapI64 := by
  induction e with
  | lit n => rfl
  | var s => rfl
  | neg a ih =>
    simp only [evalW, evalPyW, ih]
    cases evalPyW a env with
    | none => rfl
    | some x => simp only [Option.map_some, wrapI64_neg]
  | add a b iha ihb =>
    simp only [evalW, evalPyW, iha, ihb]
    cases evalPyW a env with
    | none => rfl
    | some x =>
      cases evalPyW b env with
      | none => rfl
      | some y => simp only [Option.map_some, wrapI64_add]
  | sub a b iha ihb =>
    simp only [evalW, evalPyW, iha, ihb]
    cases evalPyW a env with
    | none => rfl
    | some x =>
      cases evalPyW b env with
      | none => rfl
      | some y => simp only [Option.map_some, wrapI64_sub]
  | mul a b iha ihb =>
    simp only [evalW, evalPyW, iha, ihb]
    cases evalPyW a env with
    | none => rfl
    | some x =>
      cases evalPyW b env with
      | none => rfl
      | some y => simp only [Option.map_some, wrapI64_mul]

/-- Value-EQUALITY corollary (the bounded reading, DERIVED, not assumed): if
    the program's Python VALUE is i64-representable, the WASM result IS the
    Python result — intermediates may overflow freely, since wrapping is a
    mod-2^64 homomorphism and only the final value's range matters. -/
theorem preservationWasm_inRange (e : WExp) (env : Env) (v : Int)
    (hv : evalPyW e env = some v) (h1 : -(2 ^ 63) ≤ v) (h2 : v < 2 ^ 63) :
    evalW e env = evalPyW e env := by
  rw [preservationWasm, hv, Option.map_some, wrapI64_eq_self h1 h2]

/-- SPOT: the OVERFLOWING program `2^62 * 4` in the WASM (i64) semantics,
    routed THROUGH `preservationWasm` to the Python reference: the i64 result
    is exactly `wrapI64` of Python's `2^64` (= 0, per the `#guard` above).
    Could not close if the theorem were silently weakened — a both-sides-`none`
    version leaves the `some` goal unprovable. -/
example : evalW (.mul (.lit (2 ^ 62)) (.lit 4)) [] = some (wrapI64 (2 ^ 64)) := by
  rw [preservationWasm]
  decide

/-- SPOT: the boundary program `2^62 + 2^62` in the WASM semantics, routed
    THROUGH the theorem: Python's `2^63` lands wrapped at exactly `-2^63` —
    the deviation value itself. -/
example : evalW (.add (.lit (2 ^ 62)) (.lit (2 ^ 62))) [] = some (-(2 ^ 63)) := by
  rw [preservationWasm]
  decide

/-- info: 'PythExpandVerify.preservationWasm' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationWasm

/-- info: 'PythExpandVerify.wrapI64_add' depends on axioms: [propext] -/
#guard_msgs in
#print axioms wrapI64_add

/-! ## Tier-3 wave 9 — dictionaries as values (dict literals + key lookup)

Dicts become first-class values. The layer is SELF-CONTAINED (its own
`DVal`/`DEnv`, exactly as waves 4/6/7 defined their own layers) so the frozen
`Val`/`CExp` of waves 5/6/7 stay untouched. `DVal` is `dint` | `ddict`
(int-keyed entry list, kept in literal order). A dict LITERAL
`{k1: v1, k2: v2, …}` evaluates its value expressions left-to-right; LOOKUP
`d[k]` follows CPython: missing key → `none` (modeling `KeyError`), and
duplicate keys in a literal resolve LAST-write-wins (`{1:1, 1:2}[1] = 2`),
which `dictLookup` realizes by giving LATER entries priority. The `//`
deviation enters through the value expressions (and the lookup-key
expression), so `preservationD` is non-vacuous — the `fdiv` case is closed by
`jsFdiv_eq_fdiv` exactly as in wave 5; literal construction + lookup are
lowering-independent (structural).

**C1-rollout wave 7 (re-architected).** The previous
`preservationD : evalD true e env = evalD false e env` (with its mutual
workers `preservationD'`/`preservationDs'`) was the F1 model-vs-model
tautology: a `Bool`-flagged copy of ONE mutual evaluator pair, deviating only
in the `//` arm — stubbing the shipping lowering could not break it.
Re-architected on the wave-1/2/3/4 recipe: `evalDtgt`/`evalDstgt` are a
SEPARATE mutual recursion, parameterized by the integer-division lowering, so
`//` threads through dict VALUES and lookup KEYS exactly as in the emitted
JS; both `DVal` constructors (`dint`/`ddict`) are handled faithfully on both
sides — arithmetic on a `ddict` operand is `none` (CPython `TypeError`),
lookup requires a `ddict` base and a `dint` key, and both sides share only
the verified pure-data `dictLookup` (last-write-wins), never a lowering. The
same predicates (`DPreserves`/`DsPreserves`) are TRUE for the shipped
floor-correction (`preservationD`/`preservationDs`) and provably FALSE for
the naive truncating lowering (`preservationD_stub_fails` — value path — and
`preservationDs_stub_fails` — entry-list level); a lookup-KEY deviation is
pinned too (`{-4: 99}[-7//2]`: floor key HITS, trunc key MISSES → KeyError).

The D1 float-key deviation (issue #345 — whole-float key `0.0` and int key `0`
share one slot) is deliberately NOT modeled here: Lean's `Float` is opaque to
proofs, and an Int-embedded stand-in for "whole float" would make the
normalization lemma definitionally trivial (vacuous content). Int-keyed dicts
are the core deliverable; D1 stays a documented deviation. -/

inductive DVal where
  | dint (n : Int)
  | ddict (entries : List (Int × DVal))
deriving Repr

/-- Project a `DVal` to its int (for `#guard`s; avoids a nested `DecidableEq`). -/
def DVal.asInt : DVal → Option Int
  | .dint n => some n
  | .ddict _ => none

abbrev DEnv := List (String × DVal)

def DEnv.get (env : DEnv) (n : String) : Option DVal :=
  (env.find? (fun p => p.1 == n)).map (·.2)

/-- CPython lookup on the literal-order entry list: LATER entries win (a
    duplicated literal key overwrites the earlier value), missing key → `none`
    (`KeyError`). Structural recursion, `simp`-friendly. -/
def dictLookup (k : Int) : List (Int × DVal) → Option DVal
  | [] => none
  | (k', v) :: rest =>
      match dictLookup k rest with
      | some v' => some v'
      | none => if k' = k then some v else none

inductive DExp where
  | lit (n : Int)
  | var (s : String)
  | add (a b : DExp) | sub (a b : DExp) | mul (a b : DExp) | fdiv (a b : DExp)
  | dict (entries : List (Int × DExp))   -- literal `{k1: v1, …}`, Int-literal keys
  | get (d k : DExp)                     -- Python `d[k]`
deriving Repr

mutual
/-- PYTHON-REFERENCE dict-valued expression eval (`evalD false` throughout;
    the `tgt = true` branch is documented LEGACY from the pre-rollout
    `Bool`-flag shape — NO theorem references it; the compiled semantics is
    the INDEPENDENT `evalDtgt` below). `tgt` affects only `//` (inside values
    and lookup keys); dict construction and `dictLookup` are
    target-independent. -/
def evalD (tgt : Bool) : DExp → DEnv → Option DVal
  | .lit n, _ => some (.dint n)
  | .var s, env => env.get s
  | .add a b, env => match evalD tgt a env, evalD tgt b env with
      | some (.dint x), some (.dint y) => some (.dint (x + y)) | _, _ => none
  | .sub a b, env => match evalD tgt a env, evalD tgt b env with
      | some (.dint x), some (.dint y) => some (.dint (x - y)) | _, _ => none
  | .mul a b, env => match evalD tgt a env, evalD tgt b env with
      | some (.dint x), some (.dint y) => some (.dint (x * y)) | _, _ => none
  | .fdiv a b, env => match evalD tgt a env, evalD tgt b env with
      | some (.dint x), some (.dint y) =>
          if y = 0 then none else some (.dint (if tgt then jsFdiv x y else Int.fdiv x y))
      | _, _ => none
  | .dict entries, env => (evalDs tgt entries env).map .ddict
  | .get d k, env => match evalD tgt d env, evalD tgt k env with
      | some (.ddict es), some (.dint kv) => dictLookup kv es
      | _, _ => none
termination_by e _ => sizeOf e
/-- Eval an entry list (for `dict` literals), preserving literal order. -/
def evalDs (tgt : Bool) : List (Int × DExp) → DEnv → Option (List (Int × DVal))
  | [], _ => some []
  | (k, e) :: rest, env => match evalD tgt e env, evalDs tgt rest env with
      | some v, some vs => some ((k, v) :: vs) | _, _ => none
termination_by es _ => sizeOf es
end

-- F9 pins: the REFERENCE `evalD false` is itself pinned to CPython dict
-- semantics (not merely to the target): `{1:10, 2:20}[2]` = 20; missing key →
-- KeyError (none); last-write-wins `{1:1, 1:2}[1]` = 2; nested
-- `{1: {2: 5}}[1][2]` = 5; the `//` deviation through a dict VALUE
-- (`{0: -7//2}[0]` = floor −4) and through a lookup KEY
-- (`{-4: 99}[-7//2]` = 99 — the floor key −4 HITS; trunc −3 would MISS).
#guard ((evalD false (.get (.dict [(1, .lit 10), (2, .lit 20)]) (.lit 2)) []).bind DVal.asInt) = some 20
#guard (evalD false (.get (.dict [(1, .lit 10), (2, .lit 20)]) (.lit 3)) []).isNone       -- KeyError
#guard ((evalD false (.get (.dict [(1, .lit 1), (1, .lit 2)]) (.lit 1)) []).bind DVal.asInt) = some 2
#guard ((evalD false (.get (.get (.dict [(1, .dict [(2, .lit 5)])]) (.lit 1)) (.lit 2)) []).bind DVal.asInt) = some 5
#guard ((evalD false (.get (.dict [(0, .fdiv (.lit (-7)) (.lit 2))]) (.lit 0)) []).bind DVal.asInt) = some (-4)
#guard ((evalD false (.get (.dict [(-4, .lit 99)]) (.fdiv (.lit (-7)) (.lit 2))) []).bind DVal.asInt) = some 99

mutual
/-- **Independent target evaluator** (dict expressions): the compiled
    program's semantics under lowering `L`. A SEPARATE mutual recursion (not
    the `Bool` flag on `evalD`); the `//` arm calls the lowering's operation,
    and it threads through dict VALUES (via `evalDstgt`) and lookup-KEY
    expressions exactly as in the emitted JS. Both `DVal` constructors are
    handled faithfully: arithmetic on a `ddict` operand is `none` (CPython
    `TypeError`), `get` demands a `ddict` base + `dint` key, and lookup is
    the shared pure-data `dictLookup` (last-write-wins, missing → `none`). -/
def evalDtgt (L : IntDivLowering) : DExp → DEnv → Option DVal
  | .lit n, _ => some (.dint n)
  | .var s, env => env.get s
  | .add a b, env => match evalDtgt L a env, evalDtgt L b env with
      | some (.dint x), some (.dint y) => some (.dint (x + y)) | _, _ => none
  | .sub a b, env => match evalDtgt L a env, evalDtgt L b env with
      | some (.dint x), some (.dint y) => some (.dint (x - y)) | _, _ => none
  | .mul a b, env => match evalDtgt L a env, evalDtgt L b env with
      | some (.dint x), some (.dint y) => some (.dint (x * y)) | _, _ => none
  | .fdiv a b, env => match evalDtgt L a env, evalDtgt L b env with
      | some (.dint x), some (.dint y) =>
          if y = 0 then none else some (.dint (L.fdiv x y))
      | _, _ => none
  | .dict entries, env => (evalDstgt L entries env).map .ddict
  | .get d k, env => match evalDtgt L d env, evalDtgt L k env with
      | some (.ddict es), some (.dint kv) => dictLookup kv es
      | _, _ => none
termination_by e _ => sizeOf e
/-- Independent-target entry-list eval (for `dict` literals): every VALUE
    expression routes through the same lowering `L`; literal order kept. -/
def evalDstgt (L : IntDivLowering) : List (Int × DExp) → DEnv → Option (List (Int × DVal))
  | [], _ => some []
  | (k, e) :: rest, env => match evalDtgt L e env, evalDstgt L rest env with
      | some v, some vs => some ((k, v) :: vs) | _, _ => none
termination_by es _ => sizeOf es
end

/-- The compiled dict-expression semantics: the independent target under the
    SHIPPED lowering. -/
abbrev evalDjs : DExp → DEnv → Option DVal := evalDtgt jsELowering

/-- The compiled entry-list semantics: the independent target under the
    SHIPPED lowering. -/
abbrev evalDsjs : List (Int × DExp) → DEnv → Option (List (Int × DVal)) := evalDstgt jsELowering

/-- Dict-expression preservation as a predicate OVER the lowering — the SAME
    predicate is proved for the shipped lowering (`preservationD_real`) and
    REFUTED for the stub (`preservationD_stub_fails`). -/
def DPreserves (L : IntDivLowering) : Prop :=
  ∀ e env, evalDtgt L e env = evalD false e env

/-- Entry-list preservation as a predicate OVER the lowering
    (`preservationDs_real` vs `preservationDs_stub_fails`). -/
def DsPreserves (L : IntDivLowering) : Prop :=
  ∀ es env, evalDstgt L es env = evalDs false es env

-- Compiled-side guards (the retired `evalD true` guards, now on the genuine
-- independent target): lookup, last-write-wins, nested dict value, and the
-- `//` deviation through a dict VALUE and through a lookup KEY.
#guard ((evalDjs (.get (.dict [(1, .lit 10), (2, .lit 20)]) (.lit 2)) []).bind DVal.asInt) = some 20
#guard ((evalDjs (.get (.dict [(1, .lit 1), (1, .lit 2)]) (.lit 1)) []).bind DVal.asInt) = some 2
#guard ((evalDjs (.get (.get (.dict [(1, .dict [(2, .lit 5)])]) (.lit 1)) (.lit 2)) []).bind DVal.asInt) = some 5
#guard ((evalDjs (.get (.dict [(0, .fdiv (.lit (-7)) (.lit 2))]) (.lit 0)) []).bind DVal.asInt) = some (-4)
#guard ((evalDjs (.get (.dict [(-4, .lit 99)]) (.fdiv (.lit (-7)) (.lit 2))) []).bind DVal.asInt) = some 99

mutual
/-- Expression-side dict-preservation worker (mutual with
    `preservationDs'tgt`): binds the INDEPENDENT target under the shipped
    lowering to the Python reference `evalD false` — real mutual structural
    induction, not a flag-vs-flag identity. The `.fdiv` arm is closed by
    `jsFdiv_eq_fdiv` on the `y ≠ 0` branch; the `ddict` operand cases agree
    because BOTH sides independently reject dict arithmetic (`none` =
    CPython `TypeError`), and the `get` arm agrees because both sides feed
    the SAME pure-data `dictLookup` after the sub-evals are bound by the IH. -/
private theorem preservationD'tgt (e : DExp) (env : DEnv) :
    evalDtgt jsELowering e env = evalD false e env := by
  match e with
  | .lit n => simp only [evalDtgt, evalD]
  | .var s => simp only [evalDtgt, evalD]
  | .add a b => simp only [evalDtgt, evalD, preservationD'tgt a env, preservationD'tgt b env]
  | .sub a b => simp only [evalDtgt, evalD, preservationD'tgt a env, preservationD'tgt b env]
  | .mul a b => simp only [evalDtgt, evalD, preservationD'tgt a env, preservationD'tgt b env]
  | .fdiv a b =>
    simp only [evalDtgt, evalD, preservationD'tgt a env, preservationD'tgt b env]
    cases evalD false a env with
    | none => rfl
    | some va =>
      cases evalD false b env with
      | none => cases va <;> rfl
      | some vb =>
        cases va with
        | ddict _ => rfl
        | dint x =>
          cases vb with
          | ddict _ => rfl
          | dint y =>
            by_cases hy : y = 0
            · simp [hy]
            · simp [hy, jsELowering, jsFdiv_eq_fdiv x y hy]
  | .dict entries => simp only [evalDtgt, evalD, preservationDs'tgt entries env]
  | .get d k => simp only [evalDtgt, evalD, preservationD'tgt d env, preservationD'tgt k env]
termination_by sizeOf e
decreasing_by all_goals (simp_wf <;> omega)

/-- Entry-list preservation worker (mutual with `preservationD'tgt`). -/
private theorem preservationDs'tgt (es : List (Int × DExp)) (env : DEnv) :
    evalDstgt jsELowering es env = evalDs false es env := by
  match es with
  | [] => simp only [evalDstgt, evalDs]
  | (k, e) :: rest =>
      simp only [evalDstgt, evalDs, preservationD'tgt e env, preservationDs'tgt rest env]
termination_by sizeOf es
decreasing_by all_goals (simp_wf <;> omega)
end

/-- **Dict preservation (Tier-3 wave 9 / C1-rollout wave 7,
    re-architected).** The INDEPENDENT compiled target under the shipped
    lowering computes the Python reference on every dict expression and
    environment — dict LITERALS (Int keys, arbitrary dict-valued expressions
    as values, last-write-wins on duplicates), CPython `d[k]` lookup (missing
    key → `KeyError`/`none`), and the `//` deviation threaded through dict
    VALUES and lookup KEYS. Real mutual structural induction binding the
    independent target to `evalD false` — NOT a flag-vs-flag identity. -/
theorem preservationD (e : DExp) (env : DEnv) : evalDjs e env = evalD false e env :=
  preservationD'tgt e env

/-- Entry-list analogue: the compiled evaluation of every dict-literal entry
    list matches the Python reference (the `evalDs` side of the mutual pair). -/
theorem preservationDs (es : List (Int × DExp)) (env : DEnv) :
    evalDsjs es env = evalDs false es env :=
  preservationDs'tgt es env

/-- The re-architected statement, in predicate form: the shipped lowering
    preserves dict expressions. Same content as `preservationD`; this is the
    instantiation the stub litmus contrasts against. -/
theorem preservationD_real : DPreserves jsELowering := preservationD

/-- Predicate form for entry lists: the instantiation
    `preservationDs_stub_fails` contrasts against. -/
theorem preservationDs_real : DsPreserves jsELowering := preservationDs

/-- **Stub litmus (dict wave, expression side).** The SAME preservation
    predicate is FALSE for the naive truncating lowering, on a deviating dict
    program: `{0: -7 // 2}[0]` stores a value the stub computes as JS-trunc
    `-3` and reads it back out by key lookup, where Python floors to `-4` — a
    concrete DISCRIMINATING contradiction (floor ≠ trunc at `-7 // 2`) the
    old `evalD true = evalD false` statement could not express. -/
theorem preservationD_stub_fails : ¬ DPreserves truncELowering := by
  intro h
  have hc := h (.get (.dict [(0, .fdiv (.lit (-7)) (.lit 2))]) (.lit 0)) []
  -- The evaluators are wf-recursive (not kernel-reducible): step them via
  -- their equation lemmas; hc reduces through `some (.dint (-3)) = some (.dint (-4))`
  -- (stub `Int.tdiv (-7) 2 = -3` vs Python `Int.fdiv (-7) 2 = -4`) to `False`.
  simp [evalDtgt, evalDstgt, evalD, evalDs, truncELowering, dictLookup] at hc

/-- **Stub litmus (dict wave, entry-list side).** The SAME entry-list
    predicate is FALSE for the truncating lowering: evaluating the literal
    entry list `{0: -7 // 2}` yields stub `[(0, -3)]` where Python yields
    `[(0, -4)]` — the dict VALUE itself diverges, before any lookup. -/
theorem preservationDs_stub_fails : ¬ DsPreserves truncELowering := by
  intro h
  have hc := h [(0, .fdiv (.lit (-7)) (.lit 2))] []
  -- hc reduces through `some [(0, .dint (-3))] = some [(0, .dint (-4))]` to `False`.
  simp [evalDtgt, evalDstgt, evalD, evalDs, truncELowering] at hc

-- The contrast, concretely — BOTH deviation paths:
-- value path (`{0: -7//2}[0]`): the deviation lands in a stored dict VALUE
-- and is read back out through `dictLookup`;
#guard ((evalDjs (.get (.dict [(0, .fdiv (.lit (-7)) (.lit 2))]) (.lit 0)) []).bind
        DVal.asInt) = some (-4)                                 -- real: Python floor
#guard ((evalDtgt truncELowering (.get (.dict [(0, .fdiv (.lit (-7)) (.lit 2))]) (.lit 0)) []).bind
        DVal.asInt) = some (-3)                                 -- stub: JS trunc ✗
-- key path (`{-4: 99}[-7//2]`): the deviation flips HIT vs MISS — the floor
-- key `-4` finds the entry, the trunc key `-3` raises KeyError.
#guard ((evalDjs (.get (.dict [(-4, .lit 99)]) (.fdiv (.lit (-7)) (.lit 2))) []).bind
        DVal.asInt) = some 99                                   -- real: key hits
#guard (evalDtgt truncELowering (.get (.dict [(-4, .lit 99)]) (.fdiv (.lit (-7)) (.lit 2))) []).isNone
                                                                -- stub: KeyError ✗

/-- SPOT (through the theorem, not by evaluation): the deviating dict program
    `{1: -7//2, 2: 20}[1]` — the INDEPENDENT compiled result is rewritten to
    the Python reference via `preservationD` (fails if the statement is
    weakened back to model-vs-model; a both-sides-`none` weakening leaves the
    `some` goal unprovable), then evaluated to the Python answer `-4` (floor,
    not the JS-trunc `-3`). -/
example :
    (evalDjs (.get (.dict [(1, .fdiv (.lit (-7)) (.lit 2)), (2, .lit 20)]) (.lit 1)) []).bind DVal.asInt
      = some (-4) := by
  rw [preservationD]
  simp [evalD, evalDs, dictLookup, DVal.asInt]

/-- SPOT (nested `ddict` value): `{1: {2: -7//2}}[1][2]` — the deviating
    value sits inside a NESTED dict (the `ddict` constructor is exercised as
    a stored VALUE, not just as the outer container); the independent
    compiled result is derived via `preservationD` to Python's floor `-4`. -/
example :
    (evalDjs (.get (.get (.dict [(1, .dict [(2, .fdiv (.lit (-7)) (.lit 2))])]) (.lit 1)) (.lit 2)) []).bind DVal.asInt
      = some (-4) := by
  rw [preservationD]
  simp [evalD, evalDs, dictLookup, DVal.asInt]

/-- SPOT (deviating lookup KEY): `{-4: 99}[-7//2]` — the `//` deviation flows
    through the KEY expression; via `preservationD` the compiled lookup HITS
    (floor key `-4`) and returns `99`, where a truncating compile would raise
    `KeyError` (see the contrast `#guard` above). -/
example :
    (evalDjs (.get (.dict [(-4, .lit 99)]) (.fdiv (.lit (-7)) (.lit 2))) []).bind DVal.asInt
      = some 99 := by
  rw [preservationD]
  simp [evalD, evalDs, dictLookup, DVal.asInt]

/-- info: '_private.PythExpandVerify.0.PythExpandVerify.preservationD'tgt' depends on axioms: [propext,
 Classical.choice,
 Quot.sound] -/
#guard_msgs in
#print axioms preservationD'tgt

/-- info: '_private.PythExpandVerify.0.PythExpandVerify.preservationDs'tgt' depends on axioms: [propext,
 Classical.choice,
 Quot.sound] -/
#guard_msgs in
#print axioms preservationDs'tgt

/-- info: 'PythExpandVerify.preservationD' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationD

/-- info: 'PythExpandVerify.preservationDs' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationDs

/-- info: 'PythExpandVerify.preservationD_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationD_real

/-- info: 'PythExpandVerify.preservationDs_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationDs_real

/-- info: 'PythExpandVerify.preservationD_stub_fails' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms preservationD_stub_fails

/-- info: 'PythExpandVerify.preservationDs_stub_fails' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms preservationDs_stub_fails

/-! ## Tier-3 wave 10 — classes (instance creation, attribute get, method call)

Covers the "classes" item of the paper's fragment-coverage limitation. The
layer is SELF-CONTAINED (its own `OVal`/`OEnv`/`CExp10`/`CStmt10`, exactly as
waves 4/6/9 defined their own layers) so every frozen definition stays
untouched. An OBJECT is `OVal.obj cls fields` — its class name plus a record
of named field values (objects nest: a field may hold another object). A
CLASS TABLE `ClsTable` maps class names to a `Class10`: an ordered field-name
list plus a method table; a METHOD is a first-order function over `self` +
params whose body is a `CStmt10` (assign / seq / if / while / return — the
wave-4 statement shape).

Modeled CPython semantics:
- INSTANCE CREATION `new C(args)` — `C(args)` with the standard positional
  `__init__` pattern (`def __init__(self, x, …): self.a = x; …`): the class's
  field names are bound to the evaluated arguments in order; wrong arity →
  `none` (TypeError), unknown class → `none` (NameError).
- ATTRIBUTE GET `e.attr` — field read on the receiver object; missing field →
  `none` (AttributeError).
- METHOD CALL `e.m(args)` — dispatch on the RECEIVER's class name, bind
  `self` (the receiver) + params, run the body under fuel; missing method →
  `none`; a body that falls off the end without `return` → `none` (implicit
  `None` returns are NOT modeled).

The interpreter is wave 4's uniform-fuel shape (`evalCls10`/`evalClsArgs10`/
`evalClsS10`; every recursive call — subexpression, argument, method body,
`while` unrolling — sits at the fuel predecessor), so ONE induction on fuel
over the three-way conjunction proves preservation.

Re-architected in the C1 rollout (wave 8): the compiled target is the
INDEPENDENT mutual triple `evalCls10tgt`/`evalClsArgs10tgt`/`evalClsS10tgt`,
parameterized by the integer-division lowering the emitted JS uses (the `//`
deviation flows through field initializers, call arguments, AND method
bodies). The `tgt : Bool` flag on `evalCls10` is retained only as the
historical definition shape — its `true` branch is LEGACY and carries NO
theorem; every preservation statement binds the independent target to
`evalCls10/evalClsS10 false`. The SAME predicates (`ClsPreserves`/
`ClsSPreserves`) are proved for the shipped floor-correction lowering and
REFUTED for the naive truncating stub.

F9 domain-completeness over `OVal = oint | obj`: BOTH constructors are
handled faithfully on both sides. `if`/`while` conditions branch on CPython
truthiness over the FULL `OVal` domain via `oTruthy` (nonzero int; an OBJECT
is ALWAYS truthy — CPython default object truthiness, no `__bool__`/
`__len__` overloads modeled). Arithmetic/comparison on an `obj` operand →
`none` = CPython `TypeError` (no dunder overloads); attribute get / method
call on an `oint` receiver → `none` = CPython `AttributeError` — the same
CORRECT rejection on both sides, pinned by `#guard`s below.

OMITTED (honestly): inheritance / MRO / `super()`, attribute ASSIGNMENT after
construction (fields are bound exactly once at creation — no `self.n = …`
mutation inside method bodies; method-local variables ARE assignable),
`isinstance`, class attributes / static methods, dunder overloads, and
implicit `None` returns. Single-dispatch, multi-class, multi-method programs
with nested objects and methods calling methods ARE covered. -/

inductive OVal where
  | oint (n : Int)
  | obj (cls : String) (fields : List (String × OVal))
deriving Repr

/-- Project an `OVal` to its int (for `#guard`s; avoids a nested `DecidableEq`). -/
def OVal.asInt : OVal → Option Int
  | .oint n => some n
  | .obj _ _ => none

abbrev OEnv := List (String × OVal)

def OEnv.get (env : OEnv) (n : String) : Option OVal :=
  (env.find? (fun p => p.1 == n)).map (·.2)

def OEnv.set (env : OEnv) (n : String) (v : OVal) : OEnv := (n, v) :: env

/-- Field read on an object's field record (missing → `AttributeError`/`none`). -/
def fieldGet10 (fields : List (String × OVal)) (a : String) : Option OVal :=
  (fields.find? (fun p => p.1 == a)).map (·.2)

inductive CExp10 where
  | lit (n : Int)
  | var (s : String)
  | add (a b : CExp10) | sub (a b : CExp10) | mul (a b : CExp10)
  | fdiv (a b : CExp10) | lt (a b : CExp10)
  | new (cls : String) (args : List CExp10)      -- Python `C(args)` (positional __init__)
  | attr (e : CExp10) (a : String)               -- Python `e.attr`
  | mcall (recv : CExp10) (m : String) (args : List CExp10)  -- Python `e.m(args)`
deriving Repr

inductive CStmt10 where
  | skip
  | assign (s : String) (e : CExp10)
  | seq (a b : CStmt10)
  | ite (c : CExp10) (t e : CStmt10)
  | whileB (c : CExp10) (body : CStmt10)
  | ret (e : CExp10)
deriving Repr

structure Method10 where
  params : List String
  body : CStmt10
deriving Repr

structure Class10 where
  fields : List String
  methods : List (String × Method10)
deriving Repr

abbrev ClsTable := List (String × Class10)

def ClsTable.lookupCls (ct : ClsTable) (c : String) : Option Class10 :=
  (ct.find? (fun p => p.1 == c)).map (·.2)

def lookupMeth10 (ms : List (String × Method10)) (m : String) : Option Method10 :=
  (ms.find? (fun p => p.1 == m)).map (·.2)

/-- Bind parameter names to argument values (used for fields and params). -/
def bindArgs10 (params : List String) (vals : List OVal) : OEnv := params.zip vals

/-- A statement result: updated environment + `some rv` if a `return` fired. -/
abbrev ORes10 := OEnv × Option OVal

/-- CPython truthiness over the FULL `OVal` domain (the F9 domain-completeness
    requirement): a nonzero int is truthy; an OBJECT is ALWAYS truthy (CPython
    default object truthiness — no `__bool__`/`__len__` overloads are modeled,
    and a plain instance is `True` in boolean context: `bool(object()) == True`).
    Shared by the reference AND the independent target, so a truthiness bug
    cannot hide as a shared-wrong `rfl` on the `obj` constructor. -/
def oTruthy : OVal → Bool
  | .oint n => n ≠ 0
  | .obj _ _ => true

mutual
/-- Fuel-bounded class-fragment expression eval. `tgt = false` = Python
    REFERENCE semantics (the only branch any theorem uses). The `tgt = true`
    branch is LEGACY (the former F1 model-vs-model flag) and is NOT the
    compiled target — the genuine compiled target is the INDEPENDENT
    `evalCls10tgt (L : IntDivLowering)` below; no theorem references
    `evalCls10 true`. `none` on error / fuel exhaustion. Every recursive call
    is at the fuel predecessor. -/
def evalCls10 (ct : ClsTable) (tgt : Bool) : Nat → CExp10 → OEnv → Option OVal
  | 0, _, _ => none
  | _ + 1, .lit n, _ => some (.oint n)
  | _ + 1, .var s, env => env.get s
  | fuel + 1, .add a b, env => match evalCls10 ct tgt fuel a env, evalCls10 ct tgt fuel b env with
      | some (.oint x), some (.oint y) => some (.oint (x + y)) | _, _ => none
  | fuel + 1, .sub a b, env => match evalCls10 ct tgt fuel a env, evalCls10 ct tgt fuel b env with
      | some (.oint x), some (.oint y) => some (.oint (x - y)) | _, _ => none
  | fuel + 1, .mul a b, env => match evalCls10 ct tgt fuel a env, evalCls10 ct tgt fuel b env with
      | some (.oint x), some (.oint y) => some (.oint (x * y)) | _, _ => none
  | fuel + 1, .lt a b, env => match evalCls10 ct tgt fuel a env, evalCls10 ct tgt fuel b env with
      | some (.oint x), some (.oint y) => some (.oint (if x < y then 1 else 0)) | _, _ => none
  | fuel + 1, .fdiv a b, env => match evalCls10 ct tgt fuel a env, evalCls10 ct tgt fuel b env with
      | some (.oint x), some (.oint y) =>
          if y = 0 then none else some (.oint (if tgt then jsFdiv x y else Int.fdiv x y))
      | _, _ => none
  | fuel + 1, .new c args, env => match ct.lookupCls c with
      | none => none                                        -- NameError
      | some cd => (evalClsArgs10 ct tgt fuel args env).bind (fun vs =>
          if vs.length = cd.fields.length
          then some (.obj c (bindArgs10 cd.fields vs))      -- positional __init__ field binding
          else none)                                        -- TypeError (arity)
  | fuel + 1, .attr e a, env => match evalCls10 ct tgt fuel e env with
      | some (.obj _ fs) => fieldGet10 fs a
      | _ => none
  | fuel + 1, .mcall recv m args, env => match evalCls10 ct tgt fuel recv env with
      | some (.obj c fs) => match ct.lookupCls c with
          | none => none
          | some cd => match lookupMeth10 cd.methods m with
              | none => none
              | some mth => (evalClsArgs10 ct tgt fuel args env).bind (fun vs =>
                  if vs.length = mth.params.length
                  then (evalClsS10 ct tgt fuel mth.body
                          (("self", OVal.obj c fs) :: bindArgs10 mth.params vs)).bind (·.2)
                  else none)                                -- TypeError (arity)
      | _ => none
termination_by n _ _ => n
/-- Eval an argument list (left to right). -/
def evalClsArgs10 (ct : ClsTable) (tgt : Bool) : Nat → List CExp10 → OEnv → Option (List OVal)
  | 0, _, _ => none
  | _ + 1, [], _ => some []
  | fuel + 1, a :: as, env => match evalCls10 ct tgt fuel a env, evalClsArgs10 ct tgt fuel as env with
      | some v, some vs => some (v :: vs) | _, _ => none
termination_by n _ _ => n
/-- Fuel-bounded method-body statement eval → `(env, return?)`. `return`
    short-circuits through `seq`/`while`; `if`/`while` branch on CPython
    truthiness over the FULL `OVal` domain (`oTruthy`: nonzero int; an object
    condition is ALWAYS truthy — a `while obj:` loop spins until fuel runs
    out, CPython's nontermination). `tgt = true` is LEGACY (see `evalCls10`). -/
def evalClsS10 (ct : ClsTable) (tgt : Bool) : Nat → CStmt10 → OEnv → Option ORes10
  | 0, _, _ => none
  | _ + 1, .skip, env => some (env, none)
  | fuel + 1, .assign s e, env => (evalCls10 ct tgt fuel e env).map (fun v => (env.set s v, none))
  | fuel + 1, .ret e, env => (evalCls10 ct tgt fuel e env).map (fun v => (env, some v))
  | fuel + 1, .seq a b, env => (evalClsS10 ct tgt fuel a env).bind
      (fun r => match r.2 with | some rv => some (r.1, some rv) | none => evalClsS10 ct tgt fuel b r.1)
  | fuel + 1, .ite c t e, env => match evalCls10 ct tgt fuel c env with
      | some v => if oTruthy v then evalClsS10 ct tgt fuel t env else evalClsS10 ct tgt fuel e env
      | none => none
  | fuel + 1, .whileB c body, env => match evalCls10 ct tgt fuel c env with
      | some v =>
          if oTruthy v then (evalClsS10 ct tgt fuel body env).bind
              (fun r => match r.2 with
                | some rv => some (r.1, some rv)
                | none => evalClsS10 ct tgt fuel (.whileB c body) r.1)
          else some (env, none)
      | none => none
termination_by fuel _ _ => fuel
end

mutual
/-- **Independent target evaluator triple** for the class fragment: the
    compiled program's semantics under lowering `L`. A SEPARATE mutual
    recursion (not the `Bool` flag on `evalCls10`); the `.fdiv` arm calls the
    lowering's operation, mirroring the emitted JS — so the `//` deviation
    routes through `L` in field initializers (via `evalClsArgs10tgt` at a
    `new`), method-call arguments, AND method bodies (via `evalClsS10tgt`).
    Instance creation, field read, dispatch, and binding mirror the reference
    structurally; BOTH `OVal` constructors are handled faithfully (arithmetic
    on `obj` → `none`/TypeError, `attr`/`mcall` on `oint` → `none`/
    AttributeError — the same CORRECT rejections as the reference). -/
def evalCls10tgt (ct : ClsTable) (L : IntDivLowering) : Nat → CExp10 → OEnv → Option OVal
  | 0, _, _ => none
  | _ + 1, .lit n, _ => some (.oint n)
  | _ + 1, .var s, env => env.get s
  | fuel + 1, .add a b, env => match evalCls10tgt ct L fuel a env, evalCls10tgt ct L fuel b env with
      | some (.oint x), some (.oint y) => some (.oint (x + y)) | _, _ => none
  | fuel + 1, .sub a b, env => match evalCls10tgt ct L fuel a env, evalCls10tgt ct L fuel b env with
      | some (.oint x), some (.oint y) => some (.oint (x - y)) | _, _ => none
  | fuel + 1, .mul a b, env => match evalCls10tgt ct L fuel a env, evalCls10tgt ct L fuel b env with
      | some (.oint x), some (.oint y) => some (.oint (x * y)) | _, _ => none
  | fuel + 1, .lt a b, env => match evalCls10tgt ct L fuel a env, evalCls10tgt ct L fuel b env with
      | some (.oint x), some (.oint y) => some (.oint (if x < y then 1 else 0)) | _, _ => none
  | fuel + 1, .fdiv a b, env => match evalCls10tgt ct L fuel a env, evalCls10tgt ct L fuel b env with
      | some (.oint x), some (.oint y) =>
          if y = 0 then none else some (.oint (L.fdiv x y))
      | _, _ => none
  | fuel + 1, .new c args, env => match ct.lookupCls c with
      | none => none                                        -- NameError
      | some cd => (evalClsArgs10tgt ct L fuel args env).bind (fun vs =>
          if vs.length = cd.fields.length
          then some (.obj c (bindArgs10 cd.fields vs))      -- positional __init__ field binding
          else none)                                        -- TypeError (arity)
  | fuel + 1, .attr e a, env => match evalCls10tgt ct L fuel e env with
      | some (.obj _ fs) => fieldGet10 fs a
      | _ => none
  | fuel + 1, .mcall recv m args, env => match evalCls10tgt ct L fuel recv env with
      | some (.obj c fs) => match ct.lookupCls c with
          | none => none
          | some cd => match lookupMeth10 cd.methods m with
              | none => none
              | some mth => (evalClsArgs10tgt ct L fuel args env).bind (fun vs =>
                  if vs.length = mth.params.length
                  then (evalClsS10tgt ct L fuel mth.body
                          (("self", OVal.obj c fs) :: bindArgs10 mth.params vs)).bind (·.2)
                  else none)                                -- TypeError (arity)
      | _ => none
termination_by n _ _ => n
/-- Independent-target argument-list eval (left to right). -/
def evalClsArgs10tgt (ct : ClsTable) (L : IntDivLowering) : Nat → List CExp10 → OEnv → Option (List OVal)
  | 0, _, _ => none
  | _ + 1, [], _ => some []
  | fuel + 1, a :: as, env => match evalCls10tgt ct L fuel a env, evalClsArgs10tgt ct L fuel as env with
      | some v, some vs => some (v :: vs) | _, _ => none
termination_by n _ _ => n
/-- Independent-target method-body statement eval; `if`/`while` branch on the
    SAME full-domain `oTruthy` as the reference. -/
def evalClsS10tgt (ct : ClsTable) (L : IntDivLowering) : Nat → CStmt10 → OEnv → Option ORes10
  | 0, _, _ => none
  | _ + 1, .skip, env => some (env, none)
  | fuel + 1, .assign s e, env => (evalCls10tgt ct L fuel e env).map (fun v => (env.set s v, none))
  | fuel + 1, .ret e, env => (evalCls10tgt ct L fuel e env).map (fun v => (env, some v))
  | fuel + 1, .seq a b, env => (evalClsS10tgt ct L fuel a env).bind
      (fun r => match r.2 with | some rv => some (r.1, some rv) | none => evalClsS10tgt ct L fuel b r.1)
  | fuel + 1, .ite c t e, env => match evalCls10tgt ct L fuel c env with
      | some v => if oTruthy v then evalClsS10tgt ct L fuel t env else evalClsS10tgt ct L fuel e env
      | none => none
  | fuel + 1, .whileB c body, env => match evalCls10tgt ct L fuel c env with
      | some v =>
          if oTruthy v then (evalClsS10tgt ct L fuel body env).bind
              (fun r => match r.2 with
                | some rv => some (r.1, some rv)
                | none => evalClsS10tgt ct L fuel (.whileB c body) r.1)
          else some (env, none)
      | none => none
termination_by fuel _ _ => fuel
end

/-- The compiled class-fragment expression semantics: the independent target
    under the SHIPPED lowering. -/
abbrev evalCls10js (ct : ClsTable) : Nat → CExp10 → OEnv → Option OVal :=
  evalCls10tgt ct jsELowering

/-- The compiled method-body statement semantics: the independent target
    under the SHIPPED lowering. -/
abbrev evalClsS10js (ct : ClsTable) : Nat → CStmt10 → OEnv → Option ORes10 :=
  evalClsS10tgt ct jsELowering

/-- Class-fragment EXPRESSION preservation as a predicate OVER the lowering —
    the SAME predicate is proved for the shipped lowering
    (`preservationCls_real`) and REFUTED for the stub
    (`preservationCls_stub_fails`). Quantifies over the class table and the
    fuel too: a lowering preserves only if it does so under EVERY class table
    at EVERY recursion depth. -/
def ClsPreserves (L : IntDivLowering) : Prop :=
  ∀ ct fuel e env, evalCls10tgt ct L fuel e env = evalCls10 ct false fuel e env

/-- Class-fragment STATEMENT (method-body) preservation as a predicate OVER
    the lowering (`preservationClsS_real` vs `preservationClsS_stub_fails`). -/
def ClsSPreserves (L : IntDivLowering) : Prop :=
  ∀ ct fuel s env, evalClsS10tgt ct L fuel s env = evalClsS10 ct false fuel s env

-- executable bindings pinned to CPython class semantics.
-- `class Counter:  def __init__(self, x): self.n = x` … `Counter(21).dbl()` = 42.
def _tier3w10_counter : ClsTable :=
  [("Counter",
    { fields := ["n"],
      methods :=
        [("dbl",  { params := [],    body := .ret (.mul (.attr (.var "self") "n") (.lit 2)) }),
         ("half", { params := [],    body := .ret (.fdiv (.attr (.var "self") "n") (.lit 2)) }),
         ("addk", { params := ["k"], body := .ret (.add (.attr (.var "self") "n") (.var "k")) }),
         -- def sumto(self): i = self.n; acc = 0; (while 0 < i: acc += i; i -= 1); return acc
         ("sumto",
          { params := [],
            body := .seq (.assign "i" (.attr (.var "self") "n"))
                   (.seq (.assign "acc" (.lit 0))
                   (.seq (.whileB (.lt (.lit 0) (.var "i"))
                           (.seq (.assign "acc" (.add (.var "acc") (.var "i")))
                                 (.assign "i" (.sub (.var "i") (.lit 1)))))
                         (.ret (.var "acc")))) }),
         -- def stest(self): return 1 if self else 2  — OBJECT condition (F9 obj truthiness)
         ("stest", { params := [], body := .ite (.var "self") (.ret (.lit 1)) (.ret (.lit 2)) }),
         -- def ztest(self): return 1 if self.n else 2 — INT condition (0 falsy contrast)
         ("ztest", { params := [], body := .ite (.attr (.var "self") "n") (.ret (.lit 1)) (.ret (.lit 2)) })] })]

-- `Counter(21).dbl()` = 42 — the prompt's canonical pin: CPython reference,
-- and the INDEPENDENT compiled target (the retired `evalCls10 true` guards,
-- now on `evalCls10js`).
#guard ((evalCls10 _tier3w10_counter false 50 (.mcall (.new "Counter" [.lit 21]) "dbl" []) []).bind OVal.asInt) = some 42
#guard ((evalCls10js _tier3w10_counter 50 (.mcall (.new "Counter" [.lit 21]) "dbl" []) []).bind OVal.asInt) = some 42
-- attribute get: `Counter(21).n` = 21.
#guard ((evalCls10 _tier3w10_counter false 50 (.attr (.new "Counter" [.lit 21]) "n") []).bind OVal.asInt) = some 21
-- THE `//` DEVIATION INSIDE A METHOD BODY: `Counter(-7).half()` = `-7 // 2` = -4 (floor,
-- not JS-trunc -3), reference AND compiled agree — the emitted correction is what
-- preservation certifies.
#guard ((evalCls10 _tier3w10_counter false 50 (.mcall (.new "Counter" [.lit (-7)]) "half" []) []).bind OVal.asInt) = some (-4)
#guard ((evalCls10js _tier3w10_counter 50 (.mcall (.new "Counter" [.lit (-7)]) "half" []) []).bind OVal.asInt) = some (-4)
-- the deviation in a CONSTRUCTOR ARGUMENT: `Counter(-7 // 2).dbl()` = dbl(-4) = -8, both.
#guard ((evalCls10 _tier3w10_counter false 50 (.mcall (.new "Counter" [.fdiv (.lit (-7)) (.lit 2)]) "dbl" []) []).bind OVal.asInt) = some (-8)
#guard ((evalCls10js _tier3w10_counter 50 (.mcall (.new "Counter" [.fdiv (.lit (-7)) (.lit 2)]) "dbl" []) []).bind OVal.asInt) = some (-8)
-- method with a parameter: `Counter(40).addk(2)` = 42.
#guard ((evalCls10 _tier3w10_counter false 50 (.mcall (.new "Counter" [.lit 40]) "addk" [.lit 2]) []).bind OVal.asInt) = some 42
-- a LOOP inside a method body: `Counter(5).sumto()` = 15, both.
#guard ((evalCls10 _tier3w10_counter false 60 (.mcall (.new "Counter" [.lit 5]) "sumto" []) []).bind OVal.asInt) = some 15
#guard ((evalCls10js _tier3w10_counter 60 (.mcall (.new "Counter" [.lit 5]) "sumto" []) []).bind OVal.asInt) = some 15
-- F9 TRUTHINESS over the full `OVal` domain: an OBJECT condition is truthy even
-- when its field is 0 (CPython: `1 if Counter(0) else 2` == 1 — default object
-- truthiness), while the INT condition `self.n` at 0 is falsy (`1 if 0 else 2`
-- == 2). The pair DISCRIMINATES object-truthiness from int-truthiness.
#guard ((evalCls10 _tier3w10_counter false 50 (.mcall (.new "Counter" [.lit 0]) "stest" []) []).bind OVal.asInt) = some 1
#guard ((evalCls10js _tier3w10_counter 50 (.mcall (.new "Counter" [.lit 0]) "stest" []) []).bind OVal.asInt) = some 1
#guard ((evalCls10 _tier3w10_counter false 50 (.mcall (.new "Counter" [.lit 0]) "ztest" []) []).bind OVal.asInt) = some 2
#guard ((evalCls10js _tier3w10_counter 50 (.mcall (.new "Counter" [.lit 0]) "ztest" []) []).bind OVal.asInt) = some 2
-- `while obj:` never terminates in CPython — modeled as fuel exhaustion (`none`),
-- on the reference and the compiled target alike.
#guard (evalClsS10 _tier3w10_counter false 30 (.whileB (.new "Counter" [.lit 0]) .skip) []).isNone
#guard (evalClsS10js _tier3w10_counter 30 (.whileB (.new "Counter" [.lit 0]) .skip) []).isNone
-- error semantics: AttributeError, missing method, TypeError (arity), NameError —
-- pinned on the reference AND on the compiled target (the same CORRECT rejection
-- on both sides, not a shared-wrong `rfl`).
#guard (evalCls10 _tier3w10_counter false 50 (.attr (.new "Counter" [.lit 21]) "m") []).isNone
#guard (evalCls10js _tier3w10_counter 50 (.attr (.new "Counter" [.lit 21]) "m") []).isNone
#guard (evalCls10 _tier3w10_counter false 50 (.mcall (.new "Counter" [.lit 21]) "nope" []) []).isNone
#guard (evalCls10js _tier3w10_counter 50 (.mcall (.new "Counter" [.lit 21]) "nope" []) []).isNone
#guard (evalCls10 _tier3w10_counter false 50 (.new "Counter" [.lit 1, .lit 2]) []).isNone
#guard (evalCls10js _tier3w10_counter 50 (.new "Counter" [.lit 1, .lit 2]) []).isNone
#guard (evalCls10 _tier3w10_counter false 50 (.new "Widget" [.lit 1]) []).isNone
#guard (evalCls10js _tier3w10_counter 50 (.new "Widget" [.lit 1]) []).isNone
-- F9 CROSS-CONSTRUCTOR rejections: attribute get / method call on an INT
-- receiver → AttributeError (CPython: `(5).n`, `(5).m()`); arithmetic and `//`
-- on an OBJECT operand → TypeError (no dunder overloads modeled). Both sides.
#guard (evalCls10 _tier3w10_counter false 50 (.attr (.lit 5) "n") []).isNone
#guard (evalCls10js _tier3w10_counter 50 (.attr (.lit 5) "n") []).isNone
#guard (evalCls10 _tier3w10_counter false 50 (.mcall (.lit 5) "dbl" []) []).isNone
#guard (evalCls10js _tier3w10_counter 50 (.mcall (.lit 5) "dbl" []) []).isNone
#guard (evalCls10 _tier3w10_counter false 50 (.add (.new "Counter" [.lit 1]) (.lit 2)) []).isNone
#guard (evalCls10js _tier3w10_counter 50 (.add (.new "Counter" [.lit 1]) (.lit 2)) []).isNone
#guard (evalCls10 _tier3w10_counter false 50 (.fdiv (.new "Counter" [.lit 1]) (.lit 2)) []).isNone
#guard (evalCls10js _tier3w10_counter 50 (.fdiv (.new "Counter" [.lit 1]) (.lit 2)) []).isNone

-- MULTI-CLASS + NESTED OBJECTS: `Seg(Pt(1,2), Pt(3,4)).ends()` = 10 — objects in
-- fields, a method dispatching a method on a field's object.
def _tier3w10_geom : ClsTable :=
  [("Pt",
    { fields := ["x", "y"],
      methods :=
        [("sum",
          { params := [],
            body := .ret (.add (.attr (.var "self") "x") (.attr (.var "self") "y")) })] }),
   ("Seg",
    { fields := ["p", "q"],
      methods :=
        [("ends",
          { params := [],
            body := .ret (.add (.mcall (.attr (.var "self") "p") "sum" [])
                               (.mcall (.attr (.var "self") "q") "sum" [])) })] })]
#guard ((evalCls10 _tier3w10_geom false 50
    (.mcall (.new "Seg" [.new "Pt" [.lit 1, .lit 2], .new "Pt" [.lit 3, .lit 4]]) "ends" []) []).bind OVal.asInt) = some 10
#guard ((evalCls10js _tier3w10_geom 50
    (.mcall (.new "Seg" [.new "Pt" [.lit 1, .lit 2], .new "Pt" [.lit 3, .lit 4]]) "ends" []) []).bind OVal.asInt) = some 10

/-- Combined preservation worker for the three mutual class evaluators,
    RE-TARGETED to the independent triple: ONE induction on `fuel` carries all
    three statements together (the wave-4 pattern — uniform fuel puts every
    recursive call, including method-body execution and `while` unrolling, at
    the predecessor, so the three IHs at `fuel` discharge every recursion
    site). Binds the INDEPENDENT target under the shipped lowering to the
    Python reference `evalCls10/evalClsArgs10/evalClsS10 false` — real
    induction, not a flag-vs-flag identity. Only the `.fdiv` arm needs
    arithmetic content: `jsFdiv_eq_fdiv` closes it on the `y ≠ 0` branch;
    every other arm (creation, field read, dispatch, `oTruthy` branching, and
    the cross-constructor TypeError/AttributeError rejections) is closed
    structurally AFTER the IH rewrites because both sides take the same
    CORRECT branch on the same scrutinee. -/
private theorem preservationCls'tgt (ct : ClsTable) (fuel : Nat) :
    (∀ e env, evalCls10tgt ct jsELowering fuel e env = evalCls10 ct false fuel e env) ∧
    (∀ args env, evalClsArgs10tgt ct jsELowering fuel args env = evalClsArgs10 ct false fuel args env) ∧
    (∀ s env, evalClsS10tgt ct jsELowering fuel s env = evalClsS10 ct false fuel s env) := by
  induction fuel with
  | zero =>
    exact ⟨fun e env => by simp only [evalCls10tgt, evalCls10],
           fun args env => by simp only [evalClsArgs10tgt, evalClsArgs10],
           fun s env => by simp only [evalClsS10tgt, evalClsS10]⟩
  | succ fuel ih =>
    obtain ⟨ihE, ihA, ihS⟩ := ih
    refine ⟨fun e env => ?_, fun args env => ?_, fun s env => ?_⟩
    · cases e with
      | lit n => simp only [evalCls10tgt, evalCls10]
      | var s => simp only [evalCls10tgt, evalCls10]
      | add a b => simp only [evalCls10tgt, evalCls10, ihE]
      | sub a b => simp only [evalCls10tgt, evalCls10, ihE]
      | mul a b => simp only [evalCls10tgt, evalCls10, ihE]
      | lt a b => simp only [evalCls10tgt, evalCls10, ihE]
      | fdiv a b =>
        simp only [evalCls10tgt, evalCls10, ihE]
        cases evalCls10 ct false fuel a env with
        | none => rfl
        | some va =>
          cases evalCls10 ct false fuel b env with
          | none => cases va <;> rfl
          | some vb =>
            cases va with
            | obj c fs => rfl
            | oint x =>
              cases vb with
              | obj c fs => rfl
              | oint y =>
                by_cases hy : y = 0
                · simp [hy]
                · simp [hy, jsELowering, jsFdiv_eq_fdiv x y hy]
      | new c args => simp only [evalCls10tgt, evalCls10, ihA]
      | attr e1 a => simp only [evalCls10tgt, evalCls10, ihE]
      | mcall recv m args => simp only [evalCls10tgt, evalCls10, ihE, ihA, ihS]
    · cases args with
      | nil => simp only [evalClsArgs10tgt, evalClsArgs10]
      | cons a as => simp only [evalClsArgs10tgt, evalClsArgs10, ihE, ihA]
    · cases s with
      | skip => simp only [evalClsS10tgt, evalClsS10]
      | assign n e => simp only [evalClsS10tgt, evalClsS10, ihE]
      | seq a b => simp only [evalClsS10tgt, evalClsS10, ihS]
      | ite c t e => simp only [evalClsS10tgt, evalClsS10, ihE, ihS]
      | whileB c body => simp only [evalClsS10tgt, evalClsS10, ihE, ihS]
      | ret e => simp only [evalClsS10tgt, evalClsS10, ihE]

/-- **Class preservation (Tier-3 wave 10 / C1-rollout wave 8,
    re-architected).** Under ANY class table, for EVERY fuel, class-fragment
    expression (instance creation with positional `__init__` field binding,
    attribute get, single-dispatch method call — with the `//` deviation
    available in field initializers, arguments, and method bodies), and
    environment, the INDEPENDENT compiled target under the shipped lowering
    computes the same value as the Python reference `evalCls10 false`. -/
theorem preservationCls (ct : ClsTable) (fuel : Nat) (e : CExp10) (env : OEnv) :
    evalCls10js ct fuel e env = evalCls10 ct false fuel e env :=
  (preservationCls'tgt ct fuel).1 e env

/-- Statement-level analogue: the compiled method-body language (assignment,
    sequencing, `if`/`while` on full-`OVal` truthiness, `return`) matches the
    Python reference. -/
theorem preservationClsS (ct : ClsTable) (fuel : Nat) (s : CStmt10) (env : OEnv) :
    evalClsS10js ct fuel s env = evalClsS10 ct false fuel s env :=
  (preservationCls'tgt ct fuel).2.2 s env

/-- The re-architected statement, in predicate form: the shipped lowering
    preserves the class fragment. Same content as `preservationCls`; this is
    the instantiation the stub litmus contrasts against. -/
theorem preservationCls_real : ClsPreserves jsELowering :=
  fun ct fuel e env => preservationCls ct fuel e env

/-- Predicate form for method-body statements: the instantiation
    `preservationClsS_stub_fails` contrasts against. -/
theorem preservationClsS_real : ClsSPreserves jsELowering :=
  fun ct fuel s env => preservationClsS ct fuel s env

/-- **Stub litmus (classes, expression side).** The SAME preservation
    predicate is FALSE for the naive truncating lowering, on a deviating class
    program whose `//` sits INSIDE A METHOD BODY reached only through
    construction + dispatch: `Counter(-7).half()` (with `half` returning
    `self.n // 2`) computes JS-trunc `-3` under the stub where Python floors
    to `-4` — a concrete DISCRIMINATING contradiction (floor ≠ trunc at
    `-7 // 2`) the old `evalCls10 true = evalCls10 false` statement could not
    express. -/
theorem preservationCls_stub_fails : ¬ ClsPreserves truncELowering := by
  intro h
  have hc := h _tier3w10_counter 10 (.mcall (.new "Counter" [.lit (-7)]) "half" []) []
  -- The evaluators are fuel-recursive (not kernel-reducible): step them via
  -- their equation lemmas; hc reduces through `some (.oint (-3)) = some (.oint (-4))`
  -- (stub `Int.tdiv (-7) 2 = -3` vs Python `Int.fdiv (-7) 2 = -4`) to `False`.
  simp [_tier3w10_counter, evalCls10tgt, evalClsArgs10tgt, evalClsS10tgt,
        evalCls10, evalClsArgs10, evalClsS10, ClsTable.lookupCls, lookupMeth10,
        fieldGet10, bindArgs10, OEnv.get, truncELowering] at hc

/-- **Stub litmus (classes, statement side).** The SAME statement predicate is
    FALSE for the truncating lowering, on a deviating method-body statement
    whose `//` result decides an `if` via the full-domain truthiness:
    `if n // 2: r = 1 else: r = 2` at `n = -1` — Python floors `-1 // 2 = -1`
    (truthy, takes the THEN branch, `r = 1`) where the stub truncates to `0`
    (falsy, takes the ELSE branch, `r = 2`): the resulting STATES diverge, a
    branch-selection flip, not merely an off-by-one value. -/
theorem preservationClsS_stub_fails : ¬ ClsSPreserves truncELowering := by
  intro h
  have hc := h _tier3w10_counter 10
      (.ite (.fdiv (.var "n") (.lit 2)) (.assign "r" (.lit 1)) (.assign "r" (.lit 2)))
      [("n", .oint (-1))]
  -- hc reduces through `some (("r", .oint 2) :: …, none) = some (("r", .oint 1) :: …, none)`.
  simp [evalCls10tgt, evalClsArgs10tgt, evalClsS10tgt, evalCls10, evalClsArgs10,
        evalClsS10, OEnv.get, OEnv.set, oTruthy, truncELowering] at hc

-- The contrast, concretely — BOTH deviation paths:
-- value path: the deviation lands in a method-call RESULT (through construction,
-- field binding, dispatch, and the method body);
#guard ((evalCls10js _tier3w10_counter 10 (.mcall (.new "Counter" [.lit (-7)]) "half" []) []).bind
        OVal.asInt) = some (-4)                              -- real: Python floor
#guard ((evalCls10tgt _tier3w10_counter truncELowering 10 (.mcall (.new "Counter" [.lit (-7)]) "half" []) []).bind
        OVal.asInt) = some (-3)                              -- stub: JS trunc ✗
-- truthiness path: the deviation flips WHICH BRANCH an `if` takes — floor `-1`
-- is truthy, trunc `0` is falsy, so the stub's final state holds `r = 2`, Python's `r = 1`.
#guard ((evalClsS10js _tier3w10_counter 10
        (.ite (.fdiv (.var "n") (.lit 2)) (.assign "r" (.lit 1)) (.assign "r" (.lit 2)))
        [("n", .oint (-1))]).bind (fun r => (r.1.get "r").bind OVal.asInt)) = some 1   -- real
#guard ((evalClsS10tgt _tier3w10_counter truncELowering 10
        (.ite (.fdiv (.var "n") (.lit 2)) (.assign "r" (.lit 1)) (.assign "r" (.lit 2)))
        [("n", .oint (-1))]).bind (fun r => (r.1.get "r").bind OVal.asInt)) = some 2   -- stub ✗

/-- SPOT (through the theorem, not by evaluation): the `//` deviation inside a
    METHOD BODY — `Counter(-7).half()` where `half` returns `self.n // 2` —
    in the INDEPENDENT compiled semantics, routed THROUGH `preservationCls` to
    the Python reference and evaluated there to the floor answer -4 (JS-trunc
    would give -3). Would not close if the statement were weakened back to
    model-vs-model (a both-sides-`none` version leaves the `some` goal
    unprovable). -/
example :
    (evalCls10js _tier3w10_counter 50 (.mcall (.new "Counter" [.lit (-7)]) "half" []) []).bind OVal.asInt
      = some (-4) := by
  rw [preservationCls]
  simp [_tier3w10_counter, evalCls10, evalClsArgs10, evalClsS10, ClsTable.lookupCls,
        lookupMeth10, fieldGet10, bindArgs10, OEnv.get, OVal.asInt, oTruthy]

/-- SPOT (nested objects — the `obj` constructor exercised as a stored FIELD
    value, not just a receiver): `Seg(Pt(1,2), Pt(3,4)).ends()` — a method
    dispatching methods on OBJECT-valued fields — derived via
    `preservationCls` to the CPython answer 10. -/
example :
    (evalCls10js _tier3w10_geom 50
        (.mcall (.new "Seg" [.new "Pt" [.lit 1, .lit 2], .new "Pt" [.lit 3, .lit 4]]) "ends" []) []).bind OVal.asInt
      = some 10 := by
  rw [preservationCls]
  simp [_tier3w10_geom, evalCls10, evalClsArgs10, evalClsS10, ClsTable.lookupCls,
        lookupMeth10, fieldGet10, bindArgs10, OEnv.get, OVal.asInt]

/-- SPOT (object truthiness through the theorem): `Counter(0).stest()` — the
    `if self:` condition is an OBJECT, truthy in CPython even though the field
    is 0 — derived via `preservationCls` to 1 (an int-truthiness-only model,
    or an obj-rejecting one, would leave this goal unprovable). -/
example :
    (evalCls10js _tier3w10_counter 50 (.mcall (.new "Counter" [.lit 0]) "stest" []) []).bind OVal.asInt
      = some 1 := by
  rw [preservationCls]
  simp [_tier3w10_counter, evalCls10, evalClsArgs10, evalClsS10, ClsTable.lookupCls,
        lookupMeth10, fieldGet10, bindArgs10, OEnv.get, OVal.asInt, oTruthy]

/-- SPOT (statement level, assignment-from-method-call): `y = Counter(-7).half()`
    ends with `y = -4` in the INDEPENDENT compiled semantics, derived via
    `preservationClsS` (exercises the statement language + the argument/body
    lowering through dispatch). -/
example :
    (evalClsS10js _tier3w10_counter 50
        (.assign "y" (.mcall (.new "Counter" [.lit (-7)]) "half" [])) []).bind
      (fun r => (r.1.get "y").bind OVal.asInt) = some (-4) := by
  rw [preservationClsS]
  simp [_tier3w10_counter, evalCls10, evalClsArgs10, evalClsS10, ClsTable.lookupCls,
        lookupMeth10, fieldGet10, bindArgs10, OEnv.get, OEnv.set, OVal.asInt, oTruthy]

/-- info: '_private.PythExpandVerify.0.PythExpandVerify.preservationCls'tgt' depends on axioms: [propext,
 Classical.choice,
 Quot.sound] -/
#guard_msgs in
#print axioms preservationCls'tgt

/-- info: 'PythExpandVerify.preservationCls' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationCls

/-- info: 'PythExpandVerify.preservationClsS' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationClsS

/-- info: 'PythExpandVerify.preservationCls_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationCls_real

/-- info: 'PythExpandVerify.preservationClsS_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationClsS_real

/-- info: 'PythExpandVerify.preservationCls_stub_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationCls_stub_fails

/-- info: 'PythExpandVerify.preservationClsS_stub_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationClsS_stub_fails

/-! ## Tier-3 wave 11 — strings (code-point value class + the D3/D5 UTF-16 deviation)

Strings become first-class values. The layer is SELF-CONTAINED (its own
`SVal`/`SEnv`/`SExp`, exactly as waves 4/6/7/9/10 defined their own layers) so
the frozen `Val`/`CExp` of waves 5/6/7 stay untouched. A string is modeled as
its **code-point sequence** `List Int` — the representation Python's `str`
semantics is defined over (`len`, `s[i]`, `s[lo:hi]` all count CODE POINTS).

**The D3/D5 deviation (why this wave is non-vacuous).** Python indexes strings
by code point; JavaScript strings are UTF-16 code-unit sequences, so naive JS
`s.length`/`s[i]` count astral characters (code points > U+FFFF, encoded as
surrogate PAIRS) as TWO where Python counts ONE. The compiler therefore emits
code-point-correct string helpers — the string analogue of the seed's
floor-div correction — so `slen`/`sindex`/`sslice` compute over code points in
BOTH semantics below and preservation holds. The deviation itself is PROVED
real (not cosmetic) by a faithful UTF-16 model (`toUtf16`/`utf16Encode`/
`utf16Len`): `utf16_astral_strict` shows any string containing an astral code
point has `utf16Len` STRICTLY greater than its Python `len`, and the pinned
witnesses show naive UTF-16 indexing lands on a trailing surrogate where
Python yields a character.

Indexing reuses the verified-core `getIndex` (negative-index normalization,
OOB → `none`/IndexError, Tier 2); slicing reuses the verified-core `sliceWalk`
(the Lean twin of CPython `slice.indices`, Tier 1) with step 1 — so string
subscripts are certified by the same Tier-1/2 lemmas as list subscripts. The
`//` deviation enters through the int sub-fragment (index/bound expressions),
closed by `jsFdiv_eq_fdiv` exactly as in waves 5/9/10.

OUT of scope (documented, deliberate): string METHODS (`upper`/`split`/
`replace`/`join`/…) are runtime helpers, not modeled here; f-strings;
`str`↔`bytes` encoding; repetition `s * n`; comparison/ordering; `in`
membership. This wave is string VALUES + concat/len/index/slice — exactly
where the D3/D5 code-point-vs-UTF-16 deviation lives. -/

inductive SVal where
  | sstr (cps : List Int)   -- a string as its code-point sequence
  | sint (n : Int)
deriving Repr

/-- Project an `SVal` to its int (for `#guard`s). -/
def SVal.asInt : SVal → Option Int
  | .sint n => some n
  | .sstr _ => none

/-- Project an `SVal` to its code-point list (for `#guard`s). -/
def SVal.asCps : SVal → Option (List Int)
  | .sstr cps => some cps
  | .sint _ => none

abbrev SEnv := List (String × SVal)

def SEnv.get (env : SEnv) (n : String) : Option SVal :=
  (env.find? (fun p => p.1 == n)).map (·.2)

/-! ### The faithful UTF-16 model — the deviation witness, kept OUTSIDE the
semantics (the compiled helpers are code-point-correct; this model is what
naive JS `.length`/`s[i]` would compute, pinned so the deviation is proved
real rather than asserted). -/

/-- UTF-16 code units of one code point: astral (≥ U+10000) → surrogate pair. -/
def toUtf16 (cp : Int) : List Int :=
  if 0x10000 ≤ cp then
    [0xD800 + (cp - 0x10000) / 0x400, 0xDC00 + (cp - 0x10000) % 0x400]
  else [cp]

/-- UTF-16 code-unit sequence of a code-point string (what a JS string IS). -/
def utf16Encode : List Int → List Int
  | [] => []
  | c :: rest => toUtf16 c ++ utf16Encode rest

/-- Number of UTF-16 code units for one code point (1, or 2 for astral). -/
def utf16Units (cp : Int) : Nat := if 0x10000 ≤ cp then 2 else 1

/-- What naive JS `s.length` returns: the UTF-16 code-unit count. -/
def utf16Len : List Int → Nat
  | [] => 0
  | c :: rest => utf16Units c + utf16Len rest

-- U+1F4A9 (💩) encodes as the surrogate pair D83D DCA9.
#guard toUtf16 0x1F4A9 = [0xD83D, 0xDCA9]
#guard toUtf16 97 = [97]
-- "a💩b": Python len = 3 code points; naive JS .length = 4 code units (D3 witness).
#guard ([97, 0x1F4A9, 98] : List Int).length = 3
#guard utf16Len [97, 0x1F4A9, 98] = 4
-- "💩x": Python s[1] = 'x' (code point 120); naive UTF-16 s[1] is the TRAILING
-- SURROGATE 0xDCA9 — not a character at all (D5 witness).
#guard (utf16Encode [0x1F4A9, 120])[1]? = some 0xDCA9

/-- The concrete D3 witness as a theorem: on the astral string "a💩b" the
    UTF-16 length is NOT the code-point length. -/
theorem utf16_deviation_witness :
    utf16Len [97, 0x1F4A9, 98] ≠ ([97, 0x1F4A9, 98] : List Int).length := by decide

/-- The concrete D5 witness: naive UTF-16 indexing of "💩x" at 1 does NOT
    yield the code point Python yields (120); it yields a trailing surrogate. -/
theorem utf16_index_deviation_witness :
    (utf16Encode [0x1F4A9, 120])[1]? ≠ some 120 := by decide

theorem utf16Units_pos (cp : Int) : 1 ≤ utf16Units cp := by
  unfold utf16Units; split <;> omega

/-- Python `len` never exceeds naive JS `.length`. -/
theorem len_le_utf16Len (cps : List Int) : cps.length ≤ utf16Len cps := by
  induction cps with
  | nil => simp [utf16Len]
  | cons c rest ih =>
      have hc := utf16Units_pos c
      simp only [List.length_cons, utf16Len]
      omega

/-- **The D3 deviation is real for EVERY astral string** (not just the pinned
    example): any string containing a code point ≥ U+10000 has a UTF-16 length
    STRICTLY greater than its Python (code-point) length — so naive JS
    `.length`/`s[i]` CANNOT implement Python semantics, and the compiler's
    code-point helpers are necessary, not stylistic. -/
theorem utf16_astral_strict (cps : List Int) (c : Int) (hc : c ∈ cps)
    (hastral : 0x10000 ≤ c) : cps.length < utf16Len cps := by
  induction cps with
  | nil => cases hc
  | cons d rest ih =>
      simp only [List.length_cons, utf16Len]
      rcases List.mem_cons.mp hc with h1 | h2
      · subst h1
        have h2u : utf16Units c = 2 := by unfold utf16Units; rw [if_pos hastral]
        have := len_le_utf16Len rest
        omega
      · have := ih h2
        have := utf16Units_pos d
        omega

/-! ### The string expression fragment and its two semantics -/

inductive SExp where
  | slit (cps : List Int)             -- string literal (its code points)
  | ilit (n : Int)                    -- int literal (indices, slice bounds)
  | var (s : String)
  | iadd (a b : SExp) | isub (a b : SExp) | ifdiv (a b : SExp)  -- int sub-fragment (`//` deviates)
  | sconcat (a b : SExp)              -- Python `a + b` on strings
  | slen (e : SExp)                   -- Python `len(s)` — CODE POINTS, not UTF-16 units
  | sindex (s i : SExp)               -- Python `s[i]` — code-point index, `getIndex` normalization
  | sslice (s lo hi : SExp)           -- Python `s[lo:hi]` — code-point slice, `sliceWalk` normalization
deriving Repr

/-- Read the code points at the (already-normalized, in-bounds by
    `slice_walk_inbounds`) visited indices. Structural, `simp`-friendly. -/
def takeCps (cps : List Int) : List Int → List Int
  | [] => []
  | j :: rest =>
      match cps[j.toNat]? with
      | some c => c :: takeCps cps rest
      | none => takeCps cps rest

/-- String-fragment REFERENCE eval (the Python side is `evalS11 false`).
    `sconcat`/`slen`/`sindex`/`sslice` are code-point-correct because the
    compiler emits code-point helpers (never naive UTF-16 `.length`/`s[i]` —
    see the `utf16_*` witnesses above for what those would do). The `tgt` flag
    is DOCUMENTED-LEGACY (the historical wave-11 `Bool`-flag copy, F1 shape):
    NO theorem references `evalS11 true` — the compiled semantics is the
    INDEPENDENT `evalS11tgt` below (C1-rollout wave 9). -/
def evalS11 (tgt : Bool) : SExp → SEnv → Option SVal
  | .slit cps, _ => some (.sstr cps)
  | .ilit n, _ => some (.sint n)
  | .var s, env => env.get s
  | .iadd a b, env => match evalS11 tgt a env, evalS11 tgt b env with
      | some (.sint x), some (.sint y) => some (.sint (x + y)) | _, _ => none
  | .isub a b, env => match evalS11 tgt a env, evalS11 tgt b env with
      | some (.sint x), some (.sint y) => some (.sint (x - y)) | _, _ => none
  | .ifdiv a b, env => match evalS11 tgt a env, evalS11 tgt b env with
      | some (.sint x), some (.sint y) =>
          if y = 0 then none else some (.sint (if tgt then jsFdiv x y else Int.fdiv x y))
      | _, _ => none
  | .sconcat a b, env => match evalS11 tgt a env, evalS11 tgt b env with
      | some (.sstr xs), some (.sstr ys) => some (.sstr (xs ++ ys)) | _, _ => none
  | .slen e, env => match evalS11 tgt e env with
      | some (.sstr cps) => some (.sint cps.length) | _ => none
  | .sindex s i, env => match evalS11 tgt s env, evalS11 tgt i env with
      | some (.sstr cps), some (.sint k) =>
          ((getIndex cps.length k).bind (fun j => cps[j.toNat]?)).map (fun c => .sstr [c])
      | _, _ => none
  | .sslice s lo hi, env =>
      match evalS11 tgt s env, evalS11 tgt lo env, evalS11 tgt hi env with
      | some (.sstr cps), some (.sint l), some (.sint h) =>
          some (.sstr (takeCps cps (sliceWalk cps.length (some l) (some h) 1)))
      | _, _, _ => none

/-! ### Wave 9 (C1 rollout) — INDEPENDENT-target string preservation

The previous `preservationS11 : evalS11 true e env = evalS11 false e env` was
the F1 model-vs-model tautology: ONE evaluator with a `Bool` flag flipping only
the `//` arm — stubbing the shipping lowering could not break it. Re-architected
on the wave-1 recipe: `evalS11tgt` is a SEPARATE recursion, parameterized by the
integer-division lowering the emitted JS uses; the `//` in INDEX/BOUND
arithmetic routes through `L`, while indexing/slicing keep the verified-core
`getIndex`/`sliceWalk` code-point helpers on BOTH sides (those are certified by
the Tier-1/2 lemmas and are exactly what the compiler emits — the D3/D5 UTF-16
deviation they absorb stays proved real by `utf16_astral_strict`). The SAME
predicate (`S11Preserves`) is proved for the shipped floor-correction
(`preservationS11_real`) and REFUTED for the naive truncating lowering
(`preservationS11_stub_fails`) on a witness where floor vs truncation select
DIFFERENT characters of the string. -/

/-- **Independent target evaluator** for the string fragment: the compiled
    program's semantics under lowering `L`. A SEPARATE recursion (not a `Bool`
    flag on `evalS11`); the `//` arm (index/bound arithmetic) calls the
    lowering's operation, mirroring the emitted JS, and the string ops use the
    same code-point helpers (`getIndex`/`sliceWalk`/`takeCps`) the compiler
    actually emits — never naive UTF-16 `.length`/`s[i]`. -/
def evalS11tgt (L : IntDivLowering) : SExp → SEnv → Option SVal
  | .slit cps, _ => some (.sstr cps)
  | .ilit n, _ => some (.sint n)
  | .var s, env => env.get s
  | .iadd a b, env => match evalS11tgt L a env, evalS11tgt L b env with
      | some (.sint x), some (.sint y) => some (.sint (x + y)) | _, _ => none
  | .isub a b, env => match evalS11tgt L a env, evalS11tgt L b env with
      | some (.sint x), some (.sint y) => some (.sint (x - y)) | _, _ => none
  | .ifdiv a b, env => match evalS11tgt L a env, evalS11tgt L b env with
      | some (.sint x), some (.sint y) =>
          if y = 0 then none else some (.sint (L.fdiv x y))
      | _, _ => none
  | .sconcat a b, env => match evalS11tgt L a env, evalS11tgt L b env with
      | some (.sstr xs), some (.sstr ys) => some (.sstr (xs ++ ys)) | _, _ => none
  | .slen e, env => match evalS11tgt L e env with
      | some (.sstr cps) => some (.sint cps.length) | _ => none
  | .sindex s i, env => match evalS11tgt L s env, evalS11tgt L i env with
      | some (.sstr cps), some (.sint k) =>
          ((getIndex cps.length k).bind (fun j => cps[j.toNat]?)).map (fun c => .sstr [c])
      | _, _ => none
  | .sslice s lo hi, env =>
      match evalS11tgt L s env, evalS11tgt L lo env, evalS11tgt L hi env with
      | some (.sstr cps), some (.sint l), some (.sint h) =>
          some (.sstr (takeCps cps (sliceWalk cps.length (some l) (some h) 1)))
      | _, _, _ => none

/-- The compiled string semantics: the independent target under the SHIPPED
    lowering. -/
abbrev evalS11js : SExp → SEnv → Option SVal := evalS11tgt jsELowering

-- Executable bindings (via SVal.asCps/asInt) — F9 pins: the REFERENCE
-- `evalS11 false` matches CPython, and the compiled `evalS11js` guards mirror
-- them (the retired `evalS11 true` legacy guards now live on the independent
-- target).
-- "hello"[-1] = 'o' (code point 111) — negative-index normalization, both sides.
#guard ((evalS11 false (.sindex (.slit [104, 101, 108, 108, 111]) (.ilit (-1))) []).bind SVal.asCps) = some [111]
#guard ((evalS11js (.sindex (.slit [104, 101, 108, 108, 111]) (.ilit (-1))) []).bind SVal.asCps) = some [111]
-- THE DEVIATION PIN (CPython): "hello"[-7//2] = "hello"[-4] = 'e' (101) —
-- floor. JS-trunc would give -3 → "hello"[-3] = 'l' (108): the KEPT CHARACTER
-- differs, so this pin discriminates the lowerings (see the stub contrast below).
#guard ((evalS11 false (.sindex (.slit [104, 101, 108, 108, 111]) (.ifdiv (.ilit (-7)) (.ilit 2))) []).bind SVal.asCps) = some [101]
-- THE ASTRAL WITNESS: len("a💩b") = 3 code points — CPython reference AND the
-- COMPILED semantics — NOT the 4 that naive UTF-16 `.length` gives (utf16Len
-- guard above).
#guard ((evalS11 false (.slen (.slit [97, 0x1F4A9, 98])) []).bind SVal.asInt) = some 3
#guard ((evalS11js (.slen (.slit [97, 0x1F4A9, 98])) []).bind SVal.asInt) = some 3
-- "💩x"[1] = 'x' (120) — reference and COMPILED — naive UTF-16 s[1] would be
-- the trailing surrogate 0xDCA9 (guard above), not a character.
#guard ((evalS11 false (.sindex (.slit [0x1F4A9, 120]) (.ilit 1)) []).bind SVal.asCps) = some [120]
#guard ((evalS11js (.sindex (.slit [0x1F4A9, 120]) (.ilit 1)) []).bind SVal.asCps) = some [120]
-- "python"[1:4] = "yth" — code-point slice through the verified-core sliceWalk.
#guard ((evalS11 false (.sslice (.slit [112, 121, 116, 104, 111, 110]) (.ilit 1) (.ilit 4)) []).bind SVal.asCps) = some [121, 116, 104]
-- "hello"[-3:-1] = "ll" — negative slice bounds normalize (CPython).
#guard ((evalS11 false (.sslice (.slit [104, 101, 108, 108, 111]) (.ilit (-3)) (.ilit (-1))) []).bind SVal.asCps) = some [108, 108]
-- SLICE-BOUND deviation pin (CPython): "python"[1:-7//2] = "python"[1:-4] = "y"
-- ([121]); trunc -3 would give "python"[1:-3] = "yt" ([121, 116]) — the slice
-- CONTENT differs, discriminating the lowerings through a bound.
#guard ((evalS11 false (.sslice (.slit [112, 121, 116, 104, 111, 110]) (.ilit 1) (.ifdiv (.ilit (-7)) (.ilit 2))) []).bind SVal.asCps) = some [121]
-- "hi"[5] → IndexError (none); astral OOB too.
#guard (evalS11 false (.sindex (.slit [104, 105]) (.ilit 5)) []).isNone
#guard (evalS11 false (.sindex (.slit [0x1F4A9]) (.ilit 1)) []).isNone
-- ("py" + "thon") length 6, and concat is code-point concat ("a" + "💩" has len 2).
#guard ((evalS11 false (.slen (.sconcat (.slit [112, 121]) (.slit [116, 104, 111, 110]))) []).bind SVal.asInt) = some 6
#guard ((evalS11js (.slen (.sconcat (.slit [97]) (.slit [0x1F4A9]))) []).bind SVal.asInt) = some 2

/-- String preservation as a predicate OVER the lowering — the SAME predicate
    is proved for the shipped lowering (`preservationS11_real`) and REFUTED
    for the stub (`preservationS11_stub_fails`). -/
def S11Preserves (L : IntDivLowering) : Prop :=
  ∀ e env, evalS11tgt L e env = evalS11 false e env

/-- **String preservation (Tier-3 wave 11, C1-rollout wave 9 re-architected).**
    The INDEPENDENT compiled target under the shipped lowering computes the
    Python reference on every string-fragment expression and environment:
    concat/len/index/slice are code-point-correct on both sides (the emitted
    helpers absorb the D3/D5 UTF-16 deviation — proved real by
    `utf16_astral_strict`), and the `//` deviation in index/bound arithmetic
    is absorbed by the emitted floor-correction (`jsFdiv_eq_fdiv`). Real
    structural induction, not `rfl`: the deviation arm needs the arithmetic
    binding lemma. -/
theorem preservationS11 (e : SExp) (env : SEnv) :
    evalS11js e env = evalS11 false e env := by
  induction e with
  | slit cps => simp only [evalS11tgt, evalS11]
  | ilit n => simp only [evalS11tgt, evalS11]
  | var s => simp only [evalS11tgt, evalS11]
  | iadd a b iha ihb => simp only [evalS11tgt, evalS11, iha, ihb]
  | isub a b iha ihb => simp only [evalS11tgt, evalS11, iha, ihb]
  | ifdiv a b iha ihb =>
      simp only [evalS11tgt, evalS11, iha, ihb]
      cases evalS11 false a env with
      | none => rfl
      | some va =>
        cases evalS11 false b env with
        | none => cases va <;> rfl
        | some vb =>
          cases va with
          | sstr _ => rfl
          | sint x =>
            cases vb with
            | sstr _ => rfl
            | sint y =>
              by_cases hy : y = 0
              · simp [hy]
              · simp [hy, jsELowering, jsFdiv_eq_fdiv x y hy]
  | sconcat a b iha ihb => simp only [evalS11tgt, evalS11, iha, ihb]
  | slen e1 ih => simp only [evalS11tgt, evalS11, ih]
  | sindex s i ihs ihi => simp only [evalS11tgt, evalS11, ihs, ihi]
  | sslice s lo hi ihs ihlo ihhi => simp only [evalS11tgt, evalS11, ihs, ihlo, ihhi]

/-- The re-architected statement, in predicate form: the shipped lowering
    preserves. Same content as `preservationS11`; this is the instantiation
    the stub litmus contrasts against. -/
theorem preservationS11_real : S11Preserves jsELowering := preservationS11

/-- **Stub litmus (wave 9).** The SAME preservation predicate is FALSE for the
    naive truncating lowering, on a DISCRIMINATING string witness: for
    `"hello"[-7 // 2]` the stub computes JS-trunc `-3` → `"hello"[-3] = 'l'`
    (108) where Python floors to `-4` → `"hello"[-4] = 'e'` (101) — the KEPT
    CHARACTER differs, not merely the intermediate index. This is what the old
    `evalS11 true = evalS11 false` statement could not express (both sides
    hardcoded the same arm). -/
theorem preservationS11_stub_fails : ¬ S11Preserves truncELowering := by
  intro h
  have hc := h (.sindex (.slit [104, 101, 108, 108, 111])
      (.ifdiv (.ilit (-7)) (.ilit 2))) []
  -- hc projects (via asCps) to `some [108] = some [101]`:
  -- stub `Int.tdiv (-7) 2 = -3` → 'l'; Python `Int.fdiv (-7) 2 = -4` → 'e'.
  have hc' := congrArg (fun o => o.bind SVal.asCps) hc
  exact absurd hc' (by decide)

-- The contrast, concretely (stub is a plausible naive emission, and it selects
-- a DIFFERENT character):
#guard ((evalS11js (.sindex (.slit [104, 101, 108, 108, 111])
        (.ifdiv (.ilit (-7)) (.ilit 2))) []).bind SVal.asCps) = some [101]  -- real: floor -4 → 'e'
#guard ((evalS11tgt truncELowering (.sindex (.slit [104, 101, 108, 108, 111])
        (.ifdiv (.ilit (-7)) (.ilit 2))) []).bind SVal.asCps) = some [108]  -- stub: trunc -3 → 'l' ✗
-- and through a SLICE BOUND: real "python"[1:-4] = "y", stub "python"[1:-3] = "yt" ✗
#guard ((evalS11js (.sslice (.slit [112, 121, 116, 104, 111, 110]) (.ilit 1)
        (.ifdiv (.ilit (-7)) (.ilit 2))) []).bind SVal.asCps) = some [121]
#guard ((evalS11tgt truncELowering (.sslice (.slit [112, 121, 116, 104, 111, 110]) (.ilit 1)
        (.ifdiv (.ilit (-7)) (.ilit 2))) []).bind SVal.asCps) = some [121, 116]

-- SPOT: the `//` deviation FEEDING a string subscript — `"hello"[-7 // 2]` on
-- the INDEPENDENT compiled target, routed THROUGH the theorem to the Python
-- reference and evaluated there: -7//2 = -4 (floor), "hello"[-4] = 'e' (101).
-- JS-trunc would give -3 → 'l' (108), so this closes ONLY because the theorem
-- equates the full evals; a weakened (e.g. both-sides-`none`) statement leaves
-- the `some` goal unprovable.
example :
    (evalS11js (.sindex (.slit [104, 101, 108, 108, 111])
        (.ifdiv (.ilit (-7)) (.ilit 2))) []).bind SVal.asCps
      = some [101] := by
  rw [preservationS11]
  decide

-- SPOT 2: an astral string BUILT by concat, then measured — len("a" + "💩b")
-- on the INDEPENDENT compiled target, through the theorem, = 3 code points
-- (naive UTF-16 would report 4).
example :
    (evalS11js (.slen (.sconcat (.slit [97]) (.slit [0x1F4A9, 98]))) []).bind SVal.asInt
      = some 3 := by
  rw [preservationS11]
  decide

-- SPOT 3: the deviation through a SLICE BOUND — `"python"[1:-7 // 2]` on the
-- independent target, through the theorem: floor bound -4 keeps only "y"
-- ([121]); trunc -3 would keep "yt".
example :
    (evalS11js (.sslice (.slit [112, 121, 116, 104, 111, 110]) (.ilit 1)
        (.ifdiv (.ilit (-7)) (.ilit 2))) []).bind SVal.asCps
      = some [121] := by
  rw [preservationS11]
  decide

/-- info: 'PythExpandVerify.preservationS11' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationS11

/-- info: 'PythExpandVerify.preservationS11_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationS11_real

/-- info: 'PythExpandVerify.preservationS11_stub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationS11_stub_fails

/-- info: 'PythExpandVerify.utf16_astral_strict' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms utf16_astral_strict

/-! ## Tier-3 wave 12 — WASM integer floor-division / modulo (the obligation wave 8 left open)

Wave 8 proved i64 `+`/`-`/`×` preserve as `evalW = (evalPyW ·).map wrapI64`
and EXCLUDED `//`: raw `i64.div_s` truncates toward zero while Python floors,
and division does not commute with wrapping, so `//` could not ride the
wrapping-homomorphism argument. This wave discharges that excluded obligation
against the SHIPPED lowering: `emit_checked_i64_floordiv`
(`crates/pyths_codegen_wasm/src/emit.rs`, #358) does NOT emit raw `div_s` —
it emits `div_s` + `rem_s` + a sign-adjust
(`if r ≠ 0 ∧ (a xor b) < 0 then q := q - 1`), and `emit_checked_i64_mod`
emits `rem_s` + (`if r ≠ 0 ∧ (r xor b) < 0 then r := r + b`). The K9
three-way differential empirically confirmed the shipped `//`/`%`
FLOOR-match CPython (`-7 // 2 = -4` not `-3`; `-7 % 2 = 1` not `-1`);
`wasmFdiv_floor_correct` / `wasmMod_floor_correct` now make that a theorem:
the modeled sequences equal `wrapI64 (Int.fdiv a b)` / `wrapI64 (Int.fmod a b)`
— floor toward −∞, NOT truncation (`wrapI64 (Int.tdiv a b)`) — and the
divergence `#guard`s below prove floor ≠ trunc on sign-mixed operands inside
the frame, so the emitted correction is necessary, not cosmetic. The proofs
reuse the seed's `jsFdiv_eq_fdiv`/`jsFmod_eq_fmod` (the JS twin's floor
correction), so both backends' `//` are certified against the SAME
`Int.fdiv`/`Int.fmod` reference.

FRAME (honest):
- `b = 0` is out of frame: `i64.div_s`/`i64.rem_s` trap there, surfaced as
  Python `ZeroDivisionError`; `hb : b ≠ 0` is a genuine precondition, and
  the theorems are non-vacuous under it (the divergence `#guard`s are
  witnesses inside the frame).
- the single pair `(-2^63, -1)` is `__ovf`-guarded in the shipped `//`
  (raw `div_s` would trap on it); the glue re-runs that call on the exact JS
  twin, whose floor-correctness is `jsFdiv_eq_fdiv`. The MODEL evaluates the
  unguarded correction sequence on the mathematical-integer plane there
  (`Int.tdiv` is total), and its wrap-plane value `-2^63` still equals
  `wrapI64 (Int.fdiv (-2^63) (-1))` — so the theorem holds on ALL of
  `b ≠ 0` — but the SHIPPED value at that one pair is the exact `2^63` via
  the twin, strictly better than the wrap. `%` needs no such guard:
  `i64.rem_s (-2^63) (-1) = 0` is defined in WASM and matches Python.
- for in-range i64 operands outside that pair, every intermediate the
  machine computes stays in i64 range (the `q - 1` adjust needs `r ≠ 0`,
  impossible at `|b| = 1`, so `|q| < 2^62` whenever it fires; `|r| < |b|`;
  `r + b` moves toward zero), so the registers carry exactly the model's
  integers and the exit `wrapI64` is the identity — the value-equality
  corollaries `wasmFdiv_eq_fdiv_inRange` / `wasmMod_eq_fmod_inRange` state
  that reading hypothesis-style, exactly as `preservationWasm_inRange`. -/

/-- The shipped WASM `//` on the i64 fast path (`emit_checked_i64_floordiv`):
    `q := i64.div_s a b` (truncating), `r := i64.rem_s a b`, then the
    sign-adjust `if r ≠ 0 ∧ sign a ≠ sign b then q - 1` (emitted as the test
    `(a xor b) < 0` on the operand registers), wrapped to i64 on exit. -/
def wfdiv (a b : Int) : Int :=
  let q := Int.tdiv a b        -- i64.div_s truncates toward zero
  let r := Int.tmod a b        -- i64.rem_s carries the DIVIDEND's sign
  wrapI64 (if r = 0 then q
           else if decide (a < 0) = decide (b < 0) then q
           else q - 1)

/-- The shipped WASM `%` (`emit_checked_i64_mod`): `r := i64.rem_s a b`, then
    `if r ≠ 0 ∧ sign r ≠ sign b then r + b` (emitted as `(r xor b) < 0`),
    wrapped to i64 on exit. -/
def wmod (a b : Int) : Int :=
  let r := Int.tmod a b
  wrapI64 (if r = 0 then r
           else if decide (r < 0) = decide (b < 0) then r
           else r + b)

-- CPython pins (K9's empirical floor-match, now by evaluation of the model):
#guard wfdiv (-7) 2 = -4         -- CPython: -7 // 2 == -4 (floor, NOT -3)
#guard wfdiv 7 (-2) = -4         -- CPython: 7 // -2 == -4
#guard wfdiv (-7) (-2) = 3       -- CPython: -7 // -2 == 3
#guard wfdiv 7 2 = 3
#guard wmod (-7) 2 = 1           -- CPython: -7 % 2 == 1 (sign of the divisor)
#guard wmod 7 (-2) = -1          -- CPython: 7 % -2 == -1
#guard wmod (-7) (-2) = -1
#guard wfdiv (-7) 2 * 2 + wmod (-7) 2 = -7   -- the Python q·b + r = a identity

-- DIVERGENCE witnesses — floor ≠ trunc, so the correction is NECESSARY:
-- raw `i64.div_s`/`i64.rem_s` would ship the WRONG (truncated) values.
#guard Int.fdiv (-7) 2 = -4
#guard Int.tdiv (-7) 2 = -3                    -- raw div_s: truncation
#guard Int.fdiv (-7) 2 ≠ Int.tdiv (-7) 2
#guard wfdiv (-7) 2 ≠ wrapI64 (Int.tdiv (-7) 2)  -- shipped ≠ uncorrected div_s
#guard Int.tmod (-7) 2 = -1                    -- raw rem_s: sign of dividend
#guard wmod (-7) 2 ≠ wrapI64 (Int.tmod (-7) 2)   -- shipped ≠ uncorrected rem_s

-- a case past 2^53 (the f64-round-trip precision bug #358 replaced): exact i64
#guard wfdiv (-(2 ^ 62 + 1)) 2 = -(2 ^ 61) - 1
#guard Int.tdiv (-(2 ^ 62 + 1)) 2 = -(2 ^ 61)   -- trunc diverges here too

-- the __ovf-guarded corner, wrap-plane reading (see FRAME: the shipped binary
-- re-routes this single pair to the exact JS twin; the model's unguarded wrap
-- agrees with the theorem's RHS, and `%` at the pair is in-frame and exact):
#guard wfdiv (-(2 ^ 63)) (-1) = -(2 ^ 63)
#guard wrapI64 (Int.fdiv (-(2 ^ 63)) (-1)) = -(2 ^ 63)
#guard wmod (-(2 ^ 63)) (-1) = 0                -- i64.rem_s(MIN, -1) = 0, defined

/-- When `i64.rem_s` is nonzero it carries the DIVIDEND's sign, so the
    emitted `(a xor b) < 0` operand-sign test coincides with the JS twin's
    remainder-sign test — the bridge between `wfdiv`'s condition and
    `jsFdiv`'s. -/
private theorem tmod_neg_iff_dividend_neg {a b : Int} (h0 : Int.tmod a b ≠ 0) :
    Int.tmod a b < 0 ↔ a < 0 := by
  have h1 : 0 ≤ a → 0 ≤ Int.tmod a b := fun h => Int.tmod_nonneg b h
  have h2 : a ≤ 0 → Int.tmod a b ≤ 0 := fun h => tmod_nonpos_of_nonpos b h
  omega

/-- Core: the `div_s`-then-sign-adjust sequence computes EXACTLY Python floor
    division on the integer plane (`y ≠ 0`) — the WASM analogue of
    `jsFdiv_eq_fdiv`, and proved by routing through it. -/
private theorem wfdiv_core (a b : Int) (hb : b ≠ 0) :
    (if Int.tmod a b = 0 then Int.tdiv a b
     else if decide (a < 0) = decide (b < 0) then Int.tdiv a b
     else Int.tdiv a b - 1) = Int.fdiv a b := by
  rw [← jsFdiv_eq_fdiv a b hb]
  simp only [jsFdiv]
  rw [Int.mul_comm (Int.tdiv a b) b, ← Int.tmod_def]
  by_cases h0 : Int.tmod a b = 0
  · rw [if_pos h0, if_pos h0]
  · rw [if_neg h0, if_neg h0]
    have hs : Int.tmod a b < 0 ↔ a < 0 := tmod_neg_iff_dividend_neg h0
    have hcond : (decide (Int.tmod a b < 0) = decide (b < 0))
        ↔ (decide (a < 0) = decide (b < 0)) := by
      simp only [decide_eq_decide]
      exact iff_congr hs Iff.rfl
    by_cases hc : decide (a < 0) = decide (b < 0)
    · rw [if_pos hc, if_pos (hcond.mpr hc)]
    · rw [if_neg hc, if_neg (fun h => hc (hcond.mp h))]

/-- Core: the `rem_s`-then-add-divisor sequence computes EXACTLY Python `%`
    (sign of the divisor) on the integer plane (`y ≠ 0`). -/
private theorem wmod_core (a b : Int) (hb : b ≠ 0) :
    (if Int.tmod a b = 0 then Int.tmod a b
     else if decide (Int.tmod a b < 0) = decide (b < 0) then Int.tmod a b
     else Int.tmod a b + b) = Int.fmod a b := by
  rw [← jsFmod_eq_fmod a b hb]
  simp only [jsFmod, jsFdiv]
  rw [Int.mul_comm (Int.tdiv a b) b, ← Int.tmod_def]
  by_cases h0 : Int.tmod a b = 0
  · rw [if_pos h0, if_pos h0]
    rw [Int.mul_comm (Int.tdiv a b) b]
    exact Int.tmod_def a b
  · rw [if_neg h0, if_neg h0]
    by_cases hc : decide (Int.tmod a b < 0) = decide (b < 0)
    · rw [if_pos hc, if_pos hc]
      rw [Int.mul_comm (Int.tdiv a b) b]
      exact Int.tmod_def a b
    · rw [if_neg hc, if_neg hc]
      rw [Int.sub_mul, Int.one_mul, Int.mul_comm (Int.tdiv a b) b,
          Int.tmod_def a b]
      generalize b * Int.tdiv a b = t
      omega

/-- **WASM `//` floor-preservation (Tier-3 wave 12).** The shipped WASM
    integer floor-division — `div_s` + `rem_s` + sign-adjust, wrapped to i64 —
    computes exactly the i64 wrap of Python's FLOOR division `Int.fdiv`
    (toward −∞), NOT of the truncation `Int.tdiv` (the divergence `#guard`s
    above witness `wfdiv (-7) 2 = -4 ≠ wrapI64 (Int.tdiv (-7) 2) = -3`).
    `hb` is genuine: `b = 0` traps (Python `ZeroDivisionError`), out of
    frame. -/
theorem wasmFdiv_floor_correct (a b : Int) (hb : b ≠ 0) :
    wfdiv a b = wrapI64 (Int.fdiv a b) := by
  simp only [wfdiv]
  exact congrArg wrapI64 (wfdiv_core a b hb)

/-- **WASM `%` floor-preservation (Tier-3 wave 12).** The shipped WASM
    integer mod — `rem_s` + add-divisor correction, wrapped to i64 — computes
    exactly the i64 wrap of Python's `Int.fmod` (result takes the DIVISOR's
    sign), not of the raw `rem_s` value `Int.tmod` (dividend's sign). -/
theorem wasmMod_floor_correct (a b : Int) (hb : b ≠ 0) :
    wmod a b = wrapI64 (Int.fmod a b) := by
  simp only [wmod]
  exact congrArg wrapI64 (wmod_core a b hb)

/-- Value-EQUALITY corollary (the shipped-machine reading, DERIVED): whenever
    Python's floor quotient is i64-representable — automatic for in-range
    operands outside the `__ovf`-guarded `(-2^63, -1)` pair — the WASM `//`
    result IS Python's, exactly (`preservationWasm_inRange`-style). -/
theorem wasmFdiv_eq_fdiv_inRange (a b : Int) (hb : b ≠ 0)
    (h1 : -(2 ^ 63) ≤ Int.fdiv a b) (h2 : Int.fdiv a b < 2 ^ 63) :
    wfdiv a b = Int.fdiv a b := by
  rw [wasmFdiv_floor_correct a b hb, wrapI64_eq_self h1 h2]

/-- Value-EQUALITY corollary for `%`. -/
theorem wasmMod_eq_fmod_inRange (a b : Int) (hb : b ≠ 0)
    (h1 : -(2 ^ 63) ≤ Int.fmod a b) (h2 : Int.fmod a b < 2 ^ 63) :
    wmod a b = Int.fmod a b := by
  rw [wasmMod_floor_correct a b hb, wrapI64_eq_self h1 h2]

/-! ### The GUARDED shipping dispatch (the `__ovf` re-route)

`wfdiv` models the UNGUARDED `div_s`+adjust sequence on the mathematical-integer
plane, and `wasmFdiv_floor_correct` states `wfdiv a b = wrapI64 (Int.fdiv a b)`.
But the SHIPPED binary does NOT run `wfdiv` on the single overflow pair
`(-2^63, -1)`: raw `i64.div_s` traps there, so the emitted code `__ovf`-guards it
and re-routes to the exact JS BigInt twin, which returns the exact `2^63` — NOT
`wfdiv`'s wrap-plane value `-2^63`. So a theorem about `wfdiv` alone is not a
theorem about SHIPPED behaviour at that pair. `wfdivShipped` models the guarded
dispatch, and the theorems below are about it: the shipped `//` computes Python's
EXACT floor everywhere on the i64 operand domain (no wrap artifact) — the
representable region via the correction, and the one guarded pair via the exact
twin. (`%` needs no guard: `i64.rem_s (-2^63) (-1) = 0` is defined and matches
Python, already covered by `wasmMod_floor_correct`.) -/

/-- The shipped WASM `//` dispatch: the `__ovf` guard re-routes the single
    overflow pair `(-2^63, -1)` to the INDEPENDENT JS BigInt twin `jsFdiv` (the
    JS-number `//` correction — the floor-adjust over truncating `tdiv`, defined
    with NO reference to `Int.fdiv`), which the shipped binary re-runs there.
    Every other `b ≠ 0` runs the `wfdiv` i64 sequence. The twin's value equals
    Python floor only via the BINDING LEMMA `jsFdiv_eq_fdiv` (a real proof), not
    by definition — so this is not a same-helper equality. -/
def wfdivShipped (a b : Int) : Int :=
  if a = -(2 ^ 63) ∧ b = -1 then jsFdiv a b else wfdiv a b

/-- The guarded pair ships the EXACT `2^63` (the JS twin's value, proved via the
    `jsFdiv_eq_fdiv` binding lemma), NOT `wfdiv`'s wrapped `-2^63` — a divergence
    the unguarded model hid and that the independent twin now proves. -/
theorem wasmFdiv_shipped_ovf_pair : wfdivShipped (-(2 ^ 63)) (-1) = 2 ^ 63 := by
  rw [wfdivShipped, if_pos (⟨rfl, rfl⟩ : (-(2 ^ 63) : Int) = -(2 ^ 63) ∧ (-1 : Int) = -1),
      jsFdiv_eq_fdiv (-(2 ^ 63)) (-1) (by decide)]
  decide

-- The guard genuinely changes the result: the INDEPENDENT twin `jsFdiv` gives
-- the exact `2^63`, whereas the unguarded i64 sequence `wfdiv` wraps to `-2^63`.
#guard wfdivShipped (-(2 ^ 63)) (-1) = 2 ^ 63
#guard wfdiv (-(2 ^ 63)) (-1) = -(2 ^ 63)
#guard wfdivShipped (-(2 ^ 63)) (-1) ≠ wfdiv (-(2 ^ 63)) (-1)

/-- A `//` overflow lowering — how the compiler resolves the `(-2^63, -1)` pair. -/
abbrev OvfLowering := Int → Int → Int

/-- Correctness AT the guarded overflow pair: the lowering must yield Python's
    EXACT floor `2^63`, not a wrapped i64 artifact. -/
def OvfPairCorrect (L : OvfLowering) : Prop := L (-(2 ^ 63)) (-1) = 2 ^ 63

/-- **Holds for the shipped dispatch** — `wfdivShipped` re-routes the pair to the
    independent JS BigInt twin, proved correct via `jsFdiv_eq_fdiv` (a real
    binding, not by definition). -/
theorem wfdivShipped_ovf_correct : OvfPairCorrect wfdivShipped :=
  wasmFdiv_shipped_ovf_pair

/-- info: 'PythExpandVerify.wfdivShipped_ovf_correct' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms wfdivShipped_ovf_correct

/-- **FAILS for the wrong lowering (C3, the C1 pattern).** The SAME correctness
    property is REFUTED by the unguarded i64 sequence `wfdiv`, which wraps to
    `-2^63` at the overflow pair. So `wasmFdiv_shipped_ovf_pair` is non-vacuous:
    it distinguishes the re-routing dispatch from a naive lowering that omits the
    `__ovf` guard — a concrete contradiction at `(-2^63, -1)`. -/
theorem wasmFdiv_shipped_ovf_stub_fails : ¬ OvfPairCorrect wfdiv := by
  have h : wfdiv (-(2 ^ 63)) (-1) = -(2 ^ 63) := by decide
  simp only [OvfPairCorrect, h]
  decide

/-- info: 'PythExpandVerify.wasmFdiv_shipped_ovf_stub_fails' does not depend on any axioms -/
#guard_msgs in
#print axioms wasmFdiv_shipped_ovf_stub_fails

/-- **Shipped WASM `//` is EXACT Python floor division (Tier-3 wave 12).**
    Whenever Python's floor quotient is i64-representable — the entire i64
    operand domain EXCEPT the `__ovf`-guarded pair, which
    `wasmFdiv_shipped_ovf_pair` handles exactly via the independent twin — the
    shipped dispatch equals Python's `Int.fdiv` exactly (no wrap, no
    truncation). This is the theorem about SHIPPED behaviour that
    `wasmFdiv_floor_correct` (about the unguarded `wfdiv`) is not. At the guarded
    pair the shipped value is the twin's `2^63`, correctly excluded by the
    hypotheses, so no false claim is made there. -/
theorem wasmFdiv_shipped_correct (a b : Int) (hb : b ≠ 0)
    (h1 : -(2 ^ 63) ≤ Int.fdiv a b) (h2 : Int.fdiv a b < 2 ^ 63) :
    wfdivShipped a b = Int.fdiv a b := by
  unfold wfdivShipped
  by_cases hp : a = -(2 ^ 63) ∧ b = -1
  · rw [if_pos hp]; exact jsFdiv_eq_fdiv a b hb
  · rw [if_neg hp]; exact wasmFdiv_eq_fdiv_inRange a b hb h1 h2

/-- info: 'PythExpandVerify.wasmFdiv_shipped_correct' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms wasmFdiv_shipped_correct

/-- info: 'PythExpandVerify.wasmFdiv_shipped_ovf_pair' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms wasmFdiv_shipped_ovf_pair

/-- SPOT: the compiled WASM `-7 // 2`, routed THROUGH
    `wasmFdiv_floor_correct` to CPython's floor answer `-4`. NOT a `#guard`
    of the model: the proof rewrites by the theorem first, so it closes only
    because the theorem's RHS is the FLOOR plane — a silently-weakened or
    truncating statement (`wrapI64 (Int.tdiv a b)`) leaves the goal at
    `-3 = -4`, unprovable. -/
example : wfdiv (-7) 2 = -4 := by
  rw [wasmFdiv_floor_correct (-7) 2 (by decide)]
  decide

/-- SPOT: the sign-of-divisor pair — compiled WASM `7 % -2` THROUGH
    `wasmMod_floor_correct` to CPython's `-1` (raw `rem_s` would give `1`). -/
example : wmod 7 (-2) = -1 := by
  rw [wasmMod_floor_correct 7 (-2) (by decide)]
  decide

/-- SPOT: past-2^53 exactness (#358's point) — the compiled
    `-(2^62 + 1) // 2` THROUGH the in-range value-equality corollary lands on
    Python's exact `Int.fdiv`, no wrap and no f64 precision loss. -/
example : wfdiv (-(2 ^ 62 + 1)) 2 = Int.fdiv (-(2 ^ 62 + 1)) 2 :=
  wasmFdiv_eq_fdiv_inRange (-(2 ^ 62 + 1)) 2 (by decide) (by decide) (by decide)

/-- info: 'PythExpandVerify.wasmFdiv_floor_correct' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms wasmFdiv_floor_correct

/-- info: 'PythExpandVerify.wasmMod_floor_correct' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms wasmMod_floor_correct

/-! ## Tier-3 wave 13 — bitwise operations (arbitrary precision vs the JS 32-bit coercion)

Python bitwise operators are ARBITRARY-PRECISION: `1 << 40 == 1099511627776`,
`(1 << 62) >> 30 == 2**32`, `2**40 | 1` keeps all 41 bits. Native JavaScript
bitwise operators are NOT: ECMAScript `<<`/`>>`/`&`/`|`/`^` coerce BOTH
operands through `ToInt32` (wrap into the signed 32-bit range) and mask the
shift count to 5 bits (`n & 31`), so native JS `1 << 40` is `256` (count
masked to `40 & 31 = 8`) and `1 << 31` is `-2147483648` (sign wrap) — both
WRONG as Python. The compiler therefore emits BigInt bitwise helpers
(`pyShiftLeft`/`pyShiftRight`/`pyBitAnd`/`pyBitOr`/`pyBitXor`), which are
arbitrary-precision, so the COMPILED output matches CPython (the differential
runs confirm `1 << 40`, `2**40 | 1`, `(1 << 62) >> 30`, `-1 & 0xFF` all equal
the CPython values).

Shape (re-architected in the C1 rollout, wave 10 — the wave-1 recipe):
(1) `preservationBw` — the INDEPENDENT compiled target `evalBwtgt
(L : IntDivLowering)` under the SHIPPED lowering equals the Python reference
`evalBw false` for EVERY expression and environment; bitwise nodes are
arbitrary-precision on BOTH sides (the emitted helpers, never the native
operators), and the floor-division deviation axis enters in TWO arms — the
`//` node AND the `>>` node (Python `a >> n` IS the floor division
`a // 2^n`) — both routed through `L.fdiv` and closed by `jsFdiv_eq_fdiv`;
`BwPreserves L` is the predicate form, proved for `jsELowering`
(`preservationBw_real`) and REFUTED for the truncating stub
(`preservationBw_stub_fails`, discriminating negative-operand `>>` witness).
The old `evalBw true = evalBw false` statement was the F1 model-vs-model
flag; its `true` branch is retained below as documented LEGACY only, with NO
theorem referencing it. (2) A faithful model
of the NAIVE JS shift (`toInt32`/`js32Shl`, kept OUTSIDE the semantics —
exactly as wave 11 kept `utf16Len` outside) plus impossibility results:
`js32Shl_bounded` confines every naive-JS shift result to `[-2^31, 2^31)`,
so `js32_shl_strict` / `js32_shl_strict_neg` — ANY Python shift whose value
lands outside the signed 32-bit range is NOT computable by the native JS
operator, for every operand and every count — the bitwise analogue of
`utf16_astral_strict`; `js32_shl_deviation` pins the concrete `1 << 40`
witness (`256 ≠ 2^40`, count masking) and `js32_shl_sign_wrap` pins
`1 << 31` (`-2^31 ≠ 2^31`, overflow wrap — deviation even with an unmasked
count).

Model: `shl a n` = `a * 2^n` and `shr a n` = `Int.fdiv a (2^n)` (Python's
`>>` is an arithmetic FLOOR shift: `-1 >> 3 == -1`, `-7 >> 1 == -4`); shift
counts are `Nat` constructor arguments (Python raises ValueError on negative
counts — out of frame by construction). `band`/`bor`/`bxor` are modeled on
NON-NEGATIVE operands via `Nat.land`/`Nat.lor`/`Nat.xor` lifted to `Int`
(matching CPython there); negative operands evaluate to `none` = outside the
modeled fragment, not a semantic claim.

OUT of scope (documented, deliberate): `&`/`|`/`^` on NEGATIVE operands —
Python defines them on the infinite two's-complement expansion (`-1 & 0xFF
== 255`), which the emitted BigInt helpers implement but whose Lean model
deserves its own wave; `~x` bitwise NOT (same reason); shift counts as full
expressions (a `Nat` count already carries the whole 32-bit deviation).
Shifts — where the deviation bites hardest — are modeled on ALL of `Int`. -/

inductive BwExp where
  | lit (n : Int)
  | var (s : String)
  | shl (a : BwExp) (n : Nat)   -- Python `a << n` (arbitrary precision)
  | shr (a : BwExp) (n : Nat)   -- Python `a >> n` (arithmetic floor shift)
  | band (a b : BwExp)          -- Python `a & b`, non-negative operands only
  | bor (a b : BwExp)           -- Python `a | b`, non-negative operands only
  | bxor (a b : BwExp)          -- Python `a ^ b`, non-negative operands only
  | fdiv (a b : BwExp)          -- Python `a // b` — the seed's deviation axis
deriving Repr

/-- Bitwise-fragment eval — `tgt = false` is the Python REFERENCE semantics
    (the only branch any theorem uses; pinned to CPython below). The
    `tgt = true` branch is LEGACY (the former F1 model-vs-model flag, which
    deviated only in the `//` arm and hardcoded `Int.fdiv` in the `>>` arm on
    BOTH sides) and is NOT the compiled target — the genuine compiled target
    is the INDEPENDENT `evalBwtgt (L : IntDivLowering)` below; NO theorem
    references `evalBw true`. Every bitwise node is arbitrary-precision (the
    compiler emits BigInt helpers, never the native 32-bit operators — see
    `js32Shl` below for what THOSE would compute). -/
def evalBw (tgt : Bool) : BwExp → Env → Option Int
  | .lit n, _ => some n
  | .var s, env => env.get s
  | .shl a n, env => (evalBw tgt a env).map (fun x => x * 2 ^ n)
  | .shr a n, env => (evalBw tgt a env).map (fun x => Int.fdiv x (2 ^ n))
  | .band a b, env => match evalBw tgt a env, evalBw tgt b env with
      | some x, some y =>
          if 0 ≤ x ∧ 0 ≤ y then some (Int.ofNat (x.toNat &&& y.toNat)) else none
      | _, _ => none
  | .bor a b, env => match evalBw tgt a env, evalBw tgt b env with
      | some x, some y =>
          if 0 ≤ x ∧ 0 ≤ y then some (Int.ofNat (x.toNat ||| y.toNat)) else none
      | _, _ => none
  | .bxor a b, env => match evalBw tgt a env, evalBw tgt b env with
      | some x, some y =>
          if 0 ≤ x ∧ 0 ≤ y then some (Int.ofNat (x.toNat ^^^ y.toNat)) else none
      | _, _ => none
  | .fdiv a b, env => match evalBw tgt a env, evalBw tgt b env with
      | some x, some y =>
          if y = 0 then none else some (if tgt then jsFdiv x y else Int.fdiv x y)
      | _, _ => none

-- F9 pins: the REFERENCE `evalBw false` matches CPython on the bitwise
-- fragment INCLUDING both deviation axes (legacy `evalBw true` guards
-- retired — the compiled side is now pinned on `evalBwjs` below).
-- 1 << 40 = 1099511627776 (the headline value — native JS says 256).
#guard evalBw false (.shl (.lit 1) 40) [] = some 1099511627776
-- (1 << 62) >> 30 = 4294967296 (= 2^32) — a round trip entirely past 32 bits.
#guard evalBw false (.shr (.shl (.lit 1) 62) 30) [] = some 4294967296
-- 2**40 | 1 = 1099511627777 — OR keeps bit 40.
#guard evalBw false (.bor (.shl (.lit 1) 40) (.lit 1)) [] = some 1099511627777
-- 0xF0 | 0x0F = 255, 0xFF ^ 0x0F = 240, 0xF0 & 0xFF = 240 (CPython).
#guard evalBw false (.bor (.lit 0xF0) (.lit 0x0F)) [] = some 255
#guard evalBw false (.bxor (.lit 0xFF) (.lit 0x0F)) [] = some 240
#guard evalBw false (.band (.lit 0xF0) (.lit 0xFF)) [] = some 240
-- Python `>>` is an ARITHMETIC FLOOR shift (CPython: -1 >> 3 == -1,
-- -7 >> 1 == -4 — truncation would say -3: the DISCRIMINATING negative case).
#guard evalBw false (.shr (.lit (-1)) 3) [] = some (-1)
#guard evalBw false (.shr (.lit (-7)) 1) [] = some (-4)
-- a variable through the env: x << 8 with x = 5 is 1280.
#guard evalBw false (.shl (.var "x") 8) [("x", 5)] = some 1280
-- `//` INSIDE a bitwise expression: (-7 // 2) << 1 = -4 * 2 = -8
-- (floor; truncation would give -3 << 1 = -6).
#guard evalBw false (.shl (.fdiv (.lit (-7)) (.lit 2)) 1) [] = some (-8)
-- negative `&` operand → none: OUTSIDE the modeled fragment (see header).
#guard (evalBw false (.band (.lit (-1)) (.lit 0xFF)) []).isNone
-- division by zero → none (ZeroDivisionError).
#guard (evalBw false (.fdiv (.lit 1) (.lit 0)) []).isNone

/-- **Independent target evaluator** for the bitwise fragment: the compiled
    program's semantics under integer-division lowering `L` — a SEPARATE
    recursion, not a `Bool` flag on `evalBw`. The floor-division deviation
    axis enters in TWO arms: the `//` node AND the `>>` node — Python
    `a >> n` IS the floor division `a // 2^n` (`-7 >> 1 == -4`, not
    truncation's `-3`), so a lowering that truncates gets `>>` wrong on
    negative operands too; BOTH arms route through `L.fdiv`, so the lowering
    is what varies. `&`/`|`/`^`/`<<` are arbitrary-precision Int operations
    IDENTICAL to the reference — faithfully so, NOT shared-wrong: the
    reference arms are pinned to CPython above, and `js32_shl_strict` below
    proves the naive 32-bit alternative CANNOT compute them past 32 bits, so
    "arbitrary precision on both sides" is the verified content of those
    arms (the emitted BigInt helpers), not an artifact of copying. -/
def evalBwtgt (L : IntDivLowering) : BwExp → Env → Option Int
  | .lit n, _ => some n
  | .var s, env => env.get s
  | .shl a n, env => (evalBwtgt L a env).map (fun x => x * 2 ^ n)
  | .shr a n, env => (evalBwtgt L a env).map (fun x => L.fdiv x (2 ^ n))
  | .band a b, env => match evalBwtgt L a env, evalBwtgt L b env with
      | some x, some y =>
          if 0 ≤ x ∧ 0 ≤ y then some (Int.ofNat (x.toNat &&& y.toNat)) else none
      | _, _ => none
  | .bor a b, env => match evalBwtgt L a env, evalBwtgt L b env with
      | some x, some y =>
          if 0 ≤ x ∧ 0 ≤ y then some (Int.ofNat (x.toNat ||| y.toNat)) else none
      | _, _ => none
  | .bxor a b, env => match evalBwtgt L a env, evalBwtgt L b env with
      | some x, some y =>
          if 0 ≤ x ∧ 0 ≤ y then some (Int.ofNat (x.toNat ^^^ y.toNat)) else none
      | _, _ => none
  | .fdiv a b, env => match evalBwtgt L a env, evalBwtgt L b env with
      | some x, some y => if y = 0 then none else some (L.fdiv x y)
      | _, _ => none

/-- The compiled bitwise semantics: the independent target under the SHIPPED
    lowering. -/
abbrev evalBwjs : BwExp → Env → Option Int := evalBwtgt jsELowering

/-- Bitwise preservation as a predicate OVER the lowering — the SAME
    predicate is proved for the shipped lowering (`preservationBw_real`) and
    REFUTED for the truncating stub (`preservationBw_stub_fails`). -/
def BwPreserves (L : IntDivLowering) : Prop :=
  ∀ e env, evalBwtgt L e env = evalBw false e env

-- the COMPILED targets, same CPython values (arbitrary precision end-to-end):
#guard evalBwjs (.shl (.lit 1) 40) [] = some 1099511627776
#guard evalBwjs (.shr (.shl (.lit 1) 62) 30) [] = some 4294967296
#guard evalBwjs (.bor (.shl (.lit 1) 40) (.lit 1)) [] = some 1099511627777
#guard evalBwjs (.shr (.lit (-1)) 3) [] = some (-1)
#guard evalBwjs (.shr (.lit (-7)) 1) [] = some (-4)
#guard evalBwjs (.shl (.fdiv (.lit (-7)) (.lit 2)) 1) [] = some (-8)
#guard evalBwjs (.shl (.var "x") 8) [("x", 5)] = some 1280
#guard (evalBwjs (.fdiv (.lit 1) (.lit 0)) []).isNone
-- floor-vs-trunc contrast on the INDEPENDENT targets, BOTH deviation arms
-- (each input chosen so floor and truncation actually differ):
#guard evalBwtgt truncELowering (.shr (.lit (-7)) 1) [] = some (-3)               -- stub `>>` ✗ (CPython: -4)
#guard evalBwtgt truncELowering (.shl (.fdiv (.lit (-7)) (.lit 2)) 1) [] = some (-6) -- stub `//` ✗ (CPython: -8)

/-- `2^n ≠ 0` — the `>>` arm's divisor is always legal, so `jsFdiv_eq_fdiv`
    applies unconditionally there (no `y = 0` guard in the shift arm). -/
private theorem bw_two_pow_ne_zero (n : Nat) : ((2 : Int) ^ n) ≠ 0 := by
  have h : (0 : Int) < 2 ^ n := by
    induction n with
    | zero => decide
    | succ k ih =>
      have hs : (2 : Int) ^ (k + 1) = 2 ^ k * 2 := by rw [Int.pow_succ]
      omega
  omega

/-- **Bitwise preservation (Tier-3 wave 13, re-architected — C1 rollout wave
    10).** The INDEPENDENT compiled target under the shipped lowering
    computes the Python reference on every bitwise-fragment expression and
    environment: shifts and non-negative AND/OR/XOR are arbitrary-precision
    on both sides (the emitted BigInt helpers absorb the JS 32-bit coercion —
    proved a REAL deviation by `js32_shl_strict`/`js32_shl_deviation` below),
    and the floor-division deviation — in BOTH the `//` arm and the `>>` arm
    — is absorbed by the emitted floor correction (`jsFdiv_eq_fdiv`). Real
    structural induction, not `rfl`: the two deviation arms need the
    arithmetic binding lemma. -/
theorem preservationBw (e : BwExp) (env : Env) :
    evalBwjs e env = evalBw false e env := by
  induction e with
  | lit n => rfl
  | var s => rfl
  | shl a n ih => simp only [evalBwtgt, evalBw, ih]
  | shr a n ih =>
      simp only [evalBwtgt, evalBw, ih]
      cases evalBw false a env with
      | none => rfl
      | some x =>
          simp [jsELowering, jsFdiv_eq_fdiv x (2 ^ n) (bw_two_pow_ne_zero n)]
  | band a b iha ihb => simp only [evalBwtgt, evalBw, iha, ihb]
  | bor a b iha ihb => simp only [evalBwtgt, evalBw, iha, ihb]
  | bxor a b iha ihb => simp only [evalBwtgt, evalBw, iha, ihb]
  | fdiv a b iha ihb =>
      simp only [evalBwtgt, evalBw, iha, ihb]
      cases evalBw false a env with
      | none => rfl
      | some x =>
        cases evalBw false b env with
        | none => rfl
        | some y =>
          by_cases hy : y = 0
          · simp [hy]
          · simp [hy, jsELowering, jsFdiv_eq_fdiv x y hy]

/-- The re-architected statement in predicate form: the shipped lowering
    preserves. Same content as `preservationBw`; this is the instantiation
    the stub litmus contrasts against. -/
theorem preservationBw_real : BwPreserves jsELowering := preservationBw

/-- **Stub litmus (C1 rollout wave 10).** The SAME preservation predicate is
    FALSE for the naive truncating lowering, and the DISCRIMINATING witness
    is the `>>` arm itself — the arm the old flag statement could never vary:
    on `(-7) >> 1` the stub computes truncation `Int.tdiv (-7) 2 = -3` where
    Python floor-shifts to `Int.fdiv (-7) 2 = -4` (floor and truncation
    genuinely differ on this negative operand). The old
    `evalBw true = evalBw false` statement hardcoded `Int.fdiv` in the `>>`
    arm on BOTH sides, so stubbing the shipping lowering could not break
    it. -/
theorem preservationBw_stub_fails : ¬ BwPreserves truncELowering := by
  intro h
  have hc := h (.shr (.lit (-7)) 1) []
  -- hc reduces to `some (-3) = some (-4)`:
  -- LHS `Int.tdiv (-7) 2 = -3` (truncation), RHS `Int.fdiv (-7) 2 = -4` (floor).
  exact absurd hc (by decide)

/-! ### The faithful naive-JS 32-bit shift model — the deviation witness,
kept OUTSIDE the semantics (the compiled helpers are arbitrary-precision;
this model is what the NATIVE `<<` operator would compute, pinned so the
deviation is proved real rather than asserted — the exact analogue of wave
11's `utf16Len`). -/

/-- ECMAScript `ToInt32`: wrap into the signed 32-bit range `[-2^31, 2^31)`
    — the balanced residue mod `2^32` — applied by native JS bitwise ops to
    BOTH operands before operating and to the result after. -/
def toInt32 (x : Int) : Int := Int.bmod x (2 ^ 32)

/-- The NAIVE JS left shift `a << n`: `ToInt32(a) <<< (n & 31)`, read back as
    a signed 32-bit value. This models a translation using the native `<<`
    operator — which the compiler deliberately does NOT emit. -/
def js32Shl (a : Int) (n : Nat) : Int := toInt32 (toInt32 a * 2 ^ (n % 32))

-- ToInt32 wrap sanity: identity inside the range; 2^31 sign-wraps to -2^31.
#guard toInt32 256 = 256
#guard toInt32 (2 ^ 31) = -(2 ^ 31)
-- THE COUNT-MASKING WITNESS: native JS 1 << 40 is 256 (40 & 31 = 8), not 2^40.
#guard js32Shl 1 40 = 256
#guard js32Shl 1 40 ≠ 1 * 2 ^ 40
-- THE SIGN-WRAP WITNESS: native JS 1 << 31 is -2147483648; Python says 2^31.
#guard js32Shl 1 31 = -(2 ^ 31)
-- inside 32 bits with a small count the native op happens to agree (5 << 2):
#guard js32Shl 5 2 = 20

/-- `ToInt32` always lands in the signed 32-bit range (the balanced-residue
    bounds, direct from `Int.bmod_def`). -/
theorem toInt32_bounds (x : Int) : -(2 ^ 31) ≤ toInt32 x ∧ toInt32 x < 2 ^ 31 := by
  simp only [toInt32, Int.bmod_def]
  omega

/-- Every naive-JS shift result lies in `[-2^31, 2^31)` — regardless of the
    operand or the count. -/
theorem js32Shl_bounded (a : Int) (n : Nat) :
    -(2 ^ 31) ≤ js32Shl a n ∧ js32Shl a n < 2 ^ 31 :=
  toInt32_bounds _

/-- **The deviation is real for EVERY shift past 32 bits** (not just the
    pinned example): whenever Python's `a << n` (= `a * 2^n`) is `≥ 2^31`,
    the native JS shift CANNOT equal it — so naive JS bitwise cannot
    implement Python's, and the compiler's BigInt helpers are necessary, not
    stylistic. The bitwise analogue of `utf16_astral_strict`. -/
theorem js32_shl_strict (a : Int) (n : Nat) (h : 2 ^ 31 ≤ a * 2 ^ n) :
    js32Shl a n ≠ a * 2 ^ n := by
  intro heq
  have hb : js32Shl a n < 2 ^ 31 := (js32Shl_bounded a n).2
  rw [heq] at hb
  exact Int.not_le.mpr hb h

/-- The mirrored impossibility below the range: a Python shift `< -2^31` is
    equally unreachable by the native operator. -/
theorem js32_shl_strict_neg (a : Int) (n : Nat) (h : a * 2 ^ n < -(2 ^ 31)) :
    js32Shl a n ≠ a * 2 ^ n := by
  intro heq
  have hb : -(2 ^ 31) ≤ js32Shl a n := (js32Shl_bounded a n).1
  rw [heq] at hb
  exact Int.not_le.mpr h hb

/-- The concrete count-masking witness as a theorem: native JS `1 << 40`
    (= 256, count masked to 8) is NOT Python's `1 << 40` (= 2^40). -/
theorem js32_shl_deviation : js32Shl 1 40 ≠ (1 : Int) * 2 ^ 40 := by decide

/-- The concrete sign-wrap witness: native JS `1 << 31` (= -2^31) is NOT
    Python's `1 << 31` (= 2^31) — deviation even with an unmasked count. -/
theorem js32_shl_sign_wrap : js32Shl 1 31 ≠ (1 : Int) * 2 ^ 31 := by decide

-- both pinned witnesses are instances of the general impossibility:
example : js32Shl 1 40 ≠ (1 : Int) * 2 ^ 40 := js32_shl_strict 1 40 (by decide)
example : js32Shl 1 31 ≠ (1 : Int) * 2 ^ 31 := js32_shl_strict 1 31 (by decide)

-- SPOT: the compiled `1 << 40` — the INDEPENDENT target under the shipped
-- lowering — routed THROUGH `preservationBw` to the Python reference and
-- evaluated there: 2^40, all bits kept. The naive-JS model provably gives
-- 256 for the same source (`js32_shl_deviation`), so this closes only
-- because the compiled semantics is arbitrary-precision.
example : evalBwjs (.shl (.lit 1) 40) [] = some 1099511627776 := by
  rw [preservationBw]
  decide

-- SPOT 2: the `//` deviation FEEDING a shift — `(-7 // 2) << 1` in the
-- INDEPENDENT compiled semantics, through the theorem to the Python answer:
-- floor gives -4 << 1 = -8; JS-trunc -3 would give -6, so a weakened (e.g.
-- both-sides-`none`) statement leaves the `some` goal unprovable. The -6
-- truncating value is pinned by the separate `//`-feeding-shift contrast
-- `#guard` above; `preservationBw_stub_fails` itself witnesses the `>>` arm
-- (trunc -3 vs floor -4).
example : evalBwjs (.shl (.fdiv (.lit (-7)) (.lit 2)) 1) [] = some (-8) := by
  rw [preservationBw]
  decide

-- SPOT 3: a 41-bit OR built from a shift — `(1 << 40) | 1` compiled, through
-- the theorem: 1099511627777 (native JS OR would truncate to 32 bits first).
example : evalBwjs (.bor (.shl (.lit 1) 40) (.lit 1)) [] = some 1099511627777 := by
  rw [preservationBw]
  decide

-- SPOT 4: the `>>` deviation arm itself — compiled `(-7) >> 1` through the
-- theorem to Python's floor `-4` (the truncating stub computes -3, so this
-- SPOT is unprovable for a statement that lets the shift arm truncate).
example : evalBwjs (.shr (.lit (-7)) 1) [] = some (-4) := by
  rw [preservationBw]
  decide

/-- info: 'PythExpandVerify.preservationBw' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationBw

/-- info: 'PythExpandVerify.preservationBw_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationBw_real

/-- info: 'PythExpandVerify.preservationBw_stub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationBw_stub_fails

/-- info: 'PythExpandVerify.js32_shl_strict' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms js32_shl_strict

/-- info: 'PythExpandVerify.js32_shl_deviation' does not depend on any axioms -/
#guard_msgs in
#print axioms js32_shl_deviation


/-! ## B1 — boundedness: float exactness on the safe-integer domain

The ε-tier documents (tested, not proved) that PythScribe's float arithmetic
matches CPython. This section PROVES the bounded slice of that claim that is
provable without Lean's opaque `Float`: IEEE-754 binary64 represents every
integer in `[-2^53, 2^53]` exactly, and `+`/`-`/`*` of two such integers,
when the exact result is also in that range, incurs NO rounding — the double
result IS the exact integer. This is why `2.0 + 3.0 == 5` exactly while
`0.1 + 0.2` is not `0.3`, and it is a genuinely BOUNDED claim: the
divergence `#guard`s below witness that one step past the bound
(`2^53 + 1`) rounding really happens, so the `SafeInt` hypotheses are
load-bearing, not trivializers.

The doubles are modeled ARITHMETICALLY, with no dependency on core `Float`:
a double that carries an integer value is represented by that integer, and
the IEEE operation is modeled as compute-exact-then-round
(`floatAdd a b := roundToDouble (a + b)`), where `roundToDouble` is
round-to-nearest, ties-to-even onto the integer-double grid: spacing `1` at
or below `2^53`, spacing `2^(⌊log2 |n|⌋ - 52)` above (the binade ulp). On
this grid ties-to-even-MULTIPLE coincides with IEEE's ties-to-even-
SIGNIFICAND (within a binade the multiple index IS the significand's parity;
at a binade top the upper candidate `2^(k+1)` has even significand `2^52`
AND even multiple index `2^52`·2, so the two rules agree there too).

FRAME (honest — what is modeled vs not):
- INTEGER SLICE ONLY. Operands and results are integer-valued doubles, so
  the model lives on `Int` and never needs `Rat`: for `+`/`-`/`*` of
  integers the exact result is an integer, so the slice is closed under the
  modeled ops. Fractional dyadics (`0.5`, `0.1`…) are OUT of this frame —
  they stay in the tested-not-proved ε-tier.
- NO overflow cap. Real binary64 overflows to `∞` above `~2^1024`; the
  model's grid extends to all integers. Irrelevant on and near the safe
  domain (everything here is `< 2^55`), but stated: `roundToDouble` of an
  astronomically large integer returns a grid point, not `∞`.
- DIVISION is not covered: an integer quotient is not an integer in
  general, so `/` leaves the slice. (Integer `//`/`%` are the i64/JS twins
  proved above — different obligation.)
- `ha`/`hb` are semantically load-bearing even where a proof does not
  consume them: they are what makes "the double input EQUALS `a`" true
  (`safeInt_representable`), i.e. the theorem is read as a statement about
  doubles, not about a formal rounding of unrepresentable inputs.
- CPython crosscheck (differential, outside Lean): `float(2**53+1) ==
  2**53`, `float(2**53+3) == 2**53+4`, `67108865.0*134217729.0` = exact−1,
  `float(-(2**53+1)) == -(2**53)` — each pinned by a `#guard` on the model
  below, same values. -/

/-- The safe-integer domain of IEEE-754 binary64: every integer in
    `[-2^53, 2^53]` is exactly representable (JS `Number.MIN_SAFE_INTEGER -
    1 … MAX_SAFE_INTEGER + 1`; the closed interval is correct — `±2^53`
    itself is `±2^52·2`, exactly representable). -/
def SafeInt (n : Int) : Prop := -(2 ^ 53) ≤ n ∧ n ≤ 2 ^ 53

instance (n : Int) : Decidable (SafeInt n) :=
  inferInstanceAs (Decidable (-(2 ^ 53) ≤ n ∧ n ≤ 2 ^ 53))

/-- An integer exactly representable as a binary64 double: a significand of
    magnitude `< 2^53` times a power of two (integers need no negative
    exponents, so `e : Nat` loses nothing on this slice; the overflow cap
    `e ≤ 971` is deliberately out of frame — see the section header). -/
def representable (n : Int) : Prop :=
  ∃ (m : Int) (e : Nat), n = m * 2 ^ e ∧ m.natAbs < 2 ^ 53

/-- Round `n` to the nearest multiple of `u` (`u > 0`), ties to the EVEN
    multiple — round-to-nearest-even onto a grid of spacing `u`. Int `%`/`/`
    are `emod`/`ediv` (checked by `rfl` during development), so `n % u ∈
    [0, u)` and `n - n % u` is the grid point at or below `n`. -/
def rneMul (u n : Int) : Int :=
  if 2 * (n % u) < u then n - n % u
  else if u < 2 * (n % u) then n - n % u + u
  else if (n / u) % 2 = 0 then n - n % u
  else n - n % u + u

/-- Grid-spacing exponent (ulp) of the integer-double grid at magnitude
    `|n|`: `0` (spacing 1) at or below `2^53`, else `⌊log2 |n|⌋ - 52` — the
    binade ulp of binary64. (At an exact power `2^k`, `k ≥ 54`, this picks
    the UPPER binade's spacing; harmless, since `2^k` is a multiple of both
    and rounds to itself either way.) -/
def ulpExp (n : Int) : Nat :=
  if n.natAbs ≤ 2 ^ 53 then 0 else Nat.log2 n.natAbs - 52

/-- Round-to-nearest-even of an exact integer value onto the integer-double
    grid — the model of IEEE binary64 rounding, restricted to the integer
    slice. This is a REAL rounding function: the `#guard`s below witness it
    moving `2^53 + 1 ↦ 2^53` (tie to even) and `2^53 + 3 ↦ 2^53 + 4`. -/
def roundToDouble (n : Int) : Int := rneMul (2 ^ ulpExp n) n

/-- IEEE `+` on integer-valued doubles: compute exact, round once. -/
def floatAdd (a b : Int) : Int := roundToDouble (a + b)

/-- IEEE `-` on integer-valued doubles: compute exact, round once. -/
def floatSub (a b : Int) : Int := roundToDouble (a - b)

/-- IEEE `*` on integer-valued doubles: compute exact, round once. -/
def floatMul (a b : Int) : Int := roundToDouble (a * b)

/-- Representable-identity, semantic half: every safe integer IS an
    integer-valued double — magnitude `< 2^53` is its own significand;
    `±2^53` is `±2^52 · 2`. -/
theorem safeInt_representable {n : Int} (h : SafeInt n) : representable n := by
  by_cases hlt : n.natAbs < 2 ^ 53
  · exact ⟨n, 0, by rw [show (2 : Int) ^ (0 : Nat) = 1 by decide]; omega, hlt⟩
  · have h53 : n = 2 ^ 53 ∨ n = -(2 ^ 53) := by
      rcases h with ⟨h1, h2⟩
      omega
    rcases h53 with rfl | rfl
    · exact ⟨2 ^ 52, 1, by decide, by decide⟩
    · exact ⟨-(2 ^ 52), 1, by decide, by decide⟩

/-- On a grid of spacing 1 every integer is a grid point, so rounding is the
    identity (`n % 1 = 0` puts every input in the round-down-by-zero
    branch). -/
theorem rneMul_one (n : Int) : rneMul 1 n = n := by
  have h : n % 1 = 0 := by omega
  simp only [rneMul, h]
  split <;> omega

/-- **Representable-identity (the crux).** A safe integer is exactly
    representable (`safeInt_representable`), so IEEE rounding returns it
    unchanged: inside `[-2^53, 2^53]` the grid spacing is 1 and
    `roundToDouble` moves nothing. Outside, it genuinely rounds — the
    boundary `#guard`s below are the witnesses. -/
theorem roundToDouble_safeInt_id {n : Int} (h : SafeInt n) :
    roundToDouble n = n := by
  have habs : n.natAbs ≤ 2 ^ 53 := by
    rcases h with ⟨h1, h2⟩
    omega
  unfold roundToDouble ulpExp
  rw [if_pos habs]
  rw [show (2 : Int) ^ (0 : Nat) = 1 by decide]
  exact rneMul_one n

set_option linter.unusedVariables false in
/-- **B1 float exactness, `+`.** For doubles carrying safe integers `a`, `b`
    whose exact sum is also safe, IEEE `+` (exact-then-round) incurs no
    rounding: the result is exactly `a + b` — PythScribe's float `+` on
    integer-valued operands matches Python EXACTLY on this domain. `hs` is
    genuine: at `(2^53, 1)` (where `hs` alone fails) the modeled `+` really
    rounds, `floatAdd (2^53) 1 = 2^53 ≠ 2^53 + 1` (`#guard`ed below, and
    true of real doubles: `2.0**53 + 1.0 == 2.0**53`). -/
theorem floatAdd_exact_safeInt (a b : Int) (ha : SafeInt a) (hb : SafeInt b)
    (hs : SafeInt (a + b)) : floatAdd a b = a + b :=
  roundToDouble_safeInt_id hs

set_option linter.unusedVariables false in
/-- **B1 float exactness, `-`.** Same statement for subtraction. -/
theorem floatSub_exact_safeInt (a b : Int) (ha : SafeInt a) (hb : SafeInt b)
    (hs : SafeInt (a - b)) : floatSub a b = a - b :=
  roundToDouble_safeInt_id hs

set_option linter.unusedVariables false in
/-- **B1 float exactness, `*`.** Same statement for multiplication — `hs` is
    the load-bearing bound: `67108865 * 134217729` has both factors safe but
    an unsafe (odd, `> 2^53`) product, and the modeled `*` loses exactly the
    last unit (`#guard`ed below; real doubles agree). -/
theorem floatMul_exact_safeInt (a b : Int) (ha : SafeInt a) (hb : SafeInt b)
    (hs : SafeInt (a * b)) : floatMul a b = a * b :=
  roundToDouble_safeInt_id hs

-- In-domain exactness (the discipline pins):
#guard floatAdd 2 3 = 5
#guard floatSub 5 9 = -4
#guard floatMul 1000000 1000000 = 10 ^ 12
#guard roundToDouble (2 ^ 53) = 2 ^ 53            -- the bound itself is safe
#guard roundToDouble (-(2 ^ 53)) = -(2 ^ 53)

-- BOUNDARY witnesses — one step outside the domain, rounding REALLY occurs,
-- so the `SafeInt` hypotheses are necessary (each value crosschecked against
-- CPython floats, same results):
#guard roundToDouble (2 ^ 53 + 1) ≠ 2 ^ 53 + 1    -- 2^53 + 1 NOT representable
#guard roundToDouble (2 ^ 53 + 1) = 2 ^ 53        -- tie → even significand (down)
#guard floatAdd (2 ^ 53) 1 = 2 ^ 53               -- the op itself rounds:
#guard floatAdd (2 ^ 53) 1 ≠ 2 ^ 53 + 1           --   2.0^53 + 1.0 == 2.0^53
#guard roundToDouble (-(2 ^ 53 + 1)) = -(2 ^ 53)  -- symmetric at the negative end
#guard roundToDouble (2 ^ 53 + 2) = 2 ^ 53 + 2    -- even neighbour IS representable
#guard roundToDouble (2 ^ 53 + 3) = 2 ^ 53 + 4    -- tie → even significand (up)
#guard floatMul 67108865 134217729                 -- (2^26+1)·(2^27+1): safe factors,
    ≠ 67108865 * 134217729                         --   unsafe product → inexact
#guard floatMul 67108865 134217729 = 67108865 * 134217729 - 1

-- Non-vacuity of the frame: `SafeInt` is decidable and the input space is
-- concretely inhabited (and concretely bounded).
#guard decide (SafeInt 0) && decide (SafeInt (2 ^ 53)) && decide (SafeInt (-(2 ^ 53)))
#guard !decide (SafeInt (2 ^ 53 + 1))

/-- SPOT: a concrete safe-domain product routed THROUGH
    `floatMul_exact_safeInt` (not a `#guard` of the model): the proof
    rewrites by the theorem first, so it closes only because the theorem's
    RHS is the EXACT integer product — a weakened statement (say, RHS
    `roundToDouble (a * b)`) leaves nothing proved about exactness. -/
example : floatMul (10 ^ 7) (10 ^ 7) = 10 ^ 14 := by
  rw [floatMul_exact_safeInt (10 ^ 7) (10 ^ 7) (by decide) (by decide) (by decide)]
  decide

/-- SPOT: the inclusive upper bound — `2^52 + 2^52` lands EXACTLY ON `2^53`,
    and `hs : SafeInt (2^53)` is satisfied (closed interval), so the sum is
    exact through the theorem. One more unit and it would not be (`floatAdd
    (2^53) 1` above). -/
example : floatAdd (2 ^ 52) (2 ^ 52) = 2 ^ 53 := by
  rw [floatAdd_exact_safeInt (2 ^ 52) (2 ^ 52) (by decide) (by decide) (by decide)]
  decide

/-- info: 'PythExpandVerify.safeInt_representable' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms safeInt_representable

/-- info: 'PythExpandVerify.roundToDouble_safeInt_id' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms roundToDouble_safeInt_id

/-- info: 'PythExpandVerify.floatAdd_exact_safeInt' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms floatAdd_exact_safeInt

/-- info: 'PythExpandVerify.floatSub_exact_safeInt' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms floatSub_exact_safeInt

/-- info: 'PythExpandVerify.floatMul_exact_safeInt' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms floatMul_exact_safeInt


/-! ## Tier-3 wave 14 — integer `**` (power): exact bigint vs the JS Number double

Python `int ** int` with a non-negative exponent is EXACT arbitrary-precision:
`3 ** 35 == 50031545098999707`, every bit kept. A naive JS translation via
`Number` (`Math.pow` / the native `**` operator) produces an IEEE-754 binary64
double, which represents an integer exactly only as `m * 2^k` with
`|m| < 2^53` — past `2^53` only suitably-even integers survive, so an ODD
value exceeding `2^53` is UNREPRESENTABLE: the nearest double to `3 ** 35` is
`50031545098999704` (a multiple of 8, off by 3), and NO double equals the
true value — whatever `Math.pow` rounds to, it cannot be right. The compiler
therefore emits BigInt exponentiation for integer `**` (exact), so the
COMPILED output matches CPython.

Shape (re-architected in the C1 rollout, wave 11 — the wave-1 recipe):
(1) `preservationPow` — the INDEPENDENT compiled target `evalPowtgt
(L : IntDivLowering)` under the SHIPPED lowering equals the Python reference
`evalPow false` for EVERY expression and environment; the `pow` node is
exact arbitrary-precision integer power on BOTH sides (the emitted BigInt
power, never `Math.pow` — faithfully so, NOT shared-wrong: the reference is
pinned to CPython below and `pow_not_double` proves the naive `Number`
alternative CANNOT hold the large-odd values), and the `fdiv` node carries
the floor-division deviation axis routed through `L.fdiv`, closed by
`jsFdiv_eq_fdiv`. `PowPreserves L` is the predicate form, proved for
`jsELowering` (`preservationPow_real`) and REFUTED for the truncating stub
(`preservationPow_stub_fails`, discriminating `((-2)**3)//3` witness: floor
`-3` vs truncation `-2`). The old `evalPow true = evalPow false` statement
was the F1 model-vs-model flag; its `true` branch is retained below as
documented LEGACY only, with NO theorem referencing it. (2) A model of the
integers an IEEE double can hold (`IsDouble`, kept OUTSIDE the semantics —
exactly as wave 13 kept `js32Shl` outside) plus impossibility results:
`isDouble_gt_2pow53_even`
(a double-representable integer beyond `2^53` must be even), corollary
`odd_large_not_double`, the general `pow_not_double` (ANY power that is odd
and exceeds `2^53` equals no double — the power analogue of
`js32_shl_strict` / `utf16_astral_strict`), and the concrete pins
`pow_3_35_not_double` / `two_pow53_succ_not_double`. Agreement sanity:
`IsDouble 1024` and `IsDouble (2^53)` — the deviation is specifically the
large-odd regime, not spurious.

`IsDouble` deliberately OVER-approximates the set of integer-valued finite
doubles (any `m * 2^k` with `|m| < 2^53`, unbounded `k` — no exponent cap,
no NaN/Inf bookkeeping): every integer a binary64 holds exactly is of this
form, so `¬ IsDouble x` is STRONGER than "no actual double equals x".

OUT of scope (documented, deliberate): NEGATIVE exponents (CPython
`int ** -n` returns a FLOAT — a type change, outside this Int fragment);
float base or exponent; 3-arg modular `pow(b, e, m)`; the exponent as a full
expression — it is a `Nat` constructor argument, exactly as wave 13 treats
shift counts (a literal exponent already carries the whole precision
deviation). Negative BASES are IN scope (`(-2) ** 3 == -8`, exact on both
sides). -/

inductive PowExp where
  | lit (v : Int)
  | var (name : String)
  | pow (base : PowExp) (n : Nat)  -- Python `base ** n`, Nat literal exponent (exact bigint)
  | fdiv (a b : PowExp)            -- Python `a // b` — the seed's deviation axis
deriving Repr

/-- Power-fragment eval — `tgt = false` is the Python REFERENCE semantics
    (the only branch any theorem uses; pinned to CPython below). The
    `tgt = true` branch is LEGACY (the former F1 model-vs-model flag, which
    deviated only in the `//` arm) and is NOT the compiled target — the
    genuine compiled target is the INDEPENDENT `evalPowtgt
    (L : IntDivLowering)` below; NO theorem references `evalPow true`. The
    `pow` node is EXACT integer exponentiation because the compiler emits
    BigInt `**`, never `Math.pow` — see `IsDouble` below for what a `Number`
    power could even represent. -/
def evalPow (tgt : Bool) : PowExp → Env → Option Int
  | .lit v, _ => some v
  | .var s, env => env.get s
  | .pow b n, env => (evalPow tgt b env).map (fun x => x ^ n)
  | .fdiv a b, env => match evalPow tgt a env, evalPow tgt b env with
      | some x, some y =>
          if y = 0 then none else some (if tgt then jsFdiv x y else Int.fdiv x y)
      | _, _ => none

-- F9 pins: the REFERENCE `evalPow false` matches CPython on the power
-- fragment INCLUDING the deviation axis (legacy `evalPow true` guards
-- retired — the compiled side is now pinned on `evalPowjs` below).
-- 3 ** 35 = 50031545098999707 (the headline value — exact bigint; CPython).
#guard evalPow false (.pow (.lit 3) 35) [] = some 50031545098999707
-- 2 ** 53 = 9007199254740992 (CPython; the double boundary — itself representable).
#guard evalPow false (.pow (.lit 2) 53) [] = some 9007199254740992
-- 2 ** 53 + 1 = 9007199254740993 (CPython; the classic odd integer NO double holds).
#guard (2 : Int) ^ 53 + 1 = 9007199254740993
-- 2 ** 10 = 1024 (CPython; small power — the agreement regime).
#guard evalPow false (.pow (.lit 2) 10) [] = some 1024
-- 10 ** 16 = 10000000000000000 (CPython; even and > 2^53 — happens to be a double).
#guard evalPow false (.pow (.lit 10) 16) [] = some 10000000000000000
-- a variable base through the env: x ** 5 with x = 2 is 32 (CPython).
#guard evalPow false (.pow (.var "x") 5) [("x", 2)] = some 32
-- (-2) ** 3 = -8 (CPython; negative BASE is exact and in scope).
#guard evalPow false (.pow (.lit (-2)) 3) [] = some (-8)
-- 0 ** 0 = 1 (CPython).
#guard evalPow false (.pow (.lit 0) 0) [] = some 1
-- the deviation axis THREADED THROUGH a power sub-expression (CPython:
-- ((-2) ** 3) // 3 == (-8) // 3 == -3, FLOOR; truncation would give -2 —
-- floor and truncation genuinely differ here, the discriminating input).
#guard evalPow false (.fdiv (.pow (.lit (-2)) 3) (.lit 3)) [] = some (-3)
-- division by zero → none (ZeroDivisionError; CPython).
#guard (evalPow false (.fdiv (.lit 1) (.lit 0)) []).isNone

/-- **Independent target evaluator** for the power fragment: the compiled
    program's semantics under integer-division lowering `L` — a SEPARATE
    recursion, not a `Bool` flag on `evalPow`. The floor-division deviation
    axis (`//`) routes through `L.fdiv`, so the lowering is what varies. The
    `pow` node is exact arbitrary-precision integer power IDENTICAL to the
    reference — faithfully so, NOT shared-wrong: the reference `pow` arm is
    pinned to CPython above, and `pow_not_double` below proves the naive
    `Number` alternative CANNOT hold any large-odd exact power, so "exact
    bigint on both sides" is the verified content of that arm (the emitted
    BigInt power helper), not an artifact of copying. -/
def evalPowtgt (L : IntDivLowering) : PowExp → Env → Option Int
  | .lit v, _ => some v
  | .var s, env => env.get s
  | .pow b n, env => (evalPowtgt L b env).map (fun x => x ^ n)
  | .fdiv a b, env => match evalPowtgt L a env, evalPowtgt L b env with
      | some x, some y => if y = 0 then none else some (L.fdiv x y)
      | _, _ => none

/-- The compiled power semantics: the independent target under the SHIPPED
    lowering. -/
abbrev evalPowjs : PowExp → Env → Option Int := evalPowtgt jsELowering

/-- Power preservation as a predicate OVER the lowering — the SAME predicate
    is proved for the shipped lowering (`preservationPow_real`) and REFUTED
    for the truncating stub (`preservationPow_stub_fails`). -/
def PowPreserves (L : IntDivLowering) : Prop :=
  ∀ e env, evalPowtgt L e env = evalPow false e env

-- the COMPILED targets, same CPython values (exact bigint end-to-end):
#guard evalPowjs (.pow (.lit 3) 35) [] = some 50031545098999707
#guard evalPowjs (.pow (.lit 2) 53) [] = some 9007199254740992
#guard evalPowjs (.pow (.lit 2) 10) [] = some 1024
#guard evalPowjs (.pow (.var "x") 5) [("x", 2)] = some 32
#guard evalPowjs (.pow (.lit (-2)) 3) [] = some (-8)
#guard evalPowjs (.pow (.lit 0) 0) [] = some 1
#guard evalPowjs (.fdiv (.pow (.lit (-2)) 3) (.lit 3)) [] = some (-3)
#guard (evalPowjs (.fdiv (.lit 1) (.lit 0)) []).isNone
-- floor-vs-trunc contrast on the INDEPENDENT targets (the input chosen so
-- floor and truncation actually differ: (-8) // 3 is floor -3, trunc -2):
#guard evalPowtgt truncELowering (.fdiv (.pow (.lit (-2)) 3) (.lit 3)) [] = some (-2)  -- stub ✗ (CPython: -3)

/-- **Power preservation (Tier-3 wave 14, re-architected — C1 rollout wave
    11).** The INDEPENDENT compiled target under the shipped lowering
    computes the Python reference on every power-fragment expression and
    environment: `**` is exact bigint on both sides (the emitted BigInt
    power absorbs the Number-precision deviation — proved a REAL deviation
    by `pow_not_double` / `pow_3_35_not_double` below), and the `//`
    deviation is absorbed by the emitted floor correction
    (`jsFdiv_eq_fdiv`). Real structural induction, not `rfl`: the `fdiv`
    deviation arm needs the arithmetic binding lemma. -/
theorem preservationPow (e : PowExp) (env : Env) :
    evalPowjs e env = evalPow false e env := by
  induction e with
  | lit v => rfl
  | var s => rfl
  | pow b n ih => simp only [evalPowtgt, evalPow, ih]
  | fdiv a b iha ihb =>
      simp only [evalPowtgt, evalPow, iha, ihb]
      cases evalPow false a env with
      | none => rfl
      | some x =>
        cases evalPow false b env with
        | none => rfl
        | some y =>
          by_cases hy : y = 0
          · simp [hy]
          · simp [hy, jsELowering, jsFdiv_eq_fdiv x y hy]

/-- The re-architected statement in predicate form: the shipped lowering
    preserves. Same content as `preservationPow`; this is the instantiation
    the stub litmus contrasts against. -/
theorem preservationPow_real : PowPreserves jsELowering := preservationPow

/-- **Stub litmus (C1 rollout wave 11).** The SAME preservation predicate is
    FALSE for the naive truncating lowering, and the DISCRIMINATING witness
    threads the deviation through a power sub-expression: on
    `((-2) ** 3) // 3` the power is exact on both sides (`-8`), but the stub
    then computes truncation `Int.tdiv (-8) 3 = -2` where Python floors to
    `Int.fdiv (-8) 3 = -3` — floor and truncation genuinely differ on this
    negative quotient. The old `evalPow true = evalPow false` statement
    could only ever vary the same shared `//` helper choice, so stubbing the
    shipping lowering could not break it. -/
theorem preservationPow_stub_fails : ¬ PowPreserves truncELowering := by
  intro h
  have hc := h (.fdiv (.pow (.lit (-2)) 3) (.lit 3)) []
  -- hc reduces to `some (-2) = some (-3)`:
  -- LHS `Int.tdiv (-8) 3 = -2` (truncation), RHS `Int.fdiv (-8) 3 = -3` (floor).
  exact absurd hc (by decide)

/-! ### The IEEE-double representability model — the deviation witness, kept
OUTSIDE the semantics (the compiled power is exact BigInt; this model is the
set of integers a naive-JS `Number` power could even PRODUCE, pinned so the
deviation is proved real rather than asserted — the exact analogue of wave
13's `js32Shl` and wave 11's `utf16Len`). -/

/-- Local core-only oddness (no Mathlib): `x` is odd iff `x = 2*k + 1`. -/
def Odd (x : Int) : Prop := ∃ k : Int, x = 2 * k + 1

/-- The integers an IEEE-754 binary64 can hold exactly: a significand of
    magnitude `< 2^53` times a power of two. Deliberately over-approximates
    real doubles (no exponent cap), which only STRENGTHENS every
    `¬ IsDouble` result below. -/
def IsDouble (x : Int) : Prop :=
  ∃ (m : Int) (k : Nat), x = m * 2 ^ k ∧ m.natAbs < 2 ^ 53

/-- **The load-bearing parity lemma**: a double-representable integer that
    EXCEEDS `2^53` must be EVEN — with `k = 0` the significand alone is
    bounded by `2^53` (contradiction), so `k ≥ 1` and the value carries a
    factor of 2. -/
theorem isDouble_gt_2pow53_even {x : Int} (hd : IsDouble x) (hgt : 2 ^ 53 < x) :
    2 ∣ x := by
  obtain ⟨m, k, hx, hm⟩ := hd
  cases k with
  | zero =>
    exfalso
    have h1 : (2 : Int) ^ 0 = 1 := rfl
    rw [h1, Int.mul_one] at hx
    have h253i : (2 : Int) ^ 53 = 9007199254740992 := by decide
    have h253n : (2 : Nat) ^ 53 = 9007199254740992 := by decide
    rw [h253i] at hgt
    rw [h253n] at hm
    omega
  | succ j =>
    refine ⟨m * 2 ^ j, ?_⟩
    rw [hx]
    show m * (2 ^ j * 2) = 2 * (m * 2 ^ j)
    rw [← Int.mul_assoc, Int.mul_comm (m * 2 ^ j) 2]

/-- If `x` is odd and exceeds `2^53`, NO IEEE double equals `x`. -/
theorem odd_large_not_double {x : Int} (ho : Odd x) (hgt : 2 ^ 53 < x) :
    ¬ IsDouble x := by
  intro hd
  have hev : 2 ∣ x := isDouble_gt_2pow53_even hd hgt
  obtain ⟨k, hk⟩ := ho
  omega

/-- **The general power impossibility (Tier-3 wave 14).** For ANY base and
    exponent whose exact integer power is odd and exceeds `2^53`, no IEEE
    double — hence no naive-JS `Number` power (`Math.pow` / native `**`),
    whatever it rounds to — equals the Python value. So naive JS cannot
    implement Python integer `**`, and the compiler's BigInt power is
    necessary, not stylistic. The power analogue of `js32_shl_strict` /
    `utf16_astral_strict`. -/
theorem pow_not_double (b : Int) (n : Nat) (ho : Odd (b ^ n)) (hgt : 2 ^ 53 < b ^ n) :
    ¬ IsDouble (b ^ n) :=
  odd_large_not_double ho hgt

/-- The concrete pin: CPython `3 ** 35 == 50031545098999707` is odd and
    exceeds `2^53`, so NO double holds it (the nearest double is
    50031545098999704) — proved via the general `pow_not_double`. -/
theorem pow_3_35_not_double : ¬ IsDouble ((3 : Int) ^ 35) :=
  pow_not_double 3 35 ⟨25015772549499853, by decide⟩ (by decide)

/-- The classic first casualty: `2^53 + 1 = 9007199254740993` is odd and
    exceeds `2^53` — no double holds it (doubles skip from `2^53` straight to
    `2^53 + 2`). -/
theorem two_pow53_succ_not_double : ¬ IsDouble ((2 : Int) ^ 53 + 1) :=
  odd_large_not_double ⟨2 ^ 52, by decide⟩ (by decide)

-- Agreement sanity — the deviation is specifically the LARGE-ODD regime:
-- small powers and the even boundary value ARE representable doubles.
example : IsDouble 1024 := ⟨1, 10, by decide, by decide⟩
example : IsDouble ((2 : Int) ^ 53) := ⟨1, 53, by decide, by decide⟩

-- SPOT: the compiled `3 ** 4` — the INDEPENDENT target under the shipped
-- lowering — routed THROUGH `preservationPow` to the Python reference and
-- evaluated there: 81, exact.
example : evalPowjs (.pow (.lit 3) 4) [] = some 81 := by
  rw [preservationPow]
  decide

-- SPOT 2: a power FEEDING the `//` deviation axis — `(2 ** 5) // 3` in the
-- INDEPENDENT compiled semantics, through the theorem: 32 // 3 = 10 (floor
-- and trunc agree here; the axis is exercised without biting).
example : evalPowjs (.fdiv (.pow (.lit 2) 5) (.lit 3)) [] = some 10 := by
  rw [preservationPow]
  decide

-- SPOT 3: the deviation axis BITING — `((-2) ** 3) // 3` in the INDEPENDENT
-- compiled semantics, through the theorem to the Python answer:
-- (-8) // 3 = -3 (floor); the truncating stub computes -2 (pinned by the
-- contrast `#guard` above and refuted by `preservationPow_stub_fails`), so
-- a weakened (e.g. both-sides-`none`) statement leaves the `some` goal
-- unprovable.
example : evalPowjs (.fdiv (.pow (.lit (-2)) 3) (.lit 3)) [] = some (-3) := by
  rw [preservationPow]
  decide

-- SPOT 4: the headline large power on the COMPILED target — `3 ** 35`,
-- through the theorem: 50031545098999707 exactly. The double model provably
-- cannot hold this value (`pow_3_35_not_double`), so this closes only
-- because the compiled semantics is exact BigInt.
example : evalPowjs (.pow (.lit 3) 35) [] = some 50031545098999707 := by
  rw [preservationPow]
  decide

/-- info: 'PythExpandVerify.preservationPow' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationPow

/-- info: 'PythExpandVerify.preservationPow_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationPow_real

/-- info: 'PythExpandVerify.preservationPow_stub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationPow_stub_fails

/-- info: 'PythExpandVerify.pow_not_double' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms pow_not_double

/-- info: 'PythExpandVerify.pow_3_35_not_double' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms pow_3_35_not_double


/-! ## Tier-3 wave 15 — `round()` banker's rounding (vs the JS `Math.round` half-up deviation)

Python `round()` (no `ndigits`) uses BANKER'S ROUNDING — round-half-to-even:
`round(0.5) == 0`, `round(1.5) == 2`, `round(2.5) == 2`, `round(3.5) == 4`,
`round(-0.5) == 0`, `round(-1.5) == -2`, `round(-2.5) == -2`. Naive JavaScript
`Math.round` rounds HALF UP toward +∞: `Math.round(0.5) == 1`,
`Math.round(2.5) == 3`, `Math.round(-1.5) == -1` — all WRONG as Python. The
compiler therefore emits a banker's-rounding helper for `round()`, never the
native `Math.round`, so the COMPILED output matches CPython on every tie.

REPRESENTATION (exact, no `Float`): a value is `a / 2` with carrier `a : Int`
— integers are even carriers (`a = 2*m` ↦ value `m`), exact half-integer TIES
are odd carriers (`a = 2*m + 1` ↦ value `m + ½`). This grid captures every
integer and every tie — i.e. the ENTIRE deviation surface: on non-tie inputs
half-up and half-to-even agree by definition, so nothing is lost by staying on
the half grid, and no float-representation noise enters the model. The two
rounding functions on carriers:
  * `jsRound a = ⌊(a/2) + ½⌋ = Int.fdiv (a + 1) 2` — `Math.round`, half up
    toward +∞ (also on negatives: `Math.round(-0.5) = -0`, carrier `-1` ↦ 0);
  * `pyRound a` — banker's: even carrier `2*m` ↦ `m`; odd carrier `2*m + 1`
    ↦ the EVEN member of `{m, m + 1}`.

Convention for `RoundExp` (carrier-throughout, so EVERY node denotes the value
`carrier / 2`): `lit`/`var` are carriers a client writes; `round` consumes a
carrier and re-encodes its integer result as an EVEN carrier `2 * n`
(Python `round(x)` returns an `int`, denoted by its even carrier); `fdiv` — the
seed's shared deviation axis — is now FAITHFUL on the half grid (C5): it consumes
two carriers `ca`, `cb` denoting values `ca/2`, `cb/2`, and Python's float
floor-division of those values is `⌊(ca/2)/(cb/2)⌋ = ⌊ca/cb⌋ = Int.fdiv ca cb`,
re-encoded as the even carrier `2 * Int.fdiv ca cb`. So `2.5 // 1.5 = 1.0`
(carriers `5 // 3 → 2`) and `2.5 // 0.5 = 5.0` (carriers `5 // 1 → 10`), and the
divisor is zero exactly when its VALUE is zero (`cb = 0`) — the old model wrongly
floored each operand to `ca/2`, `cb/2` first (giving `2.5//1.5 = 2//1 = 2`) and
wrongly trapped `2.5//0.5` (carrier `cb = 1`) as division by zero. Nesting and
`round`-fed `//` are all faithful; nothing is excluded.

Shape (the wave-13 pattern): (1) `preservationRound` — compiled (tgt=true)
equals Python (tgt=false) for EVERY expression and environment; the `round`
arm applies `pyRound` on BOTH targets (the emitted helper IS banker's — that
is the point), and only the `fdiv` arm is target-dependent, closed by
`jsFdiv_eq_fdiv`. (2) The naive-JS model `jsRound` kept OUTSIDE the semantics
(exactly as wave 13 kept `js32Shl` outside) plus the impossibility:
`jsRound_ne_pyRound_tie` — on EVERY even-`m` half-integer `m + ½`, naive JS
`Math.round` provably differs from Python (`m + 1 ≠ m`), an infinite witness
family; the agreement boundary `jsRound_eq_pyRound_odd_tie` (odd-`m` ties
round up to `m + 1` under BOTH — the deviation is SPECIFICALLY even-`m`) and
`jsRound_eq_pyRound_integer` (integers never diverge) pin the deviation's
exact frontier.

OUT of scope (documented, deliberate): the two-argument form
`round(x, ndigits)` (a different helper; scaling by `10^ndigits` deserves its
own wave); float-REPRESENTATION artifacts such as `round(2.675, 2) == 2.67`
(caused by `2.675` not being exactly representable in binary — our model is
exact rationals on the half grid, so there is no such artifact by
construction); `round()` of arbitrary non-half reals (no deviation there —
see above); the `int`-vs-`number` result-type distinction. -/

/-- Local parity predicates (Lean core has no `EvenW15`/`OddW15`; defined here to
    keep the axiom footprint Mathlib-free). -/
def EvenW15 (m : Int) : Prop := ∃ k, m = 2 * k

def OddW15 (m : Int) : Prop := ∃ k, m = 2 * k + 1

/-- NAIVE JS `Math.round` on a half-grid carrier: `Math.round(x) = ⌊x + ½⌋`,
    i.e. `⌊(a + 1) / 2⌋` — half up toward +∞, including on negatives
    (`Math.round(-0.5)` is `-0`: carrier `-1` ↦ `0`). Kept OUTSIDE the
    semantics: this is what a translation using native `Math.round` WOULD
    compute — which the compiler deliberately does NOT emit. -/
def jsRound (a : Int) : Int := Int.fdiv (a + 1) 2

/-- Python `round()` on a half-grid carrier — BANKER'S rounding: an even
    carrier `2*m` is the integer `m`; an odd carrier `2*m + 1` (the tie
    `m + ½`) goes to the EVEN member of `{m, m + 1}`. Total on `Int` via the
    `% 2` case split (Int `%` is `emod`, so `a % 2 ∈ {0, 1}` on every sign;
    `Int.fdiv a 2` is the floor, giving `m` for both `2*m` and `2*m + 1`).
    This is also the semantics of the EMITTED helper — compiled `round()` is
    banker's, which is why `evalRound` below uses `pyRound` for BOTH targets. -/
def pyRound (a : Int) : Int :=
  if a % 2 = 0 then Int.fdiv a 2
  else if Int.fdiv a 2 % 2 = 0 then Int.fdiv a 2 else Int.fdiv a 2 + 1

/-- Bridge: `Int.fdiv · 2` is euclidean `/ 2` (divisor positive), putting
    floor-by-2 in `omega`'s fragment — `omega` treats `Int.fdiv` as opaque. -/
theorem fdiv2_eq_ediv2 (a : Int) : Int.fdiv a 2 = a / 2 := by
  rw [Int.fdiv_eq_ediv, if_pos (Or.inl (by decide))]
  omega

-- CPython pins (banker's), each on the DOUBLED carrier (`round(2.5)` ↦
-- `pyRound 5`). Comments give the real CPython values.
#guard pyRound 1 = 0        -- round(0.5)  == 0   (tie, even neighbor below)
#guard pyRound 3 = 2        -- round(1.5)  == 2   (tie, even neighbor above)
#guard pyRound 5 = 2        -- round(2.5)  == 2   (tie, even neighbor below)
#guard pyRound 7 = 4        -- round(3.5)  == 4   (tie, even neighbor above)
#guard pyRound (-1) = 0     -- round(-0.5) == 0
#guard pyRound (-3) = -2    -- round(-1.5) == -2
#guard pyRound (-5) = -2    -- round(-2.5) == -2
#guard pyRound 4 = 2        -- round(2)    == 2   (integers pass through)
#guard pyRound (-6) = -3    -- round(-3)   == -3
-- Naive-JS witnesses (half up toward +∞) — the deviation values:
#guard jsRound 1 = 1        -- Math.round(0.5)  == 1   (≠ Python's 0)
#guard jsRound 5 = 3        -- Math.round(2.5)  == 3   (≠ Python's 2)
#guard jsRound (-3) = -1    -- Math.round(-1.5) == -1  (≠ Python's -2)
-- ... and the agreement cases (odd-m ties + integers):
#guard jsRound 3 = 2        -- Math.round(1.5)  == 2   == round(1.5)
#guard jsRound 7 = 4        -- Math.round(3.5)  == 4   == round(3.5)
#guard jsRound (-1) = 0     -- Math.round(-0.5) == -0  == round(-0.5)
#guard jsRound 4 = 2        -- Math.round(2)    == 2   == round(2)

/-- **Independent** model of the emitted banker's-round helper on a half-grid
    carrier `a` (value `a/2`): round half to even, as the SINGLE closed form
    `⌊(a + ⌊a/2⌋ mod 2) / 2⌋`. Defined WITHOUT `pyRound`'s case split — a
    genuinely different computation, tied to the reference only by the binding
    lemma below (not by definition). This is the compiled (`tgt = true`) `round`
    lowering used by `evalRound`, so `preservationRound` binds an INDEPENDENT
    target to the `pyRound` source. -/
def bankersTwin (a : Int) : Int := Int.fdiv (a + Int.fdiv a 2 % 2) 2

/-- **Binding lemma (the genuine content).** The independent closed-form twin
    computes exactly CPython's banker's `pyRound` on every half-grid carrier —
    a real proof (case analysis on `a % 2` and `⌊a/2⌋ % 2`), NOT `rfl`. -/
theorem bankersTwin_eq_pyRound (a : Int) : bankersTwin a = pyRound a := by
  simp only [bankersTwin, pyRound, fdiv2_eq_ediv2]
  omega

/-! ### The expression fragment — `round()` in the compiled semantics

REPRESENTATION (carrier-throughout, so NESTING is faithful): every node
evaluates to a CARRIER `c : Int` denoting the value `c / 2`. `lit`/`var` are
carriers a client writes (`round(2.5)` is `.round (.lit 5)`); `round` and `fdiv`
produce INTEGER results, which are re-encoded as EVEN carriers `2 * n` so a
`round` result can be fed straight back into another `round`. `fdiv` reads its
operands AS carriers (values `ca/2`, `cb/2`) and computes Python float
floor-division faithfully as `Int.fdiv ca cb` (C5), so `round`-fed and literal
half-integer `//` both denote the right value. This is what fixes the old
`round(round(1.5))` bug WITHOUT excluding it — the previous model returned a
plain integer the outer `round` mis-read as a carrier; here
`round(round(1.5)) = round(2) = 2` evaluates correctly (see the `#guard`s),
so no well-sortedness side condition is needed and NONE is imposed. -/

inductive RoundExp where
  | lit (v : Int)               -- a half-grid CARRIER `a` (value `a / 2`)
  | var (s : String)            -- env lookup (carriers in the env)
  | round (x : RoundExp)        -- Python `round(x)` — the compiled banker's helper
  | fdiv (a b : RoundExp)       -- Python `a // b` — the seed's deviation axis
deriving Repr

/-- Round-fragment eval, carrier-throughout. `tgt` selects the TARGET lowering:
    the `round` arm uses the INDEPENDENT `bankersTwin` when `tgt = true`
    (compiled) and the CPython `pyRound` reference when `tgt = false` — so
    `preservationRound` binds two differently-defined functions, not a
    same-helper equality. **`//` is FAITHFUL on the half-grid (C5).** Every node
    denotes the value `carrier / 2`; Python floor-division of the operand VALUES
    is `⌊(ca/2)/(cb/2)⌋ = ⌊ca/cb⌋ = Int.fdiv ca cb`, so the `fdiv` arm computes
    directly on the carriers `ca`, `cb` (NOT on floored `ca/2`, `cb/2` — the old
    model wrongly floored each operand first, giving `2.5 // 1.5 = 2//1 = 2`
    instead of Python's `1`). Division by zero is exactly divisor VALUE zero,
    i.e. `cb = 0` (the old `cb / 2 = 0` wrongly trapped `2.5 // 0.5`, carrier
    `cb = 1`, as div-by-zero). `tgt` selects `jsFdiv` (compiled floor-correction)
    vs `Int.fdiv` (Python reference); `round`/`fdiv` re-encode their integer
    result as an EVEN carrier `2 * n`, so nesting (`round(round …)`,
    `round(x) // round(y)`) is FAITHFUL, not excluded. -/
def evalRound (tgt : Bool) : RoundExp → Env → Option Int
  | .lit v, _ => some v
  | .var s, env => env.get s
  | .round x, env =>
      (evalRound tgt x env).map (fun a => 2 * (if tgt then bankersTwin a else pyRound a))
  | .fdiv a b, env => match evalRound tgt a env, evalRound tgt b env with
      | some ca, some cb =>
          if cb = 0 then none
          else some (2 * (if tgt then jsFdiv ca cb else Int.fdiv ca cb))
      | _, _ => none

-- executable bindings pinned to CPython round() semantics (VALUE = carrier / 2):
#guard evalRound false (.round (.lit 1)) [] = some 0    -- round(0.5) == 0  (carrier 0)
#guard evalRound true  (.round (.lit 1)) [] = some 0
#guard evalRound false (.round (.lit 5)) [] = some 4    -- round(2.5) == 2  (carrier 4)
#guard evalRound true  (.round (.lit 5)) [] = some 4
#guard evalRound true  (.round (.lit 7)) [] = some 8    -- round(3.5) == 4  (carrier 8)
#guard evalRound true  (.round (.lit (-3))) [] = some (-4)  -- round(-1.5) == -2 (carrier -4)
-- NESTED round — the previously-EXCLUDED case, now FAITHFUL (no exclusion):
#guard evalRound true  (.round (.round (.lit 3))) [] = some 4  -- round(round(1.5)) == 2 ✓
#guard evalRound false (.round (.round (.lit 3))) [] = some 4
#guard evalRound true  (.round (.round (.lit 5))) [] = some 4  -- round(round(2.5)) == 2 ✓
-- a variable through the env: round(x) with x = 1.5 (carrier 3) is 2 (carrier 4).
#guard evalRound true (.round (.var "x")) [("x", 3)] = some 4
-- `//` downstream of round, BOTH targets: round(2.5) // round(-3.5)
-- = 2 // -4 = -1 (floor; JS truncation would give 0). Targets must agree.
-- (values: round(2.5)=carrier 4→2, round(-3.5)=carrier -8→-4, 2//-4=-1→carrier -2)
#guard evalRound true  (.fdiv (.round (.lit 5)) (.round (.lit (-7)))) [] = some (-2)
#guard evalRound false (.fdiv (.round (.lit 5)) (.round (.lit (-7)))) [] = some (-2)
-- division by zero → none (ZeroDivisionError), either target.
#guard (evalRound true (.fdiv (.round (.lit 5)) (.lit 0)) []).isNone
-- C5 — FAITHFUL half-grid `//`: the SOURCE model `evalRound false` PINNED to
-- CPython on NON-integer floor-division operands (the old model floored each
-- operand first and got these WRONG). Carriers: 2.5↦5, 1.5↦3, 0.5↦1, -2.5↦-5.
-- 2.5 // 1.5 == 1.0  (Python; the OLD model gave 2//1 = 2 ↦ carrier 4, WRONG):
#guard evalRound false (.fdiv (.lit 5) (.lit 3)) [] = some 2     -- value 1.0 ↦ carrier 2
#guard evalRound true  (.fdiv (.lit 5) (.lit 3)) [] = some 2
-- 2.5 // 0.5 == 5.0  (Python; NOT div-by-zero — the OLD model trapped it, WRONG):
#guard evalRound false (.fdiv (.lit 5) (.lit 1)) [] = some 10    -- value 5.0 ↦ carrier 10
#guard evalRound true  (.fdiv (.lit 5) (.lit 1)) [] = some 10
-- -2.5 // 1.5 == -2.0  (Python floor toward -∞; negative-operand case):
#guard evalRound false (.fdiv (.lit (-5)) (.lit 3)) [] = some (-4)  -- value -2.0 ↦ carrier -4
#guard evalRound true  (.fdiv (.lit (-5)) (.lit 3)) [] = some (-4)
-- 1.5 // 0.5 == 3.0 (another non-integer divisor that is NOT zero):
#guard evalRound false (.fdiv (.lit 3) (.lit 1)) [] = some 6     -- value 3.0 ↦ carrier 6
-- div-by-zero is EXACTLY divisor VALUE zero (carrier 0), nothing else:
#guard (evalRound false (.fdiv (.lit 5) (.lit 0)) []).isNone
#guard (evalRound false (.fdiv (.lit 5) (.lit 1)) []).isSome  -- 0.5 divisor is NOT zero

/-- **`round()` preservation (Tier-3 wave 15).** For EVERY round-fragment
    expression and environment — INCLUDING nested `round(round …)`, which is NO
    longer excluded — the compiled (`tgt = true`) program computes the same value
    as the Python reference (`tgt = false`). The `round` arm binds the
    INDEPENDENT banker's twin `bankersTwin` to the CPython `pyRound` reference via
    the real fidelity lemma `bankersTwin_eq_pyRound` (covering the half-to-even
    boundary — the twin and reference are DIFFERENTLY defined, so this is not a
    same-helper `rfl`); the `//` deviation is absorbed by the emitted floor
    correction (`jsFdiv_eq_fdiv`). Native `Math.round` is proved a REAL deviation
    by `jsRound_ne_pyRound_tie` and refuted against THIS predicate by
    `roundStub_preservation_fails` below — the helper is necessary, not
    stylistic. -/
theorem preservationRound (e : RoundExp) (env : Env) :
    evalRound true e env = evalRound false e env := by
  induction e with
  | lit v => simp only [evalRound]
  | var s => simp only [evalRound]
  | round x ih =>
      simp only [evalRound, ih]
      cases evalRound false x env with
      | none => rfl
      | some a => simp [bankersTwin_eq_pyRound]
  | fdiv a b iha ihb =>
      simp only [evalRound, iha, ihb]
      cases evalRound false a env with
      | none => rfl
      | some ca =>
        cases evalRound false b env with
        | none => rfl
        | some cb =>
          by_cases hy : cb = 0
          · simp [hy]
          · simp [hy, jsFdiv_eq_fdiv ca cb hy]

/-! ### Characterization of the two rounders on the half grid — the deviation
frontier, proved exactly (the wave-13 `js32Shl_bounded`-style toolkit). -/

/-- Naive JS on an integer (even carrier): `Math.round(m) = m`. -/
theorem jsRound_int (m : Int) : jsRound (2 * m) = m := by
  simp only [jsRound]; rw [fdiv2_eq_ediv2]; omega

/-- Naive JS on ANY tie (odd carrier): `Math.round(m + ½) = m + 1` — half up
    toward +∞ unconditionally, blind to the parity of `m`. -/
theorem jsRound_tie (m : Int) : jsRound (2 * m + 1) = m + 1 := by
  simp only [jsRound]; rw [fdiv2_eq_ediv2]; omega

/-- Python on an integer: `round(m) = m`. -/
theorem pyRound_int (m : Int) : pyRound (2 * m) = m := by
  simp only [pyRound]
  rw [if_pos (by omega : (2 * m) % 2 = 0), fdiv2_eq_ediv2]
  omega

/-- Python on an even-`m` tie: `round(m + ½) = m` — banker's takes the even
    neighbor BELOW. -/
theorem pyRound_tie_even (m : Int) (h : m % 2 = 0) : pyRound (2 * m + 1) = m := by
  have hf : Int.fdiv (2 * m + 1) 2 = m := by rw [fdiv2_eq_ediv2]; omega
  simp only [pyRound]
  rw [if_neg (by omega : ¬ (2 * m + 1) % 2 = 0), hf, if_pos h]

/-- Python on an odd-`m` tie: `round(m + ½) = m + 1` — banker's takes the
    even neighbor ABOVE. -/
theorem pyRound_tie_odd (m : Int) (h : m % 2 = 1) : pyRound (2 * m + 1) = m + 1 := by
  have hf : Int.fdiv (2 * m + 1) 2 = m := by rw [fdiv2_eq_ediv2]; omega
  simp only [pyRound]
  rw [if_neg (by omega : ¬ (2 * m + 1) % 2 = 0), hf, if_neg (by omega : ¬ m % 2 = 0)]

/-- **The deviation is real on an INFINITE family** (not just the pinned
    examples): on EVERY even-`m` half-integer `m + ½`, Python's banker's
    rounding gives `m` (the even neighbor) but naive JS `Math.round` gives
    `m + 1` — so native `Math.round` provably CANNOT implement Python's
    `round()`, and the compiler's banker's helper is necessary, not
    stylistic. The round analogue of `js32_shl_strict`. -/
theorem jsRound_ne_pyRound_tie (m : Int) (h : EvenW15 m) :
    jsRound (2 * m + 1) ≠ pyRound (2 * m + 1) := by
  obtain ⟨k, rfl⟩ := h
  rw [jsRound_tie, pyRound_tie_even _ (by omega)]
  omega

/-- The AGREEMENT boundary, tie side: on odd-`m` ties both round UP to
    `m + 1` (banker's even neighbor happens to be the half-up choice) — the
    deviation is SPECIFICALLY the even-`m` ties, nothing else. -/
theorem jsRound_eq_pyRound_odd_tie (m : Int) (h : OddW15 m) :
    jsRound (2 * m + 1) = pyRound (2 * m + 1) := by
  obtain ⟨k, rfl⟩ := h
  rw [jsRound_tie, pyRound_tie_odd _ (by omega)]

/-- The AGREEMENT boundary, integer side: integers (even carriers) never
    diverge — both rounders are the identity there. -/
theorem jsRound_eq_pyRound_integer (m : Int) : jsRound (2 * m) = pyRound (2 * m) := by
  rw [jsRound_int, pyRound_int]

/-- Concrete pin: `round(0.5)` — CPython 0, naive JS `Math.round(0.5)` = 1
    (carrier 1). -/
theorem round_half_0 : pyRound 1 = 0 ∧ jsRound 1 = 1 := by decide

/-- Concrete pin: `round(2.5)` — CPython 2, naive JS `Math.round(2.5)` = 3
    (carrier 5). -/
theorem round_5_halves : pyRound 5 = 2 ∧ jsRound 5 = 3 := by decide

-- both pinned witnesses are instances of the general impossibility
-- (0.5 is the tie at m = 0, 2.5 the tie at m = 2 — both even):
example : jsRound 1 ≠ pyRound 1 := jsRound_ne_pyRound_tie 0 ⟨0, by decide⟩
example : jsRound 5 ≠ pyRound 5 := jsRound_ne_pyRound_tie 2 ⟨1, by decide⟩
-- ... and the agreement pin: Math.round(1.5) = 2 = round(1.5) (odd m = 1):
example : jsRound 3 = pyRound 3 := jsRound_eq_pyRound_odd_tie 1 ⟨0, by decide⟩

-- SPOT: the compiled `round(0.5)`, routed THROUGH `preservationRound` to the
-- Python reference and evaluated there: value 0 = carrier 0 (banker's, even
-- neighbor below). The naive-JS model provably gives 1 for the same source
-- (`round_half_0` + `jsRound_ne_pyRound_tie`), so this closes only because the
-- compiled semantics is the banker's helper.
example : evalRound true (.round (.lit 1)) [] = some 0 := by
  rw [preservationRound]
  decide

-- SPOT 2: the compiled `round(2.5)` — the headline witness — through the
-- theorem: value 2 = carrier 4 (naive JS says 3 = carrier 6).
example : evalRound true (.round (.lit 5)) [] = some 4 := by
  rw [preservationRound]
  decide

-- SPOT 3: the compiled `round(3.5)` — value 4 = carrier 8, up to 4 under
-- banker's (here half-up happens to agree; the guarantee still routes through
-- the banker's helper, not through luck).
example : evalRound true (.round (.lit 7)) [] = some 8 := by
  rw [preservationRound]
  decide

-- SPOT 3b: NESTED round — round(round(1.5)) = round(2) = 2 = carrier 4, through
-- the theorem. The previously-EXCLUDED degenerate is now a positive SPOT, and
-- it closes only because nesting is faithful (the old model gave carrier 2).
example : evalRound true (.round (.round (.lit 3))) [] = some 4 := by
  rw [preservationRound]
  decide

-- SPOT 4: `round` FEEDING the `//` deviation axis — the compiled
-- `round(2.5) // round(-3.5)` through the theorem: 2 // -4 = -1 = carrier -2
-- (floor; JS-trunc would give 0, so a weakened statement leaves this unprovable).
example : evalRound true (.fdiv (.round (.lit 5)) (.round (.lit (-7)))) [] = some (-2) := by
  rw [preservationRound]
  decide

/-- info: 'PythExpandVerify.preservationRound' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationRound

/-- info: 'PythExpandVerify.jsRound_ne_pyRound_tie' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms jsRound_ne_pyRound_tie

/-- info: 'PythExpandVerify.round_5_halves' does not depend on any axioms -/
#guard_msgs in
#print axioms round_5_halves

/-! ### C5 redux — preservation against an INDEPENDENT round target (C1 pattern)

`evalRound`'s `round` arm already uses the INDEPENDENT `bankersTwin` for the
compiled target and `pyRound` for the source, so `preservationRound` genuinely
binds two differently-defined functions (via `bankersTwin_eq_pyRound`), not a
same-helper `rfl`. The lemmas below make the C1 shape explicit at the LOWERING
level and, crucially, add the `stub_fails` twin: the SAME preservation predicate
refuted by a naive half-up `Math.round` lowering. -/

/-- A round LOWERING: the integer-carrier rounding the target emits. -/
abbrev RoundLowering := Int → Int

/-- Round preservation for a chosen lowering `R` — the re-architected statement. -/
def RoundPreserves (R : RoundLowering) : Prop := ∀ a, R a = pyRound a

/-- **Holds for the REAL lowering** (the emitted banker's helper), via the
    binding lemma — not a same-helper `rfl`. -/
theorem roundReal_preserves : RoundPreserves bankersTwin := bankersTwin_eq_pyRound

/-- **FAILS for the stub lowering** (`Math.round`, half-up): the SAME statement
    is provably false — `Math.round(0.5) = 1 ≠ round(0.5) = 0`. -/
theorem roundStub_fails : ¬ RoundPreserves jsRound := by
  intro h
  have := h 1
  rw [show jsRound 1 = 1 from by decide, show pyRound 1 = 0 from by decide] at this
  exact absurd this (by decide)

/-- A whole-expression round lowering that emits the naive half-up `Math.round`
    helper for the `round` arm — otherwise identical to `evalRound false`. This
    is what a translation using native `Math.round` WOULD compute. -/
def evalRoundStub : RoundExp → Env → Option Int
  | .lit v, _ => some v
  | .var s, env => env.get s
  | .round x, env => (evalRoundStub x env).map (fun a => 2 * jsRound a)
  | .fdiv a b, env => match evalRoundStub a env, evalRoundStub b env with
      | some ca, some cb =>
          if cb = 0 then none else some (2 * Int.fdiv ca cb)   -- faithful `//` (C5), only `round` differs
      | _, _ => none

/-- **Stub-fails against the SAME preservation predicate (C5, the C1 pattern).**
    A naive half-up `Math.round` lowering REFUTES `evalRoundStub = evalRound
    false` — concretely at `round(2.5)`, where the stub yields carrier `6`
    (value 3) but Python yields carrier `4` (value 2). So `preservationRound` is
    non-vacuous: it distinguishes the emitted banker's helper from the stub, and
    a silently-weakened statement (both sides banker's) could not express this. -/
theorem roundStub_preservation_fails :
    evalRoundStub (.round (.lit 5)) [] ≠ evalRound false (.round (.lit 5)) [] := by
  decide

-- Real twin agrees with CPython on the ties; the stub diverges on even-m ties.
#guard bankersTwin 1 = 0 ∧ bankersTwin 5 = 2 ∧ bankersTwin 7 = 4    -- round(.5/2.5/3.5)
#guard jsRound 1 = 1 ∧ jsRound 5 = 3                                 -- stub half-up (wrong)
-- the stub-vs-source divergence at 2.5 (carriers), witnessing non-vacuity:
#guard evalRoundStub (.round (.lit 5)) [] = some 6                   -- stub: value 3
#guard evalRound false (.round (.lit 5)) [] = some 4                 -- Python: value 2

/-- info: 'PythExpandVerify.bankersTwin_eq_pyRound' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms bankersTwin_eq_pyRound

/-- info: 'PythExpandVerify.roundReal_preserves' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms roundReal_preserves

/-- info: 'PythExpandVerify.roundStub_fails' does not depend on any axioms -/
#guard_msgs in #print axioms roundStub_fails

/-- info: 'PythExpandVerify.roundStub_preservation_fails' depends on axioms: [propext] -/
#guard_msgs in #print axioms roundStub_preservation_fails

/-! ## Tier-3 wave 16 — `sorted()` value order vs the JS default-`.sort()` lexicographic deviation

Python `sorted([1, 2, 10]) == [1, 2, 10]`: numbers are ordered by VALUE.
Naive JS `[1, 2, 10].sort()` (no comparator) is `[1, 10, 2]`: the DEFAULT
comparator coerces every element to a STRING and orders lexicographically —
`"10" < "2"` because the first characters compare `'1' < '2'`. So on any
list where decimal-lexicographic order differs from numeric order the naive
translation diverges. PythScribe compiles `sorted()` with an explicit
NUMERIC comparator, so the COMPILED output matches CPython on both sides.

The heart of this wave is that the two COMPARATORS are different total
orders that provably DISAGREE — no verified sorting algorithm is needed;
a minimal insertion sort (`insSort`) makes the divergence exhibitable on
concrete permutation witnesses.

Shape (re-architected in the C1 rollout, wave 12 — the wave-1 recipe):
(1) `preservationSort` — the INDEPENDENT compiled target `evalSorttgt
(L : IntDivLowering)` under the SHIPPED lowering equals the Python reference
`evalSort false` for EVERY sort-fragment expression; the `sortNode` node
applies the NUMERIC `pySort` on BOTH sides (the emitted comparator, never
the string default — faithfully so, NOT shared-wrong: the reference is
pinned to CPython below and `sort_diverges_of_disagree` proves the naive
lexicographic default CANNOT reproduce the value order), and the `fdivEach`
node carries the seed's `//` deviation axis routed through `L.fdiv` over
every LIST ELEMENT, closed by `map_jsFdiv_eq`. `SortPreserves L` is the
predicate form, proved for `jsELowering` (`preservationSort_real`) and
REFUTED for the truncating stub (`preservationSort_stub_fails`,
discriminating `sorted([x // 3 for x in [-8, 7]])` witness: the SORTED
result itself differs — floor `[-3, 2]` vs truncation `[-2, 2]`). The old
`evalSort true = evalSort false` statement was the F1 model-vs-model flag;
its `true` branch is retained below as documented LEGACY only, with NO
theorem referencing it. (2) The comparator-divergence results:
`cmpDisagree` (numeric says `2 ≤ 10`, decimal-lex says `¬("2" ≤ "10")`),
the GENERAL `sort_diverges_of_disagree` — for ANY pair on which the two
comparators flip, the lexicographic sort of that pair cannot equal the
numeric sort (the infinite family: a string-lexicographic comparator
provably cannot reproduce Python's value order on any such pair — the
`pow_not_double` / `js32_shl_strict` analogue), the concrete pin
`sort_2_10` (`jsSort [10,2] = [10,2]` but `pySort [10,2] = [2,10]`) with
corollary `sort_2_10_ne` routed THROUGH the general lemma, and the
agreement boundary `sort_agrees_single_digit` (single-digit lists agree —
the deviation is specifically the multi-digit magnitude regime, not
spurious).

Model kept in Lean core: `pyLe` is numeric `≤` on `Int`; `toDec` renders a
`Nat` as decimal digits (most-significant first, fuel-based so `decide`
evaluates it); `lexLe` is dictionary order on digit lists (compare
digit-by-digit; on a proper prefix the shorter is smaller — standard string
`<`); `jsLe a b := lexLe (toDec a.toNat) (toDec b.toNat)`.

OUT of scope (documented, deliberate): NEGATIVE numbers under the string
comparator (JS stringifies `-1` as `"-1"` and the `'-'` character sorts
BEFORE all digits — an extra, separate JS quirk; `jsLe` models the
non-negative regime via `Int.toNat`, and the compiled side never uses a
string comparator anyway); `sorted(key=...)` and `sorted(reverse=True)`
(argument surface, not the comparator deviation); mixed-type lists (CPython
3 raises `TypeError` — a dynamic-type obligation, not an ordering one);
stability of the sort (insertion sort IS stable, but no stability theorem
is claimed — the fragment sorts `Int`s where stability is unobservable);
float elements and NaN comparator behavior. -/

/-- Python's numeric comparator: order by VALUE (`a ≤ b` on `Int`). This is
    the comparator the compiler emits for `sorted()`. -/
def pyLe (a b : Int) : Bool := decide (a ≤ b)

/-- Decimal digits of `n`, most-significant first — fuel-based so it is
    total and kernel-`decide`-friendly (`toDecAux (n+1) n` always has enough
    fuel: each step divides by 10). -/
def toDecAux : Nat → Nat → List Nat
  | 0, _ => []
  | fuel + 1, n => if n < 10 then [n] else toDecAux fuel (n / 10) ++ [n % 10]

def toDec (n : Nat) : List Nat := toDecAux (n + 1) n

-- digit-rendering sanity: 0 → [0], 10 → [1,0], 100 → [1,0,0], 2 → [2].
#guard toDec 0 = [0]
#guard toDec 2 = [2]
#guard toDec 10 = [1, 0]
#guard toDec 100 = [1, 0, 0]
#guard toDec 907 = [9, 0, 7]

/-- Dictionary (lexicographic) order on digit strings — the order JS's
    DEFAULT `.sort()` comparator induces after string coercion: compare
    position-by-position; on a proper prefix the shorter string is smaller
    (standard string `<`). Total: `lexLe_total` below. -/
def lexLe : List Nat → List Nat → Bool
  | [], _ => true
  | _ :: _, [] => false
  | a :: as, b :: bs =>
      if a < b then true else if b < a then false else lexLe as bs

/-- The naive-JS default comparator on (non-negative) integers: stringify to
    decimal, compare lexicographically. `"10" ≤ "2"` holds (`'1' < '2'`),
    `"2" ≤ "10"` does not — the exact opposite of numeric order. -/
def jsLe (a b : Int) : Bool := lexLe (toDec a.toNat) (toDec b.toNat)

/-- Insertion into a list ordered by `le` — the minimal machinery needed to
    exhibit concrete permutation witnesses for either comparator. -/
def insertLe (le : Int → Int → Bool) (x : Int) : List Int → List Int
  | [] => [x]
  | y :: ys => if le x y then x :: y :: ys else y :: insertLe le x ys

/-- Plain insertion sort parameterized by the comparator. -/
def insSort (le : Int → Int → Bool) : List Int → List Int
  | [] => []
  | x :: xs => insertLe le x (insSort le xs)

/-- Python's `sorted()` (and the compiled output): numeric order. -/
def pySort : List Int → List Int := insSort pyLe

/-- Naive JS `.sort()` with the DEFAULT comparator: string-lexicographic
    order. Never emitted by the compiler — the deviation witness. -/
def jsSort : List Int → List Int := insSort jsLe

-- executable bindings pinned to CPython `sorted()` semantics:
-- sorted([1, 2, 10]) = [1, 2, 10] (CPython — numeric value order, the headline).
#guard pySort [1, 2, 10] = [1, 2, 10]
-- sorted([10, 2]) = [2, 10] (CPython).
#guard pySort [10, 2] = [2, 10]
-- sorted([3, 1, 2]) = [1, 2, 3] (CPython).
#guard pySort [3, 1, 2] = [1, 2, 3]
-- sorted([100, 20, 3]) = [3, 20, 100] (CPython).
#guard pySort [100, 20, 3] = [3, 20, 100]

-- naive-JS default-`.sort()` witnesses (what the compiler must NOT emit):
-- [1, 2, 10].sort() = [1, 10, 2] (Node/V8 — "10" < "2" because '1' < '2').
#guard jsSort [1, 2, 10] = [1, 10, 2]
-- [10, 2].sort() = [10, 2] (Node/V8 — already "sorted" lexicographically).
#guard jsSort [10, 2] = [10, 2]
-- [100, 20, 3].sort() = [100, 20, 3] (Node/V8 — "100" < "20" < "3").
#guard jsSort [100, 20, 3] = [100, 20, 3]
-- agreement regime: [3, 1, 2].sort() = [1, 2, 3] (Node/V8 — single digits,
-- lexicographic and numeric order coincide).
#guard jsSort [3, 1, 2] = [1, 2, 3]

/-- Sort-fragment expressions. `sortNode` is Python `sorted(e)`; `fdivEach`
    is `[x // d for x in e]` — the seed's `//` deviation axis threaded
    through every list element, keeping the preservation theorem non-trivial
    in the wave-13/14 pattern (`d` a nonzero literal divisor). -/
inductive SortExp where
  | lit (xs : List Int)
  | sortNode (e : SortExp)
  | fdivEach (e : SortExp) (d : Int)
deriving Repr

/-- Sort-fragment eval — `tgt = false` is the Python REFERENCE semantics
    (the only branch any theorem uses; pinned to CPython below). The
    `tgt = true` branch is LEGACY (the former F1 model-vs-model flag, which
    deviated only in the `fdivEach` arm) and is NOT the compiled target —
    the genuine compiled target is the INDEPENDENT `evalSorttgt
    (L : IntDivLowering)` below; NO theorem references `evalSort true`. The
    `sortNode` node applies the NUMERIC `pySort` because the compiler emits
    an explicit numeric comparator for `sorted()`, never the string
    default — see the comparator-divergence theorems below for the proof
    that the default would be WRONG. -/
def evalSort (tgt : Bool) : SortExp → Option (List Int)
  | .lit xs => some xs
  | .sortNode e => (evalSort tgt e).map pySort
  | .fdivEach e d =>
      match evalSort tgt e with
      | some xs =>
          if d = 0 then none
          else some (xs.map (fun x => if tgt then jsFdiv x d else Int.fdiv x d))
      | none => none

-- F9 pins: the REFERENCE `evalSort false` matches CPython on the sort
-- fragment INCLUDING the deviation axis (legacy `evalSort true` guards
-- retired — the compiled side is now pinned on `evalSortjs` below).
-- sorted([10, 2, 1]) = [1, 2, 10] (CPython — numeric value order, the headline).
#guard evalSort false (.sortNode (.lit [10, 2, 1])) = some [1, 2, 10]
-- [x // 3 for x in [-8]] = [-3] (CPython floor; JS-trunc would give -2).
#guard evalSort false (.fdivEach (.lit [-8]) 3) = some [-3]
-- the deviation axis THREADED THROUGH a sort (CPython:
-- sorted([x // 3 for x in [-8, 7]]) == sorted([-3, 2]) == [-3, 2], FLOOR;
-- truncation would give [-2, 2] — the SORTED RESULT itself differs, the
-- discriminating input).
#guard evalSort false (.sortNode (.fdivEach (.lit [-8, 7]) 3)) = some [-3, 2]
-- division by zero → none (ZeroDivisionError; CPython).
#guard (evalSort false (.fdivEach (.lit [1, 2]) 0)).isNone

/-- Element-wise `//` correction lifted over `List.map`: with a nonzero
    divisor, mapping the JS-corrected floor division equals mapping
    CPython's `Int.fdiv` — the list-shaped form of `jsFdiv_eq_fdiv`. -/
theorem map_jsFdiv_eq (d : Int) (hd : d ≠ 0) :
    ∀ xs : List Int, xs.map (fun x => jsFdiv x d) = xs.map (fun x => Int.fdiv x d)
  | [] => rfl
  | x :: xs => by
      simp only [List.map_cons, jsFdiv_eq_fdiv x d hd, map_jsFdiv_eq d hd xs]

/-- **Independent target evaluator** for the sort fragment: the compiled
    program's semantics under integer-division lowering `L` — a SEPARATE
    recursion, not a `Bool` flag on `evalSort`. The floor-division deviation
    axis (`fdivEach`) routes through `L.fdiv` over every list element, so
    the lowering is what varies. The `sortNode` node is the NUMERIC `pySort`
    IDENTICAL to the reference — faithfully so, NOT shared-wrong: the
    reference `sortNode` arm is pinned to CPython above, and
    `sort_diverges_of_disagree` below proves the naive string-lexicographic
    default comparator CANNOT reproduce the value order on any flip pair, so
    "numeric `pySort` on both sides" is the verified content of that arm
    (the emitted numeric comparator), not an artifact of copying. -/
def evalSorttgt (L : IntDivLowering) : SortExp → Option (List Int)
  | .lit xs => some xs
  | .sortNode e => (evalSorttgt L e).map pySort
  | .fdivEach e d =>
      match evalSorttgt L e with
      | some xs =>
          if d = 0 then none
          else some (xs.map (fun x => L.fdiv x d))
      | none => none

/-- The compiled sort semantics: the independent target under the SHIPPED
    lowering. -/
abbrev evalSortjs : SortExp → Option (List Int) := evalSorttgt jsELowering

/-- Sort preservation as a predicate OVER the lowering — the SAME predicate
    is proved for the shipped lowering (`preservationSort_real`) and REFUTED
    for the truncating stub (`preservationSort_stub_fails`). -/
def SortPreserves (L : IntDivLowering) : Prop :=
  ∀ e, evalSorttgt L e = evalSort false e

-- the COMPILED targets, same CPython values:
#guard evalSortjs (.sortNode (.lit [10, 2, 1])) = some [1, 2, 10]
#guard evalSortjs (.fdivEach (.lit [-8]) 3) = some [-3]
#guard evalSortjs (.sortNode (.fdivEach (.lit [-8, 7]) 3)) = some [-3, 2]
#guard (evalSortjs (.fdivEach (.lit [1, 2]) 0)).isNone
-- floor-vs-trunc contrast on the INDEPENDENT targets (the input chosen so
-- the SORTED result itself differs: floor [-3, 2], trunc [-2, 2]):
#guard evalSorttgt truncELowering (.sortNode (.fdivEach (.lit [-8, 7]) 3)) = some [-2, 2]  -- stub ✗ (CPython: [-3, 2])

/-- **Sort preservation (Tier-3 wave 16, re-architected — C1 rollout wave
    12).** The INDEPENDENT compiled target under the shipped lowering
    computes the Python reference on every sort-fragment expression:
    `sorted()` is the NUMERIC `pySort` on both sides (the emitted numeric
    comparator absorbs the string-default deviation — proved a REAL
    deviation by `sort_diverges_of_disagree` / `sort_2_10` below), and the
    element-wise `//` deviation is absorbed by the emitted floor correction
    (`map_jsFdiv_eq`). Real structural induction, not `rfl`: the `fdivEach`
    deviation arm needs the list-shaped arithmetic binding lemma. -/
theorem preservationSort (e : SortExp) :
    evalSortjs e = evalSort false e := by
  induction e with
  | lit xs => rfl
  | sortNode e ih => simp only [evalSorttgt, evalSort, ih]
  | fdivEach e d ih =>
      simp only [evalSorttgt, evalSort, ih]
      cases evalSort false e with
      | none => rfl
      | some xs =>
        by_cases hd : d = 0
        · simp [hd]
        · simp [hd, jsELowering, map_jsFdiv_eq d hd xs]

/-- The re-architected statement in predicate form: the shipped lowering
    preserves. Same content as `preservationSort`; this is the instantiation
    the stub litmus contrasts against. -/
theorem preservationSort_real : SortPreserves jsELowering := preservationSort

/-- **Stub litmus (C1 rollout wave 12).** The SAME preservation predicate is
    FALSE for the naive truncating lowering, and the DISCRIMINATING witness
    threads the deviation through a SORT so the sorted result itself
    differs: on `sorted([x // 3 for x in [-8, 7]])` the stub computes
    truncation `Int.tdiv (-8) 3 = -2` where Python floors to
    `Int.fdiv (-8) 3 = -3` (the `7 // 3 = 2` element agrees), so the stub's
    sorted list is `[-2, 2]` against Python's `[-3, 2]` — floor and
    truncation genuinely differ on the negative element, and the divergence
    survives the sort. The old `evalSort true = evalSort false` statement
    could only ever vary the same shared `//` helper choice, so stubbing the
    shipping lowering could not break it. -/
theorem preservationSort_stub_fails : ¬ SortPreserves truncELowering := by
  intro h
  have hc := h (.sortNode (.fdivEach (.lit [-8, 7]) 3))
  -- hc reduces to `some [-2, 2] = some [-3, 2]`:
  -- LHS sorts the truncated `[-2, 2]`, RHS sorts CPython's floored `[-3, 2]`.
  exact absurd hc (by decide)

/-! ### The comparator-divergence results — the deviation witness. The
string-lexicographic comparator is a genuinely DIFFERENT total order from
the numeric one, and on any pair where they flip, the induced sorts differ.
This is the analogue of wave 14's `IsDouble` impossibility model: kept
OUTSIDE the semantics (the compiled sort is numeric; `jsSort` is what the
naive translation WOULD produce, pinned so the deviation is proved real
rather than asserted). -/

/-- `lexLe` is reflexive. -/
theorem lexLe_refl : ∀ x : List Nat, lexLe x x = true
  | [] => rfl
  | a :: as => by
      show (if a < a then true else if a < a then false else lexLe as as) = true
      rw [if_neg (Nat.lt_irrefl a), if_neg (Nat.lt_irrefl a)]
      exact lexLe_refl as

/-- `lexLe` is total: if `x ≤ y` fails then `y ≤ x` holds — dictionary
    order is a total order, just a DIFFERENT one from numeric order. -/
theorem lexLe_total : ∀ x y : List Nat, lexLe x y = false → lexLe y x = true
  | [], _, h => by simp [lexLe] at h
  | _ :: _, [], _ => rfl
  | a :: as, b :: bs, h => by
      show (if b < a then true else if a < b then false else lexLe bs as) = true
      have h' : (if a < b then true else if b < a then false else lexLe as bs) = false := h
      by_cases hab : a < b
      · rw [if_pos hab] at h'
        exact absurd h' (by simp)
      · rw [if_neg hab] at h'
        by_cases hba : b < a
        · rw [if_pos hba]
        · rw [if_neg hba] at h'
          rw [if_neg hba, if_neg hab]
          exact lexLe_total as bs h'

/-- The JS default comparator is reflexive… -/
theorem jsLe_refl (a : Int) : jsLe a a = true := lexLe_refl _

/-- …and total (it IS a consistent comparator — just the WRONG order). -/
theorem jsLe_total (a b : Int) (h : jsLe a b = false) : jsLe b a = true :=
  lexLe_total _ _ h

/-- **The comparators disagree**: numerically `2 ≤ 10`, but
    decimal-lexicographically `"2" ≤ "10"` is FALSE (`'1' < '2'`, so
    `"10"` sorts first). The concrete flip pair. -/
theorem cmpDisagree : pyLe 2 10 = true ∧ jsLe 2 10 = false := by decide

/-- **The general sort impossibility (Tier-3 wave 16).** For ANY pair on
    which the two comparators flip (numeric says `a ≤ b`, lexicographic
    says `¬(a ≤ b)`), the lexicographic sort of that two-element list
    differs from the numeric sort: a string-lexicographic default
    comparator provably cannot reproduce Python's value order on ANY such
    pair — the infinite family, not one example. So the compiler's emitted
    numeric comparator is necessary, not stylistic. The sort analogue of
    `pow_not_double` / `js32_shl_strict`. -/
theorem sort_diverges_of_disagree (a b : Int)
    (hpy : pyLe a b = true) (hjs : jsLe a b = false) :
    jsSort [b, a] ≠ pySort [a, b] := by
  have hne : a ≠ b := by
    intro h
    rw [h, jsLe_refl] at hjs
    exact Bool.noConfusion hjs
  have hba : jsLe b a = true := jsLe_total a b hjs
  show insSort jsLe [b, a] ≠ insSort pyLe [a, b]
  simp only [insSort, insertLe, hba, hpy, if_true]
  intro hcontra
  injection hcontra with h1 _
  exact hne h1.symm

/-- The concrete pin: naive JS leaves `[10, 2]` untouched (already
    lexicographically "sorted": `"10" < "2"`), while CPython
    `sorted([10, 2]) == [2, 10]`. -/
theorem sort_2_10 : jsSort [10, 2] = [10, 2] ∧ pySort [10, 2] = [2, 10] := by
  decide

/-- Corollary, routed THROUGH the general lemma: on `[10, 2]` the naive-JS
    sort and Python's `sorted()` provably differ. -/
theorem sort_2_10_ne : jsSort [10, 2] ≠ pySort [10, 2] := by
  have hps : pySort [10, 2] = pySort [2, 10] := by decide
  rw [hps]
  exact sort_diverges_of_disagree 2 10 (by decide) (by decide)

/-- Agreement boundary: on SINGLE-DIGIT lists the two orders coincide —
    the deviation is specifically the multi-digit magnitude regime, not
    spurious. -/
theorem sort_agrees_single_digit :
    jsSort [3, 1, 2] = [1, 2, 3] ∧ pySort [3, 1, 2] = [1, 2, 3] := by decide

-- SPOT: the compiled `sorted([3, 1, 2])` — the INDEPENDENT target under the
-- shipped lowering — routed THROUGH `preservationSort` to the Python
-- reference and evaluated there: [1, 2, 3] (CPython).
example : evalSortjs (.sortNode (.lit [3, 1, 2])) = some [1, 2, 3] := by
  rw [preservationSort]
  decide

-- SPOT 2: the case naive JS gets WRONG — compiled `sorted([10, 2, 1])`,
-- through the theorem: [1, 2, 10] numerically (CPython); the naive default
-- comparator would give [1, 10, 2] (`jsSort` guard above). This closes only
-- because the compiled semantics is the numeric `pySort`.
example : evalSortjs (.sortNode (.lit [10, 2, 1])) = some [1, 2, 10] := by
  rw [preservationSort]
  decide

-- SPOT 3: the `//` deviation axis BITING through a sort — compiled
-- `[x // 3 for x in sorted([10, 2, -8])]`, through the theorem:
-- sorted gives [-8, 2, 10], then floor division [-3, 0, 3] (CPython;
-- JS-trunc would give -2 for the first element, so a weakened statement
-- leaves the `some` goal unprovable).
example : evalSortjs (.fdivEach (.sortNode (.lit [10, 2, -8])) 3)
    = some [-3, 0, 3] := by
  rw [preservationSort]
  decide

-- SPOT 4: nested sorts — compiled `sorted(sorted([100, 20, 3]))`, through
-- the theorem: [3, 20, 100] (CPython; idempotent on the already-sorted
-- list). Naive JS would leave the ORIGINAL order [100, 20, 3] untouched.
example : evalSortjs (.sortNode (.sortNode (.lit [100, 20, 3])))
    = some [3, 20, 100] := by
  rw [preservationSort]
  decide

-- SPOT 5: the DEVIATING sort program — compiled
-- `sorted([x // 3 for x in [-8, 7]])`, through the theorem: [-3, 2]
-- (CPython floor; the truncating stub yields the DIFFERENT sorted list
-- [-2, 2], refuted by `preservationSort_stub_fails`), so a weakened
-- statement cannot close this goal.
example : evalSortjs (.sortNode (.fdivEach (.lit [-8, 7]) 3))
    = some [-3, 2] := by
  rw [preservationSort]
  decide

/-- info: 'PythExpandVerify.preservationSort' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationSort

/-- info: 'PythExpandVerify.preservationSort_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationSort_real

/-- info: 'PythExpandVerify.preservationSort_stub_fails' does not depend on any axioms -/
#guard_msgs in
#print axioms preservationSort_stub_fails

/-- info: 'PythExpandVerify.sort_diverges_of_disagree' depends on axioms: [propext] -/
#guard_msgs in
#print axioms sort_diverges_of_disagree

/-- info: 'PythExpandVerify.sort_2_10' does not depend on any axioms -/
#guard_msgs in
#print axioms sort_2_10

/-! ## Tier-3 wave 18 — string-method offset preservation (the UTF-16-vs-code-point deviation on the METHOD surface)

Wave 11 proved string VALUES + code-point `len`/`s[i]`/`s[lo:hi]` and pinned
the D3/D5 deviation with `utf16_astral_strict`: naive JS `.length`/`s[i]`
count UTF-16 code UNITS, Python counts code POINTS, and the two strictly
diverge on every astral (≥ U+10000) string. Wave 11 explicitly scoped OUT
string METHODS. This wave lifts exactly that deviation to the offset-returning
method surface: Python `"𝔸x".index("x") == 1` and `len("𝔸x") == 2` (code
points; 𝔸 = U+1D538 is astral), while naive JS `"𝔸x".indexOf("x") === 2` and
`"𝔸x".length === 3` (the astral char is a surrogate PAIR, width 2). So every
offset-returning method (`.index`/`.find`, and by the same width argument
`.count`-of-prefix / `.rfind`) diverges as soon as an astral code point
precedes the position. PythScribe compiles these methods against the
code-point view, matching CPython.

The model REUSES wave 11's representation wholesale: a string IS its
code-point sequence `List Int` (wave 11 `SVal.sstr`), the UTF-16 width of a
code point is wave 11's `utf16Units` (1, or 2 for astral), the naive-JS length
is wave 11's `utf16Len`, and the expression layer reuses wave 11's
`SVal`/`SEnv`/`SVal.asInt`/`SVal.asCps` unchanged. New names carry the
`sm`/`Sm` prefix.

Shape (mirrors wave 11 `preservationS11` + `utf16_astral_strict`):
- `smIndex` — Python `.index`/`.find` core: the CODE-POINT position of the
  first occurrence (`Option Nat`; `.index` raises ValueError → `none`,
  `.find` returns `-1` → the total `smFind : Int` wrapper). `.index` is the
  primary; `.find` differs only in the not-found encoding.
- `js16Index` — what naive JS `.indexOf` returns: the UTF-16 CODE-UNIT
  offset = the sum of `utf16Units` over the code points before the match.
- `preservationSm` — the INDEPENDENT compiled target (`evalSmtgt`, C1-rollout
  wave 13) == Python on the method fragment (both sides code-point; the `//`
  on returned offsets routes through the lowering `L.fdiv`, closed by
  `jsFdiv_eq_fdiv`; stub litmus `preservationSm_stub_fails`).
- `smIndex_ne_js16_astral` — the GENERAL impossibility (the method-surface
  analogue of `utf16_astral_strict`): whenever an astral code point precedes
  the match, the naive-JS offset provably differs (strictly exceeds — the
  exact excess is the astral count, `js16Index_eq_smIndex_add_astral`).
- `smIndex_eq_js16_bmp` — the agreement boundary: on all-BMP strings the two
  offsets coincide, so the deviation is specifically astral-prefixed, not
  spurious.

OUT of scope (documented, deliberate — these need Unicode tables impractical
to model in Lean core, whereas the offset deviation is table-free): Unicode
CASE MAPPING (`.upper`/`.lower` — e.g. CPython `'ß'.upper() == 'SS'` changes
the length; needs the Unicode case DB), WHITESPACE-SET methods (no-arg
`.strip`/`.split` — Python's `str.isspace` set and JS `.trim`'s set differ;
needs a whitespace table), normalization (NFC/NFD), and `.encode`
(`str`↔`bytes`). Also out: substring (multi-char) needles — the single-code-
point needle already exhibits the full offset deviation. -/

/-! ### Code-point method primitives (over wave 11's `List Int` strings) -/

/-- Python `len(s)` on the method surface — literally wave 11's code-point
    length (`List.length`), named for readability here. -/
abbrev smLen (cps : List Int) : Nat := cps.length

/-- Python `.index`/`.find` core: the CODE-POINT position of the first
    occurrence of code point `c`, `none` if absent (`.index` → ValueError). -/
def smIndex : List Int → Int → Option Nat
  | [], _ => none
  | d :: rest, c => if d = c then some 0 else (smIndex rest c).map (· + 1)

/-- Python `.find`: same position as `smIndex`, but total — `-1` when absent
    (the ONLY difference between `.find` and `.index`). -/
def smFind (cps : List Int) (c : Int) : Int :=
  match smIndex cps c with
  | some i => (i : Int)
  | none => -1

/-- What naive JS `.indexOf` returns: the UTF-16 CODE-UNIT offset of the
    first occurrence — the sum of wave 11's `utf16Units` widths of the code
    points BEFORE the match (astral predecessors count 2). -/
def js16Index : List Int → Int → Option Nat
  | [], _ => none
  | d :: rest, c => if d = c then some 0 else (js16Index rest c).map (· + utf16Units d)

/-- Number of astral (width-2) code points in a string — the exact excess of
    the naive-JS offset over the Python offset. -/
def smAstralCount : List Int → Nat
  | [] => 0
  | d :: rest => (if 0x10000 ≤ d then 1 else 0) + smAstralCount rest

-- CPython pins (𝔸 = U+1D538, 𝔹 = U+1D539, both astral):
-- len("𝔸x") == 2 (code points) — naive JS "𝔸x".length === 3 (wave 11 utf16Len).
#guard smLen [0x1D538, 120] = 2
#guard utf16Len [0x1D538, 120] = 3
-- "𝔸x".index("x") == 1 (CPython); "𝔸x".find("x") == 1.
#guard smIndex [0x1D538, 120] 120 = some 1
#guard smFind [0x1D538, 120] 120 = 1
-- "𝔸x".find("A") == -1 (CPython; absent needle); .index would raise ValueError.
#guard smFind [0x1D538, 120] 65 = -1
#guard smIndex [0x1D538, 120] 65 = none
-- "ab".index("b") == 1 (CPython, all-BMP agreement case).
#guard smIndex [97, 98] 98 = some 1
-- "𝔸𝔹x".index("x") == 2 (CPython; two astrals before the match).
#guard smIndex [0x1D538, 0x1D539, 120] 120 = some 2
-- Naive-JS witnesses: "𝔸x".indexOf("x") === 2, "𝔸𝔹x".indexOf("x") === 4,
-- and the BMP agreement "ab".indexOf("b") === 1.
#guard js16Index [0x1D538, 120] 120 = some 2
#guard js16Index [0x1D538, 0x1D539, 120] 120 = some 4
#guard js16Index [97, 98] 98 = some 1

/-! ### The general deviation law (method-surface `utf16_astral_strict`) -/

theorem smAstralCount_cons (d : Int) (rest : List Int) :
    smAstralCount (d :: rest) = (if 0x10000 ≤ d then 1 else 0) + smAstralCount rest := rfl

/-- A string containing an astral code point has a POSITIVE astral count. -/
theorem smAstralCount_pos (cps : List Int) (a : Int) (ha : a ∈ cps)
    (hastral : 0x10000 ≤ a) : 0 < smAstralCount cps := by
  induction cps with
  | nil => cases ha
  | cons d rest ih =>
      rw [smAstralCount_cons]
      rcases List.mem_cons.mp ha with h1 | h2
      · subst h1; rw [if_pos hastral]; omega
      · have := ih h2; omega

/-- **The exact deviation law**: the naive-JS UTF-16 offset equals the Python
    code-point offset PLUS the number of astral code points before the match
    (each contributes width 2 instead of 1). The quantitative core of the
    method-surface deviation. -/
theorem js16Index_eq_smIndex_add_astral (cps : List Int) (c : Int) (i : Nat)
    (h : smIndex cps c = some i) :
    js16Index cps c = some (i + smAstralCount (cps.take i)) := by
  induction cps generalizing i with
  | nil => simp [smIndex] at h
  | cons d rest ih =>
      by_cases hd : d = c
      · simp only [smIndex, if_pos hd] at h
        injection h with h
        subst h
        simp [js16Index, smAstralCount, hd]
      · simp only [smIndex, if_neg hd] at h
        cases hr : smIndex rest c with
        | none => rw [hr] at h; simp at h
        | some j =>
            rw [hr] at h
            simp only [Option.map_some] at h
            injection h with h
            subst h
            have hrec := ih j hr
            simp only [js16Index, if_neg hd, hrec, Option.map_some,
                       List.take_succ_cons, smAstralCount_cons, utf16Units]
            by_cases hda : (0x10000 : Int) ≤ d
            · simp only [if_pos hda]; congr 1; omega
            · simp only [if_neg hda]; congr 1; omega

/-- **The GENERAL impossibility — `utf16_astral_strict` lifted to the method
    surface.** Whenever ANY astral (width-2) code point occurs BEFORE the
    match position, the naive-JS `.indexOf` offset provably differs from
    Python's `.index` offset — so naive UTF-16 `.indexOf` CANNOT implement
    Python `.index`/`.find`, and the compiler's code-point method helpers are
    necessary, not stylistic. -/
theorem smIndex_ne_js16_astral (cps : List Int) (c : Int) (i : Nat)
    (h : smIndex cps c = some i) (a : Int) (ha : a ∈ cps.take i)
    (hastral : 0x10000 ≤ a) : js16Index cps c ≠ some i := by
  rw [js16Index_eq_smIndex_add_astral cps c i h]
  intro hcontra
  injection hcontra with hcontra
  have hpos := smAstralCount_pos (cps.take i) a ha hastral
  omega

/-- Strict form: with an astral predecessor the naive-JS offset STRICTLY
    exceeds the Python offset (by the astral count). -/
theorem js16Index_gt_of_astral (cps : List Int) (c : Int) (i : Nat)
    (h : smIndex cps c = some i) (a : Int) (ha : a ∈ cps.take i)
    (hastral : 0x10000 ≤ a) :
    ∃ j, js16Index cps c = some j ∧ i < j := by
  refine ⟨i + smAstralCount (cps.take i),
          js16Index_eq_smIndex_add_astral cps c i h, ?_⟩
  have := smAstralCount_pos (cps.take i) a ha hastral
  omega

/-- **Agreement boundary**: on all-BMP strings (every code point width 1) the
    naive-JS offset and the Python offset COINCIDE — the deviation is
    specifically astral-prefixed, not spurious. -/
theorem smIndex_eq_js16_bmp (cps : List Int) (c : Int)
    (hbmp : ∀ a ∈ cps, a < 0x10000) :
    js16Index cps c = smIndex cps c := by
  induction cps with
  | nil => rfl
  | cons d rest ih =>
      have hd16 : ¬ (0x10000 : Int) ≤ d := by
        have := hbmp d (List.mem_cons_self ..)
        omega
      by_cases hd : d = c
      · simp only [js16Index, smIndex, if_pos hd]
      · have ih' := ih (fun a ha => hbmp a (List.mem_cons_of_mem d ha))
        simp only [js16Index, smIndex, if_neg hd, ih', utf16Units, if_neg hd16]

/-- The concrete pin (CPython vs naive JS): `"𝔸x".index("x")` — Python `1`
    (code points), naive JS `.indexOf` `2` (UTF-16 units). -/
theorem sm_pin_astral_index :
    smIndex [0x1D538, 120] 120 = some 1 ∧ js16Index [0x1D538, 120] 120 = some 2 := by
  decide

/-- Corollary routed THROUGH the general impossibility: on `"𝔸x"` the
    naive-JS offset provably is NOT Python's `1` (the astral `𝔸` precedes
    the match). -/
theorem sm_pin_astral_ne : js16Index [0x1D538, 120] 120 ≠ some 1 :=
  smIndex_ne_js16_astral [0x1D538, 120] 120 1 (by decide) 0x1D538 (by decide) (by decide)

/-- Two astral predecessors → excess 2: `"𝔸𝔹x".index("x")` — Python `2`,
    naive JS `4`. -/
theorem sm_pin_two_astrals :
    smIndex [0x1D538, 0x1D539, 120] 120 = some 2
      ∧ js16Index [0x1D538, 0x1D539, 120] 120 = some 4 := by
  decide

/-- BMP agreement pin, routed through the boundary lemma: on `"ab"` the two
    offsets coincide (`.index("b") == 1 == .indexOf("b")`). -/
theorem sm_pin_bmp_agree : js16Index [97, 98] 98 = smIndex [97, 98] 98 :=
  smIndex_eq_js16_bmp [97, 98] 98 (by decide)

/-! ### The method expression fragment and its two semantics -/

/-- String operand of a method node: literal or variable (strings live in
    wave 11's `SEnv` as `SVal.sstr` — REUSED, not redefined). -/
inductive SmStr where
  | slit (cps : List Int)
  | svar (n : String)
deriving Repr

def SmStr.eval (env : SEnv) : SmStr → Option (List Int)
  | .slit cps => some cps
  | .svar n => (env.get n).bind SVal.asCps

/-- The offset-method fragment. All method nodes RETURN ints (offsets/
    lengths), so the seed `//` deviation threads cleanly through the results
    (`fdivNode`). -/
inductive SmExp where
  | ilit (n : Int)                   -- int literal
  | ivar (n : String)                -- int variable (wave 11 SEnv/SVal.sint)
  | indexNode (s : SmStr) (c : Int)  -- compiled s.index(c) — code-point offset; ValueError → none
  | findNode (s : SmStr) (c : Int)   -- compiled s.find(c) — total, -1 when absent
  | lenNode (s : SmStr)              -- compiled len(s) — code points
  | fdivNode (a b : SmExp)           -- seed `//` deviation axis on returned offsets
deriving Repr

/-- Method-fragment REFERENCE eval (the Python side is `evalSm false`). The
    method arms use the CODE-POINT semantics (the compiler emits code-point
    helpers, never naive UTF-16 `.indexOf`/`.length`; the `js16Index` model
    above is what naive JS WOULD compute, kept OUTSIDE the semantics exactly
    as wave 11 kept `utf16Len` outside). The `tgt` flag is DOCUMENTED-LEGACY
    (the historical wave-18 `Bool`-flag copy, F1 shape): NO theorem references
    `evalSm true` — the compiled semantics is the INDEPENDENT `evalSmtgt`
    below (C1-rollout wave 13). -/
def evalSm (tgt : Bool) : SmExp → SEnv → Option Int
  | .ilit n, _ => some n
  | .ivar n, env => (env.get n).bind SVal.asInt
  | .indexNode s c, env =>
      (s.eval env).bind fun cps => (smIndex cps c).map (fun i => (i : Int))
  | .findNode s c, env => (s.eval env).map (fun cps => smFind cps c)
  | .lenNode s, env => (s.eval env).map (fun cps => (smLen cps : Int))
  | .fdivNode a b, env =>
      match evalSm tgt a env, evalSm tgt b env with
      | some x, some y =>
          if y = 0 then none
          else some (if tgt then jsFdiv x y else Int.fdiv x y)
      | _, _ => none

/-! ### Wave 13 (C1 rollout) — INDEPENDENT-target string-method preservation

The previous `preservationSm : evalSm true e env = evalSm false e env` was the
F1 model-vs-model tautology: ONE evaluator with a `Bool` flag flipping only the
`//` arm — stubbing the shipping lowering could not break it. Re-architected on
the wave-1 recipe: `evalSmtgt` is a SEPARATE recursion, parameterized by the
integer-division lowering the emitted JS uses; the `//` on RETURNED OFFSETS
(`fdivNode`) routes through `L`, while `.index`/`.find`/`len` keep the
CODE-POINT method primitives (`smIndex`/`smFind`/`smLen`) on BOTH sides — those
are exactly what the compiler emits, and the UTF-16 deviation they absorb stays
proved real by the UNTOUCHED `smIndex_ne_js16_astral`/`js16Index_*` witnesses.
The SAME predicate (`SmPreserves`) is proved for the shipped floor-correction
(`preservationSm_real`) and REFUTED for the naive truncating lowering
(`preservationSm_stub_fails`) on a witness where floor vs truncation give
DIFFERENT values of a `//`-composed method offset. -/

/-- **Independent target evaluator** for the offset-method fragment: the
    compiled program's semantics under lowering `L`. A SEPARATE recursion (not
    a `Bool` flag on `evalSm`); the `fdivNode` arm (`//` on returned offsets)
    calls the lowering's operation, mirroring the emitted JS, and the method
    arms use the same code-point primitives (`smIndex`/`smFind`/`smLen`) the
    compiler actually emits — never naive UTF-16 `.indexOf`/`.length`. -/
def evalSmtgt (L : IntDivLowering) : SmExp → SEnv → Option Int
  | .ilit n, _ => some n
  | .ivar n, env => (env.get n).bind SVal.asInt
  | .indexNode s c, env =>
      (s.eval env).bind fun cps => (smIndex cps c).map (fun i => (i : Int))
  | .findNode s c, env => (s.eval env).map (fun cps => smFind cps c)
  | .lenNode s, env => (s.eval env).map (fun cps => (smLen cps : Int))
  | .fdivNode a b, env =>
      match evalSmtgt L a env, evalSmtgt L b env with
      | some x, some y => if y = 0 then none else some (L.fdiv x y)
      | _, _ => none

/-- The compiled method semantics: the independent target under the SHIPPED
    lowering. -/
abbrev evalSmjs : SmExp → SEnv → Option Int := evalSmtgt jsELowering

-- Executable bindings — F9 pins: the REFERENCE `evalSm false` matches CPython
-- (via the primitives' own pins above), and the compiled `evalSmjs` guards
-- mirror them (the retired `evalSm true` legacy guards now live on the
-- independent target).
-- len("𝔸x") == 2 (CPython, code points; naive JS .length === 3 — utf16Len pin).
#guard evalSm false (.lenNode (.slit [0x1D538, 120])) [] = some 2
#guard evalSmjs (.lenNode (.slit [0x1D538, 120])) [] = some 2
-- "𝔸x".index("x") == 1 (CPython; naive JS .indexOf === 2 — js16Index pin).
#guard evalSm false (.indexNode (.slit [0x1D538, 120]) 120) [] = some 1
#guard evalSmjs (.indexNode (.slit [0x1D538, 120]) 120) [] = some 1
-- "𝔸x".index("A") raises ValueError → none; .find returns -1 (CPython).
#guard (evalSm false (.indexNode (.slit [0x1D538, 120]) 65) []).isNone
#guard (evalSmjs (.indexNode (.slit [0x1D538, 120]) 65) []).isNone
#guard evalSm false (.findNode (.slit [0x1D538, 120]) 65) [] = some (-1)
#guard evalSmjs (.findNode (.slit [0x1D538, 120]) 65) [] = some (-1)
-- THE `//`-ON-OFFSET DEVIATION PIN (CPython): "𝔸x".index("x") // -2 =
-- 1 // -2 = -1 (floor). JS-trunc would give 0 — the offset VALUE differs,
-- so this pin discriminates the lowerings (see the stub contrast below).
#guard evalSm false (.fdivNode (.indexNode (.slit [0x1D538, 120]) 120)
        (.ilit (-2))) [] = some (-1)
-- The .find MISS sentinel through `//` (CPython): "𝔸x".find("A") // 2 =
-- (-1) // 2 = -1 (floor). JS-trunc would give 0 — discriminating (0 is even
-- falsy where -1 is truthy).
#guard evalSm false (.fdivNode (.findNode (.slit [0x1D538, 120]) 65)
        (.ilit 2)) [] = some (-1)
-- division by zero → none (ZeroDivisionError), both sides.
#guard (evalSm false (.fdivNode (.ilit 1) (.ilit 0)) []).isNone
#guard (evalSmjs (.fdivNode (.ilit 1) (.ilit 0)) []).isNone
-- env-bound string: s = "𝔸x"; len(s) == 2 through the environment.
#guard evalSm false (.lenNode (.svar "s")) [("s", .sstr [0x1D538, 120])] = some 2

/-- Method-offset preservation as a predicate OVER the lowering — the SAME
    predicate is proved for the shipped lowering (`preservationSm_real`) and
    REFUTED for the stub (`preservationSm_stub_fails`). -/
def SmPreserves (L : IntDivLowering) : Prop :=
  ∀ e env, evalSmtgt L e env = evalSm false e env

/-- **Method-offset preservation (Tier-3 wave 18, C1-rollout wave 13
    re-architected).** The INDEPENDENT compiled target under the shipped
    lowering computes the Python reference on every offset-method expression
    and environment: `.index`/`.find`/`len` are code-point-correct on both
    sides (the emitted helpers absorb the UTF-16 deviation — proved real on
    this surface by `smIndex_ne_js16_astral`), and the `//` deviation on
    returned offsets is absorbed by the emitted floor-correction
    (`jsFdiv_eq_fdiv`). Real structural induction, not `rfl`: the deviation
    arm needs the arithmetic binding lemma. -/
theorem preservationSm (e : SmExp) (env : SEnv) :
    evalSmjs e env = evalSm false e env := by
  induction e with
  | ilit n => simp only [evalSmtgt, evalSm]
  | ivar n => simp only [evalSmtgt, evalSm]
  | indexNode s c => simp only [evalSmtgt, evalSm]
  | findNode s c => simp only [evalSmtgt, evalSm]
  | lenNode s => simp only [evalSmtgt, evalSm]
  | fdivNode a b iha ihb =>
      simp only [evalSmtgt, evalSm, iha, ihb]
      cases evalSm false a env with
      | none => rfl
      | some x =>
          cases evalSm false b env with
          | none => rfl
          | some y =>
              by_cases hy : y = 0
              · simp [hy]
              · simp [hy, jsELowering, jsFdiv_eq_fdiv x y hy]

/-- The re-architected statement, in predicate form: the shipped lowering
    preserves. Same content as `preservationSm`; this is the instantiation the
    stub litmus contrasts against. -/
theorem preservationSm_real : SmPreserves jsELowering := preservationSm

/-- **Stub litmus (wave 13).** The SAME preservation predicate is FALSE for
    the naive truncating lowering, on a DISCRIMINATING witness threading `//`
    through a RETURNED method offset: `"𝔸x".find("A") // 2` — the `.find`
    MISS sentinel `-1` divided by `2`. Python floors `-1 // 2 = -1`; the stub
    truncates `Int.tdiv (-1) 2 = 0` — the composed offset VALUE differs (and
    `0` even flips truthiness vs `-1`). This is what the old
    `evalSm true = evalSm false` statement could not express (both lowerings
    were hardwired into the same flag-controlled evaluator, so no wrong lowering
    could ever falsify it). -/
theorem preservationSm_stub_fails : ¬ SmPreserves truncELowering := by
  intro h
  have hc := h (.fdivNode (.findNode (.slit [0x1D538, 120]) 65) (.ilit 2)) []
  -- hc: stub `Int.tdiv (-1) 2 = 0` vs Python `Int.fdiv (-1) 2 = -1` —
  -- `some 0 = some (-1)` is absurd.
  exact absurd hc (by decide)

-- The contrast, concretely (the stub is a plausible naive emission, and it
-- computes a DIFFERENT offset value):
#guard evalSmjs (.fdivNode (.findNode (.slit [0x1D538, 120]) 65)
        (.ilit 2)) [] = some (-1)  -- real: floor -1 // 2 = -1
#guard evalSmtgt truncELowering (.fdivNode (.findNode (.slit [0x1D538, 120]) 65)
        (.ilit 2)) [] = some 0     -- stub: trunc -1 / 2 = 0 ✗
-- and through an INDEX offset with a negative divisor: real 1 // -2 = -1,
-- stub 1 / -2 = 0 ✗
#guard evalSmjs (.fdivNode (.indexNode (.slit [0x1D538, 120]) 120)
        (.ilit (-2))) [] = some (-1)
#guard evalSmtgt truncELowering (.fdivNode (.indexNode (.slit [0x1D538, 120]) 120)
        (.ilit (-2))) [] = some 0

-- SPOT 1: compiled len("𝔸x") on the INDEPENDENT target, routed THROUGH the
-- theorem to the Python reference: 2 code points (naive JS .length would
-- report 3 — utf16Len pin).
example : evalSmjs (.lenNode (.slit [0x1D538, 120])) [] = some 2 := by
  rw [preservationSm]
  decide

-- SPOT 2: THE case naive JS gets wrong — compiled "𝔸x".index("x"), through
-- the theorem: 1 (CPython; js16Index/.indexOf would give 2 — sm_pin_astral_ne).
example : evalSmjs (.indexNode (.slit [0x1D538, 120]) 120) [] = some 1 := by
  rw [preservationSm]
  decide

-- SPOT 3: all-BMP agreement case — compiled "ab".index("b") = 1; here even
-- naive JS agrees (smIndex_eq_js16_bmp), pinning the deviation boundary.
example : evalSmjs (.indexNode (.slit [97, 98]) 98) [] = some 1 := by
  rw [preservationSm]
  decide

-- SPOT 4: the `//` axis on a RETURNED offset — compiled "𝔸x".index("x") // -2
-- on the INDEPENDENT target, through the theorem: index = 1, then
-- 1 // -2 = -1 (CPython floors; the trunc stub gives 0 — the SPOT the stub
-- cannot close, see preservationSm_stub_fails' contrast guards).
example :
    evalSmjs (.fdivNode (.indexNode (.slit [0x1D538, 120]) 120) (.ilit (-2))) []
      = some (-1) := by
  rw [preservationSm]
  decide

-- SPOT 5: .find miss through an ENV-bound string — s = "𝔸x", s.find("A") = -1
-- (CPython; total, unlike .index's ValueError).
example :
    evalSmjs (.findNode (.svar "s") 65) [("s", .sstr [0x1D538, 120])]
      = some (-1) := by
  rw [preservationSm]
  decide

/-- info: 'PythExpandVerify.preservationSm' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationSm

/-- info: 'PythExpandVerify.preservationSm_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationSm_real

/-- info: 'PythExpandVerify.preservationSm_stub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationSm_stub_fails

/-- info: 'PythExpandVerify.smIndex_ne_js16_astral' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms smIndex_ne_js16_astral

/-- info: 'PythExpandVerify.sm_pin_astral_index' does not depend on any axioms -/
#guard_msgs in
#print axioms sm_pin_astral_index


/-! ## Tier-3 wave 17 — negative-operand bitwise (infinite two's-complement vs
the JS 32-bit truncation)

This wave completes the part wave 13 explicitly scoped OUT (its header:
"`&`/`|`/`^` on NEGATIVE operands … deserves its own wave; `~x` bitwise NOT
(same reason)").

CPython semantics: integers are arbitrary-precision and negative operands are
treated as INFINITE two's-complement bit strings. Bitwise NOT is total and
exact: `~x == -x - 1` for ALL `x` (`~5 == -6`, `~(-1) == 0`,
`~(2**40) == -(2**40) - 1`). The clean general identity family follows:
`-1 & x == x`, `-1 | x == -1`, `x ^ -1 == ~x == -x - 1` — for EVERY `x`,
including integers far above bit 31. Native JavaScript bitwise operators
coerce BOTH operands through `ToInt32` first, so they agree on small operands
(`-1 & 255` is `255` in both) but DIVERGE as soon as a bit above position 31
matters: native JS `-1 & (2**40 - 1)` is `-1` (both operands wrap to the same
32-bit pattern `0xFFFFFFFF`), not `2**40 - 1`; native JS `~(2**40)` is `-1`
(the operand wraps to `0`), not `-(2**40) - 1`. The compiler emits BigInt
bitwise helpers (`pyBitAnd`/`pyBitOr`/`pyBitXor`/`pyBitNot`), which implement
the infinite two's-complement semantics exactly, so the COMPILED output
matches CPython on negatives too (the differential runs confirm `-1 & 0xFF`,
`~5`, `5 ^ -1` all equal the CPython values).

SHIPPING-BINDING NOTE (wave-14 iter2, the F9 the binding was FOR): binding
this model to the emitted lowering exposed a REAL shipping compiler bug —
unary `~` was emitted as raw JS `(~operand)` (ToInt32; there was NO
`pyBitNot`), so shipped `~(2**40)` computed `-1`, the exact `js32Not`
deviation `js32_not_strict` proves impossible-if-correct. Fixed AT ROOT
(pythscribe emit.rs: `UnaryOp::BitNot` now routes through the BigInt-aware
`pyBitNot` helper, added to `runtime/src/operators.js` + the inline runtime
mirror; CPython-differential re-run: `pyths run` on `~5`, `~1099511627776`,
`~(-7//2)`, `~(-1)`, `~0` all equal CPython — in particular
`~(2**40) = -1099511627777`, previously `-1` — and `~1.5`/`~"a"` raise
TypeError). The `~(2**40)` pin is ALSO held shipped-vs-CPython in the tier-3
shipped-binding harness (`experiments/pbt-ps/tier3_shipped_binding.py`,
`nb_bnot_2pow40` + companions), so the `nbNot (2^40)` `#guard` below is
bound to the real emitter, not only the model. An IN-REPO emitter binding is
also committed: the codegen test `test_bitnot_routes_through_pybitnot_shipping
_binding` asserts `~` emits `pyBitNot(` (never raw `(~`) on both default and
`--target worker` targets (the worker path needed the whole bitwise family
exported from `runtime/src/core.js` — fixed). Binary `&`/`|`/`^` already
routed through their helpers and were never affected on the default/inline
paths. B1 boundary (documented, out of scope): `pyBitNot` raises `TypeError`
on a statically-known float, but a whole-VALUED float reaching `~` through an
untyped parameter yields `~1 = -2` — the same whole-float deviation as crit-11
(floats share an untagged JS Number with ints), not a wave-14 defect.

Shape (the C1 independent-target pattern, re-architected in rollout wave 14):
(1) `preservationNb` — the INDEPENDENT compiled target `evalNbtgt
(L : IntDivLowering)` (a SEPARATE recursion, not a `Bool` flag) under the
shipped lowering equals the Python reference `evalNb false` for EVERY
expression and environment; all bitwise arms are the arbitrary-precision
two's-complement operations on BOTH sides (the emitted helpers, never the
native operators — faithfully so, NOT shared-wrong: the reference is
CPython-pinned and the naive-JS alternative is refuted by the
`js32_*_strict` witnesses), and the `//` node routes through `L.fdiv` so the
lowering is what varies, closed by `jsFdiv_eq_fdiv`. `NbPreserves L` is the
predicate form, proved for `jsELowering` (`preservationNb_real`) and REFUTED
for the truncating stub (`preservationNb_stub_fails`, discriminating
`~(-7 // 2)` witness). (2) A faithful model of the NAIVE JS negative bitwise
(`js32Band`/`js32Not`, kept OUTSIDE the semantics, reusing wave 13's
`toInt32`) plus impossibility results: `js32Band_bounded`/`js32Not_bounded`
confine every naive-JS result to `[-2^31, 2^31)`, so
`js32_band_neg_one_strict` — Python `-1 & x = x`, but for ANY `x ≥ 2^31` the
naive-JS AND provably cannot return it — and `js32_not_strict` — Python
`~x = -x - 1` escapes below `-2^31` for any `x ≥ 2^31`, out of the naive-JS
range. `js32_band_neg_one_deviation`/`js32_not_deviation` pin the concrete
`2**40` witnesses.

Model: `nbNot x := -x - 1` — Python `~`, total on all of `Int` (this single
equation IS CPython's definition of `~` on the infinite two's-complement
view). The binary operators `nbBand`/`nbBor`/`nbBxor` are DEFINED by the
standard two's-complement characterization, total on ALL sign combinations:
a negative operand `a` is `~m` with `m := nbNot a ≥ 0`, and the identities
`a & ~n = a XOR (a AND n)`, `a | ~n = ~(n XOR (n AND a))`, `a ^ ~n = ~(a ^ n)`,
`~m & ~n = ~(m | n)`, `~m | ~n = ~(m & n)`, `~m ^ ~n = m ^ n` reduce every
case to `Nat.land`/`Nat.lor`/`Nat.xor` on non-negative parts (the same
operations wave 13 used for its non-negative fragment). The GENERAL theorems
delivered are the `-1`-identity family (`neg_one_and`/`neg_one_or`/
`xor_neg_one`, each quantified over ALL `x`) plus the `~` involution
(`nbNot_involution`) — exactly wave-13's stated gap, in full generality.

Honest scope note: for arbitrary pairs of negative operands the definitions
above ARE the two's-complement characterization (and are pinned against
CPython on mixed and double-negative pairs below: `-2 & -3 == -4`,
`6 | -5 == -1`, `6 ^ -5 == -3`, …), but Lean core has no independent
bit-level specification of "Python `&`" to prove them against beyond these
identities — a general `testBit`-style correctness spec would need a bit
model (Mathlib territory), deliberately out of frame here, as in wave 13. -/

/-- Python bitwise NOT `~x`, total on all `Int`: `~x = -x - 1` (CPython's
    infinite two's-complement complement). `~5 = -6`, `~(-1) = 0`,
    `~(2^40) = -(2^40) - 1`. -/
def nbNot (x : Int) : Int := -x - 1

/-- Python `a & b` on the infinite two's-complement view, total on all sign
    combinations (negative `a` is `~(nbNot a)` with `nbNot a ≥ 0`):
    `a & ~n = a XOR (a AND n)` (clear the bits shared with `n`),
    `~m & ~n = ~(m | n)`. -/
def nbBand (a b : Int) : Int :=
  if 0 ≤ a then
    if 0 ≤ b then (a.toNat &&& b.toNat : Nat)
    else (a.toNat ^^^ (a.toNat &&& (nbNot b).toNat) : Nat)
  else
    if 0 ≤ b then (b.toNat ^^^ (b.toNat &&& (nbNot a).toNat) : Nat)
    else nbNot ((nbNot a).toNat ||| (nbNot b).toNat : Nat)

/-- Python `a | b` on the infinite two's-complement view:
    `a | ~n = ~(n XOR (n AND a))` (keep only the `n`-bits missing from `a`,
    complemented), `~m | ~n = ~(m & n)`. -/
def nbBor (a b : Int) : Int :=
  if 0 ≤ a then
    if 0 ≤ b then (a.toNat ||| b.toNat : Nat)
    else nbNot ((nbNot b).toNat ^^^ ((nbNot b).toNat &&& a.toNat) : Nat)
  else
    if 0 ≤ b then
      nbNot ((nbNot a).toNat ^^^ ((nbNot a).toNat &&& b.toNat) : Nat)
    else nbNot ((nbNot a).toNat &&& (nbNot b).toNat : Nat)

/-- Python `a ^ b` on the infinite two's-complement view:
    `a ^ ~n = ~(a ^ n)`, `~m ^ ~n = m ^ n`. -/
def nbBxor (a b : Int) : Int :=
  if 0 ≤ a then
    if 0 ≤ b then (a.toNat ^^^ b.toNat : Nat)
    else nbNot (a.toNat ^^^ (nbNot b).toNat : Nat)
  else
    if 0 ≤ b then nbNot ((nbNot a).toNat ^^^ b.toNat : Nat)
    else ((nbNot a).toNat ^^^ (nbNot b).toNat : Nat)

-- executable bindings pinned to CPython negative-bitwise semantics:
#guard nbNot 5 = -6                     -- CPython: ~5 == -6
#guard nbNot (-1) = 0                   -- CPython: ~(-1) == 0
#guard nbNot 0 = -1                     -- CPython: ~0 == -1
-- ALSO pinned shipped-vs-CPython (tier3_shipped_binding.py `nb_bnot_2pow40`):
-- `pyths run` now emits `pyBitNot(1099511627776)` → -1099511627777 (wave-14
-- iter2 fix; the pre-fix raw-`~` emission computed -1).
#guard nbNot (2 ^ 40) = -1099511627777  -- CPython: ~(2**40) == -(2**40)-1
#guard nbBand (-1) 255 = 255            -- CPython: -1 & 255 == 255
#guard nbBand (-1) 0 = 0                -- CPython: -1 & 0 == 0
#guard nbBor (-1) 5 = -1                -- CPython: -1 | 5 == -1
#guard nbBxor 5 (-1) = -6               -- CPython: 5 ^ -1 == -6
-- the headline value — native JS says -1 (32-bit truncation, see js32Band):
#guard nbBand (-1) (2 ^ 40 - 1) = 1099511627775  -- CPython: -1 & (2**40-1)
-- mixed-sign and double-negative pins (general definition vs CPython):
#guard nbBand (-2) (-3) = -4            -- CPython: -2 & -3 == -4
#guard nbBor (-2) (-3) = -1             -- CPython: -2 | -3 == -1
#guard nbBxor (-2) (-3) = 3             -- CPython: -2 ^ -3 == 3
#guard nbBand 6 (-5) = 2                -- CPython: 6 & -5 == 2
#guard nbBor 6 (-5) = -1                -- CPython: 6 | -5 == -1
#guard nbBxor 6 (-5) = -3               -- CPython: 6 ^ -5 == -3
#guard nbBand 240 255 = 240             -- CPython: 0xF0 & 0xFF == 240 (wave-13 regime agrees)

/-- `~` is an involution: `~~x = x` for all `x` (CPython: `~~5 == 5`). -/
theorem nbNot_involution (x : Int) : nbNot (nbNot x) = x := by
  simp only [nbNot]; omega

/-- **The `-1` AND-identity, fully general:** Python `-1 & x = x` for EVERY
    `x` — `-1` is the all-ones infinite bit string, the identity of `&`.
    This is precisely the identity the naive-JS 32-bit AND breaks for
    `x ≥ 2^31` (`js32_band_neg_one_strict` below). -/
theorem neg_one_and (x : Int) : nbBand (-1) x = x := by
  have hm : (nbNot (-1)).toNat = 0 := by decide
  simp only [nbBand, hm]
  split
  · omega  -- impossible branch: 0 ≤ -1
  · split
    · -- x ≥ 0: x XOR (x AND 0) = x
      rw [Nat.and_zero, Nat.xor_zero]; omega
    · -- x < 0: ~(0 ||| ~x) = ~~x = x
      rw [Nat.zero_or]; simp only [nbNot]; omega

/-- **The `-1` OR-identity, fully general:** Python `-1 | x = -1` for EVERY
    `x` — `-1` is the absorbing element of `|`. -/
theorem neg_one_or (x : Int) : nbBor (-1) x = -1 := by
  have hm : (nbNot (-1)).toNat = 0 := by decide
  simp only [nbBor, hm]
  split
  · omega  -- impossible branch: 0 ≤ -1
  · split
    · -- x ≥ 0: ~(0 XOR (0 AND x)) = ~0 = -1
      rw [Nat.zero_and, Nat.zero_xor]; decide
    · -- x < 0: ~(0 AND ~x) = ~0 = -1
      rw [Nat.zero_and]; decide

/-- **The `-1` XOR-identity, fully general:** Python `x ^ -1 = ~x = -x - 1`
    for EVERY `x` — XOR with all-ones IS complement. -/
theorem xor_neg_one (x : Int) : nbBxor x (-1) = nbNot x := by
  have hm : (nbNot (-1)).toNat = 0 := by decide
  simp only [nbBxor, hm]
  split
  · split
    · omega  -- impossible branch: 0 ≤ -1
    · -- x ≥ 0: ~(x XOR 0) = ~x
      rw [Nat.xor_zero]; simp only [nbNot]; omega
  · split
    · omega  -- impossible branch: 0 ≤ -1
    · -- x < 0: (~x XOR 0) = ~x (non-negative here since x < 0)
      rw [Nat.xor_zero]; simp only [nbNot]; omega

-- the -1 identities, exercised at the pinned CPython values:
example : nbBand (-1) (2 ^ 40 - 1) = 2 ^ 40 - 1 := neg_one_and _
example : nbBor (-1) (2 ^ 40) = -1 := neg_one_or _
example : nbBxor (2 ^ 40) (-1) = nbNot (2 ^ 40) := xor_neg_one _

inductive NbExp where
  | lit (n : Int)
  | var (s : String)
  | bnot (a : NbExp)        -- Python `~a` (infinite two's-complement NOT)
  | bandNegOne (a : NbExp)  -- Python `-1 & a` (the AND identity)
  | borNegOne (a : NbExp)   -- Python `-1 | a` (the OR absorber)
  | bxorNegOne (a : NbExp)  -- Python `a ^ -1` (complement via XOR)
  | band (a b : NbExp)      -- Python `a & b`, ALL sign combinations
  | bor (a b : NbExp)       -- Python `a | b`, ALL sign combinations
  | bxor (a b : NbExp)      -- Python `a ^ b`, ALL sign combinations
  | fdiv (a b : NbExp)      -- Python `a // b` — the seed's deviation axis
deriving Repr

/-- Negative-bitwise-fragment eval — `tgt = false` is the Python REFERENCE
    semantics (the only branch any theorem uses; pinned to CPython below).
    The `tgt = true` branch is LEGACY (the former F1 model-vs-model flag,
    which deviated only in the `//` arm) and is NOT the compiled target — the
    genuine compiled target is the INDEPENDENT `evalNbtgt (L : IntDivLowering)`
    below; NO theorem references `evalNb true`. Every bitwise arm is the
    infinite two's-complement operation because the compiler emits BigInt
    helpers, never the native 32-bit operators — see `js32Band`/`js32Not`
    below for what THOSE would compute on negatives. -/
def evalNb (tgt : Bool) : NbExp → Env → Option Int
  | .lit n, _ => some n
  | .var s, env => env.get s
  | .bnot a, env => (evalNb tgt a env).map nbNot
  | .bandNegOne a, env => (evalNb tgt a env).map (fun x => nbBand (-1) x)
  | .borNegOne a, env => (evalNb tgt a env).map (fun x => nbBor (-1) x)
  | .bxorNegOne a, env => (evalNb tgt a env).map (fun x => nbBxor x (-1))
  | .band a b, env => match evalNb tgt a env, evalNb tgt b env with
      | some x, some y => some (nbBand x y)
      | _, _ => none
  | .bor a b, env => match evalNb tgt a env, evalNb tgt b env with
      | some x, some y => some (nbBor x y)
      | _, _ => none
  | .bxor a b, env => match evalNb tgt a env, evalNb tgt b env with
      | some x, some y => some (nbBxor x y)
      | _, _ => none
  | .fdiv a b, env => match evalNb tgt a env, evalNb tgt b env with
      | some x, some y =>
          if y = 0 then none else some (if tgt then jsFdiv x y else Int.fdiv x y)
      | _, _ => none

-- F9 pins: the REFERENCE `evalNb false` matches CPython on the
-- negative-bitwise fragment INCLUDING the `//` deviation axis (legacy
-- `evalNb true` guards retired — the compiled side is now pinned on
-- `evalNbjs` below).
#guard evalNb false (.bnot (.lit 5)) [] = some (-6)                 -- ~5 == -6
#guard evalNb false (.bandNegOne (.lit 255)) [] = some 255          -- -1 & 255 == 255
#guard evalNb false (.band (.lit (-2)) (.lit (-3))) [] = some (-4)  -- -2 & -3 == -4
#guard evalNb false (.bor (.lit 6) (.lit (-5))) [] = some (-1)      -- 6 | -5 == -1
#guard evalNb false (.bxorNegOne (.var "x")) [("x", 7)] = some (-8) -- 7 ^ -1 == -8
-- ~ applied twice through the env: ~~9 == 9.
#guard evalNb false (.bnot (.bnot (.var "y"))) [("y", 9)] = some 9
-- the headline >32-bit value — native JS says -1 (see js32Band below):
#guard evalNb false (.bandNegOne (.lit (2 ^ 40 - 1))) [] = some 1099511627775
-- the bare floor-div deviation value: -7 // 2 = -4 (truncation would say -3).
#guard evalNb false (.fdiv (.lit (-7)) (.lit 2)) [] = some (-4)
-- `//` FEEDING a bitwise op: ~(-7 // 2) = ~(-4) = 3
-- (floor; truncation would give ~(-3) = 2 — the discriminating composition).
#guard evalNb false (.bnot (.fdiv (.lit (-7)) (.lit 2))) [] = some 3
-- division by zero → none (ZeroDivisionError).
#guard (evalNb false (.fdiv (.lit 1) (.lit 0)) []).isNone

/-- **Independent target evaluator** for the negative-bitwise fragment: the
    compiled program's semantics under integer-division lowering `L` — a
    SEPARATE recursion, not a `Bool` flag on `evalNb`. The floor-division
    deviation axis enters in exactly one arm: the `//` node routes through
    `L.fdiv`, so the lowering is what varies. `~`/`&`/`|`/`^` (including the
    `-1`-identity forms) are the arbitrary-precision infinite
    two's-complement operations IDENTICAL to the reference — faithfully so,
    NOT shared-wrong: the reference arms are pinned to CPython above (mixed
    and double-negative sign combinations included), and
    `js32_band_neg_one_strict`/`js32_not_strict` below prove the naive 32-bit
    alternative CANNOT compute them past 32 bits, so "infinite
    two's-complement on both sides" is the verified content of those arms
    (the emitted BigInt helpers `pyBitAnd`/`pyBitOr`/`pyBitXor`/`pyBitNot` —
    `pyBitNot` made REAL in wave-14 iter2: the binding exposed that unary `~`
    previously emitted raw JS `~` (ToInt32), fixed at root in the compiler;
    see the shipping-binding note in the wave header above), not an artifact
    of copying. -/
def evalNbtgt (L : IntDivLowering) : NbExp → Env → Option Int
  | .lit n, _ => some n
  | .var s, env => env.get s
  | .bnot a, env => (evalNbtgt L a env).map nbNot
  | .bandNegOne a, env => (evalNbtgt L a env).map (fun x => nbBand (-1) x)
  | .borNegOne a, env => (evalNbtgt L a env).map (fun x => nbBor (-1) x)
  | .bxorNegOne a, env => (evalNbtgt L a env).map (fun x => nbBxor x (-1))
  | .band a b, env => match evalNbtgt L a env, evalNbtgt L b env with
      | some x, some y => some (nbBand x y)
      | _, _ => none
  | .bor a b, env => match evalNbtgt L a env, evalNbtgt L b env with
      | some x, some y => some (nbBor x y)
      | _, _ => none
  | .bxor a b, env => match evalNbtgt L a env, evalNbtgt L b env with
      | some x, some y => some (nbBxor x y)
      | _, _ => none
  | .fdiv a b, env => match evalNbtgt L a env, evalNbtgt L b env with
      | some x, some y => if y = 0 then none else some (L.fdiv x y)
      | _, _ => none

/-- The compiled negative-bitwise semantics: the independent target under the
    SHIPPED lowering. -/
abbrev evalNbjs : NbExp → Env → Option Int := evalNbtgt jsELowering

/-- Negative-bitwise preservation as a predicate OVER the lowering — the SAME
    predicate is proved for the shipped lowering (`preservationNb_real`) and
    REFUTED for the truncating stub (`preservationNb_stub_fails`). -/
def NbPreserves (L : IntDivLowering) : Prop :=
  ∀ e env, evalNbtgt L e env = evalNb false e env

-- the COMPILED targets, same CPython values (infinite two's-complement
-- end-to-end):
#guard evalNbjs (.bnot (.lit 5)) [] = some (-6)
#guard evalNbjs (.bandNegOne (.lit 255)) [] = some 255
#guard evalNbjs (.band (.lit (-2)) (.lit (-3))) [] = some (-4)
#guard evalNbjs (.bor (.lit 6) (.lit (-5))) [] = some (-1)
#guard evalNbjs (.bxorNegOne (.var "x")) [("x", 7)] = some (-8)
#guard evalNbjs (.bnot (.bnot (.var "y"))) [("y", 9)] = some 9
#guard evalNbjs (.bandNegOne (.lit (2 ^ 40 - 1))) [] = some 1099511627775
#guard evalNbjs (.fdiv (.lit (-7)) (.lit 2)) [] = some (-4)
#guard evalNbjs (.bnot (.fdiv (.lit (-7)) (.lit 2))) [] = some 3
#guard (evalNbjs (.fdiv (.lit 1) (.lit 0)) []).isNone
-- floor-vs-trunc contrast on the INDEPENDENT targets (inputs chosen so floor
-- and truncation actually differ — the DISCRIMINATING axis):
#guard evalNbtgt truncELowering (.fdiv (.lit (-7)) (.lit 2)) [] = some (-3)          -- stub `//` ✗ (CPython: -4)
#guard evalNbtgt truncELowering (.bnot (.fdiv (.lit (-7)) (.lit 2))) [] = some 2     -- stub `~(//)` ✗ (CPython: 3)

/-- **Negative-bitwise preservation (Tier-3 wave 17, re-architected — C1
    rollout wave 14).** The INDEPENDENT compiled target under the shipped
    lowering computes the Python reference on every negative-bitwise-fragment
    expression and environment: `~` and `&`/`|`/`^` on arbitrary (including
    negative) operands are the infinite two's-complement operations on both
    sides (the emitted BigInt helpers absorb the JS `ToInt32` coercion —
    proved a REAL deviation by `js32_band_neg_one_strict`/`js32_not_strict`
    below), and the `//` deviation is absorbed by the emitted floor
    correction (`jsFdiv_eq_fdiv`). Real structural induction, not `rfl`: the
    deviation arm needs the arithmetic binding lemma. -/
theorem preservationNb (e : NbExp) (env : Env) :
    evalNbjs e env = evalNb false e env := by
  induction e with
  | lit n => rfl
  | var s => rfl
  | bnot a ih => simp only [evalNbtgt, evalNb, ih]
  | bandNegOne a ih => simp only [evalNbtgt, evalNb, ih]
  | borNegOne a ih => simp only [evalNbtgt, evalNb, ih]
  | bxorNegOne a ih => simp only [evalNbtgt, evalNb, ih]
  | band a b iha ihb => simp only [evalNbtgt, evalNb, iha, ihb]
  | bor a b iha ihb => simp only [evalNbtgt, evalNb, iha, ihb]
  | bxor a b iha ihb => simp only [evalNbtgt, evalNb, iha, ihb]
  | fdiv a b iha ihb =>
      simp only [evalNbtgt, evalNb, iha, ihb]
      cases evalNb false a env with
      | none => rfl
      | some x =>
        cases evalNb false b env with
        | none => rfl
        | some y =>
          by_cases hy : y = 0
          · simp [hy]
          · simp [hy, jsELowering, jsFdiv_eq_fdiv x y hy]

/-- The re-architected statement in predicate form: the shipped lowering
    preserves. Same content as `preservationNb`; this is the instantiation
    the stub litmus contrasts against. -/
theorem preservationNb_real : NbPreserves jsELowering := preservationNb

/-- **Stub litmus (C1 rollout wave 14).** The SAME preservation predicate is
    FALSE for the naive truncating lowering, and the DISCRIMINATING witness
    composes the deviation INTO the bitwise fragment: on `~(-7 // 2)` the
    stub computes `~(Int.tdiv (-7) 2) = ~(-3) = 2` where Python floors to
    `~(Int.fdiv (-7) 2) = ~(-4) = 3` — floor and truncation genuinely differ
    on the negative operand, and the difference survives THROUGH the
    two's-complement `~`. The old `evalNb true = evalNb false` statement
    shared every bitwise helper AND could only toggle `jsFdiv` vs `Int.fdiv`
    (both floor-correct), so stubbing the shipping lowering could not break
    it. -/
theorem preservationNb_stub_fails : ¬ NbPreserves truncELowering := by
  intro h
  have hc := h (.bnot (.fdiv (.lit (-7)) (.lit 2))) []
  -- hc reduces to `some 2 = some 3`:
  -- LHS `~(Int.tdiv (-7) 2) = ~(-3) = 2` (truncation),
  -- RHS `~(Int.fdiv (-7) 2) = ~(-4) = 3` (floor).
  exact absurd hc (by decide)

/-! ### The faithful naive-JS 32-bit negative-bitwise model — the deviation
witness, kept OUTSIDE the semantics (the compiled helpers are
arbitrary-precision; this model is what the NATIVE `&`/`~` operators would
compute, pinned so the deviation is proved real rather than asserted — the
exact analogue of wave 13's `js32Shl`, reusing its `toInt32`). -/

/-- The NAIVE JS AND `a & b`: both operands wrap through `ToInt32`, the AND
    is taken on the 32-bit patterns (modeled on the unsigned residues
    `emod 2^32`, which carry the same 32 bits), and the result is read back
    as a signed 32-bit value. This models a translation using the native `&`
    operator — which the compiler deliberately does NOT emit. -/
def js32Band (a b : Int) : Int :=
  toInt32 ((a.emod (2 ^ 32)).toNat &&& (b.emod (2 ^ 32)).toNat : Nat)

/-- The NAIVE JS NOT `~x`: `ToInt32(x)` complemented in 32 bits. Since
    `~v = -v - 1` maps `[-2^31, 2^31)` into itself, this is exactly
    `toInt32 (-x - 1) = toInt32 (nbNot x)`. -/
def js32Not (x : Int) : Int := toInt32 (nbNot x)

-- THE TRUNCATION WITNESS: native JS -1 & (2**40 - 1) is -1 (both operands
-- wrap to the same 32-bit pattern 0xFFFFFFFF), NOT Python's 2**40 - 1:
#guard js32Band (-1) (2 ^ 40 - 1) = -1
#guard js32Band (-1) (2 ^ 40 - 1) ≠ 2 ^ 40 - 1
-- THE NOT WITNESS: native JS ~(2**40) is -1 (operand wraps to 0), NOT
-- Python's -(2**40) - 1:
#guard js32Not (2 ^ 40) = -1
#guard js32Not (2 ^ 40) ≠ nbNot (2 ^ 40)
-- inside 32 bits the native ops happen to agree (-1 & 255, ~5):
#guard js32Band (-1) 255 = 255
#guard js32Not 5 = -6

/-- Every naive-JS AND result lies in `[-2^31, 2^31)` — regardless of the
    operands (wave 13's `toInt32_bounds`, reused). -/
theorem js32Band_bounded (a b : Int) :
    -(2 ^ 31) ≤ js32Band a b ∧ js32Band a b < 2 ^ 31 :=
  toInt32_bounds _

/-- Every naive-JS NOT result lies in `[-2^31, 2^31)`. -/
theorem js32Not_bounded (x : Int) :
    -(2 ^ 31) ≤ js32Not x ∧ js32Not x < 2 ^ 31 :=
  toInt32_bounds _

/-- **The AND deviation is real for EVERY `x` past 32 bits** (not just the
    pinned example): Python's `-1 & x = x` (`neg_one_and`), but whenever
    `x ≥ 2^31` the native JS AND CANNOT return it — so naive JS bitwise
    cannot implement Python's negative-operand `&`, and the compiler's BigInt
    helpers are necessary, not stylistic. The negative-operand analogue of
    `js32_shl_strict`. -/
theorem js32_band_neg_one_strict (x : Int) (h : 2 ^ 31 ≤ x) :
    js32Band (-1) x ≠ x := by
  intro heq
  have hb : js32Band (-1) x < 2 ^ 31 := (js32Band_bounded (-1) x).2
  rw [heq] at hb
  exact Int.not_le.mpr hb h

/-- **The NOT deviation is real for EVERY `x` past 32 bits**: Python's
    `~x = -x - 1` lands below `-2^31` for any `x ≥ 2^31`, out of the naive-JS
    32-bit range — so native JS `~` cannot implement Python's `~` there. -/
theorem js32_not_strict (x : Int) (h : 2 ^ 31 ≤ x) :
    js32Not x ≠ nbNot x := by
  intro heq
  have hb : -(2 ^ 31) ≤ js32Not x := (js32Not_bounded x).1
  rw [heq] at hb
  simp only [nbNot] at hb
  omega

/-- The concrete truncation witness as a theorem: native JS `-1 & (2**40-1)`
    (= -1, 32-bit wrap) is NOT Python's `-1 & (2**40-1)` (= 2**40-1). -/
theorem js32_band_neg_one_deviation :
    js32Band (-1) (2 ^ 40 - 1) ≠ 2 ^ 40 - 1 := by decide

/-- The concrete NOT witness: native JS `~(2**40)` (= -1, operand wraps to 0)
    is NOT Python's `~(2**40)` (= -(2**40)-1). -/
theorem js32_not_deviation : js32Not (2 ^ 40) ≠ nbNot (2 ^ 40) := by decide

-- both pinned witnesses are instances of the general impossibilities:
example : js32Band (-1) (2 ^ 40 - 1) ≠ 2 ^ 40 - 1 :=
  js32_band_neg_one_strict _ (by decide)
example : js32Not (2 ^ 40) ≠ nbNot (2 ^ 40) := js32_not_strict _ (by decide)

-- SPOT: the compiled `~5` — the INDEPENDENT target under the shipped
-- lowering — routed THROUGH `preservationNb` to the Python reference and
-- evaluated there: -6 (CPython). The naive-JS model agrees here — the
-- deviation is specifically the >32-bit regime, not spurious.
example : evalNbjs (.bnot (.lit 5)) [] = some (-6) := by
  rw [preservationNb]
  decide

-- SPOT 2: the agreement small case — compiled `-1 & 255`, through the
-- theorem: 255 (CPython; naive JS also 255 — see the js32Band guard above).
example : evalNbjs (.bandNegOne (.lit 255)) [] = some 255 := by
  rw [preservationNb]
  decide

-- SPOT 3: THE CASE NAIVE JS GETS WRONG — compiled `-1 & (2**40 - 1)`,
-- through the theorem: 2**40 - 1, all 40 bits kept (CPython). The naive-JS
-- model provably gives -1 for the same source (`js32_band_neg_one_deviation`),
-- so this closes only because the compiled semantics is the infinite
-- two's-complement one.
example : evalNbjs (.bandNegOne (.lit (2 ^ 40 - 1))) [] = some 1099511627775 := by
  rw [preservationNb]
  decide

-- SPOT 4: the `//` deviation axis FEEDING `~` — compiled `~(-7 // 2)` in the
-- INDEPENDENT compiled semantics, through the theorem to the Python answer:
-- floor gives ~(-4) = 3; the truncating stub computes ~(-3) = 2 (pinned by
-- the contrast `#guard` above and refuted by `preservationNb_stub_fails`),
-- so a weakened (e.g. both-sides-`none`) statement leaves the `some` goal
-- unprovable.
example : evalNbjs (.bnot (.fdiv (.lit (-7)) (.lit 2))) [] = some 3 := by
  rw [preservationNb]
  decide

/-- info: 'PythExpandVerify.preservationNb' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationNb

/-- info: 'PythExpandVerify.preservationNb_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationNb_real

/-- info: 'PythExpandVerify.preservationNb_stub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationNb_stub_fails

/-- info: 'PythExpandVerify.js32_band_neg_one_strict' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms js32_band_neg_one_strict

/-- info: 'PythExpandVerify.js32_band_neg_one_deviation' depends on axioms: [propext] -/
#guard_msgs in
#print axioms js32_band_neg_one_deviation

/-! ## Naming — soundness of the compiler's identifier-naming logic

Models the identifier-naming pipeline of the JS emitter:
- `crates/pyths_codegen_js/src/emit.rs` `fn sanitize_ident` (emit.rs:3640):
  a Python identifier emitted into a JS *binding/reference* position gets a
  `$` appended iff it is in `is_js_reserved_word`'s list; every other
  identifier passes through verbatim.
- `fn is_js_reserved_word` (emit.rs:11800): the closed 42-word list below,
  transcribed verbatim.
- `crates/pyths_codegen_js/src/react.rs` `fn snake_to_camel` /
  `generic_snake_to_camel` (react.rs:11/319) and `fn nextjs_export_mapping`
  (react.rs:302).

FRAME (honest — what is modeled vs not):
- The Rust reserved list deliberately OMITS JS reserved words that are also
  Python hard keywords (`class`, `if`, `for`, `while`, `return`, `else`,
  `break`, `continue`, `try`, `with`): those can never lex as Python
  identifiers, so `sanitize_ident` never sees them. The model transcribes
  the ACTUAL list — `isJsReserved "class" = false` is real behavior, not a
  missing case. The sharper theorem `sanitize_no_ecma_reserved_collision`
  closes the apparent gap honestly: (Rust list ∪ Python keywords) covers
  the FULL ECMA-262 reserved-word set (kernel-checked below), so on the
  reachable domain — Python identifiers that are not Python keywords — the
  emitted name is never ECMA-reserved.
- emit.rs:3637's doc comment says "`super` is intentionally excluded", but
  the shipped list CONTAINS `"super"` (emit.rs:11808). The model follows
  the code, not the comment (`#guard isJsReserved "super"`). Doc drift,
  not a soundness gap: escaping `super` is conservative.
- Identifiers are ASCII (`[A-Za-z_][A-Za-z0-9_]*`, the PythScribe lexer's
  identifier shape); on this domain the Rust byte-length/char-index
  arithmetic in `generic_snake_to_camel` coincides with char indexing, so
  the char-list model below is exact.
- `snake_to_camel` tiers 1–2 (closed React hook/prop lookup tables, ~150
  entries) are finite tables, out of frame here; tiers 3–5 (`aria_*` →
  kebab, `data_*` → kebab, generic snake→camel) are modeled faithfully.
  The aria/data tiers are deliberately NON-invertible (kebab-case prop
  names, witnessed below) and apply to prop positions, not binding
  identifiers — the round-trip theorem's domain is the generic tier.
- There is no `camel_to_snake` in the Rust. `camelToSnakeChars` is a
  spec-side inverse used to STATE the round trip; the payload is that
  `generic_snake_to_camel` is injective on well-formed snake_case (no two
  snake names silently merge after conversion). -/

/-- ASCII lowercase alphabet — the exact domain `char::to_ascii_uppercase`
    acts on (Rust `is_ascii_lowercase`). -/
def lowerAlphabet : List Char :=
  ['a','b','c','d','e','f','g','h','i','j','k','l','m',
   'n','o','p','q','r','s','t','u','v','w','x','y','z']

/-- ASCII uppercase alphabet. -/
def upperAlphabet : List Char :=
  ['A','B','C','D','E','F','G','H','I','J','K','L','M',
   'N','O','P','Q','R','S','T','U','V','W','X','Y','Z']

/-- ASCII digits. -/
def asciiDigits : List Char := ['0','1','2','3','4','5','6','7','8','9']

/-- `char::is_ascii_lowercase`. -/
def isLowerAlpha (c : Char) : Bool := lowerAlphabet.contains c

/-- `char::is_ascii_uppercase`. -/
def isUpperAlpha (c : Char) : Bool := upperAlphabet.contains c

/-- `char::is_ascii_digit`. -/
def isAsciiDigit (c : Char) : Bool := asciiDigits.contains c

/-- Rust `char::to_ascii_uppercase`, modeled by its actual action: the 26
    lowercase ASCII letters map up; every other character is identity. -/
def upperChar : Char → Char
  | 'a' => 'A' | 'b' => 'B' | 'c' => 'C' | 'd' => 'D' | 'e' => 'E'
  | 'f' => 'F' | 'g' => 'G' | 'h' => 'H' | 'i' => 'I' | 'j' => 'J'
  | 'k' => 'K' | 'l' => 'L' | 'm' => 'M' | 'n' => 'N' | 'o' => 'O'
  | 'p' => 'P' | 'q' => 'Q' | 'r' => 'R' | 's' => 'S' | 't' => 'T'
  | 'u' => 'U' | 'v' => 'V' | 'w' => 'W' | 'x' => 'X' | 'y' => 'Y'
  | 'z' => 'Z'
  | c => c

/-- Rust `char::to_ascii_lowercase` (used only by the spec-side inverse). -/
def lowerChar : Char → Char
  | 'A' => 'a' | 'B' => 'b' | 'C' => 'c' | 'D' => 'd' | 'E' => 'e'
  | 'F' => 'f' | 'G' => 'g' | 'H' => 'h' | 'I' => 'i' | 'J' => 'j'
  | 'K' => 'k' | 'L' => 'l' | 'M' => 'm' | 'N' => 'n' | 'O' => 'o'
  | 'P' => 'p' | 'Q' => 'q' | 'R' => 'r' | 'S' => 's' | 'T' => 't'
  | 'U' => 'u' | 'V' => 'v' | 'W' => 'w' | 'X' => 'x' | 'Y' => 'y'
  | 'Z' => 'z'
  | c => c

/-- Kernel-checked facts about every lowercase letter, extracted pointwise
    below: uppercasing lands in the uppercase class, lower∘upper is the
    identity, and a lowercase letter is neither uppercase nor `'_'`. -/
theorem lowerAlphabet_facts :
    lowerAlphabet.all (fun c =>
      isUpperAlpha (upperChar c) && (lowerChar (upperChar c) == c) &&
      !isUpperAlpha c && !(c == '_')) = true := by decide

/-- Kernel-checked facts about every digit: not uppercase, not `'_'`. -/
theorem asciiDigits_facts :
    asciiDigits.all (fun c => !isUpperAlpha c && !(c == '_')) = true := by
  decide

theorem isLowerAlpha_mem {c : Char} (h : isLowerAlpha c = true) :
    c ∈ lowerAlphabet := by
  simpa [isLowerAlpha] using h

theorem upperChar_isUpper {c : Char} (h : isLowerAlpha c = true) :
    isUpperAlpha (upperChar c) = true := by
  have hf := List.all_eq_true.mp lowerAlphabet_facts c (isLowerAlpha_mem h)
  simp only [Bool.and_eq_true] at hf
  exact hf.1.1.1

theorem lowerChar_upperChar {c : Char} (h : isLowerAlpha c = true) :
    lowerChar (upperChar c) = c := by
  have hf := List.all_eq_true.mp lowerAlphabet_facts c (isLowerAlpha_mem h)
  simp only [Bool.and_eq_true, beq_iff_eq] at hf
  exact hf.1.1.2

theorem isLowerAlpha_not_upper {c : Char} (h : isLowerAlpha c = true) :
    isUpperAlpha c = false := by
  have hf := List.all_eq_true.mp lowerAlphabet_facts c (isLowerAlpha_mem h)
  simp only [Bool.and_eq_true, Bool.not_eq_true'] at hf
  exact hf.1.2

theorem isLowerAlpha_ne_underscore {c : Char} (h : isLowerAlpha c = true) :
    (c == '_') = false := by
  have hf := List.all_eq_true.mp lowerAlphabet_facts c (isLowerAlpha_mem h)
  simp only [Bool.and_eq_true, Bool.not_eq_true'] at hf
  exact hf.2

theorem isAsciiDigit_not_upper {c : Char} (h : isAsciiDigit c = true) :
    isUpperAlpha c = false := by
  have hm : c ∈ asciiDigits := by simpa [isAsciiDigit] using h
  have hf := List.all_eq_true.mp asciiDigits_facts c hm
  simp only [Bool.and_eq_true, Bool.not_eq_true'] at hf
  exact hf.1

theorem isAsciiDigit_ne_underscore {c : Char} (h : isAsciiDigit c = true) :
    (c == '_') = false := by
  have hm : c ∈ asciiDigits := by simpa [isAsciiDigit] using h
  have hf := List.all_eq_true.mp asciiDigits_facts c hm
  simp only [Bool.and_eq_true, Bool.not_eq_true'] at hf
  exact hf.2

/-! ### PyIdent — valid Python identifiers

The lexer's identifier shape `[A-Za-z_][A-Za-z0-9_]*`. The single load-
bearing negative fact: `$` is NOT a Python-identifier character, so the
escape target `id ++ "$"` can never collide with an unescaped identifier. -/

/-- A character legal in a Python identifier (non-initial position). -/
def isPyIdentChar (c : Char) : Bool :=
  isLowerAlpha c || isUpperAlpha c || isAsciiDigit c || c == '_'

/-- A character legal as the first character of a Python identifier. -/
def isPyIdentStart (c : Char) : Bool :=
  isLowerAlpha c || isUpperAlpha c || c == '_'

/-- `[A-Za-z_][A-Za-z0-9_]*` on the char-list carrier. -/
def isPyIdentChars : List Char → Bool
  | [] => false
  | c :: rest => isPyIdentStart c && rest.all isPyIdentChar

/-- A valid Python identifier. -/
def isPyIdent (s : String) : Bool := isPyIdentChars s.toList

#guard isPyIdentChar '$' = false
#guard isPyIdent "default" = true
#guard isPyIdent "_private9" = true
#guard isPyIdent "snake_case_name" = true
#guard isPyIdent "9lives" = false
#guard isPyIdent "" = false
#guard isPyIdent "foo$" = false
#guard isPyIdent "kebab-case" = false

/-- `$` never occurs in a valid Python identifier. -/
theorem pyIdent_no_dollar {s : String} (h : isPyIdent s = true) :
    '$' ∉ s.toList := by
  intro hmem
  unfold isPyIdent isPyIdentChars at h
  cases hl : s.toList with
  | nil => rw [hl] at h; exact absurd h (by decide)
  | cons c rest =>
    rw [hl] at h hmem
    simp only [Bool.and_eq_true] at h
    rcases List.mem_cons.mp hmem with hc | hr
    · rw [← hc] at h
      exact absurd h.1 (by decide)
    · have := List.all_eq_true.mp h.2 '$' hr
      exact absurd this (by decide)

/-! ### The reserved-word set and the escape

`is_js_reserved_word` (emit.rs:11800–11812), transcribed verbatim, in
source order. Every entry is itself a valid Python identifier (guarded
below) — which is exactly why the emitter must rename them. -/

/-- The 42-word reserved list of `is_js_reserved_word`, verbatim. -/
def jsReservedWords : List String :=
  ["let", "const", "var", "new", "function", "this", "typeof",
   "delete", "void", "switch", "case", "default", "catch",
   "do", "enum", "export", "extends", "instanceof", "throw",
   "static", "debugger", "null", "true", "false", "undefined",
   "NaN", "Infinity", "arguments", "eval", "await", "yield",
   "super",
   "implements", "interface", "package", "private", "protected",
   "public", "finally", "in", "of", "import"]

/-- `fn is_js_reserved_word`. -/
def isJsReserved (s : String) : Bool := jsReservedWords.contains s

/-- `fn sanitize_ident` (emit.rs:3640): append `$` iff reserved. -/
def sanitizeIdent (s : String) : String :=
  if isJsReserved s then s ++ "$" else s

-- Model ↔ reality pins.
#guard sanitizeIdent "default" = "default$"
#guard sanitizeIdent "foo" = "foo"
#guard sanitizeIdent "let" = "let$"
#guard sanitizeIdent "new" = "new$"
#guard sanitizeIdent "delete" = "delete$"
#guard isJsReserved "let" = true
#guard isJsReserved "foo" = false
-- Real behavior, contra the wave brief's assumption: `class` is NOT in the
-- Rust list (it is a Python hard keyword — unreachable as an identifier).
-- The ECMA theorem below covers it on the reachable domain.
#guard isJsReserved "class" = false
#guard sanitizeIdent "class" = "class"
-- Doc-drift pin: emit.rs's comment claims `super` is excluded; the code
-- includes it. The model follows the code.
#guard isJsReserved "super" = true
#guard sanitizeIdent "super" = "super$"
-- Every reserved word is identifier-shaped (why the escape exists at all),
-- and none contains `$`.
#guard jsReservedWords.all isPyIdent = true
#guard jsReservedWords.length = 42

/-- Kernel-checked: appending `$` to any listed reserved word leaves the
    list — the escape target set is disjoint from the reserved set. -/
theorem escaped_reserved_not_reserved :
    jsReservedWords.all (fun w => !isJsReserved (w ++ "$")) = true := by
  decide

/-- **Headline 1 — no reserved-word collision.** For EVERY string (no
    hypothesis needed — stronger than the valid-identifier claim), the
    sanitized name is not a bare word of the modeled reserved set: reserved
    input gains `$` and leaves the set; non-reserved input is unchanged and
    was already outside it. -/
theorem sanitize_no_reserved_collision (s : String) :
    isJsReserved (sanitizeIdent s) = false := by
  unfold sanitizeIdent
  cases h : isJsReserved s with
  | false => simp [h]
  | true =>
    have hm : s ∈ jsReservedWords := by simpa [isJsReserved] using h
    have hf := List.all_eq_true.mp escaped_reserved_not_reserved s hm
    simpa [h] using hf

/-! ### The sharper claim — no ECMA-262 reserved word survives

`jsReservedWords` is only sound if its omissions are unreachable. The
kernel-checked `ecma_covered` closes this: every word of the FULL ECMA-262
reserved set is either in the Rust list or a Python hard keyword — and a
Python hard keyword can never lex as a Python identifier, so it can never
reach `sanitize_ident`. Hence on the reachable domain (any string that is
not a Python keyword — validity as an identifier is not even needed) the
emitted name is never ECMA-reserved. -/

/-- Python 3 hard keywords (`keyword.kwlist`, 3.10+; soft keywords
    `match`/`case`/`type` are NOT reserved and are correctly absent). -/
def pyKeywords : List String :=
  ["False", "None", "True", "and", "as", "assert", "async", "await",
   "break", "class", "continue", "def", "del", "elif", "else", "except",
   "finally", "for", "from", "global", "if", "import", "in", "is",
   "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try",
   "while", "with", "yield"]

/-- The full ECMA-262 reserved-word set: the `ReservedWord` production
    (incl. literal keywords `null`/`true`/`false`), the strict-mode future
    reserved words (`implements` … `yield`), plus `let`/`static` (strict
    binding restrictions) and `await` (module goal). -/
def ecmaReservedWords : List String :=
  ["await", "break", "case", "catch", "class", "const", "continue",
   "debugger", "default", "delete", "do", "else", "enum", "export",
   "extends", "false", "finally", "for", "function", "if", "import",
   "in", "instanceof", "new", "null", "return", "super", "switch",
   "this", "throw", "true", "try", "typeof", "var", "void", "while",
   "with", "yield",
   "let", "static",
   "implements", "interface", "package", "private", "protected", "public"]

/-- `isEcmaReserved` — spec-side, for stating the sharp theorem. -/
def isEcmaReserved (s : String) : Bool := ecmaReservedWords.contains s

#guard isEcmaReserved "class" = true
#guard pyKeywords.contains "class" = true
#guard isEcmaReserved "foo" = false

/-- Kernel-checked COVERAGE: Rust list ∪ Python keywords ⊇ ECMA reserved.
    This is the fact that makes the Rust list's omissions sound. -/
theorem ecma_covered :
    ecmaReservedWords.all
      (fun w => isJsReserved w || pyKeywords.contains w) = true := by
  decide

/-- Kernel-checked: no escaped reserved word is ECMA-reserved (`$` occurs
    in no ECMA word). -/
theorem escaped_not_ecma :
    jsReservedWords.all (fun w => !isEcmaReserved (w ++ "$")) = true := by
  decide

/-- **Headline 1′ — no ECMA-262 reserved word, on the reachable domain.**
    Any name that is not a Python hard keyword (every name the emitter can
    see: Python keywords cannot lex as identifiers) sanitizes to a
    non-ECMA-reserved name. In particular the omission of `class`/`if`/
    `for`/… from the Rust list is PROVED sound, not assumed. -/
theorem sanitize_no_ecma_reserved_collision (s : String)
    (hkw : pyKeywords.contains s = false) :
    isEcmaReserved (sanitizeIdent s) = false := by
  unfold sanitizeIdent
  cases h : isJsReserved s with
  | true =>
    have hm : s ∈ jsReservedWords := by simpa [isJsReserved] using h
    have hf := List.all_eq_true.mp escaped_not_ecma s hm
    simpa [h] using hf
  | false =>
    simp only [Bool.false_eq_true, if_false]
    cases he : isEcmaReserved s with
    | false => rfl
    | true =>
      have hm : s ∈ ecmaReservedWords := by simpa [isEcmaReserved] using he
      have hc := List.all_eq_true.mp ecma_covered s hm
      rw [h, hkw] at hc
      exact absurd hc (by decide)

/-! ### Headline 2 — injectivity: no silent shadowing

A collision would be a silent miscompile: two distinct Python bindings
emitted as one JS name. The crux is the `$`-freeness of identifiers
(`pyIdent_no_dollar`): the escaped form `id ++ "$"` can never equal an
unescaped valid identifier, and escaping is append-`$` (cancellable) while
non-escaping is the identity. -/

theorem toList_injective {a b : String} (h : a.toList = b.toList) :
    a = b := by
  have h2 := congrArg String.ofList h
  rwa [String.ofList_toList, String.ofList_toList] at h2

theorem append_dollar_toList (s : String) :
    (s ++ "$").toList = s.toList ++ ['$'] := by
  rw [String.toList_append, show "$".toList = ['$'] from by decide]

/-- **Headline 2 — `sanitize_ident` is injective on valid Python
    identifiers.** Distinct Python bindings never merge into one JS name. -/
theorem sanitize_injective (a b : String)
    (ha : isPyIdent a = true) (hb : isPyIdent b = true)
    (h : sanitizeIdent a = sanitizeIdent b) : a = b := by
  unfold sanitizeIdent at h
  cases hra : isJsReserved a with
  | true =>
    cases hrb : isJsReserved b with
    | true =>
      rw [hra, hrb] at h
      simp only [reduceIte] at h
      have ht := congrArg String.toList h
      rw [append_dollar_toList, append_dollar_toList] at ht
      exact toList_injective (List.append_cancel_right ht)
    | false =>
      rw [hra, hrb] at h
      simp only [reduceIte, Bool.false_eq_true, if_false] at h
      -- h : a ++ "$" = b, but a valid identifier `b` cannot contain `$`.
      exfalso
      apply pyIdent_no_dollar hb
      have ht := congrArg String.toList h
      rw [append_dollar_toList] at ht
      rw [← ht]
      simp
  | false =>
    cases hrb : isJsReserved b with
    | true =>
      rw [hra, hrb] at h
      simp only [reduceIte, Bool.false_eq_true, if_false] at h
      -- h : a = b ++ "$", but a valid identifier `a` cannot contain `$`.
      exfalso
      apply pyIdent_no_dollar ha
      have ht := congrArg String.toList h
      rw [append_dollar_toList] at ht
      rw [ht]
      simp
    | false =>
      rw [hra, hrb] at h
      simpa only [reduceIte, Bool.false_eq_true, if_false] using h

/-! ### snake→camel — the generic conversion and its round trip -/

/-- The loop body of `generic_snake_to_camel` (react.rs:319) strictly after
    position 0; `cap` is Rust's `capitalize_next`. Rust's index tests
    `i == 0` / `i == name.len() - 1` are reformulated positionally: this
    aux never runs at position 0, and `i = len - 1` ⟺ the tail is empty
    (on the ASCII identifier domain, byte indices = char indices, so the
    reformulation is exact). A trailing `'_'` is pushed verbatim, matching
    Rust (which also leaves `capitalize_next` set — irrelevant, the loop
    ends). -/
def stcAux : List Char → Bool → List Char
  | [], _ => []
  | c :: rest, cap =>
    if c == '_' then
      if rest.isEmpty then ['_']
      else stcAux rest true
    else if cap then upperChar c :: stcAux rest false
    else c :: stcAux rest false

/-- `generic_snake_to_camel` (react.rs:319) on the char-list carrier. A
    leading `'_'` is preserved verbatim (Rust's `i == 0` branch, which does
    NOT set `capitalize_next`). Rust's fast path
    `if !name.contains('_') { return name }` is behavior-identical to
    running the loop (no `'_'` ⇒ `capitalize_next` never set ⇒ every char
    pushed unchanged), so it needs no separate model. -/
def snakeToCamelChars : List Char → List Char
  | [] => []
  | c :: rest =>
    if c == '_' then '_' :: stcAux rest false
    else stcAux (c :: rest) false

/-- Spec-side inverse (there is NO `camel_to_snake` in the Rust): re-expand
    every uppercase ASCII letter to `'_'` + its lowercase. -/
def camelToSnakeChars : List Char → List Char
  | [] => []
  | c :: rest =>
    if isUpperAlpha c then '_' :: lowerChar c :: camelToSnakeChars rest
    else c :: camelToSnakeChars rest

/-- Well-formed snake_case, tail position: chars in `[a-z0-9_]`, every
    `'_'` internal and followed by a LOWERCASE LETTER. The
    followed-by-a-letter requirement is load-bearing, not cosmetic:
    uppercasing a digit is the identity, so `"a_1"` converts to `"a1"` and
    collides with `"a1"` itself — witnessed by `#guard` below. Digits are
    fine anywhere except immediately after `'_'`. -/
def wfSnakeAux : List Char → Bool
  | [] => true
  | c :: rest =>
    if c == '_' then
      match rest with
      | [] => false
      | d :: rest' => isLowerAlpha d && wfSnakeAux rest'
    else (isLowerAlpha c || isAsciiDigit c) && wfSnakeAux rest

/-- Well-formed snake_case: nonempty, starts with a lowercase letter, no
    leading/trailing/double underscore, `'_'` always followed by a
    lowercase letter, all chars in `[a-z0-9_]`. -/
def wellFormedSnake : List Char → Bool
  | [] => false
  | c :: rest => isLowerAlpha c && wfSnakeAux rest

-- Model ↔ reality pins (values checked against `generic_snake_to_camel`).
#guard snakeToCamelChars "generate_metadata".toList = "generateMetadata".toList
#guard snakeToCamelChars "get_server_side_props".toList = "getServerSideProps".toList
#guard snakeToCamelChars "already".toList = "already".toList
#guard snakeToCamelChars "row2_col3".toList = "row2Col3".toList
-- Rust's index branches: leading/trailing underscores preserved verbatim.
#guard snakeToCamelChars "_priv_x".toList = "_privX".toList
#guard snakeToCamelChars "x_".toList = "x_".toList
#guard snakeToCamelChars "_".toList = "_".toList
#guard snakeToCamelChars "__".toList = "__".toList
-- Domain pins.
#guard wellFormedSnake "generate_metadata".toList = true
#guard wellFormedSnake "row2_col3".toList = true
#guard wellFormedSnake "_a".toList = false
#guard wellFormedSnake "a_".toList = false
#guard wellFormedSnake "a__b".toList = false
#guard wellFormedSnake "a_1".toList = false
-- The digit collision that forces `a_1` OUT of the invertible domain:
-- two distinct snake names, one camel image.
#guard snakeToCamelChars "a_1".toList = "a1".toList
#guard snakeToCamelChars "a1".toList = "a1".toList

/-- Round trip for the tail loop: on well-formed tails, the spec-side
    inverse recovers the input exactly. -/
theorem roundTrip_aux : (l : List Char) → wfSnakeAux l = true →
    camelToSnakeChars (stcAux l false) = l
  | [], _ => rfl
  | c :: rest, h => by
    rw [wfSnakeAux.eq_def] at h
    rw [stcAux.eq_def]
    cases hc : c == '_' with
    | true =>
      simp only [hc, reduceIte] at h ⊢
      cases rest with
      | nil => exact absurd h (by decide)
      | cons d rest' =>
        simp only [Bool.and_eq_true] at h
        simp only [List.isEmpty_cons, Bool.false_eq_true, if_false]
        have hd := isLowerAlpha_ne_underscore h.1
        rw [stcAux.eq_def]
        simp only [hd, Bool.false_eq_true, if_false]
        rw [camelToSnakeChars.eq_def]
        simp only [upperChar_isUpper h.1, reduceIte,
          lowerChar_upperChar h.1]
        rw [roundTrip_aux rest' h.2, eq_of_beq hc]
    | false =>
      simp only [hc, Bool.false_eq_true, if_false] at h ⊢
      simp only [Bool.and_eq_true] at h
      have hcu : isUpperAlpha c = false := by
        cases hl : isLowerAlpha c with
        | true => exact isLowerAlpha_not_upper hl
        | false =>
          have hd : isAsciiDigit c = true := by
            have h1 := h.1
            rw [hl] at h1
            simpa using h1
          exact isAsciiDigit_not_upper hd
      rw [camelToSnakeChars.eq_def]
      simp only [hcu, Bool.false_eq_true, if_false]
      rw [roundTrip_aux rest h.2]

/-- **Headline 3 — snake↔camel round trip.** On well-formed snake_case the
    generic conversion is losslessly invertible: converting to camelCase
    and re-expanding uppercase letters recovers the exact input. Domain
    honesty: leading/trailing/double underscores and digit-after-underscore
    are excluded (the `#guard`s above witness the digit collision), and the
    aria/data kebab tiers are excluded — they are deliberately
    non-invertible prop-name conversions, shown below. -/
theorem snake_camel_round_trip (l : List Char)
    (h : wellFormedSnake l = true) :
    camelToSnakeChars (snakeToCamelChars l) = l := by
  cases l with
  | nil => exact absurd h (by decide)
  | cons c rest =>
    simp only [wellFormedSnake, Bool.and_eq_true] at h
    have hc := isLowerAlpha_ne_underscore h.1
    rw [snakeToCamelChars.eq_def]
    simp only [hc, Bool.false_eq_true, if_false]
    rw [stcAux.eq_def]
    simp only [hc, Bool.false_eq_true, if_false]
    have hcu : isUpperAlpha c = false := isLowerAlpha_not_upper h.1
    rw [camelToSnakeChars.eq_def]
    simp only [hcu, Bool.false_eq_true, if_false]
    rw [roundTrip_aux rest h.2]

/-- No-silent-merge corollary: `generic_snake_to_camel` is INJECTIVE on
    well-formed snake_case — two distinct snake names never emit the same
    camelCase name. -/
theorem snakeToCamel_injective_on_snake {a b : List Char}
    (ha : wellFormedSnake a = true) (hb : wellFormedSnake b = true)
    (h : snakeToCamelChars a = snakeToCamelChars b) : a = b := by
  have hra := snake_camel_round_trip a ha
  have hrb := snake_camel_round_trip b hb
  rw [← hra, ← hrb, h]

/-- **Off-domain collision (honesty witness).** Injectivity is DOMAIN-RESTRICTED:
    `snakeToCamelChars` is NOT injective on the full Python-identifier grammar.
    `a_1` and `a1` are both valid Python identifiers, both map to `a1` (a digit
    after `'_'` cannot be uppercased), yet they are distinct. So a compiler that
    fed arbitrary identifiers (not just well-formed snake_case) through the
    generic conversion COULD silently merge two bindings — which is exactly why
    the injectivity guarantee is scoped to `wellFormedSnake` and no call-site
    grammar theorem can widen it (the grammar genuinely admits collisions). -/
theorem snakeToCamel_collision_offdomain :
    (['a', '_', '1'] ≠ ['a', '1']) ∧
    snakeToCamelChars ['a', '_', '1'] = snakeToCamelChars ['a', '1'] := by
  decide

/-- Contrapositive of `snakeToCamel_injective_on_snake`, stated as the honest
    boundary: ANY generic-conversion collision between distinct names forces at
    least one of them OUT of the well-formed snake_case domain — the guarantee
    is precisely "no collisions WITHIN the grammar," never "no collisions." -/
theorem snakeToCamel_collision_implies_offdomain {a b : List Char}
    (hne : a ≠ b) (hcol : snakeToCamelChars a = snakeToCamelChars b) :
    wellFormedSnake a = false ∨ wellFormedSnake b = false := by
  cases hwa : wellFormedSnake a with
  | false => exact Or.inl rfl
  | true =>
    cases hwb : wellFormedSnake b with
    | false => exact Or.inr rfl
    | true => exact absurd (snakeToCamel_injective_on_snake hwa hwb hcol) hne

/-! ### The kebab tiers and the String-level wrapper -/

/-- `str::replace('_', "-")` on the char carrier. -/
def kebabChars (l : List Char) : List Char :=
  l.map (fun c => if c == '_' then '-' else c)

/-- `snake_to_camel` (react.rs:11) BELOW its closed lookup tables: tier 3
    (`aria_*` → kebab `aria-*`, gated on a nonempty rest matching
    `[a-z_]+`), tier 4 (`data_*` → kebab `data-*`, gated on nonempty
    rest), tier 5 (generic). Tiers 1–2 (the ~150-entry React hook/prop
    tables) are out of frame — this is the compiler's behavior on every
    name that misses them. -/
def snakeToCamelTier : List Char → List Char
  | 'a' :: 'r' :: 'i' :: 'a' :: '_' :: rest =>
    if !rest.isEmpty && rest.all (fun c => isLowerAlpha c || c == '_') then
      'a' :: 'r' :: 'i' :: 'a' :: '-' :: kebabChars rest
    else snakeToCamelChars ('a' :: 'r' :: 'i' :: 'a' :: '_' :: rest)
  | 'd' :: 'a' :: 't' :: 'a' :: '_' :: rest =>
    if !rest.isEmpty then 'd' :: 'a' :: 't' :: 'a' :: '-' :: kebabChars rest
    else snakeToCamelChars ('d' :: 'a' :: 't' :: 'a' :: '_' :: rest)
  | l => snakeToCamelChars l

/-- String-level `snake_to_camel` under hook/prop-table miss. -/
def snakeToCamel (s : String) : String :=
  String.ofList (snakeToCamelTier s.toList)

-- Model ↔ reality pins, incl. the wave's required gate.
#guard snakeToCamel "generate_metadata" = "generateMetadata"
#guard snakeToCamel "handle_submit_click" = "handleSubmitClick"
#guard snakeToCamel "plain" = "plain"
#guard snakeToCamel "aria_labelledby" = "aria-labelledby"
#guard snakeToCamel "aria_described_by" = "aria-described-by"
#guard snakeToCamel "data_test_id" = "data-test-id"
-- Tier gates observed: empty rest or non-`[a-z_]` rest falls to generic.
#guard snakeToCamel "aria_" = "aria_"
#guard snakeToCamel "data_" = "data_"
#guard snakeToCamel "aria_x9" = "ariaX9"
-- The kebab tiers are deliberately NON-invertible (prop-name positions,
-- not binding identifiers) — the round-trip domain excludes them:
#guard camelToSnakeChars (snakeToCamelTier "aria_label".toList) ≠ "aria_label".toList
#guard camelToSnakeChars (snakeToCamelTier "data_test_id".toList) ≠ "data_test_id".toList

/-! ### The Next.js export table — deterministic, injective, disjoint -/

/-- `nextjs_export_mapping` (react.rs:302), verbatim. Being a Lean
    function, determinism is intrinsic; injectivity and disjointness are
    proved below. -/
def nextjsExportMapping (s : String) : Option String :=
  if s == "get_static_props" then some "getStaticProps"
  else if s == "get_server_side_props" then some "getServerSideProps"
  else if s == "get_static_paths" then some "getStaticPaths"
  else if s == "generate_static_params" then some "generateStaticParams"
  else if s == "generate_metadata" then some "generateMetadata"
  else none

#guard nextjsExportMapping "generate_metadata" = some "generateMetadata"
#guard nextjsExportMapping "get_static_props" = some "getStaticProps"
#guard nextjsExportMapping "main" = none
-- The table is a cached special case of the generic conversion, not an
-- override: on all five keys it agrees with `snakeToCamel` (kernel-checked).
#guard (["get_static_props", "get_server_side_props", "get_static_paths",
         "generate_static_params", "generate_metadata"] : List String).all
       (fun k => (nextjsExportMapping k).any (fun v => snakeToCamel k == v))
       = true

/-- The five mapped outputs are pairwise distinct (kernel-checked). -/
theorem nextjs_values_nodup :
    (["getStaticProps", "getServerSideProps", "getStaticPaths",
      "generateStaticParams", "generateMetadata"] : List String).Nodup := by
  decide

/-- Exhaustive characterization: a hit on the table is one of exactly five
    key/value pairs. -/
theorem nextjs_cases {s t : String} (h : nextjsExportMapping s = some t) :
    (s = "get_static_props" ∧ t = "getStaticProps") ∨
    (s = "get_server_side_props" ∧ t = "getServerSideProps") ∨
    (s = "get_static_paths" ∧ t = "getStaticPaths") ∨
    (s = "generate_static_params" ∧ t = "generateStaticParams") ∨
    (s = "generate_metadata" ∧ t = "generateMetadata") := by
  unfold nextjsExportMapping at h
  cases h1 : s == "get_static_props" with
  | true =>
    rw [if_pos h1] at h
    exact Or.inl ⟨eq_of_beq h1, (Option.some.inj h).symm⟩
  | false =>
    rw [if_neg (ne_true_of_eq_false h1)] at h
    cases h2 : s == "get_server_side_props" with
    | true =>
      rw [if_pos h2] at h
      exact Or.inr (Or.inl ⟨eq_of_beq h2, (Option.some.inj h).symm⟩)
    | false =>
      rw [if_neg (ne_true_of_eq_false h2)] at h
      cases h3 : s == "get_static_paths" with
      | true =>
        rw [if_pos h3] at h
        exact Or.inr (Or.inr (Or.inl ⟨eq_of_beq h3, (Option.some.inj h).symm⟩))
      | false =>
        rw [if_neg (ne_true_of_eq_false h3)] at h
        cases h4 : s == "generate_static_params" with
        | true =>
          rw [if_pos h4] at h
          exact Or.inr (Or.inr (Or.inr (Or.inl
            ⟨eq_of_beq h4, (Option.some.inj h).symm⟩)))
        | false =>
          rw [if_neg (ne_true_of_eq_false h4)] at h
          cases h5 : s == "generate_metadata" with
          | true =>
            rw [if_pos h5] at h
            exact Or.inr (Or.inr (Or.inr (Or.inr
              ⟨eq_of_beq h5, (Option.some.inj h).symm⟩)))
          | false =>
            rw [if_neg (ne_true_of_eq_false h5)] at h
            exact absurd h (by simp)

/-- The table is injective: two Python export names never map to the same
    JS export name. -/
theorem nextjs_mapping_injective {a b x : String}
    (ha : nextjsExportMapping a = some x)
    (hb : nextjsExportMapping b = some x) : a = b := by
  rcases nextjs_cases ha with ⟨hs, hx⟩ | ⟨hs, hx⟩ | ⟨hs, hx⟩ | ⟨hs, hx⟩ | ⟨hs, hx⟩ <;>
    rcases nextjs_cases hb with ⟨hs', hx'⟩ | ⟨hs', hx'⟩ | ⟨hs', hx'⟩ | ⟨hs', hx'⟩ | ⟨hs', hx'⟩ <;>
      subst hs <;> subst hs' <;>
        first
          | rfl
          | (subst hx; exact absurd hx' (by decide))

/-- Every mapped output is camelCase, hence NOT well-formed snake_case.
    (Proof note: `t.toList` is first forced to a literal char list in its
    own step — one `decide` interleaving `wfSnakeAux` with a lazy `toList`
    thunk duplicates the un-decoded suffix without sharing and reduction
    blows up exponentially; two linear steps avoid it.) -/
theorem nextjs_mapped_not_snake {s t : String}
    (h : nextjsExportMapping s = some t) :
    wellFormedSnake t.toList = false := by
  rcases nextjs_cases h with ⟨_, ht⟩ | ⟨_, ht⟩ | ⟨_, ht⟩ | ⟨_, ht⟩ | ⟨_, ht⟩ <;> subst ht
  · rw [show "getStaticProps".toList
        = ['g','e','t','S','t','a','t','i','c','P','r','o','p','s']
        from by decide]
    decide
  · rw [show "getServerSideProps".toList
        = ['g','e','t','S','e','r','v','e','r','S','i','d','e','P','r','o','p','s']
        from by decide]
    decide
  · rw [show "getStaticPaths".toList
        = ['g','e','t','S','t','a','t','i','c','P','a','t','h','s']
        from by decide]
    decide
  · rw [show "generateStaticParams".toList
        = ['g','e','n','e','r','a','t','e','S','t','a','t','i','c','P','a','r','a','m','s']
        from by decide]
    decide
  · rw [show "generateMetadata".toList
        = ['g','e','n','e','r','a','t','e','M','e','t','a','d','a','t','a']
        from by decide]
    decide

/-- Rule partition: a mapped name can never collide with an
    identity-mapped well-formed snake_case name — the table cannot
    silently capture an unrelated export. -/
theorem nextjs_mapped_disjoint {s t u : String}
    (h : nextjsExportMapping s = some t)
    (hu : wellFormedSnake u.toList = true) : t ≠ u := by
  intro he
  have hns := nextjs_mapped_not_snake h
  rw [he, hu] at hns
  exact absurd hns (by decide)

/-! ### The compiled export name — composition of table ∘ generic conversion

`nextjs_mapped_disjoint` only rules out a collision with a WELL-FORMED SNAKE
`u`. That excludes the real risk: the Next.js table's output collides with the
GENERIC conversion of a camelCase source. The composition below makes this
precise. -/

/-- The compiled JS export name of a source identifier: the Next.js table when
    it hits, otherwise the generic snake→camel conversion. -/
def compiledExportName (s : String) : String :=
  (nextjsExportMapping s).getD (snakeToCamel s)

/-- **The real collision (honest correction to `nextjs_mapped_disjoint`).** The
    generic conversion — which the Next.js table merely caches (`generate_metadata`
    ↦ `generateMetadata` is exactly its generic image) — sends the two DISTINCT
    source identifiers `generate_metadata` and the already-camelCase
    `generateMetadata` to the SAME JS name. So the naming pipeline is NOT
    collision-free against a camelCase source: `nextjs_mapped_disjoint`'s
    `wellFormedSnake u` hypothesis excludes precisely this `u` (`generateMetadata`
    has an uppercase letter, so it is off the injectivity domain — cf.
    `snakeToCamel_collision_offdomain`), which is why that theorem cannot see the
    collision. Proved at char level (the collision lives entirely in the generic
    conversion). -/
theorem nextjs_generic_name_collision :
    ("generate_metadata".toList ≠ "generateMetadata".toList) ∧
    snakeToCamelChars "generate_metadata".toList
      = snakeToCamelChars "generateMetadata".toList := by
  rw [show "generate_metadata".toList
        = ['g','e','n','e','r','a','t','e','_','m','e','t','a','d','a','t','a']
        from by decide,
      show "generateMetadata".toList
        = ['g','e','n','e','r','a','t','e','M','e','t','a','d','a','t','a']
        from by decide]
  exact ⟨by decide, by decide⟩

-- The Next.js table produces the same name as generic on this key (so the
-- collision above IS a collision of the shipped compiled export names, not just
-- of the generic helper); the table caches, it does not override.
#guard nextjsExportMapping "generate_metadata" = some "generateMetadata"
#guard compiledExportName "generate_metadata" = "generateMetadata"
#guard compiledExportName "generateMetadata" = "generateMetadata"
#guard ("generate_metadata" ≠ "generateMetadata")
-- The table agrees with generic conversion on all five keys (kernel-checked):
-- the pipeline reduces to `snakeToCamel`, whose collisions are exactly the
-- generic ones (injective ONLY on well-formed snake_case).
#guard (["get_static_props", "get_server_side_props", "get_static_paths",
         "generate_static_params", "generate_metadata"] : List String).all
       (fun k => compiledExportName k == snakeToCamel k) = true

/-- **Composition theorem — the Next.js table is REDUNDANT with generic
    conversion.** For every source identifier the compiled export name is just
    the generic `snakeToCamel` result: off the table it is `snakeToCamel s` by
    definition, and on each of the five keys the table value already EQUALS the
    generic conversion (so the table CACHES, it does not override). Hence the
    whole naming pipeline reduces to `snakeToCamel`, and its collision behaviour
    is exactly the generic one (`snakeToCamel_injective_on_snake` on well-formed
    snake_case, `nextjs_generic_name_collision` off it). -/
theorem compiled_export_eq_generic (s : String) :
    compiledExportName s = snakeToCamel s := by
  unfold compiledExportName
  cases h : nextjsExportMapping s with
  | none => simp only [Option.getD_none]
  | some t =>
    simp only [Option.getD_some]
    rcases nextjs_cases h with ⟨hs, ht⟩ | ⟨hs, ht⟩ | ⟨hs, ht⟩ | ⟨hs, ht⟩ | ⟨hs, ht⟩ <;>
      subst hs <;> subst ht <;> decide

/-! ### Trust base of the naming wave — pinned as a BUILD GATE

Same discipline as the waves above: `#guard_msgs` turns the axiom
accounting into a build failure if a proof hole or new axiom appears.
Everything is at or below Lean's three standard axioms (`String` itself
carries `Classical.choice`/`Quot.sound` in Lean 4.31; the pure char-list
theorems are propext-only or axiom-free). -/

/-- info: 'PythExpandVerify.sanitize_no_reserved_collision' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms sanitize_no_reserved_collision

/-- info: 'PythExpandVerify.sanitize_no_ecma_reserved_collision' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms sanitize_no_ecma_reserved_collision

/-- info: 'PythExpandVerify.sanitize_injective' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms sanitize_injective

/-- info: 'PythExpandVerify.snake_camel_round_trip' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms snake_camel_round_trip

/-- info: 'PythExpandVerify.snakeToCamel_injective_on_snake' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms snakeToCamel_injective_on_snake

/-- info: 'PythExpandVerify.nextjs_mapping_injective' depends on axioms: [propext] -/
#guard_msgs in
#print axioms nextjs_mapping_injective

/-- info: 'PythExpandVerify.nextjs_mapped_disjoint' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms nextjs_mapped_disjoint

/-- info: 'PythExpandVerify.compiled_export_eq_generic' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms compiled_export_eq_generic

/-- info: 'PythExpandVerify.nextjs_generic_name_collision' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms nextjs_generic_name_collision

/-- info: 'PythExpandVerify.snakeToCamel_collision_implies_offdomain' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms snakeToCamel_collision_implies_offdomain


/-! ## Tier-3 wave 19 — general string methods (the SUBSTRING surface):
startswith / endswith / `in` / find / index / count / replace / lstrip /
rstrip / strip / split / join

Wave 11 proved string VALUES (concat/len/index/slice); wave 18 proved the
offset-returning methods with SINGLE-code-point needles (`.index`/`.find`)
and scoped out substring needles. This wave models the general substring
method surface with full CPython semantics:

- `smStartsWith`/`smEndsWith` — `s.startswith(t)`/`s.endswith(t)`, with
  prefix/suffix DECOMPOSITION specs (`smStartsWith_iff`/`smEndsWith_iff`).
- `smFindSub`/`smFindSubI`/`smContains` — `s.index(t)` (ValueError → `none`),
  `s.find(t)` (total, `-1`), `t in s`, SUBSTRING needles; the
  FIRST-OCCURRENCE characterization (`smFindSub_some_iff`: match at `i` AND
  at no earlier offset), absence (`smFindSub_none_iff`), the output-domain
  bound (`smFindSub_le_length`), and spec uniqueness
  (`smFindSub_spec_unique` — the anti-vacuity guard of the LeetProof triple).
- `smCount` — `s.count(t)`: NON-overlapping left-to-right scan; empty needle
  counts every gap (`len(s)+1`, the CPython rule); `smCount_pos_iff` links
  positivity to membership.
- `smReplace` — `s.replace(old, new)`: same scan structure as `smCount`
  (shared `skip` discipline), tied together by the QUANTITATIVE law
  `smReplace_length`: `len(s.replace(o, n)) = len(s) + (len(n)-len(o))*s.count(o)`
  — plus the identity laws `smReplace_self` (`s.replace(t,t) == s`) and
  `smReplace_of_not_contains`.
- `smLstrip`/`smRstrip`/`smStrip` — the strip family with EXPLICIT chars
  (no Unicode whitespace table needed); `smLstrip_spec` (all-strippable
  prefix removed, result does not start strippable) and `smLstrip_unique`
  (ANY such decomposition IS lstrip — the postcondition pins one result).
- `smSplit`/`smJoin` — `s.split(sep)` (explicit nonempty sep; `''` →
  ValueError → `none`) and `sep.join(pieces)`, tied by THE CPython
  round-trip law `smSplit_join`: `sep.join(s.split(sep)) == s`.

**Spec validation (lean-spec-quality Stage 3).** Every model function is
transliterated in `verification/spec_validate_strmethods.py` and
differentially checked against REAL CPython builtins on a boundary-heavy
corpus — 308,876 checks green (empty strings/needles, overlapping needles,
astral code points, lone surrogates, split/join round trips). The naive-JS
deviation model `js16FindSub` is additionally validated against the
ground-truth UTF-16 code-unit search (what `String.prototype.indexOf`
computes), lone surrogates excluded (ill-formed encodings — the documented
boundary of the deviation model).

**The deviation, and the SHIPPING bugs it predicted.** `js16FindSub` is
wave 18's naive-JS model generalized to substring needles:
`js16FindSub_eq_smFindSub_add_astral` (the exact excess is the astral count
before the match), `smFindSub_ne_js16_astral` (the impossibility: naive
UTF-16 `.indexOf` CANNOT implement Python `.find`/`.index` once an astral
code point precedes the match), `smFindSub_eq_js16_bmp` (all-BMP agreement
boundary). Extending the differential corpus for THIS wave then caught the
deviation LIVE in the shipping runtime: `pyFind`/`pyIndex` returned UTF-16
code-unit offsets (`'𝔸x'.find('x')` → 2, CPython says 1) and the strip
family's `Set(chars)` lookup broke on astral strip-sets
(`'𝔸a𝔸'.strip('𝔸')` was a no-op) — exactly the bug class
`smFindSub_ne_js16_astral` proves is forced by naive code-unit offsets.
Fixed in runtime.js (both copies) in this change; corpus cases
`str_find_astral*`/`str_strip_astral*` pin it.

The fragment layer REUSES wave 11's `SVal`/`SEnv` and wave 18's `SmStr`
receivers unchanged; new names carry the `sm`/`Sg` prefix. `fdivNode`
threads the seed `//` deviation through returned ints exactly as in waves
5–18 (`jsFdiv_eq_fdiv`).

OUT of scope (documented, deliberate): no-arg `strip`/`split` (Python
whitespace SET — needs a Unicode table), case mapping (`upper`/`lower` —
Unicode case DB), `find`/`count` start/end arguments and `maxsplit`
(clamping is wave-11 slice territory; the substring semantics is what is
new here), `rfind`/`rindex`/`rsplit`/`partition` (the same width argument
transfers), `format`/f-strings, `encode`. -/


/-- Bool-negation helper: `¬ b = true → b = false` (turns `by_cases`
    negatives into rewritable equations). -/
private theorem smBoolFalse {b : Bool} (h : ¬ b = true) : b = false := by
  cases b with
  | true => exact absurd rfl h
  | false => rfl

/-! ### Substring primitive: prefix decision -/

/-- Bool decision: `needle` is a prefix of `hay` (code-point lists). -/
def smIsPrefix : List Int → List Int → Bool
  | [], _ => true
  | _ :: _, [] => false
  | p :: ps, d :: ds => if p = d then smIsPrefix ps ds else false

/-- PROPOSED (human review): prefix characterization — `smIsPrefix needle hay`
    iff `hay` decomposes as `needle ++ t`. Property-based (existential
    decomposition); `t` is unique (`t = hay.drop needle.length`,
    `smIsPrefix_drop`). -/
theorem smIsPrefix_iff (needle hay : List Int) :
    smIsPrefix needle hay = true ↔ ∃ t, needle ++ t = hay := by
  induction needle generalizing hay with
  | nil => simp [smIsPrefix]
  | cons p ps ih =>
      cases hay with
      | nil =>
          simp only [smIsPrefix]
          constructor
          · intro h; simp at h
          · intro hex
            obtain ⟨t, ht⟩ := hex
            simp at ht
      | cons d ds =>
          simp only [smIsPrefix]
          by_cases hpd : p = d
          · subst hpd
            rw [if_pos rfl, ih ds]
            constructor
            · intro hex
              obtain ⟨t, ht⟩ := hex
              exact ⟨t, by rw [List.cons_append, ht]⟩
            · intro hex
              obtain ⟨t, ht⟩ := hex
              rw [List.cons_append] at ht
              refine ⟨t, ?_⟩
              injection ht with _ h2
          · rw [if_neg hpd]
            constructor
            · intro h; simp at h
            · intro hex
              obtain ⟨t, ht⟩ := hex
              rw [List.cons_append] at ht
              injection ht with h1 _
              exact absurd h1 hpd

theorem smIsPrefix_length_le (needle hay : List Int)
    (h : smIsPrefix needle hay = true) : needle.length ≤ hay.length := by
  obtain ⟨t, ht⟩ := (smIsPrefix_iff needle hay).mp h
  subst ht
  simp [List.length_append]

/-- The suffix a successful prefix match leaves is `hay.drop needle.length` —
    the uniqueness half of `smIsPrefix_iff`'s decomposition. -/
theorem smIsPrefix_drop (needle hay : List Int)
    (h : smIsPrefix needle hay = true) :
    needle ++ hay.drop needle.length = hay := by
  obtain ⟨t, ht⟩ := (smIsPrefix_iff needle hay).mp h
  subst ht
  rw [List.drop_left]

/-! ### startswith / endswith -/

/-- Python `s.startswith(pre)`. -/
def smStartsWith (hay pre : List Int) : Bool := smIsPrefix pre hay

/-- Python `s.endswith(suf)`. -/
def smEndsWith (hay suf : List Int) : Bool := smIsPrefix suf.reverse hay.reverse

/-- PROPOSED (human review): `startswith` spec — prefix decomposition. -/
theorem smStartsWith_iff (hay pre : List Int) :
    smStartsWith hay pre = true ↔ ∃ t, pre ++ t = hay := smIsPrefix_iff pre hay

/-- PROPOSED (human review): `endswith` spec — suffix decomposition. -/
theorem smEndsWith_iff (hay suf : List Int) :
    smEndsWith hay suf = true ↔ ∃ t, t ++ suf = hay := by
  unfold smEndsWith
  rw [smIsPrefix_iff]
  constructor
  · intro hex
    obtain ⟨t, ht⟩ := hex
    refine ⟨t.reverse, ?_⟩
    have := congrArg List.reverse ht
    rwa [List.reverse_append, List.reverse_reverse, List.reverse_reverse] at this
  · intro hex
    obtain ⟨t, ht⟩ := hex
    refine ⟨t.reverse, ?_⟩
    rw [← ht, List.reverse_append]

/-! ### find / index (substring needle) -/

/-- Python `.find`/`.index` core with SUBSTRING needle: first CODE-POINT
    offset where `needle` occurs in `hay`; `none` if absent. -/
def smFindSub : List Int → List Int → Option Nat
  | [], needle => if smIsPrefix needle [] then some 0 else none
  | d :: rest, needle =>
      if smIsPrefix needle (d :: rest) then some 0
      else (smFindSub rest needle).map (· + 1)

/-- Python `s.find(t)`: total, `-1` when absent. -/
def smFindSubI (hay needle : List Int) : Int :=
  match smFindSub hay needle with
  | some i => (i : Int)
  | none => -1

/-- Python `t in s` membership. -/
def smContains (hay needle : List Int) : Bool := (smFindSub hay needle).isSome

/-- PROPOSED (human review): the FIRST-OCCURRENCE characterization —
    `smFindSub hay needle = some i` iff `needle` occurs at code-point offset
    `i` AND at no earlier offset. Postcondition + minimality; with
    `smFindSub_spec_unique` this pins a single result. -/
theorem smFindSub_some_iff (hay needle : List Int) (i : Nat) :
    smFindSub hay needle = some i ↔
      smIsPrefix needle (hay.drop i) = true ∧
        ∀ j, j < i → smIsPrefix needle (hay.drop j) = false := by
  induction hay generalizing i with
  | nil =>
      constructor
      · intro h
        by_cases hp : smIsPrefix needle [] = true
        · simp only [smFindSub, if_pos hp] at h
          injection h with h
          subst h
          rw [List.drop_nil]
          exact ⟨hp, fun j hj => absurd hj (Nat.not_lt_zero j)⟩
        · simp only [smFindSub, if_neg hp] at h
          simp at h
      · intro hcon
        obtain ⟨hpre, hmin⟩ := hcon
        rw [List.drop_nil] at hpre
        cases i with
        | zero => simp only [smFindSub, if_pos hpre]
        | succ i' =>
            have := hmin 0 (Nat.succ_pos i')
            rw [List.drop_nil, hpre] at this
            simp at this
  | cons d rest ih =>
      by_cases hp : smIsPrefix needle (d :: rest) = true
      · constructor
        · intro h
          simp only [smFindSub, if_pos hp] at h
          injection h with h
          subst h
          rw [List.drop_zero]
          exact ⟨hp, fun j hj => absurd hj (Nat.not_lt_zero j)⟩
        · intro hcon
          obtain ⟨hpre, hmin⟩ := hcon
          cases i with
          | zero => simp only [smFindSub, if_pos hp]
          | succ i' =>
              have := hmin 0 (Nat.succ_pos i')
              rw [List.drop_zero, hp] at this
              simp at this
      · have hpf : smIsPrefix needle (d :: rest) = false := smBoolFalse hp
        constructor
        · intro h
          simp only [smFindSub, if_neg hp] at h
          cases hr : smFindSub rest needle with
          | none => rw [hr] at h; simp at h
          | some j =>
              rw [hr] at h
              simp only [Option.map_some] at h
              injection h with h
              subst h
              obtain ⟨hpre, hmin⟩ := (ih j).mp hr
              refine ⟨by rw [List.drop_succ_cons]; exact hpre, ?_⟩
              intro k hk
              cases k with
              | zero => rw [List.drop_zero]; exact hpf
              | succ k' =>
                  rw [List.drop_succ_cons]
                  exact hmin k' (by omega)
        · intro hcon
          obtain ⟨hpre, hmin⟩ := hcon
          cases i with
          | zero =>
              rw [List.drop_zero] at hpre
              exact absurd hpre hp
          | succ i' =>
              rw [List.drop_succ_cons] at hpre
              have hrest : smFindSub rest needle = some i' := by
                rw [ih i']
                refine ⟨hpre, fun j hj => ?_⟩
                have := hmin (j + 1) (by omega)
                rwa [List.drop_succ_cons] at this
              simp only [smFindSub, if_neg hp, hrest, Option.map_some]

/-- PROPOSED (human review): absence characterization — `none` iff `needle`
    occurs at NO offset. -/
theorem smFindSub_none_iff (hay needle : List Int) :
    smFindSub hay needle = none ↔
      ∀ j, smIsPrefix needle (hay.drop j) = false := by
  induction hay with
  | nil =>
      constructor
      · intro h j
        by_cases hp : smIsPrefix needle [] = true
        · simp only [smFindSub, if_pos hp] at h
          simp at h
        · rw [List.drop_nil]
          exact smBoolFalse hp
      · intro h
        have h0 := h 0
        rw [List.drop_nil] at h0
        simp [smFindSub, h0]
  | cons d rest ih =>
      by_cases hp : smIsPrefix needle (d :: rest) = true
      · constructor
        · intro h
          simp only [smFindSub, if_pos hp] at h
          simp at h
        · intro h
          have h0 := h 0
          rw [List.drop_zero, hp] at h0
          simp at h0
      · have hpf : smIsPrefix needle (d :: rest) = false := smBoolFalse hp
        constructor
        · intro h j
          simp only [smFindSub, if_neg hp] at h
          cases hr : smFindSub rest needle with
          | some k => rw [hr] at h; simp at h
          | none =>
              cases j with
              | zero => rw [List.drop_zero]; exact hpf
              | succ j' =>
                  rw [List.drop_succ_cons]
                  exact (ih.mp hr) j'
        · intro h
          have hrest : smFindSub rest needle = none := by
            rw [ih]
            intro j
            have := h (j + 1)
            rwa [List.drop_succ_cons] at this
          simp [smFindSub, if_neg hp, hrest]

/-- PROPOSED (human review): output-domain constraint (the #1 spec-bug
    archetype): a found offset is in range. -/
theorem smFindSub_le_length (hay needle : List Int) (i : Nat)
    (h : smFindSub hay needle = some i) : i ≤ hay.length := by
  obtain ⟨hpre, hmin⟩ := (smFindSub_some_iff hay needle i).mp h
  rcases Nat.lt_or_ge hay.length i with hgt | hge
  · exfalso
    have hdrop : hay.drop i = [] := List.drop_eq_nil_of_le (by omega)
    rw [hdrop] at hpre
    cases hn : needle with
    | nil =>
        subst hn
        have := hmin 0 (by omega)
        simp [smIsPrefix] at this
    | cons p ps =>
        subst hn
        simp [smIsPrefix] at hpre
  · omega

/-- PROPOSED (human review): uniqueness — the first-occurrence spec admits at
    most ONE offset (the anti-vacuity guard of the LeetProof triple). -/
theorem smFindSub_spec_unique (hay needle : List Int) (i i' : Nat)
    (hi : smIsPrefix needle (hay.drop i) = true ∧
      ∀ j, j < i → smIsPrefix needle (hay.drop j) = false)
    (hi' : smIsPrefix needle (hay.drop i') = true ∧
      ∀ j, j < i' → smIsPrefix needle (hay.drop j) = false) : i = i' := by
  rcases Nat.lt_trichotomy i i' with h | h | h
  · have := hi'.2 i h
    rw [hi.1] at this
    simp at this
  · exact h
  · have := hi.2 i' h
    rw [hi'.1] at this
    simp at this

/-- Membership spec: `t in s` iff `t` occurs at SOME offset. -/
theorem smContains_iff (hay needle : List Int) :
    smContains hay needle = true ↔
      ∃ j, smIsPrefix needle (hay.drop j) = true := by
  unfold smContains
  cases hr : smFindSub hay needle with
  | none =>
      constructor
      · intro hcontra
        rw [Option.isSome_none] at hcontra
        simp at hcontra
      · intro hex
        obtain ⟨j, hj⟩ := hex
        have := (smFindSub_none_iff hay needle).mp hr j
        rw [this] at hj
        simp at hj
  | some k =>
      constructor
      · intro _
        exact ⟨k, ((smFindSub_some_iff hay needle k).mp hr).1⟩
      · intro _
        rfl

/-! ### The naive-JS deviation model (substring `.indexOf`) -/

/-- What naive JS `.indexOf` returns for a substring needle: the UTF-16
    CODE-UNIT offset — the Python match position re-weighted by `utf16Units`. -/
def js16FindSub : List Int → List Int → Option Nat
  | [], needle => if smIsPrefix needle [] then some 0 else none
  | d :: rest, needle =>
      if smIsPrefix needle (d :: rest) then some 0
      else (js16FindSub rest needle).map (· + utf16Units d)

/-- PROPOSED (human review): the exact deviation law, wave-18's
    single-code-point law generalized to substring needles. -/
theorem js16FindSub_eq_smFindSub_add_astral (hay needle : List Int) (i : Nat)
    (h : smFindSub hay needle = some i) :
    js16FindSub hay needle = some (i + smAstralCount (hay.take i)) := by
  induction hay generalizing i with
  | nil =>
      by_cases hp : smIsPrefix needle [] = true
      · simp only [smFindSub, if_pos hp] at h
        injection h with h
        subst h
        simp [js16FindSub, if_pos hp, smAstralCount]
      · simp only [smFindSub, if_neg hp] at h
        simp at h
  | cons d rest ih =>
      by_cases hp : smIsPrefix needle (d :: rest) = true
      · simp only [smFindSub, if_pos hp] at h
        injection h with h
        subst h
        simp [js16FindSub, if_pos hp, smAstralCount]
      · simp only [smFindSub, if_neg hp] at h
        cases hr : smFindSub rest needle with
        | none => rw [hr] at h; simp at h
        | some j =>
            rw [hr] at h
            simp only [Option.map_some] at h
            injection h with h
            subst h
            have hrec := ih j hr
            simp only [js16FindSub, if_neg hp, hrec, Option.map_some,
                       List.take_succ_cons, smAstralCount_cons, utf16Units]
            by_cases hda : (0x10000 : Int) ≤ d
            · simp only [if_pos hda]; congr 1; omega
            · simp only [if_neg hda]; congr 1; omega

/-- PROPOSED (human review): the GENERAL impossibility on the substring
    surface — an astral code point before the match makes naive-JS `.indexOf`
    provably differ from Python `.find`/`.index`. -/
theorem smFindSub_ne_js16_astral (hay needle : List Int) (i : Nat)
    (h : smFindSub hay needle = some i) (a : Int) (ha : a ∈ hay.take i)
    (hastral : 0x10000 ≤ a) : js16FindSub hay needle ≠ some i := by
  rw [js16FindSub_eq_smFindSub_add_astral hay needle i h]
  intro hcontra
  injection hcontra with hcontra
  have hpos := smAstralCount_pos (hay.take i) a ha hastral
  omega

/-- PROPOSED (human review): agreement boundary — on all-BMP strings the two
    offsets coincide (the deviation is specifically astral-prefixed). -/
theorem smFindSub_eq_js16_bmp (hay needle : List Int)
    (hbmp : ∀ a ∈ hay, a < 0x10000) :
    js16FindSub hay needle = smFindSub hay needle := by
  induction hay with
  | nil => rfl
  | cons d rest ih =>
      have hd16 : ¬ (0x10000 : Int) ≤ d := by
        have := hbmp d (List.mem_cons_self ..)
        omega
      by_cases hp : smIsPrefix needle (d :: rest) = true
      · simp only [js16FindSub, smFindSub, if_pos hp]
      · have ih' := ih (fun a ha => hbmp a (List.mem_cons_of_mem d ha))
        simp only [js16FindSub, smFindSub, if_neg hp, ih', utf16Units,
                   if_neg hd16]

/-! ### count (non-overlapping) -/

/-- Left-to-right NON-OVERLAPPING scan (CPython `str.count`): after a match,
    the next `needle.length - 1` positions are inside the counted occurrence
    and are skipped (the `skip` counter keeps the recursion structural). -/
def smCountAux (needle : List Int) : List Int → Nat → Nat
  | [], _ => 0
  | _ :: rest, skip + 1 => smCountAux needle rest skip
  | d :: rest, 0 =>
      if smIsPrefix needle (d :: rest) then
        1 + smCountAux needle rest (needle.length - 1)
      else smCountAux needle rest 0

/-- Python `s.count(t)`: non-overlapping occurrences; empty needle counts
    every gap (`len(s) + 1`). -/
def smCount (hay needle : List Int) : Nat :=
  if needle = [] then hay.length + 1 else smCountAux needle hay 0

/-- PROPOSED (human review): positivity ↔ membership:
    `s.count(t) > 0 ↔ t in s` (for nonempty `t`; empty `t` is the
    `len + 1` arm by definition). -/
theorem smCount_pos_iff (hay needle : List Int) (hne : needle ≠ []) :
    0 < smCount hay needle ↔ smContains hay needle = true := by
  unfold smCount
  rw [if_neg hne]
  induction hay with
  | nil =>
      simp only [smCountAux]
      constructor
      · intro h; omega
      · intro h
        have hp : smIsPrefix needle [] = false := by
          cases needle with
          | nil => exact absurd rfl hne
          | cons _ _ => rfl
        simp [smContains, smFindSub, hp] at h
  | cons d rest ih =>
      by_cases hp : smIsPrefix needle (d :: rest) = true
      · simp only [smCountAux, if_pos hp]
        constructor
        · intro _
          simp [smContains, smFindSub, if_pos hp]
        · intro _
          omega
      · simp only [smCountAux, if_neg hp]
        rw [ih]
        unfold smContains
        have hstep : smFindSub (d :: rest) needle
            = (smFindSub rest needle).map (· + 1) := by
          simp only [smFindSub, if_neg hp]
        rw [hstep]
        cases smFindSub rest needle <;> simp

/-! ### replace -/

/-- Python `s.replace('', new)`: `new` inserted at every gap. -/
def smReplaceEmpty (repl : List Int) : List Int → List Int
  | [] => repl
  | d :: rest => repl ++ d :: smReplaceEmpty repl rest

/-- Left-to-right non-overlapping replacement (same `skip` discipline as
    `smCountAux` — the two functions walk the SAME match structure). -/
def smReplaceAux (needle repl : List Int) : List Int → Nat → List Int
  | [], _ => []
  | _ :: rest, skip + 1 => smReplaceAux needle repl rest skip
  | d :: rest, 0 =>
      if smIsPrefix needle (d :: rest) then
        repl ++ smReplaceAux needle repl rest (needle.length - 1)
      else d :: smReplaceAux needle repl rest 0

/-- Python `s.replace(old, new)` (full replacement, no count arg). -/
def smReplace (hay needle repl : List Int) : List Int :=
  if needle = [] then smReplaceEmpty repl hay
  else smReplaceAux needle repl hay 0

/-- PROPOSED (human review): empty-needle arm of the CPython length law —
    `len(s.replace('', new)) = len(new) * (len(s) + 1) + len(s)`. -/
theorem smReplaceEmpty_length (repl hay : List Int) :
    (smReplaceEmpty repl hay).length
      = repl.length * (hay.length + 1) + hay.length := by
  induction hay with
  | nil => simp [smReplaceEmpty]
  | cons d rest ih =>
      simp only [smReplaceEmpty, List.length_append, List.length_cons, ih]
      have hd : repl.length * (rest.length + 1 + 1)
          = repl.length * (rest.length + 1) + repl.length := by
        rw [Nat.mul_add, Nat.mul_one]
      rw [hd]
      omega

/-- The joint length invariant of the replace/count scan. -/
theorem smReplaceAux_length (needle repl : List Int) (hne : needle ≠ []) :
    ∀ (hay : List Int) (skip : Nat), skip ≤ hay.length →
    ((smReplaceAux needle repl hay skip).length : Int)
      = (hay.length : Int) - skip
        + ((repl.length : Int) - needle.length)
            * smCountAux needle hay skip := by
  intro hay
  induction hay with
  | nil =>
      intro skip hskip
      simp only [List.length_nil] at hskip
      have h0 : skip = 0 := Nat.le_zero.mp hskip
      subst h0
      simp [smReplaceAux, smCountAux]
  | cons d rest ih =>
      intro skip hskip
      cases skip with
      | succ skip' =>
          simp only [smReplaceAux, smCountAux]
          have hle : skip' ≤ rest.length := by
            simp only [List.length_cons] at hskip
            omega
          rw [ih skip' hle]
          simp only [List.length_cons]
          omega
      | zero =>
          by_cases hp : smIsPrefix needle (d :: rest) = true
          · simp only [smReplaceAux, smCountAux, if_pos hp]
            have hlen := smIsPrefix_length_le needle (d :: rest) hp
            simp only [List.length_cons] at hlen
            have hpos : 1 ≤ needle.length := by
              cases needle with
              | nil => exact absurd rfl hne
              | cons _ _ => simp
            have hle : needle.length - 1 ≤ rest.length := by omega
            have hrec := ih (needle.length - 1) hle
            rw [List.length_append, Int.natCast_add, hrec,
                Int.natCast_add, Int.natCast_one, Int.mul_add, Int.mul_one]
            simp only [List.length_cons]
            omega
          · simp only [smReplaceAux, smCountAux, if_neg hp,
                       List.length_cons]
            rw [Int.natCast_add, ih 0 (Nat.zero_le _)]
            omega

/-- PROPOSED (human review): the QUANTITATIVE spec tying `replace` to
    `count` — the CPython law
    `len(s.replace(old, new)) = len(s) + (len(new) - len(old)) * s.count(old)`
    (nonempty needle; the empty-needle arm is `smReplaceEmpty_length`). -/
theorem smReplace_length (hay needle repl : List Int) (hne : needle ≠ []) :
    ((smReplace hay needle repl).length : Int)
      = (hay.length : Int)
        + ((repl.length : Int) - needle.length) * smCount hay needle := by
  unfold smReplace smCount
  rw [if_neg hne, if_neg hne,
      smReplaceAux_length needle repl hne hay 0 (Nat.zero_le _)]
  omega

/-- Replacing a needle by ITSELF reassembles the string exactly. -/
theorem smReplaceAux_self (needle : List Int) (hne : needle ≠ []) :
    ∀ (hay : List Int) (skip : Nat),
      smReplaceAux needle needle hay skip = hay.drop skip := by
  intro hay
  induction hay with
  | nil => intro skip; cases skip <;> rfl
  | cons d rest ih =>
      intro skip
      cases skip with
      | succ skip' =>
          simp only [smReplaceAux, List.drop_succ_cons]
          exact ih skip'
      | zero =>
          by_cases hp : smIsPrefix needle (d :: rest) = true
          · obtain ⟨p, ps, rfl⟩ : ∃ p ps, needle = p :: ps := by
              cases needle with
              | nil => exact absurd rfl hne
              | cons p ps => exact ⟨p, ps, rfl⟩
            simp only [smReplaceAux, if_pos hp, List.drop_zero,
                       List.length_cons, Nat.add_sub_cancel]
            rw [ih ps.length]
            have ht := smIsPrefix_drop (p :: ps) (d :: rest) hp
            simpa [List.length_cons, List.drop_succ_cons] using ht
          · simp only [smReplaceAux, if_neg hp, List.drop_zero]
            rw [ih 0, List.drop_zero]

theorem smReplaceEmpty_nil_repl (hay : List Int) :
    smReplaceEmpty [] hay = hay := by
  induction hay with
  | nil => rfl
  | cons d rest ih => simp [smReplaceEmpty, ih]

/-- PROPOSED (human review): identity law — CPython `s.replace(t, t) == s`. -/
theorem smReplace_self (hay needle : List Int) :
    smReplace hay needle needle = hay := by
  unfold smReplace
  by_cases hn : needle = []
  · subst hn
    rw [if_pos rfl, smReplaceEmpty_nil_repl]
  · rw [if_neg hn, smReplaceAux_self needle hn hay 0, List.drop_zero]

/-- PROPOSED (human review): no occurrence → identity
    (CPython: `t not in s → s.replace(t, u) == s`). -/
theorem smReplace_of_not_contains (hay needle repl : List Int)
    (h : smContains hay needle = false) : smReplace hay needle repl = hay := by
  have hne : needle ≠ [] := by
    intro hn
    subst hn
    cases hay with
    | nil => simp [smContains, smFindSub, smIsPrefix] at h
    | cons d rest => simp [smContains, smFindSub, smIsPrefix] at h
  unfold smReplace
  rw [if_neg hne]
  induction hay with
  | nil => rfl
  | cons d rest ih =>
      by_cases hb : smIsPrefix needle (d :: rest) = true
      · simp [smContains, smFindSub, if_pos hb] at h
      · have hrest : smContains rest needle = false := by
          unfold smContains at h ⊢
          have hstep : smFindSub (d :: rest) needle
              = (smFindSub rest needle).map (· + 1) := by
            simp only [smFindSub, if_neg hb]
          rw [hstep] at h
          cases hfr : smFindSub rest needle with
          | none => rfl
          | some k => rw [hfr] at h; simp at h
        simp only [smReplaceAux, if_neg hb]
        rw [ih hrest]

/-! ### strip family (explicit chars) -/

/-- Bool membership in the strip set (self-contained decision procedure;
    `smMem_iff` links it to `∈`). -/
def smMem (c : Int) : List Int → Bool
  | [] => false
  | d :: rest => if c = d then true else smMem c rest

/-- PROPOSED (human review): `smMem` decides list membership. -/
theorem smMem_iff (c : Int) (chars : List Int) :
    smMem c chars = true ↔ c ∈ chars := by
  induction chars with
  | nil => simp [smMem]
  | cons d rest ih =>
      by_cases hcd : c = d
      · subst hcd
        simp [smMem]
      · simp only [smMem, if_neg hcd, ih]
        constructor
        · intro h
          exact List.mem_cons_of_mem d h
        · intro h
          rcases List.mem_cons.mp h with h1 | h2
          · exact absurd h1 hcd
          · exact h2

/-- Python `s.lstrip(chars)` (EXPLICIT chars): drop leading code points that
    are in the strip set. -/
def smLstrip (chars hay : List Int) : List Int :=
  hay.dropWhile (fun c => smMem c chars)

/-- Python `s.rstrip(chars)`. -/
def smRstrip (chars hay : List Int) : List Int :=
  (smLstrip chars hay.reverse).reverse

/-- Python `s.strip(chars)`. -/
def smStrip (chars hay : List Int) : List Int :=
  smRstrip chars (smLstrip chars hay)

/-- PROPOSED (human review): `lstrip` characterization — the result is the
    suffix left after removing a prefix of strippable chars, and it does not
    start with a strippable char. -/
theorem smLstrip_spec (chars hay : List Int) :
    ∃ pre, pre ++ smLstrip chars hay = hay
      ∧ (∀ c ∈ pre, smMem c chars = true)
      ∧ (∀ d, (smLstrip chars hay).head? = some d →
            smMem d chars = false) := by
  induction hay with
  | nil =>
      exact ⟨[], rfl, fun c hc => absurd hc List.not_mem_nil,
             fun d hd => by simp [smLstrip] at hd⟩
  | cons a rest ih =>
      by_cases hmem : smMem a chars = true
      · have hstep : smLstrip chars (a :: rest) = smLstrip chars rest := by
          simp only [smLstrip, List.dropWhile_cons]
          rw [if_pos hmem]
        obtain ⟨pre, hdec, hpre, hhead⟩ := ih
        refine ⟨a :: pre, ?_, ?_, ?_⟩
        · rw [List.cons_append, hstep, hdec]
        · intro c hc
          rcases List.mem_cons.mp hc with h | h
          · subst h; exact hmem
          · exact hpre c h
        · intro d hd
          rw [hstep] at hd
          exact hhead d hd
      · have hmemf : smMem a chars = false := smBoolFalse hmem
        have hstep : smLstrip chars (a :: rest) = a :: rest := by
          simp only [smLstrip, List.dropWhile_cons]
          rw [if_neg hmem]
        refine ⟨[], by rw [List.nil_append, hstep],
                fun c hc => absurd hc List.not_mem_nil, ?_⟩
        intro d hd
        rw [hstep, List.head?_cons] at hd
        injection hd with hd
        subst hd
        exact hmemf

/-- PROPOSED (human review): uniqueness — ANY decomposition into an
    all-strippable prefix and a suffix not starting with a strippable char
    IS `lstrip` (the postcondition pins a single result). -/
theorem smLstrip_unique (chars hay pre suf : List Int)
    (hdecomp : pre ++ suf = hay)
    (hpre : ∀ c ∈ pre, smMem c chars = true)
    (hsuf : ∀ d, suf.head? = some d → smMem d chars = false) :
    suf = smLstrip chars hay := by
  induction pre generalizing hay with
  | nil =>
      simp only [List.nil_append] at hdecomp
      subst hdecomp
      cases suf with
      | nil => rfl
      | cons d rest =>
          have hd := hsuf d rfl
          simp only [smLstrip, List.dropWhile_cons]
          rw [if_neg (by simp [hd])]
  | cons c pre' ih =>
      rw [List.cons_append] at hdecomp
      subst hdecomp
      have hc := hpre c (List.mem_cons_self ..)
      have hstep : smLstrip chars (c :: (pre' ++ suf))
          = smLstrip chars (pre' ++ suf) := by
        simp only [smLstrip, List.dropWhile_cons]
        rw [if_pos hc]
      rw [hstep]
      exact ih (pre' ++ suf) rfl
        (fun x hx => hpre x (List.mem_cons_of_mem c hx))

/-! ### split / join -/

/-- Python `sep.join(pieces)`. -/
def smJoin (sep : List Int) : List (List Int) → List Int
  | [] => []
  | p :: ps =>
      match ps with
      | [] => p
      | _ :: _ => p ++ sep ++ smJoin sep ps

theorem smJoin_singleton (sep p : List Int) : smJoin sep [p] = p := rfl

theorem smJoin_cons_cons (sep p q : List Int) (qs : List (List Int)) :
    smJoin sep (p :: q :: qs) = p ++ sep ++ smJoin sep (q :: qs) := rfl

/-- Split scan (nonempty `sep`), same `skip` discipline as count/replace:
    a match closes the current piece and opens a new one. -/
def smSplitAux (sep : List Int) : List Int → Nat → List (List Int)
  | [], _ => [[]]
  | _ :: rest, skip + 1 => smSplitAux sep rest skip
  | d :: rest, 0 =>
      if smIsPrefix sep (d :: rest) then
        [] :: smSplitAux sep rest (sep.length - 1)
      else
        match smSplitAux sep rest 0 with
        | piece :: pieces => (d :: piece) :: pieces
        | [] => [[d]]

/-- Python `s.split(sep)`: explicit separator; `''` → ValueError (`none`). -/
def smSplit (hay sep : List Int) : Option (List (List Int)) :=
  if sep = [] then none else some (smSplitAux sep hay 0)

theorem smSplitAux_ne_nil (sep hay : List Int) (skip : Nat) :
    smSplitAux sep hay skip ≠ [] := by
  induction hay generalizing skip with
  | nil => simp [smSplitAux]
  | cons d rest ih =>
      cases skip with
      | succ skip' => simpa [smSplitAux] using ih skip'
      | zero =>
          by_cases hp : smIsPrefix sep (d :: rest) = true
          · simp [smSplitAux, if_pos hp]
          · simp only [smSplitAux, if_neg hp]
            cases hr : smSplitAux sep rest 0 with
            | nil => simp
            | cons piece pieces => simp

/-- The join∘split scan invariant. -/
theorem smJoin_smSplitAux (sep : List Int) (hsep : sep ≠ []) :
    ∀ (hay : List Int) (skip : Nat),
      smJoin sep (smSplitAux sep hay skip) = hay.drop skip := by
  intro hay
  induction hay with
  | nil => intro skip; cases skip <;> rfl
  | cons d rest ih =>
      intro skip
      cases skip with
      | succ skip' =>
          simp only [smSplitAux, List.drop_succ_cons]
          exact ih skip'
      | zero =>
          by_cases hp : smIsPrefix sep (d :: rest) = true
          · obtain ⟨p, ps, rfl⟩ : ∃ p ps, sep = p :: ps := by
              cases sep with
              | nil => exact absurd rfl hsep
              | cons p ps => exact ⟨p, ps, rfl⟩
            simp only [smSplitAux, if_pos hp, List.drop_zero,
                       List.length_cons, Nat.add_sub_cancel]
            cases hr : smSplitAux (p :: ps) rest ps.length with
            | nil => exact absurd hr (smSplitAux_ne_nil (p :: ps) rest ps.length)
            | cons piece pieces =>
                rw [smJoin_cons_cons]
                have hj : smJoin (p :: ps) (piece :: pieces)
                    = rest.drop ps.length := by
                  rw [← hr]
                  exact ih ps.length
                rw [List.nil_append, hj]
                have ht := smIsPrefix_drop (p :: ps) (d :: rest) hp
                simpa [List.length_cons, List.drop_succ_cons] using ht
          · simp only [smSplitAux, if_neg hp, List.drop_zero]
            cases hr : smSplitAux sep rest 0 with
            | nil => exact absurd hr (smSplitAux_ne_nil sep rest 0)
            | cons piece pieces =>
                have hj : smJoin sep (piece :: pieces) = rest := by
                  have := ih 0
                  rw [hr, List.drop_zero] at this
                  exact this
                cases pieces with
                | nil =>
                    rw [smJoin_singleton] at hj
                    rw [smJoin_singleton, hj]
                | cons q qs =>
                    rw [smJoin_cons_cons] at hj
                    rw [smJoin_cons_cons, List.cons_append,
                        List.cons_append, hj]

/-- PROPOSED (human review): THE CPython split/join round-trip law —
    `sep.join(s.split(sep)) == s` whenever `split` succeeds (nonempty sep). -/
theorem smSplit_join (hay sep : List Int) (pieces : List (List Int))
    (h : smSplit hay sep = some pieces) : smJoin sep pieces = hay := by
  unfold smSplit at h
  by_cases hs : sep = []
  · rw [if_pos hs] at h
    simp at h
  · rw [if_neg hs] at h
    injection h with h
    subst h
    have := smJoin_smSplitAux sep hs hay 0
    rwa [List.drop_zero] at this

/-- PROPOSED (human review): `rstrip` characterization (mirror of
    `smLstrip_spec` through `reverse`): the input is the result plus an
    all-strippable suffix. -/
theorem smRstrip_spec (chars hay : List Int) :
    ∃ suf, smRstrip chars hay ++ suf = hay
      ∧ (∀ c ∈ suf, smMem c chars = true) := by
  obtain ⟨pre, hdec, hpre, -⟩ := smLstrip_spec chars hay.reverse
  refine ⟨pre.reverse, ?_, fun c hc => hpre c (List.mem_reverse.mp hc)⟩
  unfold smRstrip
  have := congrArg List.reverse hdec
  rwa [List.reverse_append, List.reverse_reverse] at this

-- executable bindings pinned to CPython string-method semantics (each case
-- checked against real CPython by verification/spec_validate_strmethods.py;
-- code points: a=97 b=98 c=99 h=104 i=105 x=120 z=122 ','=44 '-'=45,
-- 𝔸=U+1D538 and 💩=U+1F4A9 are astral):
-- "abc".startswith("ab") / ("") True; "abc".startswith("bc") False; astral OK.
#guard smStartsWith [97, 98, 99] [97, 98] = true
#guard smStartsWith [97, 98, 99] [] = true
#guard smStartsWith [97, 98, 99] [98, 99] = false
#guard smStartsWith [0x1D538, 120] [0x1D538] = true
-- "abc".endswith("bc") / ("") True; "𝔸x".endswith("x") True.
#guard smEndsWith [97, 98, 99] [98, 99] = true
#guard smEndsWith [97, 98, 99] [] = true
#guard smEndsWith [0x1D538, 120] [120] = true
#guard smEndsWith [97, 98, 99] [97, 98] = false
-- "b" in "abc"; "" in ""; "d" not in "abc".
#guard smContains [97, 98, 99] [98] = true
#guard smContains [] [] = true
#guard smContains [97, 98, 99] [100] = false
-- "𝔸abc".find("bc") == 2 (CODE POINTS — naive JS .indexOf === 3, the
-- SHIPPING pyFind bug this wave's differential caught); "abc".find("") == 0;
-- "abcbc".find("bc") == 1 (FIRST occurrence); "abc".find("d") == -1
-- (ValueError → none on the .index arm).
#guard smFindSub [0x1D538, 97, 98, 99] [98, 99] = some 2
#guard js16FindSub [0x1D538, 97, 98, 99] [98, 99] = some 3
#guard smFindSub [97, 98, 99] [] = some 0
#guard smFindSub [97, 98, 99, 98, 99] [98, 99] = some 1
#guard smFindSub [97, 98, 99] [100] = none
#guard smFindSubI [97, 98, 99] [100] = -1
-- "ababab".count("ab") == 3; "aaa".count("aa") == 1 (NON-overlapping);
-- "abc".count("") == 4; "𝔸".count("") == 2 (gaps in CODE POINTS); "".count("") == 1.
#guard smCount [97, 98, 97, 98, 97, 98] [97, 98] = 3
#guard smCount [97, 97, 97] [97, 97] = 1
#guard smCount [97, 98, 99] [] = 4
#guard smCount [0x1D538] [] = 2
#guard smCount [] [] = 1
#guard smCount [97, 98, 99] [100] = 0
-- "a💩b".replace("💩", "z") == "azb"; "aaa".replace("aa", "z") == "za"
-- (non-overlapping, left-to-right); "abc".replace("", "-") == "-a-b-c-";
-- "abab".replace("ab", "") == "".
#guard smReplace [97, 0x1F4A9, 98] [0x1F4A9] [122] = [97, 122, 98]
#guard smReplace [97, 97, 97] [97, 97] [122] = [122, 97]
#guard smReplace [97, 98, 99] [] [45] = [45, 97, 45, 98, 45, 99, 45]
-- "𝔸".replace("", "-") == "-𝔸-" — the ASTRAL × EMPTY-NEEDLE combination:
-- insertion between CODE POINTS, never between surrogate halves (wave-15
-- iter2: the shipped pyStrReplace used UTF-16 s.split("") here and split
-- the pair; the model was already correct — pin the combination).
#guard smReplace [0x1D538] [] [45] = [45, 0x1D538, 45]
#guard smReplace [97, 98, 97, 98] [97, 98] [] = []
-- "xxhixx".strip("x") == "hi"; "𝔸a𝔸".strip("𝔸") == "a" (the astral
-- strip-set SHIPPING bug this wave's differential caught); lstrip/rstrip
-- one-sided; chars absent → identity.
#guard smStrip [120] [120, 120, 104, 105, 120, 120] = [104, 105]
#guard smStrip [0x1D538] [0x1D538, 97, 0x1D538] = [97]
#guard smLstrip [120] [120, 104, 105, 120] = [104, 105, 120]
#guard smRstrip [120] [120, 104, 105, 120] = [120, 104, 105]
#guard smStrip [122] [104, 105] = [104, 105]
-- "a,b,,c".split(",") == ["a","b","","c"]; ",".split(",") == ["",""];
-- "".split(",") == [""]; "abab".split("ab") == ["","",""];
-- "".split("") raises ValueError → none.
#guard smSplit [97, 44, 98, 44, 44, 99] [44] = some [[97], [98], [], [99]]
#guard smSplit [44] [44] = some [[], []]
#guard smSplit [] [44] = some [[]]
#guard smSplit [97, 98, 97, 98] [97, 98] = some [[], [], []]
#guard smSplit [97] [] = none
-- ",".join(["a","b"]) == "a,b"; "".join(["a","b"]) == "ab" (empty sep is
-- LEGAL for join, unlike split).
#guard smJoin [44] [[97], [98]] = [97, 44, 98]
#guard smJoin [] [[97], [98]] = [97, 98]
#guard smJoin [44] [] = []

/-! ### The method expression fragment and its two semantics -/



/-- Result values of the general-method fragment: methods return bools
    (`startswith`/`endswith`/`in`), ints (`find`/`index`/`count`), strings
    (`replace`/`strip` family/`join`), or lists of strings (`split`). -/
inductive SgVal where
  | gbool (b : Bool)
  | gint (n : Int)
  | gstr (cps : List Int)
  | glist (ps : List (List Int))
deriving Repr, DecidableEq

def SgVal.asBool : SgVal → Option Bool
  | .gbool b => some b | _ => none

def SgVal.asInt : SgVal → Option Int
  | .gint n => some n | _ => none

def SgVal.asCps : SgVal → Option (List Int)
  | .gstr cps => some cps | _ => none

def SgVal.asList : SgVal → Option (List (List Int))
  | .glist ps => some ps | _ => none

/-- CPython numeric coercion for the `//` surface — the BOOL-INT IDENTITY
    (`bool` is a subtype of `int`: `True == 1`, `False == 0`, so
    `"ab".startswith("a") // 2 == 0` and `1 // ("d" in "abc")` raises
    ZeroDivisionError). Strings and lists are NOT numbers (`TypeError` →
    `none`). Handling EVERY `SgVal` constructor here (F9
    domain-completeness over the sum type) is what makes the `//` arm
    faithful on the gbool constructor instead of a shared-wrong `none`
    (the wave-5-iter1 defect shape). Emitted JS agrees: ToNumber coerces
    `true`/`false` to `1`/`0` in arithmetic. -/
def SgVal.asArith : SgVal → Option Int
  | .gint n => some n
  | .gbool b => some (if b then 1 else 0)
  | .gstr _ => none
  | .glist _ => none

/-- The general string-method fragment. Receivers are wave-18 `SmStr`
    operands (literal or wave-11 env variable); method arguments are
    literals; `fdivNode` threads the seed `//` deviation through returned
    ints exactly as in waves 5–18. -/
inductive SgExp where
  | ilit (n : Int)
  | startswithNode (s : SmStr) (pre : List Int)
  | endswithNode (s : SmStr) (suf : List Int)
  | containsNode (s : SmStr) (needle : List Int)
  | findNode (s : SmStr) (needle : List Int)     -- s.find(t): total, -1
  | indexNode (s : SmStr) (needle : List Int)    -- s.index(t): ValueError → none
  | countNode (s : SmStr) (needle : List Int)
  | replaceNode (s : SmStr) (old new : List Int)
  | lstripNode (s : SmStr) (chars : List Int)
  | rstripNode (s : SmStr) (chars : List Int)
  | stripNode (s : SmStr) (chars : List Int)
  | splitNode (s : SmStr) (sep : List Int)       -- empty sep: ValueError → none
  | joinNode (sep : SmStr) (pieces : List (List Int))
  | fdivNode (a b : SgExp)
deriving Repr

/-- Fragment REFERENCE eval (the Python side is `evalSg false`). Every method
    arm uses the CODE-POINT model (the compiler emits code-point helpers;
    `js16FindSub` above is what naive JS `.indexOf` WOULD compute, kept
    OUTSIDE the semantics exactly as waves 11/18 kept `utf16Len`/`js16Index`
    outside). The `//` arm coerces via `SgVal.asArith` — the CPython bool-int
    identity, every constructor handled (F9). The `tgt` flag is
    DOCUMENTED-LEGACY (the historical wave-19 `Bool`-flag copy, F1 shape):
    NO theorem references `evalSg true` — the compiled semantics is the
    INDEPENDENT `evalSgtgt` below (C1-rollout wave 15). -/
def evalSg (tgt : Bool) : SgExp → SEnv → Option SgVal
  | .ilit n, _ => some (.gint n)
  | .startswithNode s pre, env =>
      (s.eval env).map (fun h => .gbool (smStartsWith h pre))
  | .endswithNode s suf, env =>
      (s.eval env).map (fun h => .gbool (smEndsWith h suf))
  | .containsNode s t, env =>
      (s.eval env).map (fun h => .gbool (smContains h t))
  | .findNode s t, env =>
      (s.eval env).map (fun h => .gint (smFindSubI h t))
  | .indexNode s t, env =>
      (s.eval env).bind fun h => (smFindSub h t).map (fun i => .gint (i : Int))
  | .countNode s t, env =>
      (s.eval env).map (fun h => .gint (smCount h t))
  | .replaceNode s o n, env =>
      (s.eval env).map (fun h => .gstr (smReplace h o n))
  | .lstripNode s cs, env =>
      (s.eval env).map (fun h => .gstr (smLstrip cs h))
  | .rstripNode s cs, env =>
      (s.eval env).map (fun h => .gstr (smRstrip cs h))
  | .stripNode s cs, env =>
      (s.eval env).map (fun h => .gstr (smStrip cs h))
  | .splitNode s sep, env =>
      (s.eval env).bind fun h => (smSplit h sep).map .glist
  | .joinNode sep ps, env =>
      (sep.eval env).map (fun sp => .gstr (smJoin sp ps))
  | .fdivNode a b, env =>
      (evalSg tgt a env).bind fun va => (evalSg tgt b env).bind fun vb =>
        va.asArith.bind fun x => vb.asArith.bind fun y =>
          if y = 0 then none
          else some (.gint (if tgt then jsFdiv x y else Int.fdiv x y))

/-! ### Wave 15 (C1 rollout) — INDEPENDENT-target general-string-method
preservation

The previous `preservationSg : evalSg true e env = evalSg false e env` was the
F1 model-vs-model tautology: ONE evaluator with a `Bool` flag flipping only the
`//` arm — stubbing the shipping lowering could not break it. Re-architected on
the wave-1/13 recipe: `evalSgtgt` is a SEPARATE recursion, parameterized by the
integer-division lowering the emitted JS uses; the `//` on RETURNED VALUES
(`fdivNode` — offsets from `find`/`index`, counts from `count`, incl. the
`-1` miss sentinel and coerced bools) routes through `L`, while every method
arm keeps the CODE-POINT primitives (`smStartsWith`/`smEndsWith`/`smContains`/
`smFindSub`/`smFindSubI`/`smCount`/`smReplace`/`smLstrip`/`smRstrip`/`smStrip`/
`smSplit`/`smJoin`) on BOTH sides — those are exactly what the compiler emits,
and the UTF-16 deviation they absorb stays proved real by the UNTOUCHED
`smFindSub_ne_js16_astral`/`js16FindSub_*` witnesses. Every `SgVal`
constructor flows through the `//` arm faithfully on BOTH sides via
`SgVal.asArith` (gint passes, gbool coerces per the CPython bool-int
identity, gstr/glist are TypeError → `none`) — no shared-wrong `rfl` on a
non-int constructor (F9 domain-completeness). The SAME predicate
(`SgPreserves`) is proved for the shipped floor-correction
(`preservationSg_real`) and REFUTED for the naive truncating lowering
(`preservationSg_stub_fails`) on a witness where floor vs truncation give
DIFFERENT values of a `//`-composed method result.

SHIPPING-BINDING PAYOFF (iter2, NARROWED in iter3 — the wave-14 lesson
again): binding this CORRECT model to the shipped emitter exposed TWO REAL
SHIPPING BUGS (F9, runtime unfaithful to model + CPython), both fixed at
root in the runtime: (1) `pyFloorDiv`/`pyMod` (and the same slow-path gap
on `pySub`/`pyDiv`/`pyPow`) coerced non-numeric operands via
`Number()`/`BigInt()` — `"4".strip("x") // 2` compiled to `2` where
CPython and `SgVal.asArith` raise TypeError; now a numeric-operand guard
(int/float/bool only, bool coerced to its int value before arithmetic,
after dunder dispatch) in `runtime/src/operators.js` AND the `emit.rs`
`PY_ARITH_JS` inline mirror. `pyAdd`/`pyMul` are NOT guarded and string
`%`-formatting is honestly unsupported (NotImplementedError) — the
remaining arithmetic-operator type gaps are the C3/C4 workstream, NOT
claimed by this wave. (2) `pyStrReplace`'s empty-needle branch
iterated UTF-16 code units (`s.split("")`), splitting astral surrogate
pairs — `"𝔸".replace("", "-")` gave surrogate garbage where CPython and
`smReplace` (pinned by the `smReplace [0x1D538] [] [45]` guard above)
give `"-𝔸-"`; now code-point iteration (`[...s]`; a bool `count` is its
int value). Differential pins: the `w15_*` entries in
`tests/differential/cpython_corpus.json` (floordiv/mod str+list TypeError,
bool-int `//` coercion, exact bool+BigInt `+`/`-`/`**`, astral
empty-needle replace incl. count-capped/count-0/bool-count),
1,318/1,318. -/

/-- **Independent target evaluator** for the general-method fragment: the
    compiled program's semantics under lowering `L`. A SEPARATE recursion
    (not a `Bool` flag on `evalSg`); the `fdivNode` arm (`//` on returned
    method values) calls the lowering's operation, mirroring the emitted JS
    (whose ToNumber coerces booleans to `1`/`0`, matching `SgVal.asArith`),
    and the method arms use the same code-point primitives the compiler
    actually emits — never naive UTF-16 `.indexOf`/`.length`. -/
def evalSgtgt (L : IntDivLowering) : SgExp → SEnv → Option SgVal
  | .ilit n, _ => some (.gint n)
  | .startswithNode s pre, env =>
      (s.eval env).map (fun h => .gbool (smStartsWith h pre))
  | .endswithNode s suf, env =>
      (s.eval env).map (fun h => .gbool (smEndsWith h suf))
  | .containsNode s t, env =>
      (s.eval env).map (fun h => .gbool (smContains h t))
  | .findNode s t, env =>
      (s.eval env).map (fun h => .gint (smFindSubI h t))
  | .indexNode s t, env =>
      (s.eval env).bind fun h => (smFindSub h t).map (fun i => .gint (i : Int))
  | .countNode s t, env =>
      (s.eval env).map (fun h => .gint (smCount h t))
  | .replaceNode s o n, env =>
      (s.eval env).map (fun h => .gstr (smReplace h o n))
  | .lstripNode s cs, env =>
      (s.eval env).map (fun h => .gstr (smLstrip cs h))
  | .rstripNode s cs, env =>
      (s.eval env).map (fun h => .gstr (smRstrip cs h))
  | .stripNode s cs, env =>
      (s.eval env).map (fun h => .gstr (smStrip cs h))
  | .splitNode s sep, env =>
      (s.eval env).bind fun h => (smSplit h sep).map .glist
  | .joinNode sep ps, env =>
      (sep.eval env).map (fun sp => .gstr (smJoin sp ps))
  | .fdivNode a b, env =>
      (evalSgtgt L a env).bind fun va => (evalSgtgt L b env).bind fun vb =>
        va.asArith.bind fun x => vb.asArith.bind fun y =>
          if y = 0 then none else some (.gint (L.fdiv x y))

/-- The compiled general-method semantics: the independent target under the
    SHIPPED lowering. -/
abbrev evalSgjs : SgExp → SEnv → Option SgVal := evalSgtgt jsELowering

-- Executable bindings — F9 pins: the REFERENCE `evalSg false` matches CPython
-- (via the primitives' own pins above), and the compiled `evalSgjs` guards
-- mirror them (the retired `evalSg true` legacy guards now live on the
-- independent target).
-- "𝔸abc".find("bc") == 2 — code points, not the 3 naive UTF-16 units.
#guard (evalSg false (.findNode (.slit [0x1D538, 97, 98, 99]) [98, 99]) []).bind SgVal.asInt = some 2
#guard (evalSgjs (.findNode (.slit [0x1D538, 97, 98, 99]) [98, 99]) []).bind SgVal.asInt = some 2
-- "𝔸a𝔸".strip("𝔸") == "a" via an env-bound receiver.
#guard (evalSg false (.stripNode (.svar "s") [0x1D538])
    [("s", .sstr [0x1D538, 97, 0x1D538])]).bind SgVal.asCps = some [97]
#guard (evalSgjs (.stripNode (.svar "s") [0x1D538])
    [("s", .sstr [0x1D538, 97, 0x1D538])]).bind SgVal.asCps = some [97]
-- "a,b,,c".split(",") == ["a","b","","c"]; empty sep → ValueError.
#guard (evalSg false (.splitNode (.slit [97, 44, 98, 44, 44, 99]) [44]) []).bind SgVal.asList
    = some [[97], [98], [], [99]]
#guard (evalSgjs (.splitNode (.slit [97, 44, 98, 44, 44, 99]) [44]) []).bind SgVal.asList
    = some [[97], [98], [], [99]]
#guard (evalSg false (.splitNode (.slit [97]) []) []).isNone
#guard (evalSgjs (.splitNode (.slit [97]) []) []).isNone
-- "abc".index("d") raises ValueError → none; .find returns -1.
#guard (evalSg false (.indexNode (.slit [97, 98, 99]) [100]) []).isNone
#guard (evalSgjs (.indexNode (.slit [97, 98, 99]) [100]) []).isNone
#guard (evalSg false (.findNode (.slit [97, 98, 99]) [100]) []).bind SgVal.asInt = some (-1)
#guard (evalSgjs (.findNode (.slit [97, 98, 99]) [100]) []).bind SgVal.asInt = some (-1)
-- THE `//`-ON-METHOD-VALUE DEVIATION PINS (CPython):
-- "ababab".count("ab") // -2 == 3 // -2 == -2 (floor; JS-trunc gives -1 —
-- discriminating, see the stub contrast below).
#guard (evalSg false (.fdivNode (.countNode (.slit [97, 98, 97, 98, 97, 98]) [97, 98])
    (.ilit (-2))) []).bind SgVal.asInt = some (-2)
-- The .find MISS sentinel through `//`: "abc".find("d") // 2 == (-1) // 2 == -1
-- (floor; JS-trunc gives 0 — the value AND its truthiness differ).
#guard (evalSg false (.fdivNode (.findNode (.slit [97, 98, 99]) [100])
    (.ilit 2)) []).bind SgVal.asInt = some (-1)
-- BOOL-INT IDENTITY pins (CPython: bool is an int subtype):
-- "ab".startswith("a") // 2 == True // 2 == 0.
#guard (evalSg false (.fdivNode (.startswithNode (.slit [97, 98]) [97]) (.ilit 2))
    []).bind SgVal.asInt = some 0
#guard (evalSgjs (.fdivNode (.startswithNode (.slit [97, 98]) [97]) (.ilit 2))
    []).bind SgVal.asInt = some 0
-- 7 // ("b" in "abc") == 7 // True == 7.
#guard (evalSg false (.fdivNode (.ilit 7) (.containsNode (.slit [97, 98, 99]) [98]))
    []).bind SgVal.asInt = some 7
-- 7 // ("d" in "abc") == 7 // False raises ZeroDivisionError → none.
#guard (evalSg false (.fdivNode (.ilit 7) (.containsNode (.slit [97, 98, 99]) [100]))
    []).isNone
#guard (evalSgjs (.fdivNode (.ilit 7) (.containsNode (.slit [97, 98, 99]) [100]))
    []).isNone
-- "xhix".strip("x") // 2 is a TypeError (str is not a number) → none, both sides.
#guard (evalSg false (.fdivNode (.stripNode (.slit [120, 104, 105, 120]) [120])
    (.ilit 2)) []).isNone
#guard (evalSgjs (.fdivNode (.stripNode (.slit [120, 104, 105, 120]) [120])
    (.ilit 2)) []).isNone
-- division by zero → none (ZeroDivisionError), both sides.
#guard (evalSg false (.fdivNode (.ilit 1) (.ilit 0)) []).isNone
#guard (evalSgjs (.fdivNode (.ilit 1) (.ilit 0)) []).isNone

/-- General-method preservation as a predicate OVER the lowering — the SAME
    predicate is proved for the shipped lowering (`preservationSg_real`) and
    REFUTED for the stub (`preservationSg_stub_fails`). -/
def SgPreserves (L : IntDivLowering) : Prop :=
  ∀ e env, evalSgtgt L e env = evalSg false e env

/-- **General-string-method preservation (Tier-3 wave 19, C1-rollout wave 15
    re-architected).** The INDEPENDENT compiled target under the shipped
    lowering computes the Python reference on every fragment expression and
    environment: `startswith`/`endswith`/`in`/`find`/`index`/`count`/
    `replace`/`lstrip`/`rstrip`/`strip`/`split`/`join` are code-point-correct
    on both sides (the emitted helpers absorb the UTF-16 deviation — proved
    real on the substring surface by `smFindSub_ne_js16_astral`), and the `//`
    deviation on returned method values is absorbed by the emitted
    floor-correction (`jsFdiv_eq_fdiv`), through the CPython bool-int
    coercion on EVERY `SgVal` constructor. Real structural induction, not
    `rfl`: the deviation arm needs the arithmetic binding lemma. -/
theorem preservationSg (e : SgExp) (env : SEnv) :
    evalSgjs e env = evalSg false e env := by
  induction e with
  | ilit n => simp only [evalSgtgt, evalSg]
  | startswithNode s pre => simp only [evalSgtgt, evalSg]
  | endswithNode s suf => simp only [evalSgtgt, evalSg]
  | containsNode s t => simp only [evalSgtgt, evalSg]
  | findNode s t => simp only [evalSgtgt, evalSg]
  | indexNode s t => simp only [evalSgtgt, evalSg]
  | countNode s t => simp only [evalSgtgt, evalSg]
  | replaceNode s o n => simp only [evalSgtgt, evalSg]
  | lstripNode s cs => simp only [evalSgtgt, evalSg]
  | rstripNode s cs => simp only [evalSgtgt, evalSg]
  | stripNode s cs => simp only [evalSgtgt, evalSg]
  | splitNode s sep => simp only [evalSgtgt, evalSg]
  | joinNode sep ps => simp only [evalSgtgt, evalSg]
  | fdivNode a b iha ihb =>
      simp only [evalSgtgt, evalSg, iha, ihb]
      cases evalSg false a env with
      | none => rfl
      | some va =>
          cases evalSg false b env with
          | none => rfl
          | some vb =>
              -- every SgVal constructor flows through asArith on BOTH sides:
              -- gint passes, gbool coerces (bool-int identity), gstr/glist
              -- are TypeError → none — no shared-wrong non-int arm.
              simp only [Option.bind]
              cases va.asArith with
              | none => rfl
              | some x =>
                  cases vb.asArith with
                  | none => rfl
                  | some y =>
                      by_cases hy : y = 0
                      · simp [hy]
                      · simp [hy, jsELowering, jsFdiv_eq_fdiv x y hy]

/-- The re-architected statement, in predicate form: the shipped lowering
    preserves. Same content as `preservationSg`; this is the instantiation
    the stub litmus contrasts against. -/
theorem preservationSg_real : SgPreserves jsELowering := preservationSg

/-- **Stub litmus (wave 15).** The SAME preservation predicate is FALSE for
    the naive truncating lowering, on a DISCRIMINATING witness threading `//`
    through a RETURNED method value: `"abc".find("d") // 2` — the `.find`
    MISS sentinel `-1` divided by `2`. Python floors `-1 // 2 = -1`; the stub
    truncates `Int.tdiv (-1) 2 = 0` — the composed value differs (and `0`
    even flips truthiness vs `-1`). This is what the old
    `evalSg true = evalSg false` statement could not express (both lowerings
    were hardwired into the same flag-controlled evaluator, so no wrong
    lowering could ever falsify it). -/
theorem preservationSg_stub_fails : ¬ SgPreserves truncELowering := by
  intro h
  have hc := h (.fdivNode (.findNode (.slit [97, 98, 99]) [100]) (.ilit 2)) []
  -- hc: stub `Int.tdiv (-1) 2 = 0` vs Python `Int.fdiv (-1) 2 = -1` —
  -- `some (.gint 0) = some (.gint (-1))` is absurd.
  exact absurd hc (by decide)

-- The contrast, concretely (the stub is a plausible naive emission, and it
-- computes DIFFERENT method values):
#guard (evalSgjs (.fdivNode (.findNode (.slit [97, 98, 99]) [100])
    (.ilit 2)) []).bind SgVal.asInt = some (-1)  -- real: floor -1 // 2 = -1
#guard (evalSgtgt truncELowering (.fdivNode (.findNode (.slit [97, 98, 99]) [100])
    (.ilit 2)) []).bind SgVal.asInt = some 0     -- stub: trunc -1 / 2 = 0 ✗
-- and through a RETURNED COUNT with a negative divisor: real 3 // -2 = -2,
-- stub 3 / -2 = -1 ✗
#guard (evalSgjs (.fdivNode (.countNode (.slit [97, 98, 97, 98, 97, 98]) [97, 98])
    (.ilit (-2))) []).bind SgVal.asInt = some (-2)
#guard (evalSgtgt truncELowering
    (.fdivNode (.countNode (.slit [97, 98, 97, 98, 97, 98]) [97, 98])
    (.ilit (-2))) []).bind SgVal.asInt = some (-1)

-- SPOT 1: THE case the shipping runtime got wrong — compiled
-- "𝔸abc".find("bc") on the INDEPENDENT target, routed THROUGH the
-- preservation theorem to the Python reference: 2 code points (naive JS
-- .indexOf === 3; js16FindSub pin above).
example :
    (evalSgjs (.findNode (.slit [0x1D538, 97, 98, 99]) [98, 99]) []).bind SgVal.asInt
      = some 2 := by
  rw [preservationSg]
  decide

-- SPOT 2: first-occurrence minimality THROUGH smFindSub_some_iff — fails if
-- the characterization is too weak to pin the offset ("abcbc".find("bc") = 1,
-- NOT the also-matching 3).
example : smFindSub [97, 98, 99, 98, 99] [98, 99] = some 1 := by
  rw [smFindSub_some_iff]
  refine ⟨by decide, ?_⟩
  intro j hj
  have hj0 : j = 0 := by omega
  subst hj0
  decide

-- SPOT 3: the CPython split/join round-trip law on a concrete program,
-- THROUGH smSplit_join: ",".join("a,b,,c".split(",")) == "a,b,,c".
example : smJoin [44] [[97], [98], [], [99]] = [97, 44, 98, 44, 44, 99] :=
  smSplit_join _ _ _ (by decide)

-- SPOT 4: len("ababab".replace("ab", "z")) THROUGH the quantitative length
-- law: 6 + (1 - 2) * 3 = 3 (fails if the law or the count model drifts).
example : ((smReplace [97, 98, 97, 98, 97, 98] [97, 98] [122]).length : Int) = 3 := by
  rw [smReplace_length _ _ _ (by decide)]
  decide

-- SPOT 5: on "𝔸abc" the naive-JS offset provably is NOT Python's 2 — routed
-- THROUGH the general impossibility (the astral 𝔸 precedes the match).
example : js16FindSub [0x1D538, 97, 98, 99] [98, 99] ≠ some 2 :=
  smFindSub_ne_js16_astral _ _ 2 (by decide) 0x1D538 (by decide) (by decide)

-- SPOT 6: the `//` axis on a returned count — compiled
-- "ababab".count("ab") // -2 on the INDEPENDENT target, THROUGH the theorem:
-- 3 // -2 = -2 (CPython floors; the trunc stub gives -1 — the SPOT the stub
-- cannot close, see preservationSg_stub_fails' contrast guards).
example :
    (evalSgjs (.fdivNode (.countNode (.slit [97, 98, 97, 98, 97, 98]) [97, 98])
        (.ilit (-2))) []).bind SgVal.asInt = some (-2) := by
  rw [preservationSg]
  decide

-- SPOT 7: env-bound receiver — s = "𝔸a𝔸", compiled s.strip("𝔸") == "a"
-- (the astral strip-set case the shipping runtime got wrong), THROUGH the
-- preservation theorem.
example :
    (evalSgjs (.stripNode (.svar "s") [0x1D538])
        [("s", .sstr [0x1D538, 97, 0x1D538])]).bind SgVal.asCps = some [97] := by
  rw [preservationSg]
  decide

-- SPOT 8: the BOOL-INT identity through the theorem — compiled
-- "ab".startswith("a") // 2 == True // 2 == 0 (fails if a non-int SgVal
-- constructor were shared-wrong `none` through the `//` arm).
example :
    (evalSgjs (.fdivNode (.startswithNode (.slit [97, 98]) [97]) (.ilit 2))
        []).bind SgVal.asInt = some 0 := by
  rw [preservationSg]
  decide

/-- info: 'PythExpandVerify.preservationSg' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationSg

/-- info: 'PythExpandVerify.preservationSg_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationSg_real

/-- info: 'PythExpandVerify.preservationSg_stub_fails' does not depend on any axioms -/
#guard_msgs in
#print axioms preservationSg_stub_fails

/-- info: 'PythExpandVerify.smFindSub_some_iff' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms smFindSub_some_iff

/-- info: 'PythExpandVerify.smFindSub_ne_js16_astral' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms smFindSub_ne_js16_astral

/-- info: 'PythExpandVerify.smSplit_join' depends on axioms: [propext] -/
#guard_msgs in
#print axioms smSplit_join

/-- info: 'PythExpandVerify.smReplace_length' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms smReplace_length

/-- info: 'PythExpandVerify.smLstrip_unique' depends on axioms: [propext] -/
#guard_msgs in
#print axioms smLstrip_unique


/-! ## Tier-3 wave 20 — dictionary methods with int|str keys (the CPython
method surface + the stringifying-object key deviation)

Wave 9 proved dict LITERALS + `d[k]` lookup over Int keys and scoped out
everything else. This wave extends dicts to Python's int|str key space
(`DKey`) and the METHOD surface, with the CPython 3.7+ order rules:

- `d2Lookup`/`d2Mem`/`d2Get` — `d[k]` (`KeyError` → `none`, last-write-wins
  on duplicate literal keys), `k in d`, `d.get(k, default)`; specs:
  `d2Lookup_isSome_iff` (`d[k]` raises iff `k not in d`),
  `d2Mem_iff_mem_fst`, `d2Get_of_lookup`/`d2Get_of_absent`.
- `d2Keys`/`d2Values`/`d2Items`/`d2Len` — insertion order with
  FIRST-occurrence positions and LAST-write values (the CPython rule for
  `{1:'a', 2:'b', 1:'c'}`); specs: `d2Keys_mem_iff`, `d2Keys_nodup`
  (each key listed once), `d2Items_fst`/`d2Values_eq_items_snd`/
  `d2Len_coherent` (the views are consistent projections), and
  `d2Items_lookup` (every listed pair is a real lookup result).

**Spec validation (lean-spec-quality Stage 3).** Every model function is
transliterated in `verification/spec_validate_dictmethods.py` and
differentially checked against REAL CPython dicts built by literal-order
insertion — 19,538 checks green (duplicate keys, mixed int/str keys, the
`1` vs `'1'` collision pairs, empty dicts; keys/values/items compared in
ORDER).

**The deviation (why this wave is non-vacuous).** A naive JS OBJECT
stringifies its property keys, so Python's distinct keys `1` and `'1'`
collide (`jsKeyStr`); `jsObj_conflates` proves the naive object lookup
CANNOT implement Python dict lookup on any dict holding two
same-stringification keys with different values — so the shipped
Map-backed `PyDict` (#83) is necessary, not stylistic. The boundary is
exact: `jsObjLookup_eq_of_injective` (no collision → agreement) and its
all-string-keys corollary `jsObjLookup_eq_on_str_keys` (why pure-string
dicts "worked" on plain objects). The validator additionally checks
`jsObjLookup` against a REAL stringifying dict on every probe.

The fragment layer (`D2Exp`/`evalD2`, mutual with `evalD2s` exactly as
wave 9) covers dict literals with int-expression values, `d[k]`,
`d.get(k, default)`, `k in d`, `len(d)`, `keys()`, `values()`, `items()`,
env-bound dicts, and the seed `//` axis (`jsFdiv_eq_fdiv`).

**C1-rollout wave 16 (re-architected — the FINAL rollout wave).** The
previous `preservationD2 : evalD2 true e env = evalD2 false e env` (with
its mutual workers `preservationD2'`/`preservationD2s'`) was the F1
model-vs-model tautology this file's own C1 design note below used as THE
motivating example: a `Bool`-flagged copy of ONE mutual evaluator pair,
deviating only in the `//` arm — stubbing the shipping lowering could not
break it. Re-architected on the accepted wave-1…15 recipe:
`evalD2tgt`/`evalD2stgt` are a SEPARATE mutual recursion, parameterized by
the integer-division lowering, so `//` threads through dict-literal VALUES,
`get`-defaults, and arithmetic over returned METHOD results exactly as in
the emitted JS; BOTH `DKey` constructors and EVERY `D2Res` constructor are
handled faithfully on both sides (arithmetic coerces through
`D2Res.asArith` — bool-int identity, view/dict operands → TypeError;
`d.get` returns the evaluated default UNCHANGED on a miss, whatever its
constructor); both sides share only the verified pure-data method surface
(`d2Lookup`/`d2Get`/`d2Mem`/`d2Len`/`d2Keys`/`d2Values`/`d2Items` — each
CPython-pinned above, the stringifying-object alternative refuted by
`jsObj_conflates`), never a lowering. The SAME predicates
(`D2Preserves`/`D2sPreserves`) are TRUE for the shipped floor-correction
(`preservationD2`/`preservationD2s`) and provably FALSE for the naive
truncating lowering (`preservationD2_stub_fails` — the deviating value read
back through `d[k]` — and `preservationD2s_stub_fails` — the entry list
itself).

OUT of scope (documented, deliberate): bool keys (bool ⊂ int is the
value-representation deviation D-class — a bool key IS an int key after
representation), float keys (D1, wave 9's documented exclusion), tuple/
hashable-object keys, mutation (`d[k] = v` / `update` / `pop` /
`setdefault` — the statement-level dict story), and dict comprehensions
(wave 6 territory). -/


/-! ### Keys: int | str (Python-distinct, unlike a stringifying JS object) -/

/-- A dict key: Python `int` or `str` (code-point list). CPython keeps the
    two DISTINCT (`{1: 'a', '1': 'b'}` has two entries) — precisely what the
    naive JS-object model below conflates. Bool keys are out of scope
    (bool ⊂ int is the documented value-representation deviation). -/
inductive DKey where
  | kint (n : Int)
  | kstr (cps : List Int)
deriving Repr, DecidableEq

/-! ### The dict model: literal-order entry list, CPython method surface -/

/-- CPython lookup on the literal-order entry list: LATER entries win
    (duplicate literal key → last value), missing key → `none` (`KeyError`).
    Wave 9's `dictLookup`, over `DKey` keys. -/
def d2Lookup (k : DKey) : List (DKey × Int) → Option Int
  | [] => none
  | (k', v) :: rest =>
      match d2Lookup k rest with
      | some v' => some v'
      | none => if k' = k then some v else none

/-- Python `k in d`. -/
def d2Mem (k : DKey) : List (DKey × Int) → Bool
  | [] => false
  | (k', _) :: rest => if k' = k then true else d2Mem k rest

/-- Python `d.get(k, default)`. -/
def d2Get (k : DKey) (dflt : Int) (es : List (DKey × Int)) : Int :=
  (d2Lookup k es).getD dflt

/-- Python `list(d.keys())`: FIRST-occurrence order, deduplicated — the
    CPython 3.7+ insertion-order rule (a duplicated literal key keeps its
    first POSITION while `d2Lookup` gives it its last VALUE). -/
def d2Keys : List (DKey × Int) → List DKey
  | [] => []
  | (k, _) :: rest => k :: (d2Keys rest).filter (fun k' => !(k' == k))

/-- Python `list(d.items())`: each first-occurrence key paired with its
    (last-write-wins) value. -/
def d2Items (es : List (DKey × Int)) : List (DKey × Int) :=
  (d2Keys es).map (fun k => (k, (d2Lookup k es).getD 0))

/-- Python `list(d.values())`. -/
def d2Values (es : List (DKey × Int)) : List Int :=
  (d2Items es).map Prod.snd

/-- Python `len(d)`. -/
def d2Len (es : List (DKey × Int)) : Nat :=
  (d2Keys es).length

/-! ### Method-surface specs -/

/-- PROPOSED (human review): membership decides "is one of the entry keys". -/
theorem d2Mem_iff_mem_fst (k : DKey) (es : List (DKey × Int)) :
    d2Mem k es = true ↔ k ∈ es.map Prod.fst := by
  induction es with
  | nil => simp [d2Mem]
  | cons e rest ih =>
      obtain ⟨k', v⟩ := e
      by_cases hk : k' = k
      · subst hk
        simp [d2Mem]
      · simp only [d2Mem, if_neg hk, ih, List.map_cons, List.mem_cons]
        constructor
        · intro h
          exact Or.inr h
        · intro h
          rcases h with h1 | h2
          · exact absurd h1.symm hk
          · exact h2

/-- PROPOSED (human review): `keys` contains exactly the entry keys. -/
theorem d2Keys_mem_iff (k : DKey) (es : List (DKey × Int)) :
    k ∈ d2Keys es ↔ k ∈ es.map Prod.fst := by
  induction es with
  | nil => simp [d2Keys]
  | cons e rest ih =>
      obtain ⟨k', v⟩ := e
      simp only [d2Keys, List.map_cons, List.mem_cons]
      constructor
      · intro h
        rcases h with h1 | h2
        · exact Or.inl h1
        · have := (List.mem_filter.mp h2).1
          exact Or.inr (ih.mp this)
      · intro h
        rcases h with h1 | h2
        · exact Or.inl h1
        · by_cases hk : k = k'
          · exact Or.inl hk
          · refine Or.inr (List.mem_filter.mpr ⟨ih.mpr h2, ?_⟩)
            simp [hk]

/-- PROPOSED (human review): the keys list has NO duplicates (each key
    appears once, at its first occurrence). -/
theorem d2Keys_nodup (es : List (DKey × Int)) : (d2Keys es).Nodup := by
  induction es with
  | nil => exact List.nodup_nil
  | cons e rest ih =>
      obtain ⟨k, v⟩ := e
      simp only [d2Keys]
      rw [List.nodup_cons]
      constructor
      · intro hmem
        have := (List.mem_filter.mp hmem).2
        simp at this
      · exact ih.filter _

/-- PROPOSED (human review): lookup succeeds exactly on member keys
    (`d[k]` raises `KeyError` iff `k not in d`). -/
theorem d2Lookup_isSome_iff (k : DKey) (es : List (DKey × Int)) :
    (d2Lookup k es).isSome = true ↔ d2Mem k es = true := by
  induction es with
  | nil => simp [d2Lookup, d2Mem]
  | cons e rest ih =>
      obtain ⟨k', v⟩ := e
      constructor
      · intro h
        simp only [d2Mem]
        by_cases hk : k' = k
        · rw [if_pos hk]
        · rw [if_neg hk, ← ih]
          simp only [d2Lookup] at h
          cases hr : d2Lookup k rest with
          | some w => rfl
          | none =>
              rw [hr] at h
              simp [hk] at h
      · intro h
        simp only [d2Lookup]
        cases hr : d2Lookup k rest with
        | some w => rfl
        | none =>
            by_cases hk : k' = k
            · simp [hk]
            · simp only [d2Mem] at h
              rw [if_neg hk] at h
              rw [← ih, hr] at h
              simp at h

/-- Python `d.get(k, default)` on a present key returns the stored value. -/
theorem d2Get_of_lookup (k : DKey) (dflt v : Int) (es : List (DKey × Int))
    (h : d2Lookup k es = some v) : d2Get k dflt es = v := by
  unfold d2Get
  rw [h]
  rfl

/-- PROPOSED (human review): `d.get(k, default)` on an ABSENT key returns
    the default (the arm `d[k]` would raise on). -/
theorem d2Get_of_absent (k : DKey) (dflt : Int) (es : List (DKey × Int))
    (h : d2Mem k es = false) : d2Get k dflt es = dflt := by
  unfold d2Get
  cases hr : d2Lookup k es with
  | none => rfl
  | some w =>
      have : d2Mem k es = true := by
        rw [← d2Lookup_isSome_iff, hr]
        rfl
      rw [this] at h
      cases h

/-- Every listed key can actually be looked up. -/
theorem d2Lookup_of_mem_keys (k : DKey) (es : List (DKey × Int))
    (h : k ∈ d2Keys es) : (d2Lookup k es).isSome = true := by
  rw [d2Lookup_isSome_iff, d2Mem_iff_mem_fst]
  exact (d2Keys_mem_iff k es).mp h

/-- PROPOSED (human review): `items` projects to `keys` on the left. -/
theorem d2Items_fst (es : List (DKey × Int)) :
    (d2Items es).map Prod.fst = d2Keys es := by
  unfold d2Items
  generalize d2Keys es = ks
  induction ks with
  | nil => rfl
  | cons k ks ih => simp only [List.map_cons, ih]

/-- `values` IS the right projection of `items` (definitional). -/
theorem d2Values_eq_items_snd (es : List (DKey × Int)) :
    d2Values es = (d2Items es).map Prod.snd := rfl

/-- PROPOSED (human review): `len(d) = len(keys) = len(values) = len(items)`. -/
theorem d2Len_coherent (es : List (DKey × Int)) :
    d2Len es = (d2Keys es).length
      ∧ d2Len es = (d2Items es).length
      ∧ d2Len es = (d2Values es).length := by
  refine ⟨rfl, ?_, ?_⟩
  · unfold d2Items d2Len
    rw [List.length_map]
  · unfold d2Values d2Items d2Len
    rw [List.length_map, List.length_map]

/-- PROPOSED (human review): `items` is CORRECT — every listed pair is a
    real lookup result (`(k, v) ∈ d.items() → d[k] == v`). -/
theorem d2Items_lookup (k : DKey) (v : Int) (es : List (DKey × Int))
    (h : (k, v) ∈ d2Items es) : d2Lookup k es = some v := by
  unfold d2Items at h
  obtain ⟨k', hk', heq⟩ := List.mem_map.mp h
  injection heq with h1 h2
  subst h1
  have hsome := d2Lookup_of_mem_keys k' es hk'
  cases hr : d2Lookup k' es with
  | none => rw [hr] at hsome; simp at hsome
  | some w =>
      rw [hr] at h2
      simp only [Option.getD_some] at h2
      rw [h2]

/-! ### The naive-JS deviation model: stringified object keys -/

/-- Decimal digit run of a natural (code points; empty for 0 — callers
    handle the zero case). -/
def jsNatDigits : Nat → List Int
  | 0 => []
  | n + 1 => jsNatDigits ((n + 1) / 10) ++ [48 + (((n + 1) % 10 : Nat) : Int)]
decreasing_by exact Nat.div_lt_self (Nat.succ_pos n) (by decide)

/-- What a naive JS OBJECT does to a property key: stringify it. `kint n`
    becomes its decimal string, `kstr s` stays itself — so `1` and `"1"`
    collide. (A `Map`-backed dict — what PythScribe ships — keeps them
    distinct.) -/
def jsKeyStr : DKey → List Int
  | .kint n =>
      if n < 0 then 45 :: jsNatDigits n.natAbs
      else if n = 0 then [48]
      else jsNatDigits n.natAbs
  | .kstr cps => cps

/-- Naive JS-object lookup: last entry whose STRINGIFIED key matches. -/
def jsObjLookup (k : DKey) : List (DKey × Int) → Option Int
  | [] => none
  | (k', v) :: rest =>
      match jsObjLookup k rest with
      | some v' => some v'
      | none => if jsKeyStr k' = jsKeyStr k then some v else none

-- jsKeyStr pins: str(1) = "1", str(0) = "0", str(-1) = "-1", str(10) = "10";
-- and THE collision: str-key "1" has the same encoding.
#guard jsKeyStr (.kint 1) = [49]
#guard jsKeyStr (.kint 0) = [48]
#guard jsKeyStr (.kint (-1)) = [45, 49]
#guard jsKeyStr (.kint 10) = [49, 48]
#guard jsKeyStr (.kstr [49]) = [49]

/-- PROPOSED (human review): the GENERAL conflation impossibility — for ANY
    two distinct keys with the same stringification and different values,
    the naive JS-object lookup provably disagrees with the Python dict on
    the two-entry dict `{k1: v1, k2: v2}`. So a stringifying object CANNOT
    implement Python dict lookup; the shipped Map-backed representation is
    necessary, not stylistic. -/
theorem jsObj_conflates (k1 k2 : DKey) (v1 v2 : Int)
    (hcol : jsKeyStr k1 = jsKeyStr k2) (hne : k1 ≠ k2) (hv : v1 ≠ v2) :
    jsObjLookup k1 [(k1, v1), (k2, v2)] ≠ d2Lookup k1 [(k1, v1), (k2, v2)] := by
  have hpy : d2Lookup k1 [(k1, v1), (k2, v2)] = some v1 := by
    simp only [d2Lookup]
    rw [if_neg (fun h : k2 = k1 => hne h.symm)]
    simp
  have hjs : jsObjLookup k1 [(k1, v1), (k2, v2)] = some v2 := by
    simp only [jsObjLookup]
    rw [if_pos hcol.symm]
  rw [hpy, hjs]
  intro hcontra
  injection hcontra with hcontra
  exact hv hcontra.symm

/-- PROPOSED (human review): agreement boundary — when the stringification
    is INJECTIVE relative to the probe key (no entry key collides with it),
    the naive object lookup agrees with Python. The deviation is exactly
    the collision case, not spurious. -/
theorem jsObjLookup_eq_of_injective (es : List (DKey × Int)) (k : DKey)
    (hinj : ∀ k' ∈ es.map Prod.fst, jsKeyStr k' = jsKeyStr k → k' = k) :
    jsObjLookup k es = d2Lookup k es := by
  induction es with
  | nil => rfl
  | cons e rest ih =>
      obtain ⟨k', v⟩ := e
      have hrest := ih (fun k'' hk'' h => hinj k'' (by simp [hk'']) h)
      simp only [jsObjLookup, d2Lookup, hrest]
      cases d2Lookup k rest with
      | some v' => rfl
      | none =>
          by_cases hk : k' = k
          · subst hk
            rw [if_pos rfl, if_pos rfl]
          · rw [if_neg hk, if_neg (fun hcol => hk (hinj k' (by simp) hcol))]

/-- PROPOSED (human review): all-string-keys corollary — on dicts whose keys
    are all strings, the naive object agrees with Python for every string
    probe (why pure-string dicts "worked" on JS objects). -/
theorem jsObjLookup_eq_on_str_keys (es : List (DKey × Int)) (s : List Int)
    (hstr : ∀ p ∈ es, ∃ t, p.1 = DKey.kstr t) :
    jsObjLookup (.kstr s) es = d2Lookup (.kstr s) es := by
  apply jsObjLookup_eq_of_injective
  intro k' hk' hcol
  obtain ⟨p, hp, hfst⟩ := List.mem_map.mp hk'
  obtain ⟨t, ht⟩ := hstr p hp
  rw [← hfst, ht] at hcol ⊢
  simp only [jsKeyStr] at hcol
  rw [hcol]

-- executable bindings pinned to CPython dict semantics (each case checked
-- against real CPython by verification/spec_validate_dictmethods.py):
-- {1: 10, 2: 20}[2] == 20; missing → KeyError; {1: 1, 1: 2}[1] == 2
-- (last write wins).
#guard d2Lookup (.kint 2) [(.kint 1, 10), (.kint 2, 20)] = some 20
#guard d2Lookup (.kint 3) [(.kint 1, 10), (.kint 2, 20)] = none
#guard d2Lookup (.kint 1) [(.kint 1, 1), (.kint 1, 2)] = some 2
-- THE COLLISION PIN: {1: 10, '1': 20} — CPython keeps BOTH keys
-- (d[1] == 10, d['1'] == 20, len == 2); the naive stringifying object
-- conflates them (reading key 1 yields 20).
#guard d2Lookup (.kint 1) [(.kint 1, 10), (.kstr [49], 20)] = some 10
#guard d2Lookup (.kstr [49]) [(.kint 1, 10), (.kstr [49], 20)] = some 20
#guard d2Len [(.kint 1, 10), (.kstr [49], 20)] = 2
#guard jsObjLookup (.kint 1) [(.kint 1, 10), (.kstr [49], 20)] = some 20
-- {1: 'a', 2: 'b', 1: 'c'} → keys [1, 2] (FIRST position), values [c, b]
-- (LAST value), items zips them, len 2. (Int stand-ins for the values.)
#guard d2Keys [(.kint 1, 1), (.kint 2, 2), (.kint 1, 3)] = [.kint 1, .kint 2]
#guard d2Values [(.kint 1, 1), (.kint 2, 2), (.kint 1, 3)] = [3, 2]
#guard d2Items [(.kint 1, 1), (.kint 2, 2), (.kint 1, 3)]
    = [(.kint 1, 3), (.kint 2, 2)]
#guard d2Len [(.kint 1, 1), (.kint 2, 2), (.kint 1, 3)] = 2
-- get: hit → stored value; miss → default. in: 1 in {1: 'a'} but NOT
-- '1' in {1: 'a'} (key types matter).
#guard d2Get (.kstr [97]) 99 [(.kstr [97], 1)] = 1
#guard d2Get (.kstr [98]) 99 [(.kstr [97], 1)] = 99
#guard d2Mem (.kint 1) [(.kint 1, 10)] = true
#guard d2Mem (.kstr [49]) [(.kint 1, 10)] = false
-- empty dict: len 0, everything absent.
#guard d2Len [] = 0
#guard d2Mem (.kint 1) [] = false
#guard (d2Keys [] : List DKey) = []

/-! ### The dict expression fragment and its two semantics -/

/-- Results of the dict-method fragment: `d[k]`/`get`/`len` return ints,
    `in` a bool, and the views return key/value/item lists. -/
inductive D2Res where
  | rint (n : Int)
  | rbool (b : Bool)
  | rdict (es : List (DKey × Int))
  | rkeys (ks : List DKey)
  | rvals (ns : List Int)
  | ritems (ps : List (DKey × Int))
deriving Repr

def D2Res.asInt : D2Res → Option Int
  | .rint n => some n | _ => none

def D2Res.asBool : D2Res → Option Bool
  | .rbool b => some b | _ => none

def D2Res.asKeys : D2Res → Option (List DKey)
  | .rkeys ks => some ks | _ => none

def D2Res.asVals : D2Res → Option (List Int)
  | .rvals ns => some ns | _ => none

def D2Res.asItems : D2Res → Option (List (DKey × Int))
  | .ritems ps => some ps | _ => none

/-- CPython arithmetic coercion over EVERY `D2Res` constructor (F9
    domain-completeness, the wave-15 `SgVal.asArith` discipline): ints pass,
    a bool is its int value (the CPython bool-int IDENTITY —
    `7 // (k in d)` can raise ZeroDivisionError because `False == 0`),
    and dicts / key-views / value-views / item-views are `TypeError` → `none`
    (CPython: `d.keys() // 2`, `{} // 1`, … are all TypeErrors) — faithful on
    the `rbool` constructor instead of a shared-wrong `none` (the
    wave-5-iter1 defect shape). Used ONLY by the `ifdiv` (`//`) arm, the sole
    arithmetic constructor in this fragment. Emitted JS agrees on that GUARDED
    surface: `//` (with `-`/`/`/`%`/`**`) routes through wave-15's
    `__reqArithNum` numeric-operand guard, which coerces booleans and rejects
    dicts/views. HONEST BOUNDARY: `+` (`iadd`) was REMOVED from this fragment
    (wave-16 iter2) because shipped `pyAdd` is NOT yet guarded — its
    non-numeric-operand gap is the pre-existing C3/C4 arithmetic-type-safety
    workstream (the design notes), so covering `+` here would be a
    model-vs-shipped over-claim; that gap stays tracked in that workstream. -/
def D2Res.asArith : D2Res → Option Int
  | .rint n => some n
  | .rbool b => some (if b then 1 else 0)
  | .rdict _ => none
  | .rkeys _ => none
  | .rvals _ => none
  | .ritems _ => none

abbrev D2Env := List (String × List (DKey × Int))

def D2Env.get (env : D2Env) (n : String) : Option (List (DKey × Int)) :=
  (env.find? (fun p => p.1 == n)).map (·.2)

/-- The dict-method fragment: literals with `DKey` keys and int-expression
    values (evaluated left-to-right), the method surface, and the seed `//`
    deviation axis on int results. -/
inductive D2Exp where
  | ilit (n : Int)
  | ifdiv (a b : D2Exp)   -- int sub-fragment (`//` deviates; shipping-GUARDED via `__reqArithNum` on pyFloorDiv). NOTE: `iadd`(`+`) was REMOVED (wave-16 iter2) — shipped `pyAdd` is unguarded (the deferred C3/C4 arithmetic-type-safety workstream), so claiming `+`-preservation here would be a model-vs-shipped over-claim; the fragment covers only shipping-guarded arithmetic.
  | dlit (entries : List (DKey × D2Exp))       -- {k1: v1, …}
  | dvar (s : String)                          -- env-bound dict
  | getNode (d : D2Exp) (k : DKey)             -- d[k] (KeyError → none)
  | getDNode (d : D2Exp) (k : DKey) (dflt : D2Exp)  -- d.get(k, default)
  | memNode (d : D2Exp) (k : DKey)             -- k in d
  | lenNode (d : D2Exp)                        -- len(d)
  | keysNode (d : D2Exp)                       -- list(d.keys())
  | valuesNode (d : D2Exp)                     -- list(d.values())
  | itemsNode (d : D2Exp)                      -- list(d.items())
deriving Repr

mutual
/-- PYTHON-REFERENCE dict-method fragment eval (`evalD2 false` throughout;
    the `tgt = true` branch is documented LEGACY from the pre-rollout
    `Bool`-flag shape — NO theorem references it; the compiled semantics is
    the INDEPENDENT `evalD2tgt` below). `tgt` affects only `//`; dict
    construction and the whole method surface are target-independent (the
    compiler ships a Map-backed dict — `jsObjLookup` above is what a naive
    stringifying OBJECT would compute, kept OUTSIDE the semantics exactly as
    the UTF-16 models of waves 11/18/19). F9 domain-completeness over
    `D2Res`: arithmetic (`//`, the sole arithmetic arm) coerces EVERY constructor through
    `D2Res.asArith` (bool-int identity; dict/view operands → TypeError →
    `none`), and `d.get(k, default)` returns the (eagerly evaluated) default
    UNCHANGED on a miss — ANY constructor (a bool default stays a bool, a
    dict default stays a dict; on an int default this is exactly `d2Get`).
    Entry VALUES coerce through `asArith` too: a bool value is stored as its
    int REPRESENTATION (CPython `{1: True} == {1: 1}` — the documented
    D-class bool⊂int value-representation quotient, same rationale as bool
    KEYS being out of scope); dict/view VALUES are outside this fragment's
    int-valued-dict domain (`none` = out-of-fragment, NOT a CPython-TypeError
    claim — nested dict values are wave 9's `DVal` layer). -/
def evalD2 (tgt : Bool) : D2Exp → D2Env → Option D2Res
  | .ilit n, _ => some (.rint n)
  | .ifdiv a b, env =>
      (evalD2 tgt a env).bind fun ra => (evalD2 tgt b env).bind fun rb =>
        ra.asArith.bind fun x => rb.asArith.bind fun y =>
          if y = 0 then none
          else some (.rint (if tgt then jsFdiv x y else Int.fdiv x y))
  | .dlit entries, env => (evalD2s tgt entries env).map .rdict
  | .dvar s, env => (env.get s).map .rdict
  | .getNode d k, env => match evalD2 tgt d env with
      | some (.rdict es) => (d2Lookup k es).map .rint
      | _ => none
  | .getDNode d k dflt, env => match evalD2 tgt d env, evalD2 tgt dflt env with
      | some (.rdict es), some dv =>
          match d2Lookup k es with
          | some v => some (.rint v)
          | none => some dv
      | _, _ => none
  | .memNode d k, env => match evalD2 tgt d env with
      | some (.rdict es) => some (.rbool (d2Mem k es)) | _ => none
  | .lenNode d, env => match evalD2 tgt d env with
      | some (.rdict es) => some (.rint (d2Len es)) | _ => none
  | .keysNode d, env => match evalD2 tgt d env with
      | some (.rdict es) => some (.rkeys (d2Keys es)) | _ => none
  | .valuesNode d, env => match evalD2 tgt d env with
      | some (.rdict es) => some (.rvals (d2Values es)) | _ => none
  | .itemsNode d, env => match evalD2 tgt d env with
      | some (.rdict es) => some (.ritems (d2Items es)) | _ => none
termination_by e _ => sizeOf e

/-- Entry-list eval (dict literals), literal order preserved; each VALUE
    coerces through `D2Res.asArith` (bool → its int representation; see the
    `evalD2` docstring for the domain boundary on dict/view values). -/
def evalD2s (tgt : Bool) : List (DKey × D2Exp) → D2Env → Option (List (DKey × Int))
  | [], _ => some []
  | (k, e) :: rest, env =>
      (evalD2 tgt e env).bind fun v => v.asArith.bind fun n =>
        (evalD2s tgt rest env).map fun vs => (k, n) :: vs
termination_by es _ => sizeOf es
end

-- F9 pins: the REFERENCE `evalD2 false` is itself pinned to CPython (via
-- the method-surface pins above), not merely to the target. The retired
-- `evalD2 true` legacy guards now live on the independent `evalD2js` below.
-- {1: 10, '1': 20}[1] == 10 — the collision dict READ CORRECTLY.
#guard (evalD2 false (.getNode (.dlit [(.kint 1, .ilit 10), (.kstr [49], .ilit 20)])
    (.kint 1)) []).bind D2Res.asInt = some 10
-- {1: 'a', 2: 'b', 1: 'c'}.keys() == [1, 2] through the fragment.
#guard (evalD2 false (.keysNode (.dlit [(.kint 1, .ilit 1), (.kint 2, .ilit 2),
    (.kint 1, .ilit 3)])) []).bind D2Res.asKeys = some [.kint 1, .kint 2]
-- values()/items() of the duplicate-key dict: LAST-write values in
-- FIRST-occurrence positions.
#guard (evalD2 false (.valuesNode (.dlit [(.kint 1, .ilit 1), (.kint 2, .ilit 2),
    (.kint 1, .ilit 3)])) []).bind D2Res.asVals = some [3, 2]
#guard (evalD2 false (.itemsNode (.dlit [(.kint 1, .ilit 1), (.kint 2, .ilit 2),
    (.kint 1, .ilit 3)])) []).bind D2Res.asItems = some [(.kint 1, 3), (.kint 2, 2)]
-- d.get on a missing STR key (int key present) returns the default.
#guard (evalD2 false (.getDNode (.dlit [(.kint 1, .ilit 10)]) (.kstr [49])
    (.ilit 99)) []).bind D2Res.asInt = some 99
-- d.get on a PRESENT key returns the stored value, ignoring the default.
#guard (evalD2 false (.getDNode (.dlit [(.kint 1, .ilit 10)]) (.kint 1)
    (.ilit 99)) []).bind D2Res.asInt = some 10
-- d.get returns a non-int default UNCHANGED on a miss (F9: every `D2Res`
-- constructor is a legal default): {1: 10}.get('1', 2 in {}) == False.
#guard (evalD2 false (.getDNode (.dlit [(.kint 1, .ilit 10)]) (.kstr [49])
    (.memNode (.dlit []) (.kint 2))) []).bind D2Res.asBool = some false
-- '1' in {1: 10} is False; 1 in {1: 10} is True.
#guard (evalD2 false (.memNode (.dlit [(.kint 1, .ilit 10)]) (.kstr [49])) []).bind D2Res.asBool
    = some false
#guard (evalD2 false (.memNode (.dlit [(.kint 1, .ilit 10)]) (.kint 1)) []).bind D2Res.asBool
    = some true
-- env-bound dict: d = {1: 10, '1': 20}; len(d) == 2.
#guard (evalD2 false (.lenNode (.dvar "d"))
    [("d", [(.kint 1, 10), (.kstr [49], 20)])]).bind D2Res.asInt = some 2
-- KeyError → none.
#guard (evalD2 false (.getNode (.dlit [(.kint 1, .ilit 10)]) (.kint 3)) []).isNone
-- THE `//`-DEVIATION PINS (CPython): through a stored dict VALUE
-- ({0: -7//2}[0] == -4, floor — JS-trunc would store -3), through values()
-- ({0: -7//2}.values() == [-4]), through a get-DEFAULT
-- ({}.get(1, -7//2) == -4), and composed over a METHOD RESULT
-- (len({1: 10}) // -2 == 1 // -2 == -1, floor — JS-trunc gives 0, so the
-- value AND its truthiness differ).
#guard (evalD2 false (.getNode (.dlit [(.kint 0, .ifdiv (.ilit (-7)) (.ilit 2))])
    (.kint 0)) []).bind D2Res.asInt = some (-4)
#guard (evalD2 false (.valuesNode (.dlit [(.kint 0, .ifdiv (.ilit (-7)) (.ilit 2))]))
    []).bind D2Res.asVals = some [-4]
#guard (evalD2 false (.getDNode (.dlit []) (.kint 1)
    (.ifdiv (.ilit (-7)) (.ilit 2))) []).bind D2Res.asInt = some (-4)
#guard (evalD2 false (.ifdiv (.lenNode (.dlit [(.kint 1, .ilit 10)])) (.ilit (-2)))
    []).bind D2Res.asInt = some (-1)
-- BOOL-INT IDENTITY pin (CPython: bool is an int subtype), via the guarded `//`:
-- 7 // ('1' in {1: 10}) == 7 // False raises ZeroDivisionError → none.
#guard (evalD2 false (.ifdiv (.ilit 7) (.memNode (.dlit [(.kint 1, .ilit 10)])
    (.kstr [49]))) []).isNone
-- {1: (1 in {1: 10})} stores the bool VALUE as its int representation
-- (CPython: {1: True} == {1: 1} — the documented bool⊂int quotient).
#guard (evalD2 false (.getNode (.dlit [(.kint 1,
    .memNode (.dlit [(.kint 1, .ilit 10)]) (.kint 1))]) (.kint 1)) []).bind D2Res.asInt = some 1
-- d.keys() // 2 is a TypeError → none.
#guard (evalD2 false (.ifdiv (.keysNode (.dlit [(.kint 1, .ilit 10)])) (.ilit 2)) []).isNone
-- division by zero → none (ZeroDivisionError).
#guard (evalD2 false (.ifdiv (.ilit 1) (.ilit 0)) []).isNone

mutual
/-- **Independent target evaluator** (dict-method expressions): the compiled
    program's semantics under lowering `L`. A SEPARATE mutual recursion (not
    the `Bool` flag on `evalD2`); the `//` arm calls the lowering's
    operation, and it threads through dict-literal VALUES (via `evalD2stgt`),
    `get`-defaults, and arithmetic over returned METHOD results exactly as in
    the emitted JS. EVERY `D2Res` constructor is handled faithfully:
    arithmetic coerces through `D2Res.asArith` (bool-int identity —
    mirroring the emitted numeric-operand-guarded helpers; dict/view
    operands → TypeError → `none`), `d.get` returns the evaluated default
    UNCHANGED on a miss (any constructor), and BOTH `DKey` constructors flow
    through the shared pure-data method surface (`d2Lookup`/`d2Mem`/
    `d2Keys`/`d2Values`/`d2Items`/`d2Len` — the Map-backed `PyDict` keeps
    `1` and `'1'` distinct; the stringifying-object alternative is refuted
    by `jsObj_conflates`, never part of either semantics). -/
def evalD2tgt (L : IntDivLowering) : D2Exp → D2Env → Option D2Res
  | .ilit n, _ => some (.rint n)
  | .ifdiv a b, env =>
      (evalD2tgt L a env).bind fun ra => (evalD2tgt L b env).bind fun rb =>
        ra.asArith.bind fun x => rb.asArith.bind fun y =>
          if y = 0 then none
          else some (.rint (L.fdiv x y))
  | .dlit entries, env => (evalD2stgt L entries env).map .rdict
  | .dvar s, env => (env.get s).map .rdict
  | .getNode d k, env => match evalD2tgt L d env with
      | some (.rdict es) => (d2Lookup k es).map .rint
      | _ => none
  | .getDNode d k dflt, env => match evalD2tgt L d env, evalD2tgt L dflt env with
      | some (.rdict es), some dv =>
          match d2Lookup k es with
          | some v => some (.rint v)
          | none => some dv
      | _, _ => none
  | .memNode d k, env => match evalD2tgt L d env with
      | some (.rdict es) => some (.rbool (d2Mem k es)) | _ => none
  | .lenNode d, env => match evalD2tgt L d env with
      | some (.rdict es) => some (.rint (d2Len es)) | _ => none
  | .keysNode d, env => match evalD2tgt L d env with
      | some (.rdict es) => some (.rkeys (d2Keys es)) | _ => none
  | .valuesNode d, env => match evalD2tgt L d env with
      | some (.rdict es) => some (.rvals (d2Values es)) | _ => none
  | .itemsNode d, env => match evalD2tgt L d env with
      | some (.rdict es) => some (.ritems (d2Items es)) | _ => none
termination_by e _ => sizeOf e

/-- Independent-target entry-list eval (dict literals): every VALUE
    expression routes through the same lowering `L` and coerces through
    `D2Res.asArith`; literal order kept. -/
def evalD2stgt (L : IntDivLowering) : List (DKey × D2Exp) → D2Env → Option (List (DKey × Int))
  | [], _ => some []
  | (k, e) :: rest, env =>
      (evalD2tgt L e env).bind fun v => v.asArith.bind fun n =>
        (evalD2stgt L rest env).map fun vs => (k, n) :: vs
termination_by es _ => sizeOf es
end

/-- The compiled dict-method semantics: the independent target under the
    SHIPPED lowering. -/
abbrev evalD2js : D2Exp → D2Env → Option D2Res := evalD2tgt jsELowering

/-- The compiled entry-list semantics: the independent target under the
    SHIPPED lowering. -/
abbrev evalD2sjs : List (DKey × D2Exp) → D2Env → Option (List (DKey × Int)) :=
  evalD2stgt jsELowering

/-- Dict-method preservation as a predicate OVER the lowering — the SAME
    predicate is proved for the shipped lowering (`preservationD2_real`) and
    REFUTED for the stub (`preservationD2_stub_fails`). -/
def D2Preserves (L : IntDivLowering) : Prop :=
  ∀ e env, evalD2tgt L e env = evalD2 false e env

/-- Entry-list preservation as a predicate OVER the lowering
    (`preservationD2s_real` vs `preservationD2s_stub_fails`). -/
def D2sPreserves (L : IntDivLowering) : Prop :=
  ∀ es env, evalD2stgt L es env = evalD2s false es env

-- Compiled-side guards (the retired `evalD2 true` guards, now on the
-- genuine independent target), mirroring the reference pins above.
#guard (evalD2js (.getNode (.dlit [(.kint 1, .ilit 10), (.kstr [49], .ilit 20)])
    (.kint 1)) []).bind D2Res.asInt = some 10
#guard (evalD2js (.keysNode (.dlit [(.kint 1, .ilit 1), (.kint 2, .ilit 2),
    (.kint 1, .ilit 3)])) []).bind D2Res.asKeys = some [.kint 1, .kint 2]
#guard (evalD2js (.valuesNode (.dlit [(.kint 1, .ilit 1), (.kint 2, .ilit 2),
    (.kint 1, .ilit 3)])) []).bind D2Res.asVals = some [3, 2]
#guard (evalD2js (.itemsNode (.dlit [(.kint 1, .ilit 1), (.kint 2, .ilit 2),
    (.kint 1, .ilit 3)])) []).bind D2Res.asItems = some [(.kint 1, 3), (.kint 2, 2)]
#guard (evalD2js (.getDNode (.dlit [(.kint 1, .ilit 10)]) (.kstr [49])
    (.ilit 99)) []).bind D2Res.asInt = some 99
#guard (evalD2js (.getDNode (.dlit [(.kint 1, .ilit 10)]) (.kstr [49])
    (.memNode (.dlit []) (.kint 2))) []).bind D2Res.asBool = some false
#guard (evalD2js (.memNode (.dlit [(.kint 1, .ilit 10)]) (.kstr [49])) []).bind D2Res.asBool
    = some false
#guard (evalD2js (.lenNode (.dvar "d"))
    [("d", [(.kint 1, 10), (.kstr [49], 20)])]).bind D2Res.asInt = some 2
#guard (evalD2js (.getNode (.dlit [(.kint 1, .ilit 10)]) (.kint 3)) []).isNone
#guard (evalD2js (.getNode (.dlit [(.kint 0, .ifdiv (.ilit (-7)) (.ilit 2))])
    (.kint 0)) []).bind D2Res.asInt = some (-4)
#guard (evalD2js (.valuesNode (.dlit [(.kint 0, .ifdiv (.ilit (-7)) (.ilit 2))]))
    []).bind D2Res.asVals = some [-4]
#guard (evalD2js (.getDNode (.dlit []) (.kint 1)
    (.ifdiv (.ilit (-7)) (.ilit 2))) []).bind D2Res.asInt = some (-4)
#guard (evalD2js (.ifdiv (.lenNode (.dlit [(.kint 1, .ilit 10)])) (.ilit (-2)))
    []).bind D2Res.asInt = some (-1)
#guard (evalD2js (.ifdiv (.ilit 7) (.memNode (.dlit [(.kint 1, .ilit 10)])
    (.kstr [49]))) []).isNone
#guard (evalD2js (.ifdiv (.keysNode (.dlit [(.kint 1, .ilit 10)])) (.ilit 2)) []).isNone
#guard (evalD2js (.ifdiv (.ilit 1) (.ilit 0)) []).isNone

mutual
/-- Expression-side dict-method preservation worker (mutual with
    `preservationD2s'tgt`): binds the INDEPENDENT target under the shipped
    lowering to the Python reference `evalD2 false` — real mutual structural
    induction, not a flag-vs-flag identity. The `.ifdiv` arm is closed by
    `jsFdiv_eq_fdiv` on the `y ≠ 0` branch after casing EVERY `D2Res`
    constructor through `asArith` on both operands; every other arm agrees
    because both sides independently run the SAME faithful CPython-pinned
    pure-data method surface after the sub-evals are bound by the IH. -/
private theorem preservationD2'tgt (e : D2Exp) (env : D2Env) :
    evalD2tgt jsELowering e env = evalD2 false e env := by
  match e with
  | .ilit n => simp only [evalD2tgt, evalD2]
  | .ifdiv a b =>
    simp only [evalD2tgt, evalD2, preservationD2'tgt a env, preservationD2'tgt b env]
    cases evalD2 false a env with
    | none => rfl
    | some ra =>
      cases evalD2 false b env with
      | none => rfl
      | some rb =>
        -- every D2Res constructor flows through asArith on BOTH sides:
        -- rint passes, rbool coerces (bool-int identity), rdict/rkeys/
        -- rvals/ritems are TypeError → none — no shared-wrong non-int arm.
        simp only [Option.bind]
        cases ra.asArith with
        | none => rfl
        | some x =>
          cases rb.asArith with
          | none => rfl
          | some y =>
            by_cases hy : y = 0
            · simp [hy]
            · simp [hy, jsELowering, jsFdiv_eq_fdiv x y hy]
  | .dlit entries => simp only [evalD2tgt, evalD2, preservationD2s'tgt entries env]
  | .dvar s => simp only [evalD2tgt, evalD2]
  | .getNode d k => simp only [evalD2tgt, evalD2, preservationD2'tgt d env]
  | .getDNode d k dflt =>
      simp only [evalD2tgt, evalD2, preservationD2'tgt d env, preservationD2'tgt dflt env]
  | .memNode d k => simp only [evalD2tgt, evalD2, preservationD2'tgt d env]
  | .lenNode d => simp only [evalD2tgt, evalD2, preservationD2'tgt d env]
  | .keysNode d => simp only [evalD2tgt, evalD2, preservationD2'tgt d env]
  | .valuesNode d => simp only [evalD2tgt, evalD2, preservationD2'tgt d env]
  | .itemsNode d => simp only [evalD2tgt, evalD2, preservationD2'tgt d env]
termination_by sizeOf e
decreasing_by all_goals (simp_wf <;> omega)

/-- Entry-list preservation worker (mutual with `preservationD2'tgt`). -/
private theorem preservationD2s'tgt (es : List (DKey × D2Exp)) (env : D2Env) :
    evalD2stgt jsELowering es env = evalD2s false es env := by
  match es with
  | [] => simp only [evalD2stgt, evalD2s]
  | (k, e) :: rest =>
      simp only [evalD2stgt, evalD2s, preservationD2'tgt e env, preservationD2s'tgt rest env]
termination_by sizeOf es
decreasing_by all_goals (simp_wf <;> omega)
end

/-- **Dict-method preservation (Tier-3 wave 20 / C1-rollout wave 16,
    re-architected).** The INDEPENDENT compiled target under the shipped
    lowering computes the Python reference on every dict-method expression
    and environment: dict literals over int|str keys (arbitrary int
    expressions as values, last-write-wins duplicates), the CPython method
    surface — `d[k]`, `d.get(k, default)`, `k in d`, `len(d)`, `keys()`,
    `values()`, `items()` (insertion order, first-occurrence positions) —
    and the `//` deviation threaded through dict VALUES, `get`-defaults, and
    arithmetic over method results (with the CPython bool-int coercion on
    EVERY `D2Res` constructor). Real mutual structural induction binding the
    independent target to `evalD2 false` — NOT a flag-vs-flag identity. -/
theorem preservationD2 (e : D2Exp) (env : D2Env) :
    evalD2js e env = evalD2 false e env :=
  preservationD2'tgt e env

/-- Entry-list analogue: the compiled evaluation of every dict-literal entry
    list matches the Python reference (the `evalD2s` side of the mutual
    pair). -/
theorem preservationD2s (es : List (DKey × D2Exp)) (env : D2Env) :
    evalD2sjs es env = evalD2s false es env :=
  preservationD2s'tgt es env

/-- The re-architected statement, in predicate form: the shipped lowering
    preserves dict-method expressions. Same content as `preservationD2`;
    this is the instantiation the stub litmus contrasts against. -/
theorem preservationD2_real : D2Preserves jsELowering := preservationD2

/-- Predicate form for entry lists: the instantiation
    `preservationD2s_stub_fails` contrasts against. -/
theorem preservationD2s_real : D2sPreserves jsELowering := preservationD2s

/-- **Stub litmus (wave 16, expression side).** The SAME preservation
    predicate is FALSE for the naive truncating lowering, on a deviating
    dict-methods program: `{0: -7 // 2}[0]` stores a value the stub computes
    as JS-trunc `-3` and reads it back out through the method surface, where
    Python floors to `-4` — a concrete DISCRIMINATING contradiction (floor ≠
    trunc at `-7 // 2`) the old `evalD2 true = evalD2 false` statement could
    not express. -/
theorem preservationD2_stub_fails : ¬ D2Preserves truncELowering := by
  intro h
  have hc := h (.getNode (.dlit [(.kint 0, .ifdiv (.ilit (-7)) (.ilit 2))]) (.kint 0)) []
  -- The evaluators are wf-recursive (not kernel-reducible): step them via
  -- their equation lemmas; hc reduces through
  -- `some (.rint (-3)) = some (.rint (-4))` (stub `Int.tdiv (-7) 2 = -3`
  -- vs Python `Int.fdiv (-7) 2 = -4`) to `False`.
  simp [evalD2tgt, evalD2stgt, evalD2, evalD2s, truncELowering, d2Lookup,
    D2Res.asArith] at hc

/-- **Stub litmus (wave 16, entry-list side).** The SAME entry-list
    predicate is FALSE for the truncating lowering: evaluating the literal
    entry list `{0: -7 // 2}` yields stub `[(0, -3)]` where Python yields
    `[(0, -4)]` — the dict VALUE itself diverges, before any method runs. -/
theorem preservationD2s_stub_fails : ¬ D2sPreserves truncELowering := by
  intro h
  have hc := h [(.kint 0, .ifdiv (.ilit (-7)) (.ilit 2))] []
  -- hc reduces through `some [(.kint 0, -3)] = some [(.kint 0, -4)]` to `False`.
  simp [evalD2tgt, evalD2stgt, evalD2, evalD2s, truncELowering,
    D2Res.asArith] at hc

-- The contrast, concretely — the deviation paths the stub gets wrong:
-- value path ({0: -7//2}[0]): floor -4 vs trunc -3 read back through d[k];
#guard ((evalD2js (.getNode (.dlit [(.kint 0, .ifdiv (.ilit (-7)) (.ilit 2))])
    (.kint 0)) []).bind D2Res.asInt) = some (-4)             -- real: Python floor
#guard ((evalD2tgt truncELowering (.getNode (.dlit [(.kint 0, .ifdiv (.ilit (-7)) (.ilit 2))])
    (.kint 0)) []).bind D2Res.asInt) = some (-3)             -- stub: JS trunc ✗
-- method-result path (len({1: 10}) // -2): floor -1 vs trunc 0 — the value
-- AND its truthiness differ;
#guard ((evalD2js (.ifdiv (.lenNode (.dlit [(.kint 1, .ilit 10)])) (.ilit (-2)))
    []).bind D2Res.asInt) = some (-1)                        -- real: Python floor
#guard ((evalD2tgt truncELowering (.ifdiv (.lenNode (.dlit [(.kint 1, .ilit 10)])) (.ilit (-2)))
    []).bind D2Res.asInt) = some 0                           -- stub: JS trunc ✗
-- entry-list path ({0: -7//2}): the stored VALUES diverge before any method.
#guard evalD2sjs [(.kint 0, .ifdiv (.ilit (-7)) (.ilit 2))] []
    = some [(.kint 0, -4)]                                   -- real: Python floor
#guard evalD2stgt truncELowering [(.kint 0, .ifdiv (.ilit (-7)) (.ilit 2))] []
    = some [(.kint 0, -3)]                                   -- stub: JS trunc ✗

-- SPOT 1: THE collision dict {1: -7//2, '1': 20} read at key 1, in the
-- INDEPENDENT COMPILED semantics, routed THROUGH the theorem to the Python
-- reference: -7//2 = -4 (floor; JS-trunc would give -3), and the '1' entry
-- must NOT shadow it (a stringifying dict would return 20 — jsObj_conflates).
example :
    (evalD2js (.getNode (.dlit [(.kint 1, .ifdiv (.ilit (-7)) (.ilit 2)),
        (.kstr [49], .ilit 20)]) (.kint 1)) []).bind D2Res.asInt
      = some (-4) := by
  rw [preservationD2]
  simp only [evalD2, evalD2s]
  decide

-- SPOT 2: the conflation impossibility INSTANTIATED on the pinned witness
-- {1: 10, '1': 20} — proved THROUGH jsObj_conflates, so it fails if the
-- general theorem is weakened.
example :
    jsObjLookup (.kint 1) [(.kint 1, 10), (.kstr [49], 20)]
      ≠ d2Lookup (.kint 1) [(.kint 1, 10), (.kstr [49], 20)] :=
  jsObj_conflates (.kint 1) (.kstr [49]) 10 20
    (by simp [jsKeyStr, jsNatDigits]) (by decide) (by decide)

-- SPOT 3: items-correctness THROUGH d2Items_lookup — from the concrete
-- membership fact (1, 3) ∈ {1:1, 2:2, 1:3}.items(), derive d[1] == 3
-- (last-write-wins), NOT the first-write 1.
example : d2Lookup (.kint 1) [(.kint 1, 1), (.kint 2, 2), (.kint 1, 3)] = some 3 :=
  d2Items_lookup (.kint 1) 3 _ (by decide)

-- SPOT 4: all-string agreement THROUGH jsObjLookup_eq_on_str_keys — on the
-- pure-string dict {'a': 1, 'b': 2} the naive object and Python agree
-- (pinning that the deviation is exactly the collision case).
example :
    jsObjLookup (.kstr [97]) [(.kstr [97], 1), (.kstr [98], 2)]
      = d2Lookup (.kstr [97]) [(.kstr [97], 1), (.kstr [98], 2)] :=
  jsObjLookup_eq_on_str_keys _ [97]
    (by intro p hp
        rcases List.mem_cons.mp hp with h | h
        · exact ⟨[97], by rw [h]⟩
        · rcases List.mem_cons.mp h with h2 | h2
          · exact ⟨[98], by rw [h2]⟩
          · cases h2)

-- SPOT 5: compiled keys() of the duplicate-key dict through the theorem —
-- first-occurrence positions, deduplicated.
example :
    (evalD2js (.keysNode (.dlit [(.kint 1, .ilit 1), (.kint 2, .ilit 2),
        (.kint 1, .ilit 3)])) []).bind D2Res.asKeys
      = some [.kint 1, .kint 2] := by
  rw [preservationD2]
  simp only [evalD2, evalD2s]
  decide

-- SPOT 6: d.get through an env-bound dict with the `//` axis in the
-- default: d = {'a': 1}; d.get('b', -7 // 2) == -4 (CPython floors).
example :
    (evalD2js (.getDNode (.dvar "d") (.kstr [98])
        (.ifdiv (.ilit (-7)) (.ilit 2))) [("d", [(.kstr [97], 1)])]).bind D2Res.asInt
      = some (-4) := by
  rw [preservationD2]
  simp only [evalD2]
  decide

-- SPOT 7: the deviation stored through a dict VALUE and read back out by
-- values(): {0: -7//2}.values() == [-4] — the independent compiled result
-- derived via preservationD2 (the stub cannot close this: trunc gives [-3]).
example :
    (evalD2js (.valuesNode (.dlit [(.kint 0, .ifdiv (.ilit (-7)) (.ilit 2))]))
      []).bind D2Res.asVals
      = some [-4] := by
  rw [preservationD2]
  simp only [evalD2, evalD2s]
  decide

-- (The former SPOT 8 exercised the `+` (`iadd`) arm; `iadd` was removed in
-- wave-16 iter2 — see the `D2Exp` note — because shipped `pyAdd` is unguarded.
-- The bool-int identity is still demonstrated via the guarded `//` `#guard`s
-- (`7 // ('1' in {1:10})` → ZeroDivisionError) above.)

/-- info: '_private.PythExpandVerify.0.PythExpandVerify.preservationD2'tgt' depends on axioms: [propext,
 Classical.choice,
 Quot.sound] -/
#guard_msgs in
#print axioms preservationD2'tgt

/-- info: '_private.PythExpandVerify.0.PythExpandVerify.preservationD2s'tgt' depends on axioms: [propext,
 Classical.choice,
 Quot.sound] -/
#guard_msgs in
#print axioms preservationD2s'tgt

/-- info: 'PythExpandVerify.preservationD2' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationD2

/-- info: 'PythExpandVerify.preservationD2s' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationD2s

/-- info: 'PythExpandVerify.preservationD2_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationD2_real

/-- info: 'PythExpandVerify.preservationD2s_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationD2s_real

/-- info: 'PythExpandVerify.preservationD2_stub_fails' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms preservationD2_stub_fails

/-- info: 'PythExpandVerify.preservationD2s_stub_fails' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms preservationD2s_stub_fails

/-- info: 'PythExpandVerify.jsObj_conflates' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms jsObj_conflates

/-- info: 'PythExpandVerify.jsObjLookup_eq_of_injective' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms jsObjLookup_eq_of_injective

/-- info: 'PythExpandVerify.d2Keys_nodup' depends on axioms: [propext] -/
#guard_msgs in
#print axioms d2Keys_nodup

/-- info: 'PythExpandVerify.d2Items_lookup' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms d2Items_lookup

/-- info: 'PythExpandVerify.d2Lookup_isSome_iff' depends on axioms: [propext] -/
#guard_msgs in
#print axioms d2Lookup_isSome_iff

/-! ## C1 — preservation against an INDEPENDENT target model (design + PoC)

**STATUS: the rollout this PoC motivated is COMPLETE.** The PoC below was
proved first on ONE wave (wave 20, dict reads); the C1 rollout (waves 1–16,
documented in the development history) has since re-architected EVERY flag-form
`preservation*` theorem — including this section's motivating example
`preservationD2`, re-stated above as
`evalD2js e env = evalD2 false e env` against the independent
`evalD2tgt`/`evalD2stgt` — onto the independent-target + stub-refutation
pattern this PoC establishes. The PoC theorems (`poc_preservation_real`/
`poc_preservation_stub_fails`) remain as the reference exemplar of the
pattern at its smallest scale (the dict LOOKUP alone).

### The defect (codex Category-C, finding C1 — HISTORICAL; fixed by the rollout)

Every wave's `preservation` USED TO compare the model to ITSELF.
`evalD2 (tgt : Bool)` (and its siblings `evalSg`, `evalRound`, …) branch on
`tgt` ONLY at the `//` node (`jsFdiv` vs `Int.fdiv`); the ENTIRE method
surface — `d2Lookup`, `d2Keys`, `smFindSub`, the sort comparator, `pyRound`,
`pow`, … — is computed by the SAME Lean helper on both sides. So the OLD
`preservationD2 : evalD2 true e = evalD2 false e` proved only that the `//`
deviation is handled: replacing the shipping `pyGetItem`/`PyDict`/sort helper
with a STUB would not break it, because the theorem never mentioned the
lowered/emitted form of those operations. It was a model-internal coherence
theorem, not a compilation-preservation theorem. (The `tgt = true` branches
survive only as documented-legacy; NO theorem references them.)

### The re-architecture

Preservation must bind the Python reference to an INDEPENDENT *target* model
that reflects what codegen actually EMITS (the lowered JS/helper semantics), not
a `Bool`-switched copy of the reference:

  * `evalSrc` — the CPython reference (existing helpers: `d2Lookup`, `smFindSub`,
    value-sort, banker's round, bigint pow, `Int.fdiv`).
  * `evalTgt L` — the compiled semantics, parameterized by the emitted LOWERING
    `L`. Each operation is defined by MIRRORING the shipped runtime helper
    (e.g. the Map-backed `PyDict.get`), defined SEPARATELY from the reference and
    then PROVEN equal to it (a genuine binding lemma, not `rfl`). The `L`
    parameter is what makes the statement falsifiable: `Preserves L` is FALSE for
    a wrong lowering (a stub) and TRUE for the shipped one.

The Lean target model remains a *model* of the emitted JS; binding it to the
ACTUAL shipped bytes stays the job of the differential gates (`spec_validate_*`,
`expanddiff`). What the re-architecture buys is that the preservation THEOREM is
no longer vacuous: a stub lowering breaks it. (The waves already contain the
natural "stub" target models and prove they DIVERGE — `jsObjLookup`/
`jsObj_conflates`, `js16FindSub`, JS lexicographic `sort`, `Math.round`; the flaw
was only that `preservation` was stated against the `Bool`-switched copy instead
of against the emitted lowering — now fixed on every wave by the rollout.)

### PoC (wave 20, dict read): the new statement fails for a stub, holds for real -/

/-- A dict-read LOWERING: the get-operation the target actually emits. -/
abbrev DictLookup := DKey → List (DKey × Int) → Option Int

/-- First structural-key match (a plain left scan). -/
def firstMatch (k : DKey) : List (DKey × Int) → Option Int
  | [] => none
  | (k', v) :: rest => if k' = k then some v else firstMatch k rest

/-- **Independent** model of the SHIPPED Map-backed `PyDict.get`: last write
    wins, modeled as the first match in the REVERSED insertion list — a
    DIFFERENT recursion from the reference `d2Lookup` (right-recursion + prefer
    tail). This is the "real lowering," defined by mirroring the runtime, NOT
    reused from the reference. -/
def mapLookup (k : DKey) (es : List (DKey × Int)) : Option Int :=
  firstMatch k es.reverse

/-- `firstMatch` over an append: first list wins, else the second. -/
private theorem firstMatch_append (k : DKey) (xs ys : List (DKey × Int)) :
    firstMatch k (xs ++ ys)
      = match firstMatch k xs with | some v => some v | none => firstMatch k ys := by
  induction xs with
  | nil => rfl
  | cons hd rest ih =>
    obtain ⟨k', v⟩ := hd
    simp only [List.cons_append, firstMatch]
    by_cases hk : k' = k
    · rw [if_pos hk, if_pos hk]
    · rw [if_neg hk, if_neg hk, ih]

/-- **Binding lemma (the genuine content).** The independent Map-backed model
    computes exactly the Python reference — a real induction, NOT `rfl`, because
    the two functions are defined by different recursions. -/
theorem mapLookup_eq_d2Lookup (k : DKey) (es : List (DKey × Int)) :
    mapLookup k es = d2Lookup k es := by
  induction es with
  | nil => rfl
  | cons hd rest ih =>
    obtain ⟨k', v⟩ := hd
    simp only [mapLookup, List.reverse_cons] at ih ⊢
    rw [firstMatch_append]
    simp only [ih, d2Lookup, firstMatch]

/-- The compiled dict-read under lowering `L`. -/
def pocReadTgt (L : DictLookup) (es : List (DKey × Int)) (k : DKey) : Option Int := L k es

/-- The Python-reference dict-read. -/
def pocReadSrc (es : List (DKey × Int)) (k : DKey) : Option Int := d2Lookup k es

/-- Preservation for a chosen lowering `L` — the re-architected statement (ranges
    over the lowering, so a wrong `L` makes it FALSE). -/
def PocPreserves (L : DictLookup) : Prop :=
  ∀ es k, pocReadTgt L es k = pocReadSrc es k

/-- **PoC (a): holds for the REAL lowering.** The shipped Map-backed dict
    preserves the Python reference — via the binding lemma, not `rfl`. -/
theorem poc_preservation_real : PocPreserves mapLookup := by
  intro es k
  simp only [pocReadTgt, pocReadSrc]
  exact mapLookup_eq_d2Lookup k es

/-- **PoC (b): FAILS for a stubbed lowering.** Instantiating the SAME statement
    with the naive stringifying-object lowering (`jsObjLookup`) is provably
    FALSE — on the collision dict `{1: 10, '1': 20}` it reads `20` where Python
    reads `10`. The OLD flag-form `evalD2 true = evalD2 false` could not express
    this contrast (both sides hardcoded `d2Lookup`); the re-architected statement
    can. This is the non-vacuity the finding demands. -/
theorem poc_preservation_stub_fails : ¬ PocPreserves jsObjLookup := by
  intro h
  have hc := h [(.kint 1, 10), (.kstr [49], 20)] (.kint 1)
  simp only [pocReadTgt, pocReadSrc] at hc
  exact jsObj_conflates (.kint 1) (.kstr [49]) 10 20
    (by simp [jsKeyStr, jsNatDigits]) (by decide) (by decide) hc

-- The stub really is a plausible lowering (a JS object) that a naive compiler
-- WOULD emit — its divergence is the shipped-necessity argument (`jsObj_conflates`).
#guard pocReadTgt mapLookup [(.kint 1, 10), (.kstr [49], 20)] (.kint 1) = some 10  -- real ✓
#guard pocReadTgt jsObjLookup [(.kint 1, 10), (.kstr [49], 20)] (.kint 1) = some 20 -- stub ✗

/-- info: 'PythExpandVerify.mapLookup_eq_d2Lookup' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms mapLookup_eq_d2Lookup

/-- info: 'PythExpandVerify.poc_preservation_real' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms poc_preservation_real

/-- info: 'PythExpandVerify.poc_preservation_stub_fails' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms poc_preservation_stub_fails

/-! ## Preservation lattice v2 — `PyResult` carrier + error-KIND (C4) + type-tag (C2/D1) PoC (arithmetic fragment)

The 16 `preservationX` theorems above return `Option Int`-shaped carriers where
BOTH div-by-zero and unbound-var collapse to `none`: they prove error-OCCURRENCE
(C3, "errors iff errors") but are structurally BLIND to error-KIND (C4) — they
cannot tell `TypeError` from `ZeroDivisionError`. This section upgrades the
carrier to a typed result (`PyResult`, design: the design notes §1) and
proves ONE arithmetic fragment's preservation as the conjunction
**C1 (value) ∧ C2 (type-tag) ∧ C3 (error-occurrence) ∧ C4 (error-kind)** in a
single statement, with a discriminating stub per lattice axis — including two
wrong-error-KIND stubs that get every VALUE right yet raise the wrong exception
class, and a wrong-TAG (D1-collapsing) stub that gets every value right mod ρ
yet loses the float type — plus two separation lemmas: the `eraseKind`
blindness lemma (the old carrier provably CANNOT see the C4 defects) and the
ρ/tag split (`preservationA_rho_holds_d1` ∧ `preservationA_tagStub_fails`:
C1-mod-ρ provably cannot see the C2/D1 defect). Together: each added
projection is real, independently-falsifiable strength.

Same independent-target + stub-litmus discipline as wave 1
(`preservationE`/`preservationE_stub_fails` above); reuses `jsFdiv` /
`jsFdiv_eq_fdiv` and `Env`. Purely additive; no existing wave is touched. -/

/-- CPython exception CLASSES (kind only — message TEXT is out of scope by
    design; see the exception registry, the design notes §7). -/
inductive Exc where
  | typeError | valueError | zeroDiv | nameError | indexError | keyError
  | overflow | attributeError | stopIteration | custom (name : String)
  deriving DecidableEq, Repr

/-- The upgraded result carrier: success with a value, or an error WITH ITS
    KIND. Replaces the monolith's `Option` (where the kind is lost).
    `DecidableEq` is required for the `decide`-driven stub refutations and the
    `#guard` pins below (the brief's carrier plus the derivation the stubs
    force). Divergence stays fuel-modeled (C6), not carried here. -/
inductive PyResult (α : Type) where
  | ok (v : α) | err (e : Exc)
  deriving DecidableEq, Repr

/-- C3 projection: did an error occur (kind forgotten)? -/
def PyResult.isErr {α : Type} : PyResult α → Bool
  | .err _ => true | .ok _ => false

/-- Projection back to the OLD monolith carrier (`Option`): keeps the value,
    ERASES the error kind. Load-bearing for the blindness lemma
    (`eraseKind_blind_to_zeroKind`): a statement over `eraseKind` images is
    exactly what the pre-lattice `Option`-carrier theorems could express. -/
def PyResult.eraseKind {α : Type} : PyResult α → Option α
  | .ok v => some v | .err _ => none

/-- Map over the success value (errors untouched). Used to CHARACTERIZE the
    D1-collapsing lowering (`evalAtgt_d1_collapse`): the compiled result is
    exactly the reference result with each success value re-represented. -/
def PyResult.mapOk {α : Type} (f : α → α) : PyResult α → PyResult α
  | .ok v => .ok (f v) | .err e => .err e

/-- Minimal typed value domain. `pstr` exists solely to WITNESS a genuine
    CPython `TypeError` on an arithmetic operand — no string semantics beyond
    that are modeled (simplification, stated). `pfloat n` models the WHOLE
    float `n.0` as its integer value under a float TYPE TAG — exactly D1's
    domain: whole-float vs int is a type-not-value fact. NON-whole floats are
    OUT of this fragment (stated simplification): they are not the D1 case,
    and modeling them would drag in IEEE `Float`, which has no `DecidableEq`
    (NaN) and would break every `decide`-driven refutation below. -/
inductive PVal where
  | pint (n : Int) | pfloat (n : Int) | pstr (s : String)
  deriving DecidableEq, Repr

/-- Python type tags for the fragment's values — the C2 (type/representation)
    observation. -/
inductive PTag where
  | tint | tfloat | tstr
  deriving DecidableEq, Repr

/-- The C2 projection on values: which Python type a value has. -/
def typeTag : PVal → PTag
  | .pint _ => .tint | .pfloat _ => .tfloat | .pstr _ => .tstr

/-- **The D1 collapse** (exception registry, the design notes §7):
    a whole float loses its tag and shares int's untagged JS `Number`
    representation. Value-preserving (mod ρ, `valRho` below), tag-destroying —
    a faithful model of what shipped PythScribe DOES on whole floats. -/
def d1c : PVal → PVal
  | .pfloat n => .pint n | v => v

/-- The C4-PoC arithmetic fragment. Deliberately SMALL: only ops where a `str`
    operand is UNAMBIGUOUSLY a CPython `TypeError` — `-` and `//`. We do NOT
    include `+`/`*`/`<`: `"a"+"b"`, `"a"*3`, `"a"<"b"` are all VALID Python, so
    adding them with a blanket `TypeError` would mis-model CPython (F9). -/
inductive AExp where
  | lit (v : PVal)
  | var (s : String)
  | sub (a b : AExp)      -- "a" - x  and  x - "a"  are TypeError; int-int ok
  | fdiv (a b : AExp)     -- "a" // x TypeError; int//0 ZeroDivisionError; int//int floor
  deriving Repr

/-- **Reference (CPython) semantics** over the typed carrier. Faithfulness
    notes (each pinned by a `#guard` below):
    * LEFT-TO-RIGHT: the left operand is evaluated first and its error
      short-circuits (Python evaluation order).
    * Any `str` operand to `-`/`//` → `TypeError` (CPython binary-op dispatch).
    * In `//`, the TYPE check precedes the ZERO check: `"a" // 0` is
      `TypeError`, NOT `ZeroDivisionError` — faithful to CPython, where the
      zero check lives INSIDE `int.__floordiv__`, which is never reached when
      operand dispatch fails. A real C4/C5 semantic ordering choice.
    * Unbound variable → `NameError`. Environments bind ints only (the
      existing `Env`); `str`/float values enter via literals — a stated
      simplification (variables are int-valued in this fragment).
    * FLOAT TAG PROPAGATION (CPython numeric coercion): any `float` operand
      makes the result a `float` (`2.0 - 1 = 1.0`, `-7.0 // 2 = -4.0`);
      arithmetic on the Int carrier is faithful BECAUSE the fragment's floats
      are whole (`Int.fdiv` = float floor-division on whole operands).
      `float // 0` is `ZeroDivisionError` like the int case (kind, not
      message, is modeled). -/
def evalA : AExp → Env → PyResult PVal
  | .lit v, _   => .ok v
  | .var s, env => match env.get s with
      | some n => .ok (.pint n)
      | none => .err .nameError
  | .sub a b, env => match evalA a env with
      | .err e => .err e                       -- left operand error short-circuits
      | .ok va => match evalA b env with
          | .err e => .err e
          | .ok vb => match va, vb with
              | .pint x, .pint y => .ok (.pint (x - y))
              | .pint x, .pfloat y => .ok (.pfloat (x - y))
              | .pfloat x, .pint y => .ok (.pfloat (x - y))
              | .pfloat x, .pfloat y => .ok (.pfloat (x - y))
              | _, _ => .err .typeError        -- any str operand → TypeError
  | .fdiv a b, env => match evalA a env with
      | .err e => .err e
      | .ok va => match evalA b env with
          | .err e => .err e
          | .ok vb => match va, vb with
              | .pint x, .pint y =>
                  if y = 0 then .err .zeroDiv else .ok (.pint (Int.fdiv x y))
              | .pint x, .pfloat y =>
                  if y = 0 then .err .zeroDiv else .ok (.pfloat (Int.fdiv x y))
              | .pfloat x, .pint y =>
                  if y = 0 then .err .zeroDiv else .ok (.pfloat (Int.fdiv x y))
              | .pfloat x, .pfloat y =>
                  if y = 0 then .err .zeroDiv else .ok (.pfloat (Int.fdiv x y))
              | _, _ => .err .typeError        -- type check PRECEDES zero check

-- F9 faithfulness pins — the REFERENCE matches CPython on every error site
-- and on the evaluation-order choices (verified against CPython 3.12):
#guard evalA (.fdiv (.lit (.pint 2)) (.lit (.pint 0))) [] = .err .zeroDiv        -- 2 // 0
#guard evalA (.fdiv (.lit (.pstr "a")) (.lit (.pint 1))) [] = .err .typeError    -- "a" // 1
#guard evalA (.fdiv (.lit (.pstr "a")) (.lit (.pint 0))) [] = .err .typeError    -- "a" // 0: TYPE before ZERO
#guard evalA (.sub (.lit (.pstr "a")) (.lit (.pint 1))) [] = .err .typeError     -- "a" - 1
#guard evalA (.sub (.lit (.pint 2)) (.lit (.pstr "a"))) [] = .err .typeError     -- 2 - "a"
#guard evalA (.var "x") [] = .err .nameError                                     -- unbound var
#guard evalA (.fdiv (.lit (.pint (-7))) (.lit (.pint 2))) [] = .ok (.pint (-4))  -- floor, not trunc
-- float-tag propagation (CPython: any float operand → float result; whole floats):
#guard evalA (.sub (.lit (.pfloat 2)) (.lit (.pint 1))) [] = .ok (.pfloat 1)        -- 2.0 - 1 = 1.0
#guard evalA (.sub (.lit (.pint 1)) (.lit (.pfloat 2))) [] = .ok (.pfloat (-1))     -- 1 - 2.0 = -1.0
#guard evalA (.fdiv (.lit (.pfloat (-7))) (.lit (.pint 2))) [] = .ok (.pfloat (-4)) -- -7.0 // 2 = -4.0 (float floor)
#guard evalA (.fdiv (.lit (.pfloat 2)) (.lit (.pint 0))) [] = .err .zeroDiv         -- 2.0 // 0
#guard evalA (.sub (.lit (.pstr "a")) (.lit (.pfloat 1))) [] = .err .typeError      -- "a" - 1.0
-- left-to-right error ORDER (C5-adjacent, carried by the kind): the LEFT
-- error wins even when the right side would raise a different kind.
#guard evalA (.sub (.fdiv (.lit (.pint 1)) (.lit (.pint 0))) (.var "x")) [] = .err .zeroDiv
#guard evalA (.sub (.var "x") (.fdiv (.lit (.pint 1)) (.lit (.pint 0)))) [] = .err .nameError

/-- An arithmetic LOWERING: everything the emitted JS gets to choose for this
    fragment — the `//` value computation (C1 axis) and the exception KINDS
    raised at the two error sites (C4 axis). The preservation predicate ranges
    over this, so a wrong choice on ANY axis falsifies it. `nameError` is NOT
    a field: unbound-variable is scope resolution, not a codegen lowering
    choice, so it is not stub-able here. -/
structure ArithLowering where
  fdiv      : Int → Int → Int  -- C1 value axis: how the emitted JS lowers //
  zeroExc   : Exc              -- C4 axis: exception kind emitted for int//0
  typeExc   : Exc              -- C4 axis: exception kind emitted for a non-numeric operand
  reprValue : PVal → PVal      -- C2 axis (F10 knob): how the emitted code REPRESENTS a produced value

/-- The tag-faithful IDEALIZED shipped lowering: floor-corrected `//`
    (`jsFdiv`), `ZeroDivisionError` on `// 0`, `TypeError` on a non-numeric
    operand, values represented AS TAGGED (`reprValue = id`). The REAL shipped
    compiler deviates from this on exactly one axis — D1: it behaves like
    `d1CollapseAL` below on whole floats. -/
def jsAL : ArithLowering := ⟨jsFdiv, .zeroDiv, .typeError, id⟩

/-- C1-wrong stub: raw JS `Math.trunc(x / y)` — truncates instead of floors
    (kinds and representation correct). -/
def truncAL : ArithLowering := ⟨Int.tdiv, .zeroDiv, .typeError, id⟩

/-- C4-wrong stub (ZERO site): every VALUE correct (`jsFdiv`), but raises
    `ValueError` where Python raises `ZeroDivisionError`. Invisible to the old
    `Option` carrier — see `eraseKind_blind_to_zeroKind`. -/
def zeroKindAL : ArithLowering := ⟨jsFdiv, .valueError, .typeError, id⟩

/-- C4-wrong stub (TYPE site): every VALUE correct, but raises `ValueError`
    where Python raises `TypeError`. -/
def typeKindAL : ArithLowering := ⟨jsFdiv, .zeroDiv, .valueError, id⟩

/-- **C2-wrong lowering — AND the real shipped D1 behavior** (dual role,
    stated): `jsAL` with the D1 representation collapse (`= { jsAL with
    reprValue := d1c }`). Every value survives mod ρ
    (`preservationA_rho_holds_d1`), every error site and kind is untouched,
    but the float TAG is gone (`preservationA_tagStub_fails`). This is
    simultaneously (a) the discriminating C2 stub of the lattice and (b) a
    faithful model of the shipped whole-float representation (D1, registry
    §7) — the lattice's honesty machinery: the deviation the compiler
    actually has is exactly the projection this lowering fails. -/
def d1CollapseAL : ArithLowering := ⟨jsFdiv, .zeroDiv, .typeError, d1c⟩

/-- **Independent target evaluator**: the compiled program's semantics under
    lowering `L`. A SEPARATE recursion (not a flag on `evalA`); identical
    structure, but the `//` value, the `int//0` kind and the type-error kind
    are the LOWERING'S (`L.fdiv`/`L.zeroExc`/`L.typeExc`), and every produced
    success value passes through the lowering's REPRESENTATION
    (`L.reprValue`, post-composed on each `.ok` — the C2/D1 knob; `id` for
    the tag-faithful lowerings, `d1c` for the shipped collapse, so
    intermediate values are collapsed too, as in the real runtime).
    `nameError` stays hardcoded (not a lowering choice — see
    `ArithLowering`). -/
def evalAtgt (L : ArithLowering) : AExp → Env → PyResult PVal
  | .lit v, _   => .ok (L.reprValue v)
  | .var s, env => match env.get s with
      | some n => .ok (L.reprValue (.pint n))
      | none => .err .nameError
  | .sub a b, env => match evalAtgt L a env with
      | .err e => .err e
      | .ok va => match evalAtgt L b env with
          | .err e => .err e
          | .ok vb => match va, vb with
              | .pint x, .pint y => .ok (L.reprValue (.pint (x - y)))
              | .pint x, .pfloat y => .ok (L.reprValue (.pfloat (x - y)))
              | .pfloat x, .pint y => .ok (L.reprValue (.pfloat (x - y)))
              | .pfloat x, .pfloat y => .ok (L.reprValue (.pfloat (x - y)))
              | _, _ => .err L.typeExc
  | .fdiv a b, env => match evalAtgt L a env with
      | .err e => .err e
      | .ok va => match evalAtgt L b env with
          | .err e => .err e
          | .ok vb => match va, vb with
              | .pint x, .pint y =>
                  if y = 0 then .err L.zeroExc else .ok (L.reprValue (.pint (L.fdiv x y)))
              | .pint x, .pfloat y =>
                  if y = 0 then .err L.zeroExc else .ok (L.reprValue (.pfloat (L.fdiv x y)))
              | .pfloat x, .pint y =>
                  if y = 0 then .err L.zeroExc else .ok (L.reprValue (.pfloat (L.fdiv x y)))
              | .pfloat x, .pfloat y =>
                  if y = 0 then .err L.zeroExc else .ok (L.reprValue (.pfloat (L.fdiv x y)))
              | _, _ => .err L.typeExc

/-- The compiled fragment semantics: the independent target under the SHIPPED
    lowering. -/
abbrev evalAjs : AExp → Env → PyResult PVal := evalAtgt jsAL

/-- Preservation as a predicate OVER the lowering — the SAME predicate is
    proved for the shipped lowering and refuted for all three stubs. -/
def APreserves (L : ArithLowering) : Prop := ∀ e env, evalAtgt L e env = evalA e env

/-- **C1 ∧ C2 ∧ C3 ∧ C4 in ONE statement (the lattice-v2 core claim).** A
    `PyResult PVal` equation is SIMULTANEOUSLY:
    * **C1 (value)** — both `.ok` ⇒ equal values;
    * **C2 (type/representation)** — `PVal` is TAGGED, so equal `.ok` values
      have equal `typeTag`s (int vs float vs str); the tag axis is made
      independently falsifiable by `preservationA_tag` /
      `preservationA_tagStub_fails` below;
    * **C3 (error-occurrence)** — `.isErr` agrees, since `.ok _ ≠ .err _`
      constructor-wise: no silent success where Python errors, no spurious
      error where Python succeeds;
    * **C4 (error-KIND)** — both `.err` ⇒ equal `Exc` classes — the projection
      the `Option`-carrier monolith could not even STATE.
    Holds for the tag-faithful IDEALIZED `jsAL`; the real shipped D1 deviation
    is treated honestly by the ρ/tag split below. Real structural induction
    (not `rfl`): the `//` value arm needs `jsFdiv_eq_fdiv`. -/
theorem preservationA (e : AExp) (env : Env) : evalAjs e env = evalA e env := by
  induction e generalizing env with
  | lit v => rfl
  | var s => rfl
  | sub a b iha ihb =>
    simp only [evalAtgt, evalA, iha, ihb]
    cases evalA a env with
    | err e => rfl
    | ok va =>
      cases evalA b env with
      | err e => rfl
      | ok vb => cases va <;> cases vb <;> rfl
  | fdiv a b iha ihb =>
    simp only [evalAtgt, evalA, iha, ihb]
    cases evalA a env with
    | err e => rfl
    | ok va =>
      cases evalA b env with
      | err e => rfl
      | ok vb =>
        cases va with
        | pint x =>
          cases vb with
          | pint y =>
            by_cases hy : y = 0
            · simp [hy, jsAL]
            · simp [hy, jsAL, jsFdiv_eq_fdiv x y hy]
          | pfloat y =>
            by_cases hy : y = 0
            · simp [hy, jsAL]
            · simp [hy, jsAL, jsFdiv_eq_fdiv x y hy]
          | pstr s => rfl
        | pfloat x =>
          cases vb with
          | pint y =>
            by_cases hy : y = 0
            · simp [hy, jsAL]
            · simp [hy, jsAL, jsFdiv_eq_fdiv x y hy]
          | pfloat y =>
            by_cases hy : y = 0
            · simp [hy, jsAL]
            · simp [hy, jsAL, jsFdiv_eq_fdiv x y hy]
          | pstr s => rfl
        | pstr s => cases vb <;> rfl

/-- The predicate-form instantiation the three stub litmuses contrast against. -/
theorem preservationA_real : APreserves jsAL := preservationA

/-- **Stub litmus, C1 (value) axis.** `-7 // 2` → floor `-4` (Python) vs
    trunc `-3` (naive JS) — same refutation shape as wave 1, now over the
    typed carrier. -/
theorem preservationA_valueStub_fails : ¬ APreserves truncAL := by
  intro h
  have hc := h (.fdiv (.lit (.pint (-7))) (.lit (.pint 2))) []
  -- hc reduces to `.ok (.pint (-3)) = .ok (.pint (-4))`.
  exact absurd hc (by decide)

/-- **Stub litmus, C4 (error-kind) axis, ZERO site.** `1 // 0` → Python
    `.err .zeroDiv` vs stub `.err .valueError`. The stub is VALUE-correct
    everywhere (`jsFdiv`) — only the exception CLASS is wrong, which is
    exactly what the old `Option` carrier could not see
    (`eraseKind_blind_to_zeroKind` below proves that blindness). -/
theorem preservationA_zeroKindStub_fails : ¬ APreserves zeroKindAL := by
  intro h
  have hc := h (.fdiv (.lit (.pint 1)) (.lit (.pint 0))) []
  -- hc reduces to `.err .valueError = .err .zeroDiv`.
  exact absurd hc (by decide)

/-- **Stub litmus, C4 (error-kind) axis, TYPE site.** `"a" // 1` → Python
    `.err .typeError` vs stub `.err .valueError` (again VALUE-correct). -/
theorem preservationA_typeKindStub_fails : ¬ APreserves typeKindAL := by
  intro h
  have hc := h (.fdiv (.lit (.pstr "a")) (.lit (.pint 1))) []
  -- hc reduces to `.err .valueError = .err .typeError`.
  exact absurd hc (by decide)

-- The three contrasts, concretely (each stub is a plausible naive emission,
-- and each diverges from the reference on its witness — discriminating pins):
#guard evalAjs (.fdiv (.lit (.pint (-7))) (.lit (.pint 2))) [] = .ok (.pint (-4))            -- real: floor
#guard evalAtgt truncAL (.fdiv (.lit (.pint (-7))) (.lit (.pint 2))) [] = .ok (.pint (-3))   -- C1 stub ✗
#guard evalAjs (.fdiv (.lit (.pint 1)) (.lit (.pint 0))) [] = .err .zeroDiv                  -- real kind
#guard evalAtgt zeroKindAL (.fdiv (.lit (.pint 1)) (.lit (.pint 0))) [] = .err .valueError   -- C4 stub ✗
#guard evalAjs (.fdiv (.lit (.pstr "a")) (.lit (.pint 1))) [] = .err .typeError              -- real kind
#guard evalAtgt typeKindAL (.fdiv (.lit (.pstr "a")) (.lit (.pint 1))) [] = .err .valueError -- C4 stub ✗

/-- SPOT (through the theorem, not by evaluation): the compiled `x // 2` at
    `x = -7` floors to `-4`, via `preservationA` — fails if the statement is
    weakened. Environment lookup exercised. -/
example : evalAjs (.fdiv (.var "x") (.lit (.pint 2))) [("x", -7)] = .ok (.pint (-4)) := by
  rw [preservationA]; rfl

/-- SPOT (C4, through the theorem): the compiled `q // 0` raises
    `ZeroDivisionError` — the KIND, not just "an error" — via `preservationA`. -/
example : evalAjs (.fdiv (.var "q") (.lit (.pint 0))) [("q", 5)] = .err .zeroDiv := by
  rw [preservationA]; rfl

/-- SPOT (C4, through the theorem): the compiled `2 - "a"` raises `TypeError`. -/
example : evalAjs (.sub (.lit (.pint 2)) (.lit (.pstr "a"))) [] = .err .typeError := by
  rw [preservationA]; rfl

/-- **THE BLINDNESS LEMMA — C4 is a strictly stronger, independently-falsifiable
    projection.** Under `eraseKind` (the projection back to the OLD `Option`
    monolith carrier) the wrong-zero-kind lowering matches the reference
    EVERYWHERE: values agree (`jsFdiv = Int.fdiv`), and every error — of
    whatever kind — collapses to `none` on both sides. So the zeroKind
    lowering PASSES the old `Option`-carrier statement
    (`eraseKind_blind_to_zeroKind`, this lemma) yet FAILS the `PyResult`
    statement (`preservationA_zeroKindStub_fails`): a C1∧C3-only theorem — the
    entire pre-lattice monolith — would MISS this defect; only the
    C4-carrying `preservationA` catches it. -/
theorem eraseKind_blind_to_zeroKind (e : AExp) (env : Env) :
    (evalAtgt zeroKindAL e env).eraseKind = (evalA e env).eraseKind := by
  induction e generalizing env with
  | lit v => rfl
  | var s => rfl
  | sub a b iha ihb =>
    have ha := iha env
    have hb := ihb env
    simp only [evalAtgt, evalA]
    cases hA : evalAtgt zeroKindAL a env with
    | err e1 =>
      cases hA' : evalA a env with
      | err e2 => rfl                                   -- both err → none = none (kinds erased)
      | ok v => rw [hA, hA'] at ha; simp [PyResult.eraseKind] at ha
    | ok va =>
      cases hA' : evalA a env with
      | err e2 => rw [hA, hA'] at ha; simp [PyResult.eraseKind] at ha
      | ok va' =>
        rw [hA, hA'] at ha
        simp only [PyResult.eraseKind, Option.some.injEq] at ha
        subst ha
        cases hB : evalAtgt zeroKindAL b env with
        | err e1 =>
          cases hB' : evalA b env with
          | err e2 => rfl
          | ok w => rw [hB, hB'] at hb; simp [PyResult.eraseKind] at hb
        | ok vb =>
          cases hB' : evalA b env with
          | err e2 => rw [hB, hB'] at hb; simp [PyResult.eraseKind] at hb
          | ok vb' =>
            rw [hB, hB'] at hb
            simp only [PyResult.eraseKind, Option.some.injEq] at hb
            subst hb
            cases va <;> cases vb <;> rfl
  | fdiv a b iha ihb =>
    have ha := iha env
    have hb := ihb env
    simp only [evalAtgt, evalA]
    cases hA : evalAtgt zeroKindAL a env with
    | err e1 =>
      cases hA' : evalA a env with
      | err e2 => rfl
      | ok v => rw [hA, hA'] at ha; simp [PyResult.eraseKind] at ha
    | ok va =>
      cases hA' : evalA a env with
      | err e2 => rw [hA, hA'] at ha; simp [PyResult.eraseKind] at ha
      | ok va' =>
        rw [hA, hA'] at ha
        simp only [PyResult.eraseKind, Option.some.injEq] at ha
        subst ha
        cases hB : evalAtgt zeroKindAL b env with
        | err e1 =>
          cases hB' : evalA b env with
          | err e2 => rfl
          | ok w => rw [hB, hB'] at hb; simp [PyResult.eraseKind] at hb
        | ok vb =>
          cases hB' : evalA b env with
          | err e2 => rw [hB, hB'] at hb; simp [PyResult.eraseKind] at hb
          | ok vb' =>
            rw [hB, hB'] at hb
            simp only [PyResult.eraseKind, Option.some.injEq] at hb
            subst hb
            cases va with
            | pint x =>
              cases vb with
              | pint y =>
                by_cases hy : y = 0
                · simp [hy, PyResult.eraseKind]         -- err valueError / err zeroDiv → none = none
                · simp [hy, zeroKindAL, PyResult.eraseKind, jsFdiv_eq_fdiv x y hy]
              | pfloat y =>
                by_cases hy : y = 0
                · simp [hy, PyResult.eraseKind]
                · simp [hy, zeroKindAL, PyResult.eraseKind, jsFdiv_eq_fdiv x y hy]
              | pstr s => rfl
            | pfloat x =>
              cases vb with
              | pint y =>
                by_cases hy : y = 0
                · simp [hy, PyResult.eraseKind]
                · simp [hy, zeroKindAL, PyResult.eraseKind, jsFdiv_eq_fdiv x y hy]
              | pfloat y =>
                by_cases hy : y = 0
                · simp [hy, PyResult.eraseKind]
                · simp [hy, zeroKindAL, PyResult.eraseKind, jsFdiv_eq_fdiv x y hy]
              | pstr s => rfl
            | pstr s => cases vb <;> rfl

-- The blindness, concretely: on the zeroKind witness the ERASED projections
-- agree (both `none`) — the old carrier records "an error occurred", nothing more.
#guard (evalAtgt zeroKindAL (.fdiv (.lit (.pint 1)) (.lit (.pint 0))) []).eraseKind = (none : Option PVal)
#guard (evalA (.fdiv (.lit (.pint 1)) (.lit (.pint 0))) []).eraseKind = (none : Option PVal)

/-! ### C2 (type/representation) conjunct + the D1 separation

`PVal` is TAGGED, so the strict `preservationA` equation above is already
C1∧C2∧C3∧C4 for the idealized tag-faithful `jsAL`. This subsection makes the
C2(tag) axis INDEPENDENTLY falsifiable — `reprValue` is the lowering knob —
and proves the SEPARATION: the D1-collapsing lowering `d1CollapseAL` (which
is what shipped PythScribe actually DOES on whole floats) preserves
value-mod-ρ, error-occurrence and error-kind EVERYWHERE
(`preservationA_rho_holds_d1`) yet FAILS tag-preservation
(`preservationA_tagStub_fails`). So C2(tag) is strictly stronger than, and
independently falsifiable from, C1-mod-ρ — the C2 analogue of
`eraseKind_blind_to_zeroKind`, and the honest formal home of D1: the shipped
deviation is exactly the projection the collapse fails, nothing more. -/

/-- The C2 observation on results: the Python TYPE of a success value
    (`none` on error — error kinds are C4's business, not C2's). -/
def resultTag : PyResult PVal → Option PTag
  | .ok v => some (typeTag v) | .err _ => none

/-- The value-representation quotient ρ, D1 clause: structural equality PLUS
    `pint n ~ pfloat n` (same value, differing only in the D1-collapsed
    representation). ρ quotients REPRESENTATION only, never the value payload
    — see `preservationA_rhoStub_fails`. -/
def valRho : PVal → PVal → Bool
  | .pint a, .pint b => decide (a = b)
  | .pfloat a, .pfloat b => decide (a = b)
  | .pstr a, .pstr b => decide (a = b)
  | .pint a, .pfloat b => decide (a = b)   -- D1: whole float ≡ its int
  | .pfloat a, .pint b => decide (a = b)   -- D1 (symmetric)
  | _, _ => false

/-- Result relation mod ρ: success values compared mod the D1 quotient;
    errors must agree in KIND (C4 stays strict inside the mod-ρ statement). -/
def resultRho : PyResult PVal → PyResult PVal → Bool
  | .ok a, .ok b => valRho a b
  | .err a, .err b => decide (a = b)
  | _, _ => false

/-- C2(tag)-preservation as a predicate over the lowering. -/
def APreservesTag (L : ArithLowering) : Prop :=
  ∀ e env, resultTag (evalAtgt L e env) = resultTag (evalA e env)

/-- C1-mod-ρ ∧ C3 ∧ C4 (tags QUOTIENTED away) as a predicate over the
    lowering — the strongest statement the pre-C2 monolith could make about
    values under the D1 representation. -/
def APreservesRho (L : ArithLowering) : Prop :=
  ∀ e env, resultRho (evalAtgt L e env) (evalA e env) = true

/-- **C2 positive.** The tag-faithful idealized lowering preserves type tags —
    the C2 projection of the strict `preservationA` (tags live inside
    `PVal`). -/
theorem preservationA_tag : APreservesTag jsAL :=
  fun e env => congrArg resultTag (preservationA e env)

/-- **Stub litmus, C2 (type-tag) axis — and the REAL shipped D1 deviation.**
    Witness: the bare whole-float literal `2.0`. The D1-collapsing lowering
    reports tag `int` where CPython says `float`. Because `d1CollapseAL` is
    also a faithful model of the shipped whole-float representation, this
    refutation is not hypothetical: it is D1 itself, isolated to exactly the
    C2(tag) projection (its value-safety is `preservationA_rho_holds_d1`). -/
theorem preservationA_tagStub_fails : ¬ APreservesTag d1CollapseAL := by
  intro h
  have hc := h (.lit (.pfloat 2)) []
  -- hc reduces to `some .tint = some .tfloat`.
  exact absurd hc (by decide)

/-- **The D1 characterization.** Under the D1-collapsing lowering the compiled
    result is EXACTLY the reference result with each success value collapsed
    (`d1c`): no value is lost, no error site or kind moves — only the
    representation of successes changes. Structural induction; the `//`
    value arm is `jsFdiv_eq_fdiv`. -/
theorem evalAtgt_d1_collapse (e : AExp) (env : Env) :
    evalAtgt d1CollapseAL e env = (evalA e env).mapOk d1c := by
  induction e generalizing env with
  | lit v => rfl
  | var s =>
    simp only [evalAtgt, evalA]
    cases env.get s <;> rfl
  | sub a b iha ihb =>
    simp only [evalAtgt, evalA, iha, ihb]
    cases evalA a env with
    | err e => rfl
    | ok va =>
      cases evalA b env with
      | err e => rfl
      | ok vb => cases va <;> cases vb <;> rfl
  | fdiv a b iha ihb =>
    simp only [evalAtgt, evalA, iha, ihb]
    cases evalA a env with
    | err e => rfl
    | ok va =>
      cases evalA b env with
      | err e => rfl
      | ok vb =>
        cases va with
        | pint x =>
          cases vb with
          | pint y =>
            by_cases hy : y = 0
            · simp [hy, d1CollapseAL, PyResult.mapOk, d1c]
            · simp [hy, d1CollapseAL, PyResult.mapOk, d1c, jsFdiv_eq_fdiv x y hy]
          | pfloat y =>
            by_cases hy : y = 0
            · simp [hy, d1CollapseAL, PyResult.mapOk, d1c]
            · simp [hy, d1CollapseAL, PyResult.mapOk, d1c, jsFdiv_eq_fdiv x y hy]
          | pstr s => rfl
        | pfloat x =>
          cases vb with
          | pint y =>
            by_cases hy : y = 0
            · simp [hy, d1CollapseAL, PyResult.mapOk, d1c]
            · simp [hy, d1CollapseAL, PyResult.mapOk, d1c, jsFdiv_eq_fdiv x y hy]
          | pfloat y =>
            by_cases hy : y = 0
            · simp [hy, d1CollapseAL, PyResult.mapOk, d1c]
            · simp [hy, d1CollapseAL, PyResult.mapOk, d1c, jsFdiv_eq_fdiv x y hy]
          | pstr s => rfl
        | pstr s => cases vb <;> rfl

/-- ρ absorbs the D1 collapse: every result is ρ-related to its own collapsed
    image (values equal mod representation; errors identical). -/
theorem resultRho_mapOk_d1c (r : PyResult PVal) : resultRho (r.mapOk d1c) r = true := by
  cases r with
  | err e => simp [PyResult.mapOk, resultRho]
  | ok v => cases v <;> simp [PyResult.mapOk, resultRho, d1c, valRho]

/-- **THE SEPARATION (C2 ⟂ C1-mod-ρ).** The SAME D1-collapsing lowering that
    FAILS tag-preservation (`preservationA_tagStub_fails`) preserves
    value-mod-ρ, error-occurrence and error-kind on EVERY expression and
    environment. Together the pair proves C2(tag) is a strictly stronger,
    independently-falsifiable projection over C1-mod-ρ — exactly the added
    strength `eraseKind_blind_to_zeroKind` proved for C4 over C1∧C3 — and it
    gives D1 its honest formal statement: the shipped whole-float collapse
    loses the TAG and nothing else. -/
theorem preservationA_rho_holds_d1 : APreservesRho d1CollapseAL := by
  intro e env
  rw [evalAtgt_d1_collapse]
  exact resultRho_mapOk_d1c (evalA e env)

/-- ρ-preservation is itself falsifiable (not a tautology): the C1-wrong
    truncating lowering fails it on `-7 // 2` — `-3` vs `-4` differ mod ρ
    too, because ρ only quotients REPRESENTATION, never the value payload. -/
theorem preservationA_rhoStub_fails : ¬ APreservesRho truncAL := by
  intro h
  have hc := h (.fdiv (.lit (.pint (-7))) (.lit (.pint 2))) []
  -- hc reduces to `valRho (.pint (-3)) (.pint (-4)) = true`, i.e. `false = true`.
  exact absurd hc (by decide)

-- The D1 witness, concretely: the tag collapses (C2 ✗) while the value
-- survives mod ρ (C1-mod-ρ ✓) — on the SAME input:
#guard resultTag (evalAtgt d1CollapseAL (.lit (.pfloat 2)) []) = some .tint      -- shipped D1: int tag
#guard resultTag (evalA (.lit (.pfloat 2)) []) = some .tfloat                    -- CPython: float tag
#guard resultRho (evalAtgt d1CollapseAL (.lit (.pfloat 2)) []) (evalA (.lit (.pfloat 2)) []) = true
-- tags flow through arithmetic, and the idealized lowering keeps them:
#guard resultTag (evalAjs (.sub (.lit (.pfloat 2)) (.lit (.pint 1))) []) = some .tfloat
#guard resultTag (evalAtgt d1CollapseAL (.sub (.lit (.pfloat 2)) (.lit (.pint 1))) []) = some .tint

/-- SPOT (C2, through the theorem): the compiled `2.0 - 1` keeps the FLOAT
    tag under the tag-faithful lowering, via `preservationA_tag`. -/
example : resultTag (evalAjs (.sub (.lit (.pfloat 2)) (.lit (.pint 1))) []) = some .tfloat := by
  have h := preservationA_tag (.sub (.lit (.pfloat 2)) (.lit (.pint 1))) []
  rw [h]; rfl

/-- SPOT (D1, through the characterization theorem): under the SHIPPED
    collapse, `-7.0 // 2` still floors to `-4` — as an UNTAGGED int
    (`evalAtgt_d1_collapse`): the value is intact, only the float tag is gone
    (`preservationA_tagStub_fails`). -/
example :
    evalAtgt d1CollapseAL (.fdiv (.lit (.pfloat (-7))) (.lit (.pint 2))) [] = .ok (.pint (-4)) := by
  rw [evalAtgt_d1_collapse]; rfl

/-- info: 'PythExpandVerify.preservationA_tag' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationA_tag

/-- info: 'PythExpandVerify.preservationA_tagStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationA_tagStub_fails

/-- info: 'PythExpandVerify.evalAtgt_d1_collapse' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms evalAtgt_d1_collapse

/-- info: 'PythExpandVerify.resultRho_mapOk_d1c' depends on axioms: [propext] -/
#guard_msgs in
#print axioms resultRho_mapOk_d1c

/-- info: 'PythExpandVerify.preservationA_rho_holds_d1' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationA_rho_holds_d1

/-- info: 'PythExpandVerify.preservationA_rhoStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationA_rhoStub_fails

/-- info: 'PythExpandVerify.preservationA' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationA

/-- info: 'PythExpandVerify.preservationA_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationA_real

/-- info: 'PythExpandVerify.preservationA_valueStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationA_valueStub_fails

/-- info: 'PythExpandVerify.preservationA_zeroKindStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationA_zeroKindStub_fails

/-- info: 'PythExpandVerify.preservationA_typeKindStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationA_typeKindStub_fails

/-- info: 'PythExpandVerify.eraseKind_blind_to_zeroKind' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms eraseKind_blind_to_zeroKind

/-! ## Preservation lattice v2 — subscript fragment: THREE error kinds (C1 ∧ C3 ∧ C4)

The arith fragment above exercises C4 on `typeError`/`zeroDiv`/`nameError`.
Subscripting is where error-KIND matters most in real Python: the SAME
syntactic operation `c[k]` produces THREE distinct exception classes —
`lst[i]` out of range → `IndexError`, `d[k]` missing key → `KeyError`,
non-subscriptable container or bad key type → `TypeError`. A lowering that
raises the wrong class at any of the three sites is value-correct everywhere
(C1 ✓) and errors exactly where Python errors (C3 ✓), yet is un-Pythonic —
and provably INVISIBLE to the old `Option` carrier
(`eraseKind_blind_to_indexKind` below). This section makes C4 a genuine
3-kind discrimination: an independent target evaluator parameterized by a
`SubLowering` (all three kinds + the negative-index normalization are
falsifiable parameters — the F10 discipline: nothing the theorem universally
asserts is hardcoded in the target), one preservation theorem for the shipped
lowering, three wrong-KIND stubs (each isolating ONE kind), one wrong-VALUE
stub (C1 axis: no negative-index wrap), and the blindness lemma.

Reuses the arith section's carrier (`Exc`/`PyResult`/`.eraseKind`) verbatim.
Type names are `SubVal`/`SubExp` (wave 11 already owns `SVal`/`SExp` in this
namespace). Same independent-target + stub-litmus discipline; purely
additive; no existing wave or section is touched.

Stated simplifications (kind, not message, is modeled — as in the arith
section):
* Values are Int-only: int scalars, `List Int` lists, `List (Int × Int)`
  assoc-list dicts. No bools (CPython `bool ⊂ int`, so `[10,20][True] = 20`
  — out of this fragment's domain), no slices, no string keys, no
  hash-based dict representation.
* Dict lookup is FIRST-match on the assoc list. Faithful for dicts built
  without duplicate keys (CPython dict CONSTRUCTION dedups last-write-wins;
  construction is outside this fragment — every literal below is
  duplicate-free, matching what a constructed CPython dict looks like).
* The brief's order pin `(5)[[1][9]] → TypeError` was WRONG about CPython
  and is corrected here: CPython's BINARY_SUBSCR evaluates BOTH operand
  expressions before the subscript type-dispatch, so the key's `IndexError`
  is raised first and `(5)[[1][9]]` is `IndexError` (verified live on
  CPython 3.x; also matches the brief's own evaluator design). The
  left-to-right pins below record BOTH orders: container-EXPRESSION error
  before key-expression error, and key-expression error before the
  subscript TYPE check. -/

/-- Minimal value domain for the subscript fragment: int scalars, int
    lists, int→int assoc-list dicts. `DecidableEq` drives the
    `decide`-based stub refutations and `#guard` pins. -/
inductive SubVal where
  | sint (n : Int)
  | slist (xs : List Int)
  | sdict (ps : List (Int × Int))
  deriving DecidableEq, Repr

/-- The subscript fragment: literals and `c[k]`. No variables — literals
    suffice to witness every case (kept minimal). -/
inductive SubExp where
  | slit (v : SubVal)
  | ssub (c k : SubExp)      -- c[k]
  deriving Repr

/-- **Reference (CPython) semantics** over the typed carrier. Faithfulness
    notes (each pinned by a `#guard` below, all verified against live
    CPython):
    * LEFT-TO-RIGHT: the container EXPRESSION is evaluated first and its
      error short-circuits; the key EXPRESSION is evaluated second and its
      error short-circuits BEFORE the subscript type-dispatch (CPython
      BINARY_SUBSCR pops both operands before dispatching — so
      `(5)[[1][9]]` is `IndexError`, not `TypeError`).
    * Once both operands are values, the TYPE check precedes the
      range/key check: `[1,2][{}]` is `TypeError` (bad key type), not
      `IndexError`; `{1:10}[[]]` is `TypeError` (unhashable), not
      `KeyError`; `(5)[0]` is `TypeError` (not subscriptable).
    * List indexing wraps a negative index ONCE by `+ len` (Python), then
      bounds-checks: `[10,20,30][-1] = 30`, `[10,20][-9]` → `IndexError`
      (no double wrap).
    * Dict lookup is first-match on the assoc list (see the simplification
      note above); a missing key is `KeyError`. -/
def evalSub : SubExp → PyResult SubVal
  | .slit v => .ok v
  | .ssub c k => match evalSub c with
      | .err e => .err e                      -- container-expression error short-circuits first
      | .ok cv => match evalSub k with
          | .err e => .err e                  -- key-expression error precedes the type-dispatch
          | .ok kv => match cv, kv with
              | .slist xs, .sint i =>
                  let i' := if i < 0 then i + (xs.length : Int) else i
                  if 0 ≤ i' ∧ i' < (xs.length : Int) then .ok (.sint xs[i'.toNat]!)
                  else .err .indexError
              | .sdict ps, .sint key =>
                  match ps.find? (fun p => p.1 == key) with
                  | some p => .ok (.sint p.2)
                  | none => .err .keyError
              | _, _ => .err .typeError       -- non-subscriptable container / bad key type / unhashable

-- F9 faithfulness pins — the REFERENCE matches CPython on every value,
-- every error site/KIND, and both evaluation-order choices (each line
-- verified against live CPython):
#guard evalSub (.ssub (.slit (.slist [10, 20, 30])) (.slit (.sint 0))) = .ok (.sint 10)      -- [10,20,30][0]
#guard evalSub (.ssub (.slit (.slist [10, 20, 30])) (.slit (.sint (-1)))) = .ok (.sint 30)   -- [10,20,30][-1] (wrap)
#guard evalSub (.ssub (.slit (.slist [10, 20])) (.slit (.sint 5))) = .err .indexError        -- [10,20][5]
#guard evalSub (.ssub (.slit (.slist [10, 20])) (.slit (.sint (-9)))) = .err .indexError     -- [10,20][-9] (no double wrap)
#guard evalSub (.ssub (.slit (.sdict [(1, 10), (2, 20)])) (.slit (.sint 2))) = .ok (.sint 20) -- {1:10,2:20}[2]
#guard evalSub (.ssub (.slit (.sdict [(1, 10)])) (.slit (.sint 9))) = .err .keyError         -- {1:10}[9]
#guard evalSub (.ssub (.slit (.sint 5)) (.slit (.sint 0))) = .err .typeError                 -- (5)[0]: not subscriptable
#guard evalSub (.ssub (.slit (.slist [1, 2])) (.slit (.sdict []))) = .err .typeError         -- [1,2][{}]: TYPE before range
#guard evalSub (.ssub (.slit (.sdict [(1, 10)])) (.slit (.slist []))) = .err .typeError      -- {1:10}[[]]: unhashable, TYPE before key
-- Evaluation-order pins (C5-adjacent, carried by the kind):
-- (a) key-expression error precedes the subscript TYPE check — CPython
--     `(5)[[1][9]]` is IndexError (BINARY_SUBSCR evaluates both operands
--     first; the brief's claimed TypeError is corrected, see section note):
#guard evalSub (.ssub (.slit (.sint 5)) (.ssub (.slit (.slist [1])) (.slit (.sint 9)))) = .err .indexError
-- (b) container-expression error precedes key-expression error — CPython
--     `([1][9])[({}[0])]` is IndexError (left first), not KeyError:
#guard evalSub (.ssub (.ssub (.slit (.slist [1])) (.slit (.sint 9)))
                      (.ssub (.slit (.sdict [])) (.slit (.sint 0)))) = .err .indexError

/-- A subscript LOWERING: everything the emitted JS gets to choose for this
    fragment — the exception KIND at each of the THREE error sites (C4
    axis) and the negative-index normalization (C1 axis). The preservation
    predicate ranges over this, so a wrong choice on ANY axis falsifies it
    (F10: every kind `preservationSub` universally asserts is reachable by
    a falsifiable parameter — none is hardcoded in the target). -/
structure SubLowering where
  indexExc : Exc               -- C4 axis: kind emitted for list out-of-range
  keyExc   : Exc               -- C4 axis: kind emitted for dict missing key
  typeExc  : Exc               -- C4 axis: kind emitted for non-subscriptable / bad key type
  normIdx  : Int → Int → Int   -- C1 axis: negative-index normalization, (i, len) ↦ index

/-- The shipped lowering: the three Pythonic kinds + the Python
    negative-index wrap. -/
def jsSubL : SubLowering :=
  ⟨.indexError, .keyError, .typeError, fun i len => if i < 0 then i + len else i⟩

/-- C4-wrong stub (INDEX site): `KeyError` where Python raises
    `IndexError` — value-correct everywhere, only the kind differs.
    Invisible to the old `Option` carrier: `eraseKind_blind_to_indexKind`
    below proves that blindness. -/
def keyForIndexL : SubLowering := { jsSubL with indexExc := .keyError }

/-- C4-wrong stub (KEY site): `IndexError` where Python raises `KeyError`
    — the mirror-image confusion. -/
def indexForKeyL : SubLowering := { jsSubL with keyExc := .indexError }

/-- C4-wrong stub (KEY site, second confusion): `TypeError` where Python
    raises `KeyError`. -/
def typeForKeyL : SubLowering := { jsSubL with keyExc := .typeError }

/-- C1-wrong stub: no negative-index wrap (a naive JS `arr[i]` emission).
    Kinds all correct; `[-1]` becomes out-of-range instead of last-element. -/
def noWrapL : SubLowering := { jsSubL with normIdx := fun i _ => i }

/-- **Independent target evaluator**: the compiled subscript semantics
    under lowering `L`. A SEPARATE recursion (not a flag on `evalSub`);
    identical structure, but all three exception kinds and the
    negative-index normalization are the LOWERING'S
    (`L.indexExc`/`L.keyExc`/`L.typeExc`/`L.normIdx`). Error PROPAGATION
    stays hardcoded (it forwards kinds chosen at the leaves — not an
    independent lowering choice). -/
def evalSubtgt (L : SubLowering) : SubExp → PyResult SubVal
  | .slit v => .ok v
  | .ssub c k => match evalSubtgt L c with
      | .err e => .err e
      | .ok cv => match evalSubtgt L k with
          | .err e => .err e
          | .ok kv => match cv, kv with
              | .slist xs, .sint i =>
                  let i' := L.normIdx i (xs.length : Int)
                  if 0 ≤ i' ∧ i' < (xs.length : Int) then .ok (.sint xs[i'.toNat]!)
                  else .err L.indexExc
              | .sdict ps, .sint key =>
                  match ps.find? (fun p => p.1 == key) with
                  | some p => .ok (.sint p.2)
                  | none => .err L.keyExc
              | _, _ => .err L.typeExc

/-- The compiled fragment semantics: the independent target under the
    shipped lowering. -/
abbrev evalSubjs : SubExp → PyResult SubVal := evalSubtgt jsSubL

/-- Preservation as a predicate OVER the lowering — the SAME predicate is
    proved for the shipped lowering and refuted for all four stubs. -/
def SubPreserves (L : SubLowering) : Prop := ∀ e, evalSubtgt L e = evalSub e

/-- **C1 ∧ C3 ∧ C4 in ONE statement, with THREE discriminable error
    kinds.** A `PyResult SubVal` equation is simultaneously:
    * **C1 (value)** — both `.ok` ⇒ equal values (list element / dict
      value, incl. the negative-index wrap);
    * **C3 (error-occurrence)** — `.isErr` agrees: no silent success where
      Python raises, no spurious error where Python succeeds;
    * **C4 (error-KIND)** — both `.err` ⇒ equal `Exc` classes, across a
      genuine 3-kind surface (`IndexError`/`KeyError`/`TypeError`) — the
      projection the `Option`-carrier monolith could not even STATE.
    Real structural induction over the fragment (the IHs rewrite the
    compiled sub-results before the 3×3 container/key dispatch is
    compared arm-by-arm). -/
theorem preservationSub (e : SubExp) : evalSubjs e = evalSub e := by
  induction e with
  | slit v => rfl
  | ssub c k ihc ihk =>
    simp only [evalSubtgt, evalSub, ihc, ihk]
    cases evalSub c with
    | err e => rfl
    | ok cv =>
      cases evalSub k with
      | err e => rfl
      | ok kv => cases cv <;> cases kv <;> rfl

/-- The predicate-form instantiation the four stub litmuses contrast
    against. -/
theorem preservationSub_real : SubPreserves jsSubL := preservationSub

/-- **Stub litmus, C4 axis, INDEX site.** Witness `[10,20][5]`: Python
    `.err .indexError` vs stub `.err .keyError`. The stub is VALUE-correct
    everywhere and errors exactly where Python errors — only the exception
    CLASS is wrong, which is exactly what the old `Option` carrier cannot
    see (`eraseKind_blind_to_indexKind` below proves that blindness). -/
theorem preservationSub_indexKindStub_fails : ¬ SubPreserves keyForIndexL := by
  intro h
  have hc := h (.ssub (.slit (.slist [10, 20])) (.slit (.sint 5)))
  -- hc reduces to `.err .keyError = .err .indexError`.
  exact absurd hc (by decide)

/-- **Stub litmus, C4 axis, KEY site (Index-for-Key confusion).** Witness
    `{1:10}[2]`: Python `.err .keyError` vs stub `.err .indexError`. -/
theorem preservationSub_keyKindStub_fails : ¬ SubPreserves indexForKeyL := by
  intro h
  have hc := h (.ssub (.slit (.sdict [(1, 10)])) (.slit (.sint 2)))
  -- hc reduces to `.err .indexError = .err .keyError`.
  exact absurd hc (by decide)

/-- **Stub litmus, C4 axis, KEY site (Type-for-Key confusion).** Witness
    `{1:10}[2]`: Python `.err .keyError` vs stub `.err .typeError`. -/
theorem preservationSub_typeKindStub_fails : ¬ SubPreserves typeForKeyL := by
  intro h
  have hc := h (.ssub (.slit (.sdict [(1, 10)])) (.slit (.sint 2)))
  -- hc reduces to `.err .typeError = .err .keyError`.
  exact absurd hc (by decide)

/-- **Stub litmus, C1 (value) axis.** Witness `[10,20,30][-1]`: Python
    wraps (`-1 + 3 = 2`) → `.ok (.sint 30)`; the no-wrap stub leaves `-1`
    out of range → `.err .indexError`. Discriminates value AND occurrence
    (a C1∧C3 divergence, with all kinds correct). -/
theorem preservationSub_valueStub_fails : ¬ SubPreserves noWrapL := by
  intro h
  have hc := h (.ssub (.slit (.slist [10, 20, 30])) (.slit (.sint (-1))))
  -- hc reduces to `.err .indexError = .ok (.sint 30)`.
  exact absurd hc (by decide)

-- The four contrasts, concretely (each stub is a plausible naive emission,
-- and each diverges from the reference on its witness — discriminating pins):
#guard evalSubjs (.ssub (.slit (.slist [10, 20])) (.slit (.sint 5))) = .err .indexError                 -- real kind
#guard evalSubtgt keyForIndexL (.ssub (.slit (.slist [10, 20])) (.slit (.sint 5))) = .err .keyError     -- C4 stub ✗
#guard evalSubjs (.ssub (.slit (.sdict [(1, 10)])) (.slit (.sint 2))) = .err .keyError                  -- real kind
#guard evalSubtgt indexForKeyL (.ssub (.slit (.sdict [(1, 10)])) (.slit (.sint 2))) = .err .indexError  -- C4 stub ✗
#guard evalSubtgt typeForKeyL (.ssub (.slit (.sdict [(1, 10)])) (.slit (.sint 2))) = .err .typeError    -- C4 stub ✗
#guard evalSubjs (.ssub (.slit (.slist [10, 20, 30])) (.slit (.sint (-1)))) = .ok (.sint 30)            -- real: wrap
#guard evalSubtgt noWrapL (.ssub (.slit (.slist [10, 20, 30])) (.slit (.sint (-1)))) = .err .indexError -- C1 stub ✗

/-- SPOT (C1, through the theorem, not by evaluation): the compiled
    `[10,20,30][-1]` wraps to the LAST element, via `preservationSub` —
    fails if the statement is weakened. -/
example : evalSubjs (.ssub (.slit (.slist [10, 20, 30])) (.slit (.sint (-1)))) = .ok (.sint 30) := by
  rw [preservationSub]; rfl

/-- SPOT (C4, through the theorem): the compiled `{1:10}[9]` raises
    `KeyError` — the KIND, not just "an error". -/
example : evalSubjs (.ssub (.slit (.sdict [(1, 10)])) (.slit (.sint 9))) = .err .keyError := by
  rw [preservationSub]; rfl

/-- SPOT (C4, through the theorem): the compiled `[10,20][5]` raises
    `IndexError` — distinguished from the KeyError above. -/
example : evalSubjs (.ssub (.slit (.slist [10, 20])) (.slit (.sint 5))) = .err .indexError := by
  rw [preservationSub]; rfl

/-- SPOT (C4, through the theorem): the compiled `(5)[0]` raises
    `TypeError` — the third kind. -/
example : evalSubjs (.ssub (.slit (.sint 5)) (.slit (.sint 0))) = .err .typeError := by
  rw [preservationSub]; rfl

/-- Map over the error kind (successes untouched) — the error-side dual of
    `PyResult.mapOk`. Used to CHARACTERIZE the wrong-kind lowering
    (`evalSubtgt_keyForIndex_char`): the compiled result is exactly the
    reference result with the error kind re-labeled. -/
def PyResult.mapErr {α : Type} (f : Exc → Exc) : PyResult α → PyResult α
  | .ok v => .ok v
  | .err e => .err (f e)

/-- The Index→Key kind re-labeling the `keyForIndexL` stub commits. -/
def idxToKey : Exc → Exc
  | .indexError => .keyError
  | e => e

/-- **The wrong-kind characterization.** Under the Key-for-Index lowering
    the compiled result is EXACTLY the reference result with `IndexError`
    re-labeled `KeyError`: no value moves, no error site moves — only the
    kind. (The analogue of `evalAtgt_d1_collapse` for the C4 axis: the
    stub's entire defect is confined to the projection it fails.) -/
theorem evalSubtgt_keyForIndex_char (e : SubExp) :
    evalSubtgt keyForIndexL e = (evalSub e).mapErr idxToKey := by
  induction e with
  | slit v => rfl
  | ssub c k ihc ihk =>
    simp only [evalSubtgt, evalSub, ihc, ihk]
    cases evalSub c with
    | err e => rfl
    | ok cv =>
      cases evalSub k with
      | err e => rfl
      | ok kv =>
        simp only [PyResult.mapErr]
        cases cv with
        | sint n => cases kv <;> rfl
        | slist xs =>
          cases kv with
          | sint i =>
            simp only [keyForIndexL, jsSubL]
            by_cases hi : 0 ≤ (if i < 0 then i + (xs.length : Int) else i) ∧
                (if i < 0 then i + (xs.length : Int) else i) < (xs.length : Int)
            · simp [hi]
            · simp [hi, idxToKey]
          | slist _ => rfl
          | sdict _ => rfl
        | sdict ps =>
          cases kv with
          | sint key =>
            cases hf : ps.find? (fun p => p.1 == key) with
            | some p => simp [hf]
            | none => simp [hf, keyForIndexL, jsSubL, idxToKey]
          | slist _ => rfl
          | sdict _ => rfl

/-- Kind re-labelings are invisible under `eraseKind`: the old `Option`
    carrier forgets exactly what `mapErr` changes. -/
theorem eraseKind_mapErr {α : Type} (f : Exc → Exc) (r : PyResult α) :
    (r.mapErr f).eraseKind = r.eraseKind := by
  cases r <;> rfl

/-- **THE BLINDNESS LEMMA — C4's added strength for subscript.** Under
    `eraseKind` (the projection back to the OLD `Option` monolith carrier)
    the Key-for-Index lowering matches the reference EVERYWHERE: values
    agree, and every error — of whatever kind — collapses to `none` on
    both sides. So the Index-vs-Key confusion PASSES the old
    `Option`-carrier statement (this lemma) yet FAILS the `PyResult`
    statement (`preservationSub_indexKindStub_fails`): a C1∧C3-only
    theorem — the entire pre-lattice monolith — would MISS a compiler that
    raises `KeyError` for every out-of-range list index; only the
    C4-carrying `preservationSub` catches it. Immediate from the
    characterization: the stub is a kind re-labeling, and `eraseKind`
    forgets kinds. -/
theorem eraseKind_blind_to_indexKind (e : SubExp) :
    (evalSubtgt keyForIndexL e).eraseKind = (evalSub e).eraseKind := by
  rw [evalSubtgt_keyForIndex_char]
  exact eraseKind_mapErr idxToKey (evalSub e)

-- The blindness, concretely: on the Key-for-Index witness the ERASED
-- projections agree (both `none`) — the old carrier records "an error
-- occurred", nothing more; the kinds (pinned above) differ.
#guard (evalSubtgt keyForIndexL (.ssub (.slit (.slist [10, 20])) (.slit (.sint 5)))).eraseKind
        = (none : Option SubVal)
#guard (evalSub (.ssub (.slit (.slist [10, 20])) (.slit (.sint 5)))).eraseKind
        = (none : Option SubVal)

/-- info: 'PythExpandVerify.preservationSub' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms preservationSub

/-- info: 'PythExpandVerify.preservationSub_real' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms preservationSub_real

/-- info: 'PythExpandVerify.preservationSub_indexKindStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationSub_indexKindStub_fails

/-- info: 'PythExpandVerify.preservationSub_keyKindStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationSub_keyKindStub_fails

/-- info: 'PythExpandVerify.preservationSub_typeKindStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationSub_typeKindStub_fails

/-- info: 'PythExpandVerify.preservationSub_valueStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationSub_valueStub_fails

/-- info: 'PythExpandVerify.evalSubtgt_keyForIndex_char' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms evalSubtgt_keyForIndex_char

/-- info: 'PythExpandVerify.eraseKind_mapErr' does not depend on any axioms -/
#guard_msgs in
#print axioms eraseKind_mapErr

/-- info: 'PythExpandVerify.eraseKind_blind_to_indexKind' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms eraseKind_blind_to_indexKind

/-! ## Preservation lattice v2 — C5 (effect / observation-order): two proved slices

The lattice sections above cover C1 (value), C2 (type-tag), C3
(error-occurrence) and C4 (error-KIND). C5 — the ORDER in which observable
effects happen — decomposes into a LANGUAGE-OWNED part and a HOST-OWNED
part (the design notes §2/§3/§5 item 6):

* LANGUAGE-OWNED (the compiler's obligation — provable, and proved here in
  two slices): **(6a)** synchronous evaluation order — Python's `or`/`and`
  short-circuit, which decides WHETHER the right operand's effect runs at
  all; and **(6b)** the `async` → state-machine desugaring, which must keep
  effects in program order across `await` suspension points.
* HOST-OWNED (trusted, NOT modeled, NOT claimed): the JS event loop's
  scheduling of real Promise resolutions (timers / network / DOM /
  arbitrary JS) and the microtask interleaving BETWEEN tasks. This is a
  trust boundary BY CONSTRUCTION (`release_plan_v2.md` §10.2
  host-ownership): slice 6b's awaits are on ALREADY-RESOLVED modeled
  values, so its theorem binds the desugaring, never the scheduler.

Both slices follow the established lattice discipline (`preservationA` /
`preservationSub` above): an INDEPENDENT target parameterized by a
falsifiable lowering/strategy (F10: every ordering fact the universal
theorems assert is reachable by that parameter), a real preservation
theorem by structural induction, discriminating `*_stub_fails` refuting
the SAME predicate, and an added-strength separation — the `scValue`
value-only projection is provably BLIND to the 6a order defect, mirroring
`eraseKind_blind_to_zeroKind` (C4 over C3) and `preservationA_rho_holds_d1`
(C2 over C1-mod-ρ) one lattice axis up.

Stated simplifications (modeling choices, not claims):
* Effects are opaque `String` labels — one label per evaluated leaf; the
  trace `List String` records which effects ran, in order. No effect
  payload / DOM semantics are modeled.
* 6a values are int-only (`ScVal.bint`) with Python int truthiness
  (0 falsy, everything else truthy). No str/list truthiness here.
* The operand order a-BEFORE-b is common to both 6a strategies — the 6a
  falsifiable axis is WHETHER the RHS runs (short-circuit vs eager); an
  order SWAP is falsified in 6b (`reorderL`).
* 6b awaits (`AyStmt.awaitV`) carry an already-resolved value: a
  suspension point that emits nothing and must not reorder neighbouring
  effects.

Fresh `Sc…`/`Ay…` type prefixes (wave 11 owns `SVal`/`SExp`; the statement
waves own `Stmt`). Reuses the lattice carrier (`Exc`/`PyResult`) verbatim.
Purely additive; no existing wave or section is touched. -/

/-! ### Slice 6a — synchronous short-circuit + effect order -/

/-- 6a value domain: ints only (Python int truthiness — 0 falsy, else
    truthy). `DecidableEq` drives the `decide`-based stub refutations and
    `#guard` pins. -/
inductive ScVal where
  | bint (n : Int)
  deriving DecidableEq, Repr

/-- Python truthiness on the fragment's values (`bool(n)` ⇔ `n != 0`). -/
def scTruthy : ScVal → Bool
  | .bint n => decide (n ≠ 0)

/-- The short-circuit fragment: leaves EMIT an observable effect when (and
    only when) they are evaluated. `leaf n eff` emits `eff` then yields
    `n`; `fail e eff` emits `eff` then raises `e` — modeling an evaluated
    operand whose effect runs before its raise. -/
inductive ScExp where
  | leaf (n : Int) (eff : String)
  | fail (e : Exc) (eff : String)
  | orE (a b : ScExp)       -- a or b   (short-circuit: a truthy ⇒ b NOT evaluated)
  | andE (a b : ScExp)      -- a and b  (short-circuit: a falsy  ⇒ b NOT evaluated)
  deriving Repr

/-- **Reference (CPython) semantics**: `(result, trace of effects that
    actually ran, in order)`. Faithfulness notes (each pinned by a `#guard`
    below, verified against live CPython):
    * `or`/`and` return the OPERAND value, not a bool: `0 or 5` is `5`,
      `2 and 5` is `5`, `0 and 5` is `0`.
    * SHORT-CIRCUIT skips the RHS entirely — value AND effect: `1 or b`
      never runs `b`'s effect; `0 and b` never runs `b`'s effect. This is
      the C5 fact: WHICH effects run, and in which order, is part of the
      observable semantics, invisible to a value-only carrier.
    * An error in an evaluated operand propagates: `(1//0) or b` raises
      without running `b`; `1 or (1//0)` does NOT raise (the RHS is never
      evaluated); `2 and (1//0)` raises after both effects ran.
    * Truthiness is Python int truthiness (`scTruthy`). -/
def evalSc : ScExp → PyResult ScVal × List String
  | .leaf n eff => (.ok (.bint n), [eff])
  | .fail e eff => (.err e, [eff])
  | .orE a b =>
      match evalSc a with
      | (.err e, ta) => (.err e, ta)              -- a raised ⇒ propagate, b NOT run
      | (.ok va, ta) =>
          if scTruthy va then (.ok va, ta)        -- short-circuit: b NOT run
          else match evalSc b with
               | (rb, tb) => (rb, ta ++ tb)
  | .andE a b =>
      match evalSc a with
      | (.err e, ta) => (.err e, ta)
      | (.ok va, ta) =>
          if scTruthy va then
            match evalSc b with                   -- a truthy ⇒ eval b
            | (rb, tb) => (rb, ta ++ tb)
          else (.ok va, ta)                       -- short-circuit: b NOT run

-- F9 faithfulness pins — the REFERENCE matches CPython on the operand-value
-- protocol, the short-circuit (skipped RHS effect) and error propagation
-- (each line verified against live CPython; one effect label per evaluated
-- leaf):
#guard evalSc (.orE (.leaf 1 "a") (.leaf 5 "b")) = (.ok (.bint 1), ["a"])        -- 1 or 5 → 1; RHS skipped
#guard evalSc (.orE (.leaf 0 "a") (.leaf 5 "b")) = (.ok (.bint 5), ["a", "b"])   -- 0 or 5 → 5 (operand, not True)
#guard evalSc (.orE (.leaf 0 "a") (.leaf 0 "b")) = (.ok (.bint 0), ["a", "b"])   -- 0 or 0 → 0 (RHS operand)
#guard evalSc (.andE (.leaf 0 "a") (.leaf 5 "b")) = (.ok (.bint 0), ["a"])       -- 0 and 5 → 0; RHS skipped
#guard evalSc (.andE (.leaf 2 "a") (.leaf 5 "b")) = (.ok (.bint 5), ["a", "b"])  -- 2 and 5 → 5 (operand)
#guard evalSc (.andE (.leaf (-3) "a") (.leaf 0 "b")) = (.ok (.bint 0), ["a", "b"]) -- -3 truthy (nonzero)
#guard evalSc (.orE (.leaf 1 "a") (.fail .zeroDiv "b")) = (.ok (.bint 1), ["a"])   -- 1 or (1//0) → 1, NO raise
#guard evalSc (.andE (.leaf 2 "a") (.fail .zeroDiv "b")) = (.err .zeroDiv, ["a", "b"]) -- 2 and (1//0) raises
#guard evalSc (.orE (.fail .typeError "a") (.leaf 5 "b")) = (.err .typeError, ["a"])   -- LHS raise short-circuits
-- nested: (0 or 2) and 3 → 3 with effects a, b, c in program order:
#guard evalSc (.andE (.orE (.leaf 0 "a") (.leaf 2 "b")) (.leaf 3 "c")) = (.ok (.bint 3), ["a", "b", "c"])

/-- 6a evaluation STRATEGY — the falsifiable parameter (F10): `eager =
    false` short-circuits (what the compiler emits); `eager = true` ALWAYS
    evaluates both operands. WHETHER the RHS runs is the strategy's choice,
    so every short-circuit fact `preservationSc` asserts is reachable by
    the parameter — nothing is hardcoded beyond the a-before-b operand
    order common to both strategies (the order-SWAP axis is falsified in
    slice 6b). -/
structure ScStrategy where
  eager : Bool

/-- Correct strategy: short-circuits — what the compiler emits (JS
    `&&`/`||` under the Python truthiness + operand-value protocol). -/
def jsScS : ScStrategy := ⟨false⟩

/-- Stub strategy: a wrong compiler that eagerly evaluates BOTH operands
    (e.g. lowering `a or b` to `tmpA = a; tmpB = b; pyOr(tmpA, tmpB)`). -/
def eagerScS : ScStrategy := ⟨true⟩

/-- **Independent target evaluator**: the compiled semantics under strategy
    `S`. A SEPARATE recursion (not a flag on `evalSc`). With `S.eager =
    false` it short-circuits exactly like the reference; with `S.eager =
    true` it evaluates BOTH operands unconditionally — running the RHS
    effect AND threading the RHS error (the RHS really executed, so its
    raise propagates; the leftmost raise wins, matching program order) —
    then combines with the Python operand-value rules. -/
def evalSctgt (S : ScStrategy) : ScExp → PyResult ScVal × List String
  | .leaf n eff => (.ok (.bint n), [eff])
  | .fail e eff => (.err e, [eff])
  | .orE a b =>
      if S.eager then
        match evalSctgt S a, evalSctgt S b with
        | (.err e, ta), (_, tb) => (.err e, ta ++ tb)
        | (.ok _, ta), (.err e, tb) => (.err e, ta ++ tb)
        | (.ok va, ta), (.ok vb, tb) =>
            if scTruthy va then (.ok va, ta ++ tb) else (.ok vb, ta ++ tb)
      else
        match evalSctgt S a with
        | (.err e, ta) => (.err e, ta)
        | (.ok va, ta) =>
            if scTruthy va then (.ok va, ta)
            else match evalSctgt S b with
                 | (rb, tb) => (rb, ta ++ tb)
  | .andE a b =>
      if S.eager then
        match evalSctgt S a, evalSctgt S b with
        | (.err e, ta), (_, tb) => (.err e, ta ++ tb)
        | (.ok _, ta), (.err e, tb) => (.err e, ta ++ tb)
        | (.ok va, ta), (.ok vb, tb) =>
            if scTruthy va then (.ok vb, ta ++ tb) else (.ok va, ta ++ tb)
      else
        match evalSctgt S a with
        | (.err e, ta) => (.err e, ta)
        | (.ok va, ta) =>
            if scTruthy va then
              match evalSctgt S b with
              | (rb, tb) => (rb, ta ++ tb)
            else (.ok va, ta)

/-- The compiled fragment semantics: the independent target under the
    shipped (short-circuiting) strategy. -/
abbrev evalScjs : ScExp → PyResult ScVal × List String := evalSctgt jsScS

/-- Preservation as a predicate OVER the strategy — the SAME predicate is
    proved for the shipped strategy and refuted for the eager stub. -/
def ScPreserves (S : ScStrategy) : Prop := ∀ e, evalSctgt S e = evalSc e

/-- **C5 (synchronous effect order) over (value × trace), in one
    statement.** The compiled short-circuit evaluation agrees with the
    reference on the PAIR (result, effect trace): every effect that runs,
    runs in the same order, none that Python skips is run — plus the
    value / error-occurrence / error-KIND components the `PyResult` half
    of the carrier already expresses (C1/C3/C4). Real structural induction
    over the fragment (the IHs rewrite the compiled sub-results before the
    short-circuit dispatch is compared arm-by-arm). -/
theorem preservationSc (e : ScExp) : evalScjs e = evalSc e := by
  induction e with
  | leaf n eff => rfl
  | fail ex eff => rfl
  | orE a b iha ihb =>
    simp only [evalSctgt, evalSc, iha, ihb]
    cases hA : evalSc a with
    | mk ra ta =>
      cases ra with
      | err e1 => rfl
      | ok va =>
        cases hv : scTruthy va with
        | false =>
          cases hB : evalSc b with
          | mk rb tb => rfl
        | true => rfl
  | andE a b iha ihb =>
    simp only [evalSctgt, evalSc, iha, ihb]
    cases hA : evalSc a with
    | mk ra ta =>
      cases ra with
      | err e1 => rfl
      | ok va =>
        cases hv : scTruthy va with
        | false => rfl
        | true =>
          cases hB : evalSc b with
          | mk rb tb => rfl

/-- The predicate-form instantiation the stub litmuses contrast against. -/
theorem preservationSc_real : ScPreserves jsScS := preservationSc

/-- **Stub litmus, C5 (error axis).** Witness `1 or (1//0)` (with
    effects): the reference short-circuits — `(.ok 1, ["a"])`, the RHS
    raise NEVER happens — while the eager stub runs the RHS:
    `(.err .zeroDiv, ["a", "b"])`. Value AND trace differ: eager
    evaluation turns a fine Python program into a crash. -/
theorem preservationSc_eagerStub_fails : ¬ ScPreserves eagerScS := by
  intro h
  have hc := h (.orE (.leaf 1 "a") (.fail .zeroDiv "b"))
  -- hc reduces to `(.err .zeroDiv, ["a", "b"]) = (.ok (.bint 1), ["a"])`.
  exact absurd hc (by decide)

-- The 6a error-axis contrast, concretely (discriminating pins):
#guard evalScjs (.orE (.leaf 1 "a") (.fail .zeroDiv "b")) = (.ok (.bint 1), ["a"])                -- real: skip
#guard evalSctgt eagerScS (.orE (.leaf 1 "a") (.fail .zeroDiv "b")) = (.err .zeroDiv, ["a", "b"]) -- eager ✗

/-- The value-only projection: keep the result, FORGET the trace — what a
    pre-C5 (value/error-only) preservation statement observes. -/
def scValue : PyResult ScVal × List String → PyResult ScVal := Prod.fst

/-- Failure-free subclass: no `fail` leaf anywhere. Decidable by
    construction; the domain of the blindness lemma below. -/
def scNoFail : ScExp → Bool
  | .leaf _ _ => true
  | .fail _ _ => false
  | .orE a b => scNoFail a && scNoFail b
  | .andE a b => scNoFail a && scNoFail b

/-- On failure-free programs the eager evaluator never errs (both operands
    always succeed, and the eager combine only forwards operand errors).
    Load-bearing for the blindness lemma: it discharges the one case where
    eager evaluation could diverge in VALUE — a threaded RHS error the
    reference never raises. Stated for `eagerScS`, the only strategy the
    blindness lemma needs. -/
theorem evalSctgt_eager_noFail_notErr (e : ScExp) :
    scNoFail e = true → ((evalSctgt eagerScS e).fst).isErr = false := by
  induction e with
  | leaf n eff => intro _; rfl
  | fail ex eff => intro h; simp [scNoFail] at h
  | orE a b iha ihb =>
    intro h
    simp only [scNoFail, Bool.and_eq_true] at h
    have ha := iha h.1
    have hb := ihb h.2
    simp only [evalSctgt]
    cases hA : evalSctgt eagerScS a with
    | mk ra ta =>
      cases hB : evalSctgt eagerScS b with
      | mk rb tb =>
        rw [hA] at ha
        rw [hB] at hb
        cases ra with
        | err e1 => simp [PyResult.isErr] at ha
        | ok va =>
          cases rb with
          | err e2 => simp [PyResult.isErr] at hb
          | ok vb =>
            cases hv : scTruthy va <;> simp [eagerScS, hv, PyResult.isErr]
  | andE a b iha ihb =>
    intro h
    simp only [scNoFail, Bool.and_eq_true] at h
    have ha := iha h.1
    have hb := ihb h.2
    simp only [evalSctgt]
    cases hA : evalSctgt eagerScS a with
    | mk ra ta =>
      cases hB : evalSctgt eagerScS b with
      | mk rb tb =>
        rw [hA] at ha
        rw [hB] at hb
        cases ra with
        | err e1 => simp [PyResult.isErr] at ha
        | ok va =>
          cases rb with
          | err e2 => simp [PyResult.isErr] at hb
          | ok vb =>
            cases hv : scTruthy va <;> simp [eagerScS, hv, PyResult.isErr]

/-- **The C5 added-strength SEPARATION (value-correct, ORDER-wrong).** On
    the failure-free subclass (`scNoFail`), the EAGER strategy — refuted
    above on the full trace-carrying predicate — provably PRESERVES the
    value projection: a value-only theorem is structurally BLIND to the
    extra RHS effect (`1 or 5` under eager evaluation still returns `1`;
    only the TRACE betrays that `"b"` ran). Together with
    `preservationSc_trace_fails_on_success` below (the trace-carrying
    predicate fails on the SAME subclass), this proves C5(order) is
    strictly stronger than value-only preservation — the mirror of
    `eraseKind_blind_to_zeroKind` (C4 over C3) and
    `preservationA_rho_holds_d1` (C2 over C1-mod-ρ), one lattice axis
    up. -/
theorem preservationSc_value_blind_on_success (e : ScExp) :
    scNoFail e = true → scValue (evalSctgt eagerScS e) = scValue (evalSc e) := by
  induction e with
  | leaf n eff => intro _; rfl
  | fail ex eff => intro h; simp [scNoFail] at h
  | orE a b iha ihb =>
    intro h
    simp only [scNoFail, Bool.and_eq_true] at h
    have hva := iha h.1
    have hvb := ihb h.2
    have hbe := evalSctgt_eager_noFail_notErr b h.2
    simp only [evalSctgt, evalSc]
    cases hA : evalSctgt eagerScS a with
    | mk ra ta =>
      cases hA' : evalSc a with
      | mk ra' ta' =>
        cases hB : evalSctgt eagerScS b with
        | mk rb tb =>
          cases hB' : evalSc b with
          | mk rb' tb' =>
            rw [hA, hA'] at hva
            rw [hB, hB'] at hvb
            rw [hB] at hbe
            have hra : ra = ra' := hva
            have hrb : rb = rb' := hvb
            rw [← hra, ← hrb]
            cases ra with
            | err e1 => rfl
            | ok va =>
              cases rb with
              | err e2 => simp [PyResult.isErr] at hbe
              | ok vb =>
                cases hv : scTruthy va <;> simp [eagerScS, hv, scValue]
  | andE a b iha ihb =>
    intro h
    simp only [scNoFail, Bool.and_eq_true] at h
    have hva := iha h.1
    have hvb := ihb h.2
    have hbe := evalSctgt_eager_noFail_notErr b h.2
    simp only [evalSctgt, evalSc]
    cases hA : evalSctgt eagerScS a with
    | mk ra ta =>
      cases hA' : evalSc a with
      | mk ra' ta' =>
        cases hB : evalSctgt eagerScS b with
        | mk rb tb =>
          cases hB' : evalSc b with
          | mk rb' tb' =>
            rw [hA, hA'] at hva
            rw [hB, hB'] at hvb
            rw [hB] at hbe
            have hra : ra = ra' := hva
            have hrb : rb = rb' := hvb
            rw [← hra, ← hrb]
            cases ra with
            | err e1 => rfl
            | ok va =>
              cases rb with
              | err e2 => simp [PyResult.isErr] at hbe
              | ok vb =>
                cases hv : scTruthy va <;> simp [eagerScS, hv, scValue]

/-- The trace-carrying predicate is refuted ON THE SAME failure-free
    subclass where the value projection provably holds: the both-succeed
    witness `1 or 5` gives the SAME value but an extra `"b"` effect. So
    the 6a order defect is INVISIBLE to any value-only statement
    (`preservationSc_value_blind_on_success`) and VISIBLE to the
    trace-carrying one — the C5 separation, as a theorem pair. -/
theorem preservationSc_trace_fails_on_success :
    ¬ (∀ e, scNoFail e = true → evalSctgt eagerScS e = evalSc e) := by
  intro h
  have hc := h (.orE (.leaf 1 "a") (.leaf 5 "b")) (by decide)
  -- hc reduces to `(.ok (.bint 1), ["a", "b"]) = (.ok (.bint 1), ["a"])`.
  exact absurd hc (by decide)

-- The 6a separation, concretely: on the BOTH-SUCCEED witness the eager
-- stub agrees on the VALUE yet runs an EXTRA effect — the divergence
-- lives purely in the trace (the C5 axis):
#guard evalSc (.orE (.leaf 1 "a") (.leaf 5 "b")) = (.ok (.bint 1), ["a"])
#guard evalSctgt eagerScS (.orE (.leaf 1 "a") (.leaf 5 "b")) = (.ok (.bint 1), ["a", "b"])
#guard scValue (evalSctgt eagerScS (.orE (.leaf 1 "a") (.leaf 5 "b")))
     = scValue (evalSc (.orE (.leaf 1 "a") (.leaf 5 "b")))                       -- value projection: agree
#guard (evalSctgt eagerScS (.orE (.leaf 1 "a") (.leaf 5 "b"))).snd
     ≠ (evalSc (.orE (.leaf 1 "a") (.leaf 5 "b"))).snd                           -- trace: differ

/-- SPOT (through the theorem, not by evaluation): the compiled
    `1 or (1//0)` returns `1` and SKIPS the RHS — no raise, no `"b"`
    effect — via `preservationSc`; fails if the statement is weakened. -/
example : evalScjs (.orE (.leaf 1 "a") (.fail .zeroDiv "b")) = (.ok (.bint 1), ["a"]) := by
  rw [preservationSc]; rfl

/-- SPOT: the compiled `0 and (1//0)` short-circuits to `0` — the RHS
    raise and effect are skipped — via `preservationSc`. -/
example : evalScjs (.andE (.leaf 0 "a") (.fail .zeroDiv "b")) = (.ok (.bint 0), ["a"]) := by
  rw [preservationSc]; rfl

/-- SPOT (through the separation lemma): the eager strategy's VALUE on the
    both-succeed witness agrees with the reference — derived from
    `preservationSc_value_blind_on_success`, so it fails if the no-fail
    value-preservation lemma is weakened. -/
example : scValue (evalSctgt eagerScS (.orE (.leaf 1 "a") (.leaf 5 "b")))
        = scValue (evalSc (.orE (.leaf 1 "a") (.leaf 5 "b"))) :=
  preservationSc_value_blind_on_success (.orE (.leaf 1 "a") (.leaf 5 "b")) (by decide)

/-! ### Slice 6b — deterministic async → state-machine lowering
    (modeled awaits, NO host callback) -/

/-- An `async` body: observable effects, await-points on ALREADY-RESOLVED
    values (no host scheduling — see the section header's trust boundary),
    and sequencing. -/
inductive AyStmt where
  | emit (eff : String)     -- an observable effect (e.g. a log / DOM-free side effect)
  | awaitV (v : Int)        -- await an already-resolved value — a suspension point
  | seqA (a b : AyStmt)     -- sequencing
  deriving Repr

/-- Reference SEQUENTIAL semantics: the effect trace in program order. An
    await on a resolved value suspends but emits nothing and reorders
    nothing — deterministic and host-free BY CONSTRUCTION. -/
def ayRun : AyStmt → List String
  | .emit eff => [eff]
  | .awaitV _ => []
  | .seqA a b => ayRun a ++ ayRun b

-- Reference pins: program-order trace; awaits are silent suspension points:
#guard ayRun (.seqA (.emit "a") (.seqA (.awaitV 0) (.emit "b"))) = ["a", "b"]
#guard ayRun (.awaitV 7) = ([] : List String)
#guard ayRun (.seqA (.seqA (.emit "a") (.awaitV 1)) (.seqA (.emit "b") (.emit "c"))) = ["a", "b", "c"]

/-- A state-machine STEP: emit an effect, or resume from a suspension with
    the (modeled, already-resolved) awaited value. -/
inductive AyStep where
  | sEmit (eff : String)
  | sResume (v : Int)
  deriving Repr

/-- The compiled state machine: the ordered list of resumable steps. -/
abbrev AyMachine := List AyStep

/-- Running the state machine to completion: `sEmit` emits, `sResume`
    consumes its resolved value silently (the suspension/resumption pair,
    with the host's scheduling of the resumption out of scope by
    construction). -/
def ayRunSM : AyMachine → List String
  | [] => []
  | .sEmit e :: t => e :: ayRunSM t
  | .sResume _ :: t => ayRunSM t

/-- An async LOWERING — the falsifiable parameter (F10): how the compiler
    desugars an async body into a state machine. The preservation
    predicate ranges over this, so a desugaring that drops, duplicates or
    REORDERS steps falsifies it — flattening is not hardcoded in the
    target (`ayLower` only RUNS whatever machine the lowering built). -/
structure AyLowering where
  desugar : AyStmt → AyMachine

/-- CORRECT desugaring: flatten in program order — `emit ↦ sEmit`,
    `await ↦ sResume`, `seqA ↦ (desugar a ++ desugar b)`. -/
def ayFlatten : AyStmt → AyMachine
  | .emit eff => [.sEmit eff]
  | .awaitV v => [.sResume v]
  | .seqA a b => ayFlatten a ++ ayFlatten b

/-- The shipped lowering. -/
def flattenL : AyLowering := ⟨ayFlatten⟩

/-- STUB desugaring: swaps the two halves of the TOP `seqA` (all lower
    levels flattened correctly) — the shape of a codegen bug that moves
    the post-await continuation BEFORE the pre-await prefix. On the
    witness `emit "a"; await; emit "b"` it emits `"b"` before `"a"`: an
    effect crosses the suspension point. -/
def ayReorder : AyStmt → AyMachine
  | .seqA a b => ayFlatten b ++ ayFlatten a
  | s => ayFlatten s

/-- The reordering stub lowering. -/
def reorderL : AyLowering := ⟨ayReorder⟩

/-- The observable behavior of the compiled machine under lowering `L`. -/
def ayLower (L : AyLowering) (s : AyStmt) : List String := ayRunSM (L.desugar s)

/-- Preservation as a predicate OVER the lowering — the SAME predicate is
    proved for the flattening lowering and refuted for the reorder stub. -/
def AyPreserves (L : AyLowering) : Prop := ∀ s, ayLower L s = ayRun s

/-- Key algebraic step: running concatenated machines concatenates their
    traces. -/
theorem ayRunSM_append (m1 m2 : AyMachine) :
    ayRunSM (m1 ++ m2) = ayRunSM m1 ++ ayRunSM m2 := by
  induction m1 with
  | nil => rfl
  | cons s t ih =>
    cases s with
    | sEmit e => simp only [List.cons_append, ayRunSM, ih]
    | sResume v => simp only [List.cons_append, ayRunSM, ih]

/-- **C5 (async effect order across suspension points).** The flattening
    desugaring preserves the sequential effect trace: compiling an async
    body to a resumable state machine keeps every effect in program order
    across `await` suspension points. Structural induction;
    `ayRunSM_append` is the key step (the machine of `a; b` is the
    machines of `a` and `b` concatenated, and running concatenated
    machines concatenates their traces).

    **Honest boundary (trust boundary BY CONSTRUCTION,
    `release_plan_v2.md` §10.2):** awaits are on MODELED,
    ALREADY-RESOLVED values (`awaitV v`), so this theorem binds the
    COMPILER's async-desugaring obligation — the language-owned half of
    C5 — and nothing more. Real host scheduling (Promises resolved by
    timers / network / DOM / arbitrary JS, and the microtask interleaving
    BETWEEN tasks) is the trusted JS event loop; it is not modeled and
    NOT claimed. What IS claimed: however the host schedules resumptions,
    the desugared machine yields THIS body's effects in program order. -/
theorem preservationAy (s : AyStmt) : ayLower flattenL s = ayRun s := by
  show ayRunSM (ayFlatten s) = ayRun s
  induction s with
  | emit eff => rfl
  | awaitV v => rfl
  | seqA a b iha ihb =>
    show ayRunSM (ayFlatten a ++ ayFlatten b) = ayRun a ++ ayRun b
    rw [ayRunSM_append, iha, ihb]

/-- The predicate-form instantiation the reorder litmus contrasts
    against. -/
theorem preservationAy_real : AyPreserves flattenL := preservationAy

/-- **Stub litmus, C5 (reorder axis).** Witness `emit "a"; await;
    emit "b"`: reference trace `["a", "b"]` vs reordered machine
    `["b", "a"]` — a PURE order divergence (same effects, same values, no
    errors): the post-await effect crossed the suspension point. Exactly
    the defect class a value-only or occurrence-only theorem can never
    see — the discriminating heart of C5 for async. -/
theorem preservationAy_reorderStub_fails : ¬ AyPreserves reorderL := by
  intro h
  have hc := h (.seqA (.emit "a") (.seqA (.awaitV 0) (.emit "b")))
  -- hc reduces to `["b", "a"] = ["a", "b"]`.
  exact absurd hc (by decide)

-- The 6b contrast, concretely: correct lowering keeps program order;
-- the reorder stub emits the SAME effects in the WRONG order:
#guard ayLower flattenL (.seqA (.emit "a") (.seqA (.awaitV 0) (.emit "b"))) = ["a", "b"]
#guard ayLower reorderL (.seqA (.emit "a") (.seqA (.awaitV 0) (.emit "b"))) = ["b", "a"]

/-- SPOT (through the theorem, not by evaluation): the compiled
    `log1; await; log2` body yields its effects in program order, via
    `preservationAy`; fails if the statement is weakened. -/
example : ayLower flattenL (.seqA (.emit "log1") (.seqA (.awaitV 42) (.emit "log2")))
        = ["log1", "log2"] := by
  rw [preservationAy]; rfl

-- Per-declaration axiom pins (Stage-5 gate; captured from a real build):

/-- info: 'PythExpandVerify.preservationSc' depends on axioms: [Quot.sound] -/
#guard_msgs in
#print axioms preservationSc

/-- info: 'PythExpandVerify.preservationSc_real' depends on axioms: [Quot.sound] -/
#guard_msgs in
#print axioms preservationSc_real

/-- info: 'PythExpandVerify.preservationSc_eagerStub_fails' does not depend on any axioms -/
#guard_msgs in
#print axioms preservationSc_eagerStub_fails

/-- info: 'PythExpandVerify.evalSctgt_eager_noFail_notErr' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms evalSctgt_eager_noFail_notErr

/-- info: 'PythExpandVerify.preservationSc_value_blind_on_success' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms preservationSc_value_blind_on_success

/-- info: 'PythExpandVerify.preservationSc_trace_fails_on_success' does not depend on any axioms -/
#guard_msgs in
#print axioms preservationSc_trace_fails_on_success

/-- info: 'PythExpandVerify.ayRunSM_append' depends on axioms: [propext] -/
#guard_msgs in
#print axioms ayRunSM_append

/-- info: 'PythExpandVerify.preservationAy' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationAy

/-- info: 'PythExpandVerify.preservationAy_real' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationAy_real

/-- info: 'PythExpandVerify.preservationAy_reorderStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationAy_reorderStub_fails

/-! ## §7b boundary composition — PART 1: the COMPOSITION META-THEOREM

Design: the design notes §7b. C1–C6 are all *intra-domain* (Python model vs
Python reference, one semantic world). A real reference-app program COMPOSES a
Python-semantics computation with a **trusted host sink** `S` in a DIFFERENT world — a
comprehension result becoming DOM children (B-JS), or a numeric kernel the auto-router
sends to the WASM i64 fast path (B-WASM). The emitted run is `S (M_js (evalPy_js pyComp))`,
the reference `S (M_ref (evalPy_ref pyComp))`, where `M` is the compiler's **marshalling**
of a Python value into the sink's input domain and `S` is applied identically on BOTH
sides. So the composed observable is preserved **iff** `M (evalJs) = M (evalRef)` — which,
given the lattice (`evalJs ≈ρ evalRef`), reduces to the single obligation *M is ρ-faithful*.

This section formalizes that closure principle GENERICALLY. `β` is the ABSTRACT
sink-input type — a bare `Type` variable, NOT React/DOM/the WASM host: the sink `S`
stays a declared trust boundary by construction (§10.2), exactly the honest split of C5.
The theorem proves what the COMPILER owns (the marshalling `M` introduces no new
divergence) and says nothing about what the HOST owns (rendering/traps).

Empirically grounded (both DRY, 2026-08-04): the B-JS interaction differential
(`frontend/src/components/interaction/`, 15 cases) confirmed the DOM marshalling faithful,
and the B-WASM differential (`experiments/pbt-ps/wasm_shipped_binding.py`) confirmed the
WASM boundary — so these theorems have green empirical targets.

Reuses the lattice-v2 arith fragment verbatim (`PVal`/`valRho`/`resultRho`/`typeTag`/
`preservationA`/`preservationA_rho_holds_d1`/`d1CollapseAL`); purely additive, no existing
wave touched. Same independent-target + stub-litmus discipline. -/

/-- **ρ-faithfulness** — the single obligation the boundary marshalling must meet.
    A marshalling `M : PVal → β` is ρ-faithful when it maps ρ-EQUIVALENT source values
    to EQUAL sink inputs: it must not LEAK a distinction that ρ quotients away (e.g. must
    not distinguish the whole float `2.0` from the int `2` — that is exactly the D1
    quotient `valRho` already carries). `[DecidableEq β]` matches the sink-domain
    interface (equality of rendered inputs is observable); the stub `typeTag` witnesses
    that this predicate genuinely CONSTRAINS `M`. -/
def RhoFaithful {β : Type} [DecidableEq β] (M : PVal → β) : Prop :=
  ∀ a b, valRho a b = true → M a = M b

/-- Lift a value marshalling to results (errors pass through untouched — the sink never
    sees them; they are C3/C4's business, handled by the lattice, not the boundary). -/
def resultMap {β : Type} (M : PVal → β) : PyResult PVal → PyResult β
  | .ok v => .ok (M v)
  | .err e => .err e

/-- **THE COMPOSITION META-THEOREM.** A ρ-faithful marshalling maps ρ-equivalent RESULTS
    to EQUAL sink results. Applying the same trusted sink `S` to both sides is then equal
    by congruence — `S` stays abstract/unmodeled — so the composed boundary program
    preserves. All the content is concentrated in the `RhoFaithful M` hypothesis; the
    boundary itself introduces NO new divergence. Proof is a genuine case analysis on the
    result shapes (the mixed `ok`/`err` shapes are ruled out because `resultRho` is
    `false` there), NOT `rfl`: the `ok`/`ok` arm is where `hM` does the real work. -/
theorem compositionThm {β : Type} [DecidableEq β] (M : PVal → β) (hM : RhoFaithful M)
    (r₁ r₂ : PyResult PVal) (h : resultRho r₁ r₂ = true) :
    resultMap M r₁ = resultMap M r₂ := by
  cases r₁ with
  | ok a =>
    cases r₂ with
    | ok b =>
      have hv : valRho a b = true := h        -- resultRho (.ok a) (.ok b) ≡ valRho a b
      simp only [resultMap, hM a b hv]
    | err e => simp [resultRho] at h           -- resultRho (.ok _) (.err _) ≡ false
  | err e =>
    cases r₂ with
    | ok b => simp [resultRho] at h            -- resultRho (.err _) (.ok _) ≡ false
    | err e' =>
      simp only [resultRho, decide_eq_true_eq] at h   -- h : e = e'
      simp only [resultMap, h]

/-- ρ (on values, then results) is reflexive — the bridge from the STRICT lattice
    equations (`preservationA`) into the mod-ρ hypothesis `compositionThm` consumes. -/
theorem valRho_refl (v : PVal) : valRho v v = true := by cases v <;> simp [valRho]

theorem resultRho_refl (r : PyResult PVal) : resultRho r r = true := by
  cases r with
  | ok v => exact valRho_refl v
  | err e => simp [resultRho]

/-- **Instantiation on the proved arith fragment (shipped-idealized lowering).** For ANY
    ρ-faithful marshalling `M`, the composed boundary program `S (M (evalAjs e))` preserves
    — i.e. `M (evalAjs e) = M (evalA e)`. Routed THROUGH `compositionThm` (not `congrArg`):
    the hypothesis it consumes is `resultRho (evalAjs e env) (evalA e env) = true`, derived
    from the lattice's `preservationA` + ρ-reflexivity. (Here `evalAjs` is strictly equal
    to `evalA`, so `hM` is not yet exercised non-trivially — that is `_d1` below.) -/
theorem compositionThm_arith {β : Type} [DecidableEq β] (M : PVal → β)
    (hM : RhoFaithful M) (e : AExp) (env : Env) :
    resultMap M (evalAjs e env) = resultMap M (evalA e env) := by
  apply compositionThm M hM
  rw [preservationA e env]
  exact resultRho_refl (evalA e env)

/-- **The genuinely mod-ρ instantiation — where ρ-faithfulness is LOAD-BEARING.** Under the
    REAL shipped lowering `d1CollapseAL` (whole floats collapse to their int representation,
    D1), the emitted result differs from the reference in REPRESENTATION on whole-float
    successes (`preservationA_rho_holds_d1`: only mod ρ, not strictly). Yet ANY ρ-faithful
    marshalling composes cleanly — `hM` is exactly what absorbs the D1 gap at the boundary.
    A marshalling that is NOT ρ-faithful (e.g. `typeTag`) does NOT compose here — see the
    stub. -/
theorem compositionThm_arith_d1 {β : Type} [DecidableEq β] (M : PVal → β)
    (hM : RhoFaithful M) (e : AExp) (env : Env) :
    resultMap M (evalAtgt d1CollapseAL e env) = resultMap M (evalA e env) := by
  apply compositionThm M hM
  exact preservationA_rho_holds_d1 e env

/-- **STUB — ρ-faithfulness is load-bearing (the marshalling-boundary analogue of the D1 /
    C2 stub litmus).** A marshalling that LEAKS the type-tag is NOT ρ-faithful: `valRho
    (pfloat 2) (pint 2) = true` (D1 quotients whole-float ≡ int) yet `typeTag (pfloat 2) =
    tfloat ≠ tint = typeTag (pint 2)`. So `typeTag` violates the hypothesis. -/
theorem typeTag_not_rhoFaithful : ¬ RhoFaithful (β := PTag) typeTag := by
  intro h
  have hbad := h (.pfloat 2) (.pint 2) (by decide)   -- typeTag (pfloat 2) = typeTag (pint 2)
  exact absurd hbad (by decide)

/-- **STUB — the CONCLUSION of `compositionThm` FAILS for the non-ρ-faithful `typeTag`.**
    Witnessed by the ρ-equal pair `ok (pfloat 2)` / `ok (pint 2)` (D1): `resultMap typeTag`
    maps them to `ok tfloat` / `ok tint`, which DIFFER. So the `RhoFaithful` hypothesis
    genuinely constrains `M` — `compositionThm` is NOT vacuous: drop the hypothesis and the
    conclusion is false. -/
theorem compositionThm_typeTagStub_fails :
    ¬ (∀ r₁ r₂ : PyResult PVal, resultRho r₁ r₂ = true →
        resultMap typeTag r₁ = resultMap typeTag r₂) := by
  intro h
  have hbad := h (.ok (.pfloat 2)) (.ok (.pint 2)) (by decide)
  exact absurd hbad (by decide)

/-- **EVALUATOR-level D1 stub — self-contained in this section.** Under the SHIPPED
    D1-collapsing lowering, the non-ρ-faithful `typeTag` marshalling makes the compiled
    and reference results DIFFER on the whole-float program `2.0` — so the conclusion of
    `compositionThm_arith_d1` is FALSE at `M := typeTag`: the `RhoFaithful M` hypothesis
    cannot be dropped THERE either, not just at the raw-result level of
    `compositionThm_typeTagStub_fails`. (The same divergence the frozen lattice pins at
    the `resultTag` level; restated here so `_d1`'s load-bearing-ness needs no
    cross-reference.) -/
theorem compositionThm_arith_d1_typeTagStub_fails :
    resultMap typeTag (evalAtgt d1CollapseAL (.lit (.pfloat 2)) []) ≠
      resultMap typeTag (evalA (.lit (.pfloat 2)) []) := by decide

/-- **POSITIVE witness — the hypothesis class is NON-EMPTY, non-degenerately (Stage-5
    non-vacuity).** The shipped D1 collapse `d1c` (whole float → its int representation)
    IS ρ-faithful: ρ-equivalent values agree after collapsing, since ρ quotients exactly
    the tag `d1c` erases. And `d1c` is NON-constant (it distinguishes `pint 1` from
    `pint 2` — see the `#guard`s below), so `RhoFaithful` is inhabited by a genuinely
    value-distinguishing marshalling, not merely by constant maps: `compositionThm`'s
    hypothesis is satisfiable in the intended way. -/
theorem d1c_rhoFaithful : RhoFaithful d1c := by
  intro a b h
  cases a <;> cases b <;> simp_all [valRho, d1c]

-- d1c is non-constant (distinguishes distinct values) yet ρ-faithful (collapses exactly
-- the D1 pair) — the non-degenerate inhabitant of the hypothesis class:
#guard d1c (.pint 1) ≠ d1c (.pint 2)
#guard d1c (.pfloat 2) = d1c (.pint 2)

-- executable pins: the ρ-equal D1 pair a ρ-faithful M must equate, and the tags the
-- unfaithful typeTag splits them into (the discriminating witness).
#guard valRho (.pfloat 2) (.pint 2) = true
#guard resultRho (.ok (.pfloat 2)) (.ok (.pint 2)) = true
#guard resultMap typeTag (.ok (.pfloat 2)) = (.ok .tfloat : PyResult PTag)
#guard resultMap typeTag (.ok (.pint 2)) = (.ok .tint : PyResult PTag)

/-- info: 'PythExpandVerify.compositionThm' depends on axioms: [propext] -/
#guard_msgs in
#print axioms compositionThm

/-- info: 'PythExpandVerify.valRho_refl' depends on axioms: [propext] -/
#guard_msgs in
#print axioms valRho_refl

/-- info: 'PythExpandVerify.resultRho_refl' depends on axioms: [propext] -/
#guard_msgs in
#print axioms resultRho_refl

/-- info: 'PythExpandVerify.compositionThm_arith' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms compositionThm_arith

/-- info: 'PythExpandVerify.compositionThm_arith_d1' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms compositionThm_arith_d1

/-- info: 'PythExpandVerify.typeTag_not_rhoFaithful' depends on axioms: [propext] -/
#guard_msgs in
#print axioms typeTag_not_rhoFaithful

/-- info: 'PythExpandVerify.compositionThm_typeTagStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms compositionThm_typeTagStub_fails

/-- info: 'PythExpandVerify.compositionThm_arith_d1_typeTagStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms compositionThm_arith_d1_typeTagStub_fails

/-- info: 'PythExpandVerify.d1c_rhoFaithful' depends on axioms: [propext] -/
#guard_msgs in
#print axioms d1c_rhoFaithful

/-! ## §7b boundary composition — PART 2: the `M_dom` marshalling lemma (B-JS)

The concrete B-JS marshalling: a `list` comprehension result (`List PVal`) becomes DOM
CHILDREN. `childrenOf = List.map nodeOf` where `nodeOf : PVal → Node` renders one element
(code-point text / arbitrary-precision repr / `bool ⊂ int`, all validated DRY by the B-JS
interaction differential). `Node` is the ABSTRACT sink type — a bare `Type` variable,
trusted/opaque: we do NOT model React elements. `M_dom` is `childrenOf` proved
ρ-faithful AT THE LIST LEVEL, i.e. it is `RhoFaithful` lifted pointwise, in ORDER.

Two content axes (both are the concrete B-JS hazards):
  * **per-element repr** (C2 at the boundary): carried by `RhoFaithful nodeOf` — a per-node
    marshalling that ToString-16s an astral char or Number-truncates a bigint is not
    ρ-faithful (same shape as the `compositionThm` `typeTag` stub; here it would be caught
    by the B-JS astral/`~`-repr cases).
  * **order** (C5 at the boundary): `List.map` is order-preserving; the discriminating stub
    is a REORDERING marshalling, refuted below.

`Node` stays abstract in the positive lemma (true for ANY sink). The stub instantiates
`Node := PVal` with an injective marker (`id`) — the minimum needed to exhibit two DISTINCT
nodes so a reorder is observable. -/

/-- Pointwise ρ on child lists: same length, elementwise `valRho`, IN ORDER. The order is
    intrinsic to the definition (`a :: as ~ b :: bs` pairs heads with heads) — a reorder
    breaks it, which is exactly what the stub exploits. -/
def listRho : List PVal → List PVal → Bool
  | [], [] => true
  | (a :: as), (b :: bs) => valRho a b && listRho as bs
  | _, _ => false

/-- **`M_dom` — the marshalling lemma.** A ρ-faithful per-element `nodeOf`, mapped over
    child lists, is ρ-faithful at the LIST level: ρ-equivalent child lists (same length,
    elementwise ρ, in order) marshal to EQUAL DOM child lists. Genuine structural induction
    over the list (NOT `rfl`): the head uses `RhoFaithful nodeOf`, the tail the IH; the
    length-mismatch shapes are ruled out by `listRho` being `false` there. This is the B-JS
    instance of `compositionThm`, lifted from values to ordered collections. -/
theorem M_dom_rhoFaithful {Node : Type} [DecidableEq Node] (nodeOf : PVal → Node)
    (h : RhoFaithful nodeOf) (xs ys : List PVal) (hl : listRho xs ys = true) :
    xs.map nodeOf = ys.map nodeOf := by
  induction xs generalizing ys with
  | nil =>
    cases ys with
    | nil => rfl
    | cons b bs => simp [listRho] at hl            -- listRho [] (b::bs) ≡ false
  | cons a as ih =>
    cases ys with
    | nil => simp [listRho] at hl                  -- listRho (a::as) [] ≡ false
    | cons b bs =>
      simp only [listRho, Bool.and_eq_true] at hl
      obtain ⟨h1, h2⟩ := hl
      simp only [List.map, h a b h1, ih bs h2]

/-- The order-preserving DOM-children marshalling (`M_dom` itself, as a function). -/
def childrenOf {Node : Type} (nodeOf : PVal → Node) : List PVal → List Node :=
  List.map nodeOf

/-- A REORDERING marshalling — renders the same nodes but in REVERSE child order. Not
    order-faithful; the stub below refutes it. -/
def childrenOf_rev {Node : Type} (nodeOf : PVal → Node) : List PVal → List Node :=
  fun xs => (xs.reverse).map nodeOf

/-- **STUB — order is load-bearing (the C5-at-the-boundary axis).** The reordering
    marshalling is NOT equal to the order-preserving one: with the injective marker
    `nodeOf = id` (`Node := PVal`, so distinct elements give distinct nodes), the witness
    `[pint 1, pint 2]` marshals to `[n1, n2]` under `childrenOf` but `[n2, n1]` under
    `childrenOf_rev`, and `n1 ≠ n2`. So a compiler that reordered comprehension children
    would be caught — the B-JS suite's list-comp-order case, at the model level. -/
theorem M_dom_reorderStub_fails :
    ¬ (∀ xs : List PVal, childrenOf (Node := PVal) id xs = childrenOf_rev (Node := PVal) id xs) := by
  intro h
  have hbad := h [.pint 1, .pint 2]
  -- hbad : [pint 1, pint 2] = [pint 2, pint 1]
  exact absurd hbad (by decide)

-- executable pins: order matters (childrenOf ≠ childrenOf_rev on the witness), and the
-- positive marshalling agrees with itself in order.
#guard childrenOf (Node := PVal) id [.pint 1, .pint 2] = [.pint 1, .pint 2]
#guard childrenOf_rev (Node := PVal) id [.pint 1, .pint 2] = [.pint 2, .pint 1]

-- NOTE (dict → props, same shape, ADDITIONAL obligation): the `dict`-comprehension → JSX
-- props marshalling is the SAME order-preserving map, but must ALSO not COLLIDE keys —
-- distinct Python keys `1` / `'1'` must stay distinct prop names (Map-backed), never
-- silently merged. That hazard is already discharged, at the model level, by the wave-16
-- `jsObj_conflates` theorem (a stringifying prop marshalling conflates `1` and `'1'`);
-- the B-JS differential's dict-comp dedup / last-write-wins / insertion-order cases are its
-- empirical counterpart. We reuse `jsObj_conflates` here rather than restate it.

/-- info: 'PythExpandVerify.M_dom_rhoFaithful' depends on axioms: [propext] -/
#guard_msgs in
#print axioms M_dom_rhoFaithful

/-- info: 'PythExpandVerify.M_dom_reorderStub_fails' does not depend on any axioms -/
#guard_msgs in
#print axioms M_dom_reorderStub_fails

/-! ## §7b boundary composition — PART 3: `wasmRouteSafe` (B-WASM router safety)

The B-WASM boundary: the auto-router sends a pure-numeric kernel to the i64/f64 WASM fast
path. Reuses the existing WASM model verbatim (Tier-3 wave 8): `evalPyW` (arbitrary-
precision CPython reference), `evalW` (the emitted i64 wrapping path), `wrapI64`,
`preservationWasm` (`evalW e = (evalPyW e).map wrapI64`) and `preservationWasm_inRange`.

The router-safety guarantee (B-WASM differential DRY, `wasm_shipped_binding.py`): the
emitted WASM NEVER returns a SILENT WRONG VALUE. Model the two shipped outcomes with
`WasmOutcome`. The routers CONSUME the emitted-lowering model `evalW` directly (so a
drift in `evalW` falsifies the safety theorem — the binding is to the EMITTED evaluator,
not to a re-derived `wrapI64 ∘ evalPyW`). The two shipped targets guard it differently:
  * `--target wasm-edge` (`wasmRunEdge`): the `__ovf` guard fires exactly when the true
    value is out of i64 range → fail-loud `.overflowErr`; in range it returns the emitted
    i64 value, which `preservationWasm_inRange` proves EQUALS the CPython value.
  * `--target js+wasm` (`wasmRunJsWasm`, the primary shipped path): on overflow it
    transparently re-runs on the arbitrary-precision JS twin → the exact CPython value,
    no throw (modeled as the trusted twin = `evalPyW`).

SAFETY (`wasmRouteSafe`): a `.val n` outcome is ALWAYS the exact CPython value — never a
wrapped/silent-wrong one. The load-bearing content is the range GUARD: the stub `wrapRun`
(the naive i64 path with NO `__ovf` guard and NO twin) returns a WRONG `.val` on an
out-of-range witness and is refuted by the safety statement.

SCOPE (honest): `WExp` is the scalar-numeric fragment (`+`/`-`/`*`/`neg`, the existing
model). Comprehensions/collections NEVER reach WASM (admission rejects them —
the design notes §7b "handled by exclusion"), so `M_wasm`'s domain is exactly this
scalar fragment; that exclusion is a separate certified fact, not restated here. The WASM
host (trap handling) stays the abstract sink — we model the COMPILER's routing/guard, not
the host. -/

/-- The two shipped WASM boundary outcomes: a returned value, or a loud overflow error.
    A `.val` that is WRONG (silent wrap) is exactly what router-safety forbids; a wrapped
    result never appears as `.val` under the guarded routers, only under the stub. -/
inductive WasmOutcome where
  | val (n : Int) | overflowErr
  deriving DecidableEq, Repr

/-- **`--target wasm-edge` router** (fail-loud). Return the EMITTED i64 result — `evalW`,
    the emitted-lowering model itself, consumed DIRECTLY — ONLY when the true value is
    i64-representable; otherwise the `__ovf` guard raises `OverflowError`. Binding to
    `evalW` (not a re-derived `wrapI64 ∘ evalPyW`) means a drift in the emitted lowering
    falsifies `wasmRouteSafe`. `none` mirrors `evalPyW`'s unbound-variable case (not a
    value outcome). -/
def wasmRunEdge (e : WExp) (env : Env) : Option WasmOutcome :=
  match evalPyW e env with
  | none => none
  | some v =>
      if -(2 ^ 63) ≤ v ∧ v < 2 ^ 63
      then (evalW e env).map WasmOutcome.val -- in range: the EMITTED value (= CPython, thm)
      else some .overflowErr                 -- out of range: fail-loud, never silent-wrap

/-- **`--target js+wasm` router** (the primary shipped path, fail-SAFE). On overflow the
    wrapper re-runs on the arbitrary-precision JS twin → the exact CPython value, no throw.
    Modeled as the trusted twin returning `evalPyW` directly (the twin IS arbitrary
    precision); its safety is by construction — it never wraps and never throws. The
    load-bearing GUARD content lives in `wasmRunEdge` + the stub. -/
def wasmRunJsWasm (e : WExp) (env : Env) : Option WasmOutcome :=
  match evalPyW e env with
  | none => none
  | some v => some (.val v)

/-- **STUB router — the naive i64 lowering with NO `__ovf` guard and NO twin.** Returns
    the EMITTED `evalW` value UNCONDITIONALLY (same emitted-lowering binding as
    `wasmRunEdge`, guard deleted) — exactly the silent-wrap path router-safety must
    exclude. Refuted at an overflow witness below. -/
def wrapRun (e : WExp) (env : Env) : Option WasmOutcome :=
  (evalW e env).map WasmOutcome.val

/-- **`wasmRouteSafe` — router safety (correct-or-error, NEVER silent-wrong).** Whenever the
    edge router returns a value `.val n`, `n` is EXACTLY the CPython value — the i64 fast
    path never surfaces a wrapped/wrong value as a result. The proof's content is the range
    guard + `preservationWasm_inRange`, CONSUMED directly: the `.val` branch returns the
    emitted `evalW` value, which in range the preservation theorem proves equal to
    CPython's — so a drift in the emitted lowering `evalW` would falsify this theorem.
    Out of range the guard yields `.overflowErr`, never a `.val`. The naive `wrapRun` stub
    (same `evalW` binding, no guard) FAILS this — so the `__ovf` guard is load-bearing. -/
theorem wasmRouteSafe (e : WExp) (env : Env) (n : Int) :
    wasmRunEdge e env = some (.val n) → evalPyW e env = some n := by
  intro h
  simp only [wasmRunEdge] at h
  split at h
  · simp at h                                            -- none: none ≠ some _
  · rename_i v heq                                       -- heq : evalPyW e env = some v
    split at h
    · rename_i hr                                        -- hr : v in i64 range
      rw [preservationWasm_inRange e env v heq hr.1 hr.2, heq] at h
      -- h : (some v).map .val = some (.val n) — the EMITTED value, rewritten by the
      -- preservation theorem to the CPython value
      simp only [Option.map_some, Option.some.injEq, WasmOutcome.val.injEq] at h
      rw [heq, h]
    · simp at h                                          -- overflow: .overflowErr ≠ .val n

/-- Corollary — the primary `js+wasm` path is also safe (never a wrong value): its twin
    re-run returns the exact CPython value, so a `.val n` outcome is always `evalPyW`. -/
theorem wasmRouteSafe_jsWasm (e : WExp) (env : Env) (n : Int) :
    wasmRunJsWasm e env = some (.val n) → evalPyW e env = some n := by
  unfold wasmRunJsWasm
  cases hv : evalPyW e env with
  | none => intro h; simp at h
  | some v =>
    intro h
    simp only [Option.some.injEq, WasmOutcome.val.injEq] at h
    rw [h]

/-- **STUB — the `__ovf` guard is load-bearing.** The naive `wrapRun` (no guard, no twin)
    VIOLATES router safety: on the overflow witness `2^62 * 4 = 2^64` (out of i64 range) it
    returns the emitted `.val (evalW …) = .val 0` (`= wrapI64 (2^64)`, `preservationWasm`),
    a SILENT WRONG value, while CPython gives `2^64`.
    So `0 ≠ 2^64` refutes the safety property for `wrapRun` — the exact wrapping bug the
    shipped guard/twin prevent (and the B-WASM differential confirmed absent). -/
theorem wasmRouteSafe_wrapStub_fails :
    ¬ (∀ (e : WExp) (env : Env) (n : Int),
        wrapRun e env = some (.val n) → evalPyW e env = some n) := by
  intro h
  have hbad := h (.mul (.lit (2 ^ 62)) (.lit 4)) [] 0 (by decide)
  -- hbad : evalPyW (2^62 * 4) [] = some 0 — but it is some (2^64)
  exact absurd hbad (by decide)

-- executable pins: in-range → exact value; overflow → loud error (edge) / exact via twin
-- (js+wasm); the naive stub silently wraps 2^64 → 0 (the divergence safety forbids).
#guard wasmRunEdge (.sub (.mul (.lit 3) (.lit 4)) (.lit 20)) [] = some (.val (-8))   -- in-range
#guard wasmRunEdge (.mul (.lit (2 ^ 62)) (.lit 4)) [] = some .overflowErr             -- overflow → loud
#guard wasmRunJsWasm (.mul (.lit (2 ^ 62)) (.lit 4)) [] = some (.val (2 ^ 64))        -- overflow → exact via twin
#guard wrapRun (.mul (.lit (2 ^ 62)) (.lit 4)) [] = some (.val 0)                     -- STUB silently wraps
#guard evalPyW (.mul (.lit (2 ^ 62)) (.lit 4)) [] = some (2 ^ 64)                     -- CPython truth

/-- SPOT (through `wasmRouteSafe`, not a `#guard`): the in-range program `3*4 - 20` routed
    to edge WASM returns a value, and safety pins it to the exact CPython `-8`. Fails if the
    safety statement is weakened to not determine the value. -/
example : wasmRunEdge (.sub (.mul (.lit 3) (.lit 4)) (.lit 20)) [] = some (.val (-8)) →
    evalPyW (.sub (.mul (.lit 3) (.lit 4)) (.lit 20)) [] = some (-8) :=
  wasmRouteSafe _ _ _

/-- info: 'PythExpandVerify.wasmRouteSafe' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms wasmRouteSafe

/-- info: 'PythExpandVerify.wasmRouteSafe_jsWasm' depends on axioms: [propext] -/
#guard_msgs in
#print axioms wasmRouteSafe_jsWasm

/-- info: 'PythExpandVerify.wasmRouteSafe_wrapStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms wasmRouteSafe_wrapStub_fails

/-! ## Preservation lattice v2 — THE UNION: BROAD observational preservation modulo exceptions

Design: the design notes §2/§5/§6 (the join). The lattice sections above
prove each projection on its OWN fragment (arith → C1∧C2∧C3∧C4; subscript → C4×3;
short-circuit/async → C5). This section is the CENTREPIECE: a SINGLE unified evaluator
over ONE **representative combined fragment** — arithmetic (`//` with floor + zero-
guard, `%`), subscript (index / key / non-subscriptable / bad-key-type), boolean short-
circuit (`or`/`and`), and genuine unbounded recursion (`ucount` terminating countdown,
`uloop` divergence) — over a carrier that carries the WHOLE observation at once:
`PyResult UVal × UTrace`, fuel-indexed by a step budget so termination is expressible.

**The claim.** From the ONE equation

    preservationUnion (e : UExpr) (fuel : Nat) : evalUjs fuel e = evalUpy fuel e

the compiled and source semantics agree on the **FULL observation** — value, type
(modulo D1), error-occurrence, error-kind, effect/print-order, and termination — at
every fuel. This is **BROAD (complete-over-the-fragment) observational semantic
preservation MODULO the documented exception set**, NOT a claim of "six independent
axes". The named projections below are `congrArg` slices of the ONE observation, and
they are DEPENDENT, not orthogonal: the type tag is a FUNCTION of the value
(`obsTagModD1_factors`: C2 ⊂ C1); error-occurrence is `isErr`, the definedness edge of
error-kind (C3 ⊂ C4-carrier); halting is `isSome` (C6 = definedness of the option); and
effect-ORDER (C5) and TERMINATION (C6) are two observable consequences of the SAME
lazy-vs-eager compiler choice (`eager`), refuted by two distinct witnesses. We list the
slices to show the union *entails* each, not to assert they are independent knobs.

**Shipping-binding (the point).** The Lean equation is between two IN-LEAN models; on
its own it says nothing about the product. `experiments/pbt-ps/union_shipped_binding.py`
CLOSES that gap: it compiles each fragment program with the REAL `pyths`, runs the
emitted JS under node, runs the same source under CPython, and asserts the full
observation agrees `model == pyths-run == CPython` (binding `evalUjs`→real `pyths`,
`evalUpy`→real CPython) on value / kind / order / termination, and confirms the D1 tag
collapse on whole-floats. So the proved model equation corresponds to the SHIPPED
compiler on the fragment (DRY; see the harness report).

**Non-tautology discipline (F1).** `evalUpy` is an INDEPENDENT reference recursion
(hardcoded CPython semantics); `evalUtgt L` is the parameterized target;
`evalUjs := evalUtgt jsUL` is the shipped-lowering model. The equation is proved by
REAL structural-with-fuel induction (the `//` value arm needs `jsFdiv_eq_fdiv`, a
genuinely different operation from `Int.fdiv`), NOT `rfl`. The union has TEETH: wrong
lowerings are refuted per-projection on value, error-occurrence, error-kind, effect-
order and termination (the `*Stub_fails` theorems) — including the C3 zero-guard knob
(`guardZero`) the earlier `preservationA` lacked (the fable F-1 gap).

**Honest scope + the documented exception set.** The shipping-bound fragment is
INT-TYPED (int / list-of-int / dict int→int), where — as the harness confirms DRY —
shipped `pyths` is observationally IDENTICAL to CPython on all six components. The
exception set (the ONLY places compiled ≠ CPython) is:
  * **D1 — whole-float type TAG.** Shipped `pyths` UNTAGS a whole float: `type(2.0)`
    reports `int` (harness-confirmed) while CPython reports `float`. `d1UL`/
    `evalUtgt_d1_untag` MODEL exactly this TAG collapse — nothing more: `d1UL`'s
    `reprVal := uCollapse` maps `.ufloat n → .uint n`, so the model captures
    `type(2.0)→int` (observed through `obsTag`/`obsTagModD1`), and the union observes
    the tag MODULO D1 (`obsTagModD1`), which equals shipped's `type()`. The SEPARATE
    runtime fact that shipped KEEPS the repr `2.0` (`print(2.0)`→`2.0`) is NOT modeled
    by `d1UL` (which collapses the whole float to an int in the model); it is a
    documented runtime deviation the harness witnesses directly against `pyths`, listed
    below. So the modeled D1 exception is the tag collapse ONLY. NOTE: the Lean theorem
    `preservationUnion` is UNIVERSAL over `UExpr` — INCLUDING `.ufloat` literals, which
    under the idealized `jsUL` (`reprVal := id`) stay float-tagged and so match CPython (no
    deviation inside the theorem). D1 is the empirical fact that the REAL shipped `pyths`
    behaves like `d1UL` (untags whole floats), NOT `jsUL`; the harness binds `jsUL`↔`pyths`
    on the INT-typed corpus and witnesses the `d1UL` tag deviation on the whole-float cases
    separately. This is the CompCert observable-behaviour quotient, not a hidden asterisk.
  * **Whole-float repr** — shipped keeps `2.0`'s repr while reporting `int` type; a
    runtime fact harness-witnessed, OUTSIDE the `d1UL`/`uCollapse` model (which is a
    tag-observation model, not a repr-faithful one). Not claimed as modeled.
  * **Message text** — exception KIND, not message string, is the modeled observation.
Lifting to all 16 C1 waves is CONJECTURED to be (largely) mechanical future work — migrate each wave's evaluator to
the `PyResult`×`UTrace`×fuel carrier). Reuses `Exc`/`PyResult`/`jsFdiv`/`jsFdiv_eq_fdiv`
verbatim; purely additive, no existing wave or section is touched. -/

/-- The combined fragment's value domain. `ufloat n` is the WHOLE float `n.0` under a
    float tag (D1's domain — Float-free, decidable). Lists/dicts are Int-payloaded (as
    in the subscript section). `DecidableEq` drives the `decide` stub refutations. -/
inductive UVal where
  | uint (n : Int)
  | ufloat (n : Int)
  | ulist (xs : List Int)
  | udict (ps : List (Int × Int))
  deriving DecidableEq, Repr

/-- Python type tags — the C2 observation over the combined domain. -/
inductive UTag where
  | tint | tfloat | tlist | tdict
  deriving DecidableEq, Repr

/-- The C2 projection on values. -/
def uTypeTag : UVal → UTag
  | .uint _ => .tint | .ufloat _ => .tfloat | .ulist _ => .tlist | .udict _ => .tdict

/-- Python truthiness (nonzero number / nonempty container is truthy) — governs the
    short-circuit control flow (C5) and, via a diverging RHS, termination (C6). -/
def uTruthy : UVal → Bool
  | .uint n => decide (n ≠ 0)
  | .ufloat n => decide (n ≠ 0)
  | .ulist xs => decide (xs ≠ [])
  | .udict ps => decide (ps ≠ [])

/-- The D1 representation collapse (whole float → its int) — the C2 stub knob. -/
def uCollapse : UVal → UVal
  | .ufloat n => .uint n
  | v => v

/-- The observable effect trace (C5): opaque `String` labels, in execution order. -/
abbrev UTrace := List String

/-- The combined-fragment expression language:
    * `ulit` / `uleaf` / `ufail` — a literal / an effect-emitting int leaf / an
      effect-emitting raise (leaves carry the C5 trace labels);
    * `ufdiv` — floor `//` with the zero-guard (C1 value, C3 occurrence, C4 zero-kind);
    * `ufmod` — floor `%` with the zero-guard (C1 value = Python floor-mod, sign of
      divisor; C3 occurrence, C4 zero-kind — the `%` sibling of `ufdiv`);
    * `uidx` — subscript `c[k]` (C4 index/key/type kinds, C1 negative-index wrap);
    * `uor` / `uand` — Python short-circuit (C5 order, and C6 when the RHS diverges);
    * `ucount m` — a terminating countdown `while n>0: n-=1` (genuine fuel-consuming
      recursion, C6 positive);
    * `uloop` — `while True: pass` (genuine divergence, C6). -/
inductive UExpr where
  | ulit (v : UVal)
  | uleaf (n : Int) (eff : String)
  | ufail (ex : Exc) (eff : String)
  | ufdiv (a b : UExpr)
  | ufmod (a b : UExpr)
  | uidx (c k : UExpr)
  | uor (a b : UExpr)
  | uand (a b : UExpr)
  | ucount (m : Int)
  | uloop
  deriving Repr

/-- **Reference (CPython) semantics**, fuel-indexed: `none` = fuel exhausted (the
    divergence proxy), `some (result, trace)` otherwise. Faithfulness (kind, not
    message; each pinned by a `#guard` below):
    * left-to-right evaluation, error short-circuits;
    * `//`: TYPE check (str/list/dict operand → `TypeError`) precedes ZERO check
      (`int//0` → `ZeroDivisionError`); whole-float operands propagate a float tag;
    * subscript: `list[i]` wraps a negative index once then bounds-checks
      (`IndexError`); `dict[k]` first-match (`KeyError`); non-subscriptable / bad key
      type → `TypeError`;
    * `or`/`and` return the OPERAND value and SHORT-CIRCUIT (skip the RHS effect);
    * `ucount` terminates (needs fuel ≥ m+1); `uloop` never yields a value. -/
def evalUpy : Nat → UExpr → Option (PyResult UVal × UTrace)
  | 0, _ => none
  | _+1, .ulit v => some (.ok v, [])
  | _+1, .uleaf n eff => some (.ok (.uint n), [eff])
  | _+1, .ufail ex eff => some (.err ex, [eff])
  | fuel+1, .ufdiv a b =>
      match evalUpy fuel a with
      | none => none
      | some (.err e, ta) => some (.err e, ta)
      | some (.ok va, ta) =>
          match evalUpy fuel b with
          | none => none
          | some (.err e, tb) => some (.err e, ta ++ tb)
          | some (.ok vb, tb) =>
              match va, vb with
              | .uint x, .uint y =>
                  if y = 0 then some (.err .zeroDiv, ta ++ tb)
                  else some (.ok (.uint (Int.fdiv x y)), ta ++ tb)
              | .uint x, .ufloat y =>
                  if y = 0 then some (.err .zeroDiv, ta ++ tb)
                  else some (.ok (.ufloat (Int.fdiv x y)), ta ++ tb)
              | .ufloat x, .uint y =>
                  if y = 0 then some (.err .zeroDiv, ta ++ tb)
                  else some (.ok (.ufloat (Int.fdiv x y)), ta ++ tb)
              | .ufloat x, .ufloat y =>
                  if y = 0 then some (.err .zeroDiv, ta ++ tb)
                  else some (.ok (.ufloat (Int.fdiv x y)), ta ++ tb)
              | _, _ => some (.err .typeError, ta ++ tb)
  | fuel+1, .ufmod a b =>
      match evalUpy fuel a with
      | none => none
      | some (.err e, ta) => some (.err e, ta)
      | some (.ok va, ta) =>
          match evalUpy fuel b with
          | none => none
          | some (.err e, tb) => some (.err e, ta ++ tb)
          | some (.ok vb, tb) =>
              match va, vb with
              | .uint x, .uint y =>
                  if y = 0 then some (.err .zeroDiv, ta ++ tb)
                  else some (.ok (.uint (Int.fmod x y)), ta ++ tb)
              | .uint x, .ufloat y =>
                  if y = 0 then some (.err .zeroDiv, ta ++ tb)
                  else some (.ok (.ufloat (Int.fmod x y)), ta ++ tb)
              | .ufloat x, .uint y =>
                  if y = 0 then some (.err .zeroDiv, ta ++ tb)
                  else some (.ok (.ufloat (Int.fmod x y)), ta ++ tb)
              | .ufloat x, .ufloat y =>
                  if y = 0 then some (.err .zeroDiv, ta ++ tb)
                  else some (.ok (.ufloat (Int.fmod x y)), ta ++ tb)
              | _, _ => some (.err .typeError, ta ++ tb)
  | fuel+1, .uidx c k =>
      match evalUpy fuel c with
      | none => none
      | some (.err e, tc) => some (.err e, tc)
      | some (.ok cv, tc) =>
          match evalUpy fuel k with
          | none => none
          | some (.err e, tk) => some (.err e, tc ++ tk)
          | some (.ok kv, tk) =>
              match cv, kv with
              | .ulist xs, .uint i =>
                  let i' := if i < 0 then i + (xs.length : Int) else i
                  if 0 ≤ i' ∧ i' < (xs.length : Int) then some (.ok (.uint xs[i'.toNat]!), tc ++ tk)
                  else some (.err .indexError, tc ++ tk)
              | .udict ps, .uint key =>
                  match ps.find? (fun p => p.1 == key) with
                  | some p => some (.ok (.uint p.2), tc ++ tk)
                  | none => some (.err .keyError, tc ++ tk)
              | _, _ => some (.err .typeError, tc ++ tk)
  | fuel+1, .uor a b =>
      match evalUpy fuel a with
      | none => none
      | some (.err e, ta) => some (.err e, ta)
      | some (.ok va, ta) =>
          if uTruthy va then some (.ok va, ta)
          else match evalUpy fuel b with
               | none => none
               | some (rb, tb) => some (rb, ta ++ tb)
  | fuel+1, .uand a b =>
      match evalUpy fuel a with
      | none => none
      | some (.err e, ta) => some (.err e, ta)
      | some (.ok va, ta) =>
          if uTruthy va then
            match evalUpy fuel b with
            | none => none
            | some (rb, tb) => some (rb, ta ++ tb)
          else some (.ok va, ta)
  | fuel+1, .ucount m =>
      if m ≤ 0 then some (.ok (.uint 0), [])
      else evalUpy fuel (.ucount (m - 1))
  | fuel+1, .uloop => evalUpy fuel .uloop

-- F9 faithfulness pins — the REFERENCE matches CPython on every axis (verified vs
-- CPython 3.12): value, error site/KIND, evaluation order, short-circuit, termination.
#guard evalUpy 5 (.ufdiv (.uleaf (-7) "a") (.uleaf 2 "b")) = some (.ok (.uint (-4)), ["a", "b"])   -- floor
#guard evalUpy 5 (.ufmod (.uleaf (-7) "a") (.uleaf 2 "b")) = some (.ok (.uint 1), ["a", "b"])       -- % sign of divisor
#guard evalUpy 5 (.ufmod (.uleaf 7 "a") (.uleaf (-2) "b")) = some (.ok (.uint (-1)), ["a", "b"])    -- % negative divisor
#guard evalUpy 5 (.ufmod (.uleaf 5 "a") (.uleaf 0 "b")) = some (.err .zeroDiv, ["a", "b"])          -- %0 raises
#guard evalUpy 5 (.ufmod (.ulit (.ulist [1])) (.uleaf 0 "b")) = some (.err .typeError, ["b"])       -- % TYPE before ZERO
#guard evalUpy 5 (.ufdiv (.uleaf 1 "a") (.uleaf 0 "b")) = some (.err .zeroDiv, ["a", "b"])          -- //0
#guard evalUpy 5 (.ufdiv (.ulit (.ulist [1])) (.uleaf 0 "b")) = some (.err .typeError, ["b"])       -- TYPE before ZERO
#guard evalUpy 5 (.uidx (.ulit (.ulist [10, 20, 30])) (.ulit (.uint (-1)))) = some (.ok (.uint 30), [])  -- wrap
#guard evalUpy 5 (.uidx (.ulit (.ulist [10, 20])) (.ulit (.uint 5))) = some (.err .indexError, [])       -- OOR
#guard evalUpy 5 (.uidx (.ulit (.udict [(1, 10)])) (.ulit (.uint 9))) = some (.err .keyError, [])        -- missing key
#guard evalUpy 5 (.uidx (.ulit (.uint 5)) (.ulit (.uint 0))) = some (.err .typeError, [])                -- not subscriptable
#guard evalUpy 5 (.uor (.uleaf 1 "a") (.ufail .zeroDiv "b")) = some (.ok (.uint 1), ["a"])          -- or short-circuits (no raise)
#guard evalUpy 5 (.uand (.uleaf 0 "a") (.ufail .zeroDiv "b")) = some (.ok (.uint 0), ["a"])         -- and short-circuits
#guard evalUpy 10 (.ucount 5) = some (.ok (.uint 0), [])                                            -- countdown terminates
#guard evalUpy 3 (.ucount 5) = none                                                                -- ...but needs fuel ≥ m+1
#guard evalUpy 100 .uloop = (none : Option (PyResult UVal × UTrace))                                -- while True never yields
#guard evalUpy 5 (.ufail .zeroDiv "a") = some (.err .zeroDiv, ["a"])                                -- C2/D1: float tag kept
#guard uTypeTag (.ufloat 2) = UTag.tfloat

/-- A UNIFIED LOWERING — the parameters over the compiler CHOICES this fragment
    exercises. The SAME predicate is proved for the shipped `jsUL` and REFUTED by
    deliberately-wrong settings, so the union has teeth. The parameters are NOT claimed
    independent (value determines tag; order and termination share `eager`):
    * `fdivOp`   — how emitted JS lowers `//` (value); refuted by `truncUL`;
    * `fmodOp`   — how emitted JS lowers `%` (value); the `%` sibling of `fdivOp`,
                   proved `= Int.fmod` for `jsFmod` via `jsFmod_eq_fmod`;
    * `reprVal`  — how a produced value is represented; used to MODEL the shipped D1
                   whole-float untagging (`d1UL := {jsUL with reprVal := uCollapse}`),
                   NOT a wrong-lowering knob — the D1 deviation is a documented
                   exception the harness witnesses, observed via `obsTagModD1`;
    * `guardZero`— whether `//0` is guarded (error-occurrence); a `false` returns a
                   value where CPython raises (THE fable F-1 gap); refuted by `noGuardUL`;
    * `zeroExc`/`indexExc`/`keyExc`/`typeExc` — the exception CLASS at each error site
                   (error-kind); refuted by `kindUL`;
    * `eager`    — the lazy-vs-eager short-circuit strategy; this ONE choice has TWO
                   observable consequences — which effects run (ORDER) and whether a
                   divergent RHS is reached (TERMINATION) — refuted by two distinct
                   witnesses (`eagerUL` on a finite trace, and on `1 or (while True)`). -/
structure UnifiedLowering where
  fdivOp    : Int → Int → Int
  fmodOp    : Int → Int → Int
  reprVal   : UVal → UVal
  guardZero : Bool
  zeroExc   : Exc
  indexExc  : Exc
  keyExc    : Exc
  typeExc   : Exc
  eager     : Bool

/-- The SHIPPED lowering on the INT-TYPED fragment: floor-corrected `//`, int values
    as-is (`reprVal := id`; the shipped whole-float untagging — D1 — is the documented
    exception, modeled by `d1UL` and observed via `obsTagModD1`, harness-witnessed),
    `//0` guarded, the four Pythonic error kinds, lazy short-circuit. On this fragment
    `evalUjs = evalUpy` is exact AND shipping-faithful (harness: model==pyths==CPython). -/
def jsUL : UnifiedLowering := ⟨jsFdiv, jsFmod, id, true, .zeroDiv, .indexError, .keyError, .typeError, false⟩

/-- **Independent target evaluator** under lowering `L` — a SEPARATE recursion (not a
    flag on `evalUpy`); identical structure, but the `//` value, the produced-value
    representation, the zero-guard, the four exception kinds, and the short-circuit
    strategy are ALL the lowering's. `evalUjs := evalUtgt jsUL` is the shipped model. -/
def evalUtgt (L : UnifiedLowering) : Nat → UExpr → Option (PyResult UVal × UTrace)
  | 0, _ => none
  | _+1, .ulit v => some (.ok (L.reprVal v), [])
  | _+1, .uleaf n eff => some (.ok (L.reprVal (.uint n)), [eff])
  | _+1, .ufail ex eff => some (.err ex, [eff])
  | fuel+1, .ufdiv a b =>
      match evalUtgt L fuel a with
      | none => none
      | some (.err e, ta) => some (.err e, ta)
      | some (.ok va, ta) =>
          match evalUtgt L fuel b with
          | none => none
          | some (.err e, tb) => some (.err e, ta ++ tb)
          | some (.ok vb, tb) =>
              match va, vb with
              | .uint x, .uint y =>
                  if y = 0 then
                    (if L.guardZero then some (.err L.zeroExc, ta ++ tb)
                     else some (.ok (L.reprVal (.uint (L.fdivOp x y))), ta ++ tb))
                  else some (.ok (L.reprVal (.uint (L.fdivOp x y))), ta ++ tb)
              | .uint x, .ufloat y =>
                  if y = 0 then
                    (if L.guardZero then some (.err L.zeroExc, ta ++ tb)
                     else some (.ok (L.reprVal (.ufloat (L.fdivOp x y))), ta ++ tb))
                  else some (.ok (L.reprVal (.ufloat (L.fdivOp x y))), ta ++ tb)
              | .ufloat x, .uint y =>
                  if y = 0 then
                    (if L.guardZero then some (.err L.zeroExc, ta ++ tb)
                     else some (.ok (L.reprVal (.ufloat (L.fdivOp x y))), ta ++ tb))
                  else some (.ok (L.reprVal (.ufloat (L.fdivOp x y))), ta ++ tb)
              | .ufloat x, .ufloat y =>
                  if y = 0 then
                    (if L.guardZero then some (.err L.zeroExc, ta ++ tb)
                     else some (.ok (L.reprVal (.ufloat (L.fdivOp x y))), ta ++ tb))
                  else some (.ok (L.reprVal (.ufloat (L.fdivOp x y))), ta ++ tb)
              | _, _ => some (.err L.typeExc, ta ++ tb)
  | fuel+1, .ufmod a b =>
      match evalUtgt L fuel a with
      | none => none
      | some (.err e, ta) => some (.err e, ta)
      | some (.ok va, ta) =>
          match evalUtgt L fuel b with
          | none => none
          | some (.err e, tb) => some (.err e, ta ++ tb)
          | some (.ok vb, tb) =>
              match va, vb with
              | .uint x, .uint y =>
                  if y = 0 then
                    (if L.guardZero then some (.err L.zeroExc, ta ++ tb)
                     else some (.ok (L.reprVal (.uint (L.fmodOp x y))), ta ++ tb))
                  else some (.ok (L.reprVal (.uint (L.fmodOp x y))), ta ++ tb)
              | .uint x, .ufloat y =>
                  if y = 0 then
                    (if L.guardZero then some (.err L.zeroExc, ta ++ tb)
                     else some (.ok (L.reprVal (.ufloat (L.fmodOp x y))), ta ++ tb))
                  else some (.ok (L.reprVal (.ufloat (L.fmodOp x y))), ta ++ tb)
              | .ufloat x, .uint y =>
                  if y = 0 then
                    (if L.guardZero then some (.err L.zeroExc, ta ++ tb)
                     else some (.ok (L.reprVal (.ufloat (L.fmodOp x y))), ta ++ tb))
                  else some (.ok (L.reprVal (.ufloat (L.fmodOp x y))), ta ++ tb)
              | .ufloat x, .ufloat y =>
                  if y = 0 then
                    (if L.guardZero then some (.err L.zeroExc, ta ++ tb)
                     else some (.ok (L.reprVal (.ufloat (L.fmodOp x y))), ta ++ tb))
                  else some (.ok (L.reprVal (.ufloat (L.fmodOp x y))), ta ++ tb)
              | _, _ => some (.err L.typeExc, ta ++ tb)
  | fuel+1, .uidx c k =>
      match evalUtgt L fuel c with
      | none => none
      | some (.err e, tc) => some (.err e, tc)
      | some (.ok cv, tc) =>
          match evalUtgt L fuel k with
          | none => none
          | some (.err e, tk) => some (.err e, tc ++ tk)
          | some (.ok kv, tk) =>
              match cv, kv with
              | .ulist xs, .uint i =>
                  let i' := if i < 0 then i + (xs.length : Int) else i
                  if 0 ≤ i' ∧ i' < (xs.length : Int) then some (.ok (L.reprVal (.uint xs[i'.toNat]!)), tc ++ tk)
                  else some (.err L.indexExc, tc ++ tk)
              | .udict ps, .uint key =>
                  match ps.find? (fun p => p.1 == key) with
                  | some p => some (.ok (L.reprVal (.uint p.2)), tc ++ tk)
                  | none => some (.err L.keyExc, tc ++ tk)
              | _, _ => some (.err L.typeExc, tc ++ tk)
  | fuel+1, .uor a b =>
      if L.eager then
        match evalUtgt L fuel a with
        | none => none
        | some (ra, ta) =>
            match evalUtgt L fuel b with
            | none => none
            | some (rb, tb) =>
                match ra with
                | .err e => some (.err e, ta ++ tb)
                | .ok va =>
                    match rb with
                    | .err e => some (.err e, ta ++ tb)
                    | .ok vb => if uTruthy va then some (.ok va, ta ++ tb) else some (.ok vb, ta ++ tb)
      else
        match evalUtgt L fuel a with
        | none => none
        | some (.err e, ta) => some (.err e, ta)
        | some (.ok va, ta) =>
            if uTruthy va then some (.ok va, ta)
            else match evalUtgt L fuel b with
                 | none => none
                 | some (rb, tb) => some (rb, ta ++ tb)
  | fuel+1, .uand a b =>
      if L.eager then
        match evalUtgt L fuel a with
        | none => none
        | some (ra, ta) =>
            match evalUtgt L fuel b with
            | none => none
            | some (rb, tb) =>
                match ra with
                | .err e => some (.err e, ta ++ tb)
                | .ok va =>
                    match rb with
                    | .err e => some (.err e, ta ++ tb)
                    | .ok vb => if uTruthy va then some (.ok vb, ta ++ tb) else some (.ok va, ta ++ tb)
      else
        match evalUtgt L fuel a with
        | none => none
        | some (.err e, ta) => some (.err e, ta)
        | some (.ok va, ta) =>
            if uTruthy va then
              match evalUtgt L fuel b with
              | none => none
              | some (rb, tb) => some (rb, ta ++ tb)
            else some (.ok va, ta)
  | fuel+1, .ucount m =>
      if m ≤ 0 then some (.ok (L.reprVal (.uint 0)), [])
      else evalUtgt L fuel (.ucount (m - 1))
  | fuel+1, .uloop => evalUtgt L fuel .uloop

/-- The compiled combined-fragment semantics: the independent target under the shipped
    lowering. -/
abbrev evalUjs : Nat → UExpr → Option (PyResult UVal × UTrace) := evalUtgt jsUL

-- Compiled `%` binds to Python floor-mod (the emitted `jsFmod` correction), not a
-- truncated `%`: `-7 % 2 → 1` (sign of divisor), `%0` raises. `ufmod` is now a genuine
-- part of the union, so the 7 `%` corpus cases in `union_shipped_binding.py` bind.
#guard evalUjs 5 (.ufmod (.uleaf (-7) "a") (.uleaf 2 "b")) = some (.ok (.uint 1), ["a", "b"])
#guard evalUjs 5 (.ufmod (.uleaf 7 "a") (.uleaf (-2) "b")) = some (.ok (.uint (-1)), ["a", "b"])
#guard evalUjs 5 (.ufmod (.uleaf 5 "a") (.uleaf 0 "b")) = some (.err .zeroDiv, ["a", "b"])

/-- Preservation as a predicate OVER the lowering — the SAME predicate is proved for
    `jsUL` and refuted for each of the six axis stubs. -/
def UPreserves (L : UnifiedLowering) : Prop := ∀ fuel e, evalUtgt L fuel e = evalUpy fuel e

/-- **THE UNION (core induction).** The compiled evaluator equals the CPython reference
    on the WHOLE combined fragment, at every fuel — one equation over
    `PyResult UVal × UTrace` that simultaneously carries value (C1), type-tag (C2),
    error-occurrence (C3), error-kind (C4), effect-trace (C5) and halting (C6). Real
    structural-with-fuel induction: induction on fuel, then on the expression; the `//`
    value arm is discharged by `jsFdiv_eq_fdiv`, every other arm by the identical
    match structure once `jsUL`'s knobs reduce. -/
theorem preservationUnion_core : ∀ (fuel : Nat) (e : UExpr), evalUtgt jsUL fuel e = evalUpy fuel e := by
  intro fuel
  induction fuel with
  | zero => intro e; rfl
  | succ k ih =>
    intro e
    cases e with
    | ulit v => simp [evalUtgt, evalUpy, jsUL]
    | uleaf n eff => simp [evalUtgt, evalUpy, jsUL]
    | ufail ex eff => rfl
    | ufdiv a b =>
      simp only [evalUtgt, evalUpy, ih a, ih b]
      cases evalUpy k a with
      | none => rfl
      | some pa =>
        obtain ⟨ra, ta⟩ := pa
        cases ra with
        | err e => rfl
        | ok va =>
          cases evalUpy k b with
          | none => rfl
          | some pb =>
            obtain ⟨rb, tb⟩ := pb
            cases rb with
            | err e => rfl
            | ok vb =>
              cases va with
              | uint x =>
                cases vb with
                | uint y =>
                  by_cases hy : y = 0
                  · simp [hy, jsUL]
                  · simp [hy, jsUL, jsFdiv_eq_fdiv x y hy]
                | ufloat y =>
                  by_cases hy : y = 0
                  · simp [hy, jsUL]
                  · simp [hy, jsUL, jsFdiv_eq_fdiv x y hy]
                | ulist _ => rfl
                | udict _ => rfl
              | ufloat x =>
                cases vb with
                | uint y =>
                  by_cases hy : y = 0
                  · simp [hy, jsUL]
                  · simp [hy, jsUL, jsFdiv_eq_fdiv x y hy]
                | ufloat y =>
                  by_cases hy : y = 0
                  · simp [hy, jsUL]
                  · simp [hy, jsUL, jsFdiv_eq_fdiv x y hy]
                | ulist _ => rfl
                | udict _ => rfl
              | ulist _ => cases vb <;> rfl
              | udict _ => cases vb <;> rfl
    | ufmod a b =>
      simp only [evalUtgt, evalUpy, ih a, ih b]
      cases evalUpy k a with
      | none => rfl
      | some pa =>
        obtain ⟨ra, ta⟩ := pa
        cases ra with
        | err e => rfl
        | ok va =>
          cases evalUpy k b with
          | none => rfl
          | some pb =>
            obtain ⟨rb, tb⟩ := pb
            cases rb with
            | err e => rfl
            | ok vb =>
              cases va with
              | uint x =>
                cases vb with
                | uint y =>
                  by_cases hy : y = 0
                  · simp [hy, jsUL]
                  · simp [hy, jsUL, jsFmod_eq_fmod x y hy]
                | ufloat y =>
                  by_cases hy : y = 0
                  · simp [hy, jsUL]
                  · simp [hy, jsUL, jsFmod_eq_fmod x y hy]
                | ulist _ => rfl
                | udict _ => rfl
              | ufloat x =>
                cases vb with
                | uint y =>
                  by_cases hy : y = 0
                  · simp [hy, jsUL]
                  · simp [hy, jsUL, jsFmod_eq_fmod x y hy]
                | ufloat y =>
                  by_cases hy : y = 0
                  · simp [hy, jsUL]
                  · simp [hy, jsUL, jsFmod_eq_fmod x y hy]
                | ulist _ => rfl
                | udict _ => rfl
              | ulist _ => cases vb <;> rfl
              | udict _ => cases vb <;> rfl
    | uidx c kk =>
      simp only [evalUtgt, evalUpy, ih c, ih kk]
      cases evalUpy k c with
      | none => rfl
      | some pc =>
        obtain ⟨rc, tc⟩ := pc
        cases rc with
        | err e => rfl
        | ok cv =>
          cases evalUpy k kk with
          | none => rfl
          | some pk =>
            obtain ⟨rk, tk⟩ := pk
            cases rk with
            | err e => rfl
            | ok kv =>
              cases cv with
              | ulist xs =>
                cases kv with
                | uint i => simp [jsUL]
                | ufloat _ => rfl
                | ulist _ => rfl
                | udict _ => rfl
              | udict ps =>
                cases kv with
                | uint key => cases ps.find? (fun p => p.1 == key) <;> simp [jsUL]
                | ufloat _ => rfl
                | ulist _ => rfl
                | udict _ => rfl
              | uint _ => cases kv <;> rfl
              | ufloat _ => cases kv <;> rfl
    | uor a b =>
      simp only [evalUtgt, evalUpy, ih a, ih b]
      cases evalUpy k a with
      | none => simp [jsUL]
      | some pa =>
        obtain ⟨ra, ta⟩ := pa
        cases ra with
        | err e => simp [jsUL]
        | ok va =>
          cases hv : uTruthy va with
          | true => simp [jsUL, hv]
          | false =>
            cases evalUpy k b with
            | none => simp [jsUL, hv]
            | some pb => obtain ⟨rb, tb⟩ := pb; simp [jsUL, hv]
    | uand a b =>
      simp only [evalUtgt, evalUpy, ih a, ih b]
      cases evalUpy k a with
      | none => simp [jsUL]
      | some pa =>
        obtain ⟨ra, ta⟩ := pa
        cases ra with
        | err e => simp [jsUL]
        | ok va =>
          cases hv : uTruthy va with
          | true =>
            cases evalUpy k b with
            | none => simp [jsUL, hv]
            | some pb => obtain ⟨rb, tb⟩ := pb; simp [jsUL, hv]
          | false => simp [jsUL, hv]
    | ucount m =>
      simp only [evalUtgt, evalUpy, jsUL]
      by_cases hm : m ≤ 0
      · simp [hm]
      · simp only [hm, if_false]; exact ih (.ucount (m - 1))
    | uloop =>
      simp only [evalUtgt, evalUpy]; exact ih .uloop

/-- **`preservationUnion` — the centrepiece, in the brief's exact shape.** -/
theorem preservationUnion (e : UExpr) (fuel : Nat) : evalUjs fuel e = evalUpy fuel e :=
  preservationUnion_core fuel e

/-- The predicate-form instantiation the six axis stubs contrast against. -/
theorem preservationUnion_real : UPreserves jsUL := fun fuel e => preservationUnion_core fuel e

/-! ### The six axis PROJECTIONS (each a `congrArg` of the ONE union equation) -/

/-- C1 value observation. -/
def obsValue : Option (PyResult UVal × UTrace) → Option UVal
  | some (.ok v, _) => some v
  | _ => none
/-- C2 type-tag observation (RAW — the CPython tag; keeps `float`). -/
def obsTag : Option (PyResult UVal × UTrace) → Option UTag
  | some (.ok v, _) => some (uTypeTag v)
  | _ => none
/-- C2 type-tag observation MODULO D1 — the shipped `type()`: a whole float is
    untagged to `int` (`uTypeTag ∘ uCollapse`). This is the tag axis the union
    preserves and that binds to `pyths` (`type(2.0)`→`int`, harness-confirmed). -/
def obsTagModD1 : Option (PyResult UVal × UTrace) → Option UTag
  | some (.ok v, _) => some (uTypeTag (uCollapse v))
  | _ => none
/-- C3 error-occurrence observation (`none` = out of fuel; `some b` = did it raise). -/
def obsErr : Option (PyResult UVal × UTrace) → Option Bool
  | some (r, _) => some r.isErr
  | none => none
/-- C4 error-kind observation. -/
def obsKind : Option (PyResult UVal × UTrace) → Option Exc
  | some (.err e, _) => some e
  | _ => none
/-- C5 effect-trace observation. -/
def obsTrace : Option (PyResult UVal × UTrace) → Option UTrace
  | some (_, t) => some t
  | none => none
/-- C6 halting observation (single-fuel). -/
def obsHalts : Option (PyResult UVal × UTrace) → Bool
  | some _ => true | none => false

/-! ### Lean↔Python drift guard — a serialized FULL-observation signature

`experiments/pbt-ps/union_shipped_binding.py` runs a Python TRANSLITERATION of this
`evalUpy` recursion. That is an unguarded seam. `unionObsSig` serializes the WHOLE
observation (halting · occurrence · kind · value · tag-mod-D1 · effect-trace) into ONE
canonical string; the harness generates a scratch `.lean` that `#eval IO.println`s
`unionObsSig (evalUpy FUEL ·)` on EVERY corpus program and asserts it equals the
Python twin's signature — so the transliteration cannot silently drift from the model
this file proves about. -/
private def uExcName : Exc → String
  | .typeError => "TypeError" | .valueError => "ValueError" | .zeroDiv => "ZeroDivisionError"
  | .nameError => "NameError" | .indexError => "IndexError" | .keyError => "KeyError"
  | .overflow => "OverflowError" | .attributeError => "AttributeError"
  | .stopIteration => "StopIteration" | .custom n => n

private def uTagName : UTag → String
  | .tint => "int" | .tfloat => "float" | .tlist => "list" | .tdict => "dict"

/-- The observation signature (mod-D1 on the tag) — the string the drift guard compares
    against the Python transliteration for every corpus program. Records halting, error-
    occurrence, error-kind, value, type-tag (mod-D1) and effect-trace. NOTE: container
    VALUES are coarsened to their type-name (`list`/`dict`, contents dropped) — so it is a
    full VALUE observation for SCALAR (int/float) results (what the int-typed corpus
    produces), but NOT for container final values. -/
def unionObsSig : Option (PyResult UVal × UTrace) → String
  | none => "H0|occ-|kind-|val-|tag-|trace-"
  | some (res, tr) =>
    let traceStr := String.intercalate "," tr
    match res with
    | .err e => "H1|occ1|kind" ++ uExcName e ++ "|val-|tag-|trace" ++ traceStr
    | .ok v =>
      let valStr := match v with
        | .uint n => toString n | .ufloat n => toString n
        | .ulist _ => "list" | .udict _ => "dict"
      "H1|occ0|kind-|val" ++ valStr ++ "|tag" ++ uTagName (uTypeTag (uCollapse v))
        ++ "|trace" ++ traceStr

/-- **C1 (value) projection.** -/
theorem preservationUnion_value (e : UExpr) (fuel : Nat) :
    obsValue (evalUjs fuel e) = obsValue (evalUpy fuel e) :=
  congrArg obsValue (preservationUnion e fuel)
/-- **C2 (type-tag, modulo D1) projection.** The compiled `type()` (whole-float
    untagged) equals CPython's, modulo the documented D1 quotient. -/
theorem preservationUnion_tag (e : UExpr) (fuel : Nat) :
    obsTagModD1 (evalUjs fuel e) = obsTagModD1 (evalUpy fuel e) :=
  congrArg obsTagModD1 (preservationUnion e fuel)
/-- **C2 ⊂ C1 (dependency, not independence).** The mod-D1 tag is a FUNCTION of the
    value observation — so tag preservation is a COROLLARY of value preservation, not
    a separate axis. This is why the union lists projections, it does not claim them
    orthogonal. -/
theorem obsTagModD1_factors (r : Option (PyResult UVal × UTrace)) :
    obsTagModD1 r = (obsValue r).map (fun v => uTypeTag (uCollapse v)) := by
  cases r with
  | none => rfl
  | some p => obtain ⟨pr, t⟩ := p; cases pr <;> rfl
/-- **C3 (error-occurrence) projection.** -/
theorem preservationUnion_occurrence (e : UExpr) (fuel : Nat) :
    obsErr (evalUjs fuel e) = obsErr (evalUpy fuel e) :=
  congrArg obsErr (preservationUnion e fuel)
/-- **C4 (error-kind) projection.** -/
theorem preservationUnion_kind (e : UExpr) (fuel : Nat) :
    obsKind (evalUjs fuel e) = obsKind (evalUpy fuel e) :=
  congrArg obsKind (preservationUnion e fuel)
/-- **C5 (effect-trace) projection.** -/
theorem preservationUnion_trace (e : UExpr) (fuel : Nat) :
    obsTrace (evalUjs fuel e) = obsTrace (evalUpy fuel e) :=
  congrArg obsTrace (preservationUnion e fuel)
/-- **C6 (single-fuel halting) projection.** -/
theorem preservationUnion_halts (e : UExpr) (fuel : Nat) :
    obsHalts (evalUjs fuel e) = obsHalts (evalUpy fuel e) :=
  congrArg obsHalts (preservationUnion e fuel)

/-! ### C6 — termination: the `terminates` predicate, fuel-monotonicity, preservation -/

/-- **Termination**: some fuel budget makes the evaluator halt. -/
def terminatesU (ev : Nat → UExpr → Option (PyResult UVal × UTrace)) (e : UExpr) : Prop :=
  ∃ fuel, obsHalts (ev fuel e) = true

/-- **FUEL-MONOTONICITY (makes `terminatesU` well-defined).** If the reference halts at
    some fuel with result `r`, it halts with the SAME `r` at any larger budget — so
    "there exists a halting fuel" is a stable notion. Strong-ish induction on the
    budget; the combining arms use the IH to lift each already-`some` sub-evaluation. -/
theorem evalUpy_fuel_mono : ∀ (n : Nat) (e : UExpr) (r : PyResult UVal × UTrace),
    evalUpy n e = some r → evalUpy (n + 1) e = some r := by
  intro n
  induction n with
  | zero => intro e r h; simp [evalUpy] at h
  | succ k ih =>
    intro e r h
    cases e with
    | ulit v => exact h
    | uleaf n eff => exact h
    | ufail ex eff => exact h
    | ufdiv a b =>
      simp only [evalUpy] at h ⊢
      cases ha : evalUpy k a with
      | none => simp [ha] at h
      | some pa =>
        obtain ⟨ra, ta⟩ := pa
        simp only [ha] at h
        simp only [ih a _ ha]
        cases ra with
        | err e => exact h
        | ok va =>
          cases hb : evalUpy k b with
          | none => simp [hb] at h
          | some pb =>
            obtain ⟨rb, tb⟩ := pb
            simp only [hb] at h
            simp only [ih b _ hb]
            exact h
    | ufmod a b =>
      simp only [evalUpy] at h ⊢
      cases ha : evalUpy k a with
      | none => simp [ha] at h
      | some pa =>
        obtain ⟨ra, ta⟩ := pa
        simp only [ha] at h
        simp only [ih a _ ha]
        cases ra with
        | err e => exact h
        | ok va =>
          cases hb : evalUpy k b with
          | none => simp [hb] at h
          | some pb =>
            obtain ⟨rb, tb⟩ := pb
            simp only [hb] at h
            simp only [ih b _ hb]
            exact h
    | uidx c k' =>
      simp only [evalUpy] at h ⊢
      cases hc : evalUpy k c with
      | none => simp [hc] at h
      | some pc =>
        obtain ⟨rc, tc⟩ := pc
        simp only [hc] at h
        simp only [ih c _ hc]
        cases rc with
        | err e => exact h
        | ok cv =>
          cases hk : evalUpy k k' with
          | none => simp [hk] at h
          | some pk =>
            obtain ⟨rk, tk⟩ := pk
            simp only [hk] at h
            simp only [ih k' _ hk]
            exact h
    | uor a b =>
      simp only [evalUpy] at h ⊢
      cases ha : evalUpy k a with
      | none => simp [ha] at h
      | some pa =>
        obtain ⟨ra, ta⟩ := pa
        simp only [ha] at h
        simp only [ih a _ ha]
        cases ra with
        | err e => exact h
        | ok va =>
          cases hv : uTruthy va with
          | true => simp only [hv] at h ⊢; exact h
          | false =>
            simp only [hv] at h ⊢
            cases hb : evalUpy k b with
            | none => simp [hb] at h
            | some pb =>
              obtain ⟨rb, tb⟩ := pb
              simp only [hb] at h
              simp only [ih b _ hb]; exact h
    | uand a b =>
      simp only [evalUpy] at h ⊢
      cases ha : evalUpy k a with
      | none => simp [ha] at h
      | some pa =>
        obtain ⟨ra, ta⟩ := pa
        simp only [ha] at h
        simp only [ih a _ ha]
        cases ra with
        | err e => exact h
        | ok va =>
          cases hv : uTruthy va with
          | true =>
            simp only [hv] at h ⊢
            cases hb : evalUpy k b with
            | none => simp [hb] at h
            | some pb =>
              obtain ⟨rb, tb⟩ := pb
              simp only [hb] at h
              simp only [ih b _ hb]; exact h
          | false => simp only [hv] at h ⊢; exact h
    | ucount m =>
      simp only [evalUpy] at h ⊢
      by_cases hm : m ≤ 0
      · rw [if_pos hm] at h ⊢; exact h
      · rw [if_neg hm] at h ⊢; exact ih (.ucount (m - 1)) _ h
    | uloop =>
      simp only [evalUpy] at h ⊢
      exact ih .uloop _ h

/-- **C6 termination is PRESERVED** — immediate from the union equation (the halting
    projection at every fuel), no separate induction needed. -/
theorem preservationUnion_terminates (e : UExpr) :
    terminatesU evalUjs e ↔ terminatesU evalUpy e := by
  constructor
  · rintro ⟨fuel, h⟩; exact ⟨fuel, (preservationUnion_halts e fuel) ▸ h⟩
  · rintro ⟨fuel, h⟩; exact ⟨fuel, (preservationUnion_halts e fuel).symm ▸ h⟩

/-! ### The TEETH — each PROJECTION refuted by a wrong lowering; and the D1 exception model

Per-projection refuters (the codex ask): each certifies that ITS observation is
load-bearing — a deliberately-wrong lowering breaks THAT projection of the union, so
the union is not vacuous on value / error-occurrence / error-kind / effect-order /
termination. (Tag is a corollary of value — `obsTagModD1_factors`, C2 ⊂ C1 — so it
needs no separate refuter; the D1 whole-float behavior is the documented EXCEPTION,
modeled below, not a wrong lowering.) -/

/-- Value refuter: raw truncating `//` (`-7//2 → -3` not floor `-4`). -/
def truncUL : UnifiedLowering := { jsUL with fdivOp := Int.tdiv }
/-- **`d1UL` — the D1 TAG-EXCEPTION MODEL (not a refuter).** Models the whole-float
    TAG collapse ONLY: `reprVal := uCollapse` maps `.ufloat n → .uint n`, so under
    `d1UL` a whole float is observed as `int` — exactly `pyths`'s `type(2.0)`→`int`
    (harness-witnessed). It equals `jsUL` on the int-typed fragment (`uCollapse` is
    inert on ints). What `d1UL` does NOT model: the shipped runtime KEEPS the repr
    `2.0` (`print(2.0)`→`2.0`) — `uCollapse` instead collapses the whole float to an
    int in the model, so `d1UL` is a TAG-observation model, not a repr/value-faithful
    one. That repr-retention is a documented runtime deviation the harness witnesses
    directly, not a modeled claim. -/
def d1UL : UnifiedLowering := { jsUL with reprVal := uCollapse }
/-- Occurrence refuter (THE fable F-1 gap): NO zero-guard — `1//0` returns a value where
    CPython raises `ZeroDivisionError`. The occurrence knob the old `preservationA` lacked. -/
def noGuardUL : UnifiedLowering := { jsUL with guardZero := false }
/-- Kind refuter: `ValueError` at the zero site where CPython raises `ZeroDivisionError`
    (value-correct, errors in the right place — only the CLASS is wrong). -/
def kindUL : UnifiedLowering := { jsUL with zeroExc := .valueError }
/-- Order/termination refuter: EAGER short-circuit (evaluates both operands always). -/
def eagerUL : UnifiedLowering := { jsUL with eager := true }

/-- **Value projection has teeth.** `-7 // 2`: floor `-4` vs trunc `-3`. Refutes the
    value projection specifically (`obsValue`), not merely the whole equation. -/
theorem preservationUnion_valueStub_fails :
    ¬ (∀ (fuel : Nat) (e : UExpr),
        obsValue (evalUtgt truncUL fuel e) = obsValue (evalUpy fuel e)) := by
  intro h
  have hc := h 5 (.ufdiv (.uleaf (-7) "a") (.uleaf 2 "b"))
  exact absurd hc (by decide)

/-- **Error-occurrence projection has teeth — the fable F-1 gap CLOSED.** `1 // 0`:
    CPython RAISES, the no-guard lowering silently returns a VALUE; the `obsErr`
    projection differs — a defect the frozen `preservationA` could not express. -/
theorem preservationUnion_occurrenceStub_fails :
    ¬ (∀ (fuel : Nat) (e : UExpr),
        obsErr (evalUtgt noGuardUL fuel e) = obsErr (evalUpy fuel e)) := by
  intro h
  have hc := h 5 (.ufdiv (.uleaf 1 "a") (.uleaf 0 "b"))
  exact absurd hc (by decide)

/-- **Error-kind projection has teeth.** `1 // 0`: CPython `ZeroDivisionError` vs stub
    `ValueError` — errors exactly where CPython errors, only the CLASS wrong; the
    `obsKind` projection differs even though `obsErr` and `obsValue` agree. -/
theorem preservationUnion_kindStub_fails :
    ¬ (∀ (fuel : Nat) (e : UExpr),
        obsKind (evalUtgt kindUL fuel e) = obsKind (evalUpy fuel e)) := by
  intro h
  have hc := h 5 (.ufdiv (.uleaf 1 "a") (.uleaf 0 "b"))
  exact absurd hc (by decide)

/-- **Effect-order projection has teeth.** `1 or 5` with effects: lazy skips the RHS
    (`["a"]`), eager runs it (`["a","b"]`) — the `obsTrace` projection is refuted even
    though the VALUE agrees (`1`). C5 and C6 share the `eager` knob. -/
theorem preservationUnion_orderStub_fails :
    ¬ (∀ (fuel : Nat) (e : UExpr),
        obsTrace (evalUtgt eagerUL fuel e) = obsTrace (evalUpy fuel e)) := by
  intro h
  have hc := h 5 (.uor (.uleaf 1 "a") (.uleaf 5 "b"))
  exact absurd hc (by decide)

/-- **D1 TAG-exception, MODELED and non-vacuous.** On a whole-float literal the SHIPPED
    lowering (`d1UL`) reports type `int` (untagged) while the CPython reference reports
    `float` — this IS the documented D1 TAG deviation (harness-confirmed on `pyths`:
    `type(2.0)`→`int`). The union factors through it: observed MODULO D1
    (`obsTagModD1`) the two agree (`int`). The theorem is about the TAG only (repr
    retention is a separate, harness-witnessed runtime fact, not modeled here). So the
    D1 tag collapse is a real, witnessed exception the union carries, not a hidden
    asterisk. -/
theorem evalUtgt_d1_untag (n : Int) :
    obsTag (evalUtgt d1UL 1 (.ulit (.ufloat n))) = some UTag.tint
    ∧ obsTag (evalUpy 1 (.ulit (.ufloat n))) = some UTag.tfloat
    ∧ obsTagModD1 (evalUtgt d1UL 1 (.ulit (.ufloat n)))
        = obsTagModD1 (evalUpy 1 (.ulit (.ufloat n))) := by
  refine ⟨rfl, rfl, rfl⟩

/-- The C6 divergence witness: `1 or (while True)`. The lazy reference short-circuits
    (halts, returns `1`); eager evaluation reaches the diverging RHS. -/
def uDivWit : UExpr := .uor (.uleaf 1 "a") .uloop

/-- Under the EAGER lowering, `uloop` never yields a value at any fuel. -/
theorem evalUtgt_eager_uloop (fuel : Nat) : evalUtgt eagerUL fuel .uloop = none := by
  induction fuel with
  | zero => rfl
  | succ k ih => simp only [evalUtgt]; exact ih

/-- Under the EAGER lowering, the divergence witness diverges at EVERY fuel: eager
    evaluates the `uloop` operand, which is `none`, so the whole `or` is `none`. -/
theorem evalUtgt_eager_uDivWit (fuel : Nat) : evalUtgt eagerUL fuel uDivWit = none := by
  cases fuel with
  | zero => rfl
  | succ k =>
    simp only [uDivWit, evalUtgt]
    rw [evalUtgt_eager_uloop k]
    cases evalUtgt eagerUL k (.uleaf 1 "a") <;> simp [eagerUL]

/-- **Termination projection has teeth — the divergence refuter.** The EAGER lowering
    FAILS to preserve termination: on `1 or (while True)` the reference HALTS (short-
    circuits to `1`) while eager DIVERGES (never halts, at any fuel).

    NB (correcting a prior overclaim): eager is NOT value-faithful in general — it is
    the SAME wrong `eager` knob that the order refuter catches, and on e.g. `1 or raise`
    eager ERRORS where the reference returns `1` (value + occurrence differ too). On the
    divergence witness `1 or (while True)` ALL of value/trace/halting differ (eager is
    `none`); the SHARP fact this refuter certifies is precisely that TERMINATION is not
    preserved. C5 (order) and C6 (termination) are two consequences of the one `eager`
    choice, refuted by two distinct witnesses — not independent axes. -/
theorem preservationUnion_terminationStub_fails :
    (¬ terminatesU (evalUtgt eagerUL) uDivWit) ∧ terminatesU evalUpy uDivWit := by
  refine ⟨?_, ?_⟩
  · rintro ⟨fuel, h⟩
    rw [evalUtgt_eager_uDivWit fuel] at h
    simp [obsHalts] at h
  · exact ⟨3, by decide⟩

-- Concrete per-projection contrasts (each a plausible naive emission diverging from the
-- reference on its own witness — discriminating pins):
#guard obsValue (evalUtgt truncUL 5 (.ufdiv (.uleaf (-7) "a") (.uleaf 2 "b"))) = some (.uint (-3))   -- value X (real: -4)
#guard obsErr (evalUtgt noGuardUL 5 (.ufdiv (.uleaf 1 "a") (.uleaf 0 "b"))) = some false              -- occurrence X (real: true)
#guard obsKind (evalUtgt kindUL 5 (.ufdiv (.uleaf 1 "a") (.uleaf 0 "b"))) = some Exc.valueError       -- kind X (real: zeroDiv)
#guard obsTrace (evalUtgt eagerUL 5 (.uor (.uleaf 1 "a") (.uleaf 5 "b"))) = some ["a", "b"]           -- order X (real: ["a"])
#guard evalUtgt eagerUL 9 uDivWit = none                                                              -- termination X (real: halts)
-- The D1 EXCEPTION (documented): shipped `d1UL` untags the whole float, matching pyths,
-- differing from CPython's raw tag — but agreeing MODULO D1 (obsTagModD1):
#guard obsTag (evalUtgt d1UL 1 (.ulit (.ufloat 2))) = some UTag.tint                                  -- shipped: int (D1)
#guard obsTag (evalUpy 1 (.ulit (.ufloat 2))) = some UTag.tfloat                                      -- CPython: float
#guard obsTagModD1 (evalUtgt d1UL 1 (.ulit (.ufloat 2))) = obsTagModD1 (evalUpy 1 (.ulit (.ufloat 2))) -- agree mod-D1
-- ...and the reference, on the SAME refuter witnesses, gives the RIGHT answer on every axis:
#guard obsValue (evalUpy 5 (.ufdiv (.uleaf (-7) "a") (.uleaf 2 "b"))) = some (.uint (-4))
#guard obsErr (evalUpy 5 (.ufdiv (.uleaf 1 "a") (.uleaf 0 "b"))) = some true
#guard obsKind (evalUpy 5 (.ufdiv (.uleaf 1 "a") (.uleaf 0 "b"))) = some Exc.zeroDiv
#guard obsTrace (evalUpy 5 (.uor (.uleaf 1 "a") (.uleaf 5 "b"))) = some ["a"]
#guard obsHalts (evalUpy 3 uDivWit) = true

/-- SPOT (C1, through the theorem): the compiled `-7 // 2` floors to `-4`. -/
example : obsValue (evalUjs 5 (.ufdiv (.uleaf (-7) "a") (.uleaf 2 "b"))) = some (.uint (-4)) := by
  rw [preservationUnion_value]; rfl
/-- SPOT (C4, through the theorem): the compiled `1 // 0` raises `ZeroDivisionError`. -/
example : obsKind (evalUjs 5 (.ufdiv (.uleaf 1 "a") (.uleaf 0 "b"))) = some Exc.zeroDiv := by
  rw [preservationUnion_kind]; rfl
/-- SPOT (C5, through the theorem): the compiled `1 or 5` SKIPS the RHS effect. -/
example : obsTrace (evalUjs 5 (.uor (.uleaf 1 "a") (.uleaf 5 "b"))) = some ["a"] := by
  rw [preservationUnion_trace]; rfl
/-- SPOT (C6, through the termination theorem): the compiled `1 or (while True)`
    terminates iff the reference does — and the reference does (short-circuit). -/
example : terminatesU evalUjs uDivWit :=
  (preservationUnion_terminates uDivWit).mpr ⟨3, by decide⟩
/-- SPOT (C2 mod-D1, through the theorem): the compiled `[10,20,30][-1]` reports the
    right (int) type — the tag axis is bound to the value equation, modulo D1. -/
example : obsTagModD1 (evalUjs 5 (.uidx (.ulit (.ulist [10, 20, 30])) (.ulit (.uint (-1)))))
    = some UTag.tint := by
  rw [preservationUnion_tag]; rfl
/-- SPOT (D1 exception): the compiled whole-float literal is observed as `int`
    (shipped untag), MODULO the documented D1 quotient — pinned to the runtime by the
    harness (`pyths type(2.0)`→`int`). -/
example : obsTagModD1 (evalUtgt d1UL 1 (.ulit (.ufloat 2))) = some UTag.tint := by decide

-- Per-declaration axiom pins (Stage-5 gate; captured from a real build). Every
-- headline union theorem is within the pinned trio {propext, Classical.choice,
-- Quot.sound}; the six stubs + fuel-monotonicity are `[propext]`-only.

/-- info: 'PythExpandVerify.preservationUnion' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationUnion

/-- info: 'PythExpandVerify.preservationUnion_core' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationUnion_core

/-- info: 'PythExpandVerify.preservationUnion_real' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationUnion_real

/-- info: 'PythExpandVerify.preservationUnion_value' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationUnion_value

/-- info: 'PythExpandVerify.preservationUnion_tag' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationUnion_tag

/-- info: 'PythExpandVerify.preservationUnion_occurrence' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationUnion_occurrence

/-- info: 'PythExpandVerify.preservationUnion_kind' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationUnion_kind

/-- info: 'PythExpandVerify.preservationUnion_trace' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationUnion_trace

/-- info: 'PythExpandVerify.preservationUnion_halts' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationUnion_halts

/-- info: 'PythExpandVerify.evalUpy_fuel_mono' depends on axioms: [propext] -/
#guard_msgs in
#print axioms evalUpy_fuel_mono

/-- info: 'PythExpandVerify.preservationUnion_terminates' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationUnion_terminates

/-- info: 'PythExpandVerify.preservationUnion_valueStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationUnion_valueStub_fails

-- C2 tag is a projection of the value equation (obsTagModD1_factors) and the D1
-- exception is MODELED (evalUtgt_d1_untag), so the tag axis has no separate refuter.
/-- info: 'PythExpandVerify.obsTagModD1_factors' depends on axioms: [propext] -/
#guard_msgs in
#print axioms obsTagModD1_factors

/-- info: 'PythExpandVerify.evalUtgt_d1_untag' depends on axioms: [propext] -/
#guard_msgs in
#print axioms evalUtgt_d1_untag

/-- info: 'PythExpandVerify.preservationUnion_occurrenceStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationUnion_occurrenceStub_fails

/-- info: 'PythExpandVerify.preservationUnion_kindStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationUnion_kindStub_fails

/-- info: 'PythExpandVerify.preservationUnion_orderStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationUnion_orderStub_fails

/-- info: 'PythExpandVerify.preservationUnion_terminationStub_fails' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationUnion_terminationStub_fails

/-! ## MarshalTable — the JS↔WASM value-marshalling boundary as a finite table

Lean twin of the SHIPPED JS↔WASM value boundary in
`crates/pyths_codegen_wasm/src/bridge.rs` (`convert_js_to_wasm` /
`convert_wasm_to_js` / `list_elem_kind` + the `__i64Oob` argument guard, the
`__list_to_wasm` i64 element guard, the sticky `__ovf` exactness flag and the
#364 fallback ladder `__isWasmFault`), gated by the #364 numeric-kernel
boundary admission in `crates/pyths_hir/src/wasm_analysis.rs`
(`is_numeric_kernel_param` / `is_scalar_wasm_return`).

**Binding (two-sided, the same shape as route-table.txt /
wasm-admission-table.txt).** The committed fixture
`verification/marshalling-table.txt` is regenerated and compared from BOTH
sides: Rust derives it from the shipping functions themselves
(`bridge::marshalling_table()`, checked by
`cargo test marshalling_table_matches_committed_fixture`; the failure
dispositions are derived from REAL probe bridges with loud-panic snippet
assertions, never restated), and this model prints byte-identical text
(`lake exe expanddiff --check-marshalling-table`). Either side changing a
conversion, an admission bit, or a disposition breaks its own gate. The
runtime semantics behind the rows is confirmed by the shipped-binding
differential `verification/marshalling_shipped_binding.py` (real `pyths`
js+wasm vs CPython over boundary-crossing values).

**The claims (HONEST scope).**
  * `marshal_param_admitted_sound` / `marshal_ret_admitted_sound` — every
    boundary type the #364 admission accepts marshals ONLY through the
    value-exact numeric converter classes (i64-exact-guarded / f64-identity /
    bool-i32 / numeric-list); pointer marshalling (str/dict/tuple/closure/
    nested lists) is formally EXCLUDED from the admitted surface (an honest
    operation-level boundary: those conversions exist in the glue but #364
    keeps them off the fast path precisely because they are not exactness-
    disciplined).
  * `i64ArgMarshal_exact` + `i64_boundary_roundtrip` — the i64 crossing (the
    only lossy-prone scalar) either passes the EXACT value (in i64 range) or
    diverts (reroute-to-twin / throw) — NEVER a silently wrapped pass. The
    guard is load-bearing: `i64Marshal_unguardedStub_fails` refutes the
    unguarded conversion (`wrapI64`, i.e. raw ES `ToBigInt64`) — which was the
    REAL pre-fix shipped behavior of the `__list_to_wasm` i64 element path
    (PoC 2026-08-16: `pick([2**63+7])` returned −9223372036854775801; fixed by
    the element RangeError guard, disposition row `list-elem-i64-oob`).
  * `marshal_exhaustive` — the finite table is exhaustive over the WHOLE
    (infinite) representable domain: every `WasmRepr`'s conversion equals a
    table row's, because the conversion depends only on head constructor +
    element kind, all witnessed in the alphabet.

**Trust boundary (stated).** The exactness of `BigInt(Math.trunc(x))` /
`Number(bigint)` inside their stated ranges and float identity are JS-engine
semantics (host-owned, like the rest of the runtime); the runtime int
representation invariant (ints beyond 2⁵³ are BigInt, `JsInt.wf`) is the
runtime's documented representation, exercised by the differential. The f64
crossing is bit-identity on a native Number (with the glue-local `__f64Arg`
int→float coercion for the hybrid large-int form — #38/#465: the standard
`Number(bigint)` conversion in double range, and OverflowError "int too
large to convert to float" beyond it, matching the runtime authority
`__reqNum` and CPython's int-in-float-position conversion), with
the D1 whole-float display deviation owned by the existing D1 rows, not
here. Client↔server (RPC) and JS↔TS
boundaries are the SAME table shape but 0.3.x scope — NOT built here. -/

/-- #364 boundary admission for parameters — `is_numeric_kernel_param`,
    arm-for-arm: numeric scalars and FLAT lists of numeric scalars only. -/
def isNumericKernelParam : WasmTy → Bool
  | .int | .float | .bool => true
  | .list i =>
    match i with
    | .int | .float | .bool => true
    | _ => false
  | _ => false

/-- #364 boundary admission for returns — `is_scalar_wasm_return`,
    arm-for-arm: numeric scalar or a no-value return. -/
def isScalarWasmReturn : WasmTy → Bool
  | .int | .float | .bool | .wnone | .void => true
  | _ => false

/-- `list_elem_kind`, arm-for-arm: the element slot a list's contents cross
    the boundary in (f64/i64 wide slots; EVERYTHING else an i32 slot). -/
def listElemKind : WasmRepr → String
  | .f64 => "f64"
  | .i64 => "i64"
  | _ => "i32"

/-- `convert_js_to_wasm("x", ·)`, arm-for-arm — the LITERAL conversion
    expression the glue emits for a JS argument crossing into WASM. -/
def jsToWasmExpr : WasmRepr → String
  | .i64 => "(typeof x === \"bigint\" ? x : BigInt(Math.trunc(x)))"
  -- #38/#461/#465 value-boundary authority: identity on a native Number, but
  -- a large int arrives as a BigInt (the hybrid form past 2⁵³) and ToNumber
  -- at the WASM JS-API boundary THROWS on BigInt — so the glue-local
  -- `__f64Arg` coerces it with the standard int→float conversion, and (#465)
  -- a BigInt beyond IEEE-754 double range raises OverflowError ("int too
  -- large to convert to float"), the SAME rule as the runtime authority
  -- `__reqNum` and as CPython's int→float conversion — never a silent
  -- Infinity crossing.
  | .f64 => "__f64Arg(x)"
  | .i32 => "x ? 1 : 0"
  | .ptr => "__str_to_wasm(x)"
  | .ptrList i => "__list_to_wasm(x, \"" ++ listElemKind i ++ "\")"
  | .ptrDict _ _ => "__dict_to_wasm(x)"
  | .ptrTuple _ _ => "__tuple_to_wasm(x)"
  | .ptrClosure _ _ => "__closure_to_wasm(x)"

/-- `convert_wasm_to_js("x", ·)`, arm-for-arm — the conversion for a WASM
    result crossing back to JS. -/
def wasmToJsExpr : WasmRepr → String
  | .i64 => "__i64ToJs(x)"
  -- Option B (minimal int/float fidelity): an f64 result re-enters JS
  -- through the glue-local `__f64Box` — a VALUE-IDENTICAL tagged box
  -- (`Number.isInteger(v) ? new __PyFloatW(v) : v`; `valueOf()` returns
  -- the same double bit-for-bit), applied iff the result is
  -- integer-valued so 12.0 keeps its Python float identity. Still in the
  -- value-exact scalar class: no rounding, no truncation, no pointer.
  | .f64 => "__f64Box(x)"
  | .i32 => "Boolean(x)"
  | .ptr => "__str_from_wasm(x)"
  | .ptrList i => "__list_from_wasm(x, \"" ++ listElemKind i ++ "\")"
  | .ptrDict _ _ => "__dict_from_wasm(x)"
  | .ptrTuple _ _ => "__tuple_from_wasm(x)"
  | .ptrClosure _ _ => "__closure_from_wasm(x)"

/-- Whether the bridge embeds exact JS twins (`--target js+wasm`) or not
    (edge targets: workers/wasi/deno). Both ship. -/
inductive BridgeMode where
  | twins | notwins
  deriving DecidableEq, Repr

/-- The boundary failure events the glue disposes of explicitly. -/
inductive FaultEvent where
  | i64ArgOob      -- BigInt argument outside i64 (pre-call `__i64Oob` guard)
  | listElemI64Oob -- i64 LIST ELEMENT outside i64 (`__list_to_wasm` guard)
  | ovfFlag        -- sticky `__ovf` i64-exactness flag set by in-WASM arithmetic
  | wasmTrap       -- WebAssembly.RuntimeError (OOB access, unreachable, …)
  | pyException    -- deliberate Python exception dispatched by `__check_err`
  deriving DecidableEq, Repr

/-- The shipped failure DISPOSITIONS — mirror of the emitted guard/ladder
    code (derived Rust-side from real probe bridges): a fault either re-runs
    on the exact arbitrary-precision twin, throws loudly, or (a deliberate
    Python exception) propagates unchanged. NEVER a silently wrapped value. -/
def faultDispo : FaultEvent → BridgeMode → String
  | .i64ArgOob, .twins => "reroute-twin"
  | .i64ArgOob, .notwins => "throw-range"
  | .listElemI64Oob, .twins => "reroute-twin"   -- RangeError → __isWasmFault ladder → twin
  | .listElemI64Oob, .notwins => "throw-range"  -- no ladder: fail loud, never wrap
  | .ovfFlag, .twins => "reroute-twin"
  | .ovfFlag, .notwins => "throw-overflow"
  | .wasmTrap, .twins => "reroute-twin"
  | .wasmTrap, .notwins => "propagate-trap"
  | .pyException, _ => "propagate-py"           -- NOT a fault: same error as the pure-JS path

def faultEventName : FaultEvent → String
  | .i64ArgOob => "i64-arg-oob"
  | .listElemI64Oob => "list-elem-i64-oob"
  | .ovfFlag => "ovf-flag"
  | .wasmTrap => "wasm-trap"
  | .pyException => "py-exception"

def bridgeModeName : BridgeMode → String
  | .twins => "twins"
  | .notwins => "notwins"

/-- The boundary-type shape alphabet, in table order — must match
    `bridge::marshalling_alphabet()` (same names, same order). Hits every
    conversion arm, every element kind, and every admission-predicate arm. -/
def marshalAlphabet : List (String × WasmTy) :=
  wasmBaseAlpha
    ++ wasmBaseAlpha.map (fun p => ("list<" ++ p.1 ++ ">", WasmTy.list p.2))
    ++ [("list<list<int>>", .list (.list .int)),
        ("set<int>", .set .int),
        ("opt<int>", .opt .int),
        ("dict<str,int>", .dict .str .int),
        ("tuple<int,float>", .tuple .int .float),
        ("callable<int,int>", .callable .int .int),
        ("callable<int,none>", .callable .int .wnone)]

/-- One shape's `arg` + `ret` rows. `-` = no WASM representation (nothing to
    marshal); the bits are the #364 admission verdicts. -/
def marshalRow (name : String) (t : WasmTy) : String :=
  let argE := match toWasmType t with
    | some r => jsToWasmExpr r
    | none => "-"
  let retE := match toWasmType t with
    | some r => wasmToJsExpr r
    | none => "-"
  "arg " ++ name ++ " -> " ++ wasmBit (isNumericKernelParam t) ++ " ; " ++ argE ++ "\n"
    ++ "ret " ++ name ++ " -> " ++ wasmBit (isScalarWasmReturn t) ++ " ; " ++ retE ++ "\n"

def faultEvents : List FaultEvent :=
  [.i64ArgOob, .listElemI64Oob, .ovfFlag, .wasmTrap, .pyException]

/-- Must byte-match `bridge::marshalling_table()`. -/
def marshallingTable : String := Id.run do
  let mut out := ""
  for (n, t) in marshalAlphabet do
    out := out ++ marshalRow n t
  for e in faultEvents do
    for m in [BridgeMode.twins, BridgeMode.notwins] do
      out := out ++ "fault " ++ faultEventName e ++ " " ++ bridgeModeName m
        ++ " -> " ++ faultDispo e m ++ "\n"
  return out

/-! ### The i64 crossing at the VALUE level (the only lossy-prone scalar) -/

/-- The runtime's JS int representation invariant: a Number inside the safe
    range, a BigInt outside it. `wf` is the documented representation
    invariant (ints past 2⁵³ are ALWAYS BigInt) — an explicit trust-boundary
    premise, exercised by the shipped-binding differential. -/
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

/-- Outcome of marshalling one JS int across the boundary. -/
inductive ArgOutcome where
  | pass (w : Int)  -- crossed into WASM as the i64 value w
  | rerouteTwin     -- diverted to the exact JS twin (js+wasm)
  | throwRange      -- loud RangeError (edge targets)
  deriving DecidableEq, Repr

/-- The SHIPPED i64 argument crossing: `__i64Oob` guards BigInts beyond i64
    (Numbers are inside i64 whenever `wf` holds — 2⁵³ < 2⁶³); an in-range
    value crosses EXACTLY (`BigInt(Math.trunc(x))` / identity on BigInt).
    Post-fix, the `__list_to_wasm` i64 ELEMENT path has the same shape: the
    element guard throws RangeError, which the twins-mode fault ladder turns
    into a twin re-run — so this one function models BOTH the scalar and the
    element crossing (rows `i64-arg-oob` and `list-elem-i64-oob`). -/
def i64ArgMarshal (m : BridgeMode) : JsInt → ArgOutcome
  | .num n => .pass n
  | .big n =>
    if n > i64Max ∨ n < i64Min then
      match m with
      | .twins => .rerouteTwin
      | .notwins => .throwRange
    else .pass n

/-- The i64 element crossing is the same guarded discipline (see above). -/
abbrev i64ElemMarshal := i64ArgMarshal

/-- The SHIPPED i64 return crossing (`__i64ToJs`): Number inside the safe
    range, BigInt outside — always the same mathematical integer, and always
    a `wf` representation. -/
def i64RetMarshal (w : Int) : JsInt :=
  if -jsSafeMax ≤ w ∧ w ≤ jsSafeMax then .num w else .big w

/-- **i64 crossing exactness (soundness of the guarded pass).** Under the
    representation invariant, a `pass` is ALWAYS the exact value AND inside
    i64 — the conversion never wraps what it lets through; everything else
    diverts. The guard is load-bearing: see the unguarded stub below. -/
theorem i64ArgMarshal_exact (m : BridgeMode) (v : JsInt) (w : Int)
    (hwf : v.wf = true) (h : i64ArgMarshal m v = .pass w) :
    w = v.val ∧ i64Min ≤ w ∧ w ≤ i64Max := by
  cases v with
  | num n =>
    simp only [i64ArgMarshal, ArgOutcome.pass.injEq] at h
    simp only [JsInt.wf, decide_eq_true_eq, jsSafeMax] at hwf
    subst h
    refine ⟨rfl, ?_, ?_⟩ <;> simp only [JsInt.val, i64Min, i64Max] <;> omega
  | big n =>
    simp only [i64ArgMarshal] at h
    split at h
    · cases m <;> simp at h
    · rename_i hin
      simp only [ArgOutcome.pass.injEq] at h
      subst h
      simp only [not_or] at hin
      exact ⟨rfl, by omega, by omega⟩

/-- The return normalization is value-preserving and re-establishes the
    representation invariant. -/
theorem i64RetMarshal_exact (w : Int) :
    (i64RetMarshal w).val = w ∧ (i64RetMarshal w).wf = true := by
  unfold i64RetMarshal
  split
  · rename_i hin
    refine ⟨rfl, ?_⟩
    simp only [JsInt.wf, decide_eq_true_eq]
    exact hin
  · exact ⟨rfl, rfl⟩

/-- **Boundary round-trip:** a value that crosses in and comes back is the
    SAME integer — the js+wasm i64 boundary is the identity on everything it
    passes (and diverts everything else, never wraps). -/
theorem i64_boundary_roundtrip (m : BridgeMode) (v : JsInt) (w : Int)
    (hwf : v.wf = true) (h : i64ArgMarshal m v = .pass w) :
    (i64RetMarshal w).val = v.val := by
  have hx := i64ArgMarshal_exact m v w hwf h
  rw [(i64RetMarshal_exact w).1, hx.1]

/-- **STUB — the UNGUARDED crossing (raw ES `ToBigInt64`): wraps mod 2⁶⁴.**
    This IS the pre-guard shipped behavior of the `__list_to_wasm` i64
    element path (PoC 2026-08-16: `pick([2**63+7])` crossed as
    −9223372036854775801), and what `__i64Oob` prevents for scalars. -/
def i64ArgMarshal_unguarded (v : JsInt) : ArgOutcome :=
  .pass (wrapI64 v.val)

/-- **The guard is load-bearing.** The unguarded conversion VIOLATES crossing
    exactness at the discriminating witness `2⁶³` (a legal `wf` BigInt): it
    passes `wrapI64 2⁶³ = −2⁶³ ≠ 2⁶³` — the silent-wrap class the table's
    `i64-arg-oob` / `list-elem-i64-oob` dispositions exclude. (The guarded
    marshaller diverts this witness: reroute/throw, never a pass.) -/
theorem i64Marshal_unguardedStub_fails :
    ¬ (∀ (v : JsInt) (w : Int), v.wf = true →
        i64ArgMarshal_unguarded v = .pass w →
        w = v.val ∧ i64Min ≤ w ∧ w ≤ i64Max) := by
  intro h
  have hbad := h (.big (2 ^ 63)) (wrapI64 (2 ^ 63)) rfl rfl
  -- hbad.1 : wrapI64 (2^63) = 2^63 — but wrapI64 (2^63) = -(2^63)
  have : wrapI64 (2 ^ 63) = -(2 ^ 63) := by decide
  rw [this] at hbad
  exact absurd hbad.1 (by decide)

/-- `arr[i] | 0` — the i32 element slot's coercion (ES ToInt32): wraps mod
    2³². Exact ONLY for values that fit i32 (bools: 0/1) — which is why the
    i32 kind is admitted solely for `list[bool]`. -/
def wrap32 (n : Int) : Int := Int.bmod n (2 ^ 32)

/-- The i32-slot element crossing (what a MIS-KINDED int list would do). -/
def i64ElemMarshal_i32KindStub (v : JsInt) : ArgOutcome :=
  .pass (wrap32 v.val)

/-- **STUB — the element KIND is load-bearing.** If `list[int]` elements
    crossed in the i32 slot (a wrong `list_elem_kind`), the discriminating
    witness `2³² + 1` (a legal `wf` Number, well inside i64 — the CORRECT
    i64-kind crossing passes it exactly) would cross as `1`. So the table's
    `list<int> → i64`-kind row is semantically forced, not a labeling. -/
theorem listInt_i32KindStub_fails :
    ¬ (∀ (v : JsInt) (w : Int), v.wf = true →
        i64ElemMarshal_i32KindStub v = .pass w →
        w = v.val ∧ i64Min ≤ w ∧ w ≤ i64Max) := by
  intro h
  have hbad := h (.num (2 ^ 32 + 1)) (wrap32 (2 ^ 32 + 1)) (by decide) rfl
  have : wrap32 (2 ^ 32 + 1) = 1 := by decide
  rw [this] at hbad
  exact absurd hbad.1 (by decide)

/-- Pointwise relational lifting over two lists (matching the
    Batteries/Mathlib `List.Forall₂` definition — core v4.31 does not ship it
    and this project is dependency-free, so it is restated here as supporting
    infrastructure for the element-loop lemma below). `Forall₂ R xs ws` holds
    iff the lists have EQUAL LENGTH and `R` holds at every index. -/
inductive List.Forall₂ (R : α → β → Prop) : List α → List β → Prop
  | nil : Forall₂ R [] []
  | cons {a b as bs} : R a b → Forall₂ R as bs → Forall₂ R (a :: as) (b :: bs)

/-- **Element-loop exactness (∀-over-elements).** Lifting `i64ArgMarshal_exact`
    over the list structure: when a whole `list[int]` crosses the `__list_to_wasm`
    i64 element path ALL-PASS (every element admitted, none diverted), the crossed
    integers are POINTWISE exact — each equals its source element's value and lands
    inside i64. This upgrades the element crossing from the abbrev identity
    `i64ElemMarshal := i64ArgMarshal` (each element merely uses the same guarded
    function) to a claim QUANTIFIED over the whole list.
    Scope (honest, F8): this binds the VALUE crossing per element lifted over the
    list; it is shipping-bound by the SAME `__list_to_wasm` differential as the
    scalar (the `pick([2**63+7])` PoC), NOT a fresh binding. It does NOT model the
    per-element linear-memory WRITE loop — that stays differential. -/
theorem listI64Marshal_exact (m : BridgeMode) (xs : List JsInt) (ws : List Int)
    (hwf : ∀ v ∈ xs, v.wf = true)
    (h : xs.map (i64ElemMarshal m) = ws.map ArgOutcome.pass) :
    List.Forall₂ (fun v w => w = v.val ∧ i64Min ≤ w ∧ w ≤ i64Max) xs ws := by
  induction xs generalizing ws with
  | nil =>
    cases ws with
    | nil => exact List.Forall₂.nil
    | cons w ws' => simp at h
  | cons a as ih =>
    cases ws with
    | nil => simp at h
    | cons w ws' =>
      simp only [List.map_cons, List.cons.injEq] at h
      exact List.Forall₂.cons
        (i64ArgMarshal_exact m a w (hwf a (.head _)) h.1)
        (ih ws' (fun v hv => hwf v (.tail _ hv)) h.2)

/-- **The element guard is load-bearing over the whole list.** The unguarded
    element crossing (raw `ToBigInt64` per element) violates element-loop exactness
    at the singleton `[2⁶³]`: it passes `wrapI64 2⁶³ = −2⁶³`, so `w = v.val` fails. -/
theorem listI64Marshal_unguardedStub_fails :
    ¬ (∀ (xs : List JsInt) (ws : List Int),
        (∀ v ∈ xs, v.wf = true) →
        xs.map i64ArgMarshal_unguarded = ws.map ArgOutcome.pass →
        List.Forall₂ (fun v w => w = v.val ∧ i64Min ≤ w ∧ w ≤ i64Max) xs ws) := by
  intro h
  have hbad := h [JsInt.big (2 ^ 63)] [wrapI64 (2 ^ 63)]
      (by intro v hv; cases hv with
          | head => rfl
          | tail _ hv' => cases hv')
      rfl
  cases hbad with
  | cons hhead _ =>
    have hwrap : wrapI64 (2 ^ 63) = -(2 ^ 63) := by decide
    rw [hwrap] at hhead
    exact absurd hhead.1 (by decide)

/-- Non-vacuity: a concrete NON-EMPTY all-pass list the hypothesis of
    `listI64Marshal_exact` actually admits (so it is not vacuously true), and the
    conclusion holds for it. -/
example : [JsInt.num 5, JsInt.num (-3)].map (i64ElemMarshal BridgeMode.twins)
        = [(5 : Int), -3].map ArgOutcome.pass := by decide

example : List.Forall₂ (fun v w => w = JsInt.val v ∧ i64Min ≤ w ∧ w ≤ i64Max)
          [JsInt.num 5, JsInt.num (-3)] [(5 : Int), -3] := by
  refine List.Forall₂.cons ⟨rfl, by decide, by decide⟩ ?_
  exact List.Forall₂.cons ⟨rfl, by decide, by decide⟩ List.Forall₂.nil

/-- The bool crossing round-trips: `x ? 1 : 0` in, `Boolean(x)` out. -/
def boolArgMarshal (b : Bool) : Int := if b then 1 else 0
def boolRetMarshal (w : Int) : Bool := w != 0

theorem bool_boundary_roundtrip (b : Bool) :
    boolRetMarshal (boolArgMarshal b) = b := by
  cases b <;> rfl

/-! ### The headline theorems: admitted surface ⊆ value-exact classes -/

/-- The js→wasm converter classes that are VALUE-EXACT under the shipped
    guards: the guarded i64 scalar (`i64ArgMarshal_exact`), f64 identity on
    Numbers (a BigInt int coerces via `__f64Arg` by the standard int→float
    conversion — the value the float position denotes — and raises
    OverflowError beyond double range, #465, never a silent Infinity),
    bool→i32 (`bool_boundary_roundtrip`), and lists whose ELEMENT slot
    matches the element type (i64-guarded / f64 / bool-in-i32). NOT exact:
    every pointer marshalling, and any list whose elements ride a mismatched
    slot (`listInt_i32KindStub_fails`; a str/ptr element in an i32 slot is
    the same wrap class). -/
def argValueExact : WasmRepr → Bool
  | .i64 | .f64 | .i32 => true
  | .ptrList .i64 | .ptrList .f64 | .ptrList .i32 => true
  | _ => false

/-- The wasm→js return classes that are value-exact: `__i64ToJs`
    (`i64RetMarshal_exact`), f64 identity, `Boolean` on an i32 bool. -/
def retValueExact : WasmRepr → Bool
  | .i64 | .f64 | .i32 => true
  | _ => false

/-- **Marshalling soundness (params).** Every boundary type the #364
    admission accepts as a parameter lowers to a representation whose
    js→wasm conversion is in the value-exact class — the admitted JS↔WASM
    boundary NEVER marshals through a pointer converter or a mismatched
    element slot. (The finite witness is every `arg … -> 1` row of the
    committed table: `admitted_arg_rows_use_exact_marshallers`.) -/
theorem marshal_param_admitted_sound (t : WasmTy) :
    isNumericKernelParam t = true →
    ∃ r, toWasmType t = some r ∧ argValueExact r = true := by
  intro h
  cases t with
  | int => exact ⟨.i64, rfl, rfl⟩
  | float => exact ⟨.f64, rfl, rfl⟩
  | bool => exact ⟨.i32, rfl, rfl⟩
  | list i =>
    cases i with
    | int => exact ⟨.ptrList .i64, rfl, rfl⟩
    | float => exact ⟨.ptrList .f64, rfl, rfl⟩
    | bool => exact ⟨.ptrList .i32, rfl, rfl⟩
    | _ => simp [isNumericKernelParam] at h
  | _ => simp [isNumericKernelParam] at h

/-- **Marshalling soundness (returns).** Every #364-admitted return type is
    either a genuine no-value return (`none`/`void` — nothing crosses) or
    lowers to a value-exact scalar normalization; an admitted return NEVER
    marshals through a pointer converter. -/
theorem marshal_ret_admitted_sound (t : WasmTy) :
    isScalarWasmReturn t = true →
    (toWasmType t = none ∧ (t = .wnone ∨ t = .void)) ∨
      ∃ r, toWasmType t = some r ∧ retValueExact r = true := by
  intro h
  cases t with
  | int => exact Or.inr ⟨.i64, rfl, rfl⟩
  | float => exact Or.inr ⟨.f64, rfl, rfl⟩
  | bool => exact Or.inr ⟨.i32, rfl, rfl⟩
  | wnone => exact Or.inl ⟨rfl, Or.inl rfl⟩
  | void => exact Or.inl ⟨rfl, Or.inr rfl⟩
  | _ => simp [isScalarWasmReturn] at h

/-! ### Exhaustiveness — the finite table covers the whole representable
    domain -/

/-- The arg-conversion expressions the table witnesses. -/
def tableArgExprs : List String :=
  marshalAlphabet.filterMap (fun p => (toWasmType p.2).map jsToWasmExpr)

/-- The ret-conversion expressions the table witnesses. -/
def tableRetExprs : List String :=
  marshalAlphabet.filterMap (fun p => (toWasmType p.2).map wasmToJsExpr)

/-- **Exhaustiveness.** EVERY representable boundary type (the whole infinite
    `WasmRepr` domain, any nesting) marshals by an expression the finite
    table already witnesses: the conversion depends only on the head
    constructor + element kind, and the alphabet hits them all. So checking
    the 56 committed rows checks the entire boundary. -/
theorem marshal_exhaustive (r : WasmRepr) :
    jsToWasmExpr r ∈ tableArgExprs ∧ wasmToJsExpr r ∈ tableRetExprs := by
  cases r
  case ptrList i =>
    cases i <;> refine ⟨?_, ?_⟩ <;>
      simp only [jsToWasmExpr, wasmToJsExpr, listElemKind] <;> decide
  all_goals refine ⟨?_, ?_⟩ <;>
    simp only [jsToWasmExpr, wasmToJsExpr] <;> decide

/-! ### Non-vacuity + anti-forgery pins -/

-- The table is non-empty, has exactly the committed shape (46 conversion
-- rows + 10 fault rows), and every disposition class is witnessed.
#guard (marshallingTable.splitOn "\n").length = 57  -- 56 rows + trailing newline
#guard (marshallingTable.splitOn "arg ").length = 24    -- 23 arg rows
#guard (marshallingTable.splitOn "ret ").length = 24    -- 23 ret rows
#guard (marshallingTable.splitOn "fault ").length = 11  -- 10 fault rows
#guard (marshallingTable.splitOn "reroute-twin").length = 5   -- 4 twin reroutes
#guard (marshallingTable.splitOn "throw-range").length = 3
#guard (marshallingTable.splitOn "throw-overflow").length = 2
#guard (marshallingTable.splitOn "propagate-trap").length = 2
#guard (marshallingTable.splitOn "propagate-py").length = 3
#guard (marshallingTable.splitOn "-> 1 ;").length = 12  -- 11 admitted rows (6 arg + 5 ret)
#guard (marshallingTable.splitOn " ; -").length = 17    -- 16 no-marshal rows (no repr / void ret)

-- Anti-forgery: the byte-equality check REJECTS a single drifted disposition
-- row (the checker gates — a forged silent-wrap row cannot pass).
#guard marshallingTable
    ≠ marshallingTable.replace "fault list-elem-i64-oob twins -> reroute-twin"
        "fault list-elem-i64-oob twins -> silent-wrap"
#guard (marshallingTable.splitOn "silent-wrap").length = 1  -- and none is present

-- Executable pins of the value model at the discriminating witnesses.
#guard i64ArgMarshal .twins (.big (2 ^ 63)) = .rerouteTwin          -- oob → twin
#guard i64ArgMarshal .notwins (.big (2 ^ 63)) = .throwRange         -- oob → loud
#guard i64ArgMarshal .twins (.big (2 ^ 63 - 1)) = .pass (2 ^ 63 - 1) -- max passes exactly
#guard i64ArgMarshal .twins (.num 5) = .pass 5
#guard i64ArgMarshal_unguarded (.big (2 ^ 63)) = .pass (-(2 ^ 63))  -- the PoC wrap (pre-fix)
#guard i64RetMarshal (2 ^ 53) = .big (2 ^ 53)                       -- past safe: stays BigInt
#guard i64RetMarshal (2 ^ 53 - 1) = .num (2 ^ 53 - 1)               -- safe: Number
#guard wrap32 (2 ^ 32 + 1) = 1                                      -- the i32-slot wrap class

/-- SPOT (through `marshal_param_admitted_sound`, not a `#guard`): the
    admitted `list[int]` boundary lowers to the i64-slot list class — fails
    if the theorem is weakened to not pin the representation class. -/
example : ∃ r, toWasmType (.list .int) = some r ∧ argValueExact r = true :=
  marshal_param_admitted_sound _ rfl

/-- SPOT (through `i64ArgMarshal_exact`): a pass at the i64 edge is pinned to
    EXACTLY the value — fails if exactness is weakened to range-only. -/
example (w : Int) (h : i64ArgMarshal .twins (.big (2 ^ 63 - 1)) = .pass w) :
    w = 2 ^ 63 - 1 :=
  (i64ArgMarshal_exact .twins _ w rfl h).1

/-- SPOT (through `i64_boundary_roundtrip`): what crosses and returns is the
    same integer. -/
example (w : Int) (h : i64ArgMarshal .twins (.num 41) = .pass w) :
    (i64RetMarshal w).val = 41 :=
  i64_boundary_roundtrip .twins (.num 41) w rfl h

/-- info: 'PythExpandVerify.marshal_param_admitted_sound' depends on axioms: [propext] -/
#guard_msgs in
#print axioms marshal_param_admitted_sound

/-- info: 'PythExpandVerify.marshal_ret_admitted_sound' depends on axioms: [propext] -/
#guard_msgs in
#print axioms marshal_ret_admitted_sound

/-- info: 'PythExpandVerify.i64ArgMarshal_exact' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms i64ArgMarshal_exact

/-- info: 'PythExpandVerify.i64RetMarshal_exact' depends on axioms: [propext] -/
#guard_msgs in
#print axioms i64RetMarshal_exact

/-- info: 'PythExpandVerify.i64_boundary_roundtrip' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms i64_boundary_roundtrip

/-- info: 'PythExpandVerify.i64Marshal_unguardedStub_fails' does not depend on any axioms -/
#guard_msgs in
#print axioms i64Marshal_unguardedStub_fails

/-- info: 'PythExpandVerify.listInt_i32KindStub_fails' does not depend on any axioms -/
#guard_msgs in
#print axioms listInt_i32KindStub_fails

/-- info: 'PythExpandVerify.listI64Marshal_exact' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms listI64Marshal_exact

/-- info: 'PythExpandVerify.listI64Marshal_unguardedStub_fails' does not depend on any axioms -/
#guard_msgs in
#print axioms listI64Marshal_unguardedStub_fails

/-- info: 'PythExpandVerify.bool_boundary_roundtrip' does not depend on any axioms -/
#guard_msgs in
#print axioms bool_boundary_roundtrip

/-- info: 'PythExpandVerify.marshal_exhaustive' depends on axioms: [propext] -/
#guard_msgs in
#print axioms marshal_exhaustive

end PythExpandVerify
