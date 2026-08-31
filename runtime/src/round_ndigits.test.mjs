// ═══ F6 (v0.2.4) — round(x, ndigits) EXACT DECIMAL ROUNDING MATRIX ═══
//
// CPython's 2-arg round (double_round → _Py_dg_dtoa mode 3) rounds the
// DECIMAL value of the ORIGINAL double: half-even at the target digit of x's
// EXACT decimal expansion, then nearest-double back. The old scale-multiply
// (`x * 10^nd` then half-even on the product) re-rounds in BINARY first, so
// any value whose product crosses (or lands on) .5 differently diverged
// silently: round(0.05, 1) → 0.0 (CPython 0.1), round(0.015, 2) → 0.02
// (CPython 0.01), round(0.005, 2) → 0.0 (CPython 0.01). pyRound now computes
// the exact BigInt quotient of |x|·10^nd (see __pyRoundDecimal in runtime.js).
//
// Golden table: live CPython 3.12 (scratchpad round_oracle/round_sweep,
// 2026-08-26 — the sweep checked 146,966 (x, nd) pairs incl. 60k random
// bit-pattern doubles with nd ∈ [-320, 320]: 0 mismatches for this
// algorithm, hundreds for scale-multiply).

import { test } from "node:test";
import assert from "node:assert/strict";
import { pyRound, OverflowError } from "./runtime.js";
import { __pyF, pyRepr } from "./operators.js";

// [input (native number | __pyF-boxed), ndigits (null = 1-arg), CPython repr]
const GOLDEN = [
    // the silent-wrong-value class (old scale-multiply diverged on ALL of these)
    [0.05, 1, "0.1"],
    [-0.05, 1, "-0.1"],
    [0.005, 2, "0.01"],
    [-0.005, 2, "-0.01"],
    [0.015, 2, "0.01"],
    [0.025, 2, "0.03"],
    [0.065, 2, "0.07"],
    [0.075, 2, "0.07"],
    [0.085, 2, "0.09"],
    [0.15, 1, "0.1"],
    // classic representation cases — CPython rounds the STORED value
    [2.675, 2, "2.67"],
    [-2.675, 2, "-2.67"],
    [2.665, 2, "2.67"],
    [0.145, 2, "0.14"],
    [0.35, 1, "0.3"],
    [1.005, 2, "1.0"],
    [2.135, 2, "2.13"],
    [9.999, 2, "10.0"],
    [-9.999, 2, "-10.0"],
    [99.99999999999999, 13, "100.0"],
    // exact-tie half-even at the digit (expansion terminates) — PRESERVED
    [0.125, 2, "0.12"],
    [0.375, 2, "0.38"],
    [__pyF(0.5 * 2), null, "1"], // guard the boxed unwrap on the 1-arg path
    // 1-arg ties (half-even) — PRESERVED
    [2.5, null, "2"],
    [1.5, null, "2"],
    [0.5, null, "0"],
    [-2.5, null, "-2"],
    // ndigits <= 0 — floats
    [1234.5678, -2, "1200.0"],
    [__pyF(150), -2, "200.0"],
    [__pyF(250), -2, "200.0"],
    [__pyF(-150), -2, "-200.0"],
    [__pyF(12345), -2, "12300.0"],
    [0.5, 0, "0.0"],
    [1.5, 0, "2.0"],
    [2.5, 0, "2.0"],
    [1e15 + 0.5, 0, "1000000000000000.0"],
    // int x passes through (returns int)
    [150, -2, "200"],
    [2, 1, "2"],
    // extreme ndigits / subnormals / signed zero
    [0.1, 30, "0.1"],
    [__pyF(1), 5, "1.0"],
    [5e-324, 324, "5e-324"],
    [5e-324, 2, "0.0"],
    [2.675, 100, "2.675"],
    [-0.001, 1, "-0.0"],
    [-0.001, 400, "-0.001"],
    [2.675, 2000000000, "2.675"], // nd cap: no BigInt blowup
    [1234.5678, -2000000000, "0.0"],
];

test("F6 matrix: round(x, ndigits) matches CPython 3.12 reprs", () => {
    for (const [x, nd, want] of GOLDEN) {
        const got = nd === null ? pyRound(x) : pyRound(x, nd);
        assert.equal(pyRepr(got), want,
            `round(${pyRepr(x)}, ${nd}) → ${pyRepr(got)}, CPython ${want}`);
    }
});

test("F6: 2-arg float form still returns a FLOAT (boxed when integer-valued)", () => {
    const r = pyRound(9.999, 2); // 10.0
    assert.equal(r.__pyfloat__, true);
    assert.equal(pyRepr(r), "10.0");
    const r2 = pyRound(__pyF(3), 1); // round(3.0, 1) → 3.0 stays float
    assert.equal(r2.__pyfloat__, true);
    assert.equal(pyRepr(r2), "3.0");
    // 1-arg returns an int (no box)
    assert.equal(typeof pyRound(2.5), "number");
});

test("F6: overflow of the ROUNDED value raises OverflowError (CPython)", () => {
    assert.throws(() => pyRound(1.7e308, -308),
        (e) => e instanceof OverflowError
            && e.message === "rounded value too large to represent");
    // non-finite INPUT still passes through the 2-arg form (pre-existing)
    assert.equal(pyRepr(pyRound(Infinity, 2)), "inf");
    assert.equal(pyRepr(pyRound(NaN, 2)), "nan");
});

test("F6: exhaustive 3-decimal grid vs exact half-even oracle", () => {
    // The same oracle the CPython sweep validated: exact BigInt decimal
    // rounding of the stored double. Grid: x = a/1000 for a in [0, 4000),
    // nd in {1, 2} — covers every .5-boundary shape in two digit positions.
    const exact = (x, nd) => {
        // reference implementation independent of the runtime's own helper
        const dv = new DataView(new ArrayBuffer(8));
        dv.setFloat64(0, Math.abs(x));
        const hi = dv.getUint32(0), lo = dv.getUint32(4);
        const be = (hi >>> 20) & 0x7ff;
        let mant = (BigInt(hi & 0xfffff) << 32n) | BigInt(lo);
        let e = be === 0 ? -1074 : (mant |= 1n << 52n, be - 1075);
        const num = mant * 10n ** BigInt(nd) * (e > 0 ? 1n << BigInt(e) : 1n);
        const den = e < 0 ? 1n << BigInt(-e) : 1n;
        let q = num / den;
        const tw = (num % den) * 2n;
        if (tw > den || (tw === den && (q & 1n) === 1n)) q += 1n;
        const res = Number(`${q}e${-nd}`);
        return x < 0 ? -res : res;
    };
    for (let a = 0; a < 4000; a++) {
        for (const nd of [1, 2]) {
            const x = a / 1000;
            const got = pyRound(x, nd);
            const v = got != null && got.__pyfloat__ === true ? got.valueOf() : got;
            assert.equal(v, exact(x, nd), `round(${x}, ${nd})`);
        }
    }
});

// ═══ F6-r2 (v0.2.4): ndigits TYPE validation + huge-negative saturation ═══
//
// The r1 wrapper did Math.trunc(Number(ndigits)), silently accepting
// round(1.25, 1.5) and round(1.25, "1") where CPython 3.12 raises
// "'float'/'str' object cannot be interpreted as an integer"; a very
// negative BigInt ndigits on an int receiver hit V8's BigInt size cap
// (10n ** k → RangeError) where CPython returns 0. ndigits now validates
// through the __index__ protocol (__pyAsIndexInt). CPython 3.12.7 goldens.
test("F6-r2: ndigits rejects non-__index__ types like CPython", () => {
    const raisesTE = (f, msg) => assert.throws(f,
        (e) => e.name === "TypeError" && e.message === msg,
        `expected TypeError: ${msg}`);
    raisesTE(() => pyRound(1.25, 1.5),
        "'float' object cannot be interpreted as an integer");
    raisesTE(() => pyRound(1.25, __pyF(1.0)), // boxed 1.0 — Python float
        "'float' object cannot be interpreted as an integer");
    raisesTE(() => pyRound(1.25, "1"),
        "'str' object cannot be interpreted as an integer");
    raisesTE(() => pyRound(1.25, [1]),
        "'list' object cannot be interpreted as an integer");
    raisesTE(() => pyRound(2, "1"),  // int receiver validates too
        "'str' object cannot be interpreted as an integer");
    raisesTE(() => pyRound(2n ** 60n, 1.5), // bigint receiver validates too
        "'float' object cannot be interpreted as an integer");
    raisesTE(() => pyRound(1.25, { __index__: () => "x" }),
        "__index__ returned non-int (type str)");
});

test("F6-r2: ndigits via __index__/bool accepted; valid forms preserved", () => {
    const V = (r) => (r != null && r.__pyfloat__ === true ? r.valueOf() : r);
    assert.equal(V(pyRound(1.567, { __index__: () => 2 })), 1.57);
    assert.equal(V(pyRound(1.25, true)), 1.2);   // ndigits=True → 1, half-even
    assert.equal(V(pyRound(1.25, 1)), 1.2);
    assert.equal(pyRound(150, -2), 200);
    assert.equal(pyRound(2.5), 2);
});

test("F6-r2: very-negative ndigits saturates to 0 like CPython (no RangeError)", () => {
    const V = (r) => (r != null && r.__pyfloat__ === true ? r.valueOf() : r);
    // int receiver, huge-negative BigInt ndigits: CPython round(123, -10**5)
    // is int 0 (the old path threw RangeError inside 10n ** k)
    assert.equal(pyRound(2n ** 60n, -(10n ** 5n)), 0);
    assert.equal(pyRound(2n ** 60n, -(10n ** 100n)), 0);
    assert.equal(pyRound(123, -100000), 0);
    // float receiver: signed zero, boxed float (CPython -0.0 / 0.0)
    assert.equal(pyRepr(pyRound(1.5, -400)), "0.0");
    assert.equal(pyRepr(pyRound(-1.5, -400)), "-0.0");
    assert.equal(pyRepr(pyRound(1.5, -(10n ** 100n))), "0.0");
    // huge POSITIVE ndigits: value unchanged
    assert.equal(V(pyRound(2.675, 10 ** 6)), 2.675);
    assert.equal(pyRound(2n ** 60n, 10n ** 100n), 2n ** 60n);
});
