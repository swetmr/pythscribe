//! B2 (0.2.2 batch 2) — MODULE-LEVEL import rebinds, as ONE class rule.
//!
//! `from math import floor; floor = "x"` emitted the import as an immutable
//! ESM binding and the depth-0 assignment hit "Assignment to constant
//! variable" at runtime. The #443 fix promoted only depth>0 / def-class /
//! `with … as` rebinds — three special cases that validated their own shape
//! and missed every module-level (depth-0) rebind form.
//!
//! CLASS rule (collect_hoisted_names, module bodies only): an import-bound
//! name that is REBOUND by ANY non-import binding form at ANY depth — plain
//! assignment, tuple-unpack, aug-assign, for-target, del, `global`-rebind
//! from inside a def — hoists to a module `let`, which flips the import onto
//! its assignable Rebind path (`import { X as __pyimp_X_n }` + `X =
//! __pyimp_X_n;`). ONE predicate, not per-form patches.
//!
//! Function-local imports are deliberately NOT promoted: they already emit
//! assignably at the import's position, and pre-hoisting would replace the
//! use-before-import TDZ fault with a silent `undefined` read.
//!
//! Behavioral tests run under node (skipped when unavailable); expected
//! outputs are CPython's for the same programs.

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

fn node_run(js: &str, name: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pyths_b2_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    pyths_runtime::materialize_runtime_package(&dir).expect("materialize runtime");
    let path = dir.join(format!("{name}.mjs"));
    std::fs::write(&path, js).unwrap();
    let out = Command::new("node").arg(&path).output().ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "node failed on {name}:\n{stderr}\n--- JS ---\n{js}"
    );
    Some(stdout)
}

fn assert_behaves(src: &str, expected: &str, name: &str) {
    let js = compile_inline(src);
    let Some(got) = node_run(&js, name) else {
        return; // node unavailable → skip (same policy as import_rebind_443.rs)
    };
    assert_eq!(
        got.trim(),
        expected,
        "{name}: output must match CPython\n--- JS ---\n{js}"
    );
}

// ── the rebind-form × depth matrix ──────────────────────────────────────────

#[test]
fn plain_assign_module_level() {
    // CPython: floor is rebound to the string.
    assert_behaves(
        "from math import floor\nfloor = \"x\"\nprint(floor)\n",
        "x",
        "plain_mod",
    );
}

#[test]
fn plain_assign_nested_depth() {
    // depth>0 rebind (regression guard for the pre-existing #443 behavior).
    assert_behaves(
        "from math import floor\nif True:\n    floor = 7\nprint(floor)\n",
        "7",
        "plain_nested",
    );
}

#[test]
fn tuple_unpack_module_level() {
    assert_behaves(
        "from math import floor, ceil\nfloor, ceil = 1, 2\nprint(floor, ceil)\n",
        "1 2",
        "tuple_mod",
    );
}

#[test]
fn aug_assign_module_level() {
    // CPython: pi ≈ 4.141592653589793.
    assert_behaves(
        "from math import pi\npi += 1\nprint(pi)\n",
        "4.141592653589793",
        "aug_mod",
    );
}

#[test]
fn del_module_level() {
    // `del floor` must not write the immutable ESM binding.
    assert_behaves(
        "from math import floor\ndel floor\nprint(\"ok\")\n",
        "ok",
        "del_mod",
    );
}

#[test]
fn global_rebind_from_def() {
    // The rebind runs inside a def body via `global` — still a MODULE rebind.
    assert_behaves(
        "from math import floor\ndef f():\n    global floor\n    floor = 7\nf()\nprint(floor)\n",
        "7",
        "global_rebind",
    );
}

#[test]
fn module_namespace_import_rebind() {
    // `import math` binds the module namespace; a rebind of the head name
    // is the same class (issue text: `from .conf import CONF; CONF={...}`).
    assert_behaves("import math\nmath = 5\nprint(math)\n", "5", "modns");
}

#[test]
fn dict_literal_rebind_conf_shape() {
    // The reported shape: a config symbol re-pointed at a fresh dict.
    assert_behaves(
        "from math import tau\ntau = {\"a\": 1}\nprint(tau[\"a\"])\n",
        "1",
        "conf_shape",
    );
}

#[test]
fn for_target_module_level() {
    // A module-level for-target rebinding an import: loop runs, name leaks.
    assert_behaves(
        "from math import floor\nfor floor in [1, 2]:\n    pass\nprint(floor)\n",
        "2",
        "for_mod",
    );
}

#[test]
fn assign_before_import_order_preserved() {
    // Source order: assignment executes BEFORE the import rebind.
    assert_behaves(
        "floor = 1\nprint(floor)\nfrom math import floor\nprint(floor(2.5))\n",
        "1\n2",
        "assign_before",
    );
}

// ── non-promotion guards (the predicate must not over-fire) ────────────────

#[test]
fn class_attr_does_not_promote() {
    // A class-body binding is its own scope — NOT a module rebind; the
    // import stays a plain immutable binding and still resolves.
    assert_behaves(
        "from math import floor\nclass C:\n    floor = 5\nprint(floor(2.5))\nprint(C().floor)\n",
        "2\n5",
        "class_attr",
    );
}

#[test]
fn function_local_assign_does_not_promote() {
    // A def-local assignment shadows; the module import is untouched.
    assert_behaves(
        "from math import floor\ndef f():\n    floor = 9\n    return floor\nprint(f())\nprint(floor(2.5))\n",
        "9\n2",
        "fn_local",
    );
}

#[test]
fn function_local_import_keeps_local_let() {
    // Function-local import + local rebind: stays a body-local `let`, no
    // module hoist (structural + behavioral).
    let src = "def f():\n    from math import floor\n    floor = 9\n    return floor\nprint(f())\n";
    let js = compile_inline(src);
    assert!(
        js.contains("let floor = __pyimp_floor_0"),
        "function-local import must keep its body-local let:\n{js}"
    );
    assert_behaves(src, "9", "fn_local_import");
}

#[test]
fn untouched_import_stays_plain() {
    // No rebind anywhere → the import emits as a plain named binding
    // (no gratuitous `let` + rename churn).
    let js = compile_inline("from math import floor\nprint(floor(2.5))\n");
    assert!(
        js.contains("import { floor }"),
        "an unrebound import must stay a plain named import:\n{js}"
    );
    assert!(
        !js.contains("__pyimp_floor"),
        "no rebind → no unique-rename path:\n{js}"
    );
}
