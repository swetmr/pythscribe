//! DX regression tests (v0.2.2 Lane B2 — DEVTOOLS_PROBES-2026-08-13.md).
//!
//! - `breakpoint()` → a JS `debugger;` statement (was a bare call to an
//!   undefined global → runtime ReferenceError, no compile diagnostic).
//! - DX-B1: a binding named like a builtin (`set`/`list`/`dict`/`len`/`str`)
//!   in an ENCLOSING scope must shadow the builtin when referenced from a
//!   nested fn/lambda/method (was silently lowered to `pySetOf`/`.length`/…,
//!   breaking zustand's canonical `def store(set, get): def inc(): set(…)`).
//! - DX-B2: two imports converging on the same JS binding name (was: one
//!   silently dropped, order-dependent). Root fix, two regimes: the SAME
//!   Python name double-bound cross-module is a deterministic hard error;
//!   DISTINCT Python names converging only via our snake→camel conversion
//!   (`create_store` → `createStore` beside `createStore`) are valid Python —
//!   the later import is aliased under a unique JS name and its references
//!   rewritten, so BOTH bindings work, order-independently.

fn compile(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen(&module)
}

// ── breakpoint() → debugger; ──

#[test]
fn breakpoint_lowers_to_debugger() {
    let js = compile("def f():\n    breakpoint()\n    return 1\n");
    assert!(
        js.contains("debugger;"),
        "breakpoint() should emit `debugger;`:\n{js}"
    );
    assert!(
        !js.contains("breakpoint()"),
        "bare breakpoint() call must be gone:\n{js}"
    );
}

#[test]
fn shadowed_breakpoint_is_not_debugger() {
    // A user-defined `breakpoint` must call the user function, not `debugger`.
    let js = compile("def breakpoint():\n    return 5\ndef g():\n    breakpoint()\n");
    assert!(
        !js.contains("debugger;"),
        "shadowed breakpoint must not lower to debugger:\n{js}"
    );
}

// ── DX-B1: builtin-name shadowing across a function boundary ──

#[test]
fn dxb1_nested_fn_uses_enclosing_binding() {
    let js = compile(
        "def store(set, get, list, dict, len, str):\n    def inner():\n        set(1)\n        list([3])\n        dict()\n        len([1])\n        str(9)\n    return inner\n",
    );
    // None of the enclosing-bound names may be lowered to their builtin form.
    for bad in ["pySetOf", "pyDict(", "Array.from", ".length", "pyStr("] {
        assert!(
            !js.contains(bad),
            "shadowed builtin leaked `{bad}` inside nested fn:\n{js}"
        );
    }
    assert!(
        js.contains("set(1)"),
        "set must call the enclosing param:\n{js}"
    );
    assert!(
        js.contains("len([1])"),
        "len must call the enclosing param:\n{js}"
    );
}

#[test]
fn dxb1_lambda_uses_enclosing_binding() {
    let js = compile("def store(set):\n    return lambda: set(1)\n");
    assert!(!js.contains("pySetOf"), "lambda mis-lowered set:\n{js}");
    assert!(js.contains("set(1)"), "{js}");
}

#[test]
fn dxb1_method_local_closure_uses_enclosing_binding() {
    let js = compile(
        "class K:\n    def meth(self, set):\n        def inner():\n            return set(4)\n        return inner()\n",
    );
    assert!(
        !js.contains("pySetOf"),
        "method-local closure mis-lowered set:\n{js}"
    );
    assert!(js.contains("set(4)"), "{js}");
}

#[test]
fn dxb1_deeply_nested_uses_enclosing_binding() {
    let js = compile(
        "def store(set):\n    def m1():\n        def m2():\n            return set(3)\n        return m2()\n    return m1()\n",
    );
    assert!(
        !js.contains("pySetOf"),
        "doubly-nested closure mis-lowered set:\n{js}"
    );
    assert!(js.contains("set(3)"), "{js}");
}

#[test]
fn dxb1_unshadowed_builtin_still_lowers() {
    // Control: with no shadowing binding, `set(...)`/`len(...)` still lower to
    // the builtin forms — the fix must not disable legitimate lowering.
    let js = compile("def f():\n    s = set([1, 2])\n    xs = [1, 2, 3]\n    return len(xs)\n");
    assert!(
        js.contains("pySetOf"),
        "unshadowed set() should still lower:\n{js}"
    );
    assert!(
        js.contains(".length"),
        "unshadowed len(list) should still lower:\n{js}"
    );
}

// ── DX-B2: cross-module import name collision ──
//
// Two regimes (root fix):
//   * SAME Python name bound twice (`import a as z` + `import b as z`) —
//     a genuine double-bind → deterministic hard error.
//   * DISTINCT Python names converging on one JS binding only because of our
//     own snake→camel conversion (`create_store` → `createStore` beside a
//     direct `createStore`) — valid Python; the later import is ALIASED under
//     a unique JS name and its references rewritten, so BOTH bindings work,
//     order-independently.

#[test]
fn dxb2_camel_convergence_aliases_both_bindings() {
    // zustand first: it claims `createStore`; redux's `createStore` (a
    // DIFFERENT Python name) is hoisted under a unique alias and its
    // reference site rewritten. Previously redux's import was silently
    // dropped and `b` called zustand's function.
    let js = compile(
        "from zustand import create_store\nfrom redux import createStore\na = create_store(1)\nb = createStore(2)\n",
    );
    assert_valid(&js, "dxb2_alias_order_a");
    assert!(
        !js.contains("import name collision"),
        "must not hard-error:\n{js}"
    );
    assert!(
        js.contains("import { createStore } from \"zustand\";"),
        "zustand keeps the plain binding:\n{js}"
    );
    assert!(
        js.contains("import { createStore as __pyimp_createStore_0 } from \"redux\";"),
        "redux must hoist under a unique alias:\n{js}"
    );
    assert!(
        js.contains("let a = createStore(1);"),
        "a binds zustand's:\n{js}"
    );
    assert!(
        js.contains("let b = __pyimp_createStore_0(2);"),
        "b must call redux's aliased binding:\n{js}"
    );
}

#[test]
fn dxb2_camel_convergence_aliases_order_independent() {
    // Swapped order: redux claims `createStore`; zustand's `create_store`
    // (converted → createStore) is the aliased side and every `create_store`
    // reference rewrites to the unique name. Previously this order emitted an
    // UNCONVERTED `create_store(1)` → ReferenceError.
    let js = compile(
        "from redux import createStore\nfrom zustand import create_store\na = create_store(1)\nb = createStore(2)\n",
    );
    assert_valid(&js, "dxb2_alias_order_b");
    assert!(
        !js.contains("import name collision"),
        "must not hard-error:\n{js}"
    );
    assert!(
        js.contains("import { createStore } from \"redux\";"),
        "redux keeps the plain binding:\n{js}"
    );
    assert!(
        js.contains("import { createStore as __pyimp_createStore_0 } from \"zustand\";"),
        "zustand must hoist under a unique alias:\n{js}"
    );
    assert!(
        js.contains("let a = __pyimp_createStore_0(1);"),
        "a must call zustand's aliased binding:\n{js}"
    );
    assert!(
        js.contains("let b = createStore(2);"),
        "b binds redux's:\n{js}"
    );
    // No unconverted create_store leaks (the old order-B ReferenceError).
    assert!(
        !js.contains("create_store("),
        "unconverted create_store leaked:\n{js}"
    );
}

#[test]
fn dxb2_aliased_reimport_dedups() {
    // Re-importing the ALIASED side is idempotent — one unique hoist only.
    let js = compile(
        "from zustand import create_store\nfrom redux import createStore\nfrom redux import createStore\nb = createStore(2)\n",
    );
    assert_valid(&js, "dxb2_alias_reimport");
    assert_eq!(
        js.matches("from \"redux\"").count(),
        1,
        "aliased re-import must dedup:\n{js}"
    );
}

#[test]
fn dxb2_alias_shadowed_by_function_local() {
    // Control: a FUNCTION-scope binding named like the aliased import shadows
    // it (Python scoping) — the reference must stay the param, not the unique.
    let js = compile(
        "from zustand import create_store\nfrom redux import createStore\ndef f(createStore):\n    return createStore(9)\nb = createStore(2)\n",
    );
    assert_valid(&js, "dxb2_alias_shadow");
    let body = js
        .split("function f")
        .nth(1)
        .unwrap_or(&js)
        .split('}')
        .next()
        .unwrap_or("");
    assert!(
        body.contains("createStore(9)") && !body.contains("__pyimp"),
        "param must shadow the aliased import inside f:\n{js}"
    );
    // Module scope still rewrites.
    assert!(js.contains("let b = __pyimp_createStore_0(2);"), "{js}");
}

#[test]
fn dxb2_plain_import_alias_collision_is_hard_error() {
    let js = compile("import zustand as z\nimport redux as z\n");
    assert!(
        js.contains("import name collision") && js.contains("throw new Error"),
        "colliding aliased imports must hard-error:\n{js}"
    );
}

#[test]
fn dxb2_function_local_imports_distinct_scopes_bind_correct_modules() {
    // Fix J: two functions each importing a DIFFERENT module under the same
    // local alias are distinct Python-scoped bindings. They must NOT hard-error
    // (the first DX-B2 fix's false positive) AND — critically — neither import
    // may be silently DROPPED (the second fix's silent miscompile, where g()
    // returned NumPy). Assert BOTH modules are imported and that g's local `m`
    // binds the pandas namespace, not numpy.
    let js = compile(
        "def f():\n    import numpy as m\n    return m\ndef g():\n    import pandas as m\n    return m\n",
    );
    assert!(
        !js.contains("import name collision"),
        "wrongly flagged a collision:\n{js}"
    );
    // Both distinct modules must actually be imported (neither dropped).
    assert!(js.contains("from \"numpy\""), "numpy import dropped:\n{js}");
    assert!(
        js.contains("from \"pandas\""),
        "pandas import DROPPED (silent miscompile):\n{js}"
    );
    // Round-4 (findings 2 & 3): each function-local import is genuinely
    // function-local — hoisted under a UNIQUE name and bound in the body via
    // `let m = <unique>`. Neither leaks a bare top-level `m`, so f resolves m
    // to numpy and g resolves m to pandas without either shadowing the other.
    let uniq_from = |module: &str| -> String {
        js.lines()
            .find(|l| l.contains(&format!("from \"{}\"", module)))
            .and_then(|l| l.split_whitespace().nth(3))
            .expect("namespace binding name")
            .to_string()
    };
    let numpy_uniq = uniq_from("numpy");
    let pandas_uniq = uniq_from("pandas");
    assert_ne!(
        numpy_uniq, pandas_uniq,
        "the two imports must get distinct hoist names:\n{js}"
    );
    assert!(
        js.contains(&format!("let m = {}", numpy_uniq)),
        "f's local `m` must bind the numpy namespace `{numpy_uniq}`:\n{js}"
    );
    assert!(
        js.contains(&format!("let m = {}", pandas_uniq)),
        "g's local `m` must bind the pandas namespace `{pandas_uniq}`:\n{js}"
    );
    // No bare top-level `m` binding leaks out of either function.
    assert!(
        !js.contains("import * as m "),
        "function-local `m` must not leak to module top:\n{js}"
    );
}

#[test]
fn dxb2_from_import_distinct_scopes_bind_correct_modules() {
    // Fix J for from-imports: `from a import x` / `from b import x` in two
    // functions must both be imported (neither dropped) and g's `x` must bind b.
    let js = compile(
        "def f():\n    from numpy import array\n    return array(1)\ndef g():\n    from pandas import array\n    return array(2)\n",
    );
    assert!(js.contains("from \"numpy\""), "numpy import dropped:\n{js}");
    assert!(
        js.contains("from \"pandas\""),
        "pandas import DROPPED (silent miscompile):\n{js}"
    );
    // Round-4: both function-local from-imports hoist under unique names and
    // bind `array` in each body via `let array = <unique>`.
    let uniq_from = |module: &str| -> String {
        js.lines()
            .find(|l| l.contains(&format!("from \"{}\"", module)))
            .and_then(|l| l.split(" as ").nth(1))
            .and_then(|s| s.split_whitespace().next())
            .expect("renamed binding")
            .to_string()
    };
    let numpy_uniq = uniq_from("numpy");
    let pandas_uniq = uniq_from("pandas");
    assert_ne!(
        numpy_uniq, pandas_uniq,
        "distinct hoist names expected:\n{js}"
    );
    assert!(
        js.contains(&format!("let array = {}", numpy_uniq)),
        "f's `array` must bind the numpy import `{numpy_uniq}`:\n{js}"
    );
    assert!(
        js.contains(&format!("let array = {}", pandas_uniq)),
        "g's `array` must bind the pandas import `{pandas_uniq}`:\n{js}"
    );
}

#[test]
fn dxb2_same_module_reimport_is_not_an_error() {
    // Re-importing the SAME binding from the SAME module is idempotent (dedup),
    // not a collision.
    let js = compile("from collections import defaultdict\nfrom collections import defaultdict\n");
    assert!(
        !js.contains("import name collision"),
        "same-module re-import must not error:\n{js}"
    );
}

// ── node --check oracle for the reserved-word / kwargs fixes (A/C/I) ──

fn node_check(js: &str, name: &str) -> Result<(), String> {
    use std::process::Command;
    let dir = std::env::temp_dir().join(format!("pyths_dxfix_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{name}.mjs"));
    std::fs::write(&path, js).unwrap();
    let out = match Command::new("node").arg("--check").arg(&path).output() {
        Ok(o) => o,
        Err(_) => return Ok(()), // node unavailable → skip
    };
    let _ = std::fs::remove_file(&path);
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

fn assert_valid(js: &str, name: &str) {
    if let Err(e) = node_check(js, name) {
        panic!("emitted module `{name}` is not valid JS:\n{e}\n--- JS ---\n{js}");
    }
}

// ── A: import alias to a reserved word ──

#[test]
fn a_namespace_import_alias_reserved_word() {
    // `import math as default` must sanitize the binding AND its references.
    let js = compile("import math as default\nx = default.pi\n");
    assert_valid(&js, "a_ns_alias");
    assert!(
        js.contains("import * as default$"),
        "namespace binding not sanitized:\n{js}"
    );
    assert!(
        js.contains("default$.pi"),
        "reference not sanitized to match:\n{js}"
    );
    assert!(
        !js.contains("import * as default "),
        "raw reserved binding emitted:\n{js}"
    );
}

#[test]
fn a_from_import_alias_reserved_word() {
    let js = compile("from math import sqrt as default\ny = default(4)\n");
    assert_valid(&js, "a_from_alias");
    assert!(
        js.contains("sqrt as default$"),
        "from-import binding not sanitized:\n{js}"
    );
    assert!(js.contains("default$(4)"), "reference not sanitized:\n{js}");
}

// ── C: dataclass field named like a reserved word ──

#[test]
fn c_dataclass_reserved_word_field() {
    let js = compile(
        "from dataclasses import dataclass\n@dataclass\nclass Cfg:\n    default: int\n    new: str\n",
    );
    assert_valid(&js, "c_dataclass");
    // Constructor param + RHS use the sanitized local; the property stays raw.
    assert!(
        js.contains("constructor(default$"),
        "ctor param not sanitized:\n{js}"
    );
    assert!(
        js.contains("this.default = default$"),
        "assignment mismatched:\n{js}"
    );
    assert!(
        !js.contains("this.default = default;"),
        "raw reserved RHS emitted:\n{js}"
    );
}

#[test]
fn c_dataclass_reserved_field_message_label_is_raw() {
    // Review 3: the JS condition uses the sanitized var (`default$`), but the
    // user-visible message LABEL must read `Cfg.default`, not `Cfg.default$`.
    let js = compile(
        "from dataclasses import dataclass\nfrom pydantic import Field\n@dataclass\nclass Cfg:\n    default: int = Field(gt=5)\n",
    );
    assert_valid(&js, "c_msg_label");
    assert!(
        js.contains("Cfg.default: "),
        "message label must be raw:\n{js}"
    );
    assert!(
        !js.contains("Cfg.default$"),
        "message label leaked the sanitized name:\n{js}"
    );
    // The condition still uses the sanitized JS binding.
    assert!(
        js.contains("default$ <= 5"),
        "condition must use sanitized var:\n{js}"
    );
}

// ── I: breakpoint() with kwargs must not drop side effects ──

#[test]
fn i_breakpoint_with_kwargs_preserves_side_effects() {
    let js = compile("def mk():\n    return {}\ndef f():\n    breakpoint(**mk())\n");
    assert_valid(&js, "i_bp_kwargs");
    // The kwargs expression (mk()) must be evaluated, not dropped for a bare
    // `debugger;`.
    assert!(
        js.contains("mk()"),
        "breakpoint kwargs side effect dropped:\n{js}"
    );
}

#[test]
fn i_bare_breakpoint_still_debugger() {
    let js = compile("def f():\n    breakpoint()\n");
    assert!(
        js.contains("debugger;"),
        "bare breakpoint() must still be debugger:\n{js}"
    );
}

// ── Issue #438: order-independent shadow resolution (binding pre-pass) ──

#[test]
fn prepass_f_inner_def_sees_later_enclosing_binding() {
    // Case F: `set` is defined LATER in the enclosing fn than the inner def
    // that references it. The pre-pass must see it (order-independent), so the
    // inner `set(1)` is the enclosing binding, not the `pySetOf` builtin.
    let js = compile(
        "def store():\n    def inc():\n        return set(1)\n    set = lambda v: [v]\n    return inc\n",
    );
    assert!(
        !js.contains("pySetOf"),
        "inner def mis-lowered a later-declared shadow:\n{js}"
    );
    assert!(js.contains("set(1)"), "{js}");
}

#[test]
fn prepass_e_comprehension_target_shadows_builtin() {
    // Case E: the comprehension for-target `len` shadows the builtin inside the
    // element, so `len(x)` calls the target, not pyLen/`.length`.
    let js = compile("def f(xs: list) -> list:\n    return [len(x) for len in xs]\n");
    assert!(
        !js.contains("pyLen"),
        "comprehension target didn't shadow len:\n{js}"
    );
    assert!(
        !js.contains(").length"),
        "comprehension target didn't shadow len:\n{js}"
    );
    assert!(js.contains("len(x)"), "{js}");
}

#[test]
fn prepass_h_global_builtin_falls_back() {
    // Case H: `global len` means len is NOT a local binding, so `len(...)`
    // resolves to the builtin lowering (here the `.length` fast path).
    let js = compile("def f() -> int:\n    global len\n    return len([1, 2])\n");
    assert!(
        js.contains(".length") || js.contains("pyLen"),
        "global len must fall back to the builtin:\n{js}"
    );
}

#[test]
fn prepass_forward_ref_builtin_name_is_local() {
    // A builtin-named local used BEFORE its binding is still a local (Python
    // scoping) — must not lower to the builtin.
    let js = compile(
        "def f():\n    def g():\n        return list([1])\n    list = lambda v: v\n    return g\n",
    );
    assert!(
        !js.contains("Array.from"),
        "forward-ref shadow mis-lowered list:\n{js}"
    );
    assert!(js.contains("list([1])"), "{js}");
}

// ── Stage 2: import binding correctness under the pre-pass ──

#[test]
fn import_alias_shadowing_param_rebinds_locally() {
    // `def f(op): import operator as op` — the function-local import must bind
    // `op` INSIDE f (reassigning the param), not be shadowed by the param.
    let js = compile("def f(op):\n    import operator as op\n    return op\n");
    assert_valid(&js, "import_param_shadow");
    // The module is hoisted under a unique name and the param is reassigned.
    assert!(
        js.contains("from \"pyths-runtime/stdlib/operator\""),
        "operator not imported:\n{js}"
    );
    assert!(
        js.contains("op = __pyimp_op_0"),
        "param `op` must be rebound to the imported namespace:\n{js}"
    );
    // NOT a plain `import * as op` shadowed by the param.
    assert!(
        !js.contains("import * as op "),
        "raw `op` import is shadowed by the param:\n{js}"
    );
}

#[test]
fn special_react_import_collision_hard_errors() {
    // The pyths.react HYBRID path must route through collision registration:
    // `from pyths.react import style` (→ pyths-runtime/react) beside
    // `from mylib import style` (→ mylib) is a real cross-module collision.
    let js = compile("from pyths.react import style\nfrom mylib import style\nx = style()\n");
    assert!(
        js.contains("import name collision") && js.contains("throw new Error"),
        "special-import cross-module collision must hard-error:\n{js}"
    );
}

#[test]
fn special_react_same_effective_module_dedups() {
    // `from react import useState` and `from pyths.react import use_state`
    // (→ useState, both from "react") must DEDUP to one import, not emit a
    // duplicate `import { useState } from "react"` (an ESM parse error).
    let js =
        compile("from react import useState\nfrom pyths.react import use_state\nx = useState(1)\n");
    assert_valid(&js, "special_react_dedup");
    assert_eq!(
        js.matches("import { useState } from \"react\"").count(),
        1,
        "duplicate useState import (ESM already-declared):\n{js}"
    );
}

// ── Review: module scope stays SOURCE-ORDER (not order-independent) ──

#[test]
fn module_scope_forward_ref_is_source_order() {
    // A module-level name used BEFORE its module-level assignment is NOT yet a
    // declared local — the module executes top-to-bottom. So `len(...)` here
    // resolves to the BUILTIN, exactly as before the pre-pass, even though
    // `len` is assigned later at module scope.
    let js = compile("y = len([1, 2, 3])\nlen = 5\nprint(y)\n");
    assert!(
        js.contains(".length") || js.contains("pyLen"),
        "module forward-ref must resolve `len` to the builtin (source order):\n{js}"
    );
}

#[test]
fn function_still_order_independent_after_module_fix() {
    // The module fix must NOT disturb function-scope order-independence.
    let js = compile(
        "def f():\n    def g():\n        return list([1])\n    list = lambda v: v\n    return g\n",
    );
    assert!(
        !js.contains("Array.from"),
        "function forward-ref shadow regressed:\n{js}"
    );
    assert!(js.contains("list([1])"), "{js}");
}

// ── Review: comprehension leftmost iterable is evaluated in the enclosing scope ──

#[test]
fn comprehension_leftmost_iterable_resolves_in_enclosing_scope() {
    // The leftmost iterable `len([1,2,3])` is evaluated BEFORE the target binds,
    // in the enclosing scope, so its `len` is the builtin — the comprehension
    // target must not shadow it there.
    let js = compile("def f() -> list:\n    return [x for len in len([1, 2, 3])]\n");
    assert!(
        js.contains(".length") || js.contains("pyLen("),
        "leftmost iterable `len(...)` must resolve to the builtin:\n{js}"
    );
}

#[test]
fn comprehension_target_still_shadows_in_element() {
    // A target that does NOT appear in the leftmost iterable still shadows the
    // builtin inside the element (case E), keeping its own name.
    let js = compile("def f(xs: list) -> list:\n    return [len(0) for len in xs]\n");
    assert!(
        js.contains("(len) => len(0)"),
        "target must stay `len` and shadow:\n{js}"
    );
    assert!(
        !js.contains("pyLen"),
        "element must call the target, not the builtin:\n{js}"
    );
}

#[test]
fn comprehension_leftmost_list_builtin_not_target() {
    // Reviewer repro: `[list for list in list([[1],[2]])]` — the leftmost
    // `list(...)` is the builtin (enclosing scope); the target `list` is a
    // binding (a valid identifier), not lowered to the builtin arrow.
    let js = compile("def f():\n    return [list for list in list([[1], [2]])]\n");
    assert_valid(&js, "comp_leftmost_list");
    assert!(
        js.contains("pyListOf([[1], [2]])"),
        "leftmost list(...) must be the builtin:\n{js}"
    );
    assert!(
        js.contains("(list) => list"),
        "target `list` must stay a plain binding:\n{js}"
    );
}

// ── Review finding 4: binding-walk completeness ──

#[test]
fn del_makes_name_a_local_not_builtin() {
    // `del len` (even in an unreachable branch) makes `len` a local of the
    // enclosing scope, so an inner ref must NOT lower to the builtin.
    let js = compile(
        "def f() -> int:\n    def g() -> int:\n        return len([1])\n    if False:\n        del len\n    return g()\n",
    );
    assert!(
        !js.contains(".length"),
        "del-bound `len` wrongly lowered to builtin:\n{js}"
    );
    assert!(
        !js.contains("pyLen"),
        "del-bound `len` wrongly lowered to builtin:\n{js}"
    );
}

#[test]
fn walrus_in_nested_def_default_binds_enclosing() {
    // The walrus in a nested def's DEFAULT arg is evaluated in the enclosing
    // scope, binding `len` there — so `len([1])` uses it, not the builtin.
    let js = compile(
        "def outer() -> int:\n    def inner(x=(len := lambda x: x)):\n        return x\n    return len([1])\n",
    );
    assert_valid(&js, "walrus_nested_default");
    assert!(
        !js.contains(".length"),
        "walrus-bound len lowered to builtin:\n{js}"
    );
    assert!(js.contains("len([1])"), "{js}");
}

// ── Review finding 5: from-import param shadow + export identity ──

#[test]
fn from_import_shadowing_param_rebinds_locally() {
    // `def f(sqrt): from math import sqrt` must rebind the param inside f.
    let js = compile("def f(sqrt):\n    from math import sqrt\n    return sqrt(9)\n");
    assert_valid(&js, "from_import_param_shadow");
    assert!(
        js.contains("sqrt as __pyimp_sqrt_0"),
        "sqrt not hoisted under a unique name:\n{js}"
    );
    assert!(
        js.contains("sqrt = __pyimp_sqrt_0"),
        "param sqrt must be reassigned:\n{js}"
    );
}

#[test]
fn from_import_same_alias_different_export_hard_errors() {
    // Same alias `hook`, same effective module "react", DIFFERENT exports
    // (useState vs useEffect) must NOT dedup — it's a collision.
    let js = compile(
        "from react import useState as hook\nfrom pyths.react import use_effect as hook\nx = hook()\n",
    );
    assert!(
        js.contains("import name collision") && js.contains("throw new Error"),
        "different-export same-alias must hard-error:\n{js}"
    );
}

#[test]
fn from_import_same_export_still_dedups() {
    // Same export + module under the same alias is still an idempotent re-import.
    let js = compile("from math import sqrt\nfrom math import sqrt\nx = sqrt(4)\n");
    assert!(
        !js.contains("import name collision"),
        "same export re-import must not error:\n{js}"
    );
    assert_eq!(
        js.matches("import { sqrt } from").count(),
        1,
        "duplicate sqrt import:\n{js}"
    );
    // Round-3: TRUE dedup — no unique re-hoist, and above all no
    // `sqrt = __pyimp_sqrt_0;` (assignment to an immutable import binding —
    // a runtime TypeError the pre-planner ordering emitted).
    assert!(
        !js.contains("__pyimp"),
        "idempotent re-import must fully dedup:\n{js}"
    );
}

#[test]
fn plain_import_same_module_dedups_without_reassign() {
    // Round-3: `import json` twice — the second is a Python no-op rebind. The
    // planner must dedup it, NOT emit `json = __pyimp_json_0;` (assignment to
    // an immutable namespace import binding → runtime TypeError).
    let js = compile("import json\nimport json\nx = json.dumps([1])\n");
    assert_valid(&js, "plain_import_dedup");
    assert_eq!(
        js.matches("import * as json").count(),
        1,
        "duplicate json namespace import:\n{js}"
    );
    assert!(
        !js.contains("__pyimp"),
        "idempotent namespace re-import must fully dedup:\n{js}"
    );
    assert!(
        !js.contains("json = "),
        "must not reassign an import binding:\n{js}"
    );
}

#[test]
fn function_local_reimport_same_module_dedups() {
    // Round-3: a re-import INSIDE the same function is idempotent too — the
    // scope-level import-decl tracking must recognize "this binding IS this
    // import" (is_declared alone can't: the first import declared the name).
    let js = compile(
        "def f():\n    from math import sqrt\n    from math import sqrt\n    return sqrt(4)\n",
    );
    assert_valid(&js, "fn_local_reimport_dedup");
    // Round-4: the function-local import hoists once under a unique name +
    // one body-local `let sqrt = <unique>`; the SECOND (identical) import is
    // deduped — no second import line and no second binding.
    assert_eq!(
        js.matches("from \"pyths-runtime/stdlib/math\"").count(),
        1,
        "duplicate math import — second re-import not deduped:\n{js}"
    );
    assert_eq!(
        js.matches("let sqrt = ").count(),
        1,
        "duplicate `sqrt` binding — re-import not deduped:\n{js}"
    );
}

// ── Review finding 2: `global` skips intervening enclosing-function frames ──

#[test]
fn global_skips_intervening_local_frame() {
    // `global len` in inner must resolve at module scope, NOT capture outer's
    // local `len` — with no module-level `len`, it lowers to the builtin.
    let js = compile(
        "def outer() -> int:\n    len = lambda x: 7\n    def inner() -> int:\n        global len\n        return len([1])\n    return inner()\n",
    );
    // inner's `len([1])` must lower to the builtin (.length / pyLen), i.e. it did
    // NOT resolve to outer's local `len`.
    let inner = js.split("function inner").nth(1).unwrap_or(&js);
    assert!(
        inner.contains(".length") || inner.contains("pyLen"),
        "global len must skip outer's local and lower to the builtin:\n{js}"
    );
}

// ── Round-3 review: annotations participate in the binding pre-pass ──

#[test]
fn annotation_only_target_is_a_local() {
    // `len: int` with NO value makes `len` a static local (PEP 526) —
    // CPython raises UnboundLocalError on the later read. It must NOT
    // lower to the builtin.
    let js = compile("def f():\n    len: int\n    return len([1, 2])\n");
    assert!(
        !js.contains(".length") && !js.contains("pyLen"),
        "annotation-only `len: int` must suppress the builtin lowering:\n{js}"
    );
    assert!(js.contains("len([1, 2])"), "{js}");
}

#[test]
fn walrus_in_annotation_binds_the_scope() {
    // `x: (len := int)` — the annotation is never evaluated in a function
    // body, but its walrus target is still a static LOCAL (CPython:
    // UnboundLocalError on the later read). Must not lower to the builtin.
    let js = compile("def f():\n    x: (len := int)\n    return len([1, 2])\n");
    assert!(
        !js.contains(".length") && !js.contains("pyLen"),
        "walrus-in-annotation `len` must suppress the builtin lowering:\n{js}"
    );
    assert!(js.contains("len([1, 2])"), "{js}");
}

#[test]
fn walrus_in_nested_def_param_annotation_binds_enclosing_scope() {
    // A nested def's param annotation is evaluated in the ENCLOSING scope at
    // def time, so `def inner(x: (len := 1))` makes `len` a local of outer.
    let js = compile(
        "def outer():\n    def inner(x: (len := 1)):\n        return x\n    return len([1, 2])\n",
    );
    assert!(
        !js.contains(".length") && !js.contains("pyLen"),
        "walrus in nested param annotation must bind the enclosing scope:\n{js}"
    );
}

// ── Round-3 item 2: recognized-lib hybrid path applies the param-shadow rebind ──

#[test]
fn special_react_import_shadowing_param_rebinds_locally() {
    // `def f(hook): from pyths.react import use_effect as hook` — the hybrid
    // pyths.react path must REASSIGN the param (`hook = __pyimp_hook_0;`),
    // not emit `const hook = ...` (a redeclaration SyntaxError inside
    // `function f(hook)`).
    let js = compile(
        "from react import useState as hook\n\ndef f(hook):\n    from pyths.react import use_effect as hook\n    return hook\n",
    );
    assert_valid(&js, "special_react_param_shadow");
    assert!(
        js.contains("useEffect as __pyimp_hook_0"),
        "useEffect must hoist under a unique name:\n{js}"
    );
    assert!(
        js.contains("hook = __pyimp_hook_0"),
        "param `hook` must be reassigned to the import:\n{js}"
    );
    assert!(
        !js.contains("const hook = __pyimp") && !js.contains("let hook = __pyimp"),
        "must not redeclare `hook` beside the param:\n{js}"
    );
}

// ── Round-3 item 3: relative from-imports go through the shared planner ──

#[test]
fn relative_import_cross_module_collision_hard_errors() {
    // `from .a import x` + `from .b import x` at module scope — Python
    // rebinds last-wins; ESM cannot bind one name twice. Previously BOTH
    // declarations were emitted (a parse error with no diagnostic); routed
    // through the planner this is the DX-B2 hard error like every other form.
    let js = compile("from .a import x\nfrom .b import x\nprint(x)\n");
    assert!(
        js.contains("import name collision") && js.contains("throw new Error"),
        "relative cross-module collision must hard-error:\n{js}"
    );
    // The diagnostic names the Python-source dotted modules.
    assert!(
        js.contains(".a") && js.contains(".b"),
        "diagnostic should show .a/.b:\n{js}"
    );
}

#[test]
fn relative_import_same_module_reimport_dedups() {
    // Idempotent relative re-import — one declaration, no unique re-hoist.
    let js = compile("from .a import x\nfrom .a import x\nprint(x)\n");
    assert_valid(&js, "relative_reimport_dedup");
    assert_eq!(
        js.matches("import { x }").count(),
        1,
        "duplicate x import:\n{js}"
    );
    assert!(
        !js.contains("__pyimp"),
        "same-module relative re-import must dedup:\n{js}"
    );
}

#[test]
fn relative_import_shadowing_param_rebinds_locally() {
    // `def f(x): from .a import x` — the relative path must apply the same
    // param-shadow REASSIGN as every other import form.
    let js = compile("def f(x):\n    from .a import x\n    return x\n");
    assert_valid(&js, "relative_param_shadow");
    assert!(
        js.contains("x as __pyimp_x_0"),
        "relative import must hoist under a unique name:\n{js}"
    );
    assert!(
        js.contains("x = __pyimp_x_0"),
        "param `x` must be reassigned:\n{js}"
    );
    assert!(
        !js.contains("const x = __pyimp") && !js.contains("let x = __pyimp"),
        "must not redeclare `x` beside the param:\n{js}"
    );
}

#[test]
fn relative_import_distinct_function_scopes_bind_correct_modules() {
    // Fix-J semantics for relative imports: two functions importing the same
    // name from DIFFERENT relative modules must both work, g's binding
    // shadowing via a body-local rebind.
    let js = compile(
        "def f():\n    from .a import x\n    return x\ndef g():\n    from .b import x\n    return x\n",
    );
    assert!(
        !js.contains("import name collision"),
        "wrongly flagged a collision:\n{js}"
    );
    assert!(js.contains("from \"./a\""), "./a import dropped:\n{js}");
    assert!(js.contains("from \"./b\""), "./b import dropped:\n{js}");
    assert!(
        js.contains("let x = __pyimp_x_0"),
        "g must rebind `x` to the ./b import locally:\n{js}"
    );
}

// ── Round-4: import-identity cache correctness + function-local scoping ──

#[test]
fn reimport_after_assignment_is_not_deduped() {
    // Finding 1: `from math import sqrt; sqrt = ...; from math import sqrt` —
    // the intervening assignment breaks the cached identity, so the SECOND
    // import must re-emit (return math.sqrt), NOT dedup to the lambda.
    let js = compile(
        "def f(sqrt):\n    from math import sqrt\n    sqrt = lambda x: 99\n    from math import sqrt\n    return sqrt(4)\n",
    );
    assert_valid(&js, "reimport_after_assign");
    // Two distinct hoisted imports (both re-emit around the assignment).
    assert_eq!(
        js.matches("from \"pyths-runtime/stdlib/math\"").count(),
        2,
        "the re-import after the assignment must NOT be deduped:\n{js}"
    );
    // The final rebind restores the math import (not the lambda) before use.
    let body = js.split("function f").nth(1).unwrap_or(&js);
    let last_sqrt_assign = body.rfind("sqrt = ").expect("sqrt rebind");
    assert!(
        body[last_sqrt_assign..].starts_with("sqrt = __pyimp_sqrt_"),
        "the last `sqrt` binding before the return must be the re-import:\n{js}"
    );
}

#[test]
fn function_local_import_does_not_dedup_against_outer() {
    // Finding 2: a module-level `from math import sqrt` plus a function-local
    // `from math import sqrt` used BEFORE it. Python makes `sqrt` local to f
    // (UnboundLocalError on the early read), so the function-local import must
    // bind a body-local `let sqrt` (→ TDZ fault), NOT silently reuse the
    // module binding.
    let js = compile(
        "from math import sqrt\n\ndef f():\n    print(sqrt(4))\n    from math import sqrt\n",
    );
    assert_valid(&js, "fn_local_no_outer_dedup");
    let body = js.split("function f").nth(1).unwrap_or(&js);
    assert!(
        body.contains("let sqrt = __pyimp_sqrt_"),
        "function-local import must bind a body-local `let sqrt`:\n{js}"
    );
    // The early use precedes the `let`, so it is a TDZ read (≈UnboundLocalError),
    // not a resolved call to the module `sqrt`.
    let use_at = body.find("sqrt(4)").expect("early use");
    let let_at = body.find("let sqrt =").expect("local binding");
    assert!(
        use_at < let_at,
        "the early use must precede the local binding (TDZ):\n{js}"
    );
}

#[test]
fn function_local_reimport_rebind_no_function_wide_tdz() {
    // Finding 3: two DIFFERENT imports under the same alias in one function,
    // each used after its own import. The first must be a body-local `let op`
    // and the second a plain reassignment `op = ...` — NOT a second `let`
    // that would TDZ-shadow the first (correct) read from function entry.
    let js = compile(
        "def f():\n    from math import sqrt as op\n    print(op(4))\n    from math import floor as op\n    return op(3.7)\n",
    );
    assert_valid(&js, "fn_local_reimport_no_tdz");
    let body = js.split("function f").nth(1).unwrap_or(&js);
    assert_eq!(
        body.matches("let op").count(),
        1,
        "exactly one `let op` — the second import must reassign, not re-declare:\n{js}"
    );
    // The reassignment (`op = __pyimp_op_1`) rebinds without a new `let`.
    assert!(
        body.contains("op = __pyimp_op_1") || body.matches("op = __pyimp_op_").count() >= 1,
        "second import must reassign `op`:\n{js}"
    );
    // `op(4)` (first read) precedes the reassignment — no function-wide TDZ.
    let first_read = body.find("op(4)").expect("first read");
    let reassign = body.rfind("op = __pyimp_op_").expect("reassign");
    assert!(
        first_read < reassign,
        "first read must precede the rebind:\n{js}"
    );
}

#[test]
fn from_pyths_stdlib_collision_hard_errors() {
    // Finding 4: `from pyths import math as m` + `from pyths import json as m`
    // routes through the planner — a cross-module collision hard-errors,
    // instead of emitting two immutable `import * as m` (a SyntaxError).
    let js = compile("from pyths import math as m\nfrom pyths import json as m\n");
    assert!(
        js.contains("import name collision") && js.contains("throw new Error"),
        "from-pyths cross-module collision must hard-error:\n{js}"
    );
}

#[test]
fn from_pyths_stdlib_reimport_dedups() {
    // Idempotent `from pyths import math` twice → one namespace import.
    let js = compile("from pyths import math\nfrom pyths import math\nx = math.sqrt(4)\n");
    assert_valid(&js, "from_pyths_dedup");
    assert_eq!(
        js.matches("import * as math").count(),
        1,
        "duplicate math namespace import:\n{js}"
    );
}

// ── FULL_SURFACE #1: dotted import WITHOUT `as` binds the head name ──
// `import pkg.sub` used to emit `import * as pkg.sub from "pkg/sub"` — a JS
// SyntaxError (an ESM namespace binding must be an identifier). CPython
// semantics: the TOP name `pkg` is bound and `pkg.sub` is reachable.

#[test]
fn dotted_import_no_alias_binds_head() {
    let js = compile("import pkg.sub\npkg.sub.f()\n");
    assert_valid(&js, "dotted_no_alias");
    assert!(
        !js.contains("import * as pkg.sub"),
        "dotted namespace binding is invalid JS:\n{js}"
    );
    assert!(
        js.contains("import * as __pyimp_pkg_sub_0 from \"pkg/sub\";"),
        "namespace must hoist under a unique identifier:\n{js}"
    );
    assert!(js.contains("let pkg = {};"), "head object not bound:\n{js}");
    assert!(
        js.contains("pkg.sub = __pyimp_pkg_sub_0;"),
        "dotted path not grafted onto the head:\n{js}"
    );
    assert!(
        js.contains("pkg.sub.f()"),
        "reference site must stay dotted:\n{js}"
    );
}

#[test]
fn dotted_import_no_alias_deep_and_shared_head() {
    // Deep path + a second import sharing the head: ONE mutable head object,
    // intermediate levels materialized, both leaves grafted.
    let js = compile("import a.b.c\nimport a.d\nprint(a.b.c.x, a.d.y)\n");
    assert_valid(&js, "dotted_shared_head");
    assert_eq!(
        js.matches("let a = {};").count(),
        1,
        "exactly one head object:\n{js}"
    );
    assert!(
        js.contains("a.b = {};"),
        "intermediate level missing:\n{js}"
    );
    assert!(
        js.contains("a.b.c = __pyimp_a_b_c_0;"),
        "deep leaf graft:\n{js}"
    );
    assert!(
        js.contains("a.d = __pyimp_a_d_1;"),
        "second leaf graft:\n{js}"
    );
}

#[test]
fn dotted_import_no_alias_idempotent() {
    // Python re-import is a no-op — no duplicate hoists or grafts.
    let js = compile("import pkg.sub\nimport pkg.sub\npkg.sub.f()\n");
    assert_valid(&js, "dotted_idempotent");
    assert_eq!(
        js.matches("import * as __pyimp_pkg_sub_").count(),
        1,
        "re-import must dedup:\n{js}"
    );
    assert_eq!(js.matches("let pkg = {};").count(), 1);
}

// ── Behavioral (node) checks for DX-B1 / DX-B2 ──

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

/// Run `js` under node from `dir`; returns stdout, or None when node is
/// unavailable (skip, same policy as node_check).
fn node_run(dir: &std::path::Path, js: &str, name: &str) -> Option<String> {
    use std::process::Command;
    let path = dir.join(format!("{name}.mjs"));
    std::fs::write(&path, js).unwrap();
    let out = Command::new("node").arg(&path).output().ok()?;
    assert!(
        out.status.success(),
        "node failed on {name}:\n{}\n--- JS ---\n{js}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

#[test]
fn dxb1_behavioral_nested_closure_calls_enclosing_param() {
    // The zustand idiom, behaviorally: the nested action must CALL the
    // enclosing `set` param (appending to `calls`), not build a pySet.
    let src = "\
calls = []
def store(set, get):
    def increment():
        set(41)
    increment()
    return get()
r = store(lambda v: calls.append(v + 1), lambda: calls)
print(r)
";
    let js = compile_inline(src);
    let dir = std::env::temp_dir().join(format!("pyths_dxb1_beh_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let got = node_run(&dir, &js, "dxb1_closure");
    let _ = std::fs::remove_dir_all(&dir);
    let Some(got) = got else { return }; // node unavailable → skip
    assert_eq!(
        got.trim(),
        "[42]",
        "closure must call the enclosing param:\n{js}"
    );
}

#[test]
fn dxb2_behavioral_both_colliding_imports_resolve() {
    // End-to-end: fake `zustand` and `redux` packages whose createStore
    // functions tag their results — both Python bindings must reach the
    // RIGHT module, in both import orders.
    let dir = std::env::temp_dir().join(format!("pyths_dxb2_beh_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (pkg, tag) in [("zustand", "z"), ("redux", "r")] {
        let pdir = dir.join("node_modules").join(pkg);
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(
            pdir.join("package.json"),
            format!("{{\"name\":\"{pkg}\",\"type\":\"module\",\"main\":\"index.mjs\"}}"),
        )
        .unwrap();
        std::fs::write(
            pdir.join("index.mjs"),
            format!("export function createStore(x) {{ return \"{tag}\" + x; }}\n"),
        )
        .unwrap();
    }
    let order_a = "from zustand import create_store\nfrom redux import createStore\nprint(create_store(1), createStore(2))\n";
    let order_b = "from redux import createStore\nfrom zustand import create_store\nprint(create_store(1), createStore(2))\n";
    let mut ran = false;
    for (name, src) in [("order_a", order_a), ("order_b", order_b)] {
        let js = compile_inline(src);
        match node_run(&dir, &js, name) {
            None => break, // node unavailable → skip
            Some(got) => {
                ran = true;
                assert_eq!(
                    got.trim(),
                    "z1 r2",
                    "{name}: each Python name must bind its own module:\n{js}"
                );
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    let _ = ran;
}

#[test]
fn dotted_import_after_plain_import_hard_errors() {
    // `import a` binds `a` to an immutable frozen ESM namespace — the
    // submodule graft cannot be expressed; must be a LOUD compile error,
    // not silently-broken output.
    let js = compile("import numpy\nimport numpy.linalg\n");
    assert!(
        js.contains("throw new Error") && js.contains("already bound"),
        "plain-then-dotted must hard-error:\n{js}"
    );
}

// ── WB-3: `.sort(positional comparator)` honors the 2-arg comparator ──
//
// Python `list.sort` is keyword-only — `sort(*, key=None, reverse=False)` —
// so a POSITIONAL arg can never be Python `list.sort` (`[].sort(f)` is a
// TypeError in CPython). It is therefore a JS Array comparator; lowering it
// to `pyListSort(xs, cmp)` (which reads `cmp` as the `key=` option and drops
// it) silently mis-sorts. A positional arg must divert to verbatim
// `xs.sort(cmp)` (Array.prototype.sort). The keyword/no-arg forms are
// untouched.

#[test]
fn wb3_positional_comparator_diverts_from_pylistsort() {
    let js = compile("xs = [3, 1, 2]\nxs.sort(lambda a, b: b - a)\n");
    assert!(
        js.contains("xs.sort("),
        "positional comparator must emit verbatim Array.sort:\n{js}"
    );
    assert!(
        !js.contains("pyListSort"),
        "positional comparator must NOT lower to pyListSort (drops the cmp):\n{js}"
    );
}

#[test]
fn wb3_positional_comparator_def_also_diverts() {
    // A named `def cmp(a, b)` comparator (the WB-3 reproducer shape) too.
    let js = compile("xs = [3, 1, 2]\ndef cmp(a, b):\n    return b - a\nxs.sort(cmp)\n");
    assert!(
        js.contains("xs.sort(cmp)"),
        "verbatim comparator sort:\n{js}"
    );
    assert!(
        !js.contains("pyListSort"),
        "must not lower to pyListSort:\n{js}"
    );
}

#[test]
fn wb3_keyword_and_noarg_sort_stay_pylistsort() {
    // The Python idioms must be UNCHANGED: no positional arg → pyListSort.
    for src in [
        "xs = [3, 1, 2]\nxs.sort()\n",
        "xs = [3, 1, 2]\nxs.sort(key=lambda x: -x)\n",
        "xs = [3, 1, 2]\nxs.sort(reverse=True)\n",
    ] {
        let js = compile(src);
        assert!(
            js.contains("pyListSort"),
            "keyword/no-arg sort must keep Python list.sort semantics:\n{src}\n{js}"
        );
    }
    // `sorted(xs, key=...)` is a separate builtin — also unchanged.
    let js = compile("ys = sorted([3, 1, 2], key=lambda x: -x)\n");
    assert!(js.contains("pySorted"), "sorted() unchanged:\n{js}");
}

#[test]
fn wb3_behavioral_comparator_descending_and_stable() {
    // Descending comparator sorts descending; a comparator keyed on the first
    // element keeps equal-key elements in input order (stable).
    let src = "\
a = [5, 3, 1, 2, 4]
a.sort(lambda x, y: y - x)
print(a)
b = [[1, 'a'], [1, 'b'], [0, 'c'], [1, 'd']]
b.sort(lambda x, y: x[0] - y[0])
print(b)
";
    let js = compile_inline(src);
    let dir = std::env::temp_dir().join(format!("pyths_wb3_beh_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let got = node_run(&dir, &js, "wb3_sort");
    let _ = std::fs::remove_dir_all(&dir);
    let Some(got) = got else { return }; // node unavailable → skip
    let lines: Vec<&str> = got.trim().lines().collect();
    assert_eq!(lines[0], "[5, 4, 3, 2, 1]", "descending comparator:\n{js}");
    assert_eq!(
        lines[1], "[[0, 'c'], [1, 'a'], [1, 'b'], [1, 'd']]",
        "stable sort keeps equal-key input order:\n{js}"
    );
}

// ── WF-1: use_effect callback returning None must not crash React 19 ──
//
// React stores an effect's return as its cleanup ("destroy") and invokes it.
// It accepts only `undefined` or a function; a Python effect ending in
// `return None` emits `return null`, and `null !== undefined` → React calls
// it → "destroy is not a function". Codegen wraps effect callbacks in
// `__pyEffect`, which coerces any non-function return to `undefined`.

#[test]
fn wf1_effect_callback_wrapped_in_pyeffect() {
    let js = compile(
        "from pyths.react import component, use_effect\n@component\ndef Eff():\n    def _effect():\n        return None\n    use_effect(_effect, [])\n    return None\n",
    );
    assert!(
        js.contains("useEffect(__pyEffect(_effect), [])"),
        "effect callback must be wrapped in __pyEffect:\n{js}"
    );
}

#[test]
fn wf1_effect_without_deps_also_wrapped() {
    let js = compile(
        "from pyths.react import component, use_effect\n@component\ndef Eff():\n    def _effect():\n        return None\n    use_effect(_effect)\n    return None\n",
    );
    assert!(
        js.contains("useEffect(__pyEffect(_effect))"),
        "deps-less effect callback must still be wrapped:\n{js}"
    );
}

#[test]
fn wf1_layout_and_insertion_effects_wrapped() {
    let js = compile(
        "from pyths.react import component, use_layout_effect, use_insertion_effect\n@component\ndef Eff():\n    def _l():\n        return None\n    def _i():\n        return None\n    use_layout_effect(_l, [])\n    use_insertion_effect(_i, [])\n    return None\n",
    );
    assert!(
        js.contains("useLayoutEffect(__pyEffect(_l), [])"),
        "layout effect wrapped:\n{js}"
    );
    assert!(
        js.contains("useInsertionEffect(__pyEffect(_i), [])"),
        "insertion effect wrapped:\n{js}"
    );
}

#[test]
fn wf1_value_hooks_not_wrapped() {
    // use_memo / use_callback return DATA, not a cleanup — must NOT be wrapped.
    let js = compile(
        "from pyths.react import component, use_memo, use_callback\n@component\ndef C():\n    m = use_memo(lambda: 1, [])\n    cb = use_callback(lambda: 2, [])\n    return None\n",
    );
    assert!(
        !js.contains("__pyEffect"),
        "value-returning hooks must NOT be wrapped in __pyEffect:\n{js}"
    );
    assert!(
        js.contains("useMemo(") && js.contains("useCallback("),
        "hooks still emitted:\n{js}"
    );
}

#[test]
fn wf1_behavioral_pyeffect_coerces_return() {
    // Import the REAL runtime.js __pyEffect and exercise its coercion.
    use std::process::Command;
    let rt_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/src");
    if !rt_dir.join("runtime.js").exists() {
        return; // runtime tree not present → skip
    }
    let harness = r#"
import { __pyEffect } from "./runtime.js";
function fail(m) { console.error(m); process.exit(1); }
// return None/null -> coerced to undefined (React-safe, no "destroy is not a function")
if (__pyEffect(() => null)() !== undefined) fail("null not coerced to undefined");
// non-function (a number) -> undefined
if (__pyEffect(() => 5)() !== undefined) fail("number not coerced to undefined");
// no return -> undefined
if (__pyEffect(() => {})() !== undefined) fail("void not undefined");
// a REAL cleanup function is passed through and stays callable
let ran = false;
const cleanup = __pyEffect(() => () => { ran = true; })();
if (typeof cleanup !== "function") fail("real cleanup dropped");
cleanup();
if (!ran) fail("real cleanup not invoked");
console.log("OK");
"#;
    let name = format!("__wf1_pyeffect_{}.mjs", std::process::id());
    let path = rt_dir.join(&name);
    std::fs::write(&path, harness).unwrap();
    let out = Command::new("node").arg(&path).output();
    let _ = std::fs::remove_file(&path);
    let Ok(out) = out else { return }; // node unavailable → skip
    assert!(
        out.status.success(),
        "__pyEffect behavioral failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "OK");
}
