//! WB-17 — the ES primitive-wrapper globals Boolean/Number/String/Symbol/BigInt
//! must compile to a BARE call, not `new`. Called bare they are type-conversion
//! functions returning primitives; `new Boolean(x)` is a boxed OBJECT (always
//! truthy), `new Number("3")`/`new String(5)` are objects (typeof "object",
//! `!== 3`/`!== "5"`), and `new Symbol()`/`new BigInt()` THROW. The capitalized-
//! name → `new` heuristic must exempt them.

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

fn node_run(js: &str, name: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pyths_wb17_{}_{}", name, std::process::id()));
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

// ── structural: no `new` on the wrapper globals, still `new` for a user class ─

#[test]
fn wrapper_globals_emit_bare_call() {
    let js = compile_inline("a = Boolean(0)\nb = Number(\"3\")\nc = String(5)\n");
    assert!(
        !js.contains("new Boolean") && !js.contains("new Number") && !js.contains("new String"),
        "wrapper globals must NOT be `new`-called:\n{js}"
    );
    assert!(
        js.contains("Boolean(0)") && js.contains("Number(\"3\")") && js.contains("String(5)"),
        "wrapper globals must be emitted as bare calls:\n{js}"
    );
}

#[test]
fn user_capitalized_class_still_gets_new() {
    // Regression guard: the capitalization heuristic still `new`s a real class.
    let js = compile_inline("class Widget:\n    pass\nw = Widget()\n");
    assert!(
        js.contains("new Widget("),
        "a genuine user class must still be `new`-called:\n{js}"
    );
}

#[test]
fn user_class_shadowing_wrapper_name_gets_new() {
    // A user `class String` shadows the global → `new String(...)` is correct.
    let js = compile_inline("class String:\n    pass\ns = String()\n");
    assert!(
        js.contains("new String"),
        "a user class shadowing a wrapper name must still be `new`-called:\n{js}"
    );
}

// ── behavioral: primitives, not boxed objects ───────────────────────────────

#[test]
fn wrapper_globals_produce_primitives_under_node() {
    let js = compile_inline(
        "print(\"yes\" if Boolean(\"\") else \"no\")\n\
         print(Number(\"3\") == 3)\n\
         print(String(5))\n\
         print(bool(0))\n\
         print(int(\"42\") == 42)\n",
    );
    let Some(got) = node_run(&js, "prim") else {
        return; // node unavailable → skip
    };
    let lines: Vec<&str> = got.trim().lines().collect();
    assert_eq!(
        lines[0], "no",
        "Boolean(\"\") must be falsy (a primitive), not a truthy wrapper object\n--- JS ---\n{js}"
    );
    assert_eq!(
        lines[1], "True",
        "Number(\"3\") must strict-equal 3 (a primitive number)\n--- JS ---\n{js}"
    );
    assert_eq!(
        lines[2], "5",
        "String(5) must be the primitive string \"5\"\n--- JS ---\n{js}"
    );
    // Python builtins unaffected.
    assert_eq!(lines[3], "False", "bool(0) regressed\n--- JS ---\n{js}");
    assert_eq!(lines[4], "True", "int(\"42\") regressed\n--- JS ---\n{js}");
}
