// PythScribe standard library: cmath module.
//
// Complex math over the PyComplex runtime type (operators.js). Every
// function accepts a complex or a real operand and returns a PyComplex
// (CPython cmath always returns complex), except phase()/polar()'s
// magnitude/angle components, which are floats.

import { PyComplex } from "../operators.js";

export const pi = Math.PI;
export const e = Math.E;
export const tau = Math.PI * 2;
export const inf = Infinity;
export const nan = NaN;

function __c(z) {
    if (z instanceof PyComplex) return z;
    // Cross-copy interop: `pyths run` inlines its own PyComplex for the
    // program; duck-type on the (real, imag) shape.
    if (z !== null && typeof z === "object" && typeof z.real === "number" && typeof z.imag === "number") {
        return new PyComplex(z.real, z.imag);
    }
    if (typeof z === "bigint") return new PyComplex(Number(z), 0);
    if (typeof z === "boolean") return new PyComplex(z ? 1 : 0, 0);
    if (typeof z === "number") return new PyComplex(z, 0);
    // Option B: a boxed (integer-valued) float coerces by value.
    if (z != null && z.__pyfloat__ === true) return new PyComplex(z.valueOf(), 0);
    const err = new Error("must be a number");
    err.name = "TypeError";
    throw err;
}

export function phase(z) {
    z = __c(z);
    return Math.atan2(z.imag, z.real);
}

export function polar(z) {
    z = __c(z);
    const out = [Math.hypot(z.real, z.imag), Math.atan2(z.imag, z.real)];
    Object.defineProperty(out, "__pytuple__", { value: true, enumerable: false });
    return out;
}

export function rect(r, phi) {
    return new PyComplex(r * Math.cos(phi), r * Math.sin(phi));
}

export function exp(z) {
    z = __c(z);
    const m = Math.exp(z.real);
    return new PyComplex(m * Math.cos(z.imag), m * Math.sin(z.imag));
}

export function log(z, base) {
    z = __c(z);
    const ln = new PyComplex(Math.log(Math.hypot(z.real, z.imag)), Math.atan2(z.imag, z.real));
    if (base === undefined) return ln;
    const lb = log(base);
    return __div(ln, lb);
}

export function log10(z) {
    return log(z, 10);
}

export function sqrt(z) {
    z = __c(z);
    const r = Math.hypot(z.real, z.imag);
    const theta = Math.atan2(z.imag, z.real);
    const m = Math.sqrt(r);
    return new PyComplex(m * Math.cos(theta / 2), m * Math.sin(theta / 2));
}

function __mul(a, b) {
    return new PyComplex(a.real * b.real - a.imag * b.imag, a.real * b.imag + a.imag * b.real);
}
function __div(a, b) {
    const d = b.real * b.real + b.imag * b.imag;
    return new PyComplex((a.real * b.real + a.imag * b.imag) / d, (a.imag * b.real - a.real * b.imag) / d);
}
function __add(a, b) { return new PyComplex(a.real + b.real, a.imag + b.imag); }
function __sub(a, b) { return new PyComplex(a.real - b.real, a.imag - b.imag); }
const __I = () => new PyComplex(0, 1);
const __ONE = () => new PyComplex(1, 0);

export function sin(z) {
    z = __c(z);
    return new PyComplex(Math.sin(z.real) * Math.cosh(z.imag), Math.cos(z.real) * Math.sinh(z.imag));
}
export function cos(z) {
    z = __c(z);
    return new PyComplex(Math.cos(z.real) * Math.cosh(z.imag), -Math.sin(z.real) * Math.sinh(z.imag));
}
export function tan(z) {
    return __div(sin(z), cos(z));
}
export function sinh(z) {
    z = __c(z);
    return new PyComplex(Math.sinh(z.real) * Math.cos(z.imag), Math.cosh(z.real) * Math.sin(z.imag));
}
export function cosh(z) {
    z = __c(z);
    return new PyComplex(Math.cosh(z.real) * Math.cos(z.imag), Math.sinh(z.real) * Math.sin(z.imag));
}
export function tanh(z) {
    return __div(sinh(z), cosh(z));
}

// asin(z) = -i·ln(iz + sqrt(1 − z²))
export function asin(z) {
    z = __c(z);
    const iz = __mul(__I(), z);
    const root = sqrt(__sub(__ONE(), __mul(z, z)));
    const ln = log(__add(iz, root));
    return __mul(new PyComplex(0, -1), ln);
}
// acos(z) = -i·ln(z + i·sqrt(1 − z²))
export function acos(z) {
    z = __c(z);
    const root = sqrt(__sub(__ONE(), __mul(z, z)));
    const ln = log(__add(z, __mul(__I(), root)));
    return __mul(new PyComplex(0, -1), ln);
}
// atan(z) = (i/2)·ln((i + z)/(i − z))
export function atan(z) {
    z = __c(z);
    const ln = log(__div(__add(__I(), z), __sub(__I(), z)));
    return __mul(new PyComplex(0, 0.5), ln);
}
// asinh(z) = ln(z + sqrt(z² + 1))
export function asinh(z) {
    z = __c(z);
    return log(__add(z, sqrt(__add(__mul(z, z), __ONE()))));
}
// acosh(z) = ln(z + sqrt(z − 1)·sqrt(z + 1))
export function acosh(z) {
    z = __c(z);
    return log(__add(z, __mul(sqrt(__sub(z, __ONE())), sqrt(__add(z, __ONE())))));
}
// atanh(z) = ln((1 + z)/(1 − z)) / 2
export function atanh(z) {
    z = __c(z);
    return __mul(new PyComplex(0.5, 0), log(__div(__add(__ONE(), z), __sub(__ONE(), z))));
}

export function isclose(a, b, kw = {}) {
    a = __c(a); b = __c(b);
    const relTol = kw.rel_tol !== undefined ? kw.rel_tol : 1e-9;
    const absTol = kw.abs_tol !== undefined ? kw.abs_tol : 0.0;
    const d = Math.hypot(a.real - b.real, a.imag - b.imag);
    const ma = Math.hypot(a.real, a.imag);
    const mb = Math.hypot(b.real, b.imag);
    return d <= Math.max(relTol * Math.max(ma, mb), absTol);
}
export function isnan(z) { z = __c(z); return Number.isNaN(z.real) || Number.isNaN(z.imag); }
export function isinf(z) { z = __c(z); return !Number.isFinite(z.real) || !Number.isFinite(z.imag); }
export function isfinite(z) { z = __c(z); return Number.isFinite(z.real) && Number.isFinite(z.imag); }

//# sourceMappingURL=cmath.js.map
