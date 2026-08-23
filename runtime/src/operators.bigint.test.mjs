// Phase 1 (arbitrary-precision int): operator helpers must treat BigInt as
// Python `int` and Number as Python `float`, following CPython mixing rules:
//   int op int   -> int   (BigInt)
//   int op float -> float (Number)
//   int / int    -> float (true division)
// CPython is the oracle for every expected value below.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
    pyAdd, pySub, pyMul, pyDiv, pyFloorDiv, pyMod, pyPow,
    pyLt, pyLe, pyGt, pyGe, pyEq, pyNe, pyNeg,
} from "./operators.js";

test("int+int stays exact BigInt past 2**53", () => {
    assert.equal(pyAdd(9007199254740992n, 1n), 9007199254740993n);
    assert.equal(pyMul(9007199254740993n, 1n), 9007199254740993n);
    assert.equal(pyPow(2n, 53n) + 1n, 9007199254740993n);
});

test("int+float promotes to float (boxed iff integer-valued)", async () => {
    // Option B: a FLOAT operand arrives boxed when integer-valued (a bare
    // native 2.0 is an int by representation — codegen boxes float
    // literals). Integer-valued float results carry the PyFloat brand;
    // non-integer results stay native Numbers.
    const { __pyF } = await import("./operators.js");
    const a = pyAdd(1n, __pyF(2.0));
    assert.equal(a.__pyfloat__, true);
    assert.equal(Number(a), 3);
    const b = pyAdd(__pyF(2.0), 1n);
    assert.equal(b.__pyfloat__, true);
    assert.equal(Number(b), 3);
    assert.equal(pySub(5n, 1.5), 3.5); // non-integer result: native
    assert.equal(typeof pySub(5n, 1.5), "number");
    const m = pyMul(2n, 2.5); // 5.0: integer-valued -> boxed
    assert.equal(m.__pyfloat__, true);
    assert.equal(Number(m), 5);
});

test("true division always returns float", () => {
    assert.equal(pyDiv(7n, 2n), 3.5);
    assert.equal(typeof pyDiv(7n, 2n), "number");
});

test("floor div / mod use Python sign rules, downcast small to Number", () => {
    // Small integer results normalize back to native Number (hybrid).
    assert.equal(pyFloorDiv(7n, 2n), 3);
    assert.equal(pyFloorDiv(-7n, 2n), -4); // Python floors toward -inf
    assert.equal(pyMod(7n, -3n), -2); // sign of divisor
    assert.equal(pyMod(-7n, 3n), 2);
    assert.equal(typeof pyFloorDiv(7n, 2n), "number");
});

test("small int arithmetic stays native Number; overflow promotes", () => {
    assert.equal(pyAdd(2, 3), 5);
    assert.equal(typeof pyAdd(2, 3), "number");
    assert.equal(pyMul(3, 4), 12);
    // Overflow of safe-integer Numbers recomputes exactly in BigInt
    // (9007199254740991 is Number.MAX_SAFE_INTEGER; +2 is unrepresentable
    // as Number but exact via BigInt).
    assert.equal(pyAdd(9007199254740991, 2), 9007199254740993n);
    assert.equal(typeof pyAdd(9007199254740991, 2), "bigint");
});

test("comparisons work across BigInt/Number", () => {
    assert.equal(pyLt(2n, 3.0), true);
    assert.equal(pyLe(3n, 3n), true);
    assert.equal(pyGt(5n, 2.0), true);
    assert.equal(pyGe(2n, 3n), false);
});

test("equality is value-based across representations", () => {
    assert.equal(pyEq(5n, 5), true); // Python: 5 == 5.0 is True
    assert.equal(pyEq(5n, 5n), true);
    assert.equal(pyEq(5n, 6n), false);
    assert.equal(pyNe(5n, 5), false);
});

test("unary negation downcasts small, keeps large as BigInt", () => {
    assert.equal(pyNeg(5n), -5); // small → Number
    assert.equal(typeof pyNeg(5n), "number");
    assert.equal(pyNeg(90071992547409930n), -90071992547409930n); // large → BigInt
});
