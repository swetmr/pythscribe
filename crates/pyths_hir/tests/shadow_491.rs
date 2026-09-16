//! #491 call-time builtin-shadow resolution — the WASM ADMISSION half
//! (`crates/pyths_codegen_wasm/SHADOW_BINDING_DESIGN.md` §2–§3). A WASM body bakes
//! ONE resolution of every name it calls, so a function is routed only when that
//! provably equals CPython's call-time binding; otherwise it DEMOTES to the
//! cell-aware JS path with a reason (loud in `--verbose`), never a silent value.
//!
//! Every test is a paired control: the property holds with the fix and the assert
//! FAILS on revert (the base authority saw only `def` + `from math import`, had no
//! early-call rule, no local-shadow rule, and a call collector that skipped `try`
//! bodies). The end-to-end values are pinned by `tests/differential/shadow_491`.

use pyths_hir::wasm_analysis::{
    analyze_module, final_module_bindings, ShadowBinding, GLOBAL_WRITE_INDEX,
};

fn analyze(src: &str) -> pyths_hir::WasmAnalysis {
    let module = pyths_parser::parse(src).expect("parse");
    analyze_module(&module)
}

fn reason(a: &pyths_hir::WasmAnalysis, name: &str) -> String {
    a.rejected
        .iter()
        .filter(|(n, _)| n == name)
        .map(|(_, r)| r.clone())
        .collect::<Vec<_>>()
        .join(" | ")
}

const USE_ABS: &str = "def use(x: int) -> int:\n    return abs(x)\n";

/// S4: EVERY module-level binder form of a builtin name is seen by the census as an
/// `Other(kind)` binder, and a WASM candidate calling that name demotes with the
/// binder kind in its reason. (Base: only `def`/`from math import` were binders —
/// `use` was admitted and baked the builtin `abs`.)
#[test]
fn s4_every_other_binder_kind_demotes_the_caller() {
    let forms: &[(&str, &str)] = &[
        ("abs = len\n", "assignment"),
        ("abs = lambda z: z * 3\n", "assignment"),
        ("abs, other = len, max\n", "assignment"),
        ("abs: int = 5\n", "assignment"),
        ("abs = 1\nabs += 1\n", "assignment"),
        ("print((abs := len))\n", "assignment"),
        ("import math as abs\n", "import"),
        ("import abs\n", "import"),
        ("from os import path as abs\n", "import"),
        ("class abs:\n    pass\n", "class"),
        ("for abs in [len]:\n    pass\n", "for-target"),
        ("with open('f') as abs:\n    pass\n", "with-target"),
        (
            "try:\n    pass\nexcept ValueError as abs:\n    pass\n",
            "except-target",
        ),
        ("match 1:\n    case abs:\n        pass\n", "match-capture"),
        ("if len(''):\n    abs = len\n", "assignment"),
        (
            "def rebind():\n    global abs\n    abs = len\n",
            "global write",
        ),
        ("def abs(x: int) -> int:\n    return x\ndel abs\n", "del"),
    ];
    for (form, kind) in forms {
        let src = format!("{USE_ABS}{form}");
        let module = pyths_parser::parse(&src).expect("parse");
        let bindings = final_module_bindings(&module);
        match bindings.get("abs") {
            Some((_, ShadowBinding::Other(k))) => assert_eq!(
                k, kind,
                "binder kind for form {form:?}: expected {kind}, got {k}"
            ),
            other => panic!("form {form:?}: expected Other({kind}), got {other:?}"),
        }
        let a = analyze(&src);
        assert!(
            !a.eligible.contains_key("use"),
            "form {form:?}: `use` must DEMOTE (calls a name rebound by a {kind}); eligible={:?}",
            a.eligible.keys().collect::<Vec<_>>()
        );
        let r = reason(&a, "use");
        assert!(
            r.contains(kind),
            "form {form:?}: demotion reason must name the binder kind {kind:?}: {r}"
        );
    }
}

/// S4: a `global` write wins over every source-ordered binder (recorded at the
/// sentinel index) — even a LATER `def abs` does not make `abs` a routable def.
#[test]
fn s4_global_write_beats_a_later_def() {
    let src = "def rebind():\n    global abs\n    abs = len\n\
               def abs(x: int) -> int:\n    return x * 7\n\
               def use(x: int) -> int:\n    return abs(x)\n";
    let module = pyths_parser::parse(src).expect("parse");
    let b = final_module_bindings(&module);
    assert_eq!(
        b.get("abs"),
        Some(&(GLOBAL_WRITE_INDEX, ShadowBinding::Other("global write")))
    );
    let a = analyze(src);
    assert!(
        !a.eligible.contains_key("abs"),
        "the def is not the final binder"
    );
    assert!(!a.eligible.contains_key("use"), "the caller demotes");
}

/// #491 BLOCKER-2 (census twin): a `global N` + a VALUE binding of N directly in a CLASS
/// BODY is a module WRITE (`GLOBAL_WRITE_INDEX`) — plain / inside an `if` / augmented /
/// tuple / `del`, and with the class nested in a function or another class — so a WASM
/// caller of N demotes (the JS emitter's `collect_global_declared_deep` counts the same
/// set, so the cell exists). A bare `global N` in a class body with no binding of N, a
/// method-LOCAL `N = v` under a class-body `global N`, and a plain class attribute are NOT
/// module writes. (Base: the class-body walk passed `in_def = None` through, so a
/// class-body `global` write was invisible — `use` was admitted and baked the builtin.)
#[test]
fn s4_class_body_global_write_is_a_module_binder() {
    let writes = [
        "class C:\n    global abs\n    abs = 7\n",
        "class C:\n    global abs\n    if True:\n        abs = 7\n",
        "class C:\n    global abs\n    abs += 1\n",
        "class C:\n    global abs, k\n    abs, k = 7, 1\n",
        "class C:\n    global abs\n    del abs\n",
        "def f():\n    class C:\n        global abs\n        abs = 7\n",
        "class O:\n    class I:\n        global abs\n        abs = 7\n",
    ];
    for form in writes {
        let src = format!("{USE_ABS}{form}");
        let module = pyths_parser::parse(&src).expect("parse");
        assert_eq!(
            final_module_bindings(&module).get("abs"),
            Some(&(GLOBAL_WRITE_INDEX, ShadowBinding::Other("global write"))),
            "form {form:?}: a class-body `global abs` write is a module binder"
        );
        let a = analyze(&src);
        assert!(
            !a.eligible.contains_key("use"),
            "form {form:?}: `use` must DEMOTE (calls a name written by a class-body global); eligible={:?}",
            a.eligible.keys().collect::<Vec<_>>()
        );
    }
    let not_writes = [
        "class C:\n    global abs\n    y = abs(-4)\n",
        "class C:\n    global abs\n    def m(self):\n        abs = 7\n",
        "class C:\n    abs = 7\n",
    ];
    for form in not_writes {
        let src = format!("{USE_ABS}{form}");
        let module = pyths_parser::parse(&src).expect("parse");
        assert!(
            !matches!(
                final_module_bindings(&module).get("abs"),
                Some((GLOBAL_WRITE_INDEX, _))
            ),
            "form {form:?}: NOT a module write"
        );
        let a = analyze(&src);
        assert!(
            a.eligible.contains_key("use"),
            "form {form:?}: `use` stays WASM; rejected={:?}",
            a.rejected
        );
    }
}

/// B3: a WASM-eligible caller that module-level code CALLS before the shadowing
/// def demotes (its call-time binding differs between the two calls); the same
/// program with the call AFTER the def keeps both functions on WASM.
#[test]
fn b3_early_module_call_demotes_but_late_call_stays_wasm() {
    let early = "def use_abs(x: int) -> int:\n    return abs(x)\n\
                 print(use_abs(-5))\n\
                 def abs(x: int) -> int:\n    return x * 7\n\
                 print(use_abs(-5))\n";
    let a = analyze(early);
    assert!(a.eligible.contains_key("abs"), "the final def is routable");
    assert!(
        !a.eligible.contains_key("use_abs"),
        "use_abs runs BEFORE `abs` is rebound: must demote; eligible={:?}",
        a.eligible.keys().collect::<Vec<_>>()
    );
    let r = reason(&a, "use_abs");
    assert!(
        r.contains("BEFORE") && r.contains("call-time"),
        "reason: {r}"
    );

    let late = "def use_abs(x: int) -> int:\n    return abs(x)\n\
                def abs(x: int) -> int:\n    return x * 7\n\
                print(use_abs(-5))\n";
    let a = analyze(late);
    assert!(
        a.eligible.contains_key("use_abs") && a.eligible.contains_key("abs"),
        "a call only AFTER the final binder keeps WASM: rejected={:?}",
        a.rejected
    );
}

/// B3 is TRANSITIVE over the module def graph and over WASM call chains:
/// `print(g(-5))` before `def abs` where `g -> f -> abs` demotes both `f` (direct
/// caller of `abs`, reachable early) and `g` (its WASM caller, via the fixpoint).
/// Def-time parts (a default value) and a class body also execute at their
/// statement's position.
#[test]
fn b3_reachability_is_transitive_and_covers_def_time_and_class_bodies() {
    let chain = "def f(x: int) -> int:\n    return abs(x)\n\
                 def g(x: int) -> int:\n    return f(x) + 1\n\
                 print(g(-5))\n\
                 def abs(x: int) -> int:\n    return x * 7\n";
    let a = analyze(chain);
    assert!(!a.eligible.contains_key("f"), "f: {:?}", a.rejected);
    assert!(!a.eligible.contains_key("g"), "g: {:?}", a.rejected);
    assert!(a.eligible.contains_key("abs"));

    let via_default = "def f(x: int) -> int:\n    return abs(x)\n\
                       def k(v: int = f(1)) -> int:\n    return v\n\
                       def abs(x: int) -> int:\n    return x * 7\n";
    let a = analyze(via_default);
    assert!(
        !a.eligible.contains_key("f"),
        "a default value evaluates at def time (before `abs`): {:?}",
        a.eligible.keys().collect::<Vec<_>>()
    );

    let via_class = "def f(x: int) -> int:\n    return abs(x)\n\
                     class C:\n    v = f(1)\n\
                     def abs(x: int) -> int:\n    return x * 7\n";
    let a = analyze(via_class);
    assert!(
        !a.eligible.contains_key("f"),
        "a class body executes at its statement: {:?}",
        a.eligible.keys().collect::<Vec<_>>()
    );

    // A def BODY does not execute at its position: a function that merely
    // references `f` (and is itself never called early) does not drag `f` out.
    let body_only = "def f(x: int) -> int:\n    return abs(x)\n\
                     def h(x: int) -> int:\n    return f(x)\n\
                     def abs(x: int) -> int:\n    return x * 7\n";
    let a = analyze(body_only);
    assert!(
        a.eligible.contains_key("f") && a.eligible.contains_key("h"),
        "no module-level code runs before the binder: {:?}",
        a.rejected
    );
}

/// B3 applies to a CANONICAL math import too: `def sqrt` → early call → `from
/// math import sqrt` — the early call must see the user def, so the caller of the
/// (dead-by-name) def demotes instead of baking the math call.
#[test]
fn b3_canonical_math_import_after_def_with_early_call() {
    let src = "def sqrt(x: int) -> int:\n    return x * x\n\
               def use(x: int) -> float:\n    return sqrt(x)\n\
               print(use(3))\n\
               from math import sqrt\n";
    let a = analyze(src);
    assert!(
        !a.eligible.contains_key("sqrt"),
        "the def is dead-by-name (a later import wins): {:?}",
        a.eligible.keys().collect::<Vec<_>>()
    );
    assert!(
        !a.eligible.contains_key("use"),
        "use is called while `sqrt` is still the user def: {:?}",
        a.eligible.keys().collect::<Vec<_>>()
    );
}

/// S1: a body that CALLS a name it binds LOCALLY (param / local assignment / a
/// `range` param used as the for-iterator) refuses admission with the local-shadow
/// reason; a neutral local name is not touched by this rule.
#[test]
fn s1_local_shadow_of_a_special_name_refuses() {
    let cases = [
        ("def g(abs: int) -> int:\n    return abs(2)\n", "g", "abs"),
        ("def h(x: int) -> int:\n    len = 5\n    return len(x)\n", "h", "len"),
        (
            "def r(range: int) -> int:\n    t = 0\n    for i in range(3):\n        t += i\n    return t\n",
            "r",
            "range",
        ),
        (
            "def s(sqrt: float) -> float:\n    return sqrt(2.0)\n",
            "s",
            "sqrt",
        ),
        (
            "def helper(x: int) -> int:\n    return x\ndef p(helper: int) -> int:\n    return helper(1)\n",
            "p",
            "helper",
        ),
    ];
    for (src, fname, local) in cases {
        let a = analyze(src);
        assert!(!a.eligible.contains_key(fname), "{src}: must refuse");
        let r = reason(&a, fname);
        assert!(
            r.contains("binds locally") && r.contains(local),
            "{src}: expected the local-shadow reason naming `{local}`: {r}"
        );
    }
    // Neutral local name: refused for another reason (not a whitelist callee), but
    // NOT by the local-shadow rule.
    let a = analyze("def n(x: int) -> int:\n    f = 5\n    return f(x)\n");
    assert!(!reason(&a, "n").contains("binds locally"));
}

/// The call-consistency fixpoint sees a call inside a `try` body (the base
/// collector skipped `try`/`with`/`match` bodies): `f` calls `g`, which is a
/// candidate rejected for a body reason, so `f` must demote by the fixpoint
/// (not survive to an invalid-WASM fallback).
#[test]
fn fixpoint_sees_calls_inside_try_bodies() {
    let src = "def g(x: int) -> int:\n    s = \"a\"\n    return x\n\
               def f(x: int) -> int:\n    try:\n        return g(x)\n    except ValueError:\n        return 0\n";
    let a = analyze(src);
    assert!(!a.eligible.contains_key("g"));
    assert!(
        !a.eligible.contains_key("f"),
        "{:?}",
        a.eligible.keys().collect::<Vec<_>>()
    );
    let r = reason(&a, "f");
    assert!(
        r.contains("calls `g`"),
        "the fixpoint (not a codegen fallback) demoted f: {r}"
    );
}

/// Dead-by-name + the census: a def rebound by a LATER def of the same name is not
/// routable; the later one is. `final_module_bindings` reports the last index.
#[test]
fn dead_by_name_second_def_wins() {
    let src = "def abs(x: int) -> int:\n    return x * 7\n\
               def abs(x: int) -> int:\n    return x * 9\n\
               def use(x: int) -> int:\n    return abs(x)\n";
    let module = pyths_parser::parse(src).expect("parse");
    assert_eq!(
        final_module_bindings(&module).get("abs"),
        Some(&(1, ShadowBinding::UserDef))
    );
    let a = analyze(src);
    assert!(a.eligible.contains_key("abs") && a.eligible.contains_key("use"));
    assert!(
        a.rejected
            .iter()
            .any(|(n, r)| n == "abs" && r.contains("rebound later")),
        "the first def is rejected dead-by-name: {:?}",
        a.rejected
    );
}
