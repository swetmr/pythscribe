/- # C1C3C4Outcome — the unified C1∧C3∧C4 outcome-preservation theorem
    (value ∧ error-occurrence ∧ error-KIND+MESSAGE, jointly, on the `PyResult`
    carrier — non-recursive operation fragment)

**The claim.** For the NON-RECURSIVE operation fragment — arithmetic (`-`,
`//`), call-of-non-callable, iterate-non-iterable, attribute access
(present AND missing), subscript (list/str/dict + non-subscriptables), and
sequence replication (incl. the index-size overflow) — the emitted code's
evaluation OUTCOME equals CPython's outcome *exactly*: the value when it
succeeds, the OCCURRENCE of an error, and the exception KIND **and MESSAGE**
when it raises. So `C1 ∧ C3 ∧ C4` hold **jointly**, and by the crown lemma
(`Union7.pyResult_faces_iff_eq`, extended with the message channel in
`pout_faces_iff_eq`) the three faces conjoined are *equivalent to full
outcome equality* `evalTgt shippedW = evalRef` on the fragment
(`joint_faces_iff_outcome_eq`).

**What shipped (the target model's referent).** The CPython-3.14.7
oracle-bump runtime: branch `chore/oracle-bump-3.14.7` @ `555f739c`
("fix(runtime): track CPython 3.14 error-message rewords"), on top of
MERGED MAIN `b28a36da` ("Merge fix/error-kind-fidelity: Python error-KIND
fidelity at inlined op sites (C4 class)", incl. minimal Option B
`6e990502`). `555f739c` changes ONLY the message layer (the 3.14
zero-division unification — see `docs/python-oracle-policy.md`); every
non-message site below is unchanged from `b28a36da`. This pin is now FINALIZED
to the oracle-bump merge commit `f9d4d54c` (branch merged to main 2026-08-26). The
transliterated sites (all in `pythscribe` @ `555f739c`):
  * `runtime/src/runtime.js` — `pyCall`'s not-callable throw (:3102),
    `pyIter`'s null-first/float-brand/generic iterate throws (:184/:189/:192),
    `pyAttr`'s numeric-protocol block (real/imag at :2948–:2949, the
    `!isF`-gated numerator/denominator at :2961–:2963, the missing-attribute
    guard at :3077, the None guard at :2930), `pyGetItem`'s null-first
    (:1396) and non-subscriptable-primitive (:1416) guards + the
    list/str/dict index paths;
  * `runtime/src/operators.js` — `__mulRepCount`'s #471 index-size bound
    (:71–:90, `> 9223372036854775807n || < -9223372036854775808n` →
    `OverflowError("cannot fit 'int' into an index-sized integer")`),
    `pyFloorDiv`'s float path (`__isFloat(a)||__isFloat(b)`, :1066–:1068)
    and int path (:1078) — BOTH throw sites raise the single CPython-3.14
    text `"division by zero"` since `555f739c` (3.12 split them as
    `"float floor division by zero"` / `"integer division or modulo by
    zero"`; the 3.12-era wording is now a REFUTED false world, see
    `legacy312_arith_stub_fails`), and `__binOpTypeError`'s
    unsupported-operand template rendered through the #469-unified
    `__pyTypeName` (:62).
  * Type names everywhere come from `__pyTypeName`'s CARRIER dispatch
    (null → NoneType, `__pyfloat__` brand → float, boolean → bool,
    bigint → int, number → isInteger ? int : float, string → str,
    Array → list, Map → dict) — transliterated as `tgtTypeName`, distinct
    in structure from the reference's type→name map `refTypeName`.

**Reuse (build-on, not reprove).** From `PythExpandVerify`: the `PyResult`
carrier and `Exc` kinds; the vetted CPython-faithful arith reference `evalA`
(its F9 pins live upstream); the INDEPENDENT arith target `evalAtgt` at the
tag-faithful lowering `jsAL`; and `preservationA` (`evalAtgt jsAL = evalA`),
which discharges the arith arm's kind-level outcome equality here. From
`Union7`: the faces `bothOkEq` / `errAgree` / `bothErrKind` and the crown
lemma `pyResult_faces_iff_eq`, extended to the message-carrying outcome by
`pout_faces_iff_eq` (one new message conjunct; the kind-level equivalence is
the reused crown, not reproved).
  * Why `jsAL`, not `d1CollapseAL`: the Union7 arith faces
    (`arithC1_shipped`…) evaluate the FROZEN lattice model at the D1
    whole-float collapse. Merged main ships minimal Option B, which REMOVED
    that collapse — `2.0 - 1` now prints `1.0` (probe row `arith_sub_float`,
    2026-08-22, below). Evaluating this file's arith arm at `d1CollapseAL`
    would model a collapse the runtime no longer has (F9-unfaithful to
    `b28a36da`); at `jsAL` the outcome equation is EXACT — the design
    sketch's "mod D1 quotient" is the identity on this fragment post-#462
    (the repr story is `C2TypeRepr.lean`'s, unchanged here).

**Honesty tier — what is bound vs model-level.**
  * MODEL TIER, transliteration-bound: `evalTgt`'s shipped world transcribes
    the sites listed above, provenance-pinned to `b28a36da`. `evalTgt` is NOT
    a clone of `evalRef` (F1): the arith arm runs the independent `evalAtgt`
    recursion; the dispatch STRUCTURE mirrors the runtime (None-first checks,
    the numeric-protocol block with the `isF` gate, count-check-before-
    receiver-dispatch on replication) where the reference dispatches on the
    Python type directly; type names route through the carrier-dispatch
    `tgtTypeName` vs the reference's `refTypeName`; the arith messages route
    through `tgtArithMsg` vs `refArithMsg` — since the 3.14 unification
    both zero-division arms are the same constant `"division by zero"`
    (forced by reality on both sides, and still discriminating: see
    `legacy312_arith_stub_fails`), while the operand-TypeError templates
    keep their distinct `tgtPName` / `refPName` dispatch structures.
    On the container Ok-arms (subscript/replication lookup arithmetic) both
    sides share the index-normalization helpers — the added content of THIS
    file on those arms is the outcome/kind/message layer; their value layer
    is empirically bound below and model-bound by the earlier waves.
  * EMPIRICALLY bound in two layers:
      - The NON-MESSAGE surface (values, occurrence, kinds, and every
        message `555f739c` did not touch) was bound to the merged-main
        binary (`pythscribe/target/release/pyths.exe` @ `b28a36da`),
        re-run 2026-08-22 against live CPython 3.12.7:
        `experiments/errorkind-ps/errorkind_corpus_ps.py` **99/102 exact,
        0 REAL mismatches** (3 documented NYI `bit_length` exclusions —
        formally excluded from this fragment too, see the guard) — binds
        C3/C4 (kind+msg) on call/iterate/attr/subscript/replication/arith;
        fragment pin probe (68 rows: every Err (kind,msg) and Ok value
        modeled below, incl. the whole-float arith rows `2.0-1 → 1.0`,
        `-7.0//2 → -4.0`, and the attr present/absent matrix): **68/68
        exact** (its 3.12-era zero-division message rows are superseded by
        the next layer).
      - The 3.14 MESSAGE layer (the zero-division rows this file models)
        was re-bound 2026-08-26 to the oracle-bump binary (`pyths.exe` @
        `555f739c`) against live CPython 3.14.7: `pyths run` on `1/0`,
        `1.0/0`, `1%0`, `1.0%0`, `1//0`, `1.0//0`, `divmod(1,0)`,
        `divmod(1.5,0)`, `0**-1`, `0.0**-1`, `x/=0`, `x//=0`, `1 in 5` —
        **13/13 byte-exact** (`2.0//0` and `1//0` BOTH →
        `ZeroDivisionError: division by zero`), plus the full 1,376-case
        `tests/differential/run.mjs` corpus green under
        `PYTHS_ORACLE_PYTHON="py -3.14"`.
      - `experiments/pbt-ps/slice_shipped_binding.py`: 15,612 cases, 0
        mismatches — binds C1 on subscript/slice values.
      - `experiments/pbt-ps/pyeq_shipped_binding.py`: 2,063 cases, 0
        mismatches — binds C1 on equality/values.
    The CPython `#guard` pins below are the reference-faithfulness gate (F9).

**The false world this file refutes (exact discriminating witnesses).** The
PRE-errkind-fix behavior, recorded by the errorkind-ps baseline and the #471
fix comment (operators.js:74–76 documents the pre-fix JS `RangeError
("Invalid array length")`); each witness isolates ONE conjunct:
  * `witOverflow` = `[1] * (10**30)`: pre-fix `RangeError` vs CPython
    `OverflowError` → **C4 KIND** mismatch (`overflow_rep_stub_fails`).
  * `witCallDict` = `d()` on a dict: pre-fix raw-JS `TypeError: "d is not a
    function"` vs CPython `TypeError: "'dict' object is not callable"` —
    SAME kind, different MESSAGE → refuted by the message conjunct
    (`call_dict_stub_fails`), and `call_dict_kind_blind` PROVES the
    kind-only C4 face holds at this witness: the message strengthening is
    load-bearing, not decoration.
  * `witAttrMiss` = `(5).foo`: pre-fix silent JS property read → `Ok
    undefined` where CPython raises `AttributeError` → **C3 OCCURRENCE**
    mismatch (`attr_missing_stub_fails`); `attr_missing_c1_blind` /
    `attr_missing_c4_blind` prove C1 and C4 are VACUOUSLY satisfied at this
    witness — only C3 catches it. This is the machine-checked value of the
    JOIN: no single face subsumes the others.
  * `witFdivZero` = `1 // 0` (the 3.14 oracle bump's own false world): a
    runtime still emitting the CPython-3.12 split zero-division wording
    (`"integer division or modulo by zero"` / `"float floor division by
    zero"`) — i.e. the PRE-`555f739c` shipped behavior — raises the SAME
    kind, wrong MESSAGE → refuted by the message conjunct
    (`legacy312_arith_stub_fails`); `legacy312_arith_kind_blind` proves the
    kind-only face is blind here, so the post-collapse constant message is
    still DISCRIMINATING content, not a vacuous simplification.

**Fragment scope (honest domain — the decidable guard `fragOK`).**
  * Non-recursive: operands are VALUES (each op produces a value or raises
    immediately) — no fuel, no environment; binding forms and recursion are
    the earlier waves' fragments. (The design sketch's `env` parameter is
    vacuous here and dropped.)
  * Whole floats only (`vfloat n` = the boxed `n.0` class — same stated
    simplification as the upstream `PVal`); the float-numerator
    counter-case is carried by `(4.0).numerator → AttributeError` (probed,
    exact) — sharper than `2.5` because `4` vs `4.0` differ ONLY in the
    brand, pinning the model's carrier-sensitivity.
  * Attribute names guarded to {numerator, denominator, real, imag, foo,
    bar}: the fragment claims exactly the probed matrix; `bit_length` (NYI
    upstream, the corpus's 3 documented exclusions) is thereby FORMALLY
    excluded — by the guard, not by prose (F10).
  * Subscript keys: int for sequences, str for dicts (the
    "indices must be integers" cross-type diagnostics are out of fragment);
    dicts are string-keyed with int values; lists are int-element (the
    heterogeneous-container story is C2TypeRepr's).
  * Replication counts guarded to |n| ≤ 4096 ∨ n outside the ssize_t range:
    the murky middle (2^12 < n < 2^63, where CPython's MemoryError meets the
    JS allocation limit) is FORMALLY excluded, not silently claimed.
  * `iterate`'s Ok payload is the iterable itself (occurrence+kind content;
    element-value fidelity of iteration is owned by the comprehension waves
    upstream). `call` is zero-arg on a result-carrying callable token.

**F10 (the falsifiable knob reaches every op).** The target world `OWorld`
carries one compiled-target function PER OP (the `Union7.World` discipline);
`shippedW` holds the transliterations, and each `preFix*W` world perturbs
exactly one field and is refuted at its witness. A wrong shipped lowering of
ANY covered op falsifies `outcome_eq` through its field. -/
import Union7
import MessageData

/- E7 message-layer binding: every LIVE message literal below is built from
   `MessageData` (GENERATED from verification/message-table.json by
   gen-message-data.py; CI drift-gates the generation, and
   verification/message_shipped_binding.py pins the same table rows against
   the REAL `pyths` binary and the CPython oracle). A runtime message change
   therefore turns the differential red, and the table update it forces
   re-evaluates this file's #guard pins — the transcribed-literal drift the
   3.14 oracle bump exposed cannot recur. The FALSE-WORLD wordings
   (`legacy312ArithMsg`, `preFixCall`, `preFixRepl`) deliberately stay
   inline: they encode refuted worlds, not the live model. -/

namespace C1C3C4Outcome

open PythExpandVerify Union7

/-! ## Values and the message-carrying outcome carrier -/

/-- Fragment values. `vfloat n` is the boxed whole float `n.0` (minimal-B
    `PyFloat` brand); `vfun r` is a zero-arg callable returning `r`;
    `vundef` is the TARGET-ARTIFACT JS `undefined` — never a Python source
    value (the guard excludes it), produced only by the pre-fix false world's
    silent attribute read. -/
inductive OVal where
  | vint (n : Int) | vbool (b : Bool) | vfloat (n : Int)
  | vstr (s : String)
  | vlist (xs : List Int)
  | vdict (kvs : List (String × Int))
  | vnone
  | vfun (ret : Int)
  | vundef
  deriving DecidableEq, Repr

/-- The unified outcome: the kind-level `PyResult` (REUSED from the lattice)
    plus the exception-MESSAGE channel (`""` on success, by construction of
    both evaluators). C4 here is kind AND message — the `d()` witness shows
    kind alone cannot discriminate the pre-fix world. -/
structure POut where
  res : PyResult OVal
  msg : String
  deriving DecidableEq, Repr

def okV (v : OVal) : POut := ⟨.ok v, ""⟩
def errV (k : Exc) (m : String) : POut := ⟨.err k, m⟩

/-- **Crown-lemma extension (message-strengthened no-loss).** The C1 face +
    C3 face + (kind ∧ message) C4 face conjoined recover FULL outcome
    equality. The kind-level three-face equivalence is
    `Union7.pyResult_faces_iff_eq` — reused, not reproved; the message
    conjunct is the one new ingredient. -/
theorem pout_faces_iff_eq (t r : POut) :
    (bothOkEq t.res r.res ∧ errAgree t.res r.res ∧
      bothErrKind t.res r.res ∧ t.msg = r.msg) ↔ t = r := by
  constructor
  · rintro ⟨h1, h2, h3, h4⟩
    have hres : t.res = r.res := (pyResult_faces_iff_eq t.res r.res).mp ⟨h1, h2, h3⟩
    cases t; cases r
    simp_all
  · rintro rfl
    exact ⟨bothOkEq_of_eq rfl, errAgree_of_eq rfl, bothErrKind_of_eq rfl, rfl⟩

/-! ## Type names — reference vs target (two structures, one truth) -/

/-- CPython `type(x).__name__` — the SOURCE-side observation (a direct
    type→name map). -/
def refTypeName : OVal → String
  | .vint _ => "int" | .vbool _ => "bool" | .vfloat _ => "float"
  | .vstr _ => "str" | .vlist _ => "list" | .vdict _ => "dict"
  | .vnone => "NoneType" | .vfun _ => "function" | .vundef => "undefined"

/-- `__pyTypeName` transliteration (#469 — THE one value-model type-name
    authority), in the runtime's CARRIER-dispatch order: null first, then the
    `__pyfloat__` brand, then `typeof` (boolean / bigint / number-isInteger /
    string), then Array / Map. -/
def tgtTypeName : OVal → String
  | .vnone => "NoneType"       -- null/undefined guard, first
  | .vfloat _ => "float"       -- __pyfloat__ brand (Option-B boxed whole float)
  | .vbool _ => "bool"         -- typeof "boolean"
  | .vint _ => "int"           -- typeof "number" (isInteger) / "bigint"
  | .vstr _ => "str"           -- typeof "string"
  | .vlist _ => "list"         -- Array.isArray, no __pytuple__
  | .vdict _ => "dict"         -- instanceof Map
  | .vfun _ => "function"
  | .vundef => "undefined"

/-! ## The arith arm — reusing the lattice's independent evaluators -/

/-- Fragment arithmetic ops — exactly the vetted `AExp` ops (`-`, `//`),
    whose reference `evalA` carries the upstream F9 pins (type-check-precedes-
    zero-check, float-tag propagation, floor-not-trunc). -/
inductive AOp where
  | osub | ofdiv
  deriving DecidableEq, Repr

def AOp.sym : AOp → String
  | .osub => "-" | .ofdiv => "//"

/-- Non-recursive embedding into the lattice arith fragment: operands are
    literals, so the error site (if any) is the top-level op — messages are a
    function of (op, operand values). -/
def embed : AOp → PVal → PVal → AExp
  | .osub, a, b => .sub (.lit a) (.lit b)
  | .ofdiv, a, b => .fdiv (.lit a) (.lit b)

def ofPVal : PVal → OVal
  | .pint n => .vint n | .pfloat n => .vfloat n | .pstr s => .vstr s

def isFloatP : PVal → Bool
  | .pfloat _ => true | _ => false

/-- CPython type name of an arith operand (source side). -/
def refPName : PVal → String
  | .pint _ => "int" | .pfloat _ => "float" | .pstr _ => "str"

/-- `__pyTypeName` on the lowered arith operand (brand first, then typeof) —
    the target side of the operand-TypeError template. -/
def tgtPName : PVal → String
  | .pfloat _ => "float"       -- __pyfloat__ brand
  | .pint _ => "int" | .pstr _ => "str"

/-- CPython 3.14.7's messages at the arith error sites, in CPython's shape:
    since 3.14 EVERY division/modulo ZeroDivisionError carries the single
    text `"division by zero"` — the 3.12-era dependence on WHICH
    `__floordiv__` ran (both-int → `int.__floordiv__`'s text, otherwise the
    float coercion path's) was removed upstream (witness: `py -3.14 -c
    "1//0"` and `"2.0//0"` both print `division by zero`; the old split is
    the refuted `legacy312ArithMsg` below). A failed operand dispatch still
    renders the unsupported-operand template. (Junk inputs — e.g. zeroDiv
    with a str operand — never arise: type-check precedes zero-check in
    `evalA`, upstream-pinned.) -/
def refArithMsg (op : AOp) (a b : PVal) : Exc → String
  | .zeroDiv => MessageData.zeroDiv
  | .typeError => MessageData.unsupportedOperand op.sym (refPName a) (refPName b)
  | _ => ""

/-- `operators.js` @ `555f739c` transliteration: `pyFloorDiv` still has TWO
    throw sites — the float path, entered when EITHER operand is float
    (`__isFloat(a) || __isFloat(b)`, :1066, throw :1068), and the int path
    (:1078) — but since the 3.14 oracle bump BOTH raise the same
    `"division by zero"`, so the transliterated message is the constant
    (the branch structure survives only in the runtime's control flow, not
    in the emitted text; the pre-bump two-text version is
    `legacy312ArithMsg`). `__binOpTypeError` (:62) renders the
    unsupported-operand template through `__pyTypeName`. -/
def tgtArithMsg (op : AOp) (a b : PVal) : Exc → String
  | .zeroDiv => MessageData.zeroDiv
  | .typeError => MessageData.unsupportedOperand op.sym (tgtPName a) (tgtPName b)
  | _ => ""

/-! ## The reference evaluator (CPython semantics) -/

/-- Arith reference: the vetted `evalA` (reused) + CPython's messages. -/
def refArith (op : AOp) (a b : PVal) : POut :=
  match evalA (embed op a b) [] with
  | .ok v => okV (ofPVal v)
  | .err k => errV k (refArithMsg op a b k)

def refCall : OVal → POut
  | .vfun r => okV (.vint r)
  | x => errV .typeError (MessageData.notCallable (refTypeName x))

def refIter : OVal → POut
  | .vstr s => okV (.vstr s)
  | .vlist xs => okV (.vlist xs)
  | .vdict kvs => okV (.vdict kvs)
  | x => errV .typeError (MessageData.notIterable (refTypeName x))

def refAttrMiss (x : OVal) (name : String) : POut :=
  errV .attributeError (MessageData.noAttribute (refTypeName x) name)

/-- CPython attribute semantics on the fragment (probed 2026-08-22, CPython
    3.12.7): int/bool carry the Rational protocol (`numerator` = self as int,
    `denominator` = 1, `real` = self as int, `imag` = 0 — bool coerces to
    int: `True.numerator` is `1`, not `True`); float has `real`/`imag` (as
    FLOATS) but NO `numerator`/`denominator`; everything else on the
    whitelist is absent → AttributeError. -/
def refAttr : OVal → String → POut
  | .vint n, "numerator" => okV (.vint n)
  | .vint _, "denominator" => okV (.vint 1)
  | .vint n, "real" => okV (.vint n)
  | .vint _, "imag" => okV (.vint 0)
  | .vbool b, "numerator" => okV (.vint (if b then 1 else 0))
  | .vbool _, "denominator" => okV (.vint 1)
  | .vbool b, "real" => okV (.vint (if b then 1 else 0))
  | .vbool _, "imag" => okV (.vint 0)
  | .vfloat n, "real" => okV (.vfloat n)
  | .vfloat _, "imag" => okV (.vfloat 0)
  | x, name => refAttrMiss x name

/-- Python sequence indexing: negative indices count from the end; out of
    range on both ends → `none`. Shared by both sides (the value arithmetic
    is not this file's discriminating content — see the header; the slice
    differential binds it empirically). -/
def listNth (xs : List Int) (i : Int) : Option Int :=
  let j := if i < 0 then i + xs.length else i
  if j < 0 then none else xs[j.toNat]?

def strNth (s : String) (i : Int) : Option Char :=
  let cs := s.toList
  let j := if i < 0 then i + cs.length else i
  if j < 0 then none else cs[j.toNat]?

def dictGet : List (String × Int) → String → Option Int
  | [], _ => none
  | (k, v) :: rest, key => if k = key then some v else dictGet rest key

def refSub : OVal → OVal → POut
  | .vlist xs, .vint i =>
    match listNth xs i with
    | some v => okV (.vint v)
    | none => errV .indexError MessageData.listIndexOor
  | .vstr s, .vint i =>
    match strNth s i with
    | some c => okV (.vstr c.toString)
    | none => errV .indexError MessageData.strIndexOor
  | .vdict kvs, .vstr k =>
    match dictGet kvs k with
    | some v => okV (.vint v)
    | none => errV .keyError (MessageData.keyError k)
  | x, _ => errV .typeError (MessageData.notSubscriptable (refTypeName x))

/-- `Py_ssize_t` bounds — CPython's index-size range (`__index__` conversion
    raises OverflowError outside it). -/
def ssizeMax : Int := 9223372036854775807
def ssizeMin : Int := -9223372036854775808

def listRep (xs : List Int) (n : Int) : List Int :=
  (List.replicate n.toNat xs).flatten

def strRep (s : String) (n : Int) : String :=
  (List.replicate n.toNat s).foldl (· ++ ·) ""

/-- CPython replication: the receiver dispatches first (`list.__mul__`), the
    count's `__index__` conversion raises OverflowError outside ssize_t;
    negative counts yield the empty sequence. -/
def refRepl : OVal → Int → POut
  | .vlist xs, n =>
    if n > ssizeMax ∨ n < ssizeMin then
      errV .overflow MessageData.overflowIndex
    else okV (.vlist (listRep xs n))
  | .vstr s, n =>
    if n > ssizeMax ∨ n < ssizeMin then
      errV .overflow MessageData.overflowIndex
    else okV (.vstr (strRep s n))
  | _, _ => errV .typeError ""     -- out of guard (receiver ∉ {list, str})

/-! ## The fragment and its guard -/

/-- The non-recursive operation fragment (value operands — see the header).
    `call` carries the source-level callee NAME because the pre-fix raw-JS
    message referenced source text (`"d is not a function"`); the shipped and
    reference sides ignore it. -/
inductive OExp where
  | arith (op : AOp) (a b : PVal)
  | call (srcName : String) (f : OVal)
  | iter (x : OVal)
  | attr (x : OVal) (name : String)
  | subscr (x : OVal) (i : OVal)
  | repl (x : OVal) (n : Int)
  deriving Repr

def evalRef : OExp → POut
  | .arith op a b => refArith op a b
  | .call _ f => refCall f
  | .iter x => refIter x
  | .attr x n => refAttr x n
  | .subscr x i => refSub x i
  | .repl x n => refRepl x n

/-- Plain (non-callable, non-artifact) source values. -/
def plainVal : OVal → Bool
  | .vint _ | .vbool _ | .vfloat _ | .vstr _ | .vlist _ | .vdict _ | .vnone => true
  | _ => false

/-- The attribute-name whitelist — the probed present/absent matrix.
    `bit_length` (CPython-present, runtime-NYI — the corpus's documented
    exclusions) is FORMALLY outside the fragment via this guard. -/
def attrNameOK (n : String) : Bool :=
  n == "numerator" || n == "denominator" || n == "real" || n == "imag"
    || n == "foo" || n == "bar"

def subPairOK : OVal → OVal → Bool
  | .vlist _, .vint _ => true
  | .vstr _, .vint _ => true
  | .vdict _, .vstr _ => true
  | .vint _, .vint _ => true      -- non-subscriptable receivers, int key:
  | .vbool _, .vint _ => true     --   the probed TypeError rows
  | .vfloat _, .vint _ => true
  | .vnone, .vint _ => true
  | _, _ => false

/-- Replication-count guard: small counts (the value rows) or counts outside
    the ssize_t range (the OverflowError rows). The middle is excluded —
    stated in the header, enforced here. -/
def replCountOK (n : Int) : Bool :=
  decide (n.natAbs ≤ 4096) || decide (n > ssizeMax) || decide (n < ssizeMin)

/-- **The decidable fragment guard (F10: exclusions are formal, not prose).** -/
def fragOK : OExp → Bool
  | .arith _ _ _ => true
  | .call _ f => (match f with | .vundef => false | _ => true)
  | .iter x => plainVal x
  | .attr x n => plainVal x && attrNameOK n
  | .subscr x i => subPairOK x i
  | .repl x n => (match x with | .vlist _ | .vstr _ => true | _ => false)
      && replCountOK n

/-! ## The target: one compiled-target function per op (the F10 knob) -/

/-- A world: the compiled target of every fragment op (the `Union7.World`
    discipline). `shippedW` holds the transliterations; each false world
    perturbs exactly one field. -/
structure OWorld where
  tArith : AOp → PVal → PVal → POut
  tCall : String → OVal → POut
  tIter : OVal → POut
  tAttr : OVal → String → POut
  tSub : OVal → OVal → POut
  tRepl : OVal → Int → POut

def evalTgt (w : OWorld) : OExp → POut
  | .arith op a b => w.tArith op a b
  | .call nm f => w.tCall nm f
  | .iter x => w.tIter x
  | .attr x n => w.tAttr x n
  | .subscr x i => w.tSub x i
  | .repl x n => w.tRepl x n

/-- Shipped arith: the INDEPENDENT lattice target `evalAtgt` at the
    tag-faithful `jsAL` (see the header for why not `d1CollapseAL`), with the
    runtime's messages. -/
def shippedArith (op : AOp) (a b : PVal) : POut :=
  match evalAtgt jsAL (embed op a b) [] with
  | .ok v => okV (ofPVal v)
  | .err k => errV k (tgtArithMsg op a b k)

/-- `pyCall` (runtime.js:3102): callables run; everything else raises through
    `__pyTypeName`. The source name is ignored — the SHIPPED runtime raises
    the Python-style message (that it once did not is the pre-fix world). -/
def shippedCall (_srcName : String) : OVal → POut
  | .vfun r => okV (.vint r)
  | x => errV .typeError (MessageData.notCallable (tgtTypeName x))

/-- `pyIter` (runtime.js:184/:189/:192): null guard FIRST (hardcoded
    NoneType text), then the `__pyfloat__` brand (hardcoded float text), then
    the iterables, then the generic `__pyTypeName` template. -/
def shippedIter : OVal → POut
  | .vnone => errV .typeError (MessageData.notIterable "NoneType")
  | .vfloat _ => errV .typeError (MessageData.notIterable "float")
  | .vstr s => okV (.vstr s)
  | .vlist xs => okV (.vlist xs)
  | .vdict kvs => okV (.vdict kvs)
  | x => errV .typeError (MessageData.notIterable (tgtTypeName x))

def tgtAttrMiss (x : OVal) (name : String) : POut :=
  errV .attributeError (MessageData.noAttribute (tgtTypeName x) name)

/-- The numeric-protocol receiver class (runtime.js:2941): JS number, bigint,
    boolean, or the `__pyfloat__` brand. -/
def numericRecv : OVal → Bool
  | .vint _ | .vbool _ | .vfloat _ => true
  | _ => false

/-- The `isF` gate (runtime.js:2945): the float brand. -/
def isFRecv : OVal → Bool
  | .vfloat _ => true | _ => false

/-- `pyAttr` transliteration: the None guard (:2930); the numeric-protocol
    block — `real` returns the receiver (bool → int, :2948), `imag` is
    float-0 iff `isF` (:2949), `numerator`/`denominator` are gated on `!isF`
    (:2961–:2963, the errkind review-round-2 fix: a float receiver falls
    through to the missing guard); then the missing-attribute guard (:3077). -/
def shippedAttr (x : OVal) (name : String) : POut :=
  match x with
  | .vnone =>
    errV .attributeError (MessageData.noAttribute "NoneType" name)
  | x =>
    if numericRecv x then
      match name with
      | "real" =>
        okV (match x with | .vbool b => .vint (if b then 1 else 0) | v => v)
      | "imag" => okV (if isFRecv x then .vfloat 0 else .vint 0)
      | "numerator" =>
        if isFRecv x then tgtAttrMiss x name
        else okV (match x with | .vbool b => .vint (if b then 1 else 0) | v => v)
      | "denominator" =>
        if isFRecv x then tgtAttrMiss x name else okV (.vint 1)
      | _ => tgtAttrMiss x name
    else tgtAttrMiss x name

/-- `pyGetItem` transliteration: null guard first (:1396, hardcoded text);
    the non-subscriptable-primitive guard (:1416, through `__pyTypeName` —
    the lattice C4 shipping-binding's own fix site); then the list/str/dict
    paths with CPython's index normalization. -/
def shippedSub (x i : OVal) : POut :=
  match x with
  | .vnone => errV .typeError (MessageData.notSubscriptable "NoneType")
  | .vint _ | .vbool _ | .vfloat _ =>
    errV .typeError (MessageData.notSubscriptable (tgtTypeName x))
  | .vlist xs =>
    match i with
    | .vint j =>
      match listNth xs j with
      | some v => okV (.vint v)
      | none => errV .indexError MessageData.listIndexOor
    | _ => errV .typeError ""    -- out of guard (cross-type key diagnostics)
  | .vstr s =>
    match i with
    | .vint j =>
      match strNth s j with
      | some c => okV (.vstr c.toString)
      | none => errV .indexError MessageData.strIndexOor
    | _ => errV .typeError ""
  | .vdict kvs =>
    match i with
    | .vstr k =>
      match dictGet kvs k with
      | some v => okV (.vint v)
      | none => errV .keyError (MessageData.keyError k)
    | _ => errV .typeError ""
  | x => errV .typeError (MessageData.notSubscriptable (tgtTypeName x))

/-- `__mulRepCount` #471 transliteration (operators.js:71–:90): the count is
    bounds-checked FIRST (before receiver dispatch — the runtime's order,
    opposite the reference's), outside ssize_t → OverflowError with
    CPython's text; then the sequence repeats (negative → empty). -/
def shippedRepl (x : OVal) (n : Int) : POut :=
  if n > ssizeMax ∨ n < ssizeMin then
    errV .overflow MessageData.overflowIndex
  else
    match x with
    | .vlist xs => okV (.vlist (listRep xs n))
    | .vstr s => okV (.vstr (strRep s n))
    | _ => errV .typeError ""    -- out of guard

/-- **The shipped world** — the merged-main (`b28a36da`) transliterations. -/
def shippedW : OWorld :=
  { tArith := shippedArith, tCall := shippedCall, tIter := shippedIter,
    tAttr := shippedAttr, tSub := shippedSub, tRepl := shippedRepl }

/-! ## THE THEOREM — unified outcome equality, then C1∧C3∧C4 as corollary -/

/-- The arith arm: kind-level equality is `preservationA` (REUSED — the
    independent-target lattice theorem); the two differently-structured
    message models then compute the same strings at every reachable error
    site (full operand/zero case analysis). -/
theorem shippedArith_eq (op : AOp) (a b : PVal) :
    shippedArith op a b = refArith op a b := by
  unfold shippedArith refArith
  have hp : evalAtgt jsAL (embed op a b) [] = evalA (embed op a b) [] :=
    preservationA (embed op a b) []
  rw [hp]
  cases op <;> cases a <;> cases b <;>
    simp only [embed, evalA, tgtArithMsg, refArithMsg, tgtPName,
      refPName, AOp.sym] <;>
    first
      | rfl
      | (split <;> rfl)

theorem shippedCall_eq (nm : String) (f : OVal)
    (_h : fragOK (.call nm f) = true) : shippedCall nm f = refCall f := by
  cases f <;> rfl

theorem shippedIter_eq (x : OVal) (_h : fragOK (.iter x) = true) :
    shippedIter x = refIter x := by
  cases x <;> rfl

theorem shippedAttr_eq (x : OVal) (nm : String)
    (h : fragOK (.attr x nm) = true) : shippedAttr x nm = refAttr x nm := by
  simp only [fragOK, Bool.and_eq_true] at h
  have hn' := h.2
  simp only [attrNameOK, Bool.or_eq_true, beq_iff_eq] at hn'
  rcases hn' with ((((rfl | rfl) | rfl) | rfl) | rfl) | rfl <;> cases x <;> rfl

theorem shippedSub_eq (x i : OVal) (h : fragOK (.subscr x i) = true) :
    shippedSub x i = refSub x i := by
  cases x <;> cases i <;>
    first
      | rfl
      | (simp [fragOK, subPairOK] at h)

theorem shippedRepl_eq (x : OVal) (n : Int) (h : fragOK (.repl x n) = true) :
    shippedRepl x n = refRepl x n := by
  cases x <;>
    first
      | rfl
      | (simp [fragOK] at h)

/-- **`outcome_eq` — THE unified preservation equation.** On the guarded
    fragment, the shipped target's OUTCOME — value, occurrence, kind, AND
    message — is CPython's. Not `rfl`: the arith arm routes through the
    independent `evalAtgt` recursion and `preservationA`; the other arms
    reconcile two differently-structured dispatches. -/
theorem outcome_eq (e : OExp) (h : fragOK e = true) :
    evalTgt shippedW e = evalRef e := by
  cases e with
  | arith op a b => exact shippedArith_eq op a b
  | call nm f => exact shippedCall_eq nm f h
  | iter x => exact shippedIter_eq x h
  | attr x n => exact shippedAttr_eq x n h
  | subscr x i => exact shippedSub_eq x i h
  | repl x n => exact shippedRepl_eq x n h

/-! ## The three components (faces REUSED from Union7) + the corollary -/

/-- **C1 (value)**: both sides succeed ⇒ same value. -/
def C1 (w : OWorld) : Prop :=
  ∀ e, fragOK e = true → bothOkEq (evalTgt w e).res (evalRef e).res

/-- **C3 (error-occurrence)**: an error occurs iff one occurs. -/
def C3 (w : OWorld) : Prop :=
  ∀ e, fragOK e = true → errAgree (evalTgt w e).res (evalRef e).res

/-- **C4 (error-kind AND message)**: both sides err ⇒ same exception class,
    and the message channels agree. The message conjunct is load-bearing —
    see `call_dict_kind_blind`. -/
def C4 (w : OWorld) : Prop :=
  ∀ e, fragOK e = true →
    bothErrKind (evalTgt w e).res (evalRef e).res
      ∧ (evalTgt w e).msg = (evalRef e).msg

/-- **THE HEADLINE — unified C1 ∧ C3 ∧ C4 preservation for the shipped
    world**, as a COROLLARY of `outcome_eq` (faces of a proved equation). -/
theorem preservationC1C3C4 : C1 shippedW ∧ C3 shippedW ∧ C4 shippedW := by
  refine ⟨fun e h => ?_, fun e h => ?_, fun e h => ?_⟩ <;> rw [outcome_eq e h]
  · exact bothOkEq_of_eq rfl
  · exact errAgree_of_eq rfl
  · exact ⟨bothErrKind_of_eq rfl, rfl⟩

/-- **The crown corollary (no-loss).** For ANY world, the three faces
    conjoined are EQUIVALENT to full outcome equality on the fragment — via
    the reused `pyResult_faces_iff_eq` (through `pout_faces_iff_eq`). The
    decomposition weakens nothing. -/
theorem joint_faces_iff_outcome_eq (w : OWorld) :
    (C1 w ∧ C3 w ∧ C4 w)
      ↔ (∀ e, fragOK e = true → evalTgt w e = evalRef e) := by
  constructor
  · rintro ⟨h1, h3, h4⟩ e he
    exact (pout_faces_iff_eq _ _).mp
      ⟨h1 e he, h3 e he, (h4 e he).1, (h4 e he).2⟩
  · intro h
    exact ⟨fun e he => bothOkEq_of_eq (congrArg POut.res (h e he)),
           fun e he => errAgree_of_eq (congrArg POut.res (h e he)),
           fun e he => ⟨bothErrKind_of_eq (congrArg POut.res (h e he)),
                        congrArg POut.msg (h e he)⟩⟩

/-! ## NON-VACUITY — the pre-fix false worlds, refuted at the EXACT
    discriminating witnesses (each perturbs ONE `OWorld` field) -/

/-- PRE-FIX replication: no ssize_t bound — a huge count reached the JS
    allocation and threw `RangeError("Invalid array length")` (recorded in
    the #471 fix comment, operators.js:74–76, and the errorkind-ps pre-fix
    baseline). -/
def preFixRepl (x : OVal) (n : Int) : POut :=
  if n.natAbs ≤ 4096 then
    match x with
    | .vlist xs => okV (.vlist (listRep xs n))
    | .vstr s => okV (.vstr (strRep s n))
    | _ => errV .typeError ""
  else errV (.custom "RangeError") "Invalid array length"

/-- PRE-FIX call: the raw JS call — `d()` on a non-function threw the
    ENGINE's TypeError, whose message references SOURCE TEXT
    (`"d is not a function"`), not the Python type. Same KIND as CPython —
    only the message discriminates. -/
def preFixCall (srcName : String) : OVal → POut
  | .vfun r => okV (.vint r)
  | _ => errV .typeError (srcName ++ " is not a function")

/-- PRE-FIX attribute: the raw JS property read — a missing (or then-unbound
    protocol) attribute on a primitive silently yielded `undefined` instead
    of raising (the errkind fix's C3-class site: `Ok undefined` where CPython
    raises AttributeError). -/
def preFixAttr (_x : OVal) (_name : String) : POut := okV .vundef

def preFixReplW : OWorld := { shippedW with tRepl := preFixRepl }
def preFixCallW : OWorld := { shippedW with tCall := preFixCall }
def preFixAttrW : OWorld := { shippedW with tAttr := preFixAttr }
/-- The combined pre-fix world (all three recorded defect families). -/
def preFixW : OWorld :=
  { shippedW with tRepl := preFixRepl, tCall := preFixCall, tAttr := preFixAttr }

/-- `[1] * (10**30)` — the C4-KIND witness. -/
def witOverflow : OExp := .repl (.vlist [1]) (10 ^ 30)
/-- `d()` where `d` is a dict — the C4-MESSAGE witness. -/
def witCallDict : OExp := .call "d" (.vdict [("a", 1)])
/-- `(5).foo` — the C3-OCCURRENCE witness. -/
def witAttrMiss : OExp := .attr (.vint 5) "foo"

-- The witnesses are in-fragment, and each side's outcome is pinned
-- (discriminating: the pre-fix and shipped values differ at each witness):
#guard fragOK witOverflow = true
#guard fragOK witCallDict = true
#guard fragOK witAttrMiss = true
#guard evalRef witOverflow
    = errV .overflow "cannot fit 'int' into an index-sized integer"
#guard evalTgt shippedW witOverflow
    = errV .overflow "cannot fit 'int' into an index-sized integer"
#guard evalTgt preFixReplW witOverflow
    = errV (.custom "RangeError") "Invalid array length"
#guard evalRef witCallDict = errV .typeError "'dict' object is not callable"
#guard evalTgt shippedW witCallDict
    = errV .typeError "'dict' object is not callable"
#guard evalTgt preFixCallW witCallDict = errV .typeError "d is not a function"
#guard evalRef witAttrMiss
    = errV .attributeError "'int' object has no attribute 'foo'"
#guard evalTgt shippedW witAttrMiss
    = errV .attributeError "'int' object has no attribute 'foo'"
#guard evalTgt preFixAttrW witAttrMiss = okV .vundef

/-- **Stub litmus, C4 KIND axis.** The pre-fix replication world raises
    `RangeError` where CPython raises `OverflowError` — the kind face
    refutes it at `[1] * (10**30)`. -/
theorem overflow_rep_stub_fails : ¬ C4 preFixReplW := by
  intro h
  have hk := (h witOverflow (by decide)).1
  have h1 : (evalTgt preFixReplW witOverflow).res = .err (.custom "RangeError") := by
    decide
  have h2 : (evalRef witOverflow).res = .err .overflow := by decide
  have := hk _ _ h1 h2
  exact absurd this (by decide)

/-- **Stub litmus, C4 MESSAGE axis.** The pre-fix call world's message is
    the engine's `"d is not a function"`, not CPython's `"'dict' object is
    not callable"` — the MESSAGE conjunct refutes it. -/
theorem call_dict_stub_fails : ¬ C4 preFixCallW := by
  intro h
  have hm := (h witCallDict (by decide)).2
  exact absurd hm (by decide)

/-- **The kind face is BLIND at the `d()` witness** — both worlds raise
    `TypeError`, so kind-only C4 holds where message-carrying C4 fails: the
    message strengthening is load-bearing, machine-checked. -/
theorem call_dict_kind_blind :
    bothErrKind (evalTgt preFixCallW witCallDict).res (evalRef witCallDict).res := by
  intro e f he hf
  have h1 : (evalTgt preFixCallW witCallDict).res = .err .typeError := by decide
  have h2 : (evalRef witCallDict).res = .err .typeError := by decide
  rw [h1] at he; rw [h2] at hf
  injection he with he'; injection hf with hf'
  rw [← he', ← hf']

/-- **Stub litmus, C3 OCCURRENCE axis.** The pre-fix attribute world returns
    `Ok undefined` where CPython raises — the occurrence face refutes it at
    `(5).foo`. -/
theorem attr_missing_stub_fails : ¬ C3 preFixAttrW := by
  intro h
  have hocc := h witAttrMiss (by decide)
  simp only [errAgree] at hocc
  exact absurd hocc (by decide)

/-- **C1 is BLIND at the attr witness** (vacuously satisfied: the reference
    side is an error, so "both ok" never fires)… -/
theorem attr_missing_c1_blind :
    bothOkEq (evalTgt preFixAttrW witAttrMiss).res (evalRef witAttrMiss).res := by
  intro v w _ hw
  have h2 : (evalRef witAttrMiss).res = .err .attributeError := by decide
  rw [h2] at hw
  exact nomatch hw

/-- …**and C4 is BLIND too** (vacuous: the target side is `ok`, so "both
    err" never fires). Only C3 catches the pre-fix silent-undefined bug —
    the machine-checked value of the JOIN. -/
theorem attr_missing_c4_blind :
    bothErrKind (evalTgt preFixAttrW witAttrMiss).res (evalRef witAttrMiss).res := by
  intro e f he _
  have h1 : (evalTgt preFixAttrW witAttrMiss).res = .ok .vundef := by decide
  rw [h1] at he
  exact nomatch he

/-- The combined pre-fix world fails the JOINT statement (via its C3
    defect — either of the other two witnesses would also do). -/
theorem preservation_preFix_fails : ¬ (C1 preFixW ∧ C3 preFixW ∧ C4 preFixW) := by
  rintro ⟨-, h3, -⟩
  have hocc := h3 witAttrMiss (by decide)
  simp only [errAgree] at hocc
  exact absurd hocc (by decide)

/-! ## The 3.14 oracle-bump discriminator — the 3.12-era zero-division
    wording as a refuted false world (anti-vacuity for the unified
    constant: the collapse of `refArithMsg`/`tgtArithMsg` to one text is
    what CPython 3.14 DID, not a weakening of the statement — and the
    message conjunct still separates worlds the kind face cannot) -/

/-- The PRE-`555f739c` (CPython-3.12-era) arith message layer — exactly the
    split wording `pyFloorDiv` shipped before the 3.14 oracle bump: float
    path `"float floor division by zero"`, int path `"integer division or
    modulo by zero"`. Kept ONLY as a false world: against the 3.14
    reference it raises the SAME kind with the WRONG message. -/
def legacy312ArithMsg (op : AOp) (a b : PVal) : Exc → String
  | .zeroDiv =>
    if isFloatP a || isFloatP b then "float floor division by zero"
    else "integer division or modulo by zero"
  | .typeError =>
    "unsupported operand type(s) for " ++ op.sym ++ ": '"
      ++ tgtPName a ++ "' and '" ++ tgtPName b ++ "'"
  | _ => ""

/-- The legacy-wording arith target: the SAME shipped independent recursion
    (`evalAtgt jsAL`), only the message layer rolled back — isolating the
    message channel as the sole difference. -/
def legacy312Arith (op : AOp) (a b : PVal) : POut :=
  match evalAtgt jsAL (embed op a b) [] with
  | .ok v => okV (ofPVal v)
  | .err k => errV k (legacy312ArithMsg op a b k)

def legacy312ArithW : OWorld := { shippedW with tArith := legacy312Arith }

/-- `1 // 0` — the C4-MESSAGE witness on the arith arm (int path). -/
def witFdivZero : OExp := .arith .ofdiv (.pint 1) (.pint 0)
/-- `2.0 // 0` — the float-path twin: BOTH 3.12 texts are dead, not one. -/
def witFdivZeroF : OExp := .arith .ofdiv (.pfloat 2) (.pint 0)

-- The witnesses are in-fragment; each side's outcome is pinned (the
-- legacy world's messages differ from BOTH the 3.14 reference and the
-- shipped world, on BOTH paths of the old split):
#guard fragOK witFdivZero = true
#guard fragOK witFdivZeroF = true
#guard evalRef witFdivZero = errV .zeroDiv "division by zero"
#guard evalTgt shippedW witFdivZero = errV .zeroDiv "division by zero"
#guard evalRef witFdivZeroF = errV .zeroDiv "division by zero"
#guard evalTgt shippedW witFdivZeroF = errV .zeroDiv "division by zero"
#guard evalTgt legacy312ArithW witFdivZero
    = errV .zeroDiv "integer division or modulo by zero"
#guard evalTgt legacy312ArithW witFdivZeroF
    = errV .zeroDiv "float floor division by zero"

/-- **Stub litmus, C4 MESSAGE axis on ARITH (the 3.14 bump).** A runtime
    still emitting the 3.12-era wording — the pre-bump shipped behavior —
    is refuted by the message conjunct at `1 // 0`: the unified
    `"division by zero"` is load-bearing statement content (the theorem
    MOVED when the runtime moved; the 3.12 wording can no longer leak). -/
theorem legacy312_arith_stub_fails : ¬ C4 legacy312ArithW := by
  intro h
  have hm := (h witFdivZero (by decide)).2
  exact absurd hm (by decide)

/-- **The kind face is BLIND at the `1 // 0` witness** — both worlds raise
    `ZeroDivisionError`, so kind-only C4 holds where message-carrying C4
    fails: after the 3.14 collapse the message conjunct still separates
    worlds the kind cannot (the arith analogue of
    `call_dict_kind_blind`). -/
theorem legacy312_arith_kind_blind :
    bothErrKind (evalTgt legacy312ArithW witFdivZero).res
      (evalRef witFdivZero).res := by
  intro e f he hf
  have h1 : (evalTgt legacy312ArithW witFdivZero).res = .err .zeroDiv := by
    decide
  have h2 : (evalRef witFdivZero).res = .err .zeroDiv := by decide
  rw [h1] at he; rw [h2] at hf
  injection he with he'; injection hf with hf'
  rw [← he', ← hf']

/-! ## Reference-faithfulness pins (F9) — live CPython 3.12.7, 2026-08-22,
    EXCEPT the zero-division message rows, re-probed on live CPython
    3.14.7, 2026-08-26 (the 3.14 unification — see the header's message
    layer). Each row is a probed `(kind, msg)` or value; the same rows
    were run on the shipped binary (68/68 exact @ `b28a36da`; the
    zero-division rows re-run 13/13 exact @ `555f739c`, see header). -/

-- call
#guard evalRef (.call "f" (.vint 5)) = errV .typeError "'int' object is not callable"
#guard evalRef (.call "f" (.vbool true)) = errV .typeError "'bool' object is not callable"
#guard evalRef (.call "f" (.vfloat 4)) = errV .typeError "'float' object is not callable"
#guard evalRef (.call "f" (.vstr "ab")) = errV .typeError "'str' object is not callable"
#guard evalRef (.call "f" (.vlist [1, 2])) = errV .typeError "'list' object is not callable"
#guard evalRef (.call "f" .vnone) = errV .typeError "'NoneType' object is not callable"
#guard evalRef (.call "f" (.vfun 7)) = okV (.vint 7)          -- (lambda: 7)()
-- iterate
#guard evalRef (.iter (.vint 5)) = errV .typeError "'int' object is not iterable"
#guard evalRef (.iter (.vbool true)) = errV .typeError "'bool' object is not iterable"
#guard evalRef (.iter (.vfloat 4)) = errV .typeError "'float' object is not iterable"
#guard evalRef (.iter .vnone) = errV .typeError "'NoneType' object is not iterable"
#guard evalRef (.iter (.vlist [1, 2, 3])) = okV (.vlist [1, 2, 3])
-- attribute: the Ok side (the errkind-review merge-blocker class — a model
-- that RAISES on these is F9-unfaithful)
#guard evalRef (.attr (.vint 5) "numerator") = okV (.vint 5)      -- (5).numerator = 5
#guard evalRef (.attr (.vint 5) "denominator") = okV (.vint 1)    -- (5).denominator = 1
#guard evalRef (.attr (.vint 5) "real") = okV (.vint 5)           -- (5).real = 5
#guard evalRef (.attr (.vint 5) "imag") = okV (.vint 0)           -- (5).imag = 0
#guard evalRef (.attr (.vbool true) "numerator") = okV (.vint 1)  -- True.numerator = 1
#guard evalRef (.attr (.vbool false) "numerator") = okV (.vint 0) -- False.numerator = 0
#guard evalRef (.attr (.vbool true) "real") = okV (.vint 1)       -- True.real = 1 (int!)
#guard evalRef (.attr (.vfloat 4) "real") = okV (.vfloat 4)       -- (4.0).real = 4.0
#guard evalRef (.attr (.vfloat 4) "imag") = okV (.vfloat 0)       -- (4.0).imag = 0.0
-- attribute: the Err side — incl. the DISCRIMINATING float-numerator
-- counter-case ((4).numerator succeeds, (4.0).numerator raises: only the
-- carrier brand separates them)
#guard evalRef (.attr (.vfloat 4) "numerator")
    = errV .attributeError "'float' object has no attribute 'numerator'"
#guard evalRef (.attr (.vint 5) "foo")
    = errV .attributeError "'int' object has no attribute 'foo'"
#guard evalRef (.attr (.vbool true) "foo")
    = errV .attributeError "'bool' object has no attribute 'foo'"
#guard evalRef (.attr (.vstr "ab") "numerator")
    = errV .attributeError "'str' object has no attribute 'numerator'"
#guard evalRef (.attr (.vlist [1]) "foo")
    = errV .attributeError "'list' object has no attribute 'foo'"
#guard evalRef (.attr (.vdict [("a", 1)]) "foo")
    = errV .attributeError "'dict' object has no attribute 'foo'"
#guard evalRef (.attr .vnone "foo")
    = errV .attributeError "'NoneType' object has no attribute 'foo'"
-- subscript
#guard evalRef (.subscr (.vlist [1, 2, 3]) (.vint 1)) = okV (.vint 2)
#guard evalRef (.subscr (.vlist [1, 2, 3]) (.vint (-1))) = okV (.vint 3)
#guard evalRef (.subscr (.vlist [1, 2, 3]) (.vint 99))
    = errV .indexError "list index out of range"
#guard evalRef (.subscr (.vlist [1, 2, 3]) (.vint (-4)))
    = errV .indexError "list index out of range"
#guard evalRef (.subscr (.vstr "abc") (.vint 1)) = okV (.vstr "b")
#guard evalRef (.subscr (.vstr "abc") (.vint (-1))) = okV (.vstr "c")
#guard evalRef (.subscr (.vstr "ab") (.vint 99))
    = errV .indexError "string index out of range"
#guard evalRef (.subscr (.vdict [("a", 1)]) (.vstr "a")) = okV (.vint 1)
#guard evalRef (.subscr (.vdict [("a", 1)]) (.vstr "z")) = errV .keyError "'z'"
#guard evalRef (.subscr (.vint 5) (.vint 0))
    = errV .typeError "'int' object is not subscriptable"
#guard evalRef (.subscr (.vbool true) (.vint 0))
    = errV .typeError "'bool' object is not subscriptable"
#guard evalRef (.subscr (.vfloat 4) (.vint 0))
    = errV .typeError "'float' object is not subscriptable"
#guard evalRef (.subscr .vnone (.vint 0))
    = errV .typeError "'NoneType' object is not subscriptable"
-- replication
#guard evalRef (.repl (.vlist [1, 2]) 3) = okV (.vlist [1, 2, 1, 2, 1, 2])
#guard evalRef (.repl (.vlist [1, 2]) 0) = okV (.vlist [])
#guard evalRef (.repl (.vlist [1, 2]) (-3)) = okV (.vlist [])
#guard evalRef (.repl (.vstr "ab") 3) = okV (.vstr "ababab")
#guard evalRef (.repl (.vstr "ab") (-3)) = okV (.vstr "")
#guard evalRef (.repl (.vstr "ab") (10 ^ 30))
    = errV .overflow "cannot fit 'int' into an index-sized integer"
-- arith (through the reused evalA — its own F9 pins live upstream; these pin
-- the MESSAGE layer + the whole-float post-B rows)
#guard evalRef (.arith .ofdiv (.pint 7) (.pint 2)) = okV (.vint 3)
#guard evalRef (.arith .ofdiv (.pint (-7)) (.pint 2)) = okV (.vint (-4))  -- floor
#guard evalRef (.arith .ofdiv (.pfloat (-7)) (.pint 2)) = okV (.vfloat (-4))  -- -7.0//2 = -4.0
#guard evalRef (.arith .osub (.pfloat 2) (.pint 1)) = okV (.vfloat 1)     -- 2.0-1 = 1.0
#guard evalRef (.arith .osub (.pint 5) (.pfloat 2)) = okV (.vfloat 3)     -- 5-2.0 = 3.0
#guard evalRef (.arith .ofdiv (.pint 1) (.pint 0))
    = errV .zeroDiv "division by zero"        -- 3.14 unified (was "integer division or modulo by zero")
#guard evalRef (.arith .ofdiv (.pfloat 2) (.pint 0))
    = errV .zeroDiv "division by zero"        -- 3.14 unified (was "float floor division by zero")
#guard evalRef (.arith .ofdiv (.pint 2) (.pfloat 0))
    = errV .zeroDiv "division by zero"        -- 3.14 unified (float divisor path)
#guard evalRef (.arith .osub (.pstr "a") (.pint 1))
    = errV .typeError "unsupported operand type(s) for -: 'str' and 'int'"
#guard evalRef (.arith .osub (.pint 1) (.pstr "a"))
    = errV .typeError "unsupported operand type(s) for -: 'int' and 'str'"
#guard evalRef (.arith .ofdiv (.pstr "a") (.pint 0))   -- type check precedes zero check
    = errV .typeError "unsupported operand type(s) for //: 'str' and 'int'"
#guard evalRef (.arith .ofdiv (.pfloat 4) (.pstr "a"))
    = errV .typeError "unsupported operand type(s) for //: 'float' and 'str'"

/-! ## SPOTs — client facts proved THROUGH the theorem, not by evaluating
    the target -/

/-- SPOT: the compiled `{'a': 1}['z']` raises `KeyError` with CPython's
    exact message — through `outcome_eq`. -/
example : evalTgt shippedW (.subscr (.vdict [("a", 1)]) (.vstr "z"))
    = errV .keyError "'z'" := by
  rw [outcome_eq _ (by decide)]; rfl

/-- SPOT: the compiled `(5).numerator` SUCCEEDS with `5` (the
    merge-blocker's Ok dual) — through `outcome_eq`. -/
example : evalTgt shippedW (.attr (.vint 5) "numerator") = okV (.vint 5) := by
  rw [outcome_eq _ (by decide)]; rfl

/-- SPOT (C4 through the corollary): the compiled `2.0 // 0` raises
    `ZeroDivisionError` with CPython 3.14's unified `"division by zero"`
    (the float path shares the int path's text since the bump) — extracted
    from `preservationC1C3C4`'s C4 conjunct, not by evaluating the
    target. -/
example : (evalTgt shippedW (.arith .ofdiv (.pfloat 2) (.pint 0))).msg
    = "division by zero" := by
  rw [(preservationC1C3C4.2.2 (.arith .ofdiv (.pfloat 2) (.pint 0)) (by decide)).2]
  rfl

/-! ## Axiom pins (per-declaration; ⊆ {propext, Classical.choice, Quot.sound},
    no native-decide escape) -/

/-- info: 'C1C3C4Outcome.outcome_eq' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms outcome_eq

/-- info: 'C1C3C4Outcome.preservationC1C3C4' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservationC1C3C4

/-- info: 'C1C3C4Outcome.joint_faces_iff_outcome_eq' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms joint_faces_iff_outcome_eq

/-- info: 'C1C3C4Outcome.pout_faces_iff_eq' depends on axioms: [propext] -/
#guard_msgs in
#print axioms pout_faces_iff_eq

/-- info: 'C1C3C4Outcome.overflow_rep_stub_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms overflow_rep_stub_fails

/-- info: 'C1C3C4Outcome.call_dict_stub_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms call_dict_stub_fails

/-- info: 'C1C3C4Outcome.call_dict_kind_blind' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms call_dict_kind_blind

/-- info: 'C1C3C4Outcome.attr_missing_stub_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms attr_missing_stub_fails

/-- info: 'C1C3C4Outcome.attr_missing_c1_blind' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms attr_missing_c1_blind

/-- info: 'C1C3C4Outcome.attr_missing_c4_blind' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms attr_missing_c4_blind

/-- info: 'C1C3C4Outcome.preservation_preFix_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms preservation_preFix_fails

/-- info: 'C1C3C4Outcome.legacy312_arith_stub_fails' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms legacy312_arith_stub_fails

/-- info: 'C1C3C4Outcome.legacy312_arith_kind_blind' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms legacy312_arith_kind_blind

end C1C3C4Outcome
