//! 0.2.2 member-call CLASS rule — the full matrix, not a single shape.
//!
//! For a member access `<base>.<member>` where `<base>` is a core-React
//! namespace alias (`import react [as R]`, `import react_dom [as D]`,
//! `import react_dom.client as C`), EVERY member — in call position AND value
//! position, any arity — must route through ONE rule
//! (`react::route_namespace_member`): removed-in-19 check first, then
//! camel-casing + module check against the audited `REACT_HELPER_TABLE`, or a
//! compile diagnostic. The original fix special-cased `create_element` with
//! ≥2 args in call position — validating its own shape — so every ADJACENT
//! member (`react.use_state`, `react.clone_element`, `react_dom.create_portal`,
//! single-arg `create_element`, value-position references) compiled to a
//! silent-dead snake identifier or a dead `pyBoundMethod` wrap.
//!
//! Minimum bar asserted here: NO react-namespace-alias member may compile to a
//! silent-dead snake identifier — it routes correctly or it is a compile error.

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

fn assert_error_free(source: &str) {
    let errs = compile_errors(source);
    assert!(errs.is_empty(), "unexpected diagnostics for {source:?}: {errs:?}");
}

// ── ROUTED: every base × member × call form emits the camelCase export ────────

#[test]
fn routed_member_calls_matrix() {
    // (import stmt, base, snake member, args, expected camel member)
    let cases: &[(&str, &str, &str, &str, &str)] = &[
        // react core — plain and aliased base
        ("import react", "react", "use_state", "(0)", "useState"),
        ("import react as R", "R", "use_state", "(0)", "useState"),
        ("import react", "react", "clone_element", "(el)", "cloneElement"),
        ("import react as R", "R", "clone_element", "(el)", "cloneElement"),
        ("import react", "react", "use_callback", "(f, [])", "useCallback"),
        ("import react", "react", "is_valid_element", "(el)", "isValidElement"),
        ("import react", "react", "create_context", "(None)", "createContext"),
        ("import react", "react", "start_transition", "(f)", "startTransition"),
        // the B4 gap shapes: SINGLE-arg create_element (the old arm required ≥2)
        ("import react", "react", "create_element", "(\"div\")", "createElement"),
        ("import react as R", "R", "create_element", "(\"div\")", "createElement"),
        // react-dom — plain and aliased base
        ("import react_dom", "react_dom", "create_portal", "(el, node)", "createPortal"),
        ("import react_dom as D", "D", "create_portal", "(el, node)", "createPortal"),
        ("import react_dom", "react_dom", "flush_sync", "(f)", "flushSync"),
        ("import react_dom as D", "D", "flush_sync", "(f)", "flushSync"),
        ("import react_dom", "react_dom", "use_form_status", "()", "useFormStatus"),
        // react-dom/client (aliased dotted namespace import)
        ("import react_dom.client as RC", "RC", "create_root", "(node)", "createRoot"),
        ("import react_dom.client as RC", "RC", "hydrate_root", "(node, el)", "hydrateRoot"),
    ];
    for (imp, base, member, args, camel) in cases {
        let src = format!("{imp}\nel = 1\nnode = 2\nf = 3\nx = {base}.{member}{args}\n");
        let js = compile(&src);
        assert!(
            js.contains(&format!("{base}.{camel}(")),
            "{base}.{member}{args} must emit {base}.{camel}(...):\n{js}"
        );
        assert!(
            !js.contains(&format!(".{member}")),
            "the snake member .{member} must NOT survive (silent-dead class bar):\n{js}"
        );
        assert_error_free(&src);
    }
}

#[test]
fn routed_member_value_position_matrix() {
    // Bound references — the old lowering emitted a dead
    // `pyBoundMethod(react, "create_element")`. ESM namespace members are
    // plain functions: resolve to the camelCase export, no wrap.
    let cases: &[(&str, &str, &str, &str)] = &[
        ("import react", "react", "create_element", "createElement"),
        ("import react as R", "R", "create_element", "createElement"),
        ("import react", "react", "use_state", "useState"),
        ("import react", "react", "clone_element", "cloneElement"),
        ("import react_dom", "react_dom", "create_portal", "createPortal"),
        ("import react_dom as D", "D", "flush_sync", "flushSync"),
        ("import react_dom.client as RC", "RC", "create_root", "createRoot"),
        // Capitalized re-export in value position passes through unchanged.
        ("import react", "react", "Fragment", "Fragment"),
    ];
    for (imp, base, member, camel) in cases {
        let src = format!("{imp}\ng = {base}.{member}\n");
        let js = compile(&src);
        assert!(
            js.contains(&format!("{base}.{camel}")),
            "value-position {base}.{member} must resolve to {base}.{camel}:\n{js}"
        );
        assert!(
            !js.contains("pyBoundMethod"),
            "namespace member reference must NOT be pyBoundMethod-wrapped:\n{js}"
        );
        assert!(
            !js.contains(&format!(".{member}")) || member == &camel.to_string(),
            "snake member .{member} must not survive in value position:\n{js}"
        );
        assert_error_free(&src);
    }
}

#[test]
fn camel_spellings_pass_through() {
    // Already-camel members route unchanged (identity), still module-checked.
    for (imp, base, member, args) in [
        ("import react", "react", "useState", "(0)"),
        ("import react", "react", "createElement", "(\"div\")"),
        ("import react_dom", "react_dom", "createPortal", "(1, 2)"),
        ("import react_dom.client as RC", "RC", "createRoot", "(1)"),
    ] {
        let src = format!("{imp}\nx = {base}.{member}{args}\n");
        let js = compile(&src);
        assert!(
            js.contains(&format!("{base}.{member}(")),
            "camel member must pass through:\n{js}"
        );
        assert_error_free(&src);
    }
}

#[test]
fn unknown_members_get_from_import_parity_camel() {
    // Unknown symbols cannot be module-checked; they get the SAME snake→camel
    // the from-import path applies (`from react import use_whatever` binds
    // useWhatever) — never a silent-dead snake member.
    let js = compile("import react\nx = react.use_whatever_custom(1)\n");
    assert!(
        js.contains("react.useWhateverCustom(1)"),
        "unknown member must camel like the from-import path:\n{js}"
    );
    assert!(!js.contains("use_whatever_custom"), "no snake survivor:\n{js}");
    // Underscore-free unknown members are identity (react-dom 19 resource API).
    let js = compile("import react_dom\nreact_dom.preload(\"/x.css\")\n");
    assert!(js.contains("react_dom.preload("), "identity member:\n{js}");
}

// ── DIAGNOSED: wrong module — the export genuinely is not on that namespace ───

#[test]
fn wrong_module_members_are_compile_errors() {
    // (import, base, member, expected fragments in the diagnostic)
    let cases: &[(&str, &str, &str, &[&str])] = &[
        ("import react", "react", "create_portal", &["createPortal", "react-dom"]),
        ("import react", "react", "createPortal", &["createPortal", "react-dom"]),
        ("import react", "react", "create_root", &["createRoot", "react-dom/client"]),
        ("import react as R", "R", "create_portal", &["createPortal", "react-dom"]),
        ("import react_dom", "react_dom", "use_state", &["useState", "\"react\""]),
        ("import react_dom", "react_dom", "create_element", &["createElement", "\"react\""]),
        ("import react_dom as D", "D", "clone_element", &["cloneElement", "\"react\""]),
        ("import react_dom.client as RC", "RC", "create_portal", &["createPortal", "react-dom"]),
        // the pyths meta-helpers are not React exports at all
        ("import react", "react", "component", &["pyths-runtime/react"]),
    ];
    for (imp, base, member, frags) in cases {
        // call position
        let errs = compile_errors(&format!("{imp}\nx = {base}.{member}(1)\n"));
        for frag in *frags {
            assert!(
                errs.iter().any(|e| e.contains(frag)),
                "call {base}.{member}: diagnostic must mention {frag}: {errs:?}"
            );
        }
        // value position — same rule, same diagnostic
        let errs = compile_errors(&format!("{imp}\ng = {base}.{member}\n"));
        for frag in *frags {
            assert!(
                errs.iter().any(|e| e.contains(frag)),
                "value {base}.{member}: diagnostic must mention {frag}: {errs:?}"
            );
        }
    }
}

// ── DIAGNOSED: removed in React 19 — on every base, both spellings, both forms ─

#[test]
fn react_19_removed_members_are_compile_errors() {
    let cases: &[(&str, &str, &str, &str)] = &[
        ("import react_dom", "react_dom", "render", "render"),
        ("import react_dom as D", "D", "render", "render"),
        ("import react_dom", "react_dom", "hydrate", "hydrate"),
        ("import react_dom", "react_dom", "find_dom_node", "findDOMNode"),
        ("import react_dom", "react_dom", "findDOMNode", "findDOMNode"),
        ("import react_dom", "react_dom", "unmount_component_at_node", "unmountComponentAtNode"),
        ("import react", "react", "create_factory", "createFactory"),
        ("import react", "react", "createFactory", "createFactory"),
        // removed symbols diagnose on the WRONG base too (removed everywhere)
        ("import react", "react", "render", "render"),
    ];
    for (imp, base, member, js_name) in cases {
        for src in [
            format!("{imp}\nx = {base}.{member}(1)\n"), // call
            format!("{imp}\ng = {base}.{member}\n"),    // value
        ] {
            let errs = compile_errors(&src);
            assert!(
                errs.iter().any(|e| e.contains(js_name) && e.contains("React 19")),
                "{base}.{member} must be diagnosed as removed in React 19: {errs:?}"
            );
        }
    }
}

// ── from-import surface: removed set is complete AND correctly scoped ─────────

#[test]
fn removed_from_imports_diagnosed_on_core_packages() {
    let cases: &[(&str, &str)] = &[
        ("from react_dom import render", "render"),
        ("from react_dom import hydrate", "hydrate"),
        ("from react_dom import find_dom_node", "findDOMNode"),
        ("from react_dom import unmount_component_at_node", "unmountComponentAtNode"),
        ("from react import create_factory", "createFactory"),
        ("from pyths.react import render", "render"),
        ("from pyths.react import create_factory", "createFactory"),
    ];
    for (imp, js_name) in cases {
        let errs = compile_errors(&format!("{imp}\n"));
        assert!(
            errs.iter().any(|e| e.contains(js_name) && e.contains("React 19")),
            "{imp}: must be diagnosed as removed: {errs:?}"
        );
    }
}

#[test]
fn removed_check_does_not_misfire_on_ecosystem_packages() {
    // `render` is a REAL export of @testing-library/react (and other
    // ecosystem packages). The removed diagnostic is scoped to the core React
    // packages — a symbol-keyed global check would break these imports.
    for src in [
        "from at_testing_library.react import render\n",
        "from react_markdown import render\n",
    ] {
        let errs = compile_errors(src);
        assert!(
            errs.is_empty(),
            "ecosystem import must NOT trip the React-19-removed check ({src:?}): {errs:?}"
        );
    }
}

// ── ecosystem namespace aliases: from-import parity camel, no pyBoundMethod ───

#[test]
fn ecosystem_namespace_members_camel_like_from_imports() {
    let js = compile(
        "import react_router_dom as rrd\nrouter = rrd.create_browser_router([])\n",
    );
    assert!(
        js.contains("rrd.createBrowserRouter("),
        "ecosystem member call must camel like the from-import path:\n{js}"
    );
    assert!(!js.contains("create_browser_router"), "no snake survivor:\n{js}");

    let js = compile("import react_router_dom as rrd\nnav = rrd.use_navigate\n");
    assert!(
        js.contains("rrd.useNavigate") && !js.contains("pyBoundMethod"),
        "ecosystem member reference must camel without pyBoundMethod:\n{js}"
    );
}

// ── shadowing: a rebound base name wins over the namespace routing ─────────────

#[test]
fn create_element_member_props_still_transform() {
    // The original B4 shape stays covered by the uniform rule: ≥2-arg member
    // create_element transforms its props dict.
    let js = compile(
        "import react\nx = react.create_element(\"button\", {\"on_click\": 1}, \"go\")\n",
    );
    assert!(
        js.contains("react.createElement(\"button\"") && js.contains("\"onClick\":"),
        "member create_element props must transform:\n{js}"
    );
    assert!(!js.contains("on_click"), "no snake prop survivor:\n{js}");
}
