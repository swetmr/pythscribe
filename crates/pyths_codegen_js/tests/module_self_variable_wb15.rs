//! WB-15 — a bare `self` is JS `this` ONLY as an instance-method receiver.
//!
//! At module scope (or in a plain function / static method), `self` is an
//! ORDINARY variable and must emit `self`. The old codegen rewrote every
//! `Name("self")` to `this`, so `self = {}` at module top lowered to
//! `export let this = ({})` — a HARD SYNTAX ERROR that silently aborted the
//! whole module (web-bench pull-loading's `var self = this` gesture bag; the
//! touch handlers never attached, transform stayed `none`). Instance-method
//! `self.x` → `this.x` must be preserved; a nested fn inside a method keeps the
//! `__self` alias.
//!
//! The fix replaced FOUR interacting flags with ONE order-independent predicate
//! (`SelfLowering`, computed from the enclosing method's real receiver). This
//! suite pins the WHOLE class it governs — not just the happy path — with
//! BEHAVIORAL assertions: each case is compiled with the runtime inlined and
//! RUN under node, and its stdout is compared to CPython's (skipped when node
//! is unavailable, same policy as behavioral_differential.rs). Structural tests
//! additionally pin the exact lowering (`this` / `__self` / `self`) so a future
//! refactor cannot silently drift the identity back.

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

/// The inlined runtime prelude contains its own `this`/`self` references; the
/// negative assertions must only look at the emitted USER code, which follows
/// the `--- End Runtime ---` marker.
fn user_code(js: &str) -> &str {
    match js.split_once("End Runtime") {
        Some((_, rest)) => rest,
        None => js,
    }
}

/// Compile with the runtime INLINED (so the module runs standalone) and execute
/// it under node, returning its stdout. Returns None when node is unavailable.
/// This is the behavioral oracle: the emitted `self`-lowering must produce the
/// same value CPython does, not merely parse.
fn node_run(source: &str, name: &str) -> Option<String> {
    let js = compile_inline(source);
    let dir = std::env::temp_dir().join(format!("pyths_wb15run_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.mjs"));
    std::fs::write(&path, &js).unwrap();
    let out = Command::new("node").arg(&path).output().ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "compiled module `{name}` must RUN cleanly under node (rc {:?}):\n{stderr}\n--- JS ---\n{js}",
        out.status.code()
    );
    Some(stdout)
}

/// Behavioral case: (name, source, expected-stdout). Each is the exact CPython
/// output. Every branch of the `SelfLowering` predicate is exercised.
const BEHAVIORAL: &[(&str, &str, &str)] = &[
    // ── module scope: `self` is an ORDINARY variable (the original bug) ──
    (
        "module_self_bag_mutates",
        "self = {}\nself[\"ready\"] = True\ndef move():\n    if \"ready\" in self and self[\"ready\"]:\n        self[\"moved\"] = True\nmove()\nprint(self[\"moved\"])\n",
        "True\n",
    ),
    (
        "module_self_global_augassign",
        "self = 10\ndef bump():\n    global self\n    self += 1\nbump()\nprint(self)\n",
        "11\n",
    ),
    (
        "plain_fn_nested_free_self",
        "self = {\"x\": 7}\ndef f():\n    def g():\n        return self[\"x\"]\n    return g()\nprint(f())\n",
        "7\n",
    ),
    // ── instance methods: receiver `self` → `this` (no regression) ──
    (
        "instance_receiver_this",
        "class C:\n    def __init__(self, n):\n        self.n = n\n    def inc(self):\n        self.n += 1\n        return self.n\nc = C(1)\nprint(c.inc())\nprint(c.inc())\n",
        "2\n3\n",
    ),
    (
        "super_inheritance",
        "class A:\n    def __init__(self, x):\n        self.x = x\nclass B(A):\n    def __init__(self, x, y):\n        super().__init__(x)\n        self.y = y\n    def total(self):\n        return self.x + self.y\nprint(B(3, 4).total())\n",
        "7\n",
    ),
    (
        "property_get_set",
        "class T:\n    def __init__(self):\n        self._c = 0\n    @property\n    def c(self):\n        return self._c\n    @c.setter\n    def c(self, v):\n        self._c = v\nt = T()\nt.c = 25\nprint(t.c)\n",
        "25\n",
    ),
    (
        "lambda_in_method_self",
        "class C:\n    def __init__(self):\n        self.k = 3\n    def make(self):\n        return lambda x: x + self.k\nprint(C().make()(10))\n",
        "13\n",
    ),
    (
        "comprehension_in_method_self",
        "class C:\n    def __init__(self):\n        self.base = 100\n    def add(self, xs):\n        return [x + self.base for x in xs]\nprint(sum(C().add([1, 2])))\n",
        "203\n",
    ),
    (
        "nested_fn_in_instance_method_alias",
        "class W:\n    def __init__(self):\n        self.value = 55\n    def setup(self):\n        def cb():\n            return self.value\n        return cb()\nprint(W().setup())\n",
        "55\n",
    ),
    // ── @staticmethod: no receiver — free `self` stays the module var ──
    (
        "static_direct_free_self_is_module",
        "self = {\"tag\": 7}\nclass C:\n    @staticmethod\n    def m():\n        return self[\"tag\"]\nprint(C.m())\n",
        "7\n",
    ),
    // BLOCKER 1: nested fn one level deeper inside a @staticmethod must NOT
    // alias the class — free `self` is still the module var.
    (
        "static_nested_fn_free_self_is_module",
        "self = {\"tag\": 111}\nclass C:\n    @staticmethod\n    def m():\n        def inner():\n            return self[\"tag\"]\n        return inner()\nprint(C.m())\n",
        "111\n",
    ),
    // S1: a real param named `self` on a @staticmethod is an ORDINARY param —
    // it must be KEPT in the signature, not dropped (else ReferenceError).
    (
        "static_self_param_is_ordinary",
        "class C:\n    @staticmethod\n    def m(self):\n        return self + 1\nprint(C.m(32))\n",
        "33\n",
    ),
    // ── @classmethod ──
    (
        "classmethod_direct_free_self_is_module",
        "self = {\"v\": 222}\nclass C:\n    @classmethod\n    def m(cls):\n        return self[\"v\"]\nprint(C.m())\n",
        "222\n",
    ),
    // BLOCKER 1 (classmethod variant): nested fn inside a @classmethod.
    (
        "classmethod_nested_fn_free_self_is_module",
        "self = {\"tag\": 333}\nclass C:\n    @classmethod\n    def m(cls):\n        def inner():\n            return self[\"tag\"]\n        return inner()\nprint(C.m())\n",
        "333\n",
    ),
    // BLOCKER 2 (regression): a @classmethod whose first param is named `self`
    // (legal Python — binds the CLASS) must resolve `self` to the class, not
    // throw ReferenceError.
    (
        "classmethod_self_param_binds_class",
        "class C:\n    x = 5\n    @classmethod\n    def m(self):\n        return self.x\nprint(C.m())\n",
        "5\n",
    ),
    // ── receiver named something other than `self` ──
    (
        "instance_first_param_not_self",
        "self = {\"v\": 8}\nclass C:\n    def m(me):\n        return self[\"v\"]\nprint(C().m())\n",
        "8\n",
    ),
    // ── method default value: free `self` is the def/class-scope module var ──
    (
        "method_default_free_self_is_module",
        "self = {\"a\": 77}\nclass C:\n    def m(self, y=self[\"a\"]):\n        return y\nprint(C().m())\n",
        "77\n",
    ),
    // ── class-in-method ──
    (
        "class_in_method_inner_instance",
        "class Outer:\n    def __init__(self):\n        self.tag = \"outer\"\n    def m(self):\n        class Inner:\n            def __init__(self):\n                self.tag = \"inner\"\n            def who(self):\n                return self.tag\n        return Inner().who()\nprint(Outer().m())\n",
        "inner\n",
    ),
    // S3: an inner @staticmethod closing over the OUTER method's receiver `self`
    // must read it via the outer `__self` alias.
    (
        "class_in_method_inner_static_outer_self",
        "class Outer:\n    def __init__(self):\n        self.tag = \"OUTER\"\n    def m(self):\n        class Inner:\n            @staticmethod\n            def s():\n                return self.tag\n        return Inner.s()\nprint(Outer().m())\n",
        "OUTER\n",
    ),
    // S2: a nested fn inside an EXCEPTION-subclass __init__ (native constructor)
    // reads the receiver via `__self` captured after super().
    (
        "exc_ctor_nested_fn_self",
        "class MyErr(Exception):\n    def __init__(self, code):\n        super().__init__(\"err\")\n        self.code = code\n        def show():\n            return self.code\n        self.show = show\nprint(MyErr(7).show())\n",
        "7\n",
    ),
    // ── restore discipline: static → instance → static in one class ──
    (
        "interleaved_static_instance_static",
        "self = {\"m\": 5}\nclass C:\n    @staticmethod\n    def s():\n        return self[\"m\"]\n    def i(self):\n        return 99\n    @staticmethod\n    def s2():\n        return self[\"m\"] + 1\nprint(C.s(), C().i(), C.s2())\n",
        "5 99 6\n",
    ),
    // ── local BINDERS of `self` inside a method shadow the receiver (round-2
    //    review S4/S5/S6): a `self` bound as a lambda param, comprehension
    //    target, or local assignment is an ordinary identifier, not `this`. ──
    // S4: lambda param named `self` shadows the receiver.
    (
        "s4_lambda_param_self_shadows",
        "class C:\n    def m(self):\n        f = lambda self: self * 2\n        return f(21)\nprint(C().m())\n",
        "42\n",
    ),
    // S5: comprehension target named `self` → binder position must be `self`,
    // not `this` (which is a SyntaxError as an arrow param).
    (
        "s5_comprehension_target_self",
        "class C:\n    def m(self):\n        return [self for self in [1, 2, 3]]\nprint(sum(C().m()))\n",
        "6\n",
    ),
    // S6: local assignment `self = 5` in a method → `self` is an ordinary local
    // (JS `this` is not assignable) initialized to the receiver.
    (
        "s6_local_assign_self",
        "class C:\n    def m(self):\n        self = 5\n        return self\nprint(C().m())\n",
        "5\n",
    ),
    // S6 (mixed): read the receiver via `self.attr`, THEN rebind `self` — the
    // `let self = this` capture makes the pre-rebind read see the instance.
    (
        "s6_read_receiver_then_rebind",
        "class C:\n    def __init__(self):\n        self.x = 9\n    def m(self):\n        y = self.x\n        self = 5\n        return y + self\nprint(C().m())\n",
        "14\n",
    ),
    // S6 (for-target): a regular `for self in xs` in a method rebinds the local.
    (
        "s6_for_target_self",
        "class C:\n    def m(self):\n        total = 0\n        for self in [10, 20, 30]:\n            total += self\n        return total\nprint(C().m())\n",
        "60\n",
    ),
    // Nested comprehension binding `self` at the inner level.
    (
        "nested_comprehension_self",
        "class C:\n    def m(self):\n        return [[self for self in row] for row in [[1, 2], [3, 4]]]\nprint(C().m())\n",
        "[[1, 2], [3, 4]]\n",
    ),
    // A method with BOTH a self-shadowing lambda and a lambda using the REAL
    // receiver `self` — the shadow must not leak into the receiver-using one.
    (
        "lambda_shadowed_vs_real_self",
        "class C:\n    def __init__(self):\n        self.k = 100\n    def m(self):\n        shadow = lambda self: self * 2\n        real = lambda x: x + self.k\n        return shadow(3) + real(1)\nprint(C().m())\n",
        "107\n",
    ),
    // Comprehension whose TARGET is `self` but whose OUTERMOST iterable reads the
    // REAL receiver `self` — iterable stays enclosing (`this`), target is `self`.
    (
        "comprehension_target_self_iterable_receiver",
        "class C:\n    def __init__(self):\n        self.items = [1, 2, 3]\n    def m(self):\n        return sum([self for self in self.items])\nprint(C().m())\n",
        "6\n",
    ),
    // dict comprehension with a `self` target.
    (
        "dict_comprehension_target_self",
        "class C:\n    def m(self):\n        return {self: self * self for self in [2, 3]}\nprint(C().m())\n",
        "{2: 4, 3: 9}\n",
    ),
    // generator expression with a `self` target.
    (
        "genexp_target_self",
        "class C:\n    def m(self):\n        return sum(self for self in [4, 5, 6])\nprint(C().m())\n",
        "15\n",
    ),
    // multi-generator (loop-path) comprehension with a `self` target and a
    // LITERAL outermost iterable.
    (
        "comprehension_loop_path_target_self",
        "class C:\n    def m(self):\n        return [self for self in [1, 2] for _ in [0]]\nprint(sum(C().m()))\n",
        "3\n",
    ),
    // ── B3 (round-3 review): the loop path (2+ ifs / walrus / multi-generator)
    //    must evaluate the OUTERMOST iterable in the ENCLOSING scope, so a
    //    receiver-reading outer iterable (`for self in self.items`) keeps `this`
    //    while the `self` target is the ordinary comprehension variable. ──
    // Multi-generator: outer `for x in self.a`, inner target rebinds `self`.
    (
        "b3_loop_multigen_receiver_iterable",
        "class C:\n    def __init__(self):\n        self.a = [1, 2]\n    def m(self):\n        return [self * 10 for x in self.a for self in [x, x + 1]]\nprint(C().m())\n",
        "[10, 20, 20, 30]\n",
    ),
    // Two ifs force the loop path; outer iterable `self.items` reads the receiver.
    (
        "b3_loop_two_ifs_receiver_iterable",
        "class C:\n    def __init__(self):\n        self.items = [1, 2, 15]\n    def m(self):\n        return [self * 2 for self in self.items if self > 0 if self < 10]\nprint(C().m())\n",
        "[2, 4]\n",
    ),
    // Walrus forces the loop path; outer iterable `self.items` reads the receiver.
    (
        "b3_loop_walrus_receiver_iterable",
        "class C:\n    def __init__(self):\n        self.items = [1, 2, 3]\n    def m(self):\n        return [y for self in self.items if (y := self * 2) > 2]\nprint(C().m())\n",
        "[4, 6]\n",
    ),
    // Dict comprehension loop path (2 ifs) with a receiver-reading outer iterable.
    (
        "b3_loop_dict_receiver_iterable",
        "class C:\n    def __init__(self):\n        self.items = [1, 2, 3]\n    def m(self):\n        return {self: self * 2 for self in self.items if self > 1 if self < 3}\nprint(C().m())\n",
        "{2: 4}\n",
    ),
    // Set comprehension loop path (2 ifs) with a receiver-reading outer iterable.
    (
        "b3_loop_set_receiver_iterable",
        "class C:\n    def __init__(self):\n        self.items = [1, 2, 3]\n    def m(self):\n        return sorted({self * 2 for self in self.items if self > 0 if self < 3})\nprint(C().m())\n",
        "[2, 4]\n",
    ),
];

#[test]
fn wb15_self_lowering_behavioral_corpus() {
    // node-run oracle: compile (runtime inlined) + execute + compare stdout.
    let mut failures = Vec::new();
    let mut ran = false;
    for (name, src, expected) in BEHAVIORAL {
        match node_run(src, name) {
            None => {
                // node unavailable → skip the whole behavioral pass.
                eprintln!("node unavailable; skipping wb15 behavioral corpus");
                return;
            }
            Some(got) => {
                ran = true;
                if got != *expected {
                    failures.push(format!("{name}: got {got:?} want {expected:?}"));
                }
            }
        }
    }
    assert!(ran, "no behavioral case ran");
    assert!(
        failures.is_empty(),
        "{} WB-15 behavioral difference(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ── structural: pin the exact lowering identity (this / __self / self) ───────

#[test]
fn module_level_self_is_an_ordinary_variable() {
    let full = compile_inline("self = {}\nself[\"x\"] = 1\n");
    let js = user_code(&full);
    assert!(
        js.contains("let self = ") && !js.contains("let this"),
        "module-level `self = {{}}` must bind `self`, never `let this` (a syntax error):\n{js}"
    );
    assert!(
        js.contains("(self, \"x\", 1)") && !js.contains("(this,"),
        "module-level `self[...]` must reference `self`, not `this`:\n{js}"
    );
}

#[test]
fn plain_function_free_self_is_the_module_variable() {
    let full = compile_inline("self = {}\ndef f():\n    return self[\"x\"]\n");
    let js = user_code(&full);
    assert!(
        js.contains("(self, \"x\")") && !js.contains("(this,"),
        "a plain function's free `self` must read the module var, not the call-site `this`:\n{js}"
    );
}

#[test]
fn instance_method_self_is_this() {
    let full = compile_inline(
        "class C:\n    def __init__(self, n):\n        self.n = n\n    def inc(self):\n        return self.n\n",
    );
    let js = user_code(&full);
    assert!(
        js.contains("this.n = n") && js.contains("(this, \"n\")"),
        "an instance-method receiver `self` must still lower to `this`:\n{js}"
    );
}

#[test]
fn static_method_free_self_is_not_this() {
    let full = compile_inline(
        "self = {}\nclass C:\n    @staticmethod\n    def m():\n        return self[\"tag\"]\n",
    );
    let js = user_code(&full);
    assert!(
        js.contains("(self, \"tag\")") && !js.contains("(this, \"tag\")"),
        "a @staticmethod has no receiver; a free `self` must stay the module var:\n{js}"
    );
}

#[test]
fn nested_fn_in_method_keeps_self_alias() {
    let full = compile_inline(
        "class W:\n    def setup(self):\n        def cb():\n            return self.value\n        return cb\n",
    );
    let js = user_code(&full);
    assert!(
        js.contains("const __self = this") && js.contains("(__self, \"value\")"),
        "a nested fn inside a method must alias the receiver via `__self`:\n{js}"
    );
}

// BLOCKER 1: the alias must NOT appear for a nested fn inside a @staticmethod —
// its free `self` is the ordinary module identifier, never `__self`.
#[test]
fn nested_fn_in_static_method_has_no_self_alias() {
    let full = compile_inline(
        "self = {}\nclass C:\n    @staticmethod\n    def m():\n        def inner():\n            return self[\"tag\"]\n        return inner()\n",
    );
    let js = user_code(&full);
    assert!(
        !js.contains("__self") && js.contains("(self, \"tag\")"),
        "a @staticmethod with a nested fn must NOT emit `__self`; free `self` stays the module var:\n{js}"
    );
}

// BLOCKER 1 (classmethod): same, one level deep inside a @classmethod.
#[test]
fn nested_fn_in_classmethod_free_self_is_not_alias() {
    let full = compile_inline(
        "self = {}\nclass C:\n    @classmethod\n    def m(cls):\n        def inner():\n            return self[\"tag\"]\n        return inner()\n",
    );
    let js = user_code(&full);
    assert!(
        !js.contains("__self") && js.contains("(self, \"tag\")"),
        "a @classmethod's nested-fn free `self` must be the module var, not `__self`:\n{js}"
    );
}

// BLOCKER 2 (regression): a @classmethod whose first param is `self` binds the
// class via `const self = this` and KEEPS `self` referenceable.
#[test]
fn classmethod_self_param_aliases_class() {
    let full = compile_inline(
        "class C:\n    x = 5\n    @classmethod\n    def m(self):\n        return self.x\n",
    );
    let js = user_code(&full);
    assert!(
        js.contains("const self = this"),
        "a @classmethod `def m(self)` must bind its first param to the class (`const self = this`):\n{js}"
    );
    assert!(
        !js.contains("const cls = this"),
        "the class alias must use the ACTUAL first-param name, not a hardcoded `cls`:\n{js}"
    );
}

// S1: a @staticmethod's `self` param is KEPT in the JS signature (not dropped).
#[test]
fn static_method_self_param_is_kept() {
    let full =
        compile_inline("class C:\n    @staticmethod\n    def m(self):\n        return self\n");
    let js = user_code(&full);
    assert!(
        js.contains("static m(self)"),
        "a @staticmethod's `self` param is an ordinary param and must be kept in the signature:\n{js}"
    );
}

// S2: an exception-subclass __init__ with a nested fn captures `__self` AFTER
// super() (a derived constructor may not touch `this` before super()).
#[test]
fn exc_ctor_nested_fn_captures_self_after_super() {
    let full = compile_inline(
        "class MyErr(Exception):\n    def __init__(self, code):\n        super().__init__(\"err\")\n        self.code = code\n        def show():\n            return self.code\n        self.show = show\n",
    );
    let js = user_code(&full);
    let super_pos = js.find("super(").expect("super() emitted");
    let alias_pos = js
        .find("const __self = this")
        .expect("__self alias emitted");
    assert!(
        alias_pos > super_pos,
        "the `__self` alias must be captured AFTER super() in a derived constructor:\n{js}"
    );
    assert!(
        js.contains("(__self, \"code\")"),
        "the nested fn in the constructor must read the receiver via `__self`:\n{js}"
    );
}

// S3: an inner @staticmethod reads the OUTER method receiver via the outer
// `__self` alias (the enclosing instance method captures it for the nested class).
#[test]
fn class_in_method_inner_static_uses_outer_alias() {
    let full = compile_inline(
        "class Outer:\n    def m(self):\n        class Inner:\n            @staticmethod\n            def s():\n                return self.tag\n        return Inner.s()\n",
    );
    let js = user_code(&full);
    assert!(
        js.contains("const __self = this") && js.contains("(__self, \"tag\")"),
        "an inner static closing over the outer receiver must read it via the outer `__self`:\n{js}"
    );
}

// S4: a lambda param `self` in a method emits `(self) => … self …`, NEVER `this`.
#[test]
fn lambda_param_self_shadows_receiver() {
    let full = compile_inline("class C:\n    def m(self):\n        return lambda self: self * 2\n");
    let js = user_code(&full);
    assert!(
        js.contains("(self) =>") && !js.contains("(this)") && !js.contains("=> pyMul(this"),
        "a lambda param `self` shadows the receiver; body + binder must be `self`, not `this`:\n{js}"
    );
}

// S5: a comprehension target `self` emits as an arrow param `self` (a bare `this`
// arrow param is a JS SyntaxError), and the element `self` reads it — never `this`.
#[test]
fn comprehension_target_self_is_ordinary() {
    let full =
        compile_inline("class C:\n    def m(self):\n        return [self for self in [1, 2, 3]]\n");
    let js = user_code(&full);
    assert!(
        js.contains("(self) => self") && !js.contains("(this) =>"),
        "a comprehension target `self` must be an ordinary arrow param `self`, not `this`:\n{js}"
    );
}

// S6: a method-local rebind of `self` captures the receiver as `let self = this;`
// and reassigns it — never `let this = 5` (a SyntaxError).
#[test]
fn method_local_rebind_self_captures_receiver() {
    let full =
        compile_inline("class C:\n    def m(self):\n        self = 5\n        return self\n");
    let js = user_code(&full);
    assert!(
        js.contains("let self = this;") && js.contains("self = 5;") && !js.contains("let this"),
        "a method-local `self = 5` must capture `let self = this;` then reassign `self`, never `let this`:\n{js}"
    );
}

// S5 subtlety: a comprehension whose target is `self` but whose OUTERMOST
// iterable reads the receiver `self.attr` keeps the iterable as `this` while the
// target/element are the ordinary `self`.
#[test]
fn comprehension_target_self_keeps_iterable_receiver() {
    let full = compile_inline(
        "class C:\n    def m(self):\n        return [self for self in self.items]\n",
    );
    let js = user_code(&full);
    assert!(
        js.contains("(this, \"items\")") && js.contains("(self) => self"),
        "the outermost iterable `self.items` stays the receiver (`this`) while the target is `self`:\n{js}"
    );
}

// B3: on the LOOP path (2+ ifs force it here), the outermost iterable
// `self.items` must lower to the receiver (`this`) — emitted as the enclosing
// IIFE argument — while the loop target `self` reads the comprehension variable
// (`for (const self of __comp_it)`), never the receiver.
#[test]
fn comprehension_loop_path_outer_iterable_is_receiver() {
    let full = compile_inline(
        "class C:\n    def m(self):\n        return [self * 2 for self in self.items if self > 0 if self < 10]\n",
    );
    let js = user_code(&full);
    assert!(
        js.contains("(this, \"items\")"),
        "the loop-path outermost iterable `self.items` must lower to the receiver `this`:\n{js}"
    );
    assert!(
        js.contains("for (const self of __comp_it)"),
        "the loop-path outer iterable must be consumed from the enclosing-scope IIFE arg `__comp_it`, with the target as ordinary `self`:\n{js}"
    );
    assert!(
        !js.contains("of pyForIter(pyBoundMethod(self,"),
        "the loop-path outer iterable must NOT read `self.items` under the shadow (was a ReferenceError):\n{js}"
    );
}
