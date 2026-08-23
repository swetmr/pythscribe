//! Error-KIND fidelity at inlined operation sites (#471/#472/#473 class).
//!
//! The compiler inlines certain operations as raw JS; when the operand's
//! static type is provably wrong the failure must still surface as the
//! CPython exception (kind + message), never a leaked native JS
//! TypeError/RangeError:
//!
//!   * #472 — calling a non-callable local (`d = {"a": 1}; d()`) routes
//!     through __pyCall → "TypeError: 'dict' object is not callable"
//!     (was the native "d is not a function").
//!   * #473 — iterating a statically-numeric local (`n = 5; for y in n:`)
//!     routes through pyForIter → "TypeError: 'int' object is not
//!     iterable" (was the native "n is not iterable"; the guard was
//!     bypassed for provably-non-iterable static types).
//!   * #471 — sequence replication by an out-of-index-range count
//!     (`[1] * (10**30)`) raises "OverflowError: cannot fit 'int' into an
//!     index-sized integer" from __mulRepCount (was a JS RangeError
//!     "Invalid array length" from the replication loop).
//!
//! Each error lane is paired with a fast-path regression guard proving the
//! valid form still works.

use std::process::Command;

fn pyths_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pyths"))
}

/// Compile-and-run `src` through `pyths run`, returning (success, stdout, stderr).
fn run_ps(name: &str, src: &str) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!("pyths_errkind_{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join(format!("{}.ps", name));
    std::fs::write(&ps, src).unwrap();
    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap()])
        .output()
        .expect("pyths run failed to spawn");
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n");
    let _ = std::fs::remove_dir_all(&dir);
    (out.status.success(), stdout, stderr)
}

/// Assert the snippet FAILS and stderr carries the CPython error KIND and
/// MESSAGE — and does NOT leak the native JS message it used to leak.
/// (Node renders the kind as `TypeError_:` at the throw preview or
/// `Error [TypeError]:` for name-tagged Errors, so kind and message are
/// asserted as separate substrings.)
fn expect_err(name: &str, src: &str, kind: &str, python_msg: &str, leaked_js_msg: &str) {
    let (ok, stdout, stderr) = run_ps(name, src);
    assert!(
        !ok,
        "{name}: expected a runtime error, but the program exited 0\nstdout={stdout}\nstderr={stderr}"
    );
    assert!(
        stderr.contains(kind),
        "{name}: stderr must name the CPython exception kind {kind:?}\nstderr={stderr}"
    );
    assert!(
        stderr.contains(python_msg),
        "{name}: stderr must contain the CPython message {python_msg:?}\nstderr={stderr}"
    );
    assert!(
        !stderr.contains(leaked_js_msg),
        "{name}: stderr still leaks the native JS message {leaked_js_msg:?}\nstderr={stderr}"
    );
}

/// Assert the snippet SUCCEEDS with the expected stdout (fast-path guard).
fn expect_ok(name: &str, src: &str, expected_stdout: &str) {
    let (ok, stdout, stderr) = run_ps(name, src);
    assert!(
        ok,
        "{name}: expected success; stderr={stderr}\nstdout={stdout}"
    );
    assert_eq!(stdout, expected_stdout, "{name}: wrong stdout");
}

// ── #472: calling a non-callable ─────────────────────────────────────────

#[test]
fn call_non_callable_dict_raises_python_typeerror() {
    expect_err(
        "call_dict",
        "d = {\"a\": 1}\nd()\n",
        "TypeError",
        "'dict' object is not callable",
        "is not a function",
    );
}

#[test]
fn call_non_callable_int_raises_python_typeerror() {
    expect_err(
        "call_int",
        "n = 5\nn()\n",
        "TypeError",
        "'int' object is not callable",
        "is not a function",
    );
}

#[test]
fn call_non_callable_list_raises_python_typeerror() {
    expect_err(
        "call_list",
        "xs = [1, 2]\nxs()\n",
        "TypeError",
        "'list' object is not callable",
        "is not a function",
    );
}

#[test]
fn call_non_callable_expression_callees_raise_python_typeerror() {
    // Round 3 (corpus deep-close): EXPRESSION callees — literals and
    // parenthesized non-callables — take the same __pyCall guard as Name
    // callees; previously they emitted a raw JS call and leaked
    // "5 is not a function".
    expect_err(
        "call_expr_int",
        "(5)()\n",
        "TypeError",
        "'int' object is not callable",
        "is not a function",
    );
    expect_err(
        "call_expr_str",
        "(\"ab\")()\n",
        "TypeError",
        "'str' object is not callable",
        "is not a function",
    );
    expect_err(
        "call_expr_list",
        "([1])()\n",
        "TypeError",
        "'list' object is not callable",
        "is not a function",
    );
    expect_err(
        "call_expr_dict",
        "({\"a\": 1})()\n",
        "TypeError",
        "'dict' object is not callable",
        "is not a function",
    );
    expect_err(
        "call_expr_tuple",
        "((1,))()\n",
        "TypeError",
        "'tuple' object is not callable",
        "is not a function",
    );
    expect_err(
        "call_expr_set",
        "({1, 2})()\n",
        "TypeError",
        "'set' object is not callable",
        "is not a function",
    );
    expect_err(
        "call_expr_none",
        "(None)()\n",
        "TypeError",
        "'NoneType' object is not callable",
        "is not a function",
    );
    expect_err(
        "call_expr_bool",
        "True()\n",
        "TypeError",
        "'bool' object is not callable",
        "is not a function",
    );
}

#[test]
fn callable_chain_and_builtins_still_direct() {
    expect_ok(
        "call_chain_ok",
        concat!(
            "def f():\n    return lambda: 42\n",
            "print(f()())\n",
            "print((lambda: 7)())\n",
            "print(abs(-3))\n",
        ),
        "42\n7\n3\n",
    );
}

#[test]
fn call_real_function_still_works() {
    expect_ok(
        "call_fn_ok",
        "def f():\n    return 42\nprint(f())\n",
        "42\n",
    );
}

#[test]
fn call_function_valued_variable_still_works() {
    // A variable holding a function must keep working even though the
    // callee is not a known def name.
    expect_ok(
        "call_var_fn_ok",
        "def f():\n    return 7\ng = f\nprint(g())\n",
        "7\n",
    );
}

// ── #473: iterating a non-iterable ───────────────────────────────────────

#[test]
fn for_over_int_raises_python_typeerror() {
    expect_err(
        "iter_int",
        "n = 5\nfor y in n:\n    print(y)\n",
        "TypeError",
        "'int' object is not iterable",
        "n is not iterable",
    );
}

#[test]
fn for_over_float_raises_python_typeerror() {
    expect_err(
        "iter_float",
        "x = 2.5\nfor y in x:\n    print(y)\n",
        "TypeError",
        "'float' object is not iterable",
        "x is not iterable",
    );
}

#[test]
fn for_over_none_raises_python_typeerror() {
    expect_err(
        "iter_none",
        "v = None\nfor y in v:\n    print(y)\n",
        "TypeError",
        "'NoneType' object is not iterable",
        "v is not iterable",
    );
}

#[test]
fn for_over_list_still_works() {
    expect_ok(
        "iter_list_ok",
        "for x in [1, 2, 3]:\n    print(x)\n",
        "1\n2\n3\n",
    );
}

#[test]
fn for_over_string_still_works() {
    // str is the one Primitive that IS iterable — the pyForIter wrap must
    // pass it through (code points, like Python).
    expect_ok(
        "iter_str_ok",
        "s = \"abc\"\nfor c in s:\n    print(c)\n",
        "a\nb\nc\n",
    );
}

// ── #471: sequence replication by an absurd count ────────────────────────

#[test]
fn list_repeat_huge_count_raises_overflowerror() {
    expect_err(
        "repeat_huge",
        "xs = [1] * (10**30)\nprint(len(xs))\n",
        "OverflowError",
        "cannot fit 'int' into an index-sized integer",
        "Invalid array length",
    );
}

#[test]
fn str_repeat_huge_count_raises_overflowerror() {
    expect_err(
        "repeat_str_huge",
        "s = \"ab\" * (10**30)\nprint(len(s))\n",
        "OverflowError",
        "cannot fit 'int' into an index-sized integer",
        "Invalid string length",
    );
}

// ── attribute access on a primitive with no such attribute ───────────────

#[test]
fn attr_on_int_raises_python_attributeerror() {
    expect_err(
        "attr_int",
        "x = 5\nprint(x.foo)\n",
        "AttributeError",
        "'int' object has no attribute 'foo'",
        "undefined",
    );
}

#[test]
fn attr_on_float_raises_python_attributeerror() {
    expect_err(
        "attr_float",
        "x = 2.5\nprint(x.foo)\n",
        "AttributeError",
        "'float' object has no attribute 'foo'",
        "undefined",
    );
}

#[test]
fn attr_on_str_raises_python_attributeerror() {
    expect_err(
        "attr_str",
        "s = \"a\"\nprint(s.foo)\n",
        "AttributeError",
        "'str' object has no attribute 'foo'",
        "undefined",
    );
}

#[test]
fn attr_on_bool_raises_python_attributeerror() {
    expect_err(
        "attr_bool",
        "b = True\nprint(b.foo)\n",
        "AttributeError",
        "'bool' object has no attribute 'foo'",
        "undefined",
    );
}

#[test]
fn attr_on_none_raises_python_attributeerror() {
    expect_err(
        "attr_none",
        "v = None\nprint(v.foo)\n",
        "AttributeError",
        "'NoneType' object has no attribute 'foo'",
        "undefined",
    );
}

#[test]
fn real_attribute_and_method_access_still_work() {
    expect_ok(
        "attr_ok",
        concat!(
            "print(\"a\".upper())\n",
            "xs = []\nxs.append(3)\nprint(xs)\n",
            "class C:\n    def __init__(self):\n        self.v = 9\n",
            "c = C()\nprint(c.v)\n",
            "n = 5\nprint(n.real)\n",
            "x = 2.5\nprint(x.is_integer())\n",
            "d = {\"a\": 1}\nprint(d.get(\"a\"))\n",
            // Review round 2 (the coverage gap that let the over-raise
            // blocker slip): int/bool Rational protocol — CPython values.
            "print(n.numerator)\n",
            "print(n.denominator)\n",
            "m = -7\nprint(m.numerator)\n",
            "b = True\nprint(b.numerator)\n",
            "f = False\nprint(f.numerator)\n",
            "print(b.denominator)\n",
            "big = 10**30\nprint(big.numerator == big)\n",
        ),
        "A\n[3]\n9\n5\nFalse\n1\n5\n1\n-7\n1\n0\n1\nTrue\n",
    );
}

#[test]
fn attr_on_native_containers_raises_python_attributeerror() {
    // Round 3 (corpus deep-close): absent attributes on native Python
    // containers were a SILENT OK(None) — a C3 occurrence violation.
    expect_err(
        "attr_list",
        "xs = [1]\nprint(xs.foo)\n",
        "AttributeError",
        "'list' object has no attribute 'foo'",
        "undefined",
    );
    expect_err(
        "attr_dict",
        "d = {\"a\": 1}\nprint(d.foo)\n",
        "AttributeError",
        "'dict' object has no attribute 'foo'",
        "undefined",
    );
    expect_err(
        "attr_set",
        "s = {1, 2}\nprint(s.foo)\n",
        "AttributeError",
        "'set' object has no attribute 'foo'",
        "undefined",
    );
    expect_err(
        "attr_tuple",
        "t = (1, 2)\nprint(t.foo)\n",
        "AttributeError",
        "'tuple' object has no attribute 'foo'",
        "undefined",
    );
    // tuple has ONLY count/index — list methods must NOT leak onto it.
    expect_err(
        "attr_tuple_append",
        "t = (1, 2)\nprint(t.append)\n",
        "AttributeError",
        "'tuple' object has no attribute 'append'",
        "undefined",
    );
}

#[test]
fn container_methods_and_user_objects_still_work() {
    expect_ok(
        "attr_container_ok",
        concat!(
            // method CALLS (compile-lowered) keep working
            "xs = []\nxs.append(1)\nprint(xs)\n",
            "d = {\"a\": 1}\nprint(d.get(\"a\"))\n",
            "print(list(d.keys()))\n",
            // method REFERENCES resolve and mutate the receiver
            "m = xs.append\nm(2)\nprint(xs)\n",
            "idx = [10, 20].index\nprint(idx(20))\n",
            "cnt = (1, 2, 1).count\nprint(cnt(1))\n",
            "sd = d.setdefault\nsd(\"b\", 9)\nprint(d.get(\"b\"))\n",
            "s = {1, 2}\ndis = s.discard\ndis(1)\nprint(sorted(s))\n",
            "print(sorted(s.union({7})))\n",
            // user class instances keep the undefined pass-through
            // (hasattr/getattr-default idioms must not start raising)
            "class C:\n    pass\n",
            "c = C()\nprint(getattr(c, \"zz\", \"fallback\"))\n",
            "print(hasattr(c, \"zz\"))\n",
        ),
        "[1]\n1\n['a']\n[1, 2]\n1\n2\n9\n[2]\n[2, 7]\nfallback\nFalse\n",
    );
}

#[test]
fn float_numerator_still_raises_attributeerror() {
    // CPython: float has NO numerator/denominator — the int/bool Rational
    // arms must not leak into the float receiver.
    expect_err(
        "attr_float_numerator",
        "x = 2.5\nprint(x.numerator)\n",
        "AttributeError",
        "'float' object has no attribute 'numerator'",
        "undefined",
    );
}

// ── `%` by zero message (CPython: modulo, not division-or-modulo) ────────

#[test]
fn int_modulo_by_zero_message_matches_cpython() {
    expect_err(
        "mod_zero",
        "print(1 % 0)\n",
        "ZeroDivisionError",
        "integer modulo by zero",
        "integer division or modulo by zero",
    );
}

#[test]
fn floor_div_by_zero_message_unchanged() {
    expect_err(
        "floordiv_zero",
        "print(1 // 0)\n",
        "ZeroDivisionError",
        "integer division or modulo by zero",
        "% is not",
    );
}

#[test]
fn modulo_still_works() {
    expect_ok(
        "mod_ok",
        "print(7 % 3)\nprint(-7 % 3)\nprint(2.5 % 2)\n",
        "1\n2\n0.5\n",
    );
}

#[test]
fn list_repeat_small_count_still_works() {
    expect_ok(
        "repeat_small_ok",
        "xs = [0] * 3\nprint(xs)\nprint(\"ab\" * 2)\n",
        "[0, 0, 0]\nabab\n",
    );
}
