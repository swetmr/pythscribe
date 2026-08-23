// E2-lite operand-type guard (#466 class) — the binary-op OPERAND-TYPE
// matrix. Verifies that every incompatible operand pair raises CPython's
// exact exception KIND + MESSAGE through the ONE authority
// (__binOpTypeError in operators.js), and that every valid combination
// still succeeds (regression guard for the already-closed instances:
// #322 None+int, #25 list-concat, #215 bool-as-int, string/list/bytes
// replication, set ops, the numeric hybrid).
//
// Goldens generated from live CPython 3.12.7 (2026-08-22). Each error row
// was verified NON-VACUOUS against base 7e19ec9b: pre-fix, pyAdd's raw
// `a + b` fall-through returned silent garbage (b"abc" + 1 → "97,98,991",
// [1] + 1 → "11", set+set → "[object Set][object Set]") and pyMul's count
// branches admitted floats (b"ab" * 2.5 → RangeError crash, "ab" * 2.5 →
// silent "abab", [1] * 2.5 → 3-element list, "a" * "b" → SyntaxError from
// BigInt("a"), "ab" * -1 → RangeError, bytes*bool → SyntaxError).

import test from "node:test";
import assert from "node:assert/strict";

import {
    pyAdd, pySub, pyMul, pyDiv, pyFloorDiv, pyPow,
    pyBitAnd, pyBitNot,
    pyBytesOf, pyBytearrayOf, pyRepr, pyTuple, __pyF,
    __binOpTypeError, __mulRepCount,
} from "./operators.js";
import { pySeq } from "./runtime.js";

const B = (s) => pyBytesOf([...s].map((c) => c.charCodeAt(0)));
const BA = (s) => pyBytearrayOf([...s].map((c) => c.charCodeAt(0)));

/** Assert f() throws exactly `name: message` (CPython golden). */
function raises(f, name, message) {
    let threw = null;
    try {
        f();
    } catch (e) {
        threw = e;
    }
    assert.notEqual(threw, null, "expected a throw, got a value");
    assert.equal(`${threw.name}: ${threw.message}`, `${name}: ${message}`);
}

// ── The authority itself always throws ───────────────────────────────────
test("authority: __binOpTypeError always throws, per-op message shapes", () => {
    raises(() => __binOpTypeError("+", B("a"), 1), "TypeError", "can't concat int to bytes");
    raises(() => __binOpTypeError("+", "a", 1), "TypeError", 'can only concatenate str (not "int") to str');
    raises(() => __binOpTypeError("+", [1], 1), "TypeError", 'can only concatenate list (not "int") to list');
    raises(() => __binOpTypeError("*", "a", null), "TypeError", "can't multiply sequence by non-int of type 'NoneType'");
    raises(() => __binOpTypeError("-", "a", 1), "TypeError", "unsupported operand type(s) for -: 'str' and 'int'");
});

test("authority: __mulRepCount admits int/bool only (bool ⊂ int)", () => {
    assert.equal(__mulRepCount(3), 3);
    assert.equal(__mulRepCount(-1), -1);
    assert.equal(__mulRepCount(true), 1);
    assert.equal(__mulRepCount(false), 0);
    assert.equal(__mulRepCount(2n), 2);
    assert.equal(__mulRepCount(2.5), null);       // float
    assert.equal(__mulRepCount(__pyF(2)), null);  // boxed 2.0 is a float
    assert.equal(__mulRepCount(null), null);
    assert.equal(__mulRepCount("2"), null);
});

// ── `+` incompatible pairs (#466 is row 1) ───────────────────────────────
test("+ : bytes/bytearray ⊕ non-bytes → can't concat X to bytes", () => {
    raises(() => pyAdd(B("abc"), 1), "TypeError", "can't concat int to bytes"); // #466
    raises(() => pyAdd(B("ab"), "x"), "TypeError", "can't concat str to bytes");
    raises(() => pyAdd(B("ab"), [1]), "TypeError", "can't concat list to bytes");
    raises(() => pyAdd(B("ab"), 2.5), "TypeError", "can't concat float to bytes");
    raises(() => pyAdd(B("ab"), null), "TypeError", "can't concat NoneType to bytes");
    raises(() => pyAdd(BA("a"), 1), "TypeError", "can't concat int to bytearray");
    raises(() => pyAdd(BA("a"), "x"), "TypeError", "can't concat str to bytearray");
});

test("+ : str ⊕ non-str → can only concatenate str (not \"X\") to str", () => {
    raises(() => pyAdd("a", 1), "TypeError", 'can only concatenate str (not "int") to str');
    raises(() => pyAdd("a", B("x")), "TypeError", 'can only concatenate str (not "bytes") to str');
    raises(() => pyAdd("a", null), "TypeError", 'can only concatenate str (not "NoneType") to str');
    raises(() => pyAdd("a", [1]), "TypeError", 'can only concatenate str (not "list") to str');
    raises(() => pyAdd("a", 1.5), "TypeError", 'can only concatenate str (not "float") to str');
});

test("+ : list/tuple ⊕ non-sequence → can only concatenate list (not \"X\") to list", () => {
    raises(() => pyAdd([1], 1), "TypeError", 'can only concatenate list (not "int") to list');
    raises(() => pyAdd([1], "a"), "TypeError", 'can only concatenate list (not "str") to list');
    raises(() => pyAdd(pyTuple(1), 1), "TypeError", 'can only concatenate tuple (not "int") to tuple');
    // mixed list/tuple keeps its pre-existing CPython message (crit-13)
    raises(() => pyAdd([1], pyTuple(1)), "TypeError", 'can only concatenate list (not "tuple") to list');
    raises(() => pyAdd(pyTuple(1), [1]), "TypeError", 'can only concatenate tuple (not "list") to tuple');
});

test("+ : left operand not a sequence → unsupported operand type(s)", () => {
    raises(() => pyAdd(1, "a"), "TypeError", "unsupported operand type(s) for +: 'int' and 'str'");
    raises(() => pyAdd(1, [1]), "TypeError", "unsupported operand type(s) for +: 'int' and 'list'");
    raises(() => pyAdd(1, B("a")), "TypeError", "unsupported operand type(s) for +: 'int' and 'bytes'");
    raises(() => pyAdd(null, "a"), "TypeError", "unsupported operand type(s) for +: 'NoneType' and 'str'");
    raises(() => pyAdd(null, 1), "TypeError", "unsupported operand type(s) for +: 'NoneType' and 'int'"); // #322 kept
    raises(() => pyAdd(new Set([1]), new Set([2])), "TypeError", "unsupported operand type(s) for +: 'set' and 'set'");
    raises(() => pyAdd(new Map(), new Map()), "TypeError", "unsupported operand type(s) for +: 'dict' and 'dict'");
    raises(() => pyAdd(true, "x"), "TypeError", "unsupported operand type(s) for +: 'bool' and 'str'");
});

// ── `*` incompatible pairs ───────────────────────────────────────────────
test("* : sequence × non-int → can't multiply sequence by non-int of type 'X'", () => {
    raises(() => pyMul(B("ab"), 2.5), "TypeError", "can't multiply sequence by non-int of type 'float'");
    raises(() => pyMul(B("ab"), __pyF(2)), "TypeError", "can't multiply sequence by non-int of type 'float'"); // boxed 2.0
    raises(() => pyMul(B("ab"), B("c")), "TypeError", "can't multiply sequence by non-int of type 'bytes'");
    raises(() => pyMul(B("ab"), null), "TypeError", "can't multiply sequence by non-int of type 'NoneType'");
    raises(() => pyMul("ab", 2.5), "TypeError", "can't multiply sequence by non-int of type 'float'");
    raises(() => pyMul("ab", "c"), "TypeError", "can't multiply sequence by non-int of type 'str'");
    raises(() => pyMul("ab", null), "TypeError", "can't multiply sequence by non-int of type 'NoneType'");
    raises(() => pyMul([1], 2.5), "TypeError", "can't multiply sequence by non-int of type 'float'");
    raises(() => pyMul([1], [2]), "TypeError", "can't multiply sequence by non-int of type 'list'");
    // CPython names the NON-sequence side when the sequence is on the right
    raises(() => pyMul(2.5, "ab"), "TypeError", "can't multiply sequence by non-int of type 'float'");
    raises(() => pyMul(null, "ab"), "TypeError", "can't multiply sequence by non-int of type 'NoneType'");
    raises(() => pyMul(1.5, B("a")), "TypeError", "can't multiply sequence by non-int of type 'float'");
    raises(() => pyMul(2.5, [1]), "TypeError", "can't multiply sequence by non-int of type 'float'");
});

// ── `*` index-sized replication count (#471) ─────────────────────────────
test("* : sequence × out-of-index-range int → OverflowError, not RangeError", () => {
    const msg = "cannot fit 'int' into an index-sized integer";
    raises(() => pyMul([1], 10n ** 30n), "OverflowError", msg);
    raises(() => pyMul("ab", 10n ** 30n), "OverflowError", msg);
    raises(() => pyMul(B("ab"), 10n ** 30n), "OverflowError", msg);
    raises(() => pyMul(10n ** 30n, [1]), "OverflowError", msg); // right-hand sequence
    raises(() => pyMul([1], -(10n ** 30n)), "OverflowError", msg); // negative overflow
    raises(() => pyMul([1], 2n ** 63n), "OverflowError", msg); // first out-of-range value
    // __mulRepCount itself is the ONE count rule — probe it directly
    raises(() => __mulRepCount(10n ** 30n), "OverflowError", msg);
    raises(() => __mulRepCount(2 ** 63), "OverflowError", msg); // Number-typed interop count
    assert.equal(typeof __mulRepCount(9223372036854775807n), "number"); // max ssize_t still a valid count (no throw)
    // in-range counts unchanged (the loop/clamp behavior is the caller's)
    assert.equal(__mulRepCount(3), 3);
    assert.equal(__mulRepCount(-5n), -5);
});

test("* : no sequence involved → unsupported operand type(s)", () => {
    raises(() => pyMul(new Set([1]), 2), "TypeError", "unsupported operand type(s) for *: 'set' and 'int'");
    raises(() => pyMul(new Map(), 2), "TypeError", "unsupported operand type(s) for *: 'dict' and 'int'");
    raises(() => pyMul(null, null), "TypeError", "unsupported operand type(s) for *: 'NoneType' and 'NoneType'");
});

// ── numeric-only ops route through the same authority (bytes named right) ─
test("-, /, //, ** : non-numeric operands → unsupported, 'bytes' not 'PyBytes'", () => {
    raises(() => pySub(B("a"), 1), "TypeError", "unsupported operand type(s) for -: 'bytes' and 'int'");
    raises(() => pySub("a", 1), "TypeError", "unsupported operand type(s) for -: 'str' and 'int'");
    raises(() => pyDiv(B("a"), 2), "TypeError", "unsupported operand type(s) for /: 'bytes' and 'int'");
    raises(() => pyFloorDiv(B("a"), 2), "TypeError", "unsupported operand type(s) for //: 'bytes' and 'int'");
    raises(() => pyPow(B("a"), 2), "TypeError", "unsupported operand type(s) for ** or pow(): 'bytes' and 'int'");
    raises(() => pyPow("a", 2), "TypeError", "unsupported operand type(s) for ** or pow(): 'str' and 'int'");
});

// ── REGRESSION GUARD: every valid combination still succeeds ─────────────
test("regression: valid + operations unchanged", () => {
    assert.equal(pyAdd("a", "b"), "ab");
    assert.equal(pyRepr(pyAdd([1], [2])), "[1, 2]");
    assert.equal(pyRepr(pyAdd(pyTuple(1), pyTuple(2))), "(1, 2)");
    assert.equal(pyRepr(pyAdd(B("a"), B("b"))), "b'ab'");
    assert.equal(pyRepr(pyAdd(BA("a"), B("b"))), "bytearray(b'ab')"); // left-type rule (item 5)
    assert.equal(pyRepr(pyAdd(B("a"), BA("b"))), "b'ab'");
    assert.equal(pyAdd(1, 2), 3);
    assert.equal(pyAdd(1, 2.5), 3.5);
    assert.equal(pyAdd(true, 1), 2);                       // #215 bool-as-int
    assert.equal(pyRepr(pyAdd(9007199254740992, 1)), "9007199254740993"); // hybrid intact (#460)
});

test("regression: valid * replications unchanged (+ new bool/negative counts)", () => {
    assert.equal(pyMul("ab", 3), "ababab");
    assert.equal(pyMul(2, "ab"), "abab");
    assert.equal(pyMul("ab", true), "ab");
    assert.equal(pyMul("ab", false), "");
    assert.equal(pyMul(true, "ab"), "ab");
    assert.equal(pyMul("ab", -1), "");                     // was a RangeError crash
    assert.equal(pyRepr(pyMul(B("ab"), 2)), "b'abab'");
    assert.equal(pyRepr(pyMul(B("ab"), true)), "b'ab'");   // was a SyntaxError crash
    assert.equal(pyRepr(pyMul(B("ab"), -1)), "b''");
    assert.equal(pyRepr(pyMul(BA("a"), 2)), "bytearray(b'aa')");
    assert.equal(pyRepr(pyMul([0], 3)), "[0, 0, 0]");
    assert.equal(pyRepr(pyMul([1], true)), "[1]");
    assert.equal(pyRepr(pyMul([1, 2], -3)), "[]");
    assert.equal(pyRepr(pyMul(pyTuple(1, 2), 2)), "(1, 2, 1, 2)"); // tuple-ness kept
    assert.equal(pyMul(true, 2), 2);
    assert.equal(pyRepr(pyMul(2.5, 4)), "10.0");
    assert.equal(pyRepr(pyMul(3, 4)), "12");
});

// ── #469 CROSS-AUTHORITY GUARD — ONE type-name source (__pyTypeName) ─────
// Three type-name computers used to DISAGREE: __pyTypeName (runtime.js,
// complete), __opTypeName (operators.js — no function/plain-object arms →
// leaked JS 'Function'/'Object'), and the inline __bitTypeName (emit.rs —
// no __pyfloat__ arm → leaked 'PyFloat'). All operand/bit sites now route
// through __pyTypeName, so the SAME value renders the SAME CPython name in
// the operand path (+), the bit path (&, ~), and the iteration path (for).
// Goldens from live CPython 3.12.7 (2026-08-22). The function/class/dict rows
// FAIL on base 321a3568 in BOTH copies (the divergence existed there). The
// boxed-float ('PyFloat') leak was INLINE-only on base — the package
// __opTypeName already had the __pyfloat__ arm — so that row's package path
// passed on base; it is caught by the inline parity battery, not this file.

test("#469: plain-object dict names 'dict' across + / & / for (was 'Object')", () => {
    raises(() => pyAdd({ a: 1 }, 1), "TypeError",
        "unsupported operand type(s) for +: 'dict' and 'int'");
    raises(() => pyBitAnd({ a: 1 }, 1), "TypeError",
        "unsupported operand type(s) for &: 'dict' and 'int'");
    // CPython: {'a':1} & {'b':2} → TypeError too (& is set/int only)
    raises(() => pyBitAnd({ a: 1 }, { b: 2 }), "TypeError",
        "unsupported operand type(s) for &: 'dict' and 'dict'");
    // iteration path: a dict ITERATES (keys) — same authority, no over-raise
    assert.deepEqual(pySeq({ a: 1, b: 2 }), ["a", "b"]);
});

test("#469: function names 'function' across + / & / ~ / for (was 'Function')", () => {
    function f() {}
    raises(() => pyAdd(f, 1), "TypeError",
        "unsupported operand type(s) for +: 'function' and 'int'");
    raises(() => pyBitAnd(f, 1), "TypeError",
        "unsupported operand type(s) for &: 'function' and 'int'");
    raises(() => pyBitNot(f), "TypeError", "bad operand type for unary ~: 'function'");
    raises(() => pySeq(f), "TypeError", "'function' object is not iterable");
    // the sequence-facing message shapes name 'function' too
    raises(() => pyMul("a", f), "TypeError",
        "can't multiply sequence by non-int of type 'function'");
    raises(() => pyAdd("x", f), "TypeError",
        'can only concatenate str (not "function") to str');
});

test("#469: class/type object names 'type' across + / & / for (was 'Function')", () => {
    class Widget {}
    raises(() => pyAdd(Widget, 1), "TypeError",
        "unsupported operand type(s) for +: 'type' and 'int'");
    raises(() => pyBitAnd(Widget, 1), "TypeError",
        "unsupported operand type(s) for &: 'type' and 'int'");
    raises(() => pySeq(Widget), "TypeError", "'type' object is not iterable");
    raises(() => pyAdd([1], Widget), "TypeError",
        'can only concatenate list (not "type") to list');
});

test("#469: boxed 2.0 names 'float' across + / & / ~ / for (bit path was 'PyFloat')", () => {
    const boxed = __pyF(2); // boxed integer-valued float (Option B)
    raises(() => pyAdd(boxed, []), "TypeError",
        "unsupported operand type(s) for +: 'float' and 'list'");
    raises(() => pyBitAnd(boxed, 1), "TypeError",
        "unsupported operand type(s) for &: 'float' and 'int'");
    raises(() => pyBitNot(boxed), "TypeError", "bad operand type for unary ~: 'float'");
    raises(() => pySeq(boxed), "TypeError", "'float' object is not iterable");
});

test("#469 regression: every already-correct name stays correct in + and &", () => {
    // [name, value] — the currently-correct sweep; each must render the same
    // CPython name in BOTH the operand and the bit path.
    const forms = [
        ["int", 7],
        ["bool", true],
        ["str", "ab"],
        ["list", [1]],
        ["tuple", pyTuple(1)],
        ["set", new Set([1])],
        ["bytes", pyBytesOf([97])],
        ["NoneType", null],
        ["float", 2.5], // unboxed non-integer float
    ];
    for (const [tn, v] of forms) {
        // pair each with a dict partner (plain objects support neither + nor
        // & with any of these forms). Dict on the LEFT for `+` so no form
        // hits a concat-specific message shape; form on the LEFT for `&`.
        raises(() => pyAdd({ z: 1 }, v), "TypeError",
            `unsupported operand type(s) for +: 'dict' and '${tn}'`);
        raises(() => pyBitAnd(v, { z: 1 }), "TypeError",
            `unsupported operand type(s) for &: '${tn}' and 'dict'`);
    }
    // class INSTANCES keep their class name (constructor-name path)
    class Widget {}
    Widget.__name__ = "Widget"; // compiled classes carry __name__
    raises(() => pyAdd(new Widget(), 1), "TypeError",
        "unsupported operand type(s) for +: 'Widget' and 'int'");
    raises(() => pyBitAnd(new Widget(), 1), "TypeError",
        "unsupported operand type(s) for &: 'Widget' and 'int'");
});

test("regression: set ops and dunder dispatch untouched", () => {
    assert.equal(pyRepr(pySub(new Set([1, 2]), new Set([1]))), "{2}");
    const withMul = { __mul__(o) { return `mul:${o}`; } };
    assert.equal(pyMul(withMul, "x"), "mul:x");            // user dunder wins over seq check
    const withRadd = { __radd__(o) { return `radd:${o}`; } };
    assert.equal(pyAdd("s", withRadd), "radd:s");          // reflected dunder wins over gate
});
