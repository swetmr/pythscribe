// #479 — `list += <iterable>` extends in place from ANY iterable (CPython's
// list.__iadd__ IS list.extend), raises TypeError on a non-iterable, and
// preserves in-place aliasing. SPOT witnesses pinned to the SHIPPED pyIAdd;
// each is a NEGATIVE CONTROL — reverting pyIAdd to the old `Array.isArray(b)`
// gate turns the str/set/dict/generator rows RED, and dropping the snapshot
// turns the self-extend row RED (RangeError). Oracle: CPython 3.14.
import test from "node:test";
import assert from "node:assert/strict";
import { pyIAdd } from "./operators.js";

const arr = (x) => Array.from(x);

test("#479 list += every iterable kind (extend, not concat-only)", () => {
    // tuple (a pyths tuple is a branded Array — the pre-fix path already
    // handled this via Array.isArray)
    const t = [1, 2]; const tup = [3, 4];
    Object.defineProperty(tup, "__pytuple__", { value: true, enumerable: false });
    assert.deepEqual(pyIAdd(t, tup), [1, 2, 3, 4]);
    // str → code points (CPython: [1] += "ab" → [1, 'a', 'b'])
    assert.deepEqual(pyIAdd([1], "ab"), [1, "a", "b"]);
    // set → elements
    assert.deepEqual(pyIAdd([1], new Set([5, 6])).sort(), [1, 5, 6]);
    // dict (Map) → KEYS (CPython extends over keys)
    const m = new Map([["a", 1], ["b", 2]]);
    assert.deepEqual(pyIAdd([1], m), [1, "a", "b"]);
    // generator → consumed
    function* g() { yield 0; yield 1; yield 2; }
    assert.deepEqual(pyIAdd([9], g()), [9, 0, 1, 2]);
});

test("#479 in-place aliasing preserved (y is x after x += (…))", () => {
    const y = [1, 2];
    const x = y;
    const r = pyIAdd(x, [3, 4]);
    assert.equal(r, y);          // identity: same object mutated & returned
    assert.deepEqual(y, [1, 2, 3, 4]);
});

test("#479 self-extend terminates (snapshot; CPython x += x copies)", () => {
    const x = [1, 2];
    assert.deepEqual(pyIAdd(x, x), [1, 2, 1, 2]);
});

test("#479 non-iterable rhs raises CPython's TypeError", () => {
    assert.throws(() => pyIAdd([1], 5),
        (e) => e.name === "TypeError" && /'int' object is not iterable/.test(e.message));
    assert.throws(() => pyIAdd([1], null),
        (e) => e.name === "TypeError" && /'NoneType' object is not iterable/.test(e.message));
});
