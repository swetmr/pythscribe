// F4 (v0.2.4) — unary -/+/abs() OPERAND-TYPE matrix (the E7 conformance net).
//
// The wave-15 operand-type guard was binary-only; unary minus/plus and
// abs() never entered its dispatch, so `-'a'` → NaN, `-[1]` → -1,
// `+'a'` → NaN, `abs(None)` → 0 leaked silent-wrong JS coercions where
// CPython raises TypeError. Class fix: __unaryTypeGuard after dunder
// dispatch in pyNeg/pyPos/pyAbs.
//
// Goldens generated from live CPython 3.12.7 (2026-08-26).
//
// NOTE (codegen half, owned by the emit.rs workstream): the emitter's
// Primitive fast path still writes bare `-x` for statically-str operands
// (`-'a'` as a literal); operands of Unknown type route through pyNeg/
// pyPos and are covered here.

import test from "node:test";
import assert from "node:assert/strict";

import {
    pyNeg, pyPos, pyAbs, pyBytesOf, pyTuple, __pyF,
} from "./operators.js";
import { Decimal } from "./stdlib/decimal.js";

const B = (s) => pyBytesOf([...s].map((c) => c.charCodeAt(0)));

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

// Non-numeric operand rows with their CPython type names.
const BAD = [
    ["str", () => "a"],
    ["NoneType", () => null],
    ["NoneType", () => undefined],
    ["list", () => [1]],
    ["tuple", () => pyTuple(1, 2)],
    ["dict", () => ({})],
    ["set", () => new Set([1])],
    ["bytes", () => B("ab")],
];

test("unary - raises TypeError on non-numeric operands", () => {
    for (const [t, f] of BAD) {
        raisesTE(() => pyNeg(f()), `bad operand type for unary -: '${t}'`);
    }
});

test("unary + raises TypeError on non-numeric operands", () => {
    for (const [t, f] of BAD) {
        raisesTE(() => pyPos(f()), `bad operand type for unary +: '${t}'`);
    }
});

test("abs() raises TypeError on non-numeric operands", () => {
    for (const [t, f] of BAD) {
        raisesTE(() => pyAbs(f()), `bad operand type for abs(): '${t}'`);
    }
});

test("numeric operands preserved (int/float/bool/BigInt/boxed/Decimal)", () => {
    assert.equal(pyNeg(3), -3);
    assert.equal(pyNeg(2.5), -2.5);
    assert.equal(pyNeg(true), -1);                    // -True == -1
    assert.equal(pyNeg(2n ** 60n), -(2n ** 60n));
    assert.equal(pyNeg(__pyF(8.0)).valueOf(), -8);    // boxed float via __neg__
    assert.equal(pyNeg(__pyF(8.0)).__pyfloat__, true); // stays a float box
    assert.equal(pyPos(3), 3);
    assert.equal(pyPos(true), 1);                     // +True == 1
    assert.equal(pyPos(2n ** 60n), 2n ** 60n);
    assert.equal(pyPos(__pyF(8.0)).__pyfloat__, true);
    assert.equal(pyAbs(-3), 3);
    assert.equal(pyAbs(-2.5), 2.5);
    assert.equal(pyAbs(true), 1);                     // abs(True) == 1
    assert.equal(pyAbs(-(2n ** 60n)), 2n ** 60n);
    assert.equal(pyAbs(__pyF(-8.0)).valueOf(), 8);    // boxed float via __abs__
    // Decimal keeps its own type through __neg__/__abs__ dunders.
    assert.equal(String(pyNeg(new Decimal("5.5"))), "-5.5");
    assert.equal(String(pyAbs(new Decimal("-5.5"))), "5.5");
});
