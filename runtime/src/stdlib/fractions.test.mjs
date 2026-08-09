// Unit tests for the `fractions` stdlib module. Behavior parity against
// real CPython is covered by the differential corpus
// (tests/differential/cpython_corpus.json, frac_* entries) — these tests
// exercise the JS API directly: construction paths, normalization,
// dunder dispatch, and float-mixing fallback.
//
// Run with: node --test runtime/src/stdlib/fractions.test.mjs

import { test } from "node:test";
import assert from "node:assert/strict";

import { Fraction } from "./fractions.js";

test("construct from two ints, normalized (gcd-reduced, sign on numerator)", () => {
    const f = new Fraction(2, 4);
    assert.equal(f.numerator, 1n);
    assert.equal(f.denominator, 2n);
    const g = new Fraction(1, -2);
    assert.equal(g.numerator, -1n);
    assert.equal(g.denominator, 2n);
});

test("construct from string: fraction form, decimal form, whole int", () => {
    assert.equal(String(new Fraction("3/10")), "3/10");
    assert.equal(String(new Fraction("0.5")), "1/2");
    assert.equal(String(new Fraction("7")), "7");
});

test("construct from float is the exact binary expansion (matches CPython)", () => {
    const half = new Fraction(0.5);
    assert.equal(half.numerator, 1n);
    assert.equal(half.denominator, 2n);
    const tenth = new Fraction(0.1);
    assert.equal(tenth.numerator, 3602879701896397n);
    assert.equal(tenth.denominator, 36028797018963968n);
});

test("zero denominator raises ZeroDivisionError", () => {
    assert.throws(() => new Fraction(1, 0), /ZeroDivisionError|zero/i);
});

test("+ - * / route through dunders", () => {
    const a = new Fraction(1, 10);
    const b = new Fraction(2, 10);
    assert.equal(String(a.__add__(b)), "3/10");
    assert.equal(String(new Fraction(3, 4).__sub__(new Fraction(1, 4))), "1/2");
    assert.equal(String(new Fraction(2, 3).__mul__(new Fraction(3, 4))), "1/2");
    assert.equal(String(new Fraction(1, 2).__truediv__(new Fraction(1, 4))), "2");
});

test("arithmetic against a plain int operand stays a Fraction", () => {
    assert.equal(String(new Fraction(1, 2).__add__(1)), "3/2");
    assert.equal(String(new Fraction(1, 2).__radd__(1)), "3/2");
});

test("arithmetic against a float falls back to plain float (matches CPython)", () => {
    const r = new Fraction(1, 4).__add__(0.5);
    assert.equal(typeof r, "number");
    assert.equal(r, 0.75);
});

test("** only supports an integer exponent (documented subset)", () => {
    assert.equal(String(new Fraction(2, 3).__pow__(2)), "4/9");
    assert.equal(String(new Fraction(2, 3).__pow__(-1)), "3/2");
    assert.throws(() => new Fraction(2, 3).__pow__(0.5), TypeError);
});

test("neg / abs", () => {
    assert.equal(String(new Fraction(1, 2).__neg__()), "-1/2");
    assert.equal(String(new Fraction(-3, 4).__abs__()), "3/4");
});

test("comparisons, including against int", () => {
    assert.equal(new Fraction(1, 2).__eq__(new Fraction(2, 4)), true);
    assert.equal(new Fraction(4, 1).__eq__(4), true);
    assert.equal(new Fraction(1, 3).__lt__(new Fraction(1, 2)), true);
    assert.equal(new Fraction(5, 2).__gt__(2), true);
});

test("str/repr", () => {
    assert.equal(String(new Fraction(3, 10)), "3/10");
    assert.equal(String(new Fraction(7, 1)), "7");
    assert.equal(new Fraction(3, 10).__repr__(), "Fraction(3, 10)");
});

test("float() conversion via valueOf()", () => {
    assert.equal(Number(new Fraction(1, 4)), 0.25);
});
