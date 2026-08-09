// PythScribe standard library: math module
// Maps Python math functions to JavaScript Math equivalents

export const pi = Math.PI;
export const e = Math.E;
export const tau = Math.PI * 2;
export const inf = Infinity;
export const nan = NaN;

// CPython's math functions accept ints (any magnitude) and return floats;
// PythScribe represents large ints as BigInt, which JS Math.* rejects
// ("Cannot convert a BigInt value to a number"). Coerce BigInt → Number at
// the boundary so e.g. `math.sqrt(2 * math.pow(10, 19))` works (Mbpp/739 —
// the int-valued float from math.pow gets BigInt-promoted by pyMul).
const __n = (x) => (typeof x === "bigint" ? Number(x) : x);

export function ceil(x) { return Math.ceil(__n(x)); }
export function floor(x) { return Math.floor(__n(x)); }
export function trunc(x) { return Math.trunc(__n(x)); }
export function sqrt(x) { return Math.sqrt(__n(x)); }
export function pow(x, y) { return Math.pow(__n(x), __n(y)); }
export function exp(x) { return Math.exp(__n(x)); }
export function log(x, base) {
    if (base === undefined) return Math.log(__n(x));
    return Math.log(__n(x)) / Math.log(__n(base));
}
export function log2(x) { return Math.log2(__n(x)); }
export function log10(x) { return Math.log10(__n(x)); }
export function sin(x) { return Math.sin(__n(x)); }
export function cos(x) { return Math.cos(__n(x)); }
export function tan(x) { return Math.tan(__n(x)); }
export function asin(x) { return Math.asin(__n(x)); }
export function acos(x) { return Math.acos(__n(x)); }
export function atan(x) { return Math.atan(__n(x)); }
export function atan2(y, x) { return Math.atan2(__n(y), __n(x)); }
export function sinh(x) { return Math.sinh(__n(x)); }
export function cosh(x) { return Math.cosh(__n(x)); }
export function tanh(x) { return Math.tanh(__n(x)); }
export function degrees(x) { return __n(x) * (180 / Math.PI); }
export function radians(x) { return __n(x) * (Math.PI / 180); }
export function abs(x) { return Math.abs(__n(x)); }
export function fabs(x) { return Math.abs(__n(x)); }

export function factorial(n) {
    if (n < 0) throw new Error("factorial() not defined for negative values");
    if (n === 0 || n === 1) return 1;
    let result = 1;
    for (let i = 2; i <= n; i++) result *= i;
    return result;
}

export function gcd(a, b) {
    a = Math.abs(a); b = Math.abs(b);
    while (b) { [a, b] = [b, a % b]; }
    return a;
}

export function lcm(a, b) {
    return Math.abs(a * b) / gcd(a, b);
}

export function isclose(a, b, { rel_tol = 1e-9, abs_tol = 0.0 } = {}) {
    return Math.abs(a - b) <= Math.max(rel_tol * Math.max(Math.abs(a), Math.abs(b)), abs_tol);
}

export function isfinite(x) { return Number.isFinite(__n(x)); }
export function isinf(x) { const n = __n(x); return !Number.isFinite(n) && !Number.isNaN(n); }
export function isnan(x) { return Number.isNaN(__n(x)); }

export function copysign(x, y) {
    return Math.abs(__n(x)) * Math.sign(__n(y));
}

export function fmod(x, y) { return __n(x) % __n(y); }

export function hypot(...args) { return Math.hypot(...args.map(__n)); }

export function comb(n, k) {
    if (k < 0 || k > n) return 0;
    if (k === 0 || k === n) return 1;
    k = Math.min(k, n - k);
    let result = 1;
    for (let i = 0; i < k; i++) {
        result = result * (n - i) / (i + 1);
    }
    return Math.round(result);
}

export function perm(n, k) {
    if (k === undefined) k = n;
    if (k < 0 || k > n) return 0;
    let result = 1;
    for (let i = n; i > n - k; i--) result *= i;
    return result;
}

export function prod(iterable, { start = 1 } = {}) {
    let result = start;
    for (const x of iterable) result *= x;
    return result;
}

export function fsum(iterable) {
    let sum = 0;
    for (const x of iterable) sum += x;
    return sum;
}
