// #467 guard — CPython type-NAME fidelity in runtime error messages.
//
// CPython names the value's PYTHON type in diagnostics ("'float' object is
// not iterable"); several runtime sites used JS `typeof`, so values whose JS
// typeof differs from their Python type name ('number', 'bigint', 'boolean',
// 'string', 'object') rendered the wrong name. Every such message now routes
// through the ONE value-model type-name source (__pyTypeName, backed by
// pyType), so the name is right by construction.
//
// Matrix: "not iterable" / "not subscriptable" / "not callable" across the
// value forms of the runtime value model — float (non-integer AND boxed
// integer-valued), int (small Number + BigInt), None, bool, str, dict, set,
// bytes, bytearray. Every expected message below was captured byte-for-byte
// from CPython 3.12.7 (python -c on the corresponding expression).
//
// Run with: node --test runtime/src/errmsg_typename.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import {
    pySeq, pyIter, pyForIter, pyGetItem, __pyCall, pyLen, pyContains,
    pyGenSend, pyGenClose, pyGenThrow, pyDelSlice, pyDict, pyRange,
    pyAppend, pyExtend, pyBoundMethod,
} from "./runtime.js";
import { pyBytesOf, pyBytearrayOf, __pyF, pyMod, pyFloorDiv, pyDivmod, pyTuple } from "./operators.js";
import { Fraction } from "./stdlib/fractions.js";
import { timedelta, date, datetime } from "./stdlib/datetime.js";

// Assert `fn` throws an error whose Python exception name is `name` and whose
// message is byte-identical to CPython's.
function throwsPy(fn, name, msg) {
    assert.throws(fn, (e) => {
        assert.equal(e.name, name, `expected ${name}, got ${e.name}: ${e.message}`);
        assert.equal(e.message, msg, "message mismatch");
        return true;
    });
}

// The value forms, keyed by the CPython type name each must render.
const forms = () => ([
    ["float", 8.5],                       // unboxed non-integer float — the #467 repro value
    ["float", __pyF(8.0)],                // boxed integer-valued float (Option B)
    ["int", 7],                           // small int (JS Number)
    ["int", 9007199254740993n],           // big int (JS BigInt)
    ["NoneType", null],
    ["bool", true],
    ["str", "ab"],
    ["dict", { a: 1 }],
    ["set", new Set([1])],
    ["bytes", pyBytesOf("ab", "utf-8")],
    ["bytearray", pyBytearrayOf(pyBytesOf("ab", "utf-8"))],
]);

const RAISES_ITER = new Set(["float", "int", "NoneType", "bool"]);
const RAISES_GETITEM = new Set(["float", "int", "NoneType", "bool", "set"]);

// ── "'T' object is not iterable" — comprehension (pySeq), iter() (pyIter),
//    and the for-loop lowering (pyForIter) ─────────────────────────────────
test("not-iterable matrix names the Python type (pySeq / pyIter / pyForIter)", () => {
    for (const [tn, v] of forms()) {
        if (RAISES_ITER.has(tn)) {
            const msg = `'${tn}' object is not iterable`;
            throwsPy(() => pySeq(v), "TypeError", msg);
            throwsPy(() => pyIter(v), "TypeError", msg);
            // pyForIter may defer to for..of — consume to force the raise.
            throwsPy(() => { for (const _ of pyForIter(v)) void _; }, "TypeError", msg);
        } else {
            // CPython iterates these fine — the guard must not over-raise.
            assert.doesNotThrow(() => pySeq(v));
            assert.doesNotThrow(() => { for (const _ of pyForIter(v)) void _; });
        }
    }
});

// The literal #467 reproducer: [x for x in 8.5] lowers through the seq path.
test("#467: [x for x in 8.5] says 'float', not 'number'", () => {
    throwsPy(() => pySeq(8.5), "TypeError", "'float' object is not iterable");
});

// ── "'T' object is not subscriptable" — pyGetItem ─────────────────────────
test("not-subscriptable matrix names the Python type (pyGetItem)", () => {
    for (const [tn, v] of forms()) {
        if (RAISES_GETITEM.has(tn)) {
            throwsPy(() => pyGetItem(v, 0), "TypeError",
                `'${tn}' object is not subscriptable`);
        } else if (tn !== "dict") { // dict[0] is a KeyError (kind, not name — out of scope)
            assert.doesNotThrow(() => pyGetItem(v, 0)); // str/bytes/bytearray index fine
        }
    }
});

// ── "'T' object is not callable" — __pyCall (every form raises) ───────────
test("not-callable matrix names the Python type (__pyCall)", () => {
    for (const [tn, v] of forms()) {
        throwsPy(() => __pyCall(v, []), "TypeError", `'${tn}' object is not callable`);
    }
});

// ── The other routed sites, one discriminating row each ───────────────────

test("len(8.5) names 'float' (CPython-exact)", () => {
    throwsPy(() => pyLen(8.5), "TypeError", "object of type 'float' has no len()");
});

test("generator-protocol AttributeErrors name the Python type", () => {
    // CPython: (8.5).send(1) → AttributeError: 'float' object has no attribute 'send'
    throwsPy(() => pyGenSend(8.5, 1), "AttributeError",
        "'float' object has no attribute 'send'");
    throwsPy(() => pyGenClose(8.5), "AttributeError",
        "'float' object has no attribute 'close'");
    throwsPy(() => pyGenThrow(8.5, new Error("x")), "AttributeError",
        "'float' object has no attribute 'throw'");
    // null receiver reads as NoneType, not JS "object".
    throwsPy(() => pyGenSend(null, 1), "AttributeError",
        "'NoneType' object has no attribute 'send'");
});

test("del s[0:1] on a str names 'str' (CPython-exact)", () => {
    throwsPy(() => pyDelSlice("abc", 0, 1, null), "TypeError",
        "'str' object does not support item deletion");
});

test("dict(8.5) names 'float' (CPython-exact)", () => {
    throwsPy(() => pyDict(8.5), "TypeError", "'float' object is not iterable");
});

test("membership over a non-container names the Python type (pyContains)", () => {
    // CPython: 1 in 8.5 → TypeError: argument of type 'float' is not iterable
    throwsPy(() => pyContains(8.5, 1), "TypeError",
        "argument of type 'float' is not iterable");
    throwsPy(() => pyContains(7, 1), "TypeError",
        "argument of type 'int' is not iterable");
    throwsPy(() => pyContains(true, 1), "TypeError",
        "argument of type 'bool' is not iterable");
});

test("range() over a class instance names the class (CPython-exact)", () => {
    class Widget {}
    Widget.__name__ = "Widget"; // compiled classes carry __name__
    // CPython: range(Widget()) → TypeError: 'Widget' object cannot be
    // interpreted as an integer (was: the blanket JS-object fallback 'dict').
    throwsPy(() => pyRange(new Widget()), "TypeError",
        "'Widget' object cannot be interpreted as an integer");
});

test("receiver-dispatch fallbacks name the Python type (pyAppend/pyExtend)", () => {
    // Runtime-shaped message (not CPython's AttributeError) — only the type
    // NAME is under test here; the message shape is pre-existing.
    throwsPy(() => pyAppend(8.5, 1), "TypeError",
        "object of type 'float' has no append()");
    throwsPy(() => pyExtend(8.5, []), "TypeError",
        "object of type 'float' has no extend()");
});

test("stdlib operand messages name the Python type (Fraction, timedelta)", () => {
    // CPython: Fraction(1, 2) + 'x' → unsupported operand type(s) for +:
    // 'Fraction' and 'str' (was: ... and 'string').
    throwsPy(() => new Fraction(1n, 2n).__add__("x"), "TypeError",
        "unsupported operand type(s) for +: 'Fraction' and 'str'");
    // CPython: timedelta(1) < 'x' → '<' not supported between instances of
    // 'datetime.timedelta' and 'str' (was: ... and 'string').
    throwsPy(() => new timedelta(1).__lt__("x"), "TypeError",
        "'<' not supported between instances of 'datetime.timedelta' and 'str'");
});

test("#470: date/datetime __lt__ names the operand's Python type (CPython-exact)", () => {
    // Same class as #467's timedelta fix: date.__lt__/datetime.__lt__ built
    // their TypeError with a hard-coded literal "and other" instead of the
    // operand's type name. Every expected message below was captured
    // byte-for-byte from CPython 3.12.7 (e.g. datetime.date(2020, 1, 1) < 5
    // → TypeError: '<' not supported between instances of 'datetime.date'
    // and 'int' — CPython uses the QUALIFIED lhs name and the BARE rhs name).
    const d = new date(2020, 1, 1);
    throwsPy(() => d.__lt__(5), "TypeError",
        "'<' not supported between instances of 'datetime.date' and 'int'");
    throwsPy(() => d.__lt__("x"), "TypeError",
        "'<' not supported between instances of 'datetime.date' and 'str'");
    throwsPy(() => d.__lt__(null), "TypeError",
        "'<' not supported between instances of 'datetime.date' and 'NoneType'");
    const dt = new datetime(2020, 1, 1);
    throwsPy(() => dt.__lt__(5), "TypeError",
        "'<' not supported between instances of 'datetime.datetime' and 'int'");
    throwsPy(() => dt.__lt__(2.5), "TypeError",
        "'<' not supported between instances of 'datetime.datetime' and 'float'");
    throwsPy(() => dt.__lt__(null), "TypeError",
        "'<' not supported between instances of 'datetime.datetime' and 'NoneType'");
    // Comparable operands still compare (the guard must not over-raise).
    assert.equal(d.__lt__(new date(2021, 1, 1)), true);
    assert.equal(dt.__lt__(new datetime(2019, 1, 1)), false);
});

// ── error-kind batch (#471/#472/#473 + differential-net finds): primitive
//    attribute misses raise AttributeError; `%` by zero says "modulo" ─────
test("missing attribute on a primitive raises CPython's AttributeError", () => {
    throwsPy(() => pyBoundMethod(5, "foo"), "AttributeError", "'int' object has no attribute 'foo'");
    throwsPy(() => pyBoundMethod(9007199254740993n, "foo"), "AttributeError", "'int' object has no attribute 'foo'");
    throwsPy(() => pyBoundMethod(2.5, "foo"), "AttributeError", "'float' object has no attribute 'foo'");
    throwsPy(() => pyBoundMethod(__pyF(8.0), "foo"), "AttributeError", "'float' object has no attribute 'foo'");
    throwsPy(() => pyBoundMethod(true, "foo"), "AttributeError", "'bool' object has no attribute 'foo'");
    throwsPy(() => pyBoundMethod("ab", "foo"), "AttributeError", "'str' object has no attribute 'foo'");
    throwsPy(() => pyBoundMethod(null, "foo"), "AttributeError", "'NoneType' object has no attribute 'foo'");
    // Real access must not over-raise: numeric protocol, JS-backed members,
    // dict convenience methods, and OBJECT receivers (undefined pass-through
    // preserved for interop/props reads).
    assert.equal(pyBoundMethod(5, "real"), 5);
    assert.equal(typeof pyBoundMethod(5, "conjugate"), "function");
    assert.equal(pyBoundMethod("ab", "length"), 2); // JS-backed member stays readable
    assert.equal(typeof pyBoundMethod({ a: 1 }, "get"), "function");
    assert.equal(pyBoundMethod({ a: 1 }, "missing"), undefined); // object receivers keep pass-through
    // Review round 2: int/bool Rational protocol is CPython-VALID — the
    // guard must not over-raise on it. Float has NO numerator/denominator.
    assert.equal(pyBoundMethod(5, "numerator"), 5);
    assert.equal(pyBoundMethod(5, "denominator"), 1);
    assert.equal(pyBoundMethod(-7, "numerator"), -7);
    assert.equal(pyBoundMethod(9007199254740993n, "numerator"), 9007199254740993n);
    assert.equal(pyBoundMethod(9007199254740993n, "denominator"), 1);
    assert.equal(pyBoundMethod(true, "numerator"), 1);
    assert.equal(pyBoundMethod(false, "numerator"), 0);
    assert.equal(pyBoundMethod(true, "denominator"), 1);
    throwsPy(() => pyBoundMethod(2.5, "numerator"), "AttributeError", "'float' object has no attribute 'numerator'");
    throwsPy(() => pyBoundMethod(__pyF(8.0), "numerator"), "AttributeError", "'float' object has no attribute 'numerator'");
    throwsPy(() => pyBoundMethod(2.5, "denominator"), "AttributeError", "'float' object has no attribute 'denominator'");
});

test("% by zero says 'integer modulo by zero' (CPython 3.12), // unchanged", () => {
    throwsPy(() => pyMod(1, 0), "ZeroDivisionError", "integer modulo by zero");
    throwsPy(() => pyMod(9007199254740993n, 0), "ZeroDivisionError", "integer modulo by zero");
    throwsPy(() => pyMod(2.5, 0), "ZeroDivisionError", "float modulo by zero");
    throwsPy(() => pyFloorDiv(1, 0), "ZeroDivisionError", "integer division or modulo by zero");
    throwsPy(() => pyDivmod(7, 0), "ZeroDivisionError", "integer division or modulo by zero");
    assert.equal(pyMod(7, 3), 1);
    assert.equal(pyMod(-7, 3), 2);
});

// ── round 3 (corpus deep-close): native-container absent attributes raise;
//    container method REFERENCES resolve; interop boundaries preserved ────
test("absent attribute on a native container raises CPython's AttributeError", () => {
    throwsPy(() => pyBoundMethod([1], "foo"), "AttributeError", "'list' object has no attribute 'foo'");
    throwsPy(() => pyBoundMethod([1], "numerator"), "AttributeError", "'list' object has no attribute 'numerator'");
    throwsPy(() => pyBoundMethod(new Map([["a", 1]]), "foo"), "AttributeError", "'dict' object has no attribute 'foo'");
    throwsPy(() => pyBoundMethod(new Set([1]), "foo"), "AttributeError", "'set' object has no attribute 'foo'");
    const tup = pyTuple(1, 2);
    throwsPy(() => pyBoundMethod(tup, "foo"), "AttributeError", "'tuple' object has no attribute 'foo'");
    // tuple has ONLY count/index — the list-method bindings must not leak
    throwsPy(() => pyBoundMethod(tup, "append"), "AttributeError", "'tuple' object has no attribute 'append'");
    // plain-object dict raises ONLY under the codegen strict flag (an
    // unproven plain object may be a JS-interop/props object)
    throwsPy(() => pyBoundMethod({ a: 1 }, "foo", 1), "AttributeError", "'dict' object has no attribute 'foo'");
    assert.equal(pyBoundMethod({ a: 1 }, "foo"), undefined); // lenient without the flag
});

test("container method references resolve through the runtime helpers", () => {
    const xs = [1];
    const app = pyBoundMethod(xs, "append");
    app(2);
    assert.deepEqual(xs, [1, 2]);
    assert.equal(pyBoundMethod([10, 20], "index")(20), 1);
    assert.equal(pyBoundMethod(pyTuple(1, 2, 1), "count")(1), 2);
    const d = { a: 1 };
    pyBoundMethod(d, "setdefault")("b", 9);
    assert.equal(d.b, 9);
    assert.equal(typeof pyBoundMethod(d, "update"), "function");
    const s = new Set([1, 2]);
    pyBoundMethod(s, "discard")(1);
    assert.deepEqual([...s], [2]);
    assert.equal(pyBoundMethod(s, "issubset")(new Set([2, 3])), true);
    assert.equal([...pyBoundMethod(s, "union")(new Set([7]))].sort().join(","), "2,7");
    // expando/JS-backed members still pass the `in` check (interop reads)
    const arr = [1];
    arr.custom = "x";
    assert.equal(pyBoundMethod(arr, "custom"), "x");
});
