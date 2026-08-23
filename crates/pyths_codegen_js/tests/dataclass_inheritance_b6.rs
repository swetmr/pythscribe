//! B6 (0.2.2 batch 2) — @dataclass inheritance: the derived constructor must
//! satisfy the BASE constructor's contract.
//!
//! `@dataclass class Pt: x:int; y:int` + `@dataclass class Pt3(Pt): z:int`
//! emitted a bare `super();` in Pt3's ctor, so Pt's field validators ran on
//! `undefined` and EVERY `Pt3(...)` construction threw. CLASS rule: the
//! derived ctor destructures the kwargs-object form FIRST (never touches
//! `this`, so it is legal before `super`), then calls `super(<base init
//! fields>)` — the first base's registered dataclass contract, in field
//! order. Bases without a registered dataclass contract keep `super()`.
//!
//! Matrix: single + multi-level inheritance, required + defaulted base
//! fields, positional + keyword construction, redeclared base field.
//! Behavioral tests run under node (skipped when unavailable); expected
//! outputs are CPython's (`dataclasses`) for the same programs.

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

fn node_run(js: &str, name: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pyths_b6_{}_{}", name, std::process::id()));
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

fn assert_behaves(src: &str, expected: &str, name: &str) {
    let js = compile_inline(src);
    let Some(got) = node_run(&js, name) else {
        return; // node unavailable → skip
    };
    assert_eq!(
        got.trim(),
        expected,
        "{name}: output must match CPython\n--- JS ---\n{js}"
    );
}

const BASE2: &str = "from dataclasses import dataclass\n\
@dataclass\n\
class Pt:\n    x: int\n    y: int\n\
@dataclass\n\
class Pt3(Pt):\n    z: int\n";

// ── single-level, required base fields ──────────────────────────────────────

#[test]
fn derived_positional_construction() {
    // The exact reported repro: every Pt3(...) threw on the base validators.
    assert_behaves(
        &format!("{BASE2}p = Pt3(1, 2, 3)\nprint(p.x, p.y, p.z)\n"),
        "1 2 3",
        "pos",
    );
}

#[test]
fn derived_keyword_construction() {
    assert_behaves(
        &format!("{BASE2}p = Pt3(x=1, y=2, z=3)\nprint(p.x, p.y, p.z)\n"),
        "1 2 3",
        "kw",
    );
}

#[test]
fn derived_repr_covers_all_fields() {
    assert_behaves(
        &format!("{BASE2}print(Pt3(1, 2, 3))\n"),
        "Pt3(x=1, y=2, z=3)",
        "repr",
    );
}

#[test]
fn super_call_carries_base_fields() {
    // Structural: the derived ctor passes the base contract, and the
    // kwargs-object destructure runs BEFORE super (feeds real values).
    let js = compile_inline(BASE2);
    assert!(
        js.contains("super(x, y);"),
        "derived ctor must call super with the base's init fields:\n{js}"
    );
    let destructure = js.find("({x, y, z} =").expect("kwargs destructure");
    let sup = js.find("super(x, y);").expect("super call");
    assert!(
        destructure < sup,
        "kwargs-object destructure must run before super(...):\n{js}"
    );
}

// ── defaulted base fields ───────────────────────────────────────────────────

#[test]
fn defaulted_base_field_flows_through() {
    // Base default `y = 10` — derived omits it; the base ctor must receive
    // the default-filled param, not `undefined`.
    let src = "from dataclasses import dataclass\n\
@dataclass\n\
class A:\n    x: int\n    y: int = 10\n\
@dataclass\n\
class B(A):\n    z: int = 20\n\
b = B(1)\nprint(b.x, b.y, b.z)\n\
b2 = B(1, 2, 3)\nprint(b2.x, b2.y, b2.z)\n";
    assert_behaves(src, "1 10 20\n1 2 3", "defaults");
}

// ── multi-level inheritance ─────────────────────────────────────────────────

#[test]
fn multi_level_chain() {
    // C(B(A)) — B's ctor must call super(x, y) into A; C's must call
    // super(x, y, z) into B (each level's registered contract).
    let src = "from dataclasses import dataclass\n\
@dataclass\n\
class A:\n    x: int\n    y: int\n\
@dataclass\n\
class B(A):\n    z: int\n\
@dataclass\n\
class C(B):\n    w: int\n\
c = C(1, 2, 3, 4)\nprint(c.x, c.y, c.z, c.w)\nprint(c)\n";
    assert_behaves(src, "1 2 3 4\nC(x=1, y=2, z=3, w=4)", "multilevel");
}

#[test]
fn multi_level_keyword_construction() {
    let src = "from dataclasses import dataclass\n\
@dataclass\n\
class A:\n    x: int\n    y: int\n\
@dataclass\n\
class B(A):\n    z: int\n\
@dataclass\n\
class C(B):\n    w: int\n\
c = C(x=1, y=2, z=3, w=4)\nprint(c.x, c.y, c.z, c.w)\n";
    assert_behaves(src, "1 2 3 4", "multilevel_kw");
}

// ── redeclared base field ───────────────────────────────────────────────────

#[test]
fn redeclared_base_field_keeps_position() {
    // Derived redeclares `y` with a default — CPython keeps y's ORIGINAL
    // position with the derived default; super still receives (x, y).
    let src = "from dataclasses import dataclass\n\
@dataclass\n\
class A:\n    x: int\n    y: int = 1\n\
@dataclass\n\
class B(A):\n    y: int = 5\n\
b = B(7)\nprint(b.x, b.y)\n";
    assert_behaves(src, "7 5", "redeclared");
}

// ── non-dataclass base keeps the bare super() ───────────────────────────────

#[test]
fn plain_dataclass_no_base_unchanged() {
    // No-base dataclass: no super call at all (regression guard).
    let js =
        compile_inline("from dataclasses import dataclass\n@dataclass\nclass P:\n    x: int\n");
    assert!(
        !js.contains("super("),
        "a base-less dataclass must not emit any super call:\n{js}"
    );
}
