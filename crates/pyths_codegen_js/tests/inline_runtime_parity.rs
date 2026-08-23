//! WB-20 root fix — the inline-vs-package runtime PARITY GATE.
//!
//! `pyths run`/`bundle`/`test` embed an INLINE runtime assembled in emit.rs;
//! `pyths compile` imports the PACKAGE runtime (runtime/src/*.js). Fixes
//! repeatedly landed in one copy only (WB-20's pyDictGet MobX read landed in
//! the package but not inline; pyContains had the same un-fixed sibling;
//! earlier: pyFormatSpec, the exception classes, popitem, …). This gate
//! closes the CLASS three ways:
//!
//!   A. FREEZE — the set of hand-inlined helpers that shadow a package
//!      top-level name is frozen. A NEW hand-inline of a package name fails
//!      this test: new shared helpers MUST flow through the #170 extraction
//!      (`append_extracted_helpers`), which copies the canonical package
//!      source verbatim — identical by construction. The migrated names
//!      (pyDictGet/pyContains/pyUpdate/pyDictItems/pyDictSetdefault/pyClear/
//!      pyCopy/__pyEffect/pySetItem/pyDelItem/…, see MIGRATED_TO_EXTRACTION)
//!      must STAY extracted.
//!
//!   B. BEHAVIORAL BATTERY — every drift-prone frozen helper runs a battery
//!      of real invocations under BOTH runtimes in one node process; any
//!      outcome difference (value, error kind, mutation) fails. Reintroduced
//!      drift in a battery-covered helper can no longer ship silently.
//!
//!      BATTERY-COVERAGE RULE (0.2.2 hold blocker 2): when a fix changes a
//!      helper's behavior, the battery MUST cover the cases the fix changed —
//!      ERROR paths and KINDS included, not just happy paths. The pyDelItem/
//!      pySetItem drift (1b28bae5 fixed error kinds in the package only; the
//!      stale inline copy shipped IndexError-for-TypeError and a SILENT tuple
//!      splice) sailed through a GREEN battery because its entries exercised
//!      only happy paths. "Drift can't recur silently" holds only if the
//!      battery discriminates the drift. Concretely:
//!        * every battery entry should probe each shape-dispatch branch AND
//!          the error kind each branch raises (`t()` serializes error NAMES,
//!          so a wrong-kind regression fails the diff);
//!        * every helper in MIGRATED_TO_EXTRACTION must HAVE a battery entry
//!          (enforced by `migrated_helpers_have_battery_entries` below) — the
//!          entry pins package behavior itself, so a package-side regression
//!          is caught even though inline == package by construction.
//!
//!   C. WB-20 ABSOLUTE SEMANTICS — pyDictGet and pyContains must fire a host
//!      read-trap (`get`) even for a MISSING key, in BOTH runtimes (the MobX
//!      observable-tracking contract). Asserted absolutely, not just parity.
//!
//! Behavioral parts skip when node is unavailable; the freeze (A) is pure
//! Rust and always runs.

use std::collections::BTreeSet;
use std::process::Command;

// ─────────────────────────────────────────────────────────────────────────────
// Shared parsing: top-level declaration names, the same line-based rule as
// emit.rs's package_runtime_slices (column-0 `[export] [async] function/
// class/const/let NAME`).
// ─────────────────────────────────────────────────────────────────────────────

fn decl_names(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in src.lines() {
        let mut rest = line;
        if let Some(r) = rest.strip_prefix("export ") {
            rest = r;
        }
        if let Some(r) = rest.strip_prefix("async ") {
            rest = r;
        }
        for kw in ["function*", "function", "class", "const", "let"] {
            if let Some(r) = rest.strip_prefix(kw) {
                let name: String = r
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if !name.is_empty() {
                    out.insert(name);
                }
                break;
            }
        }
    }
    out
}

const EXTRACTION_MARKER: &str = "// --- Extracted from package runtime (#170 fallback) ---";

fn full_inline_runtime() -> String {
    pyths_codegen_js::inline_runtime_for_test(pyths_codegen_js::EMITTABLE_RUNTIME_SYMBOLS)
}

fn package_source() -> String {
    let mut pkg = String::new();
    for s in [
        include_str!("../../../runtime/src/runtime.js"),
        include_str!("../../../runtime/src/operators.js"),
        include_str!("../../../runtime/src/types.js"),
        include_str!("../../../runtime/src/classes.js"),
    ] {
        pkg.push_str(s);
        pkg.push('\n');
    }
    pkg
}

// ─────────────────────────────────────────────────────────────────────────────
// A. The freeze. Sorted; snapshot as of the WB-20 gate landing. Shrinking it
// (migrating a helper to extraction) is ALWAYS welcome — delete the name
// here. GROWING it is the drift class this gate exists to prevent: hand-
// inlining a package helper requires (1) a reason extraction cannot serve,
// (2) a battery entry below, (3) adding the name here.
// ─────────────────────────────────────────────────────────────────────────────

const FROZEN_HAND_INLINED: &[&str] = &[
    "PyComplex",
    "PyDict",
    "PyObject",
    "__MAX_SAFE_BIG",
    "__NP_RANGES",
    "__PY_MIXIN",
    "__TUPKEY",
    "__arithNumOk",
    // E2 (#466): the binary-op operand-type authority + the replication-
    // count rule are hand-inlined for the same reason as the rest of the
    // arith family (self-contained plain-Error construction via
    // __arithTypeErr instead of the package exception classes); their
    // battery entries below compare full error MESSAGES, not just kinds.
    // (__arithNoneGuard was deleted in both copies — subsumed by
    // __binOpTypeError's generic branch.)
    "__binOpTypeError",
    "__bitIntOk",
    "__boolNum",
    "__complexFmtPart",
    "__complexRepr",
    "__cpNonPrintable",
    "__encodeTupleKey",
    "__fits32",
    "__fixedHalfEven",
    "__intBin",
    "__isFloat",
    "__isPlainObj",
    "__mulRepCount",
    "__norm",
    "__numeric",
    "__objId",
    "__objIdCounter",
    "__objIds",
    // #469: __opTypeName (operators.js) and the inline __bitTypeName were
    // DELETED — both were incomplete copies of the type-name computation
    // (no function/class/plain-object arms → leaked 'Function'/'Object';
    // no __pyfloat__ arm → leaked 'PyFloat'). Every operand/bit message now
    // routes through __pyTypeName, extracted from the canonical runtime.js
    // (see MIGRATED_TO_EXTRACTION).
    "__pyAddrMap",
    "__pyAddrNext",
    "__pyC3",
    "__pyClass",
    "__pyClassAttr",
    "__pyClassCall",
    "__pyDictWrite",
    "__pyIsInstance",
    "__pyKey",
    "__pyKeyObjs",
    "__pyObjAddr",
    "__pyPropKey",
    "__pyRangeIter",
    "__pyRangeLen",
    "__pyRangeNorm",
    "__pyRangeUseBig",
    "__pySuper",
    "__reqArithNum",
    "__reqBitInt",
    "__reqNum",
    "__roundBigNeg",
    "__seqLt",
    "__toBig",
    "__toComplex",
    "parseFormatSpec",
    "pyAbs",
    "pyAdd",
    "pyBitAnd",
    "pyBitNot",
    "pyBitOr",
    "pyBitXor",
    "pyChr",
    "pyComplex",
    "pyDictKeys",
    "pyDictMerge",
    "pyDictPopitem",
    "pyDictValues",
    "pyDiv",
    "pyDivmod",
    "pyEnumerate",
    "pyFixed",
    "pyFloat",
    "pyFloorDiv",
    "pyForIter",
    "pyFormatDynamic",
    "pyFormatFloat",
    "pyGe",
    "pyGetItem",
    "pyGt",
    "pyInt",
    "pyIter",
    "pyLe",
    "pyLen",
    "pyLt",
    "pyMap",
    "pyMod",
    "pyMul",
    "pyNeg",
    "pyNext",
    "pyOrd",
    "pyPow",
    "pyPrint",
    "pyProperty",
    "pyRange",
    "pyRepr",
    "pyReversed",
    "pyRound",
    "pySeq",
    "pySorted",
    "pyStr",
    "pySub",
    "pySum",
    "pyTuple",
    "pyTupleOf",
    "pyZip",
];

/// Names whose hand-inlined copies were DELETED in favor of #170 extraction —
/// they must never re-appear in the hand region.
const MIGRATED_TO_EXTRACTION: &[&str] = &[
    // Bytes-completeness root fix: the hand-inlined pyBool had no bytes/
    // bytearray branch (#457 stayed broken under `pyths run` after the
    // package fix) -- truthiness and its four dependents now extract from
    // the canonical types.js.
    "pyBool",
    "pyAnd",
    "pyOr",
    "pyAny",
    "pyAll",
    "__pyEffect",
    "pyDelItem",
    "pyPop",
    "pySetItem",
    "__pyEffectArgs",
    "pyClear",
    "pyContains",
    "pyCopy",
    "pyDictGet",
    "pyDictItems",
    "pyDictSetdefault",
    "pyFormatSpec",
    "pyUpdate",
    // #469: THE ONE type-name source. Already extracted for the #467 sites
    // (pySeq/pyGetItem/pyLen/…); now ALSO the operand (+) and bit (&, ~)
    // message paths reference it — the incomplete __opTypeName/__bitTypeName
    // copies were deleted, so a hand copy re-appearing would re-open the
    // three-computers divergence.
    "__pyTypeName",
];

#[test]
fn freeze_no_new_hand_inlined_package_helpers() {
    let rt = full_inline_runtime();
    let hand = rt
        .split(EXTRACTION_MARKER)
        .next()
        .expect("inline runtime text");
    let hand_names = decl_names(hand);
    let pkg_names = decl_names(&package_source());
    let shared: BTreeSet<&str> = hand_names
        .intersection(&pkg_names)
        .map(|s| s.as_str())
        .collect();
    let frozen: BTreeSet<&str> = FROZEN_HAND_INLINED.iter().copied().collect();

    let new_names: Vec<&&str> = shared.difference(&frozen).collect();
    assert!(
        new_names.is_empty(),
        "NEW hand-inlined copies of package runtime helpers: {new_names:?}.\n\
         This is the inline-drift class (WB-20). Do NOT hand-inline a package \
         helper — delete the inline copy and let the #170 extraction \
         (append_extracted_helpers) pull the canonical package source. If a \
         hand-inline is truly unavoidable, add a behavioral battery entry in \
         inline_runtime_parity.rs AND extend FROZEN_HAND_INLINED."
    );
    let gone: Vec<&&str> = frozen.difference(&shared).collect();
    assert!(
        gone.is_empty(),
        "frozen hand-inlined helpers no longer found in the hand region: \
         {gone:?}. If migrated to extraction (good!), remove them from \
         FROZEN_HAND_INLINED and add them to MIGRATED_TO_EXTRACTION."
    );
}

fn battery_entry_names() -> Vec<&'static str> {
    let mut names: Vec<&str> = Vec::new();
    for line in BATTERY_JS.lines() {
        let l = line.trim_start();
        if let Some(colon) = l.find(": (H) =>") {
            names.push(&l[..colon]);
        }
    }
    names
}

/// Battery-coverage rule (0.2.2 hold blocker 2): every helper migrated to
/// #170 extraction must keep a behavioral battery entry. Inline == package by
/// construction for these, so the entry's job is to PIN the package behavior
/// (a package-side regression fails the diff against the recorded shape of
/// the other runtime the moment they diverge again — e.g. if a helper is ever
/// hand-inlined anew, the entry immediately discriminates drift). Migrating a
/// helper without battery coverage re-opens the hole this gate closed.
#[test]
fn migrated_helpers_have_battery_entries() {
    let names = battery_entry_names();
    for name in MIGRATED_TO_EXTRACTION {
        assert!(
            names.contains(name),
            "`{name}` was migrated to #170 extraction but has NO behavioral \
             battery entry in BATTERY_JS. Add one covering each dispatch \
             branch and the error KIND it raises (battery-coverage rule)."
        );
    }
}

#[test]
fn migrated_helpers_stay_extracted() {
    let rt = full_inline_runtime();
    let hand = rt
        .split(EXTRACTION_MARKER)
        .next()
        .expect("inline runtime text");
    let hand_names = decl_names(hand);
    let full_names = decl_names(&rt);
    for name in MIGRATED_TO_EXTRACTION {
        assert!(
            !hand_names.contains(*name),
            "`{name}` has a hand-inlined copy again — it was migrated to \
             #170 extraction exactly because its hand copy drifted (WB-20 \
             family). Delete the hand copy."
        );
        assert!(
            full_names.contains(*name),
            "`{name}` is missing from the full inline runtime — extraction \
             failed to pull it from the package sources."
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Numeric value-boundary SINGLE-AUTHORITY freeze (#460/#461/#464, #38).
// The 4-form × op behavioral matrix lives in
// runtime/src/numeric_boundary.test.mjs; this test pins the STRUCTURAL
// invariant behind it: int/float classification, safe-range promotion, and
// float box/unbox are each defined in exactly ONE place per runtime copy,
// with the same classifier semantics, and no stdlib module re-decides
// representation with a local coercion copy (the #461 "Cannot mix BigInt"
// class was exactly such copies in stdlib/math.js).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn numeric_boundary_single_authority() {
    // 1. Exactly one definition of each authority helper in the package
    //    runtime (they all live in operators.js).
    let pkg = package_source();
    for (pat, what) in [
        ("const __isFloat", "int/float classifier"),
        ("function __intBin", "exact-int promotion"),
        ("const __pyF", "float box rule"),
        ("const __pyJs", "native-sink unbox rule"),
        ("const __reqNum", "int→float coercion"),
        ("const __norm", "BigInt demotion"),
    ] {
        let n = pkg.matches(pat).count();
        assert_eq!(
            n, 1,
            "the package runtime must define the {what} (`{pat}`) exactly \
             once — found {n}. A second copy is the drift class the \
             value-boundary authority (#38) exists to prevent."
        );
    }
    // The classifier is the AUTHORITY form: brand OR non-integer Number.
    // (The pre-fix `!Number.isSafeInteger` form classified inbound unsafe
    // integer Numbers as float and rounded them — #464.)
    assert!(
        pkg.contains("(typeof x === \"number\" && !Number.isInteger(x))"),
        "package __isFloat lost the authority classifier form"
    );

    // 2. The inline mirror carries the SAME single authority.
    let rt = full_inline_runtime();
    assert_eq!(
        rt.matches("const __isFloat").count(),
        1,
        "inline runtime must define __isFloat exactly once"
    );
    assert_eq!(
        rt.matches("function __intBin").count(),
        1,
        "inline runtime must define __intBin exactly once"
    );
    assert!(
        rt.contains("(typeof x === \"number\" && !Number.isInteger(x))"),
        "inline __isFloat drifted from the authority classifier form"
    );
    assert!(
        rt.contains("Number.isSafeInteger(a) && Number.isSafeInteger(b)"),
        "inline __intBin lost the unsafe-operand promotion (#460/#464)"
    );

    // 3. No stdlib module may carry a local numeric-coercion copy — every
    //    boundary must import the operators.js authority. (`__n` in math.js
    //    was the local copy behind the #461 throws; it is now an import
    //    alias of __reqNum.)
    let stdlib_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/src/stdlib");
    let mut scanned = 0usize;
    for entry in std::fs::read_dir(&stdlib_dir).expect("stdlib dir") {
        let path = entry.expect("dir entry").path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".js") || name.ends_with(".test.mjs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("read stdlib file");
        scanned += 1;
        for bad in [
            "Number.isSafeInteger", // classification/promotion is operators.js's job
            "const __isFloat",
            "function __intBin",
            "const __n = (x) => (typeof x === \"bigint\"", // the old local coercion copy
        ] {
            assert!(
                !src.contains(bad),
                "stdlib/{name} contains a local numeric-boundary decision \
                 (`{bad}`) — route through the operators.js authority \
                 (import __reqNum/__toBig/__norm/__pyF or the pyXxx ops) \
                 instead of re-deciding representation locally (#38)."
            );
        }
    }
    assert!(
        scanned >= 10,
        "stdlib scan looks wrong: only {scanned} files"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Type-name SINGLE-AUTHORITY freeze (#469) — same pattern as
// numeric_boundary_single_authority above. Three type-name computers used to
// coexist and DISAGREE: __pyTypeName (runtime.js, complete), __opTypeName
// (operators.js + an emit.rs inline mirror — no function/class/plain-object
// arms → leaked 'Function'/'Object'), and the inline __bitTypeName (no
// __pyfloat__ arm → leaked 'PyFloat'). This test pins the STRUCTURAL
// invariant behind the cross-authority behavioral matrix (#469 rows in
// runtime/src/operand_type.test.mjs and the battery below): EXACTLY ONE
// function computes a Python type name, in the package runtime, in the
// emit.rs inline templates, and in the assembled inline runtime — so every
// error-message path agrees by construction and a future value form cannot
// drift between operand/bit/iteration messages.
// One-liner rationale: a single source is what brings the type-name family
// under the two_copy_drift_refuted internal-consistency theorem (it covers
// copies of ONE helper — it only engages once there IS one); this guard
// keeps a third computer from re-opening the gap the Wave-3 meta-review
// found (#469).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn type_name_single_authority() {
    let pkg = package_source();
    let emit_src = include_str!("../src/emit.rs");
    let rt = full_inline_runtime();

    // 1. __pyTypeName is defined EXACTLY once in the package runtime, with
    //    the canonical pyType-backed body (not an ad-hoc re-computation).
    assert_eq!(
        pkg.matches("function __pyTypeName").count(),
        1,
        "the package runtime must define __pyTypeName (THE type-name source) \
         exactly once"
    );
    assert!(
        pkg.contains("return (t && (t.__name__ || t.name)) || typeof o;"),
        "package __pyTypeName lost its canonical pyType-backed body"
    );

    // 2. No second computer survives or reappears — as a definition — in the
    //    package sources, the emit.rs inline templates, or the assembled
    //    inline runtime. (Substring, not decl_names(): the emit.rs inline JS
    //    lives inside Rust string literals where a definition is not at
    //    column 0 of the Rust file.)
    for name in ["__opTypeName", "__bitTypeName"] {
        for kw in ["function ", "const ", "let ", "var "] {
            let def = format!("{kw}{name}");
            assert!(
                !pkg.contains(&def),
                "`{def}` reappeared in the package runtime — #469 deleted it; \
                 route the message through __pyTypeName instead"
            );
            assert!(
                !emit_src.contains(&def),
                "`{def}` reappeared in emit.rs — #469 deleted the inline \
                 mirror; reference __pyTypeName (extracted on use) instead"
            );
            assert!(
                !rt.contains(&def),
                "`{def}` is defined in the assembled inline runtime — a \
                 second type-name computer is the #469 divergence class"
            );
        }
    }

    // 3. No independent constructor-name fallback logic outside __pyTypeName:
    //    `|| "object"` after a constructor-name read was the discriminating
    //    tail of BOTH deleted copies (the exact code that leaked 'Function'/
    //    'Object'). If a delegator must exist for import-structure reasons it
    //    must be a pass-through to __pyTypeName, never carry this logic.
    for (src, what) in [(pkg.as_str(), "package runtime"), (emit_src, "emit.rs")] {
        assert!(
            !src.contains(r#"(c && (c.__name__ || c.name)) || "object""#),
            "{what} contains an independent constructor-name type-name \
             fallback — the #469 leak pattern; use __pyTypeName"
        );
    }

    // 4. The assembled inline runtime carries EXACTLY one __pyTypeName
    //    definition (the extracted canonical copy) — extraction dedupe holds.
    assert_eq!(
        rt.matches("function __pyTypeName").count(),
        1,
        "the assembled inline runtime must define __pyTypeName exactly once \
         (extracted from the canonical runtime.js)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// B + C. Behavioral battery under node.
// ─────────────────────────────────────────────────────────────────────────────

/// Battery entries: `NAME: (H) => [outcome, ...]` — executed against the
/// inline helpers and the package helpers; the serialized outcome arrays
/// must match exactly. `t(fn)` runs fn catching Python-kind errors.
const BATTERY_JS: &str = r#"
function ser(v, d = 0) {
    if (d > 4) return "deep";
    if (v === undefined) return "undef";
    if (v === null) return "null";
    const t = typeof v;
    if (t === "number") return Object.is(v, -0) ? "-0" : String(v);
    if (t === "bigint") return v + "n";
    if (t === "string") return JSON.stringify(v);
    if (t === "boolean") return String(v);
    if (t === "function") return "[fn]";
    if (Array.isArray(v)) return (v.__pytuple__ ? "tup[" : "arr[") + v.map((x) => ser(x, d + 1)).join(",") + "]";
    if (v instanceof Map) return "map{" + [...v.entries()].map(([k, x]) => ser(k, d + 1) + ":" + ser(x, d + 1)).join(",") + "}";
    if (v instanceof Set) return "set{" + [...v].map((x) => ser(x, d + 1)).join(",") + "}";
    if (v instanceof Error) return "err(" + v.name + ")";
    if (t === "object") return "obj{" + Object.keys(v).map((k) => k + ":" + ser(v[k], d + 1)).join(",") + "}";
    return String(v);
}
function t(f) { try { return ser(f()); } catch (e) { return "throw(" + ((e && e.name) || "Error") + ")"; } }
// #467: message-inclusive probe — the type-NAME in runtime error messages is
// part of the contract ("'float' object is not iterable", not "'number' …").
// Use for entries whose fix changed the MESSAGE, which name-only t() cannot
// discriminate.
function tm(f) { try { return ser(f()); } catch (e) { return "throw(" + ((e && e.name) || "Error") + ": " + ((e && e.message) || "") + ")"; } }
// E2 (#466): message-level probe — operand-type errors must carry the SAME
// CPython message in both runtimes, not just the same kind.
function em(f) { try { return "ok " + ser(f()); } catch (e) { return ((e && e.name) || "Error") + ": " + e.message; } }

const battery = {
    // ---- dict family (the WB-6/WB-20 drift epicenter) ----
    pyDictKeys: (H) => [t(() => H.pyDictKeys({ a: 1, b: 2 })), t(() => H.pyDictKeys(new Map([["x", 1]]))), t(() => H.pyDictKeys(null))],
    pyDictValues: (H) => [t(() => H.pyDictValues({ a: 1 })), t(() => H.pyDictValues(new Map([["x", 9]]))), t(() => H.pyDictValues(null))],
    pyDictPopitem: (H) => { const d = { a: 1, b: 2 }; return [t(() => H.pyDictPopitem(d)), ser(d), t(() => H.pyDictPopitem({}))]; },
    pyDictMerge: (H) => [t(() => H.pyDictMerge({ a: 1 }, { b: 2, a: 3 }))],
    // pyPop: list/dict happy paths PLUS the receiver-class cases item 3
    // changed — set.pop() is a real method (arbitrary element / empty
    // KeyError / arg TypeError), and None/int/str/bytes receivers raise
    // AttributeError, not the dict-path KeyError.
    pyPop: (H) => {
        const xs = [1, 2, 3]; const d = { k: 9 };
        const out = [t(() => H.pyPop(xs)), ser(xs), t(() => H.pyPop(d, "k")), ser(d), t(() => H.pyPop({}, "nope"))];
        const st = new Set([7, 8]);
        out.push(t(() => H.pyPop(st)), ser(st));            // arbitrary elem removed
        out.push(t(() => H.pyPop(new Set())));               // KeyError (empty)
        out.push(t(() => H.pyPop(new Set([1]), 1)));         // TypeError (arg given)
        out.push(t(() => H.pyPop(null)));                     // AttributeError
        out.push(t(() => H.pyPop(5)));                        // AttributeError
        out.push(t(() => H.pyPop("ab")));                     // AttributeError
        out.push(t(() => H.pyPop(new Uint8Array([65]))));     // AttributeError (bytes)
        out.push(t(() => H.pyPop([], 0)));                    // IndexError (empty list)
        out.push(t(() => H.pyPop([1], 5)));                   // IndexError (oob)
        // bytearray dispatches to its own pop (mutator surface)
        out.push(t(() => { const ba = H.pyBytearrayOf([97, 98]); const v = H.pyPop(ba); return [v, ba.__repr__()]; }));
        out.push(t(() => H.pyPop(H.pyBytearrayOf([97]), 5))); // IndexError (oob)
        return out;
    },
    // #467: tm() probes pin the not-subscriptable MESSAGE type-name.
    pyGetItem: (H) => [t(() => H.pyGetItem([10, 20, 30], -1)), t(() => H.pyGetItem("abc", -1)), t(() => H.pyGetItem({ k: 1 }, "k")), t(() => H.pyGetItem([1], 5)), tm(() => H.pyGetItem(8.5, 0)), tm(() => H.pyGetItem(7, 0)), tm(() => H.pyGetItem(9007199254740993n, 0)), tm(() => H.pyGetItem(true, 0))],
    // pySetItem/pyDelItem: happy paths PLUS the error KINDS the 1b28bae5 fix
    // changed (battery-coverage rule — the pre-fix inline copy raised
    // IndexError for a non-integer list index and SILENTLY SPLICED a tuple;
    // these entries fail on that shape).
    pySetItem: (H) => {
        const d = {}; const xs = [1, 2];
        const out = [t(() => H.pySetItem(d, "a", 1)), ser(d), t(() => H.pySetItem(xs, -1, 9)), ser(xs)];
        // non-integer index TYPE → TypeError (NOT IndexError); message carries the index type
        out.push(t(() => H.pySetItem([1, 2], null, 5)));
        out.push(t(() => H.pySetItem([1, 2], 1.5, 5)));
        out.push(t(() => H.pySetItem([1, 2], "k", 5)));
        // genuinely out-of-range integer stays IndexError
        out.push(t(() => H.pySetItem([1, 2], 99, 5)));
        // tuple is immutable → TypeError, and the tuple must NOT mutate
        const tp = H.pyTuple(1, 2);
        out.push(t(() => H.pySetItem(tp, 0, 9)), ser(tp));
        // None receiver → TypeError
        out.push(t(() => H.pySetItem(null, 0, 1)));
        return out;
    },
    pyDelItem: (H) => {
        const d = { a: 1 }; const xs = [1, 2, 3];
        const out = [t(() => H.pyDelItem(d, "a")), ser(d), t(() => H.pyDelItem(xs, 0)), ser(xs)];
        // non-integer index TYPE → TypeError (NOT "list assignment index out of range")
        out.push(t(() => H.pyDelItem([1, 2], null)));
        out.push(t(() => H.pyDelItem([1, 2], 2.5)));
        out.push(t(() => H.pyDelItem([1, 2], "x")));
        // out-of-range integer stays IndexError
        out.push(t(() => H.pyDelItem([1, 2], 99)));
        // tuple → TypeError AND no silent splice (the pre-fix inline spliced)
        const tp = H.pyTuple(1, 2, 3);
        out.push(t(() => H.pyDelItem(tp, 0)), ser(tp));
        // non-subscriptable / immutable receivers → TypeError (not KeyError)
        out.push(t(() => H.pyDelItem(5, 0)));
        out.push(t(() => H.pyDelItem("ab", 0)));
        out.push(t(() => H.pyDelItem(new Set([1]), 0)));
        out.push(t(() => H.pyDelItem(new Uint8Array([65, 66]), 0)));
        // missing dict key stays KeyError; None receiver → TypeError
        out.push(t(() => H.pyDelItem({}, "missing")));
        out.push(t(() => H.pyDelItem(null, 0)));
        return out;
    },
    // migrated helper (battery-coverage rule): pins package format-spec
    // behavior — float precision, zero-pad width, numeric code on a str.
    pyFormatSpec: (H) => [t(() => H.pyFormatSpec(3.14159, { precision: 2, type: "f" })), t(() => H.pyFormatSpec(42, { width: 6, zero: true, type: "d" })), t(() => H.pyFormatSpec("hi", { type: "f" }))],
    // ---- extracted dict family: pins behavior against the package ----
    pyDictGet: (H) => [t(() => H.pyDictGet({ k: 1 }, "k")), t(() => H.pyDictGet({}, "m", "dflt")), t(() => H.pyDictGet(new Map([["k", 2]]), "k")), t(() => H.pyDictGet(null, "k", "d"))],
    pyDictItems: (H) => {
        const q = Object.create({ items() { return "user items"; } });
        return [t(() => H.pyDictItems({ a: 1 })), t(() => H.pyDictItems(q)), t(() => H.pyDictItems(null))];
    },
    pyDictSetdefault: (H) => { const d = { a: 1 }; return [t(() => H.pyDictSetdefault(d, "b", 2)), ser(d), t(() => H.pyDictSetdefault(d, "a", 99))]; },
    pyUpdate: (H) => {
        const d = {}; const m = new Map(); const s = new Set([1]);
        return [t(() => H.pyUpdate(d, [["k", "v"]])), ser(d),
                t(() => H.pyUpdate(m, [["a", 1]])), ser(m),
                t(() => H.pyUpdate(s, [2, 3])), ser(s),
                t(() => H.pyUpdate({}, [["only-one"]]))];
    },
    pyClear: (H) => {
        const d = { a: 1 }; const xs = [1]; const u = Object.create({ clear() { return "user clear"; } });
        return [t(() => H.pyClear(d)), ser(d), t(() => H.pyClear(xs)), ser(xs), t(() => H.pyClear(u))];
    },
    pyCopy: (H) => { const xs = [1, 2]; const c = H.pyCopy(xs); xs.push(3); return [ser(c), t(() => H.pyCopy({ a: 1 })), t(() => H.pyCopy(5))]; },
    pyContains: (H) => [t(() => H.pyContains([1, 2], 2)), t(() => H.pyContains({ k: 1 }, "k")), t(() => H.pyContains("abc", "b")), t(() => H.pyContains("abc", 1)), t(() => H.pyContains(new Set([1]), true)), t(() => H.pyContains(null, 1))],
    // ---- truthiness family (bytes-completeness migration): pyBool and its
    // four dependents extract from types.js; the entries pin every dispatch
    // branch INCLUDING the bytes/bytearray one the old hand copy lacked ----
    pyBool: (H) => [t(() => H.pyBool(null)), t(() => H.pyBool(undefined)), t(() => H.pyBool(0)), t(() => H.pyBool(2)), t(() => H.pyBool("")), t(() => H.pyBool("x")), t(() => H.pyBool([])), t(() => H.pyBool([0])), t(() => H.pyBool(new Set())), t(() => H.pyBool(new Map([["a", 1]]))), t(() => H.pyBool({})), t(() => H.pyBool({ k: 1 })), t(() => H.pyBool(H.pyBytesOf(undefined))), t(() => H.pyBool(H.pyBytesOf([97]))), t(() => H.pyBool(H.pyBytearrayOf(undefined))), t(() => H.pyBool(H.pyBytearrayOf([97]))), t(() => H.pyBool({ __bool__() { return false; } })), t(() => H.pyBool({ __len__() { return 0; } }))],
    pyAnd: (H) => [t(() => H.pyAnd([], () => "rhs")), t(() => H.pyAnd([1], () => "rhs")), t(() => H.pyAnd(H.pyBytesOf(undefined), () => "rhs"))],
    pyOr: (H) => [t(() => H.pyOr([], () => "rhs")), t(() => H.pyOr("x", () => "rhs")), t(() => H.pyOr(H.pyBytesOf(undefined), () => "rhs"))],
    pyAny: (H) => [t(() => H.pyAny([[], "", 0])), t(() => H.pyAny([[], [0]])), t(() => H.pyAny([])), t(() => H.pyAny([H.pyBytesOf(undefined)])), t(() => H.pyAny([H.pyBytesOf([97])]))],
    pyAll: (H) => [t(() => H.pyAll([1, "a"])), t(() => H.pyAll([1, []])), t(() => H.pyAll([])), t(() => H.pyAll([H.pyBytesOf([97]), H.pyBytearrayOf([98])])), t(() => H.pyAll([H.pyBytesOf(undefined)]))],
    // ---- effect wrappers (WF-1) ----
    __pyEffect: (H) => {
        const wrapped = H.__pyEffect(() => null);
        const wrapped2 = H.__pyEffect(() => () => 7);
        return [ser(wrapped()), ser(typeof wrapped2())];
    },
    __pyEffectArgs: (H) => [ser(H.__pyEffectArgs(() => null, [1, 2]).length), ser(typeof H.__pyEffectArgs(() => null)[0]), ser(H.__pyEffectArgs("notfn", [1])[0])],
    // ---- bytes/bytearray constructors (item 5: give other entries typed
    // bytes-likes to probe with; __repr__ discriminates bytes vs bytearray) ----
    pyBytesOf: (H) => [t(() => H.pyBytesOf([97, 98]).__repr__()), t(() => H.pyBytesOf(undefined).__repr__()), t(() => H.pyBytesOf(3).__repr__())],
    pyBytearrayOf: (H) => [t(() => H.pyBytearrayOf([97]).__repr__()), t(() => { const ba = H.pyBytearrayOf([97]); ba.append(98); return ba.__repr__(); })],
    // ---- equality (item 5: bytes/bytearray content equality, cross-type) ----
    pyEq: (H) => [t(() => H.pyEq(H.pyBytesOf([97]), H.pyBytesOf([97]))), t(() => H.pyEq(H.pyBytearrayOf([97]), H.pyBytesOf([97]))), t(() => H.pyEq(H.pyBytesOf([97]), H.pyBytesOf([98]))), t(() => H.pyEq(H.pyBytesOf([97]), "a")), t(() => H.pyEq([1, [2]], [1, [2]])), t(() => H.pyEq({ a: 1 }, { a: 1 }))],
    // ---- arithmetic ----
    // pyAdd/pyMul bytes cases (item 5): the result TYPE must follow the
    // bytes-like operand — bytearray + bytes -> bytearray, bytes + bytearray
    // -> bytes, bytearray * n -> bytearray (repr discriminates the type).
    pyAdd: (H) => [t(() => H.pyAdd(1, 2)), t(() => H.pyAdd("a", "b")), t(() => H.pyAdd([1], [2])), t(() => H.pyAdd(1, "x")), t(() => H.pyAdd(null, 1)), t(() => H.pyAdd(H.pyBytearrayOf([97]), H.pyBytesOf([98])).__repr__()), t(() => H.pyAdd(H.pyBytesOf([97]), H.pyBytearrayOf([98])).__repr__()), t(() => H.pyAdd(H.pyBytesOf([97]), H.pyBytesOf([98])).__repr__()),
        // E2 (#466) operand-type rows — message-level parity via em():
        em(() => H.pyAdd(H.pyBytesOf([97, 98, 99]), 1)),          // can't concat int to bytes (#466)
        em(() => H.pyAdd(H.pyBytearrayOf([97]), "x")),            // can't concat str to bytearray
        em(() => H.pyAdd("a", 1)),                                 // can only concatenate str (not "int") to str
        em(() => H.pyAdd("a", H.pyBytesOf([98]))),                 // … (not "bytes") …
        em(() => H.pyAdd([1], 1)),                                 // can only concatenate list (not "int") to list
        em(() => H.pyAdd(1, "a")),                                 // unsupported +: 'int' and 'str'
        em(() => H.pyAdd(new Set([1]), new Set([2]))),             // unsupported +: 'set' and 'set'
        em(() => H.pyAdd(null, 1)),                                // unsupported +: 'NoneType' and 'int' (#322 kept)
        // #469 cross-authority rows — the forms the deleted __opTypeName
        // copies mis-named ('Function'/'Function'/'Object'):
        em(() => { function fx() {} return H.pyAdd(fx, 1); }),     // 'function' and 'int'
        em(() => { class Kls {} return H.pyAdd(Kls, 1); }),        // 'type' and 'int'
        em(() => H.pyAdd({ a: 1 }, 1)),                            // 'dict' and 'int'
        em(() => H.pyAdd(H.__pyF(2), []))],                        // 'float' and 'list' (boxed 2.0)
    pySub: (H) => [t(() => H.pySub(5, 3)), t(() => H.pySub(5.5, 0.5)), t(() => H.pySub("a", 1))],
    // ---- numeric value-boundary authority (#460/#461/#464): the 4 forms
    // (Number-int, unsafe-Number-int, BigInt-int, boxed float) must behave
    // identically under BOTH runtimes — values via pyRepr (box-aware) and
    // representation via typeof. ----
    __intBin: (H) => [
        t(() => H.pyRepr(H.pyAdd(9007199254740992, 1))),          // inbound 2**53 + 1 → exact (#464)
        t(() => typeof H.pyAdd(9007199254740992, 1)),              // → bigint (promotion, #460)
        t(() => H.pyRepr(H.pySub(9007199254740993n, 2))),          // demote back to Number
        t(() => typeof H.pySub(9007199254740993n, 2)),
        t(() => H.pyRepr(H.pyMul(9007199254740992, 9007199254740992))),
        t(() => H.pyRepr(H.pyMod(9007199254740990, 9007199254740991))), // intermediate-overflow guard
        t(() => H.pyRepr(H.pyFloorDiv(9007199254740992, 3))),
        t(() => H.pyRepr(H.pyPow(9007199254740992, 2))),
    ],
    __isFloat: (H) => [
        t(() => H.pyRepr(H.pyAdd(9007199254740993n, H.__pyF(8)))), // bigint + boxed float, no throw (#461)
        t(() => H.pyRepr(H.pyMul(9007199254740993n, 2.5))),        // bigint × native float
        t(() => H.pyRepr(H.pyDiv(9007199254740992, 4))),           // int/int → float
        t(() => H.pyRepr(H.pyMod(2.5, 9007199254740992))),         // float % huge int → 2.5 (no re-mod rounding)
        t(() => H.pyRepr(9007199254740992)),                        // raw unsafe int Number reprs as int
    ],
    __pyF: (H) => [
        t(() => H.pyRepr(H.__pyF(8))),                              // box iff integer-valued
        t(() => typeof H.__pyF(2.5)),                               // non-integer stays native
        t(() => H.__pyF(8).__pyfloat__ === true),
    ],
    pyPos: (H) => [
        t(() => H.pyRepr(H.pyPos(9007199254740993n))),             // +int identity at any magnitude
        t(() => H.pyPos(7)),
        t(() => H.pyRepr(H.pyPos(H.__pyF(8)))),                     // +float keeps the box
        t(() => H.pyPos(true)),
    ],
    pyMul: (H) => [t(() => H.pyMul(3, 4)), t(() => H.pyMul("ab", 2)), t(() => H.pyMul([0], 3)), t(() => H.pyMul(H.pyBytearrayOf([97]), 2).__repr__()), t(() => H.pyMul(2, H.pyBytearrayOf([98])).__repr__()), t(() => H.pyMul(H.pyBytesOf([97]), 2).__repr__()), t(() => H.pyMul(H.pyBytesOf([97]), -1).__repr__()),
        // E2 (#466) operand-type rows — message-level parity via em():
        em(() => H.pyMul(H.pyBytesOf([97]), 2.5)),                 // can't multiply sequence by non-int of type 'float'
        em(() => H.pyMul(H.pyBytesOf([97]), H.pyBytesOf([98]))),   // … of type 'bytes'
        em(() => H.pyMul("ab", 2.5)),                              // … of type 'float' (was silent "abab")
        em(() => H.pyMul("ab", "c")),                              // … of type 'str' (was SyntaxError crash)
        em(() => H.pyMul([1], 2.5)),                               // … of type 'float' (was silent 3-elem list)
        em(() => H.pyMul(2.5, "ab")),                              // … of type 'float' (right-hand sequence)
        em(() => H.pyMul(new Set([1]), 2)),                        // unsupported *: 'set' and 'int'
        // #471: index-sized replication count — OverflowError, not a JS
        // RangeError/hang from the replication loop:
        em(() => H.pyMul([1], 10n ** 30n)),                        // OverflowError: cannot fit 'int' into an index-sized integer
        em(() => H.pyMul("ab", 10n ** 30n)),                       // same via str * huge
        em(() => H.pyMul(10n ** 30n, [1])),                        // right-hand sequence, huge left count
        em(() => H.pyMul(H.pyBytesOf([97]), 10n ** 30n)),          // same via bytes * huge
        em(() => H.pyMul([1], -(10n ** 30n))),                     // negative out-of-range count overflows too
        // valid counts that were crashes/misvalues on base:
        em(() => H.pyMul("ab", true)),                             // "ab" (bool count)
        em(() => H.pyMul("ab", -1)),                               // "" (was RangeError)
        em(() => H.pyMul(H.pyBytesOf([97]), true).__repr__()),     // b'a' (was SyntaxError)
        em(() => H.pyMul(true, 2)),                                // 2 (bool·int via preamble)
        em(() => { const tp = H.pyTuple(1, 2); const r = H.pyMul(tp, 2); return [r.__pytuple__ === true, r]; })], // tuple*int stays tuple
    // E2 (#466): the operand-type authority itself — every branch, message-level.
    __binOpTypeError: (H) => [
        em(() => H.__binOpTypeError("+", H.pyBytesOf([97]), 1)),       // bytes concat branch
        em(() => H.__binOpTypeError("+", H.pyBytearrayOf([97]), "x")), // bytearray concat branch
        em(() => H.__binOpTypeError("+", "a", null)),                  // str concat branch
        em(() => H.__binOpTypeError("+", [1], "x")),                   // list concat branch
        em(() => H.__binOpTypeError("+", H.pyTuple(1), 1)),            // tuple concat branch
        em(() => H.__binOpTypeError("*", "ab", 2.5)),                  // seq-left multiply branch
        em(() => H.__binOpTypeError("*", null, [1])),                  // seq-right multiply branch
        em(() => H.__binOpTypeError("-", "a", 1)),                     // generic branch
        em(() => H.__binOpTypeError("+", 1, "a"))],                    // generic branch on +
    // E2 (#466): replication-count rule — int/bool admitted, float/boxed/str not.
    __mulRepCount: (H) => [t(() => H.__mulRepCount(3)), t(() => H.__mulRepCount(-1)), t(() => H.__mulRepCount(true)), t(() => H.__mulRepCount(false)), t(() => H.__mulRepCount(2n)), t(() => H.__mulRepCount(2.5)), t(() => H.__mulRepCount(H.__pyF(2))), t(() => H.__mulRepCount(null)), t(() => H.__mulRepCount("2"))],
    pyDiv: (H) => [t(() => H.pyDiv(7, 2)), t(() => H.pyDiv(1, 0))],
    pyFloorDiv: (H) => [t(() => H.pyFloorDiv(-7, 2)), t(() => H.pyFloorDiv(7, 2)), t(() => H.pyFloorDiv(1, 0))],
    pyMod: (H) => [t(() => H.pyMod(-7, 2)), t(() => H.pyMod(7, -2)),
        // em() carries the MESSAGE (battery-coverage rule): a drift back to
        // "integer division or modulo by zero" is the same ZeroDivisionError
        // NAME, so t() alone would not catch it.
        em(() => H.pyMod(1, 0)),                                   // integer modulo by zero (CPython 3.12 `%` wording)
        em(() => H.pyMod(2.5, 0))],                                // float modulo by zero
    pyPow: (H) => [t(() => H.pyPow(2, 10)), t(() => H.pyPow(2, -1)), t(() => H.pyPow(4, 0.5)),
        // Bug 3 (enriched differential): zero to a negative power is
        // CPython's ZeroDivisionError with its exact message, never the
        // OverflowError the raw Infinity used to mislabel.
        em(() => H.pyPow(0, -1)),
        em(() => H.pyPow(0.5, -1, 1)),                             // fctx float path unaffected
        t(() => H.pyPow(0, 0)), t(() => H.pyPow(0, 2))],
    // Bug 2 (enriched differential): a negative shift count raises
    // ValueError("negative shift count") — JS BigInt shifts silently shift
    // the other way. Happy paths pin arbitrary-precision behavior.
    pyShiftLeft: (H) => [t(() => H.pyShiftLeft(1, 3)), t(() => H.pyShiftLeft(1, 99)), em(() => H.pyShiftLeft(1, -1)), em(() => H.pyShiftLeft(1.5, 1))],
    pyShiftRight: (H) => [t(() => H.pyShiftRight(8, 2)), t(() => H.pyShiftRight(-8, 1)), em(() => H.pyShiftRight(8, -2))],
    // Bug 1 (aliasing soundness): the in-place aug-assign protocol helpers —
    // mutable targets are mutated IN PLACE and returned (identity preserved,
    // serialized via the `same` flag), immutables fall back to the binary
    // value helper (fresh object).
    pyIAdd: (H) => { const xs = [1, 2]; const r = H.pyIAdd(xs, [5]); return [ser(r), String(r === xs), t(() => H.pyIAdd("x", "y")), t(() => H.pyIAdd(1, 2)), em(() => H.pyIAdd([1], 1))]; },
    pyISub: (H) => { const s = new Set([1, 2]); const r = H.pyISub(s, new Set([2])); return [ser(r), String(r === s), t(() => H.pyISub(5, 2)), em(() => H.pyISub([1], [1]))]; },
    pyIMul: (H) => { const xs = [1, 2]; const r = H.pyIMul(xs, 2); const e = H.pyIMul(xs.slice(), 0); return [ser(r), String(r === xs), ser(e), t(() => H.pyIMul(3, 4)), em(() => H.pyIMul([1], 1.5))]; },
    pyIBitOr: (H) => {
        const s = new Set([1]); const rs = H.pyIBitOr(s, new Set([2]));
        const d = { a: 1 }; const rd = H.pyIBitOr(d, { b: 2 });
        const m = new Map([["x", 1]]); const rm = H.pyIBitOr(m, { y: 2 });
        return [ser(rs), String(rs === s), ser(rd), String(rd === d), ser(rm), String(rm === m), t(() => H.pyIBitOr(4, 1)), em(() => H.pyIBitOr(1.5, 1))];
    },
    pyIBitAnd: (H) => { const s = new Set([1, 2, 3]); const r = H.pyIBitAnd(s, new Set([2, 3, 4])); return [ser(r), String(r === s), t(() => H.pyIBitAnd(6, 3)), em(() => H.pyIBitAnd(1.5, 1))]; },
    // frozenset brand: pyFrozensetOf stamps __pyfrozen__, and every in-place
    // set helper must then REBIND (fresh set, frozen original untouched)
    // instead of mutating — AND the rebound result is RE-BRANDED frozen, so
    // a SECOND in-place op also rebinds (the aliasing-soundness contract
    // must hold across repeated ops, not just the first).
    pyFrozensetOf: (H) => {
        const f = H.pyFrozensetOf([1, 2, 3]);
        const and = H.pyIBitAnd(f, new Set([2, 3, 4]));
        const or = H.pyIBitOr(f, new Set([9]));
        const sub = H.pyISub(f, new Set([1]));
        const xor = H.pyIBitXor(f, new Set([3, 9]));
        // second-op chain: |= on the |= result must rebind again.
        const or2 = H.pyIBitOr(or, new Set([10]));
        return [
            String(f.__pyfrozen__ === true),
            String(and === f), ser([...and].sort()), String(and.__pyfrozen__ === true),
            String(or === f), ser([...or].sort()), String(or.__pyfrozen__ === true),
            String(sub === f), ser([...sub].sort()), String(sub.__pyfrozen__ === true),
            String(xor === f), ser([...xor].sort()), String(xor.__pyfrozen__ === true),
            String(or2 === or), ser([...or].sort()), ser([...or2].sort()),
            ser([...f].sort()), // original untouched by all four
        ];
    },
    pyIBitXor: (H) => { const s = new Set([1, 2]); const r = H.pyIBitXor(s, new Set([2, 3])); return [ser(r), String(r === s), t(() => H.pyIBitXor(6, 3)), em(() => H.pyIBitXor(1.5, 1))]; },
    pyDivmod: (H) => [t(() => H.pyDivmod(7, 2)), t(() => H.pyDivmod(-7, 2))],
    pyAbs: (H) => [t(() => H.pyAbs(-5)), t(() => H.pyAbs(-5.5)), t(() => H.pyAbs("x"))],
    pyNeg: (H) => [t(() => H.pyNeg(5)), t(() => H.pyNeg(-0.0))],
    pyRound: (H) => [t(() => H.pyRound(2.5)), t(() => H.pyRound(3.5)), t(() => H.pyRound(-2.5)), t(() => H.pyRound(2.675, 2))],
    pyBitAnd: (H) => [t(() => H.pyBitAnd(6, 3)), t(() => H.pyBitAnd(true, 1)), t(() => H.pyBitAnd(1.5, 1)),
        // E2 (#466): bytes operand must be named 'bytes' in BOTH copies.
        em(() => H.pyBitAnd(H.pyBytesOf([97]), 1)),
        // #469 cross-authority rows — the bit path names the SAME CPython
        // type as the operand path (the deleted inline __bitTypeName said
        // 'PyFloat' for a boxed 2.0; both deleted copies said 'Function'/
        // 'Object' for function/class/plain-object):
        em(() => H.pyBitAnd(H.__pyF(2), 1)),                       // 'float' and 'int'
        em(() => { function fx() {} return H.pyBitAnd(fx, 1); }),  // 'function' and 'int'
        em(() => { class Kls {} return H.pyBitAnd(Kls, 1); }),     // 'type' and 'int'
        em(() => H.pyBitAnd({ a: 1 }, 1))],                        // 'dict' and 'int'
    pyBitOr: (H) => [t(() => H.pyBitOr(4, 1))],
    pyBitXor: (H) => [t(() => H.pyBitXor(6, 3))],
    pyBitNot: (H) => [t(() => H.pyBitNot(5)),
        // #469: unary ~ names boxed 2.0 'float' and a function 'function'.
        em(() => H.pyBitNot(H.__pyF(2))),
        em(() => { function fx() {} return H.pyBitNot(fx); })],
    // #469: THE ONE type-name source, probed directly across the full value
    // model (function / class / plain-object / boxed float are the forms the
    // deleted copies mis-named; the rest are the regression sweep).
    __pyTypeName: (H) => { function fx() {} class Kls {} return [t(() => H.__pyTypeName(fx)), t(() => H.__pyTypeName(Kls)), t(() => H.__pyTypeName({ a: 1 })), t(() => H.__pyTypeName(H.__pyF(2))), t(() => H.__pyTypeName(7)), t(() => H.__pyTypeName(9007199254740993n)), t(() => H.__pyTypeName(true)), t(() => H.__pyTypeName("ab")), t(() => H.__pyTypeName([1])), t(() => H.__pyTypeName(H.pyTuple(1))), t(() => H.__pyTypeName(new Set([1]))), t(() => H.__pyTypeName(new Map())), t(() => H.__pyTypeName(H.pyBytesOf([97]))), t(() => H.__pyTypeName(H.pyBytearrayOf([97]))), t(() => H.__pyTypeName(null)), t(() => H.__pyTypeName(2.5))]; },
    // ---- comparisons ----
    pyLt: (H) => [t(() => H.pyLt(1, 2)), t(() => H.pyLt("a", "b")), t(() => H.pyLt([1, 2], [1, 3])), t(() => H.pyLt(1, "a"))],
    pyLe: (H) => [t(() => H.pyLe(2, 2)), t(() => H.pyLe([1], [1]))],
    pyGt: (H) => [t(() => H.pyGt(2, 1)), t(() => H.pyGt("b", "a"))],
    pyGe: (H) => [t(() => H.pyGe(2, 2)), t(() => H.pyGe([2], [1, 9]))],
    // ---- conversions / predicates ----
    pyInt: (H) => [t(() => H.pyInt("42")), t(() => H.pyInt(3.9)), t(() => H.pyInt(-3.9)), t(() => H.pyInt("3.5")), t(() => H.pyInt("ff", 16))],
    pyFloat: (H) => [t(() => H.pyFloat("3.5")), t(() => H.pyFloat("nan")), t(() => H.pyFloat("x"))],
    pyBool: (H) => [t(() => H.pyBool(0)), t(() => H.pyBool("")), t(() => H.pyBool([])), t(() => H.pyBool({})), t(() => H.pyBool(null)), t(() => H.pyBool(new Map()))],
    // #467: the trailing tm() probes pin the MESSAGE type-name ('float'/'int'/
    // 'bool', not JS 'number'/'boolean') — name-only t() cannot see that drift.
    pyLen: (H) => [t(() => H.pyLen([1, 2])), t(() => H.pyLen("ab")), t(() => H.pyLen({ a: 1 })), t(() => H.pyLen(new Map())), t(() => H.pyLen(new Set([1]))), t(() => H.pyLen(5)), tm(() => H.pyLen(8.5)), tm(() => H.pyLen(true))],
    pyStr: (H) => [t(() => H.pyStr(1.5)), t(() => H.pyStr(true)), t(() => H.pyStr(null)), t(() => H.pyStr([1, "x"]))],
    pyRepr: (H) => [t(() => H.pyRepr("a'b")), t(() => H.pyRepr(1.0)), t(() => H.pyRepr([1, "x", true, null])), t(() => H.pyRepr(new Map([["k", 1]])))],
    pyFormatFloat: (H) => [t(() => H.pyFormatFloat(0.1)), t(() => H.pyFormatFloat(-0.0)), t(() => H.pyFormatFloat(1e20)), t(() => H.pyFormatFloat(NaN)), t(() => H.pyFormatFloat(Infinity))],
    pyFormatDynamic: (H) => [t(() => H.pyFormatDynamic(3.14159, ".2f")), t(() => H.pyFormatDynamic(42, "06d")), t(() => H.pyFormatDynamic("hi", ">5"))],
    pyChr: (H) => [t(() => H.pyChr(97)), t(() => H.pyChr(128512))],
    pyOrd: (H) => [t(() => H.pyOrd("a")), t(() => H.pyOrd("ab"))],
    // ---- iteration / builtins ----
    pySum: (H) => [t(() => H.pySum([1, 2, 3])), t(() => H.pySum([0.1, 0.2]))],
    pyAll: (H) => [t(() => H.pyAll([1, 2])), t(() => H.pyAll([1, 0]))],
    pyAny: (H) => [t(() => H.pyAny([0, 1])), t(() => H.pyAny([]))],
    pyAnd: (H) => [t(() => H.pyAnd(0, () => 5)), t(() => H.pyAnd(1, () => 5))],
    pyOr: (H) => [t(() => H.pyOr(0, () => 5)), t(() => H.pyOr(2, () => 5))],
    pySorted: (H) => [t(() => H.pySorted([3, 1, 2])), t(() => H.pySorted(["b", "a"]))],
    pyRange: (H) => [t(() => [...H.pyRange(3)]), t(() => [...H.pyRange(5, 1, -2)])],
    // #467: tm() probes pin the not-iterable MESSAGE type-name.
    pyIter: (H) => [t(() => { const it = H.pyIter([7]); return typeof it.next; }), tm(() => H.pyIter(8.5)), tm(() => H.pyIter(7)), tm(() => H.pyIter(true))],
    pyNext: (H) => [t(() => H.pyNext(H.pyIter([7]))), t(() => H.pyNext(H.pyIter([])))],
    pyEnumerate: (H) => [t(() => [...H.pyEnumerate(["a", "b"])]), t(() => [...H.pyEnumerate(["a"], 5)])],
    pyZip: (H) => [t(() => [...H.pyZip([1, 2], ["a", "b", "c"])])],
    pyMap: (H) => [t(() => [...H.pyMap((x) => x * 2, [1, 2])])],
    pyReversed: (H) => [t(() => [...H.pyReversed([1, 2, 3])]), t(() => [...H.pyReversed("ab")])],
    // #467: tm() probes pin the not-iterable MESSAGE type-name (and the
    // boxed-float / primitive raise pyForIter previously deferred to for..of).
    pyForIter: (H) => [t(() => [...H.pyForIter({ a: 1, b: 2 })]), t(() => [...H.pyForIter(new Map([["k", 1]]))]), tm(() => [...H.pyForIter(8.5)]), tm(() => [...H.pyForIter(7)]), tm(() => [...H.pyForIter(true)])],
    // #467: pySeq (the comprehension path — the literal #467 repro).
    pySeq: (H) => [t(() => H.pySeq([1, 2])), t(() => H.pySeq("ab")), t(() => H.pySeq({ a: 1 })), tm(() => H.pySeq(8.5)), tm(() => H.pySeq(7)), tm(() => H.pySeq(true)), tm(() => H.pySeq(null))],
    pyTuple: (H) => [t(() => H.pyTuple(1, 2)), t(() => H.pyTuple(1, 2).__pytuple__ === true)],
};

// C. WB-20 absolute semantics: a MISSING-key probe must fire the host `get`
// trap in BOTH runtimes (MobX dependency tracking).
function wb20(H, label) {
    const log = [];
    const target = {};
    const proxy = new Proxy(target, {
        get(t_, k, r) { if (typeof k === "string") log.push("get:" + k); return Reflect.get(t_, k, r); },
        has(t_, k) { if (typeof k === "string") log.push("has:" + k); return Reflect.has(t_, k); },
    });
    const got = H.pyDictGet(proxy, "missing", "dflt");
    if (got !== "dflt") return label + ": pyDictGet default wrong: " + ser(got);
    if (!log.includes("get:missing")) return label + ": pyDictGet did NOT read d[k] for a missing key (WB-20 regressed). log=" + JSON.stringify(log);
    log.length = 0;
    const has = H.pyContains(proxy, "absent");
    if (has !== false) return label + ": pyContains wrong: " + ser(has);
    if (!log.includes("get:absent")) return label + ": pyContains did NOT read container[k] for a missing key (WB-20 sibling regressed). log=" + JSON.stringify(log);
    return null;
}
"#;

fn node_run_gate(script: &str) -> Option<(bool, String, String)> {
    let dir = std::env::temp_dir().join(format!("pyths_parity_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    pyths_runtime::materialize_runtime_package(&dir).expect("materialize runtime");
    let path = dir.join("gate.mjs");
    std::fs::write(&path, script).unwrap();
    let out = Command::new("node").arg(&path).output().ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    Some((out.status.success(), stdout, stderr))
}

#[test]
fn behavioral_parity_battery() {
    let rt = full_inline_runtime();
    // Names the node script needs back from the inline text: every battery
    // key (declared in BATTERY_JS as `NAME:`) — extracted or hand-inlined,
    // all are defined somewhere in the full inline runtime.
    let names = battery_entry_names();
    assert!(
        names.len() >= 50,
        "battery parse found only {} entries — the `NAME: (H) =>` convention changed?",
        names.len()
    );
    let script = format!(
        "import * as pkg from \"pyths-runtime\";\n\
         const inlineNS = new Function({rt_json} + \"\\n; return {{ {names} }};\")();\n\
         {battery}\n\
         let bad = 0;\n\
         for (const [name, run] of Object.entries(battery)) {{\n\
             if (typeof pkg[name] === \"undefined\") {{ console.log(\"MISSING-IN-PKG \" + name); bad++; continue; }}\n\
             if (typeof inlineNS[name] === \"undefined\") {{ console.log(\"MISSING-IN-INLINE \" + name); bad++; continue; }}\n\
             let a, b;\n\
             try {{ a = JSON.stringify(run(inlineNS)); }} catch (e) {{ a = \"BATTERY-THREW \" + e; }}\n\
             try {{ b = JSON.stringify(run(pkg)); }} catch (e) {{ b = \"BATTERY-THREW \" + e; }}\n\
             if (a !== b) {{ console.log(\"DRIFT \" + name + \"\\n  inline:  \" + a + \"\\n  package: \" + b); bad++; }}\n\
         }}\n\
         const w1 = wb20(inlineNS, \"inline\");\n\
         if (w1) {{ console.log(\"WB20 \" + w1); bad++; }}\n\
         const w2 = wb20(pkg, \"package\");\n\
         if (w2) {{ console.log(\"WB20 \" + w2); bad++; }}\n\
         if (bad) {{ console.log(\"PARITY FAILED: \" + bad); process.exit(1); }}\n\
         console.log(\"PARITY OK: \" + Object.keys(battery).length + \" helpers\");\n",
        rt_json = js_string(&rt),
        names = names.join(", "),
        battery = BATTERY_JS,
    );
    let Some((ok, stdout, stderr)) = node_run_gate(&script) else {
        return; // node unavailable → skip (freeze tests still gate)
    };
    assert!(
        ok && stdout.contains("PARITY OK"),
        "inline-vs-package parity gate failed:\n{stdout}\n{stderr}"
    );
}

/// Minimal JS string literal fallback (no serde_json dependency needed).
fn js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
