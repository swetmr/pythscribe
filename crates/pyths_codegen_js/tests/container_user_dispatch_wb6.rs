//! WB-6 (0.2.2 batch 2) — container-method runtime helpers must fall through
//! to a USER receiver's own method, and `dict.update` must accept the
//! iterable-of-pairs form.
//!
//! `q.items()` on a user object routed to pyDictItems' attribute-dict
//! snapshot (silent wrong data — the F2 method-collision family at runtime
//! level); `.setdefault()` had no user dispatch at all; `d.update([("k",
//! "v")])` walked the ARRAY's own keys → `{'0': ('k', 'v')}`.
//!
//! CLASS rules (both runtime copies — the inline copies of the dict family
//! were deleted and now extract from the canonical package runtime, see the
//! inline_runtime_parity gate):
//!   * every dict-family helper dispatches to a non-container receiver's own
//!     method of the PYTHON name (pyDictItems→items, pyDictSetdefault→
//!     setdefault; pyUpdate/pyClear/pyDictPopitem/pyPop already did);
//!   * pyUpdate consumes a non-mapping iterable as (k, v) PAIRS (CPython's
//!     `hasattr(o, "keys")` rule), validating each element's pair shape.
//!
//! Tests run the INLINE codegen under node (exercises the extracted copies);
//! behavioral expectations are CPython's.

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

fn node_run(js: &str, name: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pyths_wb6_{}_{}", name, std::process::id()));
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
    assert_eq!(got.trim(), expected, "{name}\n--- JS ---\n{js}");
}

const USER_CLASS: &str = "class Q:
    def __init__(self):
        self.v = 1
    def items(self):
        return \"user items\"
    def clear(self):
        return \"user clear\"
    def setdefault(self, k, d=None):
        return \"user setdefault\"
    def update(self, *a):
        return \"user update\"
";

// ── user-object dispatch ────────────────────────────────────────────────────

#[test]
fn user_items_dispatches_to_user_method() {
    assert_behaves(
        &format!("{USER_CLASS}q = Q()\nprint(q.items())\n"),
        "user items",
        "u_items",
    );
}

#[test]
fn user_clear_dispatches_to_user_method() {
    assert_behaves(
        &format!("{USER_CLASS}q = Q()\nprint(q.clear())\n"),
        "user clear",
        "u_clear",
    );
}

#[test]
fn user_setdefault_dispatches_to_user_method() {
    assert_behaves(
        &format!("{USER_CLASS}q = Q()\nprint(q.setdefault(\"k\", 0))\n"),
        "user setdefault",
        "u_setdefault",
    );
}

#[test]
fn user_update_dispatches_to_user_method() {
    assert_behaves(
        &format!("{USER_CLASS}q = Q()\nprint(q.update({{\"a\": 1}}))\n"),
        "user update",
        "u_update",
    );
}

// ── real containers keep the container paths ────────────────────────────────

#[test]
fn real_dict_items_unchanged() {
    assert_behaves(
        "d = {\"a\": 1, \"b\": 2}\nprint(d.items())\nprint(d.setdefault(\"c\", 3))\nprint(d)\n",
        "[('a', 1), ('b', 2)]\n3\n{'a': 1, 'b': 2, 'c': 3}",
        "real_dict",
    );
}

#[test]
fn real_dict_clear_unchanged() {
    assert_behaves(
        "d = {\"a\": 1}\nd.clear()\nprint(d)\nxs = [1, 2]\nxs.clear()\nprint(xs)\n",
        "{}\n[]",
        "real_clear",
    );
}

// ── dict.update: pairs-iterable + mapping forms ─────────────────────────────

#[test]
fn update_pairs_list_of_tuples() {
    assert_behaves(
        "d = {}\nd.update([(\"k\", \"v\"), (\"k2\", 2)])\nprint(d)\n",
        "{'k': 'v', 'k2': 2}",
        "up_pairs",
    );
}

#[test]
fn update_mapping_form_unchanged() {
    assert_behaves(
        "d = {\"a\": 1}\nd.update({\"b\": 2})\nprint(d)\n",
        "{'a': 1, 'b': 2}",
        "up_mapping",
    );
}

#[test]
fn update_generator_of_pairs() {
    assert_behaves(
        "d = {}\nd.update((k, k * 2) for k in [\"a\", \"b\"])\nprint(d)\n",
        "{'a': 'aa', 'b': 'bb'}",
        "up_gen",
    );
}

#[test]
fn update_bad_pair_shape_raises() {
    // CPython: ValueError (element has length 1; 2 is required).
    assert_behaves(
        "d = {}\ntry:\n    d.update([(\"k\",)])\nexcept ValueError:\n    print(\"ValueError\")\nprint(d)\n",
        "ValueError\n{}",
        "up_badpair",
    );
}

#[test]
fn set_update_still_unions() {
    // The Set-receiver branch runs FIRST — `s.update([...])` stays a union,
    // never the pairs path.
    assert_behaves(
        "s = {1, 2}\ns.update([3, 4])\nprint(sorted(s))\n",
        "[1, 2, 3, 4]",
        "set_update",
    );
}

#[test]
fn counter_style_user_update_still_wins() {
    // #242 regression guard: a custom receiver's own update wins outright.
    assert_behaves(
        "from collections import Counter\nc = Counter({\"a\": 1})\nc.update({\"a\": 2})\nprint(c[\"a\"])\n",
        "3",
        "counter_update",
    );
}
