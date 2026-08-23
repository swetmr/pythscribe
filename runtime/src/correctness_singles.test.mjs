// Regression coverage for the correctness-singles fix batch (B3/B4/B6),
// exercising the PACKAGE runtime helpers directly (the path `pyths compile`
// imports). The behavioral_differential suite covers the inline `pyths run`
// path; these lock the same three bugs at the package-helper level so the two
// copies can't silently diverge.
import { test } from "node:test";
import assert from "node:assert/strict";
import { PyObject, __pyClass, __pySuper } from "./classes.js";
import { pyListOf, pyTupleOf } from "./operators.js";
import { pyStrFormat } from "./runtime.js";

// ---- B3: diamond-inheritance method resolution ----
// Faithfully mirror the codegen's class emission: `class X extends <first-base>`
// + `__pyClass(X, [bases])`, with `super()` lowered to `__pySuper(X, this)`.
function buildDiamond(dHasWho) {
    class A extends PyObject {
        who() { return "A"; }
    }
    __pyClass(A, []);
    class B extends A {}
    __pyClass(B, [A]);
    class C extends A {
        who() { return "C"; }
    }
    __pyClass(C, [A]);
    let D;
    if (dHasWho) {
        D = class extends B {
            who() { return "D" + __pySuper(D, this).who(); }
        };
    } else {
        D = class extends B {};
    }
    __pyClass(D, [B, C]);
    return D;
}

test("B3: D(B, C) resolves who() to C's genuine override, not B's flattened A-copy", () => {
    const D = buildDiamond(false);
    assert.equal(new D().who(), "C");
});

test("B3: cooperative super() in a diamond reaches C, not B's inherited copy", () => {
    const D = buildDiamond(true);
    assert.equal(new D().who(), "DC");
});

test("B3: cooperative chain B->C->A composes left-to-right (super in every branch)", () => {
    class A extends PyObject {
        who() { return "A"; }
    }
    __pyClass(A, []);
    let B, C;
    B = class extends A {
        who() { return "B" + __pySuper(B, this).who(); }
    };
    __pyClass(B, [A]);
    C = class extends A {
        who() { return "C" + __pySuper(C, this).who(); }
    };
    __pyClass(C, [A]);
    class D extends B {}
    __pyClass(D, [B, C]);
    assert.equal(new D().who(), "BCA");
});

// ---- B4: list()/tuple() over a plain-shaped dict ----
test("B4: pyListOf on a plain-object dict yields the keys", () => {
    assert.deepEqual(pyListOf({ b: 1, a: 2 }), ["b", "a"]);
});
test("B4: pyTupleOf on a plain-object dict yields the keys (no crash)", () => {
    const t = pyTupleOf({ b: 1, a: 2 });
    assert.deepEqual([...t], ["b", "a"]);
    assert.equal(t.__pytuple__, true);
});
test("B4: pyListOf copies an array (independent of source) and unmarks tuples", () => {
    const xs = [1, 2, 3];
    const ys = pyListOf(xs);
    ys.push(4);
    assert.deepEqual(xs, [1, 2, 3]);
    assert.deepEqual(ys, [1, 2, 3, 4]);
    const tup = pyTupleOf([7, 8]);
    const lst = pyListOf(tup);
    assert.equal(lst.__pytuple__, undefined);
});
test("B4: pyListOf preserves real-iterable behavior (string/set/Map-dict)", () => {
    assert.deepEqual(pyListOf("hi"), ["h", "i"]);
    assert.deepEqual(pyListOf(new Set([1, 2, 3])).sort(), [1, 2, 3]);
    assert.deepEqual(pyListOf(new Map([[1, "a"], [2, "b"]])), [1, 2]); // dict keys
});

// ---- B6: str.format() specs, escaping, conversions ----
test("B6: format honors width/precision/padding specs", () => {
    assert.equal(pyStrFormat("{}-{:03d}", "x", 7), "x-007");
    assert.equal(pyStrFormat("{0:>5}|{1:.2f}", "hi", 3.14159), "   hi|3.14");
    assert.equal(pyStrFormat("{:+.1f}", 3.14159), "+3.1");
});
test("B6: format handles brace escaping and named fields", () => {
    assert.equal(pyStrFormat("{{literal}} {}", "x"), "{literal} x");
    assert.equal(pyStrFormat("{name:>6}", { name: "hi" }), "    hi");
});
test("B6: format handles !r conversion and index/attr access", () => {
    assert.equal(pyStrFormat("{!r}", "a"), "'a'");
    assert.equal(pyStrFormat("{0[1]}", ["a", "b", "c"]), "b");
});

// ---- Bound-method review fixes (B1/S1/S2/S3) ----
import { pyBoundMethod, pyGetattr, __pyMarkTuple } from "./runtime.js";
import { pyEq } from "./operators.js";

test("B1: pyBoundMethod fires a property getter exactly ONCE", () => {
    let count = 0;
    class P extends PyObject {
        get val() { count += 1; return 42; }
    }
    __pyClass(P, []);
    const p = new P();
    assert.equal(pyBoundMethod(p, "val"), 42);
    assert.equal(count, 1);
});

test("S1: a.f == a.f is True, a.f in [a.f] is True, a.f is a.f is False", () => {
    class A extends PyObject {
        f() { return 1; }
    }
    __pyClass(A, []);
    const a = new A();
    const m1 = pyBoundMethod(a, "f");
    const m2 = pyBoundMethod(a, "f");
    assert.notEqual(m1, m2);          // `is` → distinct wrapper objects
    assert.equal(pyEq(m1, m2), true); // `==` → same func + same receiver
    assert.equal([m1].some((x) => pyEq(x, m2)), true); // `in`
    const b = new A();
    assert.equal(pyEq(pyBoundMethod(a, "f"), pyBoundMethod(b, "f")), false);
    assert.equal(pyEq(m1, A.prototype.f), false); // method != plain function
    // getattr() path carries the same identity stamps
    assert.equal(pyEq(pyGetattr(a, "f"), m1), true);
});

test("S2: __pyMarkTuple marks a rest array as a tuple (idempotent)", () => {
    const args = [1, 2];
    assert.equal(__pyMarkTuple(args), args);
    assert.equal(args.__pytuple__, true);
    assert.equal(Object.keys(args).length, 2); // marker non-enumerable
    __pyMarkTuple(args); // second call is a no-op, not a redefine error
});

test("S3: a function stored as instance DATA is returned unbound (identity kept)", () => {
    const freefn = () => "free";
    class A extends PyObject {
        __init__() { this.cb = freefn; }
        m() { return this; }
    }
    __pyClass(A, []);
    const a = new A();
    assert.equal(pyBoundMethod(a, "cb"), freefn); // own property → not bound
    const g = pyBoundMethod(a, "m");              // prototype method → bound
    assert.equal(g(), a);
});
