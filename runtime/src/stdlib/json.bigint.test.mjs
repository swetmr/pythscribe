// P3 boundary: json.dumps must serialize arbitrary-precision ints (BigInt)
// as unquoted integer literals (CPython behavior), without disturbing
// legitimate all-digit STRING values.
import { test } from "node:test";
import assert from "node:assert/strict";
import { dumps } from "./json.js";

// NB: expected strings use CPython json.dumps DEFAULT separators (", " / ": ",
// i.e. with spaces) — the runtime is CPython-faithful here; a compact-separator
// expectation was the stale (JS JSON.stringify) assumption.
test("BigInt serializes as an unquoted integer literal", () => {
    assert.equal(dumps({ big: 9007199254740993n }), '{"big": 9007199254740993}');
    assert.equal(dumps([1, 2, 9007199254740993n]), "[1, 2, 9007199254740993]");
    assert.equal(dumps(-9007199254740993n), "-9007199254740993");
});

test("a legitimate all-digit string keeps its quotes", () => {
    assert.equal(dumps({ s: "123", n: 9007199254740993n }), '{"s": "123", "n": 9007199254740993}');
});

test("mixed small Number / float / BigInt", () => {
    assert.equal(dumps({ a: 5, b: 1.5, c: 10000000000000000n }), '{"a": 5, "b": 1.5, "c": 10000000000000000}');
});

// Option B (#451 minimal): a BOXED (integer-valued) float keeps its '.0'
// like CPython json.dumps; a native small Number is an int and prints bare
// digits; a non-integer float is native and already prints correctly.
// (Salvaged from the Option A float-fidelity rows, re-based onto the
// hybrid int model: int stays a native Number, only 8.0-style floats box.)
test("boxed integer-valued floats keep '.0' (values and keys)", async () => {
    const { pyFloat, __pyF } = await import("../operators.js");
    assert.equal(dumps({ a: pyFloat(2), b: 3.14, c: 5 }), '{"a": 2.0, "b": 3.14, "c": 5}');
    assert.equal(dumps([pyFloat(1), 2.5]), "[1.0, 2.5]");
    assert.equal(dumps(__pyF(-0)), "-0.0");
    assert.equal(dumps(__pyF(1e16)), "1e+16");
    // Map-backed dict with a boxed-float KEY serializes via float repr.
    const m = new Map([[__pyF(8), "x"]]);
    assert.equal(dumps(m), '{"8.0": "x"}');
    // A tagged repr must not strip quotes from a legitimate string that
    // LOOKS like a float.
    assert.equal(dumps({ s: "8.0", v: __pyF(8) }), '{"s": "8.0", "v": 8.0}');
});

test("sort_keys recurses (and still handles BigInt)", () => {
    assert.equal(
        dumps({ b: 1, a: { d: 9007199254740993n, c: 2 } }, { sort_keys: true }),
        '{"a": {"c": 2, "d": 9007199254740993}, "b": 1}',
    );
});
