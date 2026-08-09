// P3 boundary: json.dumps must serialize arbitrary-precision ints (BigInt)
// as unquoted integer literals (CPython behavior), without disturbing
// legitimate all-digit STRING values.
import { test } from "node:test";
import assert from "node:assert/strict";
import { dumps } from "./json.js";

test("BigInt serializes as an unquoted integer literal", () => {
    assert.equal(dumps({ big: 9007199254740993n }), '{"big":9007199254740993}');
    assert.equal(dumps([1, 2, 9007199254740993n]), "[1,2,9007199254740993]");
    assert.equal(dumps(-9007199254740993n), "-9007199254740993");
});

test("a legitimate all-digit string keeps its quotes", () => {
    assert.equal(dumps({ s: "123", n: 9007199254740993n }), '{"s":"123","n":9007199254740993}');
});

test("mixed small Number / float / BigInt", () => {
    assert.equal(dumps({ a: 5, b: 1.5, c: 10000000000000000n }), '{"a":5,"b":1.5,"c":10000000000000000}');
});

test("sort_keys recurses (and still handles BigInt)", () => {
    assert.equal(
        dumps({ b: 1, a: { d: 9007199254740993n, c: 2 } }, { sort_keys: true }),
        '{"a":{"c":2,"d":9007199254740993},"b":1}',
    );
});
