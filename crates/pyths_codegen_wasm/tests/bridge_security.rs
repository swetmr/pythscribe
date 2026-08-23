//! SECURITY regression: WASM-bridge codegen injection / invalid-identifier
//! cluster (Codex-Security scan 2026-08-12, findings #4-bridge and #13-bridge).
//!
//! - #4 (CWE-94/116): the artifact filename is spliced into a
//!   `new URL('<file>', import.meta.url)` string in every loader. A `'` or
//!   newline in the output-stem-derived name broke out of the literal and
//!   injected JS at module scope. Fix: single-quoted JS string serializer.
//! - #13 (CWE-116): WASM function parameter names are emitted raw in JS binding
//!   positions. A reserved-word param (`default`, `new`, `in`, ...) produced
//!   `export function f(default, new)` — a SyntaxError. Fix: sanitize each JS
//!   binding name while preserving the Python names in `__pyparams__`.
//!
//! Oracle: `node --check` over the emitted glue rejects a string breakout or a
//! reserved-word binding as a SyntaxError; string assertions pin that the
//! hostile substring is escaped, not raw. `node` unavailable → skip that half.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use pyths_codegen_wasm::bridge::BridgeTarget;
use pyths_codegen_wasm::types::WasmType;
use pyths_codegen_wasm::{
    generate_bridge_for_target, generate_bridge_js, WasmCodegenOutput, WasmExportInfo,
};

fn make_output(exports: Vec<WasmExportInfo>) -> WasmCodegenOutput {
    WasmCodegenOutput {
        wasm: vec![0x00],
        compiled_functions: exports.iter().map(|e| e.name.clone()).collect(),
        rejected_functions: vec![],
        export_info: exports,
        math_imports: BTreeSet::new(),
        needs_strings: false,
        needs_errors: false,
        needs_dicts: false,
        custom_exceptions: BTreeMap::new(),
        has_ovf: false,
    }
}

/// `node --check` the emitted glue as an ES module. `import.meta` is only legal
/// in a module, so write a `.mjs`. Skips (Ok) when node is unavailable.
fn node_check(js: &str, name: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("pyths_bridgesec_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{name}.mjs"));
    std::fs::write(&path, js).unwrap();
    let out = match Command::new("node").arg("--check").arg(&path).output() {
        Ok(o) => o,
        Err(_) => {
            eprintln!("node unavailable — skipping node --check for {name}");
            return Ok(());
        }
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
        panic!("emitted glue `{name}` is not valid JS (injection/breakout):\n{e}\n--- JS ---\n{js}");
    }
}

// ── Finding #4: hostile artifact filename ──

#[test]
fn sec4_wasm_filename_breakout_all_loaders() {
    let hostile = "a.wasm', import.meta.url); globalThis.PWNED=1; new URL('b.wasm";
    let o = make_output(vec![WasmExportInfo {
        name: "f".to_string(),
        params: vec![("x".to_string(), WasmType::F64)],
        return_type: Some(WasmType::F64),
    }]);
    // Browser, WASI, and Deno loaders all use `new URL('<file>', ...)`.
    for target in [BridgeTarget::Browser, BridgeTarget::Wasi, BridgeTarget::Deno] {
        let g = generate_bridge_for_target(target, hostile, &o);
        // Primary oracle: the whole module must remain valid JS. A breakout
        // would make the injected `globalThis.PWNED=1` top-level and the
        // dangling `new URL('b.wasm` an unterminated-string SyntaxError.
        assert_valid(&g, &format!("sec4_filename_{target:?}"));
        // The single quotes in the payload must be escaped (`\'`), proving the
        // payload stayed confined to the string literal rather than closing it.
        assert!(
            g.contains("a.wasm\\', import.meta.url"),
            "filename quote not escaped for {target:?} (breakout):\n{g}"
        );
    }
}

#[test]
fn sec4_wasm_filename_newline() {
    // A newline in the name would split the `new URL('...')` statement.
    let hostile = "a.wasm');globalThis.PWNED=1;//\n//";
    let o = make_output(vec![WasmExportInfo {
        name: "f".to_string(),
        params: vec![("x".to_string(), WasmType::F64)],
        return_type: Some(WasmType::F64),
    }]);
    let g = generate_bridge_js(&o, hostile, None);
    assert_valid(&g, "sec4_filename_newline");
    // The raw newline must be escaped to the two-char `\n`; no literal newline
    // may appear inside the payload (a real newline would end the statement).
    assert!(
        !g.contains("PWNED=1;//\n//"),
        "newline in filename survived unescaped (statement split):\n{g}"
    );
    assert!(
        g.contains("PWNED=1;//\\n//"),
        "newline should be escaped to `\\n`:\n{g}"
    );
}

// ── Finding #13: reserved-word WASM parameter names ──

#[test]
fn sec13_wasm_param_reserved_words() {
    // Each of these is a legal Python identifier but a JS reserved word; emitted
    // raw in a binding position it produces invalid JS.
    // All legal Python identifiers that are JS reserved words. (Python
    // *keywords* like `class` can never reach codegen as a param, so they are
    // out of scope — matching the emit.rs reserved-word list.)
    for kw in ["default", "new", "in", "let", "delete", "static"] {
        let o = make_output(vec![WasmExportInfo {
            name: "f".to_string(),
            params: vec![
                (kw.to_string(), WasmType::F64),
                ("y".to_string(), WasmType::F64),
            ],
            return_type: Some(WasmType::F64),
        }]);
        let g = generate_bridge_js(&o, "./m.wasm", None);
        assert_valid(&g, &format!("sec13_param_{kw}"));
        // The binding is sanitized (`kw$`), but the Python name is preserved
        // verbatim in the keyword-binding metadata.
        assert!(
            g.contains(&format!("{kw}$")),
            "reserved param `{kw}` should be sanitized to `{kw}$`:\n{g}"
        );
        assert!(
            g.contains(&format!("__pyparams__ = [\"{kw}\", \"y\"]")),
            "Python name `{kw}` must be preserved in __pyparams__:\n{g}"
        );
    }
}

#[test]
fn sec13_wasm_param_i64_reserved_ovf() {
    // The i64 overflow-guard path also references the param name; make sure the
    // sanitized binding is used there too (has_ovf=true, I64 param).
    let mut o = make_output(vec![WasmExportInfo {
        name: "g".to_string(),
        params: vec![("in".to_string(), WasmType::I64)],
        return_type: Some(WasmType::I64),
    }]);
    o.has_ovf = true;
    let g = generate_bridge_js(&o, "./m.wasm", None);
    assert_valid(&g, "sec13_param_i64_in");
    assert!(
        g.contains("__i64Oob(in$)"),
        "i64 overflow guard must reference the sanitized binding `in$`:\n{g}"
    );
}

// ── #439: reserved-word EXPORT NAME coordination (not just params) ──

#[test]
fn issue439_wasm_export_name_reserved_words() {
    use pyths_codegen_wasm::bridge::generate_bridge;
    use std::collections::BTreeSet;
    // A function whose NAME is a legal Python identifier but a JS reserved word.
    // Emitted raw, `export function default(...)` is a SyntaxError, and the
    // `__jsfb` twin object / `.__pyparams__` attach reference an undeclared
    // reserved word. Every JS binding/reference site must be sanitized to
    // `<kw>$` and coordinate with the twin (which codegen_js names `<kw>$`),
    // while the WASM binary export (a property access) stays the raw name.
    for kw in ["default", "new", "in", "let", "delete", "static"] {
        let o = make_output(vec![WasmExportInfo {
            name: kw.to_string(),
            params: vec![("a".to_string(), WasmType::I64)],
            return_type: Some(WasmType::I64),
        }]);
        // A hand-written twin, exactly as codegen_js would name it (`<kw>$`),
        // and has_ovf=true so the twin/`__jsfb` fallback ladder is emitted.
        let twin = format!("export function {kw}$(a) {{\n  return a;\n}}\n");
        let mut o = o;
        o.has_ovf = true;
        let g = generate_bridge(
            "./m.wasm",
            &o.export_info,
            &BTreeSet::new(),
            false,
            false,
            &std::collections::BTreeMap::new(),
            false,
            true,
            Some(&twin),
        );
        assert_valid(&g, &format!("issue439_export_{kw}"));
        // Wrapper decl, twin-object shorthand, __jsfb dispatch, and pyparams
        // all use the sanitized name.
        assert!(
            g.contains(&format!("export function {kw}$(")),
            "wrapper must be `export function {kw}$`:\n{g}"
        );
        assert!(
            g.contains(&format!("return {{ {kw}$ }}")),
            "__jsfb object must key on the sanitized twin name `{kw}$`:\n{g}"
        );
        assert!(
            g.contains(&format!("__jsfb.{kw}$(")),
            "fallback dispatch must call `__jsfb.{kw}$`:\n{g}"
        );
        assert!(
            g.contains(&format!("{kw}$.__pyparams__")),
            "kwarg metadata must attach to `{kw}$`:\n{g}"
        );
        // The WASM binary export is accessed as a property — raw name kept.
        assert!(
            g.contains(&format!("__wasm.{kw}(")),
            "WASM export access stays the raw name `__wasm.{kw}`:\n{g}"
        );
        // Never a bare reserved-word declaration.
        assert!(
            !g.contains(&format!("export function {kw}(")),
            "must not emit a bare reserved-word function declaration:\n{g}"
        );
    }
}
