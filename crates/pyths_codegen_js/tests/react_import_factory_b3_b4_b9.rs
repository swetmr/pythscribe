//! B3/B4/B9 — the React createElement import/factory family (0.2.2 blockers).
//!
//! One integrated surface: whatever import spelling binds React's
//! `create_element`, a direct call must (B3) use the RESOLVED JS binding at the
//! call site and (B4) transform its props dict identically (`on_click` →
//! `onClick`, `aria_label` → `aria-label`). Plus (B9) the hybrid `pyths.react`
//! split must ESCAPE the (config-derivable) module specifier.
//!
//! Import spellings covered (all must behave the same):
//!   * `from react import create_element`            (unaliased — B3 root)
//!   * `from react import create_element as h`        (alias)
//!   * `from pyths.react import create_element`       (hybrid — B4 root)
//!   * `from pyths.react import create_element as h`  (hybrid alias)
//!   * `import react; react.create_element(...)`      (namespace member)
//! and the unsupported `import pyths.react as R` namespace form is diagnosed.

use std::collections::HashMap;

fn compile(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen(&module)
}

fn compile_errors(source: &str) -> Vec<String> {
    let module = pyths_parser::parse(source).expect("parse failed");
    let mut gen = pyths_codegen_js::JsCodegen::new();
    gen.emit_module(&module);
    gen.take_errors()
}

// ── B3: the call site uses the RESOLVED JS binding, not the raw Python name ───

#[test]
fn b3_unaliased_from_react_uses_resolved_binding() {
    let js = compile(
        "from react import create_element\n\
         x = create_element(\"button\", {\"on_click\": 1, \"aria_label\": \"g\"}, \"go\")\n",
    );
    assert!(
        js.contains("import { createElement } from \"react\""),
        "unaliased import binds createElement:\n{js}"
    );
    // The bug: call site emitted the raw `create_element(...)` → ReferenceError.
    assert!(
        js.contains("createElement(\"button\""),
        "call must use the resolved binding `createElement`, not `create_element`:\n{js}"
    );
    assert!(
        !js.contains("create_element(\"button\""),
        "the raw Python name must NOT survive at the call site:\n{js}"
    );
    // B4 for this spelling: props transformed.
    assert!(
        js.contains("\"onClick\":") && js.contains("\"aria-label\":"),
        "props dict must be transformed (onClick / aria-label):\n{js}"
    );
    assert!(
        !js.contains("on_click") && !js.contains("aria_label"),
        "no snake-case prop names may survive:\n{js}"
    );
}

#[test]
fn b3_aliased_from_react_still_works() {
    let js = compile(
        "from react import create_element as h\n\
         x = h(\"button\", {\"on_click\": 1}, \"go\")\n",
    );
    assert!(
        js.contains("import { createElement as h } from \"react\""),
        "alias binds `h`:\n{js}"
    );
    assert!(js.contains("h(\"button\""), "call uses the alias `h`:\n{js}");
    assert!(
        js.contains("\"onClick\":"),
        "aliased factory still transforms props:\n{js}"
    );
}

// ── B4: props transform on EVERY create-element import surface ─────────────────

#[test]
fn b4_hybrid_pyths_react_transforms_props() {
    // The bug: the hybrid `pyths.react` path returned early without registering
    // the factory binding, so the props dict was emitted verbatim (dead handler).
    let js = compile(
        "from pyths.react import create_element\n\
         x = create_element(\"button\", {\"on_click\": 1}, \"go\")\n",
    );
    assert!(
        js.contains("import { createElement } from \"react\""),
        "pyths.react routes create_element to the real react package:\n{js}"
    );
    assert!(
        js.contains("createElement(\"button\"") && js.contains("\"onClick\":"),
        "hybrid-import factory must transform props to onClick:\n{js}"
    );
    assert!(
        !js.contains("\"on_click\":"),
        "the verbatim snake-case prop must NOT survive (dead handler bug):\n{js}"
    );
}

#[test]
fn b4_hybrid_pyths_react_aliased_transforms_props() {
    let js = compile(
        "from pyths.react import create_element as h\n\
         x = h(\"button\", {\"on_click\": 1}, \"go\")\n",
    );
    assert!(
        js.contains("h(\"button\"") && js.contains("\"onClick\":"),
        "aliased hybrid-import factory must transform props:\n{js}"
    );
    assert!(!js.contains("\"on_click\":"), "no verbatim prop:\n{js}");
}

#[test]
fn b4_react_namespace_member_call_camelcases_and_transforms() {
    // `import react; react.create_element(...)` — the star namespace binds
    // `react.createElement`, so the raw member access is undefined AND props
    // were verbatim. Camel-case the attr and transform props.
    let js = compile(
        "import react\n\
         x = react.create_element(\"button\", {\"on_click\": 1}, \"go\")\n",
    );
    assert!(
        js.contains("react.createElement(\"button\""),
        "member call must use the real export `react.createElement`:\n{js}"
    );
    assert!(
        !js.contains("react.create_element"),
        "raw snake-case member access must NOT survive:\n{js}"
    );
    assert!(
        js.contains("\"onClick\":") && !js.contains("\"on_click\":"),
        "member-form props must be transformed:\n{js}"
    );
}

#[test]
fn b4_import_pyths_react_namespace_is_diagnosed() {
    // The hybrid surface can't be a single ESM namespace — diagnose instead of
    // emitting a dead `import * as R from "pyths-runtime/react"`.
    let errs = compile_errors(
        "import pyths.react as R\n\
         x = R.create_element(\"button\", {\"on_click\": 1}, \"go\")\n",
    );
    assert!(
        errs.iter().any(|e| e.contains("import pyths.react")
            && e.contains("not supported")),
        "namespace import of pyths.react must be diagnosed: {errs:?}"
    );
}

// ── 0.2.2 kwargs class fix: keyword props + literal-beside-spread dicts ────────
//
// The factory-call surface has ONE prop-key rule (write_react_prop_key):
// every statically-known key — a string-literal dict key OR a keyword name —
// is transformed; only the genuinely-dynamic parts (`**spread`, computed keys)
// stay verbatim (the real TB-1 boundary). Matrix: kwargs-form × spread-mix ×
// (name-bound factory | namespace member) × malformed-form diagnostics.

#[test]
fn kwargs_props_transform_on_every_factory_surface() {
    // (source, callee fragment expected in output)
    let cases: &[(&str, &str)] = &[
        (
            "from react import create_element as h\nx = h(\"div\", on_click=1)\n",
            "h(\"div\", {",
        ),
        (
            "from react import create_element\nx = create_element(\"div\", on_click=1)\n",
            "createElement(\"div\", {",
        ),
        (
            "from pyths.react import create_element as h\nx = h(\"div\", on_click=1)\n",
            "h(\"div\", {",
        ),
        (
            "import react\nx = react.create_element(\"div\", on_click=1)\n",
            "react.createElement(\"div\", {",
        ),
    ];
    for (src, callee) in cases {
        let js = compile(src);
        assert!(js.contains(callee), "kwargs form must reach the factory:\n{js}");
        assert!(
            js.contains("\"onClick\": 1"),
            "static kwarg key must transform (dead-handler class):\n{js}"
        );
        assert!(!js.contains("on_click"), "no snake prop may survive:\n{js}");
        let errs = compile_errors(src);
        assert!(errs.is_empty(), "well-formed kwargs must not diagnose: {errs:?}");
    }
}

#[test]
fn kwargs_children_follow_props_in_createelement_order() {
    // PSX-flat semantics: kwargs are the props, positionals after the tag are
    // children — emitted in React's createElement(type, props, ...children)
    // order.
    let js = compile(
        "from react import create_element as h\nx = h(\"div\", \"hi\", on_click=1)\n",
    );
    assert!(
        js.contains("h(\"div\", {\"onClick\": 1}, \"hi\")"),
        "children must follow the kwargs-props object:\n{js}"
    );
}

#[test]
fn kwargs_static_keys_transform_beside_spread() {
    // `**spread` in the kwargs form stays verbatim (dynamic boundary); the
    // static keys beside it still transform.
    let js = compile(
        "from react import create_element as h\nbase = {\"id\": \"a\"}\n\
         x = h(\"div\", on_click=1, **base)\n",
    );
    assert!(
        js.contains("\"onClick\": 1") && js.contains("...base"),
        "static kwargs transform, spread stays verbatim:\n{js}"
    );
    assert!(!js.contains("on_click"), "no snake prop survivor:\n{js}");
}

#[test]
fn positional_dict_literal_keys_transform_beside_spread() {
    // The literal-beside-spread positional form: `{"on_click": 1, **base}`
    // used to fall out of the props transform entirely (whole dict verbatim →
    // dead handler). Static keys now transform; the spread stays verbatim.
    for src in [
        "from react import create_element as h\nbase = {\"id\": \"a\"}\n\
         x = h(\"div\", {\"on_click\": 1, **base}, \"t\")\n",
        "import react\nbase = {\"id\": \"a\"}\n\
         x = react.create_element(\"div\", {\"on_click\": 1, **base}, \"t\")\n",
        "from react import create_element as h\nbase = {\"id\": \"a\"}\n\
         x = h(\"div\", {**base, \"on_click\": 1, \"aria_label\": \"g\"}, \"t\")\n",
    ] {
        let js = compile(src);
        assert!(
            js.contains("\"onClick\": 1") && js.contains("...base"),
            "static dict keys must transform beside a spread:\n{js}"
        );
        assert!(!js.contains("on_click"), "no snake prop survivor:\n{js}");
    }
}

#[test]
fn fully_dynamic_props_stay_verbatim_tb1_boundary() {
    // Spread-only dicts and non-dict props expressions have no statically
    // transformable keys — they stay verbatim (the genuine TB-1 boundary).
    let js = compile(
        "from react import create_element as h\nbase = {\"id\": \"a\"}\n\
         x = h(\"div\", {**base}, \"t\")\ny = h(\"div\", base, \"t\")\n",
    );
    assert!(
        !js.contains("\"onClick\""),
        "nothing to transform in fully-dynamic props:\n{js}"
    );
}

#[test]
fn kwargs_proto_key_is_proto_safe() {
    // A `__proto__=...` kwarg must emit a COMPUTED key — a quoted
    // `"__proto__":` in an object literal still triggers the prototype setter.
    let js = compile(
        "from react import create_element as h\nx = h(\"div\", __proto__=1)\n",
    );
    assert!(
        js.contains("[\"__proto__\"]: 1"),
        "__proto__ kwarg must be a computed key:\n{js}"
    );
}

#[test]
fn ambiguous_props_dict_plus_kwargs_is_diagnosed() {
    // Two competing props objects — silently picking one drops the other.
    for src in [
        "from react import create_element as h\nx = h(\"div\", {\"a\": 1}, on_click=1)\n",
        "import react\nx = react.create_element(\"div\", {\"a\": 1}, on_click=1)\n",
    ] {
        let errs = compile_errors(src);
        assert!(
            errs.iter().any(|e| e.contains("ambiguous") && e.contains("createElement")),
            "positional props dict + kwargs must be diagnosed: {errs:?}"
        );
    }
}

#[test]
fn kwargs_without_tag_is_diagnosed() {
    let errs = compile_errors(
        "from react import create_element as h\nx = h(on_click=1)\n",
    );
    assert!(
        errs.iter().any(|e| e.contains("createElement") && e.contains("first")),
        "keyword props with no tag must be diagnosed: {errs:?}"
    );
}

// ── B9: the hybrid split escapes the (config-derivable) module specifier ───────

#[test]
fn b9_hybrid_module_specifier_is_escaped() {
    // A malicious `[npm.imports]` override for `react` containing a quote must
    // be escaped in the emitted `import ... from "..."` — not interpolated raw
    // (which would let it break out of the specifier string).
    let mut npm = HashMap::new();
    npm.insert(
        "react".to_string(),
        "x\"; globalThis.PWNED = 1; import \"y".to_string(),
    );
    let module = pyths_parser::parse(
        "from pyths.react import create_element\n\
         x = create_element(\"b\", {\"on_click\": 1}, \"t\")\n",
    )
    .expect("parse");
    let js = pyths_codegen_js::codegen_with_npm_imports(&module, &npm);
    // The escaped specifier keeps the injected quote INSIDE the string literal.
    assert!(
        js.contains("globalThis.PWNED = 1")
            && !js.contains("import \"y\";")
            && !js.contains("\"; globalThis.PWNED = 1; import \"y\""),
        "the config-derived module specifier must be escaped, not break out:\n{js}"
    );
    // Specifically: no bare top-level `globalThis.PWNED` statement outside a string.
    assert!(
        !js.lines().any(|l| l.trim_start().starts_with("globalThis.PWNED")),
        "the override must not escape into a top-level statement:\n{js}"
    );
}
