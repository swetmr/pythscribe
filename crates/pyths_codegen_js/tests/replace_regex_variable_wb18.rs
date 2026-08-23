//! WB-18 — `.replace(regexVAR, repl)` where the RegExp is held in a VARIABLE
//! must apply the regex, not mis-route to Python `str.replace`. Compile time
//! cannot know a variable's type, so the WB-10 SYNTACTIC arg check (inline
//! `RegExp(...)` / fn literal) missed it. Root fix: a RUNTIME dispatcher
//! `pyStrReplaceSmart(recv, a, b)` — `a instanceof RegExp || typeof b ===
//! "function"` ⇒ native `.replace`; else Python `pyStrReplace`. Inline regex
//! still short-circuits to a verbatim native `.replace`; plain str/str keeps
//! Python replace-all semantics — so WB-10, F2, WB-3, WB-6 are all preserved.

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

fn node_run(js: &str, name: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pyths_wb18_{}_{}", name, std::process::id()));
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

// ── structural: variable regex routes through the runtime smart dispatcher ────

#[test]
fn variable_regex_routes_through_smart_dispatcher() {
    let js =
        compile_inline("gp = RegExp(r\"^(a|b):\")\ndef f(s):\n    return s.replace(gp, \"\")\n");
    assert!(
        js.contains("pyStrReplaceSmart("),
        "variable-regex .replace must route through pyStrReplaceSmart:\n{js}"
    );
}

#[test]
fn inline_regex_still_verbatim() {
    // WB-10 preserved: an inline RegExp short-circuits to a verbatim native
    // `.replace(new RegExp(...))` before reaching the runtime helper.
    let js = compile_inline("def f(s):\n    return s.replace(RegExp(r\"^a:\"), \"\")\n");
    assert!(
        js.contains(".replace(new RegExp(") && !js.contains("pyStrReplaceSmart"),
        "inline-regex .replace must stay verbatim (WB-10):\n{js}"
    );
}

// ── behavioral: var/inline agree; plain str/str keeps Python replace-all ──────

#[test]
fn replace_var_and_inline_regex_agree_under_node() {
    // Bracket each result so a leading space survives the outer trim().
    let js = compile_inline(
        "gp = RegExp(r\"^(linearGradient|radialGradient):\")\n\
         s = \"linearGradient: 0% #9A9185\"\n\
         print(\"[\" + s.replace(gp, \"\") + \"]\")\n\
         print(\"[\" + s.replace(RegExp(r\"^(linearGradient|radialGradient):\"), \"\") + \"]\")\n\
         print(\"a.b.c\".replace(\".\", \"_\"))\n\
         print(\"banana\".replace(\"a\", \"X\", 2))\n",
    );
    let Some(got) = node_run(&js, "behave") else {
        return; // node unavailable → skip
    };
    let lines: Vec<&str> = got.trim().lines().collect();
    assert_eq!(
        lines[0], "[ 0% #9A9185]",
        "variable regex must strip the whole prefix\n--- JS ---\n{js}"
    );
    assert_eq!(
        lines[1], "[ 0% #9A9185]",
        "inline regex must match the variable-regex result\n--- JS ---\n{js}"
    );
    assert_eq!(
        lines[2], "a_b_c",
        "plain str/str .replace must keep Python replace-ALL semantics (not first-only)\n--- JS ---\n{js}"
    );
    assert_eq!(
        lines[3], "bXnXna",
        "plain str/str .replace must honor the count argument\n--- JS ---\n{js}"
    );
}
