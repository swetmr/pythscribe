//! TB-1 — the PSX prop-name transform (snake→camel/kebab: item_id→itemId,
//! on_click→onClick, aria_label→aria-label) must NOT leak into PLAIN dict
//! literals. A plain dict literal emits its keys VERBATIM, always; the
//! transform is scoped to PSX-prop emission (createElement attribute names)
//! only. Before the fix a key on the DOM-attr allowlist was camelCased in the
//! LITERAL while the subscript READ stayed verbatim — an asymmetric silent
//! KeyError (same naming-soundness family as the intrinsic-tag rule / F2 / #420).
//!
//! Two tests lock the BOUNDARY:
//!   * `plain_dict_key_is_verbatim` — a plain dict literal keeps every key
//!     verbatim, so literal-key and subscript-read agree (the round-trip).
//!   * `psx_props_still_lowered` — the transform STILL fires for real PSX
//!     props inside @psx/@component (className / onClick / aria-label).

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

fn node_run(js: &str, name: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pyths_tb1_{}_{}", name, std::process::id()));
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

// ── boundary (a): a PLAIN dict literal keeps keys verbatim ───────────────────

#[test]
fn plain_dict_key_is_verbatim() {
    // Keys on the DOM-attr/event allowlist must NOT be mangled in a plain dict.
    let js = compile_inline("d = {\"item_id\": 1, \"on_click\": 5, \"aria_label\": 7}\n");
    assert!(
        js.contains("\"item_id\":"),
        "plain-dict key `item_id` must stay verbatim (not `itemId`):\n{js}"
    );
    assert!(
        js.contains("\"on_click\":"),
        "plain-dict key `on_click` must stay verbatim (not `onClick`):\n{js}"
    );
    assert!(
        js.contains("\"aria_label\":"),
        "plain-dict key `aria_label` must stay verbatim (not `aria-label`):\n{js}"
    );
    assert!(
        !js.contains("itemId") && !js.contains("onClick") && !js.contains("aria-label"),
        "no PSX prop-name transform may leak into a plain dict literal:\n{js}"
    );
}

#[test]
fn plain_dict_key_roundtrips_under_node() {
    // The regression: literal key camelCased, subscript read verbatim → KeyError.
    let js = compile_inline(
        "d = {\"item_id\": 1, \"server_text\": 2, \"user_id\": 3, \"on_click\": 5}\n\
         print(d[\"item_id\"])\n\
         print(d[\"on_click\"])\n",
    );
    let Some(got) = node_run(&js, "roundtrip") else {
        return; // node unavailable → skip
    };
    let lines: Vec<&str> = got.trim().lines().collect();
    assert_eq!(
        lines[0], "1",
        "d[\"item_id\"] must round-trip its key\n--- JS ---\n{js}"
    );
    assert_eq!(
        lines[1], "5",
        "d[\"on_click\"] must round-trip its key\n--- JS ---\n{js}"
    );
}

// ── boundary (b): PSX props are STILL lowered inside @psx/@component ──────────

#[test]
fn psx_props_still_lowered() {
    let js = compile_inline(
        "@psx\ndef Row():\n    return div(class_name=\"r\", on_click=lambda e: 0, aria_label=\"x\")\n",
    );
    assert!(
        js.contains("className:"),
        "PSX prop class_name must still lower to className:\n{js}"
    );
    assert!(
        js.contains("onClick:"),
        "PSX prop on_click must still lower to onClick:\n{js}"
    );
    assert!(
        js.contains("\"aria-label\":"),
        "PSX prop aria_label must still lower to aria-label:\n{js}"
    );
}
