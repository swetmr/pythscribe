//! WB-10 — `.replace(regex, …)` / `.replace(str, fn)` on a JS string must be
//! emitted verbatim as `String.prototype.replace`, not lowered to the runtime
//! `pyStrReplace` (Python `str.replace`, which drops capture groups / `$1`
//! backrefs and stringifies a function replacer).
//!
//! Root fix (arg-type discriminator, same family as WB-3's positional-arity
//! signal): Python `str.replace(old, new[, count])` takes only string args, so
//! a call whose FIRST arg is a regex (`RegExp(...)`) OR whose SECOND arg is a
//! function (lambda / known `def`) can never be Python `str.replace` — emit it
//! verbatim (`String.prototype.replace`, which supports regex + function
//! replacer). A plain `s.replace(str, str)` keeps the `pyStrReplace` lowering.
//!
//! Structural tests assert dispatch; behavioral tests run under node (skipped
//! when node is unavailable, same policy as optchain_helper_wb9.rs).

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

/// Run `js` under node from a scratch dir; None when node is unavailable.
fn node_run(js: &str, name: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pyths_wb10_{}_{}", name, std::process::id()));
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

// ── structural: dispatch decisions ──────────────────────────────────────────

#[test]
fn regex_first_arg_emits_verbatim_replace() {
    // `s.replace(RegExp(...), "<strong>$1</strong>")` — regex first arg proves
    // JS replace; must be verbatim `.replace(new RegExp(...))`, NOT pyStrReplace.
    let js = compile_inline(
        "def bold(s):\n    return s.replace(RegExp(r\"\\*\\*(.*?)\\*\\*\", \"g\"), \"<strong>$1</strong>\")\n",
    );
    assert!(
        js.contains(".replace(new RegExp("),
        "regex-first .replace must be verbatim String.prototype.replace:\n{js}"
    );
    assert!(
        !js.contains("pyStrReplace"),
        "regex-first .replace must NOT lower to pyStrReplace:\n{js}"
    );
}

#[test]
fn function_second_arg_lambda_emits_verbatim_replace() {
    // `s.replace(RegExp(...), lambda m: ...)` — a function replacer proves JS
    // replace even structurally; must be verbatim.
    let js =
        compile_inline("def f(s):\n    return s.replace(RegExp(r\"(\\d)\", \"g\"), lambda m: m)\n");
    assert!(
        js.contains(".replace(new RegExp("),
        "function-replacer .replace must be verbatim:\n{js}"
    );
    assert!(
        !js.contains("pyStrReplace"),
        "function-replacer .replace must NOT lower to pyStrReplace:\n{js}"
    );
}

#[test]
fn function_second_arg_known_def_emits_verbatim_replace() {
    // A bare name resolving to a `def` in the 2nd slot is a function replacer.
    let js = compile_inline(
        "def repl(m):\n    return m\ndef f(s):\n    return s.replace(RegExp(r\"x\", \"g\"), repl)\n",
    );
    assert!(
        js.contains(".replace(new RegExp("),
        "def-name replacer .replace must be verbatim:\n{js}"
    );
    assert!(
        !js.contains("pyStrReplace"),
        "def-name replacer .replace must NOT lower to pyStrReplace:\n{js}"
    );
}

#[test]
fn plain_str_str_replace_unchanged() {
    // `s.replace("a", "b")` — neither signal; keeps the pyStrReplace lowering
    // (Python replace-all semantics). Regression guard.
    let js = compile_inline("def f(s):\n    return s.replace(\"a\", \"b\")\n");
    assert!(
        js.contains("pyStrReplace"),
        "plain str/str .replace must stay on pyStrReplace:\n{js}"
    );
    assert!(
        !js.contains(".replace(\"a\""),
        "plain str/str .replace must NOT be emitted verbatim:\n{js}"
    );
}

#[test]
fn regex_first_string_replacement_is_verbatim() {
    // regex first + STRING replacement → still JS-only (regex is JS syntax).
    let js = compile_inline("def f(s):\n    return s.replace(RegExp(r\"a\", \"g\"), \"b\")\n");
    assert!(
        js.contains(".replace(new RegExp(") && !js.contains("pyStrReplace"),
        "regex-first with a string replacement is verbatim:\n{js}"
    );
}

// ── behavioral: node confirms capture groups / callback survive ──────────────

#[test]
fn replace_regex_and_callback_behave_under_node() {
    let js = compile_inline(
        "def bold(s):\n\
         \x20   return s.replace(RegExp(r\"\\*\\*(.*?)\\*\\*\", \"g\"), \"<strong>$1</strong>\")\n\
         def upper_digits(s):\n\
         \x20   return s.replace(RegExp(r\"(\\d)\", \"g\"), lambda m: \"[\" + m + \"]\")\n\
         def plain(s):\n\
         \x20   return s.replace(\"a\", \"b\")\n\
         print(bold(\"**hi**\"))\n\
         print(upper_digits(\"a1b2\"))\n\
         print(plain(\"banana\"))\n",
    );
    let Some(got) = node_run(&js, "behave") else {
        return; // node unavailable → skip
    };
    let lines: Vec<&str> = got.trim().lines().collect();
    assert_eq!(
        lines[0], "<strong>hi</strong>",
        "capture-group backref must survive\n--- JS ---\n{js}"
    );
    assert_eq!(
        lines[1], "a[1]b[2]",
        "function replacer must be called per match, not stringified\n--- JS ---\n{js}"
    );
    assert_eq!(
        lines[2], "bbnbnb",
        "plain str/str replace keeps Python replace-all\n--- JS ---\n{js}"
    );
}
