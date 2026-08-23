//! B8 (0.2.2 batch 2) — the DX-B2 snake→camel import-rename lane must be
//! BINDING-AWARE. Two silent miscompiles:
//!
//!   (a) a function PARAM/LOCAL named like a snake_case lib import
//!       (`def f(create_store): return create_store`) had its references
//!       renamed to the import's camel binding (`return createStore`) —
//!       returning the IMPORT instead of the argument;
//!   (b) a MODULE-level user binding of the manufactured camel name
//!       (`createStore = "local"` beside `from zustand import
//!       create_store`) collided with the import's JS binding —
//!       "Identifier 'createStore' has already been declared".
//!
//! CLASS rules:
//!   (a) the import→camel rename applies ONLY to references that resolve to
//!       the import — a param/local binding of the Python name in any
//!       enclosing function scope shadows it (same predicate as the
//!       import_ref_renames guard; applied at BOTH rename sites:
//!       emit_expr's Name branch and resolve_name_ref);
//!   (b) a conversion-manufactured JS name that collides with ANY
//!       module-level user binding routes through the DX-B2
//!       alias-and-rewrite lane (unique hoist + reference rewrite).
//!
//! Behavioral tests stub the lib module in node_modules (zustand isn't
//! installable offline); skipped when node is unavailable.

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

/// Run `js` under node with a stubbed `zustand` module whose `createStore`
/// returns the tag "\<import createStore\>" so tests can tell the import
/// apart from user values.
fn node_run(js: &str, name: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pyths_b8_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    pyths_runtime::materialize_runtime_package(&dir).expect("materialize runtime");
    let zdir = dir.join("node_modules").join("zustand");
    std::fs::create_dir_all(&zdir).unwrap();
    std::fs::write(
        zdir.join("package.json"),
        r#"{"name":"zustand","version":"0.0.0","type":"module","main":"index.js"}"#,
    )
    .unwrap();
    std::fs::write(
        zdir.join("index.js"),
        "export const createStore = (..._a) => \"<import createStore>\";\n",
    )
    .unwrap();
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
        return; // node unavailable → skip
    };
    assert_eq!(
        got.trim(),
        expected,
        "{name}: output must match CPython scoping\n--- JS ---\n{js}"
    );
}

// ── (a) param/local shadow — references resolve to the binding ──────────────

#[test]
fn param_shadow_returns_argument() {
    assert_behaves(
        "from zustand import create_store\n\
         def f(create_store):\n    return create_store\n\
         print(f(\"arg\"))\nprint(create_store())\n",
        "arg\n<import createStore>",
        "param_shadow",
    );
}

#[test]
fn local_shadow_snake_name() {
    // A local of the import's Python name shadows for the WHOLE function.
    assert_behaves(
        "from zustand import create_store\n\
         def f():\n    create_store = \"local\"\n    return create_store\n\
         print(f())\nprint(create_store())\n",
        "local\n<import createStore>",
        "local_shadow",
    );
}

#[test]
fn nested_scope_shadow() {
    assert_behaves(
        "from zustand import create_store\n\
         def outer():\n\
         \x20   def inner(create_store):\n\
         \x20       return create_store\n\
         \x20   return inner(\"x\")\n\
         print(outer())\nprint(create_store())\n",
        "x\n<import createStore>",
        "nested_shadow",
    );
}

#[test]
fn enclosing_scope_shadow_closes_over_local() {
    // The binding lives in the ENCLOSING function; the inner reference
    // closes over it, not the import.
    assert_behaves(
        "from zustand import create_store\n\
         def outer(create_store):\n\
         \x20   def inner():\n\
         \x20       return create_store\n\
         \x20   return inner()\n\
         print(outer(\"closed\"))\n",
        "closed",
        "enclosing_shadow",
    );
}

#[test]
fn genuine_import_ref_still_renames() {
    // No shadow anywhere: the reference must still camel-rename.
    assert_behaves(
        "from zustand import create_store\n\
         store = create_store(lambda: {})\nprint(store)\n",
        "<import createStore>",
        "genuine_ref",
    );
}

#[test]
fn genuine_ref_inside_unshadowed_function() {
    assert_behaves(
        "from zustand import create_store\n\
         def make():\n    return create_store(lambda: {})\n\
         print(make())\n",
        "<import createStore>",
        "genuine_fn_ref",
    );
}

// ── (b) module-level camel collision — alias-and-rewrite ───────────────────

#[test]
fn module_camel_local_coexists_with_import() {
    // User binds the manufactured camel name at module level; the import
    // hoists under a unique name and BOTH resolve correctly.
    assert_behaves(
        "from zustand import create_store\n\
         createStore = \"local\"\n\
         print(createStore)\nprint(create_store())\n",
        "local\n<import createStore>",
        "mod_camel",
    );
}

#[test]
fn module_camel_def_coexists_with_import() {
    // A module `def` of the camel name is the same collision class.
    assert_behaves(
        "from zustand import create_store\n\
         def createStore():\n    return \"user def\"\n\
         print(createStore())\nprint(create_store())\n",
        "user def\n<import createStore>",
        "mod_camel_def",
    );
}

#[test]
fn module_camel_collision_structural() {
    let js = compile_inline(
        "from zustand import create_store\ncreateStore = \"local\"\nprint(create_store())\n",
    );
    assert!(
        js.contains("createStore as __pyimp_createStore_"),
        "colliding import must hoist under a unique alias:\n{js}"
    );
    assert!(
        js.contains("__pyimp_createStore_0()"),
        "the import's Python-name reference sites must rewrite to the alias:\n{js}"
    );
}
