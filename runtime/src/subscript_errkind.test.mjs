// Error-KIND parity for subscript deletion / assignment (0.2.2 batch, items
// 2b + 2c). CPython raises a TypeError — with a specific message — when a
// subscript key is the wrong TYPE, or when the object does not support the
// operation at all. The runtime used to raise the wrong KIND (IndexError /
// KeyError) or a truncated message.
//
// Run with: node --test runtime/src/subscript_errkind.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import { pyDelItem, pySetItem } from "./runtime.js";
import { pyTupleOf } from "./operators.js";

// Assert `fn` throws an error whose Python type name is `name` and whose
// message equals `msg` (CPython-exact).
function throwsPy(fn, name, msg) {
    assert.throws(fn, (e) => {
        assert.equal(e.name, name, `expected ${name}, got ${e.name}: ${e.message}`);
        assert.equal(e.message, msg, `message mismatch`);
        return true;
    });
}

// ── 2b: pyDelItem — F7 type-rejection guards ────────────────────────────────

test("del list[non-int] is TypeError (not IndexError)", () => {
    // CPython: del [1,2,3][1.5] -> TypeError: list indices must be integers or
    // slices, not float. Was: IndexError "list assignment index out of range".
    throwsPy(() => pyDelItem([1, 2, 3], 1.5), "TypeError",
        "list indices must be integers or slices, not float");
    throwsPy(() => pyDelItem([1, 2, 3], "k"), "TypeError",
        "list indices must be integers or slices, not str");
    throwsPy(() => pyDelItem([1, 2, 3], null), "TypeError",
        "list indices must be integers or slices, not NoneType");
});

test("del list[int] out of range stays IndexError", () => {
    throwsPy(() => pyDelItem([1, 2, 3], 5), "IndexError",
        "list assignment index out of range");
});

test("del list[valid int] deletes (incl. negative and bool index)", () => {
    const xs = [10, 20, 30];
    pyDelItem(xs, -1);
    assert.deepEqual(xs, [10, 20]);
    pyDelItem(xs, true); // bool ⊂ int → index 1
    assert.deepEqual(xs, [10]);
});

test("del tuple[i] is TypeError (immutable), not a silent splice", () => {
    const t = pyTupleOf([1, 2, 3]);
    throwsPy(() => pyDelItem(t, 0), "TypeError",
        "'tuple' object doesn't support item deletion");
    assert.equal(t.length, 3); // untouched
});

test("del on a non-subscriptable object is TypeError (not KeyError)", () => {
    throwsPy(() => pyDelItem(5, 0), "TypeError",
        "'int' object doesn't support item deletion");
    throwsPy(() => pyDelItem(3.5, 0), "TypeError",
        "'float' object doesn't support item deletion");
    throwsPy(() => pyDelItem(true, 0), "TypeError",
        "'bool' object doesn't support item deletion");
    throwsPy(() => pyDelItem("abc", 0), "TypeError",
        "'str' object doesn't support item deletion");
    throwsPy(() => pyDelItem(new Set([1, 2]), 0), "TypeError",
        "'set' object doesn't support item deletion");
});

// ── 2c: pySetItem — full "…, not <type>" message ────────────────────────────

test("list[non-int] = v carries the ', not <type>' suffix", () => {
    // CPython: [1,2,3]["k"] = 5 -> TypeError: list indices must be integers or
    // slices, not str. Was: "list indices must be integers or slices".
    throwsPy(() => pySetItem([1, 2, 3], "k", 5), "TypeError",
        "list indices must be integers or slices, not str");
    throwsPy(() => pySetItem([1, 2, 3], 1.5, 5), "TypeError",
        "list indices must be integers or slices, not float");
    throwsPy(() => pySetItem([1, 2, 3], null, 5), "TypeError",
        "list indices must be integers or slices, not NoneType");
});

test("list[valid int] = v still assigns (bool index coerces)", () => {
    const xs = [1, 2, 3];
    pySetItem(xs, 0, 99);
    pySetItem(xs, true, 88); // index 1
    assert.deepEqual(xs, [99, 88, 3]);
});
