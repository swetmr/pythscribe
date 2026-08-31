// ═══ F5 (v0.2.4) — SLICE COMPONENT TYPE GUARD MATRIX ═══
//
// CPython (_PyEval_SliceIndex): a slice start/stop/step must be an int, None,
// or expose __index__ — anything else raises
//   TypeError: slice indices must be integers or None or have an __index__ method
// The INDEX arm has carried this guard since crit-8/F7 ([1,2,3][1.0] raises),
// but the SLICE arm silently accepted floats: [1,2,3][0:2.0] returned [1,2].
// One validator (__pySliceIndex) now fronts ALL THREE slice ops — this matrix
// enumerates {get, set, del} × {list, str, tuple, bytearray} × {start, stop,
// step} × {boxed integer-valued float, native non-integer float, str,
// container} so a per-arm regression cannot reopen the class silently.
// F5-r2: the bytearray WRITE/DELETE arms (__byteArraySetSlice/DelSlice) were
// dispatched through __setitem__/__delitem__ BEFORE validation and coerced
// bounds via Number(...) — they now validate through the same __pySliceIndex
// pre-mutation (rows below assert the bytearray is untouched on rejection).
//
// THEOREM-BLIND: the Lean slice theorems quantify over integer bounds; this
// guard sits in FRONT of them at runtime — no Lean statement was touched.
//
// Goldens: live CPython 3.12 (scratchpad f5_rows.py sweep, 2026-08-26).

import { test } from "node:test";
import assert from "node:assert/strict";
import {
    pySlice, pySetSlice, pyDelSlice, TypeError as PyTypeError, ValueError,
} from "./runtime.js";
import { __pyF, pyRepr, pyBytearrayOf } from "./operators.js";

const MSG = "slice indices must be integers or None or have an __index__ method";

// The value kinds CPython rejects as slice components. Boxed integer-valued
// floats (__pyF) are how `2.0` reaches the runtime under Option B; native
// non-integer numbers are how `2.5` does; a runtime-typed variable is the
// same value at this level (the runtime cannot see literalness).
const BAD = {
    "float-boxed": () => __pyF(2),
    "float-native": () => 2.5,
    str: () => "1",
    list: () => [1],
    dict: () => new Map([[1, 2]]),
};

const SEQS = {
    list: () => [1, 2, 3, 4],
    str: () => "abcd",
    tuple: () => {
        const t = [1, 2, 3, 4];
        Object.defineProperty(t, "__pytuple__", { value: true, enumerable: false });
        return t;
    },
};

function expectTypeError(fn, label) {
    assert.throws(fn, (e) => e instanceof PyTypeError && e.message === MSG,
        `${label}: expected TypeError('${MSG}')`);
}

test("F5 matrix: get-slice rejects every non-index component kind", () => {
    for (const [sname, mk] of Object.entries(SEQS)) {
        for (const [vname, mkv] of Object.entries(BAD)) {
            expectTypeError(() => pySlice(mk(), mkv(), null, null), `get ${sname} start=${vname}`);
            expectTypeError(() => pySlice(mk(), null, mkv(), null), `get ${sname} stop=${vname}`);
            expectTypeError(() => pySlice(mk(), null, null, mkv()), `get ${sname} step=${vname}`);
            expectTypeError(() => pySlice(mk(), 0, mkv(), null), `get ${sname} mixed stop=${vname}`);
        }
    }
});

test("F5 matrix: set-slice and del-slice reject the same kinds (list)", () => {
    for (const [vname, mkv] of Object.entries(BAD)) {
        expectTypeError(() => pySetSlice([1, 2, 3], mkv(), null, null, [9]), `set start=${vname}`);
        expectTypeError(() => pySetSlice([1, 2, 3], 0, mkv(), null, [9]), `set stop=${vname}`);
        expectTypeError(() => pySetSlice([1, 2, 3, 4], null, null, mkv(), [9, 9]), `set step=${vname}`);
        expectTypeError(() => pyDelSlice([1, 2, 3], mkv(), null, null), `del start=${vname}`);
        expectTypeError(() => pyDelSlice([1, 2, 3], 0, mkv(), null), `del stop=${vname}`);
        expectTypeError(() => pyDelSlice([1, 2, 3, 4], null, null, mkv()), `del step=${vname}`);
    }
});

test("F5-r2 matrix: bytearray slice WRITE/DELETE validate before mutation", () => {
    for (const [vname, mkv] of Object.entries(BAD)) {
        const mkba = () => pyBytearrayOf([97, 98, 99, 100]); // bytearray(b'abcd')
        let ba = mkba();
        expectTypeError(() => pySetSlice(ba, mkv(), null, null, mkba()), `ba set start=${vname}`);
        assert.equal(pyRepr(ba), "bytearray(b'abcd')", `ba untouched after start=${vname}`);
        ba = mkba();
        expectTypeError(() => pySetSlice(ba, 0, mkv(), null, mkba()), `ba set stop=${vname}`);
        assert.equal(pyRepr(ba), "bytearray(b'abcd')", `ba untouched after stop=${vname}`);
        ba = mkba();
        expectTypeError(() => pySetSlice(ba, null, null, mkv(), mkba()), `ba set step=${vname}`);
        assert.equal(pyRepr(ba), "bytearray(b'abcd')", `ba untouched after step=${vname}`);
        ba = mkba();
        expectTypeError(() => pyDelSlice(ba, mkv(), null, null), `ba del start=${vname}`);
        assert.equal(pyRepr(ba), "bytearray(b'abcd')", `ba untouched after del start=${vname}`);
        ba = mkba();
        expectTypeError(() => pyDelSlice(ba, 0, mkv(), null), `ba del stop=${vname}`);
        ba = mkba();
        expectTypeError(() => pyDelSlice(ba, null, null, mkv()), `ba del step=${vname}`);
    }
    // valid bytearray slice writes/deletes preserved (resizing + extended)
    const ba = pyBytearrayOf([97, 98, 99, 100]);
    pySetSlice(ba, 0, 2, null, pyBytearrayOf([120]));       // ba[0:2] = b'x'
    assert.equal(pyRepr(ba), "bytearray(b'xcd')");
    pySetSlice(ba, null, null, 2, pyBytearrayOf([65, 66])); // ba[::2] = b'AB'
    assert.equal(pyRepr(ba), "bytearray(b'AcB')");
    pyDelSlice(ba, 0, 1, null);                              // del ba[0:1]
    assert.equal(pyRepr(ba), "bytearray(b'cB')");
    // __index__ components accepted, zero step stays ValueError
    pySetSlice(ba, { __index__: () => 0 }, 1, null, pyBytearrayOf([122]));
    assert.equal(pyRepr(ba), "bytearray(b'zB')");
    assert.throws(() => pySetSlice(ba, null, null, 0, pyBytearrayOf([])),
        (e) => e instanceof ValueError && e.message === "slice step cannot be zero");
    assert.throws(() => pyDelSlice(ba, null, null, 0),
        (e) => e instanceof ValueError && e.message === "slice step cannot be zero");
});

test("F5: witnesses — the exact reported silent-wrong-value cases now raise", () => {
    // [1,2,3][0:2.0] was [1,2]; [1,2,3][1.0:] was [2,3]; [1,2,3,4][::2.0] was
    // [1,3]; 'abcd'[0:2.0] was 'ab'; x=2.0; [1,2,3][0:x] was [1,2].
    expectTypeError(() => pySlice([1, 2, 3], 0, __pyF(2), null), "[1,2,3][0:2.0]");
    expectTypeError(() => pySlice([1, 2, 3], __pyF(1), null, null), "[1,2,3][1.0:]");
    expectTypeError(() => pySlice([1, 2, 3, 4], null, null, __pyF(2)), "[1,2,3,4][::2.0]");
    expectTypeError(() => pySlice("abcd", 0, __pyF(2), null), "'abcd'[0:2.0]");
    const x = __pyF(2); // runtime-typed: x = 2.0
    expectTypeError(() => pySlice([1, 2, 3], 0, x, null), "x=2.0; [1,2,3][0:x]");
});

test("F5: valid components preserved — int/None/bool/bigint/__index__", () => {
    assert.deepEqual(pySlice([1, 2, 3], 0, 2, null), [1, 2]);
    assert.deepEqual(pySlice([1, 2, 3], true, null, null), [2, 3]); // bool ⊂ int
    assert.deepEqual(pySlice([1, 2, 3, 4], null, null, 2), [1, 3]);
    assert.equal(pySlice("abcd", 1, 3, null), "bc");
    // huge int bounds still demote/clamp per CPython
    assert.deepEqual(pySlice([1, 2, 3], 10n ** 100n, null, null), []);
    assert.deepEqual(pySlice([1, 2], null, null, 10n ** 100n), [1]);
    // __index__ protocol object is accepted (CPython: np.int64-style indexes)
    const ix = { __index__: () => 2 };
    assert.deepEqual(pySlice([1, 2, 3], 0, ix, null), [1, 2]);
    // negative-step read unchanged
    assert.deepEqual(pySlice([1, 2, 3], null, null, -1), [3, 2, 1]);
    // write/delete arms still work
    const xs = [1, 2, 3, 4];
    pySetSlice(xs, 0, 2, null, [9]);
    assert.deepEqual(xs, [9, 3, 4]);
    pyDelSlice(xs, 0, 1, null);
    assert.deepEqual(xs, [3, 4]);
    // tuple slicing preserved (kind + values)
    const t = SEQS.tuple();
    const r = pySlice(t, 1, 3, null);
    assert.equal(pyRepr(r), "(2, 3)");
});

test("F5: step precedence and zero-step kind preserved", () => {
    // CPython PySlice_Unpack validates step FIRST: float start + zero step →
    // ValueError (step), not TypeError (start).
    assert.throws(() => pySlice([1, 2, 3], __pyF(1), null, 0),
        (e) => e instanceof ValueError && e.message === "slice step cannot be zero");
    // zero step alone stays ValueError on all three ops
    assert.throws(() => pySetSlice([1, 2], null, null, 0, []),
        (e) => e instanceof ValueError);
    assert.throws(() => pyDelSlice([1, 2], null, null, 0),
        (e) => e instanceof ValueError);
});

test("F5: custom __getitem__/__setitem__ objects still receive the raw slice", () => {
    // CPython validates components only when a BUILT-IN sequence consumes
    // them — slice(0, 2.5) handed to a user __getitem__ arrives untouched.
    let got = null;
    const obj = { __getitem__(s) { got = s; return "ok"; } };
    assert.equal(pySlice(obj, 0, 2.5, null), "ok");
    assert.equal(got.stop, 2.5);
});
