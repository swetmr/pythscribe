//! SECURITY regression: codegen JS-injection / encoding cluster (Codex-Security
//! scan 2026-08-12, findings #3, #4-emit, #9, #13-emit). Each test is a concrete
//! reproducer: a `.ps` input whose *emitted JS* demonstrated an injection or an
//! invalid-identifier breakout BEFORE the fix, and is a passing regression AFTER.
//!
//! Primary oracle = `node --check` on the full compiled module: a JS string- or
//! regex-literal breakout, or a reserved-word binding (`let let = ...`), yields a
//! SyntaxError that `--check` rejects. Where a payload is syntactically valid but
//! semantically injected (an expression spliced into a message concat), a
//! string-level assertion pins that the hostile substring is escaped, not raw.
//!
//! These findings were STATIC (single-pass, not sandbox-PoC). This file is the
//! PoC-before-report discipline (E11): the reproducer must fire before the fix.

use std::process::Command;

fn compile(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen(&module)
}

/// Run `node --check` over the emitted module (parse-only; imports are not
/// resolved). Returns Ok(()) if syntactically valid, Err(stderr) otherwise.
/// Skips (returns Ok) when node is unavailable so the suite still runs offline.
fn node_check(js: &str, name: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("pyths_secinj_{}", std::process::id()));
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
        panic!("emitted module for `{name}` is not valid JS (injection/breakout):\n{e}\n--- JS ---\n{js}");
    }
}

// ── Finding #3: dataclass field-constraint / validator strings as raw JS ──

#[test]
fn sec3_dataclass_pattern_regex_breakout() {
    // A `/`- and newline-bearing regex pattern closes the `/.../ ` literal early.
    let js = compile(
        "from dataclasses import dataclass, Field\n@dataclass\nclass C:\n    x: str = Field(pattern=\"a/) || (globalThis.PWNED=1) || (/b\")\n",
    );
    assert_valid(&js, "sec3_pattern");
    // Routed through the RegExp constructor (encoded string), not a raw `/.../ `
    // literal — the pre-fix raw form `!/a/) ||` must be gone.
    assert!(js.contains("new RegExp("), "regex not routed via constructor:\n{js}");
    // Pre-fix emitted a raw `/.../ ` test literal `!/a/) || ...`; that condition
    // form must be gone (the payload now lives only inside the RegExp string arg
    // and the quoted message, both as data). node --check above proves validity.
    assert!(
        !js.contains("!/a/) ||"),
        "pattern still spliced as a raw regex test literal:\n{js}"
    );
}

#[test]
fn sec3_dataclass_pattern_message_breakout() {
    // The pattern is ALSO spliced into the error-message string literal.
    let js = compile(
        "from dataclasses import dataclass, Field\n@dataclass\nclass C:\n    x: str = Field(pattern=\"a\\\"; globalThis.PWNED=1; \\\"b\")\n",
    );
    assert_valid(&js, "sec3_pattern_msg");
    // The `"` in the payload must be escaped (`\"`), keeping it inside the
    // literal as data. Pre-fix the raw `"` closed the string early (invalid JS,
    // rejected by node --check above); post-fix the escaped form is present.
    assert!(
        js.contains("\\\"; globalThis.PWNED=1; \\\""),
        "payload not escaped inside the literal:\n{js}"
    );
}

#[test]
fn sec3_dataclass_starts_with_breakout() {
    // A quote+newline prefix breaks the `startsWith("...")` literal.
    let js = compile(
        "from dataclasses import dataclass, Field\n@dataclass\nclass C:\n    name: str = Field(starts_with=\"p\\\"); globalThis.PWNED=1; (\\\"q\")\n",
    );
    assert_valid(&js, "sec3_starts_with");
    assert!(
        !js.contains("\"); globalThis.PWNED=1; (\""),
        "starts_with broke out:\n{js}"
    );
}

#[test]
fn sec3_dataclass_includes_breakout() {
    let js = compile(
        "from dataclasses import dataclass, Field\n@dataclass\nclass C:\n    name: str = Field(includes=\"m\\\"); globalThis.PWNED=1; (\\\"d\")\n",
    );
    assert_valid(&js, "sec3_includes");
    assert!(
        !js.contains("\"); globalThis.PWNED=1; (\""),
        "includes broke out:\n{js}"
    );
}

#[test]
fn sec3_dataclass_validator_selector_breakout() {
    // The @validator("...") arg becomes `this.<arg>` member access — a hostile
    // arg injects statements around the assignment.
    let js = compile(
        "from dataclasses import dataclass\nfrom pydantic import validator\n@dataclass\nclass C:\n    name: str\n    @validator(\"name = (globalThis.PWNED=1)); ((this.z\")\n    def v(self, value):\n        return value\n",
    );
    assert_valid(&js, "sec3_validator");
    // Routed through safe computed access `this[<encoded>]`, not `this.<raw>`.
    // The pre-fix raw member-access form must be gone.
    assert!(
        js.contains("this[\"name = (globalThis.PWNED=1"),
        "validator selector not routed through computed access:\n{js}"
    );
    assert!(
        !js.contains("this.name = (globalThis.PWNED=1"),
        "validator selector still emits raw member access:\n{js}"
    );
}

// ── Finding #9: match mapping-pattern capture key interpolated raw ──

#[test]
fn sec9_match_mapping_key_breakout() {
    // `case {"<key>": x}` splices the key between JS quotes in
    // `subj.get("<key>")` / `subj["<key>"]`; a `"`/newline/backslash broke out.
    let js = compile(
        "def f(x):\n    match x:\n        case {\"k\\\"); globalThis.PWNED=1; ((y\": y}:\n            return y\n",
    );
    // Primary oracle: `node --check` — a raw key breakout closes the
    // `.get("...")` / `["..."]` string and makes `globalThis.PWNED=1` a live
    // statement, an unterminated-string / stray-token SyntaxError.
    assert_valid(&js, "sec9_mapping_key");
    // And the `"` must appear ESCAPED (`\"`) — the pre-fix raw form was `k");`
    // (bare quote), which cannot contain the backslash-escaped sequence.
    assert!(
        js.contains("k\\\"); globalThis.PWNED=1"),
        "mapping key quote not escaped inside the literal:\n{js}"
    );
}

// ── Finding #13 (emit): reserved-word identifiers at JS binding sites ──

#[test]
fn sec13_except_alias_reserved_word() {
    // `except E as let:` emitted `let let = __exc` — a SyntaxError. The alias
    // must be sanitized (`let$`) at the declaration and every reference.
    let js = compile(
        "def f(x):\n    try:\n        pass\n    except Exception as let:\n        return let\n",
    );
    assert_valid(&js, "sec13_except_let");
    assert!(
        !js.contains("let let ="),
        "reserved-word alias emitted raw (SyntaxError):\n{js}"
    );
    assert!(
        js.contains("let$"),
        "alias not sanitized to `let$`:\n{js}"
    );
}

#[test]
fn sec13_match_capture_reserved_word() {
    // A match capture pattern binding a reserved word (`case new:`) emitted
    // `let new = ...`; it must be sanitized too.
    let js = compile("def f(x):\n    match x:\n        case new:\n            return new\n");
    assert_valid(&js, "sec13_capture_new");
    assert!(
        !js.contains("let new ="),
        "reserved-word capture emitted raw (SyntaxError):\n{js}"
    );
}

// ── Finding #4 (emit portion): WASM re-export import specifier ──

#[test]
fn sec4_wasm_reexport_glue_filename_breakout() {
    // `emit_wasm_reexports` splices the glue filename into an import specifier
    // string; a `"`/newline broke out. Drive it directly with a hostile stem.
    let mut cg = pyths_codegen_js::JsCodegen::new();
    let mut skip = std::collections::HashSet::new();
    skip.insert("kernel".to_string());
    cg.set_wasm_skip(skip);
    cg.emit_wasm_reexports("m.js\"; globalThis.PWNED=1; import \"x");
    let js = cg.finish();
    assert_valid(&js, "sec4_reexport");
    assert!(
        !js.contains("\"; globalThis.PWNED=1; import \""),
        "glue filename broke out of the import specifier:\n{js}"
    );
}
