// PythScribe standard library: decimal module
//
// decimal.Decimal — General-Decimal-Arithmetic-style exact decimal
// arithmetic: sign (0/1) + coefficient (BigInt digits, >= 0) + exponent
// (integer). value = (-1)^sign * coefficient * 10^exponent. NOT a float
// wrapper — every operation is exact BigInt math, rounded only at the
// end to the fixed default context (28 significant digits,
// ROUND_HALF_EVEN), matching CPython's `decimal.Decimal` for that
// subset.
//
// Construction: string (preserves exponent/trailing zeros exactly —
// "0.30" keeps its trailing zero, "1E+2" keeps scientific form), int,
// and float via the EXACT IEEE-754 binary expansion (mirrors CPython's
// `Decimal.from_float`: `n, d = f.as_integer_ratio(); k = d.bit_length()
// - 1; coefficient = n * 5**k; exponent = -k` — so `Decimal(0.1)` prints
// the full 55-digit expansion, exactly like CPython).
//
// Out of scope (documented, per the spec's subset boundary): custom
// contexts (`getcontext()`/`setcontext()`), precision/rounding other
// than the fixed 28-digit ROUND_HALF_EVEN default, traps/flags/signals,
// `ln`/`exp`/`sqrt`, `FloatOperation`, thread-local contexts, and
// NaN/Infinity construction — all raise a clear error rather than
// silently producing a wrong value.

import { ZeroDivisionError, ValueError } from "../runtime.js";

const DEFAULT_PREC = 28;

export const ROUND_CEILING = "ROUND_CEILING";
export const ROUND_DOWN = "ROUND_DOWN";
export const ROUND_FLOOR = "ROUND_FLOOR";
export const ROUND_HALF_EVEN = "ROUND_HALF_EVEN";
export const ROUND_HALF_UP = "ROUND_HALF_UP";
export const ROUND_UP = "ROUND_UP";

function _numDigits(coefficient) {
    return coefficient === 0n ? 1 : coefficient.toString().length;
}

function _pyTypeName(x) {
    if (x === null || x === undefined) return "NoneType";
    if (typeof x === "boolean") return "bool";
    if (typeof x === "bigint") return "int";
    if (typeof x === "number") return Number.isInteger(x) ? "int" : "float";
    if (typeof x === "string") return "str";
    return x?.constructor?.name ?? typeof x;
}

/** Decide whether to round the magnitude up given the dropped remainder
 *  (`remainder` out of `divisor`) under rounding mode `mode`. `sign` is
 *  0 (positive) or 1 (negative); `quotient` is the magnitude kept so
 *  far — needed for the HALF_EVEN tie-break. */
function _shouldRoundUp(mode, sign, remainder, divisor, quotient) {
    if (remainder === 0n) return false;
    switch (mode) {
        case ROUND_DOWN:
            return false;
        case ROUND_UP:
            return true;
        case ROUND_CEILING:
            return sign === 0;
        case ROUND_FLOOR:
            return sign === 1;
        case ROUND_HALF_UP:
            return remainder * 2n >= divisor;
        case ROUND_HALF_EVEN:
            if (remainder * 2n > divisor) return true;
            if (remainder * 2n < divisor) return false;
            return (quotient % 2n) === 1n;
        default:
            throw new TypeError(`invalid rounding mode: ${mode}`);
    }
}

/** Round (sign, coefficient, exponent) to at most `prec` significant
 *  digits under `mode`. No-op if already within `prec` digits. */
function _round(sign, coefficient, exponent, prec, mode) {
    const digits = _numDigits(coefficient);
    if (digits <= prec) return { sign, coefficient, exponent };
    const drop = digits - prec;
    const divisor = 10n ** BigInt(drop);
    let quotient = coefficient / divisor;
    const remainder = coefficient % divisor;
    if (_shouldRoundUp(mode, sign, remainder, divisor, quotient)) quotient += 1n;
    let newExponent = exponent + drop;
    if (_numDigits(quotient) > prec) {
        // Carry, e.g. 999...9 (prec digits) rounds up to 1000...0 (prec+1).
        quotient /= 10n;
        newExponent += 1;
    }
    // A round-to-zero result carries no sign (matches CPython _fix).
    if (quotient === 0n) sign = 0;
    return { sign, coefficient: quotient, exponent: newExponent };
}

/** Exact `[mantissaBigInt, exponent]` such that `value === mantissa *
 *  2**exponent`, from the double's IEEE-754 bit pattern. */
function _floatBits(x) {
    const buf = new ArrayBuffer(8);
    const view = new DataView(buf);
    view.setFloat64(0, x);
    const hi = view.getUint32(0);
    const lo = view.getUint32(4);
    const rawExp = (hi >>> 20) & 0x7ff;
    const mantHi = hi & 0xfffff;
    let mantissa = (BigInt(mantHi) << 32n) | BigInt(lo);
    let e;
    if (rawExp === 0) {
        e = -1074;
    } else {
        mantissa |= 1n << 52n;
        e = rawExp - 1075;
    }
    return [mantissa, e];
}

function _bigGcd(a, b) {
    if (a < 0n) a = -a;
    if (b < 0n) b = -b;
    while (b) {
        [a, b] = [b, a % b];
    }
    return a;
}

/** CPython `Decimal.from_float`: n/d = |f|.as_integer_ratio() (d a power
 *  of two); k = d.bit_length() - 1; coefficient = n * 5**k; exponent =
 *  -k. Exact for every finite double, including whole-valued ones. */
function _decimalFromFloat(f) {
    if (!Number.isFinite(f)) {
        throw new ValueError("cannot construct a Decimal from a non-finite float in this subset");
    }
    const sign = f < 0 || Object.is(f, -0) ? 1 : 0;
    const absF = Math.abs(f);
    if (absF === 0) return { sign, coefficient: 0n, exponent: 0 };
    let [mantissa, e] = _floatBits(absF);
    let num = mantissa;
    let den = 1n;
    if (e >= 0) num <<= BigInt(e);
    else den = 1n << BigInt(-e);
    const g = _bigGcd(num, den) || 1n;
    num /= g;
    den /= g;
    // den is a power of two (or 1); k = bit_length(den) - 1.
    let k = 0n;
    let t = den;
    while (t > 1n) {
        t >>= 1n;
        k++;
    }
    const coefficient = num * (5n ** k);
    const exponent = -Number(k);
    return { sign, coefficient, exponent };
}

// [sign][int][.frac][e/Eexp] — at least one digit required.
const _DECIMAL_RE = /^([+-]?)(\d*)(?:\.(\d*))?(?:[eE]([+-]?\d+))?$/;

function _decimalFromString(raw) {
    const s = raw.trim();
    const m = _DECIMAL_RE.exec(s);
    if (!m || (!m[2] && !m[3])) {
        throw new ValueError(`Invalid literal for Decimal: '${raw}'`);
    }
    const sign = m[1] === "-" ? 1 : 0;
    const intPart = m[2] || "";
    const fracPart = m[3] || "";
    const expPart = m[4] ? parseInt(m[4], 10) : 0;
    const digits = intPart + fracPart;
    const coefficient = BigInt(digits === "" ? "0" : digits);
    const exponent = -fracPart.length + expPart;
    return { sign, coefficient, exponent };
}

function _signedScaled(sign, coefficient, exponent, targetExponent) {
    const v = coefficient * (10n ** BigInt(exponent - targetExponent));
    return sign ? -v : v;
}

/** Coerce an operand for arithmetic/comparison: Decimal passes through;
 *  int (Number or BigInt) becomes an exponent-0 Decimal; anything else
 *  (float, str, ...) is unsupported — CPython's Decimal deliberately
 *  does not auto-coerce floats/strings in arithmetic. */
function _coerceOperand(x) {
    if (x instanceof Decimal) return x;
    if (typeof x === "bigint") {
        return Decimal._fromParts(x < 0n ? 1 : 0, x < 0n ? -x : x, 0);
    }
    if (typeof x === "number" && Number.isInteger(x)) {
        const bx = BigInt(x);
        return Decimal._fromParts(bx < 0n ? 1 : 0, bx < 0n ? -bx : bx, 0);
    }
    return null;
}

export class Decimal {
    #sign;
    #coefficient;
    #exponent;

    constructor(value = "0") {
        if (value && typeof value === "object" && value.__pydecimal_raw__) {
            this.#sign = value.sign;
            this.#coefficient = value.coefficient;
            this.#exponent = value.exponent;
            return;
        }
        if (value instanceof Decimal) {
            this.#sign = value.#sign;
            this.#coefficient = value.#coefficient;
            this.#exponent = value.#exponent;
            return;
        }
        let parts;
        if (typeof value === "string") {
            parts = _decimalFromString(value);
        } else if (typeof value === "bigint") {
            parts = { sign: value < 0n ? 1 : 0, coefficient: value < 0n ? -value : value, exponent: 0 };
        } else if (typeof value === "number") {
            // Always the exact IEEE-754 expansion (see module docstring);
            // for a whole-valued float this reduces to the same result
            // as the plain-int path, so no int/float branch is needed.
            parts = _decimalFromFloat(value);
        } else {
            throw new TypeError(`conversion from ${_pyTypeName(value)} to Decimal is not supported`);
        }
        this.#sign = parts.sign;
        this.#coefficient = parts.coefficient;
        this.#exponent = parts.exponent;
    }

    static _fromParts(sign, coefficient, exponent) {
        return new Decimal({ __pydecimal_raw__: true, sign, coefficient, exponent });
    }

    static _compare(a, b) {
        const minExp = Math.min(a.#exponent, b.#exponent);
        const av = _signedScaled(a.#sign, a.#coefficient, a.#exponent, minExp);
        const bv = _signedScaled(b.#sign, b.#coefficient, b.#exponent, minExp);
        if (av < bv) return -1;
        if (av > bv) return 1;
        return 0;
    }

    __add__(other) {
        const b = _coerceOperand(other);
        if (b === null) throw new TypeError(`unsupported operand type(s) for +: 'Decimal' and '${_pyTypeName(other)}'`);
        const minExp = Math.min(this.#exponent, b.#exponent);
        const sum = _signedScaled(this.#sign, this.#coefficient, this.#exponent, minExp)
            + _signedScaled(b.#sign, b.#coefficient, b.#exponent, minExp);
        const sign = sum < 0n ? 1 : 0;
        const mag = sum < 0n ? -sum : sum;
        const r = _round(sign, mag, minExp, DEFAULT_PREC, ROUND_HALF_EVEN);
        return Decimal._fromParts(r.sign, r.coefficient, r.exponent);
    }

    __radd__(other) {
        return this.__add__(other);
    }

    __sub__(other) {
        const b = _coerceOperand(other);
        if (b === null) throw new TypeError(`unsupported operand type(s) for -: 'Decimal' and '${_pyTypeName(other)}'`);
        const minExp = Math.min(this.#exponent, b.#exponent);
        const diff = _signedScaled(this.#sign, this.#coefficient, this.#exponent, minExp)
            - _signedScaled(b.#sign, b.#coefficient, b.#exponent, minExp);
        const sign = diff < 0n ? 1 : 0;
        const mag = diff < 0n ? -diff : diff;
        const r = _round(sign, mag, minExp, DEFAULT_PREC, ROUND_HALF_EVEN);
        return Decimal._fromParts(r.sign, r.coefficient, r.exponent);
    }

    __rsub__(other) {
        const b = _coerceOperand(other);
        if (b === null) throw new TypeError(`unsupported operand type(s) for -: '${_pyTypeName(other)}' and 'Decimal'`);
        return b.__sub__(this);
    }

    __mul__(other) {
        const b = _coerceOperand(other);
        if (b === null) throw new TypeError(`unsupported operand type(s) for *: 'Decimal' and '${_pyTypeName(other)}'`);
        const sign = this.#sign ^ b.#sign;
        const coeff = this.#coefficient * b.#coefficient;
        const exponent = this.#exponent + b.#exponent;
        const r = _round(sign, coeff, exponent, DEFAULT_PREC, ROUND_HALF_EVEN);
        return Decimal._fromParts(r.sign, r.coefficient, r.exponent);
    }

    __rmul__(other) {
        return this.__mul__(other);
    }

    __truediv__(other) {
        const b = _coerceOperand(other);
        if (b === null) throw new TypeError(`unsupported operand type(s) for /: 'Decimal' and '${_pyTypeName(other)}'`);
        if (b.#coefficient === 0n) throw new ZeroDivisionError("division by zero");
        const sign = this.#sign ^ b.#sign;
        if (this.#coefficient === 0n) {
            return Decimal._fromParts(sign, 0n, this.#exponent - b.#exponent);
        }
        const prec = DEFAULT_PREC;
        const c1 = this.#coefficient, c2 = b.#coefficient;
        const e1 = this.#exponent, e2 = b.#exponent;
        // CPython's Decimal.__truediv__ algorithm: scale so the integer
        // quotient has prec+1 digits, then either strip trailing zeros
        // back toward the ideal exponent (exact division) or nudge a
        // false-tie coefficient before the final context-precision round.
        const shift = _numDigits(c2) - _numDigits(c1) + prec + 1;
        let numerator, denominator;
        if (shift >= 0) {
            numerator = c1 * (10n ** BigInt(shift));
            denominator = c2;
        } else {
            numerator = c1;
            denominator = c2 * (10n ** BigInt(-shift));
        }
        let coeff = numerator / denominator;
        const remainder = numerator % denominator;
        let exponent = e1 - e2 - shift;
        if (remainder !== 0n) {
            if (coeff % 5n === 0n) coeff += 1n;
        } else {
            const idealExponent = e1 - e2;
            while (exponent < idealExponent && coeff % 10n === 0n) {
                coeff /= 10n;
                exponent += 1;
            }
        }
        const r = _round(sign, coeff, exponent, prec, ROUND_HALF_EVEN);
        return Decimal._fromParts(r.sign, r.coefficient, r.exponent);
    }

    __rtruediv__(other) {
        const b = _coerceOperand(other);
        if (b === null) throw new TypeError(`unsupported operand type(s) for /: '${_pyTypeName(other)}' and 'Decimal'`);
        return b.__truediv__(this);
    }

    __neg__() {
        if (this.#coefficient === 0n) return Decimal._fromParts(0, 0n, this.#exponent);
        return Decimal._fromParts(this.#sign ? 0 : 1, this.#coefficient, this.#exponent);
    }

    __abs__() {
        return Decimal._fromParts(0, this.#coefficient, this.#exponent);
    }

    __pos__() {
        return this;
    }

    __eq__(other) {
        if (other instanceof Decimal) return Decimal._compare(this, other) === 0;
        const b = _coerceOperand(other);
        if (b === null) return false; // float/str/etc: no auto-coercion, matches CPython
        return Decimal._compare(this, b) === 0;
    }

    __lt__(other) {
        const b = _coerceOperand(other);
        if (b === null) throw new TypeError(`'<' not supported between instances of 'Decimal' and '${_pyTypeName(other)}'`);
        return Decimal._compare(this, b) < 0;
    }

    __le__(other) {
        const b = _coerceOperand(other);
        if (b === null) throw new TypeError(`'<=' not supported between instances of 'Decimal' and '${_pyTypeName(other)}'`);
        return Decimal._compare(this, b) <= 0;
    }

    __gt__(other) {
        const b = _coerceOperand(other);
        if (b === null) throw new TypeError(`'>' not supported between instances of 'Decimal' and '${_pyTypeName(other)}'`);
        return Decimal._compare(this, b) > 0;
    }

    __ge__(other) {
        const b = _coerceOperand(other);
        if (b === null) throw new TypeError(`'>=' not supported between instances of 'Decimal' and '${_pyTypeName(other)}'`);
        return Decimal._compare(this, b) >= 0;
    }

    /** `exp` must be a Decimal (or something Decimal(...)-constructible)
     *  whose exponent is the target exponent — matches CPython's
     *  `Decimal.quantize(exp, rounding=...)`. PythScribe compiles a
     *  keyword argument (`rounding=ROUND_HALF_UP`) to a trailing options
     *  object, not a positional parameter — so the second parameter is
     *  destructured, matching every other kwarg-taking stdlib function
     *  (e.g. `math.isclose`). */
    quantize(exp, { rounding = ROUND_HALF_EVEN } = {}) {
        const target = exp instanceof Decimal ? exp : new Decimal(exp);
        const targetExponent = target.#exponent;
        const scale = this.#exponent - targetExponent;
        let coeff;
        if (scale >= 0) {
            coeff = this.#coefficient * (10n ** BigInt(scale));
        } else {
            const drop = -scale;
            const divisor = 10n ** BigInt(drop);
            let quotient = this.#coefficient / divisor;
            const remainder = this.#coefficient % divisor;
            if (_shouldRoundUp(rounding, this.#sign, remainder, divisor, quotient)) quotient += 1n;
            coeff = quotient;
        }
        if (_numDigits(coeff) > DEFAULT_PREC) {
            throw new TypeError("quantize result has too many digits for the default context");
        }
        const sign = coeff === 0n ? 0 : this.#sign;
        return Decimal._fromParts(sign, coeff, targetExponent);
    }

    /** `float(Decimal(...))` — `float(x)` compiles to bare `Number(x)`,
     *  which invokes `valueOf()`. Building the value through a decimal
     *  string literal (rather than `Number(coeff) * 10**exp`) gets a
     *  correctly-rounded conversion for arbitrarily large coefficients. */
    valueOf() {
        const s = (this.#sign ? "-" : "") + this.#coefficient.toString() + "e" + this.#exponent;
        return Number(s);
    }

    toString() {
        const intStr = this.#coefficient === 0n ? "0" : this.#coefficient.toString();
        const leftdigits = this.#exponent + intStr.length;
        const dotplace = this.#exponent <= 0 && leftdigits > -6 ? leftdigits : 1;
        let intpart, fracpart;
        if (dotplace <= 0) {
            intpart = "0";
            fracpart = "." + "0".repeat(-dotplace) + intStr;
        } else if (dotplace >= intStr.length) {
            intpart = intStr + "0".repeat(dotplace - intStr.length);
            fracpart = "";
        } else {
            intpart = intStr.slice(0, dotplace);
            fracpart = "." + intStr.slice(dotplace);
        }
        let exp = "";
        if (leftdigits !== dotplace) {
            const e = leftdigits - dotplace;
            exp = "E" + (e >= 0 ? "+" : "") + e;
        }
        return (this.#sign ? "-" : "") + intpart + fracpart + exp;
    }

    __str__() {
        return this.toString();
    }

    __repr__() {
        return `Decimal('${this.toString()}')`;
    }
}
