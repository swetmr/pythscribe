//! #443 — import-identity invalidation + emission on NON-assignment rebinds.
//!
//! Round-4 (`cc67c1b2`) invalidated the `scope_import_decls` cache on plain
//! assignment / tuple-unpack / aug-assign / del. This issue covers the four
//! remaining rebind forms: walrus (`:=`), `with … as X`, `except … as X`,
//! and `def X` / `class X`. For each: post-rebind references must resolve to
//! the LOCAL (not the import), and a re-import after the rebind must
//! RE-EMIT (restoring the imported value) instead of deduping to the stale
//! identity. Before the fix:
//!   * walrus left the cache intact → the re-import deduped (stale value);
//!   * a module-scope `with … as floor` assigned the immutable ESM import
//!     binding → runtime TypeError;
//!   * `def floor` / `class sqrt` after the import emitted a declaration
//!     beside the import binding → "Identifier … already declared".
//!
//! Behavioral tests run under node (skipped when node is unavailable); the
//! expected outputs are CPython's for the same programs.

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

fn compile(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen(&module)
}

/// Run `js` under node from a scratch dir; None when node is unavailable.
/// The runtime package is materialized into the dir so stdlib imports the
/// inline codegen still emits (`from math import floor` → `pyths-runtime`)
/// resolve — same harness as `behavioral_differential.rs`.
fn node_run(js: &str, name: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pyths_443_{}_{}", name, std::process::id()));
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
        return; // node unavailable → skip (same policy as dx_fixes.rs)
    };
    assert_eq!(
        got.trim(),
        expected,
        "{name}: output must match CPython\n--- JS ---\n{js}"
    );
}

// ── walrus (`:=`) ───────────────────────────────────────────────────────────

#[test]
fn walrus_rebind_module_scope_then_reimport() {
    // CPython: 111 then 2 — the re-import must restore math.floor, not
    // dedup to the walrus lambda.
    assert_behaves(
        "from math import floor\n\
         if (floor := (lambda x: 111)):\n    pass\n\
         print(floor(2.5))\n\
         from math import floor\n\
         print(floor(2.5))\n",
        "111\n2",
        "walrus_mod",
    );
}

#[test]
fn walrus_rebind_function_scope_then_reimport() {
    assert_behaves(
        "def f():\n\
         \x20   from math import floor\n\
         \x20   if (floor := (lambda v: 777)):\n\
         \x20       pass\n\
         \x20   r1 = floor(2.5)\n\
         \x20   from math import floor\n\
         \x20   return [r1, floor(2.5)]\n\
         print(f())\n",
        "[777, 2]",
        "walrus_fn",
    );
}

#[test]
fn walrus_rebind_invalidates_cache_structurally() {
    // The re-import must hoist a SECOND unique import (re-emission), not
    // dedup to the first.
    let js = compile(
        "from math import floor\n\
         if (floor := (lambda x: 111)):\n    pass\n\
         print(floor(2.5))\n\
         from math import floor\n\
         print(floor(2.5))\n",
    );
    assert!(
        js.matches("import { floor as __pyimp_floor_").count() >= 2,
        "re-import after a walrus rebind must re-emit, not dedup:\n{js}"
    );
}

// ── with … as X ─────────────────────────────────────────────────────────────

const CM_CLASS: &str = "class CM:\n\
    \x20   def __enter__(self):\n\
    \x20       return lambda x: 222\n\
    \x20   def __exit__(self, a, b, c):\n\
    \x20       return False\n";

#[test]
fn with_as_rebind_module_scope_then_reimport() {
    // Before the fix this threw "Assignment to constant variable" — the
    // `as floor` assignment landed on the immutable ESM import binding.
    let src = format!(
        "from math import floor\n{CM_CLASS}\
         with CM() as floor:\n\
         \x20   print(floor(2.5))\n\
         print(floor(2.5))\n\
         from math import floor\n\
         print(floor(2.5))\n"
    );
    assert_behaves(&src, "222\n222\n2", "with_mod");
}

#[test]
fn with_as_rebind_function_scope_then_reimport() {
    let src = "def f():\n\
         \x20   from math import floor\n\
         \x20   class CM2:\n\
         \x20       def __enter__(self):\n\
         \x20           return lambda x: 222\n\
         \x20       def __exit__(self, a, b, c):\n\
         \x20           return False\n\
         \x20   with CM2() as floor:\n\
         \x20       pass\n\
         \x20   r1 = floor(2.5)\n\
         \x20   from math import floor\n\
         \x20   return [r1, floor(2.5)]\n\
         print(f())\n";
    assert_behaves(src, "[222, 2]", "with_fn");
}

// ── except … as X ───────────────────────────────────────────────────────────

#[test]
fn except_as_rebind_module_scope() {
    // Inside the handler the name IS the exception; after the handler a
    // re-import restores the imported value (CPython deletes the name at
    // handler exit, so the re-import result is what's observable).
    assert_behaves(
        "from math import floor\n\
         try:\n\
         \x20   raise ValueError(\"boom\")\n\
         except ValueError as floor:\n\
         \x20   print(str(floor))\n\
         from math import floor\n\
         print(floor(2.5))\n",
        "boom\n2",
        "except_mod",
    );
}

#[test]
fn except_as_rebind_function_scope() {
    assert_behaves(
        "def g():\n\
         \x20   from math import floor\n\
         \x20   try:\n\
         \x20       raise ValueError(\"boom\")\n\
         \x20   except ValueError as floor:\n\
         \x20       return str(floor)\n\
         \x20   return \"no\"\n\
         def h():\n\
         \x20   from math import floor\n\
         \x20   try:\n\
         \x20       raise ValueError(\"boom2\")\n\
         \x20   except ValueError as floor:\n\
         \x20       pass\n\
         \x20   from math import floor\n\
         \x20   return floor(2.5)\n\
         print(g())\n\
         print(h())\n",
        "boom\n2",
        "except_fn",
    );
}

// ── def X / class X ─────────────────────────────────────────────────────────

#[test]
fn def_rebind_module_scope_then_reimport() {
    // Before the fix: `export function floor` beside the import binding —
    // "Identifier 'floor' has already been declared".
    assert_behaves(
        "from math import floor\n\
         def floor(x):\n\
         \x20   return 333\n\
         print(floor(2.5))\n\
         from math import floor\n\
         print(floor(2.5))\n",
        "333\n2",
        "def_mod",
    );
}

#[test]
fn def_rebind_function_scope_then_reimport() {
    assert_behaves(
        "def dd():\n\
         \x20   from math import floor\n\
         \x20   def floor(x):\n\
         \x20       return 333\n\
         \x20   r1 = floor(2.5)\n\
         \x20   from math import floor\n\
         \x20   return [r1, floor(2.5)]\n\
         print(dd())\n",
        "[333, 2]",
        "def_fn",
    );
}

#[test]
fn class_rebind_module_scope_then_reimport() {
    // The class shadows the import (instances work), and the re-import
    // restores math.floor — including the CALL lowering: `floor(2.5)` after
    // the re-import must be a plain call (2), not `new`-construction of the
    // shadowed class.
    assert_behaves(
        "from math import floor\n\
         class floor:\n\
         \x20   def val(self):\n\
         \x20       return 444\n\
         print(floor().val())\n\
         from math import floor\n\
         print(floor(2.5))\n",
        "444\n2",
        "class_mod",
    );
}

#[test]
fn class_rebind_function_scope_then_reimport() {
    assert_behaves(
        "def cc():\n\
         \x20   from math import floor\n\
         \x20   class floor:\n\
         \x20       def val(self):\n\
         \x20           return 444\n\
         \x20   r1 = floor().val()\n\
         \x20   from math import floor\n\
         \x20   return [r1, floor(2.5)]\n\
         print(cc())\n",
        "[444, 2]",
        "class_fn",
    );
}

// ── guards: the fix must not disturb ordinary defs/classes/withs ────────────

#[test]
fn plain_def_class_emission_unchanged() {
    // No import collision → first def/class keep the declaration form.
    let js = compile(
        "def foo(x):\n    return x\n\
         class Bar:\n    pass\n\
         print(foo(1))\n",
    );
    assert!(
        js.contains("export function foo("),
        "plain def must stay a declaration:\n{js}"
    );
    assert!(
        js.contains("export class Bar"),
        "plain class must stay a declaration:\n{js}"
    );
}

#[test]
fn stdlib_capwords_class_import_still_news() {
    // The known_classes drop only fires on a REBIND of an emitted class —
    // a fresh stdlib CapWords import (Counter) keeps `new`-lowering.
    let js = compile(
        "from collections import Counter\n\
         c = Counter([1, 2, 2])\n\
         print(c)\n",
    );
    assert!(
        js.contains("new Counter("),
        "fresh stdlib class import must keep new-lowering:\n{js}"
    );
}
