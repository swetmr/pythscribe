// PythScribe standard library: fractions module
//
// fractions.Fraction — exact rational arithmetic (BigInt numerator /
// denominator, normalized: gcd-reduced, sign carried on the numerator,
// denominator always > 0). A faithful subset of CPython's
// `fractions.Fraction`: construction from int, Fraction, str ("3/10",
// "0.5", "7"), and float (EXACT binary expansion of the double's bits —
// matches `float.as_integer_ratio()`); `+ - * /` and `**` with an
// integer exponent; comparisons (`== < <= > >=`, including against
// int/float); `numerator`/`denominator` properties; `str()`/`repr()`;
// `float()` conversion via `valueOf()`.
//
// Mixing with `float` mirrors CPython: the result is a plain float
// (Number), not a Fraction — matches `fractions.Fraction.__add__` et al.
// falling back to float arithmetic when the other operand is a float.

import { ZeroDivisionError, ValueError } from "../runtime.js";

function _bigGcd(a, b) {
    if (a < 0n) a = -a;
    if (b < 0n) b = -b;
    while (b) {
        [a, b] = [b, a % b];
    }
    return a;
}

/**
 * Exact `[numerator, denominator]` (BigInt, reduced, denominator a power
 * of two or 1) for a finite JS double — the same decomposition CPython's
 * `float.as_integer_ratio()` produces, derived directly from the IEEE-754
 * bit pattern (sign/exponent/mantissa), not from decimal-string parsing.
 */
function _floatToExactRatio(x) {
    if (!Number.isFinite(x)) {
        throw new ValueError("cannot convert non-finite float to a ratio");
    }
    if (x === 0) return [0n, 1n];
    const buf = new ArrayBuffer(8);
    const view = new DataView(buf);
    view.setFloat64(0, x);
    const hi = view.getUint32(0);
    const lo = view.getUint32(4);
    const negative = (hi >>> 31) === 1;
    const rawExp = (hi >>> 20) & 0x7ff;
    const mantHi = hi & 0xfffff;
    let mantissa = (BigInt(mantHi) << 32n) | BigInt(lo);
    let e;
    if (rawExp === 0) {
        // Subnormal: no implicit leading bit.
        e = -1074;
    } else {
        mantissa |= 1n << 52n; // implicit leading 1 bit
        e = rawExp - 1075;
    }
    let num = negative ? -mantissa : mantissa;
    let den = 1n;
    if (e >= 0) {
        num = num << BigInt(e);
    } else {
        den = 1n << BigInt(-e);
    }
    const g = _bigGcd(num, den) || 1n;
    return [num / g, den / g];
}

// CPython's _RATIONAL_FORMAT, simplified: `[sign]num[/den]` or a decimal
// literal `[sign]int[.frac][e/Eexp]` (at least one digit required).
const _FRACTION_RE = /^([+-]?)(\d+)\/(\d+)$/;
const _DECIMAL_RE = /^([+-]?)(\d*)(?:\.(\d*))?(?:[eE]([+-]?\d+))?$/;

function _fractionFromString(raw) {
    const s = raw.trim();
    let m = _FRACTION_RE.exec(s);
    if (m) {
        const sign = m[1] === "-" ? -1n : 1n;
        const den = BigInt(m[3]);
        if (den === 0n) {
            throw new ZeroDivisionError(`Fraction(${m[2]}, 0)`);
        }
        return [sign * BigInt(m[2]), den];
    }
    m = _DECIMAL_RE.exec(s);
    if (m && (m[2] || m[3])) {
        const sign = m[1] === "-" ? -1n : 1n;
        const intPart = m[2] || "";
        const fracPart = m[3] || "";
        const exp = m[4] ? parseInt(m[4], 10) : 0;
        const digits = intPart + fracPart;
        let num = sign * BigInt(digits === "" ? "0" : digits);
        const scale = exp - fracPart.length;
        let den = 1n;
        if (scale >= 0) num *= 10n ** BigInt(scale);
        else den = 10n ** BigInt(-scale);
        return [num, den];
    }
    throw new ValueError(`Invalid literal for Fraction: '${raw}'`);
}

function _toBigInt(x, label) {
    if (typeof x === "bigint") return x;
    if (typeof x === "number" && Number.isInteger(x)) return BigInt(x);
    throw new TypeError(`Fraction ${label} should be an int, got ${typeof x}`);
}

export class Fraction {
    #n;
    #d;

    constructor(numerator = 0, denominator = undefined) {
        let n, d;
        if (denominator !== undefined) {
            n = _toBigInt(numerator, "numerator");
            d = _toBigInt(denominator, "denominator");
            if (d === 0n) {
                throw new ZeroDivisionError(`Fraction(${numerator}, 0)`);
            }
        } else if (numerator instanceof Fraction) {
            n = numerator.#n;
            d = numerator.#d;
        } else if (typeof numerator === "string") {
            [n, d] = _fractionFromString(numerator);
        } else if (typeof numerator === "bigint") {
            n = numerator;
            d = 1n;
        } else if (typeof numerator === "number") {
            // Always the exact IEEE-754 expansion — for a whole-valued
            // float (e.g. 5.0) this reduces to n/1 anyway, so it's exact
            // for genuine ints represented as JS Number too.
            [n, d] = _floatToExactRatio(numerator);
        } else {
            throw new TypeError("argument should be a string or a Rational instance");
        }
        const g = _bigGcd(n, d) || 1n;
        n /= g;
        d /= g;
        if (d < 0n) {
            n = -n;
            d = -d;
        }
        this.#n = n;
        this.#d = d;
    }

    get numerator() {
        return this.#n;
    }

    get denominator() {
        return this.#d;
    }

    /** Classify + unpack `other` for a binary op: "frac" → [n, d]
     *  bigints; "float" → defer to plain float math; "unsupported" →
     *  the caller should raise TypeError. */
    _pair(other) {
        if (other instanceof Fraction) return [other.#n, other.#d, "frac"];
        if (typeof other === "bigint") return [other, 1n, "frac"];
        if (typeof other === "number") {
            if (Number.isInteger(other)) return [BigInt(other), 1n, "frac"];
            return [null, null, "float"];
        }
        return [null, null, "unsupported"];
    }

    __add__(other) {
        const [n2, d2, kind] = this._pair(other);
        if (kind === "unsupported") throw new TypeError(`unsupported operand type(s) for +: 'Fraction' and '${typeof other}'`);
        if (kind === "float") return Number(this) + other;
        return new Fraction(this.#n * d2 + n2 * this.#d, this.#d * d2);
    }

    __radd__(other) {
        return this.__add__(other);
    }

    __sub__(other) {
        const [n2, d2, kind] = this._pair(other);
        if (kind === "unsupported") throw new TypeError(`unsupported operand type(s) for -: 'Fraction' and '${typeof other}'`);
        if (kind === "float") return Number(this) - other;
        return new Fraction(this.#n * d2 - n2 * this.#d, this.#d * d2);
    }

    __rsub__(other) {
        const [n2, d2, kind] = this._pair(other);
        if (kind === "unsupported") throw new TypeError(`unsupported operand type(s) for -: '${typeof other}' and 'Fraction'`);
        if (kind === "float") return other - Number(this);
        return new Fraction(n2 * this.#d - this.#n * d2, this.#d * d2);
    }

    __mul__(other) {
        const [n2, d2, kind] = this._pair(other);
        if (kind === "unsupported") throw new TypeError(`unsupported operand type(s) for *: 'Fraction' and '${typeof other}'`);
        if (kind === "float") return Number(this) * other;
        return new Fraction(this.#n * n2, this.#d * d2);
    }

    __rmul__(other) {
        return this.__mul__(other);
    }

    __truediv__(other) {
        const [n2, d2, kind] = this._pair(other);
        if (kind === "unsupported") throw new TypeError(`unsupported operand type(s) for /: 'Fraction' and '${typeof other}'`);
        if (kind === "float") return Number(this) / other;
        if (n2 === 0n) throw new ZeroDivisionError("Fraction(" + this.#n + ", 0)");
        return new Fraction(this.#n * d2, this.#d * n2);
    }

    __rtruediv__(other) {
        const [n2, d2, kind] = this._pair(other);
        if (kind === "unsupported") throw new TypeError(`unsupported operand type(s) for /: '${typeof other}' and 'Fraction'`);
        if (kind === "float") return other / Number(this);
        if (this.#n === 0n) throw new ZeroDivisionError(`Fraction(${n2}, 0)`);
        return new Fraction(n2 * this.#d, d2 * this.#n);
    }

    /** Only an integer exponent is supported (the documented subset). */
    __pow__(exp) {
        let e;
        if (typeof exp === "bigint") e = exp;
        else if (typeof exp === "number" && Number.isInteger(exp)) e = BigInt(exp);
        else throw new TypeError("Fraction ** only supports integer exponents in this subset");
        if (e >= 0n) {
            return new Fraction(this.#n ** e, this.#d ** e);
        }
        if (this.#n === 0n) throw new ZeroDivisionError("0th power of 0 to a negative power");
        const ne = -e;
        return new Fraction(this.#d ** ne, this.#n ** ne);
    }

    __neg__() {
        return new Fraction(-this.#n, this.#d);
    }

    __abs__() {
        return new Fraction(this.#n < 0n ? -this.#n : this.#n, this.#d);
    }

    __pos__() {
        return this;
    }

    __eq__(other) {
        const [n2, d2, kind] = this._pair(other);
        if (kind === "unsupported") return false;
        if (kind === "float") return Number(this) === other;
        return this.#n === n2 && this.#d === d2;
    }

    __lt__(other) {
        const [n2, d2, kind] = this._pair(other);
        if (kind === "float") return Number(this) < other;
        if (kind === "unsupported") throw new TypeError(`'<' not supported between instances of 'Fraction' and '${typeof other}'`);
        return this.#n * d2 < n2 * this.#d;
    }

    __le__(other) {
        const [n2, d2, kind] = this._pair(other);
        if (kind === "float") return Number(this) <= other;
        if (kind === "unsupported") throw new TypeError(`'<=' not supported between instances of 'Fraction' and '${typeof other}'`);
        return this.#n * d2 <= n2 * this.#d;
    }

    __gt__(other) {
        const [n2, d2, kind] = this._pair(other);
        if (kind === "float") return Number(this) > other;
        if (kind === "unsupported") throw new TypeError(`'>' not supported between instances of 'Fraction' and '${typeof other}'`);
        return this.#n * d2 > n2 * this.#d;
    }

    __ge__(other) {
        const [n2, d2, kind] = this._pair(other);
        if (kind === "float") return Number(this) >= other;
        if (kind === "unsupported") throw new TypeError(`'>=' not supported between instances of 'Fraction' and '${typeof other}'`);
        return this.#n * d2 >= n2 * this.#d;
    }

    /** `float(Fraction(...))` — `float(x)` compiles to bare `Number(x)`,
     *  which invokes `valueOf()` per the standard JS coercion protocol. */
    valueOf() {
        return Number(this.#n) / Number(this.#d);
    }

    toString() {
        return this.#d === 1n ? `${this.#n}` : `${this.#n}/${this.#d}`;
    }

    __str__() {
        return this.toString();
    }

    __repr__() {
        return `Fraction(${this.#n}, ${this.#d})`;
    }
}
