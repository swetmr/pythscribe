// #478 — functools.partial honors keyword arguments, both CONSTRUCTION-time
// (partial(power, exp=2)) and CALL-time (partial(f, 10)(20, c=30)), routed
// through the SHIPPED kwargs calling convention (__pyCallKw → __pyKwArgs →
// marked carrier → __pyTakeKw). Each assertion is a NEGATIVE CONTROL:
// reverting partial to the old `func(...args, ...moreArgs)` body makes the
// kwarg rows RED (the kwargs leak in as a trailing positional dict), and
// reverting __PYKW_MARK from Symbol.for back to Symbol() makes them RED in
// the cross-copy scenario. Oracle: CPython 3.14.
import test from "node:test";
import assert from "node:assert/strict";
import { partial } from "./functools.js";
import { __pyCallKw } from "../runtime.js";

function f(a, b, c) { return [a, b, c]; }
f.__pyparams__ = ["a", "b", "c"];
function power(base, exp) { return base ** exp; }
power.__pyparams__ = ["base", "exp"];

test("#478 call-time kwargs bind by name (partial(f,10)(20, c=30))", () => {
    const g = partial(f, 10);
    assert.deepEqual(__pyCallKw(g, [20], { c: 30 }), [10, 20, 30]); // CPython (10,20,30)
});

test("#478 construction-time kwargs captured (partial(power, exp=2)(5))", () => {
    const square = __pyCallKw(partial, [power], { exp: 2 });
    assert.equal(square(5), 25); // CPython 25
});

test("#478 construction kwarg + later positional (partial(f,1,c=99)(2))", () => {
    const h = __pyCallKw(partial, [f, 1], { c: 99 });
    assert.deepEqual(h(2), [1, 2, 99]); // CPython (1,2,99)
});

test("#478 call-time kwarg overrides construction-time (CPython merge)", () => {
    const h = __pyCallKw(partial, [f, 1], { c: 99 });
    assert.deepEqual(__pyCallKw(h, [2], { c: 7 }), [1, 2, 7]); // call wins
});

test("#478 no-kwargs positional path unchanged (interop-safe)", () => {
    // A metadata-less callee must NOT receive a spurious trailing options
    // object when no keywords are involved.
    const seen = [];
    const jsfn = (...a) => { seen.push(a.length); return a; };
    assert.deepEqual(partial(jsfn, 1)(2, 3), [1, 2, 3]);
    assert.deepEqual(seen, [3]); // exactly 3 args, no trailing {}
    assert.deepEqual(partial(f, 1)(2, 3), [1, 2, 3]);
});
