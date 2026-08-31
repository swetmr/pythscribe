// F1 (v0.2.4) — cross-type rich-comparison MATRIX (the E7 conformance net).
//
// CPython raises TypeError when `<`/`<=`/`>`/`>=` operands are not
// order-compatible; the old pyLt/pyLe/pyGt/pyGe final fallback leaked JS
// `<` coercion and returned a silent-wrong boolean (1 < 'a' → False,
// None < 1 → True, [1] < 1 → False, min(3, 'a') → 3, sorted([1, 'a'])
// → [1, 'a']). This matrix pins the full receiver-type cross product for
// all four operators AND every derived comparison surface (min/max,
// sorted, list.sort, heapq, bisect, nested-sequence recursion), so the
// class cannot silently regress.
//
// Goldens generated from live CPython 3.12.7 (2026-08-26).
//
// F1-r2 (v0.2.4): the former tuple-vs-list residual is CLOSED — the array
// arm of pyLt/pyLe/pyGt/pyGe now requires matching __pytuple__ brands, so
// `[1] < (2,)` raises CPython's TypeError ('list' vs 'tuple') instead of
// silently ordering. list↔list and tuple↔tuple still order; the matrix
// below asserts the mixed pair raises for all four operators.
//
// F1 runtime.js half (LANDED, v0.2.4): pySorted / pyListSort / __minmax
// (pyMin/pyMax) are consolidated onto pyLt/pyGt, so the cross-type guard
// reaches every derived comparison surface; the previously-skipped tests
// at the bottom now run.

import test from "node:test";
import assert from "node:assert/strict";

import {
    pyLt, pyLe, pyGt, pyGe,
    pyBytesOf, pyTuple, __pyF,
} from "./operators.js";
import { pyMin, pyMax, pySorted, pyListSort } from "./runtime.js";
import { heappush } from "./stdlib/heapq.js";
import { bisect_left } from "./stdlib/bisect.js";

const B = (s) => pyBytesOf([...s].map((c) => c.charCodeAt(0)));

/** Assert f() throws exactly `TypeError: message` (CPython golden). */
function raisesTE(f, message) {
    let threw = null;
    try {
        f();
    } catch (e) {
        threw = e;
    }
    assert.notEqual(threw, null, `expected TypeError(${message}), got a value`);
    assert.equal(`${threw.name}: ${threw.message}`, `TypeError: ${message}`);
}

// ── the cross-type matrix ────────────────────────────────────────────────
// Value factories (fresh per row) with their CPython type names.
const KINDS = [
    ["int", () => 1],
    ["float", () => __pyF(2.0)],       // boxed integer-valued float
    ["float", () => 2.5],              // native non-integer float
    ["bool", () => true],
    ["str", () => "a"],
    ["NoneType", () => null],
    ["list", () => [1]],
    ["tuple", () => pyTuple(1, 2)],
    ["set", () => new Set([1])],
    ["dict", () => ({})],
    ["bytes", () => B("ab")],
];
const NUMERIC = new Set(["int", "float", "bool"]);
const SEQ_ARR = new Set(["list", "tuple"]);

const OPS = [
    ["<", pyLt],
    ["<=", pyLe],
    [">", pyGt],
    [">=", pyGe],
];

test("cross-type comparison matrix: TypeError exactly where CPython raises", () => {
    for (const [opName, opFn] of OPS) {
        for (const [ta, fa] of KINDS) {
            for (const [tb, fb] of KINDS) {
                const a = fa();
                const b = fb();
                const compatible =
                    (NUMERIC.has(ta) && NUMERIC.has(tb))
                    || (ta === "str" && tb === "str")
                    || (ta === "bytes" && tb === "bytes")
                    || (ta === "set" && tb === "set")
                    // F1-r2: SAME sequence kind only — list-vs-tuple raises.
                    || (SEQ_ARR.has(ta) && ta === tb);
                if (compatible) {
                    // Must NOT throw; result is a boolean.
                    assert.equal(typeof opFn(a, b), "boolean",
                        `${ta} ${opName} ${tb} should compare`);
                } else {
                    raisesTE(() => opFn(a, b),
                        `'${opName}' not supported between instances of '${ta}' and '${tb}'`);
                }
            }
        }
    }
});

// ── F1-r2: list-vs-tuple witnesses (the review's exact arms) ─────────────
test("list-vs-tuple ordering raises like CPython (all four ops + derived)", () => {
    raisesTE(() => pyLt([1], pyTuple(2)),
        "'<' not supported between instances of 'list' and 'tuple'");
    raisesTE(() => pyLe(pyTuple(1), [1]),
        "'<=' not supported between instances of 'tuple' and 'list'");
    raisesTE(() => pyGt([1, 2], pyTuple(1, 2)),
        "'>' not supported between instances of 'list' and 'tuple'");
    raisesTE(() => pyGe(pyTuple(), []),
        "'>=' not supported between instances of 'tuple' and 'list'");
    // element recursion: ([1],) < ((1,),) raises through __seqLt → pyLt
    raisesTE(() => pyLt(pyTuple([1]), pyTuple(pyTuple(1))),
        "'<' not supported between instances of 'list' and 'tuple'");
    // derived surfaces reach the same guard
    raisesTE(() => pySorted([[1], pyTuple(2)]),
        "'<' not supported between instances of 'tuple' and 'list'");
    raisesTE(() => pyMin([1], pyTuple(2)),
        "'<' not supported between instances of 'tuple' and 'list'");
    // same-kind pairs still order lexicographically
    assert.equal(pyLt([1], [2]), true);
    assert.equal(pyLe(pyTuple(1), pyTuple(1)), true);
    assert.equal(pyGt(pyTuple(2, 1), pyTuple(2)), true);
});

// ── same-type ordering and numeric mixing still hold ─────────────────────
test("numeric mixing and same-type ordering preserved", () => {
    assert.equal(pyLt(1, 2.0), true);          // 1 < 2.0
    assert.equal(pyLt(1, __pyF(2.0)), true);   // 1 < boxed 2.0
    assert.equal(pyLt(__pyF(8.0), 9), true);   // boxed 8.0 < 9
    assert.equal(pyLt(1, true), false);        // 1 < True
    assert.equal(pyLe(true, 1), true);         // True <= 1
    assert.equal(pyLt(false, true), true);     // False < True
    assert.equal(pyGt(2n ** 60n, 5), true);    // BigInt int vs Number int
    assert.equal(pyLt("a", "b"), true);
    assert.equal(pyGe("b", "a"), true);
    assert.equal(pyLt([1, 2], [1, 3]), true);  // lexicographic
    assert.equal(pyLt(pyTuple(-6, "s"), pyTuple(-2, "x")), true);
    assert.equal(pyLe(new Set([1]), new Set([1, 2])), true); // subset
});

// ── bytes ordering (new typed arm; was silent-wrong via toString) ────────
test("bytes/bytearray order by byte value like CPython", () => {
    assert.equal(pyLt(Uint8Array.from([2]), Uint8Array.from([16])), true); // b'\x02' < b'\x10'
    assert.equal(pyLt(B("ab"), B("b")), true);
    assert.equal(pyLe(B("ab"), B("ab")), true);
    assert.equal(pyGt(B("b"), B("ab")), true);
    assert.equal(pyGe(B("ab"), B("ab")), true);
    assert.equal(pyLt(B("ab"), B("abc")), true); // prefix is smaller
});

// ── nested-sequence recursion hits the guard ─────────────────────────────
test("(1, 'a') < (1, 2) raises through element recursion", () => {
    raisesTE(() => pyLt(pyTuple(1, "a"), pyTuple(1, 2)),
        "'<' not supported between instances of 'str' and 'int'");
    raisesTE(() => pyLt([1, "a"], [1, 2]),
        "'<' not supported between instances of 'str' and 'int'");
    // Equal prefixes that never reach the mixed element stay fine.
    assert.equal(pyLt([0, "a"], [1, 2]), true);
});

// ── derived arms already routing through pyLt: heapq, bisect ─────────────
test("heapq / bisect comparators raise on cross-type", () => {
    const h = [];
    heappush(h, 1);
    raisesTE(() => heappush(h, "a"),
        "'<' not supported between instances of 'str' and 'int'");
    raisesTE(() => bisect_left([1, 2, 3], "a"),
        "'<' not supported between instances of 'int' and 'str'");
});

// ── derived arms through the runtime.js consolidation (F1, v0.2.4) ───────

test("min/max raise on cross-type (2-arg and iterable forms)", () => {
    raisesTE(() => pyMin(3, "a"),
        "'<' not supported between instances of 'str' and 'int'");
    raisesTE(() => pyMax("a", 3),
        "'>' not supported between instances of 'int' and 'str'");
    raisesTE(() => pyMin([3, "a"]),
        "'<' not supported between instances of 'str' and 'int'");
    raisesTE(() => pyMax([3, "a"]),
        "'>' not supported between instances of 'str' and 'int'");
    assert.equal(pyMin(3, 2.5), 2.5);
    assert.equal(pyMax("a", "b"), "b");
    // #214-class: tuple keys now compare lexicographically in min/max too.
    assert.deepEqual(pyMin([pyTuple(2, "x"), pyTuple(-6, "s")]), pyTuple(-6, "s"));
});

test("sorted / list.sort raise on cross-type elements", () => {
    raisesTE(() => pySorted([1, "a"]),
        "'<' not supported between instances of 'str' and 'int'");
    const xs = [1, "a"];
    raisesTE(() => pyListSort(xs),
        "'<' not supported between instances of 'str' and 'int'");
    assert.deepEqual(pySorted([3, 1, 2]), [1, 2, 3]);
    assert.deepEqual(pySorted(["b", "a"]), ["a", "b"]);
    // CPython: 1 == True, stable sort keeps input order on the tie.
    assert.deepEqual(pySorted([2.5, 1, true]), [1, true, 2.5]); // numeric mix OK
});

// ── r2 should-fix: reflected-method precedence for a right-hand subclass ──
test("right-hand strict subclass reflected method dispatches first (CPython)", () => {
    class A {
        __lt__(o) { return "A.lt"; }
        __gt__(o) { return "A.gt"; }
        __le__(o) { return "A.le"; }
        __ge__(o) { return "A.ge"; }
    }
    class Bc extends A {
        __lt__(o) { return "B.lt"; }
        __gt__(o) { return "B.gt"; }
        __le__(o) { return "B.le"; }
        __ge__(o) { return "B.ge"; }
    }
    // CPython goldens: A() < B() → 'B.gt' (reflected-first), B() < A() → 'B.lt'
    assert.equal(pyLt(new A(), new Bc()), "B.gt");
    assert.equal(pyLt(new Bc(), new A()), "B.lt");
    assert.equal(pyLe(new A(), new Bc()), "B.ge");
    assert.equal(pyGt(new A(), new Bc()), "B.lt");
    assert.equal(pyGe(new A(), new Bc()), "B.le");
    // subclass that does NOT override keeps the normal left-first order
    class C extends A {}
    assert.equal(pyLt(new A(), new C()), "A.lt");
    // unrelated classes: left-first order unchanged
    class D { __gt__(o) { return "D.gt"; } }
    assert.equal(pyLt(new A(), new D()), "A.lt");
});
