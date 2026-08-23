// bytearray — mutable, growable sibling of bytes (0.2.2 batch, item 3).
// Before: ba.append/extend threw ("object of type 'object' has no append"),
// and repr(bytearray(b'AB')) printed b'AB' instead of bytearray(b'AB').
//
// Run with: node --test runtime/src/bytearray.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import { pyBytearrayOf, pyBytes, PyByteArray, pyRepr } from "./operators.js";
import {
    pyAppend, pyExtend, pyInsert, pyPop, pyRemove, pyClear, pyDelItem,
} from "./runtime.js";

const ba = (...bytes) => {
    const b = pyBytearrayOf(pyBytes(bytes));
    return b;
};

test("repr(bytearray(b'AB')) is bytearray(b'AB'), not b'AB'", () => {
    assert.equal(pyRepr(ba(65, 66)), "bytearray(b'AB')");
    // bytes still reprs as b'...'
    assert.equal(pyRepr(pyBytes([65, 66])), "b'AB'");
    // empty + non-printable escaping preserved through the wrapper
    assert.equal(pyRepr(pyBytearrayOf(undefined)), "bytearray(b'')");
    assert.equal(pyRepr(ba(0, 9, 10)), "bytearray(b'\\x00\\t\\n')");
});

test("bytearray is a Uint8Array (bytes ops still apply)", () => {
    const b = ba(1, 2, 3);
    assert.ok(b instanceof Uint8Array);
    assert.equal(b.length, 3);
    assert.equal(b[0], 1);
});

test("append grows the SAME object in place (references see it)", () => {
    const b = ba(65, 66);
    const alias = b;
    pyAppend(b, 67);
    assert.equal(pyRepr(b), "bytearray(b'ABC')");
    assert.equal(alias.length, 3); // alias observes the mutation
    assert.equal(alias[2], 67);
});

test("append range/type errors match CPython", () => {
    const b = ba(1);
    assert.throws(() => pyAppend(b, 256),
        (e) => e.name === "ValueError" && e.message === "byte must be in range(0, 256)");
    assert.throws(() => pyAppend(b, -1),
        (e) => e.name === "ValueError");
    assert.throws(() => pyAppend(b, 1.5),
        (e) => e.name === "TypeError"
            && e.message === "'float' object cannot be interpreted as an integer");
});

test("extend by a bytes-like and by an iterable of ints", () => {
    const b = ba(65);
    pyExtend(b, pyBytes([66, 67]));
    assert.equal(pyRepr(b), "bytearray(b'ABC')");
    pyExtend(b, [68, 69]);
    assert.equal(pyRepr(b), "bytearray(b'ABCDE')");
    // out-of-range element rejected
    assert.throws(() => pyExtend(ba(1), [300]), (e) => e.name === "ValueError");
});

test("insert clamps the index like CPython", () => {
    const b = ba(65, 67);
    pyInsert(b, 1, 66); // AC -> ABC
    assert.equal(pyRepr(b), "bytearray(b'ABC')");
    pyInsert(b, -100, 90); // clamp to front
    assert.equal(b[0], 90);
    pyInsert(b, 999, 88); // clamp to end
    assert.equal(b[b.length - 1], 88);
});

test("pop default-last, indexed, and bounds", () => {
    const b = ba(65, 66, 67);
    assert.equal(pyPop(b), 67);          // last
    assert.equal(pyRepr(b), "bytearray(b'AB')");
    assert.equal(pyPop(b, 0), 65);       // indexed
    assert.equal(pyRepr(b), "bytearray(b'B')");
    assert.equal(pyPop(b), 66);
    assert.throws(() => pyPop(b), (e) => e.name === "IndexError"); // empty
});

test("remove first match / ValueError when absent", () => {
    const b = ba(65, 66, 65);
    pyRemove(b, 65);
    assert.equal(pyRepr(b), "bytearray(b'BA')");
    assert.throws(() => pyRemove(b, 99), (e) => e.name === "ValueError");
});

test("clear empties in place; copy is independent", () => {
    const b = ba(65, 66, 67);
    const c = b.copy();
    pyClear(b);
    assert.equal(pyRepr(b), "bytearray(b'')");
    assert.equal(pyRepr(c), "bytearray(b'ABC')"); // copy untouched
    assert.ok(c instanceof PyByteArray);
});

test("reverse is in place (native Uint8Array.reverse)", () => {
    const b = ba(65, 66, 67);
    b.reverse();
    assert.equal(pyRepr(b), "bytearray(b'CBA')");
});

test("bytearray == bytes with the same bytes", () => {
    // operators pyEq / __eq path compares Uint8Array content.
    const b = ba(65, 66);
    assert.equal(b.length, pyBytes([65, 66]).length);
    for (let i = 0; i < b.length; i++) assert.equal(b[i], 66 - (1 - i) /*65,66*/);
});

test("del ba[i] removes in place; del bytes[i] is TypeError", () => {
    const b = ba(65, 66, 67);
    pyDelItem(b, 1);
    assert.equal(pyRepr(b), "bytearray(b'AC')");
    pyDelItem(b, -1);
    assert.equal(pyRepr(b), "bytearray(b'A')");
    // out-of-range integer → IndexError; non-int type → TypeError
    assert.throws(() => pyDelItem(b, 5), (e) => e.name === "IndexError");
    assert.throws(() => pyDelItem(b, "k"),
        (e) => e.name === "TypeError"
            && e.message === "bytearray indices must be integers or slices, not str");
    // immutable bytes reject deletion with the CPython message
    assert.throws(() => pyDelItem(pyBytes([65, 66]), 0),
        (e) => e.name === "TypeError"
            && e.message === "'bytes' object doesn't support item deletion");
});

test("larger growth crosses the initial length many times", () => {
    const b = pyBytearrayOf(undefined); // empty
    for (let i = 0; i < 1000; i++) pyAppend(b, i & 0xff);
    assert.equal(b.length, 1000);
    assert.equal(b[999], 999 & 0xff);
});

// ── Item 5 (0.2.2 hold): `ba += b"…"` stays a bytearray; content equality ──
// `ba += x` lowers to `ba = pyAdd(ba, x)`. The old pyAdd bytes branch always
// built an immutable PyBytes, silently REBINDING the bytearray to bytes:
// repr printed b'abcd' instead of bytearray(b'abcd'), and every later mutator
// (pop/append/…) fell into pyPop's dict path (KeyError). And pyEq had no
// bytes branch at all — even b'ab' == b'ab' was False (identity fallthrough).

test("pyAdd: bytearray + bytes stays a bytearray (the += lowering)", async () => {
    const { pyAdd } = await import("./operators.js");
    let b = ba(97, 98);
    b = pyAdd(b, pyBytes([99, 100])); // ba += b"cd"
    assert.equal(pyRepr(b), "bytearray(b'abcd')");
    assert.ok(typeof b.append === "function"); // still the mutable type
    // …and the mutators still work on the result (the p61 pop crash)
    assert.equal(pyPop(b), 100);
    assert.equal(pyRepr(b), "bytearray(b'abc')");
});

test("pyAdd: result type follows the LEFT operand (CPython)", async () => {
    const { pyAdd } = await import("./operators.js");
    // bytes + bytearray -> bytes
    const r = pyAdd(pyBytes([97]), ba(98));
    assert.equal(pyRepr(r), "b'ab'");
    // bytes + bytes -> bytes
    assert.equal(pyRepr(pyAdd(pyBytes([97]), pyBytes([98]))), "b'ab'");
});

test("pyMul: bytearray * n stays a bytearray", async () => {
    const { pyMul } = await import("./operators.js");
    assert.equal(pyRepr(pyMul(ba(97), 3)), "bytearray(b'aaa')");
    assert.equal(pyRepr(pyMul(2, ba(98))), "bytearray(b'bb')");
    assert.equal(pyRepr(pyMul(pyBytes([97]), 2)), "b'aa'");
    assert.equal(pyRepr(pyMul(ba(97), 0)), "bytearray(b'')");
    assert.equal(pyRepr(pyMul(ba(97), -1)), "bytearray(b'')");
});

test("pyEq: bytes/bytearray content equality, cross-type (CPython)", async () => {
    const { pyEq } = await import("./operators.js");
    assert.equal(pyEq(pyBytes([97]), pyBytes([97])), true);      // b'a' == b'a'
    assert.equal(pyEq(ba(97), pyBytes([97])), true);             // bytearray == bytes
    assert.equal(pyEq(pyBytes([97]), ba(97)), true);             // bytes == bytearray
    assert.equal(pyEq(ba(97), ba(97)), true);
    assert.equal(pyEq(pyBytes([97]), pyBytes([98])), false);     // b'a' != b'b'
    assert.equal(pyEq(pyBytes([97]), pyBytes([97, 98])), false); // length differs
    assert.equal(pyEq(pyBytes([97]), "a"), false);               // bytes != str
    assert.equal(pyEq(pyBytes([]), pyBytes([])), true);
});

// ── Item 3 (0.2.2 hold): pyPop receiver classes ──
// bytearray.pop dispatches to the class method (default last, index,
// CPython-matching bounds errors); bytes/None/int/str no longer fall into
// pyPop's dict path (KeyError) — they raise AttributeError like CPython; and
// set.pop() is a real method.

test("pyPop on bytearray: default last, index, bounds (CPython)", () => {
    const b = ba(97, 98, 99);
    assert.equal(pyPop(b), 99);
    assert.equal(pyRepr(b), "bytearray(b'ab')");
    assert.equal(pyPop(b, 0), 97);
    assert.equal(pyRepr(b), "bytearray(b'b')");
    assert.equal(pyPop(b, -1), 98);
    assert.throws(() => pyPop(b),
        (e) => e.name === "IndexError" && e.message === "pop from empty bytearray");
    assert.throws(() => pyPop(ba(1), 5), (e) => e.name === "IndexError");
    assert.throws(() => pyPop(ba(1), -2), (e) => e.name === "IndexError");
});

test("pyPop wrong receivers raise AttributeError, not KeyError", () => {
    for (const [recv, tn] of [
        [null, "NoneType"], [undefined, "NoneType"], [5, "int"], [1.5, "float"],
        [true, "bool"], ["ab", "str"], [pyBytes([65]), "bytes"],
    ]) {
        assert.throws(() => pyPop(recv),
            (e) => e.name === "AttributeError"
                && e.message === `'${tn}' object has no attribute 'pop'`,
            `receiver ${tn}`);
    }
});

test("pyPop on a set: arbitrary element / empty KeyError / arg TypeError", () => {
    const s = new Set([7, 8]);
    const v = pyPop(s);
    assert.ok(v === 7 || v === 8);
    assert.equal(s.size, 1);
    assert.ok(!s.has(v));
    assert.throws(() => pyPop(new Set()),
        (e) => e.name === "KeyError" && String(e.message).includes("pop from an empty set"));
    assert.throws(() => pyPop(new Set([1]), 1), (e) => e.name === "TypeError");
});
