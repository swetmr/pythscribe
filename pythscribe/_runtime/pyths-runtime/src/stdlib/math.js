// PythScribe standard library: math module
// Maps Python math functions to JavaScript Math equivalents

export const pi = Math.PI;
export const e = Math.E;
export const tau = Math.PI * 2;
export const inf = Infinity;
export const nan = NaN;

// CPython's math functions accept ints (any magnitude) and return floats;
// PythScribe represents large ints as BigInt, which JS Math.* rejects
// ("Cannot convert a BigInt value to a number" / "Cannot mix BigInt", #461).
// EVERY numeric boundary here routes through THE value-boundary authority
// in ../operators.js — no local coercion copies (#38 deep fix):
//   * __n (= __reqNum)  — any numeric form → native double, for the
//     float-only Math.* sinks (a BigInt beyond double range raises
//     OverflowError exactly like CPython's int→float conversion);
//   * __pyF             — the single float-box rule for float RESULTS
//     (box iff integer-valued: sqrt(16) -> 4.0 keeps its float tag);
//   * pyMul/pySub/pyFloorDiv/pyAbs (+ __toBig/__norm) — exact hybrid-int
//     arithmetic for the int-valued functions (gcd/lcm/factorial/comb/
//     perm/prod), which must promote past 2**53 instead of rounding or
//     throwing on a BigInt operand.
import {
    __pyF, __reqNum as __n, __toBig, __norm,
    pySub, pyMul, pyFloorDiv, pyAbs,
} from "../operators.js";

// ceil/floor/trunc return INTS in CPython — a BigInt int is already its own
// ceil/floor/trunc (coercing it through Number would round past 2**53).
export function ceil(x) { return typeof x === "bigint" ? x : Math.ceil(__n(x)); }
export function floor(x) { return typeof x === "bigint" ? x : Math.floor(__n(x)); }
export function trunc(x) { return typeof x === "bigint" ? x : Math.trunc(__n(x)); }
export function sqrt(x) { return __pyF(Math.sqrt(__n(x))); }
export function pow(x, y) { return __pyF(Math.pow(__n(x), __n(y))); }
export function exp(x) { return __pyF(Math.exp(__n(x))); }
export function expm1(x) { return __pyF(Math.expm1(__n(x))); }
export function log1p(x) { return __pyF(Math.log1p(__n(x))); }
export function log(x, base) {
    if (base === undefined) return __pyF(Math.log(__n(x)));
    return __pyF(Math.log(__n(x)) / Math.log(__n(base)));
}
export function log2(x) { return __pyF(Math.log2(__n(x))); }
export function log10(x) { return __pyF(Math.log10(__n(x))); }
export function sin(x) { return __pyF(Math.sin(__n(x))); }
export function cos(x) { return __pyF(Math.cos(__n(x))); }
export function tan(x) { return __pyF(Math.tan(__n(x))); }
export function asin(x) { return __pyF(Math.asin(__n(x))); }
export function acos(x) { return __pyF(Math.acos(__n(x))); }
export function atan(x) { return __pyF(Math.atan(__n(x))); }
export function atan2(y, x) { return __pyF(Math.atan2(__n(y), __n(x))); }
export function sinh(x) { return __pyF(Math.sinh(__n(x))); }
export function cosh(x) { return __pyF(Math.cosh(__n(x))); }
export function tanh(x) { return __pyF(Math.tanh(__n(x))); }
export function asinh(x) { return __pyF(Math.asinh(__n(x))); }
export function acosh(x) { return __pyF(Math.acosh(__n(x))); }
export function atanh(x) { return __pyF(Math.atanh(__n(x))); }
export function degrees(x) { return __pyF(__n(x) * (180 / Math.PI)); }
export function radians(x) { return __pyF(__n(x) * (Math.PI / 180)); }
export function abs(x) { return Math.abs(__n(x)); }
export function fabs(x) { return __pyF(Math.abs(__n(x))); }

export function factorial(n) {
    if (n < 0) throw new Error("factorial() not defined for negative values");
    if (n == 0 || n == 1) return 1;
    // Exact hybrid-int product via the authority (CPython factorials are
    // exact arbitrary-precision ints; the old `result *= i` rounded past
    // 2**53).
    let result = 1;
    const count = Number(n);
    for (let i = 2; i <= count; i++) result = pyMul(result, i);
    return result;
}

export function gcd(a, b) {
    // Authority coerce-on-mix: a BigInt operand takes the exact BigInt
    // Euclid (Math.abs / raw `%` on a BigInt throw, #461-class); the
    // all-Number path is unchanged.
    if (typeof a === "bigint" || typeof b === "bigint") {
        let x = __toBig(a), y = __toBig(b);
        if (x < 0n) x = -x;
        if (y < 0n) y = -y;
        while (y) { [x, y] = [y, x % y]; }
        return __norm(x);
    }
    a = Math.abs(a); b = Math.abs(b);
    while (b) { [a, b] = [b, a % b]; }
    return a;
}

export function lcm(a, b) {
    // Exact: |a*b| // gcd via the authority ops (raw `a * b` mixed BigInt
    // with Number and rounded past 2**53). CPython: lcm(0, 0) == 0.
    const g = gcd(a, b);
    if (g == 0) return 0;
    return pyFloorDiv(pyAbs(pyMul(a, b)), g);
}

export function isclose(a, b, { rel_tol = 1e-9, abs_tol = 0.0 } = {}) {
    // Float-only comparison: coerce every form through the authority (a
    // BigInt operand made `a - b` throw "Cannot mix BigInt", #461).
    a = __n(a); b = __n(b);
    return Math.abs(a - b) <= Math.max(rel_tol * Math.max(Math.abs(a), Math.abs(b)), abs_tol);
}

// CPython: an int (any magnitude) is always finite, never inf/nan — answer
// directly for the BigInt form instead of coercing (a 10**400 int must not
// overflow-raise, and must not read as inf).
export function isfinite(x) { return typeof x === "bigint" ? true : Number.isFinite(__n(x)); }
export function isinf(x) { if (typeof x === "bigint") return false; const n = __n(x); return !Number.isFinite(n) && !Number.isNaN(n); }
export function isnan(x) { return typeof x === "bigint" ? false : Number.isNaN(__n(x)); }

export function copysign(x, y) {
    return __pyF(Math.abs(__n(x)) * Math.sign(__n(y)));
}

export function fmod(x, y) { return __pyF(__n(x) % __n(y)); }

export function hypot(...args) { return __pyF(Math.hypot(...args.map(__n))); }

export function comb(n, k) {
    // Exact hybrid-int binomial via the authority: multiply-then-divide in
    // exact int arithmetic (each partial product is divisible), promoting
    // past 2**53 instead of rounding a float accumulator (BigInt n/k also
    // just work now).
    if (k < 0 || k > n) return 0;
    if (k == 0 || k == n) return 1;
    const kk = Number(pySub(n, k) < k ? pySub(n, k) : k);
    let result = 1;
    for (let i = 0; i < kk; i++) {
        result = pyFloorDiv(pyMul(result, pySub(n, i)), i + 1);
    }
    return result;
}

export function perm(n, k) {
    // Exact hybrid-int falling factorial via the authority (same class as
    // comb: the old `result *= i` rounded past 2**53 and threw on BigInt).
    if (k === undefined) k = n;
    if (k < 0 || k > n) return 0;
    const kk = Number(k);
    let result = 1;
    for (let i = 0; i < kk; i++) result = pyMul(result, pySub(n, i));
    return result;
}

export function prod(iterable, { start = 1 } = {}) {
    // CPython: int result for all-int inputs (EXACT, arbitrary precision);
    // float if any float participated. pyMul is the authority for both —
    // it promotes int products past 2**53 and carries the float tag.
    let result = start;
    for (const x of iterable) result = pyMul(result, x);
    return result;
}

export function fsum(iterable) {
    // CPython: fsum ALWAYS returns a float — coerce every element through
    // the authority (a BigInt element made `sum += x` throw, #461).
    let sum = 0;
    for (const x of iterable) sum += __n(x);
    return __pyF(sum);
}

//# sourceMappingURL=math.js.map
