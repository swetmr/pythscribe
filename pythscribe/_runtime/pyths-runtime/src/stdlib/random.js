// PythScribe standard library: random module
//
// FULL_SURFACE #4: every module-level function draws from ONE shared
// default `Random` instance (below), so `random.seed(n)` gives
// reproducible sequences — the same architecture as CPython, where the
// module functions are bound methods of a hidden `Random()` instance.
//
// BY-DESIGN DEVIATION (documented in docs/known-limitations.md): the
// generator is mulberry32, not CPython's Mersenne Twister — a given seed
// is deterministic WITHIN PythScribe but the values do not match
// CPython's for the same seed (like the <=4-ULP transcendental class of
// deviation). Unseeded use remains nondeterministic, as in CPython.

// Option B: CPython random.* float-returning functions return FLOATS —
// an integer-valued draw (uniform(0, 2) landing on exactly 2.0, gauss
// hitting a whole value) must keep the float tag, so results route
// through the runtime's single box authority __pyF (boxes iff
// integer-valued; the overwhelmingly common non-integer draw stays a
// native Number and pays only the isInteger check).
// __toBig/__norm are the operators.js SINGLE numeric-boundary authority
// (#38/#460/#461): __toBig coerces to BigInt, __norm demotes back to Number
// iff the value fits the safe range. This module NEVER re-decides numeric
// representation locally — it only branches on `typeof` and delegates
// classification/promotion to these helpers.
import { __pyF, __toBig, __norm } from "../operators.js";

export function random() {
    return __pyF(__defaultRandom._next());
}

export function seed(s) {
    __defaultRandom.seed(s);
}

export function randint(a, b) {
    return __defaultRandom.randint(a, b);
}

export function randrange(start, stop, step) {
    return __defaultRandom.randrange(start, stop, step);
}

export function choice(seq) {
    return __defaultRandom.choice(seq);
}

export function choices(population, { weights, k = 1 } = {}) {
    const result = [];
    if (weights) {
        const cumWeights = [];
        let total = 0;
        // Weights are RELATIVE selection probabilities — Number precision is
        // the right domain for the draw math, and a BigInt weight (a large
        // Python int) must not mix into the float `_next() * total` product
        // (#461 class). choices() returns population ELEMENTS, never the
        // weight sum, so there is no precision obligation on `total`.
        for (const w of weights) { total += Number(w); cumWeights.push(total); }
        for (let i = 0; i < k; i++) {
            const r = __defaultRandom._next() * total;
            for (let j = 0; j < cumWeights.length; j++) {
                if (r < cumWeights[j]) { result.push(population[j]); break; }
            }
        }
    } else {
        for (let i = 0; i < k; i++) {
            result.push(population[Math.floor(__defaultRandom._next() * population.length)]);
        }
    }
    return result;
}

export function shuffle(arr) {
    for (let i = arr.length - 1; i > 0; i--) {
        const j = Math.floor(__defaultRandom._next() * (i + 1));
        [arr[i], arr[j]] = [arr[j], arr[i]];
    }
    return arr;
}

export function sample(population, k) {
    if (k > population.length) throw new Error("Sample larger than population");
    const pool = [...population];
    const result = [];
    for (let i = 0; i < k; i++) {
        const j = Math.floor(__defaultRandom._next() * pool.length);
        result.push(pool.splice(j, 1)[0]);
    }
    return result;
}

export function uniform(a, b) {
    return __defaultRandom.uniform(a, b);
}

export function gauss(mu = 0, sigma = 1) {
    // Float-domain function: coerce ints (incl. BigInt) to Number up front,
    // identity for the already-working Number path.
    mu = Number(mu);
    sigma = Number(sigma);
    // Box-Muller transform
    let u1, u2;
    do { u1 = __defaultRandom._next(); } while (u1 === 0);
    u2 = __defaultRandom._next();
    const z = Math.sqrt(-2.0 * Math.log(u1)) * Math.cos(2.0 * Math.PI * u2);
    return __pyF(mu + z * sigma);
}

export { gauss as normalvariate };

export function expovariate(lambd) {
    return __pyF(-Math.log(1.0 - __defaultRandom._next()) / Number(lambd));
}

export function triangular(low = 0, high = 1, mode) {
    low = Number(low);
    high = Number(high);
    mode = mode === undefined ? (low + high) / 2 : Number(mode);
    const u = __defaultRandom._next();
    const c = (mode - low) / (high - low);
    if (u <= c) {
        return __pyF(low + Math.sqrt(u * (high - low) * (mode - low)));
    } else {
        return __pyF(high - Math.sqrt((1 - u) * (high - low) * (high - mode)));
    }
}

export function betavariate(alpha, beta) {
    const x = gammavariate(alpha, 1);
    const y = gammavariate(beta, 1);
    return __pyF(Number(x) / (Number(x) + Number(y)));
}

export function gammavariate(alpha, beta) {
    alpha = Number(alpha);
    beta = Number(beta);
    // Marsaglia and Tsang's method
    if (alpha < 1) {
        return __pyF(Number(gammavariate(alpha + 1, beta)) * Math.pow(__defaultRandom._next(), 1.0 / alpha));
    }
    const d = alpha - 1.0 / 3.0;
    const c = 1.0 / Math.sqrt(9.0 * d);
    while (true) {
        let x, v;
        do {
            x = gauss();
            v = 1.0 + c * x;
        } while (v <= 0);
        v = v * v * v;
        const u = __defaultRandom._next();
        if (u < 1.0 - 0.0331 * (x * x) * (x * x)) return __pyF(d * v * beta);
        if (Math.log(u) < 0.5 * x * x + d * (1.0 - v + Math.log(v))) return __pyF(d * v * beta);
    }
}

// Python `random.Random(seed)` — an independent, seedable PRNG instance.
// Uses mulberry32 so a given seed is reproducible WITHIN PythScribe (the
// sequence intentionally does not match CPython's Mersenne Twister; code that
// needs an exact CPython stream is out of scope for an edge-target runtime).
export class Random {
    constructor(seed) {
        this.seed(seed);
    }
    seed(s) {
        let n;
        if (s === undefined || s === null) {
            n = (Math.floor(Math.random() * 0x100000000)) >>> 0;
        } else if (typeof s === "bigint") {
            n = Number(s & 0xffffffffn) >>> 0;
        } else if (typeof s === "string") {
            n = 0;
            for (let i = 0; i < s.length; i++) n = (Math.imul(n, 31) + s.charCodeAt(i)) >>> 0;
        } else {
            n = Math.floor(Number(s)) >>> 0;
        }
        // Avoid a zero state (mulberry32 still works, but nudge for variety).
        this._state = (n ^ 0x9e3779b9) >>> 0;
    }
    // RAW PRNG core — internal math stays native (no box), so guards like
    // `u === 0` and index arithmetic never see a Number object.
    _next() {
        this._state = (this._state + 0x6d2b79f5) | 0;
        let t = this._state;
        t = Math.imul(t ^ (t >>> 15), t | 1);
        t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    }
    // Option B: the Python-facing instance methods return FLOATS — a
    // whole-valued draw (uniform(5, 5) -> 5.0) keeps the float tag, same
    // rule as the module-level functions.
    random() {
        return __pyF(this._next());
    }
    // Integer-domain numeric authority (the #461-class closure for this
    // module): a uniform draw in [0, n). EVERY integer-draw method routes
    // through here — no other site in this module may mix `_next()` floats
    // with possibly-BigInt integer arguments (that mix is exactly the
    // "Cannot mix BigInt and other types" class).
    //
    //   * Number path (`typeof n === "number"` — callers only pass a Number
    //     here when the authority's __norm classified it as fitting): the
    //     ORIGINAL algorithm `Math.floor(this._next() * n)` — bit-for-bit
    //     the pre-fix values, so seeded sequences and existing tests are
    //     unchanged.
    //   * BigInt path: CPython's `_randbelow` structure — draw ceil(k/32)
    //     32-bit words from `_next()`, keep exactly k = bitLength(n) bits,
    //     and rejection-sample until the value is < n (unbiased). Returns a
    //     BigInt.
    //
    // NOTE: branching here is `typeof`-only; the safe-vs-big REPRESENTATION
    // decision belongs to operators.js (__norm) — never re-derived locally.
    _randbelow(n) {
        if (typeof n === "number") return Math.floor(this._next() * n);
        if (n <= 0n) return 0n;
        const k = n.toString(2).length; // bit length of n
        const words = Math.ceil(k / 32);
        const drop = BigInt(words * 32 - k);
        for (;;) {
            let r = 0n;
            for (let i = 0; i < words; i++) {
                r = (r << 32n) | BigInt((this._next() * 0x100000000) >>> 0);
            }
            r >>= drop;
            if (r < n) return r;
        }
    }
    randint(a, b) {
        if (typeof a === "bigint" || typeof b === "bigint") {
            // Any BigInt endpoint promotes the draw; __norm hands back the
            // canonical representation (BigInt iff outside the safe range).
            const lo = __toBig(a);
            const hi = __toBig(b);
            return __norm(lo + this._randbelow(hi - lo + 1n));
        }
        // Both endpoints are plain Numbers (safe integers by the Option-B
        // invariant). Let the AUTHORITY classify the span: __norm returns a
        // Number iff the span fits, so `typeof nn` makes the decision —
        // and the Number branch is the unchanged original algorithm.
        // A span > 2**53 (extreme, e.g. randint(-9e15, 9e15)) deliberately
        // routes through the BigInt algorithm — exact, unbiased, correct.
        const nn = __norm(__toBig(b - a + 1));
        if (typeof nn === "number") return a + this._randbelow(nn);
        return __norm(__toBig(a) + this._randbelow(nn));
    }
    randrange(start, stop, step) {
        if (stop === undefined) { stop = start; start = 0; }
        if (step === undefined) step = 1;
        if (typeof start === "bigint" || typeof stop === "bigint" || typeof step === "bigint") {
            const lo = __toBig(start);
            const hi = __toBig(stop);
            const st = __toBig(step);
            // ceil((hi - lo) / st) in exact integer arithmetic, valid for
            // either step sign (CPython's range-length formula).
            const width = hi - lo;
            const n = st > 0n ? (width + st - 1n) / st : (width + st + 1n) / st;
            return __norm(lo + this._randbelow(n) * st);
        }
        // Plain-Number path: same authority discipline as randint — __norm
        // classifies the count; the Number branch is the original algorithm.
        const n = Math.ceil((stop - start) / step);
        const nn = __norm(__toBig(n));
        if (typeof nn === "number") return start + this._randbelow(nn) * step;
        return __norm(__toBig(start) + this._randbelow(nn) * __toBig(step));
    }
    uniform(a, b) {
        // Python's uniform accepts ints (incl. arbitrarily large) but
        // returns a FLOAT — coerce to Number first (identity for the
        // already-working Number path; large-int precision loss is inherent
        // to returning a float, matching Python).
        const af = Number(a);
        const bf = Number(b);
        return __pyF(af + this._next() * (bf - af));
    }
    choice(seq) {
        if (seq.length === 0) throw new Error("Cannot choose from an empty sequence");
        return seq[Math.floor(this._next() * seq.length)];
    }
    shuffle(arr) {
        for (let i = arr.length - 1; i > 0; i--) {
            const j = Math.floor(this._next() * (i + 1));
            [arr[i], arr[j]] = [arr[j], arr[i]];
        }
        return arr;
    }
    sample(population, k) {
        if (k > population.length) throw new Error("Sample larger than population");
        const pool = [...population];
        const result = [];
        for (let i = 0; i < k; i++) {
            const j = Math.floor(this._next() * pool.length);
            result.push(pool.splice(j, 1)[0]);
        }
        return result;
    }
}


// The module-level shared instance behind random()/seed()/randint()/...
// Constructed unseeded (entropy from Math.random), exactly like CPython's
// hidden module-level Random() instance.
const __defaultRandom = new Random();

//# sourceMappingURL=random.js.map
