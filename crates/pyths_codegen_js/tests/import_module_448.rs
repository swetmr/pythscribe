//! #448 — dynamic import via `import_module(spec)` (importlib.import_module).
//!
//! Lowers to native ES dynamic `import(spec)` so `.ps` can express
//! code-splitting / lazy-load. Three surfaces must all reach the native form:
//!   * bare `import_module(...)` (a pyths builtin, no import statement);
//!   * `from importlib import import_module` (unaliased);
//!   * `from importlib import import_module as im` (aliased).
//! Documented deviation: importlib.import_module is synchronous; ES `import()`
//! returns a Promise → the `.ps` form must be `await`ed.
//!
//! Structural tests assert the emitted `import(...)`; a behavioral test runs a
//! real dynamic import + module-namespace subscript under node (skipped when
//! node is unavailable, same policy as import_rebind_443.rs).

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

// ── structural: all three surfaces lower to native `import(...)` ─────────────

#[test]
fn bare_import_module_builtin_lowers_to_native_import() {
    // No import statement at all — the pyths builtin.
    let js = compile_inline("m = await import_module(\"./x.js\")\n");
    assert!(
        js.contains("await import(\"./x.js\")"),
        "bare import_module must lower to native dynamic import:\n{js}"
    );
    assert!(
        !js.contains("import_module"),
        "no `import_module` identifier should survive in the output:\n{js}"
    );
}

#[test]
fn from_importlib_unaliased_lowers_to_native_import() {
    let js = compile_inline(
        "from importlib import import_module\nm = await import_module(\"./x.js\")\n",
    );
    assert!(
        js.contains("await import(\"./x.js\")"),
        "`from importlib import import_module` must lower to native import:\n{js}"
    );
    // The importlib import itself emits nothing (no broken specifier).
    assert!(
        !js.contains("importlib"),
        "the importlib import must emit nothing:\n{js}"
    );
}

#[test]
fn from_importlib_aliased_lowers_to_native_import() {
    let js =
        compile_inline("from importlib import import_module as im\nm = await im(\"./x.js\")\n");
    assert!(
        js.contains("await import(\"./x.js\")"),
        "aliased import_module must lower to native import:\n{js}"
    );
}

#[test]
fn fstring_spec_lowers_to_template_import() {
    let js = compile_inline("name = \"widget\"\nm = await import_module(f\"./{name}.js\")\n");
    // f-string → template literal argument inside import(...).
    assert!(
        js.contains("import(`./${") && js.contains(".js`)"),
        "computed/f-string spec must lower to import(`...`):\n{js}"
    );
}

#[test]
fn local_shadow_wins_over_import_module_alias() {
    // A local binding named `im` must call THAT, not the native import.
    let js = compile_inline(
        "from importlib import import_module as im\n\
         def im(x):\n    return 7\n\
         r = im(\"./x.js\")\n",
    );
    assert!(
        !js.contains("import(\"./x.js\")"),
        "a local `im` must shadow the importlib alias:\n{js}"
    );
}

// ── #448 diagnostics: the three wrong-code-with-no-diagnostic surfaces ────────

fn compile_errors(source: &str) -> Vec<String> {
    let module = pyths_parser::parse(source).expect("parse failed");
    let mut gen = pyths_codegen_js::JsCodegen::new();
    gen.emit_module(&module);
    gen.take_errors()
}

#[test]
fn import_module_value_position_is_compile_error() {
    // `f = import_module` — the native `import(...)` form is not a JS value.
    let errs = compile_errors("from importlib import import_module\nf = import_module\n");
    assert!(
        errs.iter()
            .any(|e| e.contains("import_module") && e.contains("cannot be used as a value")),
        "value-position import_module must be diagnosed: {errs:?}"
    );
}

#[test]
fn import_module_package_kwarg_is_compile_error() {
    // `import_module("mod", package="pkg")` — no native relative-import anchor.
    let errs = compile_errors("m = import_module(\"mod\", package=\"pkg\")\n");
    assert!(
        errs.iter()
            .any(|e| e.contains("import_module") && e.contains("keyword")),
        "package= kwarg must be diagnosed: {errs:?}"
    );
}

#[test]
fn importlib_member_call_is_compile_error() {
    // `import importlib; importlib.import_module("mod")` — the member form has
    // no valid lowering (importlib is not a real module in the output).
    let errs = compile_errors("import importlib\nm = importlib.import_module(\"mod\")\n");
    assert!(
        errs.iter()
            .any(|e| e.contains("import_module") && e.contains("member form")),
        "importlib.import_module member call must be diagnosed: {errs:?}"
    );
}

// ── 0.2.2 CLASS rule: ANY reference to a tracked importlib namespace ──────────
//
// `import importlib` emits no binding, so the old code diagnosed only the
// `.import_module(...)` member-call shape — every OTHER reference (bare
// `importlib`, `importlib.reload(x)`, `f = importlib.import_module`,
// `importlib.util...`) emitted an unbound identifier: a ReferenceError with no
// diagnostic, WORSE than pre-fix. One rule at the Name emission (the root of
// every reference form) closes the whole class.

#[test]
fn importlib_reference_forms_matrix_all_diagnosed() {
    let cases: &[&str] = &[
        // member call, non-import_module attr
        "import importlib\nimportlib.reload(x)\n",
        // aliased namespace, member call
        "import importlib as il\nil.reload(x)\n",
        // bare reference (value position)
        "import importlib\nprint(importlib)\n",
        // bound member reference (value position — the dead pyBoundMethod shape)
        "import importlib\nf = importlib.import_module\n",
        // aliased bound member reference
        "import importlib as il\nf = il.import_module\n",
        // dotted submodule import + deep member chain
        "import importlib.util\ns = importlib.util.find_spec(\"m\")\n",
        // aliased submodule import
        "import importlib.machinery as m\nl = m.SourceFileLoader(\"a\", \"b\")\n",
        // attribute read
        "import importlib\nv = importlib.reload\n",
    ];
    for src in cases {
        let errs = compile_errors(src);
        assert!(
            errs.iter()
                .any(|e| e.contains("importlib") || e.contains("import_module")),
            "every importlib-namespace reference must be diagnosed ({src:?}): {errs:?}"
        );
    }
}

#[test]
fn importlib_submodule_from_imports_are_diagnosed() {
    // `from importlib.util import ...` — import_module lives at the importlib
    // TOP level only; submodule from-imports must not emit a broken specifier.
    for src in [
        "from importlib.util import find_spec\n",
        "from importlib.machinery import SourceFileLoader\n",
        // import_module is NOT importable from a submodule
        "from importlib.util import import_module\n",
    ] {
        let errs = compile_errors(src);
        assert!(
            errs.iter().any(|e| e.contains("not supported")),
            "importlib submodule from-import must be diagnosed ({src:?}): {errs:?}"
        );
    }
}

#[test]
fn importlib_shadowing_local_wins_over_diagnostic() {
    // A user binding that shadows the tracked name is a plain local — the
    // class rule must not misfire on it.
    let errs = compile_errors("import importlib\nimportlib = 5\nprint(importlib + 1)\n");
    assert!(
        errs.is_empty(),
        "a shadowing local must win over the importlib diagnostic: {errs:?}"
    );
}

#[test]
fn supported_import_module_forms_still_have_no_errors() {
    // Regression guard: the working forms must NOT be diagnosed.
    for src in [
        "m = await import_module(\"./x.js\")\n",
        "from importlib import import_module\nm = await import_module(\"./x.js\")\n",
        "from importlib import import_module as im\nm = await im(\"./x.js\")\n",
    ] {
        let errs = compile_errors(src);
        assert!(
            errs.is_empty(),
            "supported form wrongly diagnosed ({src:?}): {errs:?}"
        );
    }
}

// ── behavioral: real dynamic import + module-namespace subscript ─────────────

#[test]
fn dynamic_import_and_namespace_subscript_under_node() {
    // Top-level await (ESM) — no asyncio import needed.
    let js = compile_inline(
        "name = \"greeter\"\n\
         m = await import_module(f\"./{name}.mjs\")\n\
         fn = m[name]\n\
         print(fn())\n",
    );
    let dir = std::env::temp_dir().join(format!("pyths_448_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    pyths_runtime::materialize_runtime_package(&dir).expect("materialize runtime");
    // the lazily-imported module: a named export `greeter`.
    std::fs::write(
        dir.join("greeter.mjs"),
        "export function greeter() { return \"hello from greeter\"; }\n",
    )
    .unwrap();
    let path = dir.join("main.mjs");
    std::fs::write(&path, &js).unwrap();
    let Ok(out) = Command::new("node").arg(&path).output() else {
        let _ = std::fs::remove_dir_all(&dir);
        return; // node unavailable → skip
    };
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let ok = out.status.success();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ok, "node failed:\n{stderr}\n--- JS ---\n{js}");
    assert_eq!(
        stdout.trim(),
        "hello from greeter",
        "dynamic import + namespace subscript must work\n--- JS ---\n{js}"
    );
}
