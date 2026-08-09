// PythScribe standard library: random module
// Maps Python random functions to JavaScript Math.random equivalents

export function random() {
    return Math.random();
}

export function randint(a, b) {
    return Math.floor(Math.random() * (b - a + 1)) + a;
}

export function randrange(start, stop, step) {
    if (stop === undefined) { stop = start; start = 0; }
    if (step === undefined) step = 1;
    const n = Math.ceil((stop - start) / step);
    return start + Math.floor(Math.random() * n) * step;
}

export function choice(seq) {
    if (seq.length === 0) throw new Error("Cannot choose from an empty sequence");
    return seq[Math.floor(Math.random() * seq.length)];
}

export function choices(population, { weights, k = 1 } = {}) {
    const result = [];
    if (weights) {
        const cumWeights = [];
        let total = 0;
        for (const w of weights) { total += w; cumWeights.push(total); }
        for (let i = 0; i < k; i++) {
            const r = Math.random() * total;
            for (let j = 0; j < cumWeights.length; j++) {
                if (r < cumWeights[j]) { result.push(population[j]); break; }
            }
        }
    } else {
        for (let i = 0; i < k; i++) {
            result.push(population[Math.floor(Math.random() * population.length)]);
        }
    }
    return result;
}

export function shuffle(arr) {
    for (let i = arr.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [arr[i], arr[j]] = [arr[j], arr[i]];
    }
    return arr;
}

export function sample(population, k) {
    if (k > population.length) throw new Error("Sample larger than population");
    const pool = [...population];
    const result = [];
    for (let i = 0; i < k; i++) {
        const j = Math.floor(Math.random() * pool.length);
        result.push(pool.splice(j, 1)[0]);
    }
    return result;
}

export function uniform(a, b) {
    return a + Math.random() * (b - a);
}

export function gauss(mu = 0, sigma = 1) {
    // Box-Muller transform
    let u1, u2;
    do { u1 = Math.random(); } while (u1 === 0);
    u2 = Math.random();
    const z = Math.sqrt(-2.0 * Math.log(u1)) * Math.cos(2.0 * Math.PI * u2);
    return mu + z * sigma;
}

export { gauss as normalvariate };

export function expovariate(lambd) {
    return -Math.log(1.0 - Math.random()) / lambd;
}

export function triangular(low = 0, high = 1, mode) {
    if (mode === undefined) mode = (low + high) / 2;
    const u = Math.random();
    const c = (mode - low) / (high - low);
    if (u <= c) {
        return low + Math.sqrt(u * (high - low) * (mode - low));
    } else {
        return high - Math.sqrt((1 - u) * (high - low) * (high - mode));
    }
}

export function betavariate(alpha, beta) {
    const x = gammavariate(alpha, 1);
    const y = gammavariate(beta, 1);
    return x / (x + y);
}

export function gammavariate(alpha, beta) {
    // Marsaglia and Tsang's method
    if (alpha < 1) {
        return gammavariate(alpha + 1, beta) * Math.pow(Math.random(), 1.0 / alpha);
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
        const u = Math.random();
        if (u < 1.0 - 0.0331 * (x * x) * (x * x)) return d * v * beta;
        if (Math.log(u) < 0.5 * x * x + d * (1.0 - v + Math.log(v))) return d * v * beta;
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
    random() {
        this._state = (this._state + 0x6d2b79f5) | 0;
        let t = this._state;
        t = Math.imul(t ^ (t >>> 15), t | 1);
        t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    }
    randint(a, b) {
        return a + Math.floor(this.random() * (b - a + 1));
    }
    randrange(start, stop, step) {
        if (stop === undefined) { stop = start; start = 0; }
        if (step === undefined) step = 1;
        const n = Math.ceil((stop - start) / step);
        return start + Math.floor(this.random() * n) * step;
    }
    uniform(a, b) {
        return a + this.random() * (b - a);
    }
    choice(seq) {
        if (seq.length === 0) throw new Error("Cannot choose from an empty sequence");
        return seq[Math.floor(this.random() * seq.length)];
    }
    shuffle(arr) {
        for (let i = arr.length - 1; i > 0; i--) {
            const j = Math.floor(this.random() * (i + 1));
            [arr[i], arr[j]] = [arr[j], arr[i]];
        }
        return arr;
    }
    sample(population, k) {
        if (k > population.length) throw new Error("Sample larger than population");
        const pool = [...population];
        const result = [];
        for (let i = 0; i < k; i++) {
            const j = Math.floor(this.random() * pool.length);
            result.push(pool.splice(j, 1)[0]);
        }
        return result;
    }
}
