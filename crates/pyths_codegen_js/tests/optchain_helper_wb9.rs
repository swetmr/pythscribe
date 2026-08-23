//! WB-9 — `?.`-chained method lowered to a runtime helper must preserve the
//! optional-chaining short-circuit.
//!
//! `el?.classList.remove("x")` lowers `.remove` → the `pyRemove` shim. The
//! `?.` guards only the receiver *argument* (`el?.classList` → `undefined`
//! when `el` is null), but the helper `pyRemove(undefined, "x")` was then
//! called UNCONDITIONALLY and threw ("object of type 'undefined' has no
//! remove()"). Native `?.add`/`?.toggle` short-circuit correctly; only the
//! helper-lowered container-method shims (pyRemove/pyPop/pyAppend/…) broke.
//!
//! Root fix (one site — `emit_runtime_method`, the sole emission point for
//! the Runtime + Hybrid-runtime shim paths): when the method is reached via
//! optional chaining, temp-bind the emitted receiver once and short-circuit
//! the whole helper call:
//!   `((__optrecv0) => __optrecv0 == null ? undefined : pyRemove(__optrecv0, "x"))(el?.classList)`.
//!
//! Structural tests assert the guard; behavioral tests run under node
//! (skipped when node is unavailable, same policy as import_rebind_443.rs).

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

/// Run `js` under node from a scratch dir; None when node is unavailable.
fn node_run(js: &str, name: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pyths_wb9_{}_{}", name, std::process::id()));
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

// ── structural: the guard is emitted for helper-lowered `?.` methods ────────

#[test]
fn optchain_remove_helper_is_guarded() {
    // `el?.classList.remove("x")` — receiver chain short-circuits; `.remove`
    // lowers to pyRemove. The helper call must be guarded, not invoked on
    // the short-circuited `undefined`.
    let js = compile_inline("el = document.querySelector(\".x\")\nel?.classList.remove(\"foo\")\n");
    assert!(
        js.contains("pyRemove"),
        ".remove must still lower to the pyRemove shim:\n{js}"
    );
    assert!(
        js.contains("== null ? undefined : pyRemove("),
        "helper-lowered `?.` method must short-circuit-guard the whole call:\n{js}"
    );
    // The guard temp-binds the emitted receiver (which itself carries `?.`).
    assert!(
        js.contains("el?.classList"),
        "the receiver chain (with its own `?.`) is still emitted, once:\n{js}"
    );
}

#[test]
fn optchain_pop_helper_is_guarded() {
    // A second shim under `?.` — `pop` → pyPop — proves the fix is at the
    // one lowering site, not a per-method patch.
    let js = compile_inline("el = None\nel?.data.pop()\n");
    assert!(
        js.contains("== null ? undefined : pyPop("),
        "pyPop under `?.` must also be guarded:\n{js}"
    );
}

#[test]
fn optchain_native_add_still_short_circuits() {
    // `.add` is a native Rename (not a helper) — it must keep the native
    // `?.` short-circuit and gain NO helper guard.
    let js = compile_inline("el = None\nel?.classList.add(\"foo\")\n");
    assert!(
        js.contains("el?.classList.add("),
        "native `?.add` must stay native and short-circuit:\n{js}"
    );
    assert!(
        !js.contains("== null ? undefined :"),
        "native method must NOT get a helper guard:\n{js}"
    );
}

#[test]
fn non_optional_remove_helper_unchanged() {
    // No `?.` anywhere → the plain helper call form is unchanged (no guard).
    let js = compile_inline("el = get_el()\nel.classList.remove(\"foo\")\n");
    assert!(
        js.contains("pyRemove(") && js.contains("classList"),
        ".remove must lower to pyRemove:\n{js}"
    );
    assert!(
        !js.contains("== null ? undefined :"),
        "a non-optional helper call must NOT be guarded:\n{js}"
    );
}

// ── behavioral: null short-circuits (no throw); present receiver acts ────────

#[test]
fn optchain_helper_behaves_under_node() {
    // present receiver → the shim runs and mutates; null receiver → the
    // whole `?.` call short-circuits with no throw. Before the fix the null
    // case threw "undefined has no remove()".
    let js = compile_inline(
        "class Box:\n\
         \x20   def __init__(self, s):\n\
         \x20       self.items = s\n\
         def run():\n\
         \x20   out = []\n\
         \x20   b = Box([1, 2, 3])\n\
         \x20   b?.items.remove(2)\n\
         \x20   out.append(b.items)\n\
         \x20   n = None\n\
         \x20   n?.items.remove(2)\n\
         \x20   n?.items.pop()\n\
         \x20   out.append(\"survived\")\n\
         \x20   return out\n\
         print(run())\n",
    );
    let Some(got) = node_run(&js, "behave") else {
        return; // node unavailable → skip
    };
    assert_eq!(
        got.trim(),
        "[[1, 3], 'survived']",
        "present `?.` acts; null `?.` short-circuits with no throw\n--- JS ---\n{js}"
    );
}

// ═════════════════════════════════════════════════════════════════════════
// WB-9 round 2 (0.2.2 batch 2) — METHOD-LEVEL `?.` (`recv?.m(...)`). The
// dispatch site discarded the ATTRIBUTE-level optional flag entirely, so:
//   * helper lowerings ran unguarded (`l?.remove(1)` → `pyRemove(l, 1)` —
//     threw on None);
//   * Rename lowerings dropped the `?.` (`s?.upper()` → `s.toUpperCase()`);
//   * Inline lowerings dropped it too (`s?.strip()`).
// CLASS rule: the attribute-level flag folds into ONE `optional` at the
// dispatch site, honored uniformly by Rename (`?.` kept), Inline (guard
// arrow), Hybrid (routes to the guarded runtime helper) and Runtime
// (existing guard). None-receiver short-circuits to undefined (Python
// `?.` semantics), a present receiver behaves exactly as before.
// ═════════════════════════════════════════════════════════════════════════

fn assert_behaves2(src: &str, expected: &str, name: &str) {
    let js = compile_inline(src);
    let Some(got) = node_run(&js, name) else {
        return; // node unavailable → skip
    };
    assert_eq!(got.trim(), expected, "{name}\n--- JS ---\n{js}");
}

#[test]
fn method_level_optional_list_helper() {
    // list helper (Runtime path): None short-circuits; real list mutates.
    assert_behaves2(
        "l = None\nprint(l?.remove(1))\nxs = [1, 2, 3]\nxs?.remove(2)\nprint(xs)\n",
        "None\n[1, 3]",
        "m_list",
    );
}

#[test]
fn method_level_optional_rename_str() {
    // Rename path: `s?.upper()` keeps the `?.` on the renamed method.
    let js = compile_inline("s = None\nprint(s?.upper())\n");
    assert!(
        js.contains("s?.toUpperCase()"),
        "rename lowering must keep the `?.`:\n{js}"
    );
    assert_behaves2(
        "s = None\nprint(s?.upper())\nt = \"ab\"\nprint(t?.upper())\n",
        "None\nAB",
        "m_rename",
    );
}

#[test]
fn method_level_optional_inline_str() {
    // Inline path (Strip): guard arrow around the inline form.
    assert_behaves2(
        "s = None\nprint(s?.strip())\nt = \" ab \"\nprint(t?.strip())\n",
        "None\nab",
        "m_inline",
    );
}

#[test]
fn method_level_optional_str_helper() {
    // str helper (pyStrReplace family): None short-circuits.
    assert_behaves2(
        "s = None\nprint(s?.replace(\"a\", \"b\"))\nt = \"aa\"\nprint(t?.replace(\"a\", \"b\"))\n",
        "None\nbb",
        "m_str_helper",
    );
}

#[test]
fn method_level_optional_dict_helper() {
    // dict helpers: get / setdefault / items through `?.`.
    assert_behaves2(
        "d = None\nprint(d?.get(\"k\"))\nprint(d?.setdefault(\"k\", 1))\nprint(d?.items())\n\
         e = {\"k\": 2}\nprint(e?.get(\"k\"))\n",
        "None\nNone\nNone\n2",
        "m_dict_helper",
    );
}

#[test]
fn method_level_optional_hybrid_clear() {
    // Hybrid path (clear): `?.` routes through the guarded runtime helper —
    // even on a receiver the inline form would otherwise claim.
    assert_behaves2(
        "n = None\nprint(n?.clear())\nxs = [1, 2]\nxs?.clear()\nprint(xs)\n",
        "None\n[]",
        "m_hybrid",
    );
}

#[test]
fn method_level_optional_provable_list_append() {
    // A PROVABLY-list receiver normally takes the `.push` inline — with `?.`
    // it must still short-circuit on None (guard or helper, not bare push).
    assert_behaves2(
        "def f(flag):\n\
         \x20   xs = [1] if flag else None\n\
         \x20   xs?.append(2)\n\
         \x20   return xs\n\
         print(f(True))\nprint(f(False))\n",
        "[1, 2]\nNone",
        "m_append",
    );
}

#[test]
fn plain_method_calls_unchanged() {
    // Regression guard: no `?.` → no guard, identical lowering shapes.
    let js = compile_inline(
        "s = \" a \"\nprint(s.strip())\nxs = [3, 1]\nxs.append(2)\nd = {\"k\": 1}\nprint(d.get(\"k\"))\n",
    );
    assert!(
        !js.contains("__optrecv"),
        "plain calls must not gain guards:\n{js}"
    );
}
