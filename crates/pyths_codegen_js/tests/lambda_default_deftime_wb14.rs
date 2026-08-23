//! WB-14 — a lambda default arg must be evaluated ONCE at def-time (Python
//! semantics), not at call-time (bare JS default param).
//!
//! `lambda _t=tmp` where `tmp` is reassigned in a loop must freeze that
//! iteration's value (`[1,2,3]`, not `[3,3,3]`); `lambda x=x` must capture the
//! OUTER `x` (no `(x=x)=>` self-reference TDZ). Root fix: wrap the arrow in a
//! def-time IIFE that captures each default once —
//! `((__ld0) => (p = __ld0) => …)(expr)`.
//!
//! Structural tests assert the IIFE; behavioral tests run under node (skipped
//! when node is unavailable, same policy as optchain_helper_wb9.rs).

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

/// Run `js` under node from a scratch dir; None when node is unavailable.
fn node_run(js: &str, name: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pyths_wb14_{}_{}", name, std::process::id()));
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

// ── structural ──────────────────────────────────────────────────────────────

#[test]
fn lambda_default_hoists_to_deftime_iife() {
    let js = compile_inline("def f(tmp):\n    return lambda _t=tmp: _t\n");
    // The default is captured once via an outer IIFE, referenced by a const.
    assert!(
        js.contains("(__ld$") && js.contains(") => (_t = __ld$"),
        "lambda default must be hoisted into a def-time IIFE:\n{js}"
    );
    // The call-time re-evaluation form `(_t = tmp) =>` must NOT be emitted.
    assert!(
        !js.contains("(_t = tmp)"),
        "lambda default must NOT be a bare call-time JS default param:\n{js}"
    );
}

#[test]
fn lambda_self_reference_default_breaks_tdz() {
    // `lambda x=x` must not emit `(x = x) =>` (TDZ self-reference); the default
    // must read the OUTER x through the IIFE argument.
    let js = compile_inline("def f(x):\n    return lambda x=x: x\n");
    assert!(
        !js.contains("(x = x)"),
        "`lambda x=x` must NOT emit a self-referencing `(x = x) =>`:\n{js}"
    );
    assert!(
        js.contains("(__ld$") && js.contains(") => (x = __ld$"),
        "`lambda x=x` must read the outer x via the def-time IIFE:\n{js}"
    );
}

#[test]
fn lambda_without_default_unchanged() {
    let js = compile_inline("def f():\n    return lambda a: a + 1\n");
    assert!(
        js.contains("(a) =>") && !js.contains("__ld$"),
        "a lambda with no default must be a plain arrow (no IIFE):\n{js}"
    );
}

// ── behavioral ───────────────────────────────────────────────────────────────

#[test]
fn lambda_defaults_are_deftime_under_node() {
    let js = compile_inline(
        "def demo_snapshot():\n\
         \x20   out = []\n\
         \x20   tmp = 0\n\
         \x20   for n in [1, 2, 3]:\n\
         \x20       tmp = n\n\
         \x20       out.append(lambda _t=tmp: _t)\n\
         \x20   return [f() for f in out]\n\
         def demo_tdz():\n\
         \x20   x = 10\n\
         \x20   g = lambda a=None, x=x: x\n\
         \x20   return g()\n\
         def multi():\n\
         \x20   a = 1\n\
         \x20   b = 2\n\
         \x20   h = lambda p=a, q=b: p * 10 + q\n\
         \x20   a = 99\n\
         \x20   b = 99\n\
         \x20   return h()\n\
         print(demo_snapshot())\n\
         print(demo_tdz())\n\
         print(multi())\n",
    );
    let Some(got) = node_run(&js, "behave") else {
        return; // node unavailable → skip
    };
    let lines: Vec<&str> = got.trim().lines().collect();
    assert_eq!(
        lines[0], "[1, 2, 3]",
        "loop-var default must freeze per-iteration (def-time), not read the last value\n--- JS ---\n{js}"
    );
    assert_eq!(
        lines[1], "10",
        "`lambda x=x` must capture the outer x (no TDZ)\n--- JS ---\n{js}"
    );
    assert_eq!(
        lines[2], "12",
        "multiple defaults each frozen at def-time (before a/b were reassigned)\n--- JS ---\n{js}"
    );
}
