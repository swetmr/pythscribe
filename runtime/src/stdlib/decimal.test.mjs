// Unit tests for the `decimal` stdlib module. Behavior parity against
// real CPython is covered by the differential corpus
// (tests/differential/cpython_corpus.json, dec_* entries) — these tests
// instead exercise the JS API directly: construction paths, dunder
// dispatch, rounding-mode plumbing, and the documented out-of-scope
// exclusions (never a silently-wrong result).
//
// Run with: node --test runtime/src/stdlib/decimal.test.mjs

import { test } from "node:test";
import assert from "node:assert/strict";

import {
    Decimal,
    ROUND_HALF_EVEN, ROUND_HALF_UP, ROUND_DOWN, ROUND_UP, ROUND_FLOOR, ROUND_CEILING,
} from "./decimal.js";

test("construct from string preserves exponent/trailing zeros", () => {
    assert.equal(String(new Decimal("0.30")), "0.30");
    assert.equal(String(new Decimal("1E+2")), "1E+2");
    assert.equal(String(new Decimal("100")), "100");
    assert.equal(String(new Decimal("0E-7")), "0E-7");
});

test("construct from int", () => {
    assert.equal(String(new Decimal(42)), "42");
    assert.equal(String(new Decimal(-7)), "-7");
    assert.equal(String(new Decimal(9007199254740993n)), "9007199254740993");
});

test("construct from float is the EXACT binary expansion (matches CPython)", () => {
    assert.equal(
        String(new Decimal(0.1)),
        "0.1000000000000000055511151231257827021181583404541015625",
    );
    assert.equal(String(new Decimal(100.0)), "100");
    assert.equal(String(new Decimal(-0.5)), "-0.5");
});

test("+ - * / route through dunders and stay exact", () => {
    const a = new Decimal("0.1");
    const b = new Decimal("0.2");
    assert.equal(String(a.__add__(b)), "0.3");
    assert.equal(String(new Decimal("1.30").__add__(new Decimal("1.20"))), "2.50");
    assert.equal(String(new Decimal("5").__sub__(new Decimal("3.2"))), "1.8");
    assert.equal(String(new Decimal("2.5").__mul__(new Decimal("4"))), "10.0");
    assert.equal(String(new Decimal(1).__truediv__(new Decimal(3))), "0.3333333333333333333333333333");
    assert.equal(String(new Decimal(1).__truediv__(new Decimal(4))), "0.25"); // exact, no trailing pad
});

test("arithmetic against a plain int operand", () => {
    assert.equal(String(new Decimal("1.5").__add__(3)), "4.5");
    assert.equal(String(new Decimal("1.5").__radd__(3)), "4.5");
});

test("arithmetic against float/str raises TypeError (CPython does not auto-coerce)", () => {
    assert.throws(() => new Decimal("1").__add__(1.5), TypeError);
    assert.throws(() => new Decimal("1").__add__("1"), TypeError);
});

test("division by zero raises ZeroDivisionError", () => {
    assert.throws(() => new Decimal("1").__truediv__(new Decimal(0)), /ZeroDivisionError|division by zero/);
});

test("comparisons: numeric equality across differing exponents", () => {
    assert.equal(new Decimal("0.3").__eq__(new Decimal("0.30")), true);
    assert.equal(new Decimal("5").__eq__(5), true);
    assert.equal(new Decimal("1.1").__lt__(new Decimal("1.2")), true);
    assert.equal(new Decimal("3").__ge__(new Decimal("3")), true);
    assert.equal(new Decimal("2").__lt__(3), true);
});

test("comparison against float: == is False (no throw), ordering throws (matches CPython)", () => {
    // 1.0 is indistinguishable from the int 1 as a JS Number (PythScribe
    // has no separate runtime float tag for whole values), so only a
    // genuinely fractional float exercises the "unsupported operand"
    // path here.
    assert.equal(new Decimal("1").__eq__(1.5), false);
    assert.throws(() => new Decimal("1").__lt__(1.5), TypeError);
});

test("abs / neg", () => {
    assert.equal(String(new Decimal("-5.5").__abs__()), "5.5");
    assert.equal(String(new Decimal("-5.5").__neg__()), "5.5");
    assert.equal(String(new Decimal("5.5").__neg__()), "-5.5");
    // CPython special case: negating zero does not flip the sign under
    // the default (non-FLOOR) rounding mode.
    assert.equal(String(new Decimal("0").__neg__()), "0");
    assert.equal(String(new Decimal("0.00").__neg__()), "0.00");
});

test("quantize rounds to the target exponent under each rounding mode", () => {
    assert.equal(String(new Decimal("2.5").quantize(new Decimal("1"))), "2"); // HALF_EVEN default
    assert.equal(String(new Decimal("1.5").quantize(new Decimal("1"))), "2");
    assert.equal(String(new Decimal("2.5").quantize(new Decimal("1"), { rounding: ROUND_HALF_UP })), "3");
    assert.equal(String(new Decimal("2.7").quantize(new Decimal("1"), { rounding: ROUND_DOWN })), "2");
    assert.equal(String(new Decimal("2.1").quantize(new Decimal("1"), { rounding: ROUND_UP })), "3");
    assert.equal(String(new Decimal("-2.1").quantize(new Decimal("1"), { rounding: ROUND_FLOOR })), "-3");
    assert.equal(String(new Decimal("2.1").quantize(new Decimal("1"), { rounding: ROUND_CEILING })), "3");
    assert.equal(String(new Decimal("3.14159").quantize(new Decimal("0.01"))), "3.14");
    // rounding= defaults to HALF_EVEN when omitted
    assert.equal(String(new Decimal("2.5").quantize(new Decimal("1"), {})), "2");
});

test("str/repr formatting edges (the CPython fidelity hot-spot)", () => {
    assert.equal(String(new Decimal("1E+2")), "1E+2");
    assert.equal(String(new Decimal("100")), "100");
    assert.equal(String(new Decimal("0.30")), "0.30");
    assert.equal(String(new Decimal("0E-7")), "0E-7");
    assert.equal(new Decimal("0.3").__repr__(), "Decimal('0.3')");
    assert.equal(String(new Decimal("123E+10")), "1.23E+12");
});

test("float() conversion via valueOf()", () => {
    assert.equal(Number(new Decimal("0.25")), 0.25);
    assert.equal(Number(new Decimal("100")), 100);
});

test("documented out-of-scope exclusion: non-finite float raises a clear error, never a silent wrong value", () => {
    assert.throws(() => new Decimal(Infinity), /non-finite/);
    assert.throws(() => new Decimal(NaN), /non-finite/);
});

test("documented out-of-scope exclusion: quantize exceeding context precision raises rather than truncating silently", () => {
    // Quantizing to 30 fractional places would need a 31-digit
    // coefficient — past the fixed 28-digit default context.
    assert.throws(() => new Decimal("1").quantize(new Decimal("1E-30")));
});
