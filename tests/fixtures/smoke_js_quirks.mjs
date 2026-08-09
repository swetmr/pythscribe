// End-to-end smoke for JS-quirk Python-faithfulness.
// Pulls helpers from the runtime source directly and re-evaluates the
// compiled functions against them.

import { pyBool } from "../../runtime/src/types.js";
import { pyEq } from "../../runtime/src/operators.js";

// Recreate the compiled functions by hand, mirroring js_quirks.js output.
function truthy_test() {
    let empty = [];
    if (pyBool(empty)) return "js";
    return "python";
}

function eq_test() {
    let a = [1, 2, 3];
    let b = [1, 2, 3];
    return pyEq(a, b);
}

function concat_test() {
    let a = [1, 2];
    let b = [3, 4];
    let c = [...a, ...b];
    return c.length;
}

function dict_truthy_test() {
    let d = ({});
    if (pyBool(d)) return "js";
    return "python";
}

function neq_test() {
    let a = [1, 2];
    let b = [1, 2, 3];
    return (!pyEq(a, b));
}

let failed = 0;
function assert(name, actual, expected) {
    const ok = actual === expected;
    console.log(`${ok ? "PASS" : "FAIL"}  ${name}: got ${JSON.stringify(actual)}, expected ${JSON.stringify(expected)}`);
    if (!ok) failed++;
}

assert("truthy_test (empty list falsy)", truthy_test(), "python");
assert("eq_test ([1,2,3] == [1,2,3])", eq_test(), true);
assert("concat_test (len of [1,2]+[3,4])", concat_test(), 4);
assert("dict_truthy_test (empty dict falsy)", dict_truthy_test(), "python");
assert("neq_test ([1,2] != [1,2,3])", neq_test(), true);

if (failed > 0) {
    console.error(`\n${failed} test(s) failed`);
    process.exit(1);
}
console.log(`\nAll JS-quirk smoke tests passed.`);
