/-! # C2TypeRepr — full type-representation preservation (C2), post-#462 minimal Option B

**The claim.** The compiler preserves Python type-REPRESENTATION across the
lowering: a Python `int` lowers to a target value that reprs as an int, and a
Python `float` lowers to one that reprs as a float — **including the hard case
that motivated #451/#462: an integer-valued float (`8.0`) inside a container
still reprs as `8.0`, never `8`** — for scalars AND container elements
(list / tuple / dict), the untyped channel where the pre-fix model leaked.

**What shipped (the target model's referent).** MINIMAL OPTION B (#462),
merged to dev-main `6e990502` ("int/float fidelity via minimal Option B —
box only integer-valued floats"). The value model, `runtime/src/operators.js`
@ `6e990502`:
  * **int is UNTOUCHED** — the hybrid Number/bigint carrier: small ints
    (|n| ≤ `Number.MAX_SAFE_INTEGER`) are plain JS `number`
    (`ExprKind::IntLiteral` / `__norm()`, L456–457), large ints are `bigint`;
  * **a non-integer float stays a native `number`** — already
    `Number.isInteger`-distinguishable from every int;
  * **ONLY the ambiguous class is boxed**: an integer-valued float becomes
    `new PyFloat(v)` — `class PyFloat extends Number` with the `__pyfloat__`
    brand (L470–478); THE single box authority is
    `__pyF = (v) => (Number.isInteger(v) ? new PyFloat(v) : v)` (L481);
    `__pyJs` (L485) unboxes at native-JS sinks (C8's business, not C2's).
  * dispatch, `typeName`/`__opTypeName` (L13–27): `__pyfloat__ === true →
    "float"` FIRST, then `typeof`: `"number" → Number.isInteger ? "int" :
    "float"`, `"bigint" → "int"`;
  * `pyRepr`: the `__pyfloat__` branch (L1150) → `pyFormatFloat(valueOf)`
    (float display, whole values get `".0"`); the `number` branch
    (L1199–1202) → `Number.isInteger && |obj| ≤ MAX_SAFE → String(obj)` (int
    display), else `pyFormatFloat`; `bigint` → `String(obj)`; containers repr
    by mapping `pyRepr` over elements, so each element's own carrier travels.
The model's two knobs (`NumModel` = how int / integer-valued-float land in
JS; `ReprModel` = the repr native-integer-number arm) are exactly the
falsifiable parameters: B changed ONLY the value model (`lowFloat` boxes);
the repr `number` arm is byte-identical pre/post-fix. That the fix lives
entirely in the CARRIER is machine-checked below
(`preFixCarrier_cannot_repr` + `preservationC2` sharing the SAME `ReprModel`).

**The false world this file refutes (the discriminating witness).** The
pre-fix shipped model — this very branch's `runtime/src/operators.js` (no
`PyFloat`, no `__pyF`): integer-valued floats land as plain `number`,
carrier-identical to demoted small ints, and `pyRepr`'s number branch
(`Number.isInteger` heuristic — "a documented, accepted residual gap of the
A4 fix") gives them the int display. In that world an integer-valued float
in a container collapses: `[8.0]` → `[8]` — the exact shipped bug. This file
(a) refutes `C2Preserves` for that world at the exact witness `[8.0]`, and
(b) proves the information-loss fact: NO repr function on the pre-fix
carrier can be C2-faithful, because the pre-fix lowering CONFLATES `8` and
`8.0` before repr ever runs (`preFixCarrier_cannot_repr`) — the
machine-checked reason the fix had to change the VALUE MODEL (box the
ambiguous class) rather than patch `pyRepr`. Option B is the MINIMAL such
change: it boxes exactly the conflated class and nothing else
(`optionB_boxes_only_floats`, `lowerB_numI_inv`).

**Honesty tier — what is bound vs model-level.**
  * MODEL TIER, transliteration-bound: the target lowering + repr + tag
    dispatch are transliterations of the SHIPPED post-fix code,
    provenance-pinned to `6e990502` (files/lines above); the broken world is
    a transliteration of the pre-fix code literally in THIS branch's
    `runtime/src/`.
  * EMPIRICALLY EXECUTED on both real runtimes (node, 2026-08-21, recorded
    here):
      pre-fix  (this branch):  pyRepr([8.0]) = "[8]",  pyRepr(8.0) = "8",
                               pyRepr(Map{1:8.0}) = "{1: 8}";
      post-fix (`6e990502`):   pyRepr([__pyF(8)]) = "[8.0]",
                               pyRepr([8]) = "[8]",  pyRepr(8) = "8",
                               pyRepr(__pyF(8)) = "8.0",
                               pyRepr(9007199254740993n) = "9007199254740993",
                               pyRepr(Map{1:__pyF(8)}) = "{1: 8.0}",
                               pyRepr((__pyF(8),)) = "(8.0,)",
                               pyRepr([1, __pyF(2), 'a', true]) = "[1, 2.0, 'a', True]",
                               typeof __pyF(8) = "object" (boxed),
                               __pyF(3.14) NOT boxed (native number).
    CPython 3.12.7 pins for the reference (`reprRef`) are the `#guard`s
    below, each checked against a live `repr(...)` run (2026-08-21).
  * NOT independently shipping-bound FROM THIS FILE: the wave-14/15-style
    differential (real post-fix `pyths` vs CPython over a corpus) lives with
    the #462 fix branch itself — `behavioral_differential` green + the
    optB float-repr/int-repr suites at `6e990502`. The post-fix binary is
    NOT on this proof branch's ancestry, so this file states the
    correspondence and pins it by provenance + the recorded runs; re-running
    the differential against this model is the reviewer's cross-check, not a
    claim made here.

**Fragment scope (honest domain).**
  * Integer-valued floats are `pfloat (n : Int)` — the `Number.isInteger`
    hazard class, THE discriminating channel. Their display law
    `toString n ++ ".0"` models CPython's fixed-notation whole-float repr,
    i.e. |value| < 10^16; the scientific-notation switchover (`1e+16`,
    `1e+308`), `-0.0`, `inf`, `nan` are owned by `pyFormatFloat` — unchanged
    by the fix, out of model.
  * Non-integer floats are `pfloatX (d : String)` where `d` is CPython's
    shortest-repr digit string (e.g. "3.14"): they stay native `number`s
    under EVERY world (`__pyF` boxes iff integer-valued) and display via
    `pyFormatFloat`, which reproduces CPython's shortest-repr digits (V8
    `toExponential` — same dtoa class; empirically pinned, not proved).
    Modeling the digit string as data makes the `number + !isInteger →
    float` arm of the shipped classification explicit while keeping the
    formatter itself outside the trusted surface. This channel is
    fix-invariant — identical in all three worlds — and participates in no
    refutation.
  * **The #462 residual — injected integer-valued floats — is a stated
    domain boundary, not a hidden gap.** The model's domain is
    PYTHS-ORIGINATED values: everything `__pyF` had the chance to box. An
    integer-valued float INJECTED from the host as a raw native `number`
    (FFI/JSON/DOM, never through `__pyF`) is carrier-identical to a small
    int; the shipped dispatch necessarily reports int. In the model that
    value is `.numI 8` — which is NOT in the image of the lowering for any
    float (`lowerB_numI_inv`: a lowered `.numI` can only come from `pint`);
    `injected_float_residual` states what the dispatch does to it. The C2
    claim is exactly: on the lowering's image the representation is
    preserved; the injected channel is outside that image (#462 scope).
  * Strings are modeled escape-free (`repr = 's'`); CPython string-repr
    escaping (#329) is a separate obligation.
  * `pdict` keys range over all `PyVal` — wider than Python's hashable
    domain. Harmless for a repr-preservation EQUATION: it also holds at the
    unreachable points and no theorem below is load-bearing on them.
  * `set` / `bytes` / `None` are out of fragment (bytes carry their own
    #451 follow-ups #455–#458).

**Relation to the previous (Option-A-based) revision of this file.** The
prior revision transliterated Option A (`c63d0dd7`: int = bigint
UNCONDITIONALLY, float = number, repr dispatch purely on `typeof`). Option A
was REVERTED for host-interop breakage (the `slice(0n)` class + the `.d.ts`
type-lie) and replaced by minimal B at `6e990502` — A is now the C8 axis's
discriminating witness (`C8HostInterop.lean`), where its C2-side fidelity is
still PROVED (`preservationC2_optionA` there) so that "A satisfied the
source-fidelity axes and violated host interop" is a theorem, not prose.
The `NumModel`/`ReprModel` knobs here are shared with that file.

**Relation to the frozen lattice-v2 C2 (PythExpandVerify) and JsWasmMirror.**
Unchanged from the prior revision: the frozen fragment's `d1CollapseAL`
theorems remain true as statements about that model; `JsWasmMirror`'s
`i64RetMarshal` follow-up stands (post-B the safe-range demotion at the WASM
return crossing again matches the shipped hybrid int — the Option-A-era
staleness note no longer applies, but the `.num`-arm marshalling row review
is still open upstream). Flagged for review, deliberately not edited here. -/

namespace C2TypeRepr

/-! ## Source side — Python values of the fragment + CPython-faithful repr -/

/-- Python values of the fragment. `pfloat n` is the integer-VALUED float
    `n.0` (the `Number.isInteger` hazard class); `pfloatX d` is a
    non-integer float carrying its CPython shortest-repr digit string —
    see the header's fragment-scope notes. -/
inductive PyVal where
  | pint    (n : Int)
  | pfloat  (n : Int)
  | pfloatX (d : String)
  | pstr    (s : String)
  | pbool   (b : Bool)
  | plist   (xs : List PyVal)
  | ptuple  (xs : List PyVal)
  | pdict   (kvs : List (PyVal × PyVal))

/-- Python type tags of the fragment (the C2 tag axis). -/
inductive PTag where
  | tint | tfloat | tstr | tbool | tlist | ttuple | tdict
  deriving DecidableEq, Repr

/-- The Python type of a value. -/
def pyTag : PyVal → PTag
  | .pint _ => .tint | .pfloat _ => .tfloat | .pfloatX _ => .tfloat
  | .pstr _ => .tstr | .pbool _ => .tbool | .plist _ => .tlist
  | .ptuple _ => .ttuple | .pdict _ => .tdict

mutual
/-- CPython `repr` on the fragment (the REFERENCE): ints bare, whole floats
    with `.0`, non-integer floats by their shortest-repr digits,
    `True`/`False`, `'s'`, `[..]` / `(..)` (singleton trailing comma) /
    `{k: v}` with recursive element repr. Faithfulness is pinned by the
    CPython 3.12 `#guard`s below (F9 discipline). -/
def reprRef : PyVal → String
  | .pint n    => toString n
  | .pfloat n  => toString n ++ ".0"
  | .pfloatX d => d
  | .pstr s    => "'" ++ s ++ "'"
  | .pbool b   => if b then "True" else "False"
  | .plist xs  => "[" ++ reprRefList xs ++ "]"
  | .ptuple xs => "(" ++ reprRefList xs ++ (if xs.length == 1 then ",)" else ")")
  | .pdict kvs => "{" ++ reprRefPairs kvs ++ "}"

/-- Comma-joined element reprs (head). -/
def reprRefList : List PyVal → String
  | [] => ""
  | v :: vs => reprRef v ++ reprRefListTail vs

/-- Comma-joined element reprs (tail — each element prefixed by `", "`). -/
def reprRefListTail : List PyVal → String
  | [] => ""
  | v :: vs => ", " ++ reprRef v ++ reprRefListTail vs

/-- Comma-joined `k: v` pair reprs (dict body, insertion order; head). -/
def reprRefPairs : List (PyVal × PyVal) → String
  | [] => ""
  | (k, v) :: ps => reprRef k ++ ": " ++ reprRef v ++ reprRefPairsTail ps

/-- Dict body, tail. -/
def reprRefPairsTail : List (PyVal × PyVal) → String
  | [] => ""
  | (k, v) :: ps => ", " ++ reprRef k ++ ": " ++ reprRef v ++ reprRefPairsTail ps
end

/-! ## Target side — the JS value carrier under minimal Option B

The carriers are runtime-tag-level:
  * `numI n`  ↔ a plain JS `number` that is integer-valued (`typeof
    "number"`, `Number.isInteger` true). Safe-range by construction of the
    shipped lowering (`__norm` demotes only within ±MAX_SAFE; the repr
    number-branch's `|obj| ≤ MAX_SAFE` conjunct is therefore satisfied
    whenever a `numI` is pyths-originated).
  * `big n`   ↔ `typeof "bigint"` (large int).
  * `boxF n`  ↔ a `PyFloat` box (`__pyfloat__` brand; `typeof "object"`) —
    the integer-valued float, THE carrier Option B added.
  * `numX d`  ↔ a plain non-integer `number` carrying its shortest-repr
    digits (fix-invariant channel).
  * `arr`/`tup`/`map` the Array / `__pytuple__`-marked Array / `Map`.
The int/float distinction for the hazard class is a CARRIER brand
(`__pyfloat__`), not a `Number.isInteger` heuristic on a shared carrier —
that is minimal Option B's whole content. -/

inductive JsVal where
  | numI (n : Int)              -- typeof "number", integer-valued (safe range)
  | big  (n : Int)              -- typeof "bigint"
  | boxF (n : Int)              -- PyFloat box (__pyfloat__, typeof "object")
  | numX (d : String)           -- typeof "number", NOT integer-valued
  | str  (s : String)
  | bool (b : Bool)
  | arr  (xs : List JsVal)
  | tup  (xs : List JsVal)
  | map  (kvs : List (JsVal × JsVal))

/-- `Number.MAX_SAFE_INTEGER` — the shipped small-int/bigint split
    (`__norm`, operators.js L456–457). -/
def maxSafe : Nat := 2 ^ 53 - 1

#guard maxSafe = 9007199254740991

/-! ## The two knobs (the falsifiable parameters, F10) -/

/-- HOW the two hazard-class number types land in JS — the VALUE-MODEL knob
    (`lowInt` = the int carrier; `lowFloat` = the integer-valued-float
    carrier). Non-integer floats are not a knob: native `number` in every
    world (`__pyF` boxes iff integer-valued). -/
structure NumModel where
  lowInt   : Int → JsVal
  lowFloat : Int → JsVal

/-- The SHIPPED hybrid int carrier — small → plain number, large → bigint
    (`__norm` / `ExprKind::IntLiteral`). UNCHANGED by the fix; shared by the
    pre-fix world and Option B. -/
def hybridInt (n : Int) : JsVal :=
  if n.natAbs ≤ maxSafe then .numI n else .big n

/-- SHIPPED post-#462 (minimal Option B): int untouched (hybrid), an
    integer-valued float is BOXED (`__pyF` → `new PyFloat(v)`,
    merged `6e990502`). -/
def optionB : NumModel := ⟨hybridInt, .boxF⟩

/-- PRE-FIX: int identical (hybrid — the fix did not touch ints); an
    integer-valued float lands as a plain `number`, carrier-identical to a
    demoted small int. The conflation lives in exactly this channel. -/
def preFix : NumModel := ⟨hybridInt, .numI⟩

/-- HOW `pyRepr` renders a plain integer-valued `number` — the REPR knob.
    (`bigint` always renders `String(v)`; a `boxF` always renders via
    `pyFormatFloat`; `numX` always renders its digits — all
    knob-independent, both worlds.) -/
structure ReprModel where
  numArm : Int → String

/-- SHIPPED (pre-fix AND post-fix — byte-identical branch, operators.js
    L1199–1202): `Number.isInteger(obj) && |obj| ≤ MAX_SAFE → String(obj)` —
    the INT display. Minimal B deliberately KEPT this branch; the fix is
    that a pyths-originated float never reaches it (it is boxed). -/
def intDisplay : ReprModel := ⟨fun n => toString n⟩

/-- Option A's repr arm (operators.js @ `c63d0dd7`): EVERY plain number is a
    float — whole values get `".0"`. Not shipped anymore; used by
    `C8HostInterop.lean` to express the reverted world. -/
def floatDisplay : ReprModel := ⟨fun n => toString n ++ ".0"⟩

/-! ## The lowering and the target repr, parameterized by the knobs -/

mutual
/-- The compiler's value lowering under number-model `M`; containers are
    structural (Array / marked Array / Map, element order kept). -/
def lowerM (M : NumModel) : PyVal → JsVal
  | .pint n    => M.lowInt n
  | .pfloat n  => M.lowFloat n
  | .pfloatX d => .numX d
  | .pstr s    => .str s
  | .pbool b   => .bool b
  | .plist xs  => .arr (lowerMList M xs)
  | .ptuple xs => .tup (lowerMList M xs)
  | .pdict kvs => .map (lowerMPairs M kvs)

def lowerMList (M : NumModel) : List PyVal → List JsVal
  | [] => []
  | v :: vs => lowerM M v :: lowerMList M vs

def lowerMPairs (M : NumModel) : List (PyVal × PyVal) → List (JsVal × JsVal)
  | [] => []
  | (k, v) :: ps => (lowerM M k, lowerM M v) :: lowerMPairs M ps
end

mutual
/-- The runtime's `pyRepr` under repr-model `R`: dispatch on the CARRIER
    (the `__pyfloat__` brand first — L1150 — then `typeof`); containers map
    the element repr in order (`obj.map(pyRepr)`, `Map.entries()`), exactly
    the shipped container branches. -/
def jsReprM (R : ReprModel) : JsVal → String
  | .numI n  => R.numArm n            -- THE knob (the shipped isInteger arm)
  | .big n   => toString n            -- String(bigint)
  | .boxF n  => toString n ++ ".0"    -- __pyfloat__ → pyFormatFloat(valueOf)
  | .numX d  => d                     -- !isInteger → pyFormatFloat
  | .str s   => "'" ++ s ++ "'"
  | .bool b  => if b then "True" else "False"
  | .arr xs  => "[" ++ jsReprMList R xs ++ "]"
  | .tup xs  => "(" ++ jsReprMList R xs ++ (if xs.length == 1 then ",)" else ")")
  | .map kvs => "{" ++ jsReprMPairs R kvs ++ "}"

def jsReprMList (R : ReprModel) : List JsVal → String
  | [] => ""
  | w :: ws => jsReprM R w ++ jsReprMListTail R ws

def jsReprMListTail (R : ReprModel) : List JsVal → String
  | [] => ""
  | w :: ws => ", " ++ jsReprM R w ++ jsReprMListTail R ws

def jsReprMPairs (R : ReprModel) : List (JsVal × JsVal) → String
  | [] => ""
  | (k, v) :: ps => jsReprM R k ++ ": " ++ jsReprM R v ++ jsReprMPairsTail R ps

def jsReprMPairsTail (R : ReprModel) : List (JsVal × JsVal) → String
  | [] => ""
  | (k, v) :: ps => ", " ++ jsReprM R k ++ ": " ++ jsReprM R v ++ jsReprMPairsTail R ps
end

/-- The SHIPPED world (post-#462): minimal Option-B lowering. -/
abbrev lowerB : PyVal → JsVal := lowerM optionB
/-- The SHIPPED world: the shipped repr (int display on plain whole numbers
    — the SAME arm the pre-fix world had; the fix is the carrier). -/
abbrev jsReprB : JsVal → String := jsReprM intDisplay

/-- The PRE-FIX world: unboxed-float lowering. -/
abbrev lowerPre : PyVal → JsVal := lowerM preFix
/-- The PRE-FIX world's repr — the SAME `intDisplay` knob as shipped B.
    The two worlds differ ONLY in the value model; that the repr knob alone
    cannot repair the pre-fix world is `preFixCarrier_cannot_repr`. -/
abbrev jsReprPre : JsVal → String := jsReprM intDisplay

/-! ## The C2 predicate (ONE predicate; all worlds instantiate it) -/

/-- **C2 full type-representation preservation**: the lowered value reprs
    EXACTLY as CPython reprs the source value — scalars and every container
    element. The predicate ranges over BOTH knobs, so a wrong value model or
    a wrong repr dispatch each falsify it (F10: the falsifiable parameter
    reaches the claimed operations). -/
def C2Preserves (lower : PyVal → JsVal) (reprT : JsVal → String) : Prop :=
  ∀ v : PyVal, reprT (lower v) = reprRef v

/-- The tag projection of C2: the target's `typeName` dispatch reports the
    Python type of the source value. -/
def C2PreservesTag (lower : PyVal → JsVal) (tagT : JsVal → PTag) : Prop :=
  ∀ v : PyVal, tagT (lower v) = pyTag v

/-! ## Plumbing lemmas -/

/-- The lowering preserves list length (tuples keep their singleton-comma). -/
theorem lowerMList_length (M : NumModel) (xs : List PyVal) :
    (lowerMList M xs).length = xs.length := by
  induction xs with
  | nil => rfl
  | cons v vs ih => simp only [lowerMList, List.length_cons, ih]

/-- Both arms of the hybrid int carrier repr as the bare int under the
    shipped repr (small → `String(number)`, large → `String(bigint)`). -/
theorem jsRepr_hybridInt (n : Int) :
    jsReprM intDisplay (hybridInt n) = toString n := by
  unfold hybridInt
  split <;> rfl

/-! ## THE THEOREM — shipped C2 preservation (scalars + container elements) -/

mutual
/-- **`preservationC2`, pointwise**: under the SHIPPED minimal-B lowering
    and the SHIPPED repr, every fragment value — scalar or arbitrarily
    nested container — reprs exactly as CPython reprs the source. The
    `[8.0]`-in-a-container case is the `plist`/`ptuple`/`pdict` arms with a
    `pfloat` element (lowered to a `boxF`, whose brand travels with it). -/
theorem preservationC2_val (v : PyVal) :
    jsReprM intDisplay (lowerM optionB v) = reprRef v := by
  cases v with
  | pint n => exact jsRepr_hybridInt n
  | pfloat n => rfl
  | pfloatX d => rfl
  | pstr s => rfl
  | pbool b => rfl
  | plist xs =>
    simp only [lowerM, jsReprM, reprRef]
    rw [preservationC2_list xs]
  | ptuple xs =>
    simp only [lowerM, jsReprM, reprRef]
    rw [preservationC2_list xs, lowerMList_length]
  | pdict kvs =>
    simp only [lowerM, jsReprM, reprRef]
    rw [preservationC2_pairs kvs]

theorem preservationC2_list (xs : List PyVal) :
    jsReprMList intDisplay (lowerMList optionB xs) = reprRefList xs := by
  cases xs with
  | nil => rfl
  | cons v vs =>
    simp only [lowerMList, jsReprMList, reprRefList]
    rw [preservationC2_val v, preservationC2_listTail vs]

theorem preservationC2_listTail (xs : List PyVal) :
    jsReprMListTail intDisplay (lowerMList optionB xs) = reprRefListTail xs := by
  cases xs with
  | nil => rfl
  | cons v vs =>
    simp only [lowerMList, jsReprMListTail, reprRefListTail]
    rw [preservationC2_val v, preservationC2_listTail vs]

theorem preservationC2_pairs (kvs : List (PyVal × PyVal)) :
    jsReprMPairs intDisplay (lowerMPairs optionB kvs) = reprRefPairs kvs := by
  cases kvs with
  | nil => rfl
  | cons kv ps =>
    obtain ⟨k, v⟩ := kv
    simp only [lowerMPairs, jsReprMPairs, reprRefPairs]
    rw [preservationC2_val k, preservationC2_val v, preservationC2_pairsTail ps]

theorem preservationC2_pairsTail (kvs : List (PyVal × PyVal)) :
    jsReprMPairsTail intDisplay (lowerMPairs optionB kvs) = reprRefPairsTail kvs := by
  cases kvs with
  | nil => rfl
  | cons kv ps =>
    obtain ⟨k, v⟩ := kv
    simp only [lowerMPairs, jsReprMPairsTail, reprRefPairsTail]
    rw [preservationC2_val k, preservationC2_val v, preservationC2_pairsTail ps]
end

/-- **THE HEADLINE — C2 full preservation for the shipped world (minimal
    Option B, `6e990502`).** -/
theorem preservationC2 : C2Preserves lowerB jsReprB :=
  fun v => preservationC2_val v

/-! ## The tag projection (shipped `typeName`/`__opTypeName` dispatch) -/

/-- Shipped post-#462 dispatch (operators.js `6e990502` L13–27):
    `__pyfloat__ → "float"` FIRST, then `typeof`: number →
    `isInteger ? "int" : "float"`, bigint → `"int"`, containers by
    instance. NOTE: this is byte-identical in effect to the pre-fix
    dispatch on the pre-fix carrier — the `boxF` arm is simply unreachable
    there. ONE function, two worlds; the carrier is the fix. -/
def jsTagShipped : JsVal → PTag
  | .numI _ => .tint                  -- isInteger → int
  | .big _  => .tint
  | .boxF _ => .tfloat                -- __pyfloat__ brand
  | .numX _ => .tfloat                -- !isInteger → float
  | .str _  => .tstr | .bool _ => .tbool
  | .arr _  => .tlist | .tup _ => .ttuple | .map _ => .tdict

/-- Both arms of the hybrid int carrier report `int` under the shipped
    dispatch. -/
theorem jsTag_hybridInt (n : Int) : jsTagShipped (hybridInt n) = .tint := by
  unfold hybridInt
  split <;> rfl

/-- **C2 tag preservation (shipped B).** `type(x)` reports the Python type
    — including `type(8.0) is float` through the box brand. -/
theorem preservationC2_tag : C2PreservesTag lowerB jsTagShipped := by
  intro v
  cases v with
  | pint n => exact jsTag_hybridInt n
  | pfloat n => rfl
  | pfloatX d => rfl
  | pstr s => rfl
  | pbool b => rfl
  | plist xs => rfl
  | ptuple xs => rfl
  | pdict kvs => rfl

/-- **Container elements carry the tag (shipped B)**: the lowered list's
    elementwise tags are the source's elementwise Python types. -/
theorem preservationC2_tag_elements (xs : List PyVal) :
    (lowerMList optionB xs).map jsTagShipped = xs.map pyTag := by
  induction xs with
  | nil => rfl
  | cons v vs ih =>
    simp only [lowerMList, List.map_cons, ih, preservationC2_tag v]

/-! ## Minimality + the #462 residual scope (stated, machine-checked) -/

/-- **Minimal-B minimality**: the ONLY boxed carrier the lowering ever
    produces is an integer-valued float — ints (small or large) and
    non-integer floats stay native. -/
theorem optionB_boxes_only_floats :
    ∀ v n, lowerB v = .boxF n → v = .pfloat n := by
  intro v n h
  cases v with
  | pint m =>
    simp only [lowerB, lowerM, optionB, hybridInt] at h
    split at h <;> exact JsVal.noConfusion h
  | pfloat m =>
    simp only [lowerB, lowerM, optionB] at h
    injection h with h'
    rw [h']
  | pfloatX d => exact JsVal.noConfusion h
  | pstr s => exact JsVal.noConfusion h
  | pbool b => exact JsVal.noConfusion h
  | plist xs => exact JsVal.noConfusion h
  | ptuple xs => exact JsVal.noConfusion h
  | pdict kvs => exact JsVal.noConfusion h

/-- **The #462 scope inverse**: a pyths-originated plain integer-valued
    `number` can ONLY be an int — no float in the fragment lowers to a bare
    `numI`. This is the machine-checked statement that the injected-float
    residual lies OUTSIDE the lowering's image (the model's domain). -/
theorem lowerB_numI_inv : ∀ v n, lowerB v = .numI n → v = .pint n := by
  intro v n h
  cases v with
  | pint m =>
    simp only [lowerB, lowerM, optionB, hybridInt] at h
    split at h
    · injection h with h'
      rw [h']
    · exact JsVal.noConfusion h
  | pfloat m => exact JsVal.noConfusion h
  | pfloatX d => exact JsVal.noConfusion h
  | pstr s => exact JsVal.noConfusion h
  | pbool b => exact JsVal.noConfusion h
  | plist xs => exact JsVal.noConfusion h
  | ptuple xs => exact JsVal.noConfusion h
  | pdict kvs => exact JsVal.noConfusion h

/-- **The #462 residual, stated**: a raw native integer-valued `number`
    INJECTED from the host (never through `__pyF`) is dispatched as an int
    — display `"8"`, tag `int` — even if the host meant `8.0`. By
    `lowerB_numI_inv` this value is not in the lowering's image as a float;
    the preservation theorems above are silent about it BY SCOPE, and this
    lemma records exactly what the shipped dispatch does to it. -/
theorem injected_float_residual :
    jsReprB (.numI 8) = "8" ∧ jsTagShipped (.numI 8) = .tint :=
  ⟨rfl, rfl⟩

/-! ## NON-VACUITY — the pre-fix false world is REFUTED (same predicates)

The false world is not a strawman: it is the code in THIS branch's
`runtime/src/operators.js`, empirically re-run 2026-08-21
(`[8.0]` → `"[8]"`). It shares the SHIPPED repr knob (`intDisplay`) —
only the value model differs — so these refutations isolate the fix to
the carrier. -/

/-- The pre-fix carrier CONFLATES int and whole float BEFORE repr runs:
    `8` and `8.0` land as the same untagged plain number — scalar… -/
theorem lowerPre_conflates_scalar :
    lowerPre (.pint 8) = lowerPre (.pfloat 8) := rfl

/-- …and inside a container: the lowered `[8]` and `[8.0]` are the SAME
    JS value in the pre-fix world. -/
theorem lowerPre_conflates_container :
    lowerPre (.plist [.pint 8]) = lowerPre (.plist [.pfloat 8]) := rfl

/-- The SHIPPED carrier does NOT conflate: int and float are
    brand-distinguishable at every value (minimal B's core content) —
    unconditionally, both the small-int and the large-int arm. -/
theorem lowerB_separates (n m : Int) :
    lowerB (.pint n) ≠ lowerB (.pfloat m) := by
  intro h
  simp only [lowerB, lowerM, optionB, hybridInt] at h
  split at h <;> exact JsVal.noConfusion h

/-- **INFORMATION LOSS — no repr on the pre-fix carrier can be C2-faithful.**
    Because the pre-fix lowering conflates `8` and `8.0`, ANY repr function
    over that carrier returns one string for both, while CPython requires
    `"8"` vs `"8.0"`. This is the machine-checked reason the fix had to
    change the VALUE MODEL (box the ambiguous class) rather than patch
    `pyRepr` — and since shipped B kept the repr arm BYTE-IDENTICAL, this
    theorem is exactly the statement that the entire fix content is the
    carrier. (Proof is representation-independent: length arithmetic, no
    string evaluation.) -/
theorem preFixCarrier_cannot_repr :
    ¬ ∃ r : JsVal → String, C2Preserves lowerPre r := by
  rintro ⟨r, h⟩
  have h1 := h (.pint 8)
  have h2 := h (.pfloat 8)
  rw [lowerPre_conflates_scalar] at h1
  simp only [reprRef] at h1 h2
  rw [h1] at h2
  have hlen := congrArg String.length h2
  have hd : (".0" : String).length = 2 := by decide
  rw [String.length_append, hd] at hlen
  omega

/-- **The pre-fix world violates C2** — the SAME predicate the shipped world
    satisfies, refuted. Follows from the information-loss theorem (the
    pre-fix `pyRepr` is one of the reprs that cannot work). -/
theorem brokenContainerRepr_fails : ¬ C2Preserves lowerPre jsReprPre :=
  fun h => preFixCarrier_cannot_repr ⟨jsReprPre, h⟩

/-- **The refutation AT THE EXACT SHIPPED-BUG WITNESS `[8.0]`**: in the
    pre-fix world the lowered container's repr DIFFERS from CPython's
    `"[8.0]"` — it is `"[8]"` (see the `#guard` pins: both strings are
    computed and pinned literally). Same length-arithmetic proof shape. -/
theorem brokenContainerRepr_fails_at_witness :
    jsReprPre (lowerPre (.plist [.pfloat 8])) ≠ reprRef (.plist [.pfloat 8]) := by
  intro h
  simp only [lowerPre, lowerM, lowerMList, preFix, jsReprPre, jsReprM, jsReprMList,
    jsReprMListTail, intDisplay, reprRef, reprRefList, reprRefListTail] at h
  have hlen := congrArg String.length h
  have hd : (".0" : String).length = 2 := by decide
  simp only [String.length_append, hd] at hlen
  omega

/-- **The tag collapse is refuted too**: pre-fix `type(8.0)` reports int
    (`tint`) under the SAME shipped dispatch function — because the carrier
    is a bare `numI` — where CPython says float (`tfloat`). Closed by the
    shipped world (`preservationC2_tag`). -/
theorem brokenTag_fails : ¬ C2PreservesTag lowerPre jsTagShipped := by
  intro h
  have hbad := h (.pfloat 8)
  have hstep : jsTagShipped (lowerPre (.pfloat 8)) = PTag.tint := rfl
  rw [hstep] at hbad
  exact PTag.noConfusion hbad

/-! ## Reference-faithfulness pins (F9) — CPython 3.12.7, live-run 2026-08-21 -/

#guard reprRef (.pfloat 8) = "8.0"                     -- repr(8.0)
#guard reprRef (.pint 8) = "8"                         -- repr(8)
#guard reprRef (.pfloat (-3)) = "-3.0"                 -- repr(-3.0)
#guard reprRef (.pfloatX "3.14") = "3.14"              -- repr(3.14)
#guard reprRef (.pint 9007199254740993) = "9007199254740993"  -- repr(2**53+1)
#guard reprRef (.plist [.pfloat 8]) = "[8.0]"          -- repr([8.0])
#guard reprRef (.ptuple [.pfloat 8]) = "(8.0,)"        -- repr((8.0,))
#guard reprRef (.pdict [(.pint 1, .pfloat 8)]) = "{1: 8.0}"  -- repr({1: 8.0})
#guard reprRef (.plist []) = "[]"                      -- repr([])
#guard reprRef (.ptuple []) = "()"                     -- repr(())
#guard reprRef (.pdict []) = "{}"                      -- repr({})
#guard reprRef (.plist [.pint 1, .pfloat 2, .pstr "a", .pbool true])
    = "[1, 2.0, 'a', True]"                            -- repr([1, 2.0, 'a', True])
#guard reprRef (.plist [.plist [.pfloat 2], .ptuple [.pint 1], .pdict [(.pint 0, .pfloat 0)]])
    = "[[2.0], (1,), {0: 0.0}]"                        -- repr([[2.0], (1,), {0: 0.0}])

/-! ## Shipped-behavior pins — the FIXED world at the bug witnesses
    (match the recorded post-fix node runs @ `6e990502`, header) -/

#guard jsReprB (lowerB (.plist [.pfloat 8])) = "[8.0]"        -- THE fixed bug
#guard jsReprB (lowerB (.plist [.pint 8])) = "[8]"
#guard jsReprB (lowerB (.pfloat 8)) = "8.0"                   -- pyRepr(__pyF(8))
#guard jsReprB (lowerB (.pint 8)) = "8"
#guard jsReprB (lowerB (.pint 9007199254740993)) = "9007199254740993"  -- bigint arm
#guard jsReprB (lowerB (.pfloatX "3.14")) = "3.14"
#guard jsReprB (lowerB (.pdict [(.pint 1, .pfloat 8)])) = "{1: 8.0}"
#guard jsReprB (lowerB (.ptuple [.pfloat 8])) = "(8.0,)"
#guard jsReprB (lowerB (.plist [.pint 1, .pfloat 2, .pstr "a", .pbool true]))
    = "[1, 2.0, 'a', True]"
#guard jsTagShipped (lowerB (.pfloat 8)) = .tfloat            -- type(8.0) is float
#guard jsTagShipped (lowerB (.pint 8)) = .tint
#guard jsTagShipped (lowerB (.pint 9007199254740993)) = .tint -- large int stays int
#guard jsTagShipped (lowerB (.pfloatX "3.14")) = .tfloat
-- the carrier split itself (typeof pins: number / bigint / object) —
-- kernel-checked by rfl (JsVal carries no DecidableEq; these are pins, not
-- theorems):
example : lowerB (.pint 8) = .numI 8 := rfl
example : lowerB (.pint 9007199254740993) = .big 9007199254740993 := rfl
example : lowerB (.pfloat 8) = .boxF 8 := rfl

/-! ## Broken-world pins — the pre-fix world REPRODUCES the recorded bug
    (match the recorded pre-fix node runs 2026-08-21: "[8]", "8", "{1: 8}") -/

#guard jsReprPre (lowerPre (.plist [.pfloat 8])) = "[8]"      -- [8.0] → [8]  ← the bug
#guard jsReprPre (lowerPre (.pfloat 8)) = "8"                 -- 8.0 → 8
#guard jsReprPre (lowerPre (.pdict [(.pint 1, .pfloat 8)])) = "{1: 8}"  -- {1: 8.0} → {1: 8}
#guard jsTagShipped (lowerPre (.pfloat 8)) = .tint            -- type(8.0) is int (D1)
-- The broken world is NOT garbage — it is correct on pure-int values AND on
-- non-integer floats (which is why it shipped for so long); the defect is
-- exactly the integer-valued-float class:
#guard jsReprPre (lowerPre (.plist [.pint 8])) = "[8]"
#guard jsReprPre (lowerPre (.pint 8)) = "8"
#guard jsReprPre (lowerPre (.pfloatX "3.14")) = "3.14"

/-! ## SPOTs — client facts proved THROUGH the theorem, not by evaluation -/

/-- SPOT: the compiled `[8.0]` reprs as `"[8.0]"` — obtained from
    `preservationC2` + the reference computation, NOT by evaluating the
    target side. -/
example : jsReprB (lowerB (.plist [.pfloat 8])) = "[8.0]" := by
  rw [preservationC2 (.plist [.pfloat 8])]
  rfl

/-- SPOT: a nested mixed container — small int, boxed float, large int —
    round-trips its representation through the theorem. -/
example :
    jsReprB (lowerB (.pdict [(.pstr "k", .plist [.pfloat 1, .pint 2, .pint 9007199254740993])]))
      = reprRef (.pdict [(.pstr "k", .plist [.pfloat 1, .pint 2, .pint 9007199254740993])]) :=
  preservationC2 _

/-- SPOT (tag): the compiled float keeps the float tag under the shipped
    dispatch — through `preservationC2_tag`. -/
example : jsTagShipped (lowerB (.pfloat 8)) = .tfloat :=
  preservationC2_tag (.pfloat 8)

/-! ## Axiom pins (per-declaration; ⊆ the three standard, no native-decide escape) -/

/-- info: 'C2TypeRepr.preservationC2' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationC2

/-- info: 'C2TypeRepr.preservationC2_tag' does not depend on any axioms -/
#guard_msgs in
#print axioms preservationC2_tag

/-- info: 'C2TypeRepr.preservationC2_tag_elements' depends on axioms: [propext] -/
#guard_msgs in
#print axioms preservationC2_tag_elements

/-- info: 'C2TypeRepr.preFixCarrier_cannot_repr' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preFixCarrier_cannot_repr

/-- info: 'C2TypeRepr.brokenContainerRepr_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms brokenContainerRepr_fails

/-- info: 'C2TypeRepr.brokenContainerRepr_fails_at_witness' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms brokenContainerRepr_fails_at_witness

/-- info: 'C2TypeRepr.brokenTag_fails' does not depend on any axioms -/
#guard_msgs in
#print axioms brokenTag_fails

/-- info: 'C2TypeRepr.lowerB_separates' does not depend on any axioms -/
#guard_msgs in
#print axioms lowerB_separates

/-- info: 'C2TypeRepr.lowerPre_conflates_container' does not depend on any axioms -/
#guard_msgs in
#print axioms lowerPre_conflates_container

/-- info: 'C2TypeRepr.optionB_boxes_only_floats' does not depend on any axioms -/
#guard_msgs in
#print axioms optionB_boxes_only_floats

/-- info: 'C2TypeRepr.lowerB_numI_inv' does not depend on any axioms -/
#guard_msgs in
#print axioms lowerB_numI_inv

/-- info: 'C2TypeRepr.injected_float_residual' depends on axioms: [propext] -/
#guard_msgs in
#print axioms injected_float_residual

end C2TypeRepr
