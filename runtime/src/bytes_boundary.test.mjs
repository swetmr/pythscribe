// Bytes dispatch authority — boundary test (#455/#456/#457/#458 root fix).
//
// One suite exercises a bytes/bytearray value across the WHOLE runtime
// surface, so a future bytes op that bypasses the authority (__pyBytesKind
// + the PyBytes prototype method surface) fails here rather than shipping
// as the next "missing arm" instance:
//   * truthiness (pyBool) — empty + nonempty, both kinds        (#457)
//   * type()/__name__ + isinstance identity                     (#456)
//   * slice READ (kind-preserving) + slice ASSIGN + slice DELETE (#455)
//   * element-write validation (immutable bytes / byte range)
//   * bound-method extraction (pyBoundMethod) + direct dispatch
//     through the method-table Multi helpers                    (#458)
// Every expected value/message is transcribed from live CPython 3.12.
//
// Run with: node --test runtime/src/bytes_boundary.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import { pyBytes, pyBytesOf, pyBytearrayOf, pyRepr } from "./operators.js";
import {
    pyBool, __pyBytesKind, __pyBytesName, pyType, pySlice, pySetSlice,
    pyDelSlice, pySetItem, pyBoundMethod, pyCount, pyFind, pyIndex,
    pyStrStartswith, pyStrEndswith, pyStrRfind, pyStrRindex,
    __pyTypeBytes, __pyTypeBytearray,
} from "./index.js";
import { __pyIsInstance } from "./classes.js";

const B = (s) => pyBytesOf(s, "ascii");
const BA = (s) => pyBytearrayOf(s, "ascii");

const throwsPy = (fn, name, message) => {
    assert.throws(fn, (e) => {
        assert.equal(e.name, name);
        if (message !== undefined) assert.equal(e.message, message);
        return true;
    });
};

test("authority: __pyBytesKind classifies bytes / bytearray / other", () => {
    assert.equal(__pyBytesKind(B("ab")), "bytes");
    assert.equal(__pyBytesKind(BA("ab")), "bytearray");
    assert.equal(__pyBytesKind(new Uint8Array([1])), "bytes"); // raw interop
    assert.equal(__pyBytesKind("ab"), null);
    assert.equal(__pyBytesKind([1, 2]), null);
    assert.equal(__pyBytesKind(null), null);
    assert.equal(__pyBytesName(B("")), "bytes");
    assert.equal(__pyBytesName(BA("")), "bytearray");
});

test("#457 truthiness: bool(b'') is False, nonempty True, both kinds", () => {
    assert.equal(pyBool(B("")), false);
    assert.equal(pyBool(B("x")), true);
    assert.equal(pyBool(BA("")), false);
    assert.equal(pyBool(BA("y")), true);
    assert.equal(pyBool(new Uint8Array(0)), false); // raw interop routes too
});

test("#456 type(): interned singletons with CPython __name__ and __mro__", () => {
    assert.equal(pyType(B("ab")), __pyTypeBytes);
    assert.equal(pyType(BA("ab")), __pyTypeBytearray);
    assert.equal(pyType(B("ab")).__name__, "bytes");
    assert.equal(pyType(BA("ab")).__name__, "bytearray");
    assert.equal(String(__pyTypeBytes.__repr__()), "<class 'bytes'>");
    // bytes.__mro__ ends in object; bytearray is NOT a bytes subclass.
    assert.equal(__pyTypeBytes.__mro__.at(-1).__name__, "object");
    assert.equal(__pyTypeBytearray.__mro__.length, 2);
    // The singletons are callable constructors (first-class `bytes`).
    assert.equal(pyRepr(__pyTypeBytes([97, 98])), "b'ab'");
    assert.equal(pyRepr(__pyTypeBytearray([97])), "bytearray(b'a')");
});

test("isinstance sentinels route through the authority", () => {
    assert.equal(__pyIsInstance(B("a"), "bytes"), true);
    assert.equal(__pyIsInstance(BA("a"), "bytearray"), true);
    assert.equal(__pyIsInstance(BA("a"), "bytes"), false); // CPython: not a subclass
    assert.equal(__pyIsInstance(B("a"), "bytearray"), false);
    assert.equal(__pyIsInstance("a", "bytes"), false);
    assert.equal(__pyIsInstance(B("a"), __pyTypeBytes), true); // type-object form
});

test("#455 slice READ preserves the kind (bytes -> bytes, bytearray -> bytearray)", () => {
    assert.equal(pyRepr(pySlice(B("banana"), 1, 4, null)), "b'ana'");
    assert.equal(pyRepr(pySlice(B("banana"), null, null, -1)), "b'ananab'");
    assert.equal(pyRepr(pySlice(BA("banana"), 2, null, null)), "bytearray(b'nana')");
});

test("#455 slice ASSIGN: replace / grow / shrink / insert / self-source", () => {
    const x = BA("hello");
    pySetSlice(x, 1, 3, null, B("XY"));
    assert.equal(pyRepr(x), "bytearray(b'hXYlo')");
    pySetSlice(x, 1, 3, null, B("LONGER"));
    assert.equal(pyRepr(x), "bytearray(b'hLONGERlo')");
    assert.equal(x.length, 9);
    pySetSlice(x, 2, 4, null, []);
    assert.equal(pyRepr(x), "bytearray(b'hLGERlo')");
    const y = BA("abc");
    pySetSlice(y, 2, 0, null, B("QQ")); // inverted range inserts at 2
    assert.equal(pyRepr(y), "bytearray(b'abQQc')");
    const z = BA("abc");
    pySetSlice(z, 1, 3, null, z); // self-source snapshots first
    assert.equal(pyRepr(z), "bytearray(b'aabc')");
    const w = BA("abc");
    pySetSlice(w, 0, 2, null, [66, true]); // iterable of ints, bool <= int
    assert.equal(pyRepr(w), "bytearray(b'B\\x01c')");
});

test("#455 slice ASSIGN: extended slices, exact-length rule", () => {
    const x = BA("abcdef");
    pySetSlice(x, null, null, 2, B("XYZ"));
    assert.equal(pyRepr(x), "bytearray(b'XbYdZf')");
    const y = BA("abcdef");
    pySetSlice(y, null, null, -2, B("XYZ"));
    assert.equal(pyRepr(y), "bytearray(b'aZcYeX')");
    throwsPy(() => pySetSlice(BA("abcdef"), null, null, 2, B("XY")),
        "ValueError", "attempt to assign bytes of size 2 to extended slice of size 3");
    throwsPy(() => pySetSlice(BA("abc"), 0, 2, 0, B("")),
        "ValueError", "slice step cannot be zero");
});

test("#455 slice ASSIGN error kinds: immutable bytes, bad RHS", () => {
    throwsPy(() => pySetSlice(B("abc"), 1, 2, null, B("X")),
        "TypeError", "'bytes' object does not support item assignment");
    throwsPy(() => pySetSlice(BA("abc"), 0, 2, null, "xy"),
        "TypeError", "can assign only bytes, buffers, or iterables of ints in range(0, 256)");
    throwsPy(() => pySetSlice(BA("abc"), 0, 2, null, 5),
        "TypeError", "can assign only bytes, buffers, or iterables of ints in range(0, 256)");
    throwsPy(() => pySetSlice(BA("abc"), 0, 2, null, [300]),
        "ValueError", "byte must be in range(0, 256)");
});

test("slice DELETE: simple + extended + clamped no-op; bytes is immutable", () => {
    const d = BA("abcdef");
    pyDelSlice(d, 1, 3, null);
    assert.equal(pyRepr(d), "bytearray(b'adef')");
    const e = BA("abcdef");
    pyDelSlice(e, null, null, 2);
    assert.equal(pyRepr(e), "bytearray(b'bdf')");
    const f = BA("abcdef");
    pyDelSlice(f, 10, 20, null);
    assert.equal(pyRepr(f), "bytearray(b'abcdef')");
    throwsPy(() => pyDelSlice(B("abc"), 0, 1, null),
        "TypeError", "'bytes' object does not support item deletion");
});

test("element WRITE: bytes immutable; bytearray validates index and byte", () => {
    throwsPy(() => pySetItem(B("abc"), 0, 65),
        "TypeError", "'bytes' object does not support item assignment");
    const ba = BA("abc");
    pySetItem(ba, -1, 90);
    assert.equal(pyRepr(ba), "bytearray(b'abZ')");
    throwsPy(() => pySetItem(BA("abc"), 10, 1),
        "IndexError", "bytearray index out of range");
    throwsPy(() => pySetItem(BA("abc"), 0, "x"),
        "TypeError", "'str' object cannot be interpreted as an integer");
    throwsPy(() => pySetItem(BA("abc"), 0, 300),
        "ValueError", "byte must be in range(0, 256)");
    throwsPy(() => pySetItem(BA("abc"), "k", 1),
        "TypeError", "bytearray indices must be integers or slices, not str");
});

test("#458 bound-method EXTRACTION dispatches the same engine as direct calls", () => {
    const b = B("banana");
    const m = pyBoundMethod(b, "count");
    assert.equal(typeof m, "function");
    assert.equal(m(B("an")), 2);
    assert.equal(m(97), 3);
    assert.equal(m(97, 2), 2); // optional start survives extraction
    const f = pyBoundMethod(b, "find");
    assert.equal(f(B("na")), 2);
    const s = pyBoundMethod(BA("banana"), "startswith");
    assert.equal(s(B("ban")), true);
    const i = pyBoundMethod(b, "index");
    assert.equal(i(B("an")), 1);
    // Extraction and direct dispatch agree (same prototype method).
    assert.equal(m(B("an")), pyCount(b, B("an")));
});

test("#458 direct dispatch through the Multi/Str runtime helpers", () => {
    const b = B("banana");
    assert.equal(pyCount(b, B("an")), 2);
    assert.equal(pyCount(b, B(""), 2, 4), 3);   // start/end forwarded
    assert.equal(pyCount(b, 97, 2, 4), 1);
    assert.equal(pyFind(b, B("na"), -3), 4);
    assert.equal(pyFind(b, B("zz")), -1);
    assert.equal(pyIndex(b, B("an"), 1, 4), 1);
    assert.equal(pyStrStartswith(b, B("ba")), true);
    assert.equal(pyStrStartswith(b, [B("x"), B("ban")]), true); // tuple form
    assert.equal(pyStrEndswith(b, B("an"), 0, 3), true);
    assert.equal(pyStrRfind(b, B("na"), 0, 4), 2);
    assert.equal(pyStrRindex(b, B("na")), 4);
});

test("method error kinds match CPython", () => {
    const b = B("banana");
    throwsPy(() => b.count("x"),
        "TypeError", "argument should be integer or bytes-like object, not 'str'");
    throwsPy(() => b.count(300), "ValueError", "byte must be in range(0, 256)");
    throwsPy(() => b.find(1.5),
        "TypeError", "argument should be integer or bytes-like object, not 'float'");
    throwsPy(() => b.index(B("zz")), "ValueError", "subsection not found");
    throwsPy(() => pyStrRindex(b, B("zz")), "ValueError", "subsection not found");
    throwsPy(() => b.startswith("ba"),
        "TypeError", "startswith first arg must be bytes or a tuple of bytes, not str");
    throwsPy(() => b.startswith(98),
        "TypeError", "startswith first arg must be bytes or a tuple of bytes, not int");
    throwsPy(() => b.endswith(98),
        "TypeError", "endswith first arg must be bytes or a tuple of bytes, not int");
    throwsPy(() => b.startswith([B("a"), "b"]),
        "TypeError", "a bytes-like object is required, not 'str'");
});

test("query-window semantics: empty needles, clamps, inverted windows", () => {
    const b = B("banana");
    assert.equal(b.count(B("")), 7);
    assert.equal(b.count(B(""), 10), 0);
    assert.equal(b.count(B(""), 4, 2), 0);
    assert.equal(b.count(B("an"), true), 2); // bool start index
    assert.equal(b.find(B(""), 6), 6);
    assert.equal(b.find(B(""), 10), -1);
    assert.equal(b.find(B(""), 4, 2), -1);
    assert.equal(b.rfind(B("")), 6);
    assert.equal(b.rfind(B(""), 0, 4), 4);
    assert.equal(b.rfind(B(""), 10), -1);
    assert.equal(b.startswith(B(""), 10), false);
    assert.equal(b.startswith(B("na"), 2), true);
});

// The literal constructor path (pyBytes) yields the same authority-visible
// kind as pyBytesOf — the guard covers both spellings.
test("pyBytes literal ctor classifies identically", () => {
    const lit = pyBytes([97, 98]);
    assert.equal(__pyBytesKind(lit), "bytes");
    assert.equal(pyType(lit).__name__, "bytes");
    assert.equal(pyBool(pyBytes([])), false);
});
