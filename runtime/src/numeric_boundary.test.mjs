// ═══ NUMERIC VALUE-BOUNDARY GUARD (#460/#461/#464, umbrella #38) ═══
//
// The recurrence guard for the four coexisting numeric value forms:
//
//   int-small     — Python int as a native JS Number (safe range)
//   int-unsafeNum — Python int as a Number PAST 2**53 (inbound native-JS
//                   boundary form: exactly-representable, e.g. 2**53)
//   int-big       — Python int as a BigInt (the exact hybrid form)
//   float-native  — Python float as a non-integer Number (2.5)
//   float-boxed   — Python float as a PyFloat box (8.0)
//
// crossed with every arithmetic op family (+ − × // % / ** and compares),
// asserting (a) the CPython-verified VALUE (via pyRepr) and (b) the
// canonical RESULT FORM — which proves representation is decided by THE
// one authority in operators.js (__isFloat/__intBin/__pyF/__norm/__reqNum)
// and no op path re-decides it locally. The textual single-authority
// freeze lives in crates/pyths_codegen_js/tests/inline_runtime_parity.rs
// (numeric_boundary_single_authority).
//
// Golden values: computed by CPython 3.12 (generator preserved in the
// fix/int-boundary PR description). ONE documented model deviation: int
// operands in a FLOAT-context op (and in true division) are pre-converted
// to double per-operand (the Pyodide/JS representation model), so those
// goldens are CPython's `float(a) op float(b)`. Int-context ops are
// CPython-exact at every magnitude (arbitrary precision).
//
// This file FAILS on the pre-fix runtime (base 3f604460): the old
// __isFloat classified unsafe-integer Numbers as float (#464), __intBin
// never promoted unsafe Number operands (#460), and math.isclose/fsum/gcd
// threw "Cannot mix BigInt" / "Cannot convert a BigInt" (#461).

import { test } from "node:test";
import assert from "node:assert/strict";
import {
    pyAdd, pySub, pyMul, pyDiv, pyFloorDiv, pyMod, pyPow,
    pyLt, pyGt, pyEq, pyNeg, pyPos, pyRepr, pyInt, pyStr,
    __pyF, __pyJs, __reqNum,
} from "./operators.js";
import * as math from "./stdlib/math.js";

const MAKE = {
    // a/b columns of the matrix (values chosen so every form is exercised;
    // int-unsafeNum is EXACTLY representable as a double).
    a: {
        "int-small": () => 7,
        "int-unsafeNum": () => 9007199254740992, // 2**53, inbound Number form
        "int-big": () => 9007199254740993n,
        "float-native": () => 2.5,
        "float-boxed": () => __pyF(8),
    },
    b: {
        "int-small": () => 3,
        "int-unsafeNum": () => 9007199254740992,
        "int-big": () => 9007199254740995n,
        "float-native": () => 2.5,
        "float-boxed": () => __pyF(8),
    },
};
// pow exponents (small, to keep int results printable).
const MAKE_POW_B = {
    "int-small": () => 2,
    "float-boxed": () => __pyF(2),
};

const OPS = {
    add: pyAdd, sub: pySub, mul: pyMul,
    floordiv: pyFloorDiv, mod: pyMod, truediv: pyDiv, pow: pyPow,
};

/** Canonical form of a runtime value — the invariant the authority owns. */
function formOf(v) {
    if (typeof v === "bigint") return "bigint";
    if (v != null && v.__pyfloat__ === true) return "boxed-float";
    if (typeof v === "number") {
        return Number.isInteger(v) ? "num-int" : "num-float";
    }
    return "other:" + typeof v;
}

// [aForm, op, bForm, CPython repr, canonical result form]
const MATRIX = [
    ["int-small", "add", "int-small", "10", "num-int"],
    ["int-small", "sub", "int-small", "4", "num-int"],
    ["int-small", "mul", "int-small", "21", "num-int"],
    ["int-small", "floordiv", "int-small", "2", "num-int"],
    ["int-small", "mod", "int-small", "1", "num-int"],
    ["int-small", "truediv", "int-small", "2.3333333333333335", "num-float"],
    ["int-small", "add", "int-unsafeNum", "9007199254740999", "bigint"],
    ["int-small", "sub", "int-unsafeNum", "-9007199254740985", "num-int"],
    ["int-small", "mul", "int-unsafeNum", "63050394783186944", "bigint"],
    ["int-small", "floordiv", "int-unsafeNum", "0", "num-int"],
    ["int-small", "mod", "int-unsafeNum", "7", "num-int"],
    ["int-small", "truediv", "int-unsafeNum", "7.771561172376096e-16", "num-float"],
    ["int-small", "add", "int-big", "9007199254741002", "bigint"],
    ["int-small", "sub", "int-big", "-9007199254740988", "num-int"],
    ["int-small", "mul", "int-big", "63050394783186965", "bigint"],
    ["int-small", "floordiv", "int-big", "0", "num-int"],
    ["int-small", "mod", "int-big", "7", "num-int"],
    ["int-small", "truediv", "int-big", "7.771561172376093e-16", "num-float"],
    ["int-small", "add", "float-native", "9.5", "num-float"],
    ["int-small", "sub", "float-native", "4.5", "num-float"],
    ["int-small", "mul", "float-native", "17.5", "num-float"],
    ["int-small", "floordiv", "float-native", "2.0", "boxed-float"],
    ["int-small", "mod", "float-native", "2.0", "boxed-float"],
    ["int-small", "truediv", "float-native", "2.8", "num-float"],
    ["int-small", "add", "float-boxed", "15.0", "boxed-float"],
    ["int-small", "sub", "float-boxed", "-1.0", "boxed-float"],
    ["int-small", "mul", "float-boxed", "56.0", "boxed-float"],
    ["int-small", "floordiv", "float-boxed", "0.0", "boxed-float"],
    ["int-small", "mod", "float-boxed", "7.0", "boxed-float"],
    ["int-small", "truediv", "float-boxed", "0.875", "num-float"],
    ["int-unsafeNum", "add", "int-small", "9007199254740995", "bigint"],
    ["int-unsafeNum", "sub", "int-small", "9007199254740989", "num-int"],
    ["int-unsafeNum", "mul", "int-small", "27021597764222976", "bigint"],
    ["int-unsafeNum", "floordiv", "int-small", "3002399751580330", "num-int"],
    ["int-unsafeNum", "mod", "int-small", "2", "num-int"],
    ["int-unsafeNum", "truediv", "int-small", "3002399751580330.5", "num-float"],
    ["int-unsafeNum", "add", "int-unsafeNum", "18014398509481984", "bigint"],
    ["int-unsafeNum", "sub", "int-unsafeNum", "0", "num-int"],
    ["int-unsafeNum", "mul", "int-unsafeNum", "81129638414606681695789005144064", "bigint"],
    ["int-unsafeNum", "floordiv", "int-unsafeNum", "1", "num-int"],
    ["int-unsafeNum", "mod", "int-unsafeNum", "0", "num-int"],
    ["int-unsafeNum", "truediv", "int-unsafeNum", "1.0", "boxed-float"],
    ["int-unsafeNum", "add", "int-big", "18014398509481987", "bigint"],
    ["int-unsafeNum", "sub", "int-big", "-3", "num-int"],
    ["int-unsafeNum", "mul", "int-big", "81129638414606708717386769367040", "bigint"],
    ["int-unsafeNum", "floordiv", "int-big", "0", "num-int"],
    ["int-unsafeNum", "mod", "int-big", "9007199254740992", "bigint"],
    ["int-unsafeNum", "truediv", "int-big", "0.9999999999999996", "num-float"],
    ["int-unsafeNum", "add", "float-native", "9007199254740994.0", "boxed-float"],
    ["int-unsafeNum", "sub", "float-native", "9007199254740990.0", "boxed-float"],
    ["int-unsafeNum", "mul", "float-native", "2.251799813685248e+16", "boxed-float"],
    ["int-unsafeNum", "floordiv", "float-native", "3602879701896396.0", "boxed-float"],
    ["int-unsafeNum", "mod", "float-native", "2.0", "boxed-float"],
    ["int-unsafeNum", "truediv", "float-native", "3602879701896397.0", "boxed-float"],
    ["int-unsafeNum", "add", "float-boxed", "9007199254741000.0", "boxed-float"],
    ["int-unsafeNum", "sub", "float-boxed", "9007199254740984.0", "boxed-float"],
    ["int-unsafeNum", "mul", "float-boxed", "7.205759403792794e+16", "boxed-float"],
    ["int-unsafeNum", "floordiv", "float-boxed", "1125899906842624.0", "boxed-float"],
    ["int-unsafeNum", "mod", "float-boxed", "0.0", "boxed-float"],
    ["int-unsafeNum", "truediv", "float-boxed", "1125899906842624.0", "boxed-float"],
    ["int-big", "add", "int-small", "9007199254740996", "bigint"],
    ["int-big", "sub", "int-small", "9007199254740990", "num-int"],
    ["int-big", "mul", "int-small", "27021597764222979", "bigint"],
    ["int-big", "floordiv", "int-small", "3002399751580331", "num-int"],
    ["int-big", "mod", "int-small", "0", "num-int"],
    ["int-big", "truediv", "int-small", "3002399751580330.5", "num-float"],
    ["int-big", "add", "int-unsafeNum", "18014398509481985", "bigint"],
    ["int-big", "sub", "int-unsafeNum", "1", "num-int"],
    ["int-big", "mul", "int-unsafeNum", "81129638414606690702988259885056", "bigint"],
    ["int-big", "floordiv", "int-unsafeNum", "1", "num-int"],
    ["int-big", "mod", "int-unsafeNum", "1", "num-int"],
    ["int-big", "truediv", "int-unsafeNum", "1.0", "boxed-float"],
    ["int-big", "add", "int-big", "18014398509481988", "bigint"],
    ["int-big", "sub", "int-big", "-2", "num-int"],
    ["int-big", "mul", "int-big", "81129638414606717724586024108035", "bigint"],
    ["int-big", "floordiv", "int-big", "0", "num-int"],
    ["int-big", "mod", "int-big", "9007199254740993", "bigint"],
    ["int-big", "truediv", "int-big", "0.9999999999999996", "num-float"],
    ["int-big", "add", "float-native", "9007199254740994.0", "boxed-float"],
    ["int-big", "sub", "float-native", "9007199254740990.0", "boxed-float"],
    ["int-big", "mul", "float-native", "2.251799813685248e+16", "boxed-float"],
    ["int-big", "floordiv", "float-native", "3602879701896396.0", "boxed-float"],
    ["int-big", "mod", "float-native", "2.0", "boxed-float"],
    ["int-big", "truediv", "float-native", "3602879701896397.0", "boxed-float"],
    ["int-big", "add", "float-boxed", "9007199254741000.0", "boxed-float"],
    ["int-big", "sub", "float-boxed", "9007199254740984.0", "boxed-float"],
    ["int-big", "mul", "float-boxed", "7.205759403792794e+16", "boxed-float"],
    ["int-big", "floordiv", "float-boxed", "1125899906842624.0", "boxed-float"],
    ["int-big", "mod", "float-boxed", "0.0", "boxed-float"],
    ["int-big", "truediv", "float-boxed", "1125899906842624.0", "boxed-float"],
    ["float-native", "add", "int-small", "5.5", "num-float"],
    ["float-native", "sub", "int-small", "-0.5", "num-float"],
    ["float-native", "mul", "int-small", "7.5", "num-float"],
    ["float-native", "floordiv", "int-small", "0.0", "boxed-float"],
    ["float-native", "mod", "int-small", "2.5", "num-float"],
    ["float-native", "truediv", "int-small", "0.8333333333333334", "num-float"],
    ["float-native", "add", "int-unsafeNum", "9007199254740994.0", "boxed-float"],
    ["float-native", "sub", "int-unsafeNum", "-9007199254740990.0", "boxed-float"],
    ["float-native", "mul", "int-unsafeNum", "2.251799813685248e+16", "boxed-float"],
    ["float-native", "floordiv", "int-unsafeNum", "0.0", "boxed-float"],
    ["float-native", "mod", "int-unsafeNum", "2.5", "num-float"],
    ["float-native", "truediv", "int-unsafeNum", "2.7755575615628914e-16", "num-float"],
    ["float-native", "add", "int-big", "9007199254740998.0", "boxed-float"],
    ["float-native", "sub", "int-big", "-9007199254740994.0", "boxed-float"],
    ["float-native", "mul", "int-big", "2.251799813685249e+16", "boxed-float"],
    ["float-native", "floordiv", "int-big", "0.0", "boxed-float"],
    ["float-native", "mod", "int-big", "2.5", "num-float"],
    ["float-native", "truediv", "int-big", "2.7755575615628904e-16", "num-float"],
    ["float-native", "add", "float-native", "5.0", "boxed-float"],
    ["float-native", "sub", "float-native", "0.0", "boxed-float"],
    ["float-native", "mul", "float-native", "6.25", "num-float"],
    ["float-native", "floordiv", "float-native", "1.0", "boxed-float"],
    ["float-native", "mod", "float-native", "0.0", "boxed-float"],
    ["float-native", "truediv", "float-native", "1.0", "boxed-float"],
    ["float-native", "add", "float-boxed", "10.5", "num-float"],
    ["float-native", "sub", "float-boxed", "-5.5", "num-float"],
    ["float-native", "mul", "float-boxed", "20.0", "boxed-float"],
    ["float-native", "floordiv", "float-boxed", "0.0", "boxed-float"],
    ["float-native", "mod", "float-boxed", "2.5", "num-float"],
    ["float-native", "truediv", "float-boxed", "0.3125", "num-float"],
    ["float-boxed", "add", "int-small", "11.0", "boxed-float"],
    ["float-boxed", "sub", "int-small", "5.0", "boxed-float"],
    ["float-boxed", "mul", "int-small", "24.0", "boxed-float"],
    ["float-boxed", "floordiv", "int-small", "2.0", "boxed-float"],
    ["float-boxed", "mod", "int-small", "2.0", "boxed-float"],
    ["float-boxed", "truediv", "int-small", "2.6666666666666665", "num-float"],
    ["float-boxed", "add", "int-unsafeNum", "9007199254741000.0", "boxed-float"],
    ["float-boxed", "sub", "int-unsafeNum", "-9007199254740984.0", "boxed-float"],
    ["float-boxed", "mul", "int-unsafeNum", "7.205759403792794e+16", "boxed-float"],
    ["float-boxed", "floordiv", "int-unsafeNum", "0.0", "boxed-float"],
    ["float-boxed", "mod", "int-unsafeNum", "8.0", "boxed-float"],
    ["float-boxed", "truediv", "int-unsafeNum", "8.881784197001252e-16", "num-float"],
    ["float-boxed", "add", "int-big", "9007199254741004.0", "boxed-float"],
    ["float-boxed", "sub", "int-big", "-9007199254740988.0", "boxed-float"],
    ["float-boxed", "mul", "int-big", "7.205759403792797e+16", "boxed-float"],
    ["float-boxed", "floordiv", "int-big", "0.0", "boxed-float"],
    ["float-boxed", "mod", "int-big", "8.0", "boxed-float"],
    ["float-boxed", "truediv", "int-big", "8.881784197001248e-16", "num-float"],
    ["float-boxed", "add", "float-native", "10.5", "num-float"],
    ["float-boxed", "sub", "float-native", "5.5", "num-float"],
    ["float-boxed", "mul", "float-native", "20.0", "boxed-float"],
    ["float-boxed", "floordiv", "float-native", "3.0", "boxed-float"],
    ["float-boxed", "mod", "float-native", "0.5", "num-float"],
    ["float-boxed", "truediv", "float-native", "3.2", "num-float"],
    ["float-boxed", "add", "float-boxed", "16.0", "boxed-float"],
    ["float-boxed", "sub", "float-boxed", "0.0", "boxed-float"],
    ["float-boxed", "mul", "float-boxed", "64.0", "boxed-float"],
    ["float-boxed", "floordiv", "float-boxed", "1.0", "boxed-float"],
    ["float-boxed", "mod", "float-boxed", "0.0", "boxed-float"],
    ["float-boxed", "truediv", "float-boxed", "1.0", "boxed-float"],
    ["int-small", "pow", "int-small", "49", "num-int"],
    ["int-small", "pow", "float-boxed", "49.0", "boxed-float"],
    ["int-unsafeNum", "pow", "int-small", "81129638414606681695789005144064", "bigint"],
    ["int-unsafeNum", "pow", "float-boxed", "8.112963841460668e+31", "boxed-float"],
    ["float-native", "pow", "int-small", "6.25", "num-float"],
    ["float-native", "pow", "float-boxed", "6.25", "num-float"],
    ["float-boxed", "pow", "int-small", "64.0", "boxed-float"],
    ["float-boxed", "pow", "float-boxed", "64.0", "boxed-float"],
];

test("4-form × op matrix: CPython value AND canonical result form", () => {
    const failures = [];
    for (const [an, opName, bn, wantRepr, wantForm] of MATRIX) {
        const a = MAKE.a[an]();
        const b = (opName === "pow" ? MAKE_POW_B : MAKE.b)[bn]();
        let r;
        try {
            r = OPS[opName](a, b);
        } catch (e) {
            failures.push(`${an} ${opName} ${bn}: THREW ${e.name}: ${e.message}`);
            continue;
        }
        const gotRepr = pyRepr(r);
        const gotForm = formOf(r);
        if (gotRepr !== wantRepr) {
            failures.push(`${an} ${opName} ${bn}: repr ${gotRepr} != CPython ${wantRepr}`);
        }
        if (gotForm !== wantForm) {
            failures.push(`${an} ${opName} ${bn}: form ${gotForm} != canonical ${wantForm}`);
        }
    }
    assert.deepEqual(failures, [], `matrix violations:\n${failures.join("\n")}`);
});

test("comparisons are exact across all four forms", () => {
    // CPython-verified: 2**53 < 2**53+1, 2**53 != 2**53+1, boxes by value.
    assert.equal(pyLt(9007199254740992, 9007199254740993n), true);
    assert.equal(pyGt(9007199254740993n, 9007199254740992), true);
    assert.equal(pyEq(9007199254740992, 9007199254740993n), false);
    assert.equal(pyEq(9007199254740992, 9007199254740992n), true);
    assert.equal(pyEq(__pyF(8), 8), true);
    assert.equal(pyEq(__pyF(8), 8n), true);
    assert.equal(pyLt(2.5, 9007199254740993n), true);
});

test("inbound native boundary (#464): unsafe integer Number is an INT", () => {
    const inbound = 9007199254740992; // exactly 2**53, from native JS
    // add_ints(2**53, 1) must be the exact int 2**53+1 — a bigint, never a
    // float box of a rounded value.
    const r = pyAdd(inbound, 1);
    assert.equal(typeof r, "bigint");
    assert.equal(r, 9007199254740993n);
    // repr/str of the raw inbound value: int digits, no ".0", no exponent.
    assert.equal(pyRepr(inbound), "9007199254740992");
    assert.equal(pyStr(inbound), "9007199254740992");
    // running sum crossing 2**53 promotes and stays exact (#460).
    let s = 0;
    for (const v of [9007199254740992, 1, 1]) s = pyAdd(s, v);
    assert.equal(s, 9007199254740994n);
    // unary ops keep int-ness at every magnitude.
    assert.equal(pyNeg(9007199254740993n), -9007199254740993n);
    assert.equal(pyPos(9007199254740993n), 9007199254740993n);
    assert.equal(pyPos(inbound), inbound);
});

test("int↔float mixing never throws 'Cannot mix BigInt' (#461)", () => {
    const big = 9007199254740993n;
    // CPython: 9007199254740993 * 2.0 == 1.8014398509481984e+16 (the float
    // 2.0 is integer-valued, so it arrives in its boxed form).
    assert.equal(pyRepr(pyMul(big, __pyF(2))), "1.8014398509481984e+16");
    assert.equal(pyRepr(pyAdd(big, __pyF(8))), "9007199254741000.0");
    // math-module float sinks coerce, never throw (CPython-verified):
    assert.equal(math.isclose(big, 2.5), false);
    assert.equal(pyRepr(math.fsum([9007199254740995n, 2.5])), "9007199254740998.0");
    // math-module exact-int functions promote instead of rounding/throwing:
    assert.equal(math.gcd(9007199254740994n, 2), 2);
    assert.equal(math.lcm(9007199254740994n, 4), 18014398509481988n);
    assert.equal(math.factorial(25), 15511210043330985984000000n);
    assert.equal(math.comb(60, 30), 118264581564861424n);
    assert.equal(math.perm(30, 15), 202843204931727360000n);
    assert.equal(math.prod([9007199254740992, 4]), 36028797018963968n);
    assert.equal(pyRepr(math.prod([3, __pyF(2)])), "6.0");
    assert.equal(pyRepr(math.prod([3, 2.5])), "7.5");
    assert.equal(math.ceil(9007199254740993n), 9007199254740993n);
    assert.equal(math.floor(9007199254740993n), 9007199254740993n);
    assert.equal(math.trunc(9007199254740993n), 9007199254740993n);
});

test("native-JS sink and conversion authorities", () => {
    // __pyJs: THE unbox authority — box → primitive, everything else id.
    assert.equal(typeof __pyJs(__pyF(8)), "number");
    assert.equal(__pyJs(__pyF(8)), 8);
    assert.equal(__pyJs(9007199254740993n), 9007199254740993n);
    // __reqNum: THE int→float coercion — exact-double for BigInt in range.
    assert.equal(__reqNum(9007199254740993n), 9007199254740992);
    assert.equal(__reqNum(__pyF(8)), 8);
    assert.throws(() => __reqNum(10n ** 400n), (e) => e.name === "OverflowError");
    // pyInt: canonical int form out (Number iff safe, else exact BigInt).
    assert.equal(pyInt(1e300), BigInt(1e300));
    assert.equal(typeof pyInt(3.9), "number");
    assert.equal(pyInt(3.9), 3);
});
