//! WB-19 — `pyths compile --dts` must declare a `@component`'s parameters as the
//! SINGLE destructured props object that the JS emitter actually produces, not
//! as positional params.
//!
//! For `@component def ZipList(names, nums): ...` the JS emitter lowers to
//! `export function ZipList({names, nums} = {})` — React calls it with one props
//! object. The old `.d.ts` declared `ZipList(names: any, nums: any): JSX.Element`
//! (positional), which TS rejects as a JSX component (TS2786, "too few
//! arguments") for any component with >=2 params, and is prop-name-unsound even
//! at arity 1. The declaration must match the emitted calling convention:
//! `ZipList(props: { names: any; nums: any }): JSX.Element`.
//!
//! Scope: ONLY `@component`. `@psx` helpers keep positional params in the JS
//! (the props-destructure is gated on `is_component`), so their positional
//! declaration already matches and must stay unchanged; ordinary functions are
//! untouched.

fn dts(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_dts(&module)
}

#[test]
fn component_zero_params_unchanged() {
    let d = dts("@component\ndef Zero():\n    return ul(child=\"x\")\n");
    assert!(
        d.contains("export declare function Zero(): JSX.Element;"),
        "0-param component decl wrong:\n{d}"
    );
}

#[test]
fn component_one_param_is_props_object() {
    let d = dts("@component\ndef One(p):\n    return ul(child=\"x\")\n");
    assert!(
        d.contains("export declare function One(props: { p: any }): JSX.Element;"),
        "1-param component decl must be a props object:\n{d}"
    );
    // And must NOT be the old positional form.
    assert!(
        !d.contains("function One(p: any)"),
        "1-param component decl still positional:\n{d}"
    );
}

#[test]
fn component_two_params_is_props_object() {
    let d = dts("@component\ndef ZipList(names, nums):\n    return ul(child=\"x\")\n");
    assert!(
        d.contains(
            "export declare function ZipList(props: { names: any; nums: any }): JSX.Element;"
        ),
        "2-param component decl must be a props object:\n{d}"
    );
    // The old positional form (the TS2786 root cause) must be gone.
    assert!(
        !d.contains("function ZipList(names: any, nums: any)"),
        "2-param component decl still positional:\n{d}"
    );
}

#[test]
fn component_defaulted_param_is_optional_member() {
    // A defaulted prop is optional in the destructured object (`{a, b = 1}`),
    // so the props-object member is optional (`b?`).
    let d = dts("@component\ndef WithDefault(a, b=1):\n    return ul(child=\"x\")\n");
    assert!(
        d.contains(
            "export declare function WithDefault(props: { a: any; b?: any }): JSX.Element;"
        ),
        "defaulted component prop must be optional in the props object:\n{d}"
    );
}

#[test]
fn component_rest_kwargs_adds_index_signature() {
    // `**rest` destructures leftover props (`{title, ...rest} = {}`), so the
    // props object carries an open index signature alongside the named member.
    let d = dts("@component\ndef RestProps(title, **rest):\n    return ul(child=\"x\")\n");
    assert!(
        d.contains(
            "export declare function RestProps(props: { title: any; [key: string]: any }): JSX.Element;"
        ),
        "component with **rest must add an index signature:\n{d}"
    );
}

#[test]
fn component_whole_props_param_unchanged() {
    // A single param literally named `props` binds the whole object in the JS
    // (`function C(props)`), which is already a single-object convention and
    // JSX-callable — kept as one param.
    let d = dts("@component\ndef WholeProps(props):\n    return ul(child=\"x\")\n");
    assert!(
        d.contains("export declare function WholeProps(props: any): JSX.Element;"),
        "whole-props component decl should stay a single object param:\n{d}"
    );
}

#[test]
fn psx_helper_stays_positional() {
    // `@psx` uses positional params in the emitted JS (`function PsxHelper(a, b)`),
    // so its declaration must stay positional to match the calling convention —
    // it is NOT rewritten to a props object.
    let d = dts("@psx\ndef PsxHelper(a, b):\n    return ul(child=\"x\")\n");
    assert!(
        d.contains("export declare function PsxHelper(a: any, b: any)"),
        "@psx helper must keep positional params:\n{d}"
    );
    assert!(
        !d.contains("PsxHelper(props:"),
        "@psx helper must NOT be rewritten to a props object:\n{d}"
    );
}

#[test]
fn ordinary_function_decl_unchanged() {
    // An ordinary (non-component) function keeps positional params and its
    // ordinary return type — the fix must not touch it.
    let d = dts("def ordinary(a, b):\n    return a + b\n");
    assert!(
        d.contains("export declare function ordinary(a: any, b: any): any;"),
        "ordinary function decl must be unchanged:\n{d}"
    );
}
