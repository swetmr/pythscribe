// F3 (v0.2.4) — int()/float() RECEIVER-TYPE matrix (the E7 conformance net).
//
// pyInt fell through to `Math.trunc(Number(x))` and pyFloat to
// `__pyF(Number(x))` for arbitrary receivers, inheriting JS coercion:
// int(None) → (old) TypeError-by-luck but float(None) → 0.0, int((2,)) → 2,
// int([1]) → 1, int({}) → NaN, float([1]) → 1.0 — silent wrong values where
// CPython raises TypeError. Class fix: receiver-type guard (TypeError with
// CPython's message) + bytes-like acceptance (int(b'12'), float(b'1.5')).
//
// F3-r2 (v0.2.4): the r1 valueOf allowance (__numProtoOk) was a JS-coercion
// heuristic, not Python's numeric protocol — any bare-valueOf object slid
// through, __int__ results were unvalidated, __index__ was never tried,
// int(7, 2) ran the numeric branch before base validation, and base was
// taken via Number(). Now: pyInt dispatches __int__ (validated) then
// __index__ (PyNumber_Long order); pyFloat dispatches __float__ (validated)
// then __index__ (PyNumber_Float); explicit base requires a str/bytes
// receiver and converts base via __index__; Decimal/Fraction implement
// exact __int__/__float__ (BigInt-backed — no 2^53 truncation loss).
//
// Goldens generated from live CPython 3.12.7 (2026-08-26).

import test from "node:test";
import assert from "node:assert/strict";

import {
    pyInt, pyFloat, pyBytesOf, pyBytearrayOf, pyTuple, __pyF,
} from "./operators.js";
import { Decimal } from "./stdlib/decimal.js";
import { Fraction } from "./stdlib/fractions.js";

const B = (s) => pyBytesOf([...s].map((c) => c.charCodeAt(0)));
const BA = (s) => pyBytearrayOf([...s].map((c) => c.charCodeAt(0)));

function raises(f, name, message) {
    let threw = null;
    try {
        f();
    } catch (e) {
        threw = e;
    }
    assert.notEqual(threw, null, `expected ${name}(${message}), got a value`);
    assert.equal(`${threw.name}: ${threw.message}`, `${name}: ${message}`);
}

// ── invalid receivers: TypeError with CPython's message ──────────────────
const INT_MSG = (t) =>
    `int() argument must be a string, a bytes-like object or a real number, not '${t}'`;
const FLOAT_MSG = (t) =>
    `float() argument must be a string or a real number, not '${t}'`;

test("int() rejects non-(str|bytes|number) receivers like CPython", () => {
    const rows = [
        ["NoneType", null],
        ["NoneType", undefined],
        ["tuple", pyTuple(2)],
        ["list", [1]],
        ["dict", {}],
        ["set", new Set([1])],
    ];
    for (const [t, v] of rows) raises(() => pyInt(v), "TypeError", INT_MSG(t));
});

test("float() rejects non-(str|bytes|number) receivers like CPython", () => {
    const rows = [
        ["NoneType", null],
        ["NoneType", undefined],
        ["tuple", pyTuple(1)],
        ["list", [1]],
        ["dict", {}],
        ["set", new Set([1])],
    ];
    for (const [t, v] of rows) raises(() => pyFloat(v), "TypeError", FLOAT_MSG(t));
});

// ── bytes-like receivers are accepted (new arm) ──────────────────────────
test("int()/float() accept bytes and bytearray like CPython", () => {
    assert.equal(pyInt(B("12")), 12);
    assert.equal(pyInt(BA("12")), 12);
    assert.equal(pyInt(B("  12  ")), 12);
    assert.equal(pyInt(B("ff"), 16), 255);
    assert.equal(pyFloat(B("1.5")).valueOf(), 1.5);
    assert.equal(pyFloat(B(" 1.5 ")).valueOf(), 1.5);
    assert.equal(pyFloat(BA("2.5")).valueOf(), 2.5);
    // Invalid bytes literal: ValueError with the b'…' repr, not the str one.
    raises(() => pyInt(B("xyz")), "ValueError",
        "invalid literal for int() with base 10: b'xyz'");
    raises(() => pyFloat(B("xyz")), "ValueError",
        "could not convert string to float: b'xyz'");
});

// ── preserved behavior: valid receivers unchanged ────────────────────────
test("int()/float() valid receivers preserved", () => {
    assert.equal(pyInt("123"), 123);
    assert.equal(pyInt("1_000"), 1000);
    assert.equal(pyInt(3.5), 3);
    assert.equal(pyInt(true), 1);
    assert.equal(pyInt(__pyF(8.0)), 8);      // boxed float
    assert.equal(pyInt(2n ** 60n), 2n ** 60n);
    raises(() => pyInt("xyz"), "ValueError",
        "invalid literal for int() with base 10: 'xyz'");
    assert.equal(pyFloat("inf").valueOf(), Infinity);
    assert.equal(pyFloat(3).valueOf(), 3);
    assert.equal(pyFloat(true).valueOf(), 1);
    assert.equal(pyFloat("1.5"), 1.5);
});

// ── numeric objects convert through the real protocol (__int__/__float__) ─
test("Decimal/Fraction still convert through int()/float()", () => {
    assert.equal(pyInt(new Decimal("3.7")), 3);
    assert.equal(pyFloat(new Decimal("1.5")).valueOf(), 1.5);
    assert.equal(pyInt(new Fraction(7, 2)), 3);
    assert.equal(pyFloat(new Fraction(1, 4)).valueOf(), 0.25);
    // F3-r2: negative truncation toward zero + EXACT past 2^53 (the old
    // Math.trunc(Number(x)) route lost low digits). CPython goldens:
    // int(Decimal(2)**200) with default 28-digit context, int(Fraction(-7,2)).
    assert.equal(pyInt(new Decimal("-3.7")), -3);
    assert.equal(pyInt(new Fraction(-7, 2)), -3);
    const d = new Decimal(2n ** 100n);   // exact: no context rounding at 31 digits
    assert.equal(pyInt(d), 2n ** 100n);
    assert.equal(pyInt(new Decimal("1e30")), 10n ** 30n);
});

// ── F3-r2 matrix: the Python numeric protocol, per arm ───────────────────
test("pyInt dispatches __int__ (validated) then __index__ like CPython", () => {
    // __int__ returning a non-int → CPython's exact TypeError
    raises(() => pyInt({ __int__: () => 1.5 }), "TypeError",
        "__int__ returned non-int (type float)");
    raises(() => pyInt({ __int__: () => "7" }), "TypeError",
        "__int__ returned non-int (type str)");
    raises(() => pyInt({ __int__: () => __pyF(2.0) }), "TypeError",
        "__int__ returned non-int (type float)");
    // valid __int__ results: int, bool (bool ⊂ int), huge int
    assert.equal(pyInt({ __int__: () => 7 }), 7);
    assert.equal(pyInt({ __int__: () => true }), 1);
    assert.equal(pyInt({ __int__: () => 2n ** 60n }), 2n ** 60n);
    // __int__ WINS over __index__ (PyNumber_Long order — CPython golden 7)
    assert.equal(pyInt({ __int__: () => 7, __index__: () => 3 }), 7);
    // __index__-only receiver converts (CPython golden 5)
    assert.equal(pyInt({ __index__: () => 5 }), 5);
    assert.equal(pyInt({ __index__: () => true }), 1); // 3.12: bool accepted
    raises(() => pyInt({ __index__: () => "x" }), "TypeError",
        "__index__ returned non-int (type str)");
});

test("pyFloat dispatches __float__ (validated) then __index__ like CPython", () => {
    assert.equal(pyFloat({ __float__: () => 2.5 }).valueOf(), 2.5);
    assert.equal(pyFloat({ __float__: () => __pyF(3.0) }).valueOf(), 3);
    // __float__ returning a non-float → CPython's exact TypeError (the
    // receiver's type name prefixes the message)
    raises(() => pyFloat({ __float__: () => 3 }), "TypeError",
        "dict.__float__ returned non-float (type int)");
    raises(() => pyFloat({ __float__: () => true }), "TypeError",
        "dict.__float__ returned non-float (type bool)");
    // __index__-only receiver converts to the float value (CPython 5.0)
    assert.equal(pyFloat({ __index__: () => 5 }).valueOf(), 5);
});

test("bare-valueOf objects are rejected (JS-coercion heuristic closed)", () => {
    class V { valueOf() { return 42; } }
    raises(() => pyInt(new V()), "TypeError", INT_MSG("V"));
    raises(() => pyFloat(new V()), "TypeError", FLOAT_MSG("V"));
});

test("int() explicit base: receiver + base validated like CPython", () => {
    // non-string receiver with explicit base — the old code silently
    // returned the numeric receiver (int(7, 2) → 7); CPython raises
    raises(() => pyInt(7, 2), "TypeError",
        "int() can't convert non-string with explicit base");
    raises(() => pyInt(true, 10), "TypeError",
        "int() can't convert non-string with explicit base");
    raises(() => pyInt(7.5, 3), "TypeError",
        "int() can't convert non-string with explicit base");
    // base validated FIRST, via __index__ (CPython order: P1 before P2).
    // A Python float base compiles to a BOXED value (__pyF) — a native
    // integer-valued 2.0 IS int 2 in the value model, so the boxed form is
    // the faithful `int(7, 2.0)` witness.
    raises(() => pyInt(7, __pyF(2.0)), "TypeError",
        "'float' object cannot be interpreted as an integer");
    raises(() => pyInt(7, 2.5), "TypeError",
        "'float' object cannot be interpreted as an integer");
    raises(() => pyInt("101", __pyF(2.0)), "TypeError",
        "'float' object cannot be interpreted as an integer");
    raises(() => pyInt("101", "2"), "TypeError",
        "'str' object cannot be interpreted as an integer");
    raises(() => pyInt("10", null), "TypeError",
        "'NoneType' object cannot be interpreted as an integer");
    raises(() => pyInt("101", 99), "ValueError",
        "int() base must be >= 2 and <= 36, or 0");
    raises(() => pyInt("101", 1), "ValueError",
        "int() base must be >= 2 and <= 36, or 0");
    // base through __index__ (CPython golden 5); kwargs form preserved
    assert.equal(pyInt("101", { __index__: () => 2 }), 5);
    assert.equal(pyInt("101", { base: 2 }), 5);
    assert.equal(pyInt("ff", 16), 255);
    assert.equal(pyInt(B("101"), 2), 5);
});

// ── r2 should-fix: bytes receivers strip only ASCII whitespace ───────────
test("int()/float() bytes receivers reject non-ASCII whitespace (CPython)", () => {
    const NB = (bytes) => pyBytesOf(bytes);
    // b'\xa012' — U+00A0 is JS-trim()-able but NOT Python bytes whitespace
    raises(() => pyInt(NB([0xa0, 49, 50])), "ValueError",
        "invalid literal for int() with base 10: b'\\xa012'");
    raises(() => pyFloat(NB([0xa0, 49, 46, 53])), "ValueError",
        "could not convert string to float: b'\\xa01.5'");
    // ASCII whitespace still accepted on bytes; str receivers keep the
    // full-unicode trim ('\xa0'.isspace() is True in Python)
    assert.equal(pyInt(NB([32, 9, 49, 50, 10])), 12);
    assert.equal(pyInt(" 42"), 42);
    assert.equal(pyFloat(" 1.5"), 1.5);
});
