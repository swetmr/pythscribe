// PythScribe standard library: itertools module

import { pyEq } from "../operators.js";
import { pyBool } from "../types.js";

// Non-enumerable tuple marker (local twin of operators.js pyTuple — see
// runtime.js __markTuple). CPython's combinatoric/pairing iterators all
// yield TUPLES; marking lets pyRepr print `(a, b)` instead of `[a, b]`.
function __tup(items) {
    Object.defineProperty(items, "__pytuple__", { value: true, enumerable: false });
    return items;
}

// The codegen's calling convention wraps keyword arguments into a single
// trailing plain-object literal (`product(xs, repeat=2)` →
// `product(xs, {repeat: 2})`). Detect that shape.
function __isKwargs(x, ...keys) {
    return x !== null && typeof x === "object"
        && Object.getPrototypeOf(x) === Object.prototype
        && keys.some((k) => Object.prototype.hasOwnProperty.call(x, k));
}

export function* chain(...iterables) {
    for (const it of iterables) yield* it;
}

export function* chain_from_iterable(iterable) {
    for (const it of iterable) yield* it;
}
// Python spells it `chain.from_iterable` (classmethod on the chain type).
chain.from_iterable = chain_from_iterable;

export function* islice(iterable, ...args) {
    let start = 0, stop, step = 1;
    if (args.length === 1) { stop = args[0]; }
    else if (args.length === 2) { start = args[0]; stop = args[1]; }
    else { start = args[0]; stop = args[1]; step = args[2]; }
    if (stop === null) stop = Infinity; // islice(it, start, None)
    let i = 0;
    for (const item of iterable) {
        if (i >= stop) return;
        if (i >= start && (i - start) % step === 0) yield item;
        i++;
    }
}

export function* cycle(iterable) {
    const saved = [];
    for (const item of iterable) { yield item; saved.push(item); }
    while (saved.length > 0) yield* saved;
}

export function* repeat(value, times) {
    if (__isKwargs(times, "times")) times = times.times;
    if (times === undefined || times === null) { while (true) yield value; }
    else { for (let i = 0; i < times; i++) yield value; }
}

export function* count(start = 0, step = 1) {
    if (__isKwargs(start, "start", "step")) {
        const kw = start;
        start = kw.start ?? 0;
        step = kw.step ?? 1;
    } else if (__isKwargs(step, "step")) {
        step = step.step;
    }
    let n = start;
    while (true) { yield n; n += step; }
}

export function* zip_longest(...iterables) {
    let fillvalue = null; // Python default fillvalue=None
    if (iterables.length > 0 && __isKwargs(iterables[iterables.length - 1], "fillvalue")) {
        fillvalue = iterables[iterables.length - 1].fillvalue;
        iterables = iterables.slice(0, -1);
    }
    const iters = iterables.map(it => it[Symbol.iterator]());
    while (true) {
        const results = iters.map(it => it.next());
        if (results.every(r => r.done)) return;
        yield __tup(results.map(r => r.done ? fillvalue : r.value));
    }
}

export function* accumulate(iterable, func, opts) {
    // Keyword forms: accumulate(xs, initial=...) arrives with the kwargs
    // object in `func` position; accumulate(xs, f, initial=...) in `opts`.
    if (__isKwargs(func, "initial", "func")) { opts = func; func = opts.func; }
    const initial = __isKwargs(opts, "initial") ? opts.initial : undefined;
    let total;
    let started = false;
    if (initial !== undefined) { total = initial; yield total; started = true; }
    for (const item of iterable) {
        if (!started) { total = item; started = true; }
        else { total = func ? func(total, item) : total + item; }
        yield total;
    }
}

export function* takewhile(predicate, iterable) {
    for (const item of iterable) {
        if (!predicate(item)) return;
        yield item;
    }
}

export function* dropwhile(predicate, iterable) {
    let dropping = true;
    for (const item of iterable) {
        if (dropping && predicate(item)) continue;
        dropping = false;
        yield item;
    }
}

// CPython itertools.compress(data, selectors): yield the data elements
// whose corresponding selector is truthy (Python truthiness — stops at the
// shorter of the two).
export function* compress(data, selectors) {
    const sel = selectors[Symbol.iterator]();
    for (const d of data) {
        const s = sel.next();
        if (s.done) return;
        if (pyBool(s.value)) yield d;
    }
}

export function* filterfalse(predicate, iterable) {
    for (const item of iterable) {
        if (!predicate(item)) yield item;
    }
}

export function* starmap(func, iterable) {
    for (const args of iterable) yield func(...args);
}

export function* product(...iterables) {
    let repeatN = 1;
    if (iterables.length > 0 && __isKwargs(iterables[iterables.length - 1], "repeat")) {
        repeatN = iterables[iterables.length - 1].repeat;
        iterables = iterables.slice(0, -1);
    }
    let pools = iterables.map(it => [...it]);
    if (repeatN !== 1) {
        const base = pools;
        pools = [];
        for (let i = 0; i < repeatN; i++) pools.push(...base);
    }
    if (pools.length === 0) { yield __tup([]); return; }
    if (pools.some(pool => pool.length === 0)) return;
    let indices = new Array(pools.length).fill(0);
    yield __tup(pools.map((pool, i) => pool[indices[i]]));
    while (true) {
        let i = pools.length - 1;
        while (i >= 0) {
            indices[i]++;
            if (indices[i] < pools[i].length) break;
            indices[i] = 0;
            i--;
        }
        if (i < 0) return;
        yield __tup(pools.map((pool, j) => pool[indices[j]]));
    }
}

export function* permutations(iterable, r) {
    const pool = [...iterable];
    const n = pool.length;
    r = (r === undefined || r === null) ? n : r;
    if (r > n) return;
    let indices = Array.from({ length: n }, (_, i) => i);
    let cycles = Array.from({ length: r }, (_, i) => n - i);
    yield __tup(indices.slice(0, r).map(i => pool[i]));
    while (true) {
        let found = false;
        for (let i = r - 1; i >= 0; i--) {
            cycles[i]--;
            if (cycles[i] === 0) {
                indices.push(indices.splice(i, 1)[0]);
                cycles[i] = n - i;
            } else {
                const j = indices.length - cycles[i];
                [indices[i], indices[j]] = [indices[j], indices[i]];
                yield __tup(indices.slice(0, r).map(k => pool[k]));
                found = true;
                break;
            }
        }
        if (!found) return;
    }
}

export function* combinations(iterable, r) {
    const pool = [...iterable];
    const n = pool.length;
    if (r > n) return;
    let indices = Array.from({ length: r }, (_, i) => i);
    yield __tup(indices.map(i => pool[i]));
    while (true) {
        let i = r - 1;
        while (i >= 0 && indices[i] === i + n - r) i--;
        if (i < 0) return;
        indices[i]++;
        for (let j = i + 1; j < r; j++) indices[j] = indices[j - 1] + 1;
        yield __tup(indices.map(k => pool[k]));
    }
}

export function* combinations_with_replacement(iterable, r) {
    const pool = [...iterable];
    const n = pool.length;
    if (n === 0 && r > 0) return;
    let indices = new Array(r).fill(0);
    yield __tup(indices.map(i => pool[i]));
    while (true) {
        let i = r - 1;
        while (i >= 0 && indices[i] === n - 1) i--;
        if (i < 0) return;
        const val = indices[i] + 1;
        for (let j = i; j < r; j++) indices[j] = val;
        yield __tup(indices.map(k => pool[k]));
    }
}

export function* groupby(iterable, key) {
    if (__isKwargs(key, "key")) key = key.key;
    key = key || (x => x);
    let currentKey, group = [];
    let first = true;
    for (const item of iterable) {
        const k = key(item);
        // CPython groupby compares consecutive keys by `==` (value equality),
        // so equal-by-value lists/dicts/tuples group together — JS `!==`
        // identity-compares references and split every composite key.
        if (first || !pyEq(k, currentKey)) {
            if (!first) yield __tup([currentKey, group]);
            currentKey = k;
            group = [item];
            first = false;
        } else {
            group.push(item);
        }
    }
    if (!first) yield __tup([currentKey, group]);
}

export function tee(iterable, n = 2) {
    // CPython returns a TUPLE of n independent iterators (not a generator
    // of them — `a, b = tee(xs)` destructures immediately).
    const source = [...iterable];
    return __tup(Array.from({ length: n }, () => source[Symbol.iterator]()));
}

export function* pairwise(iterable) {
    const iter = iterable[Symbol.iterator]();
    let prev = iter.next();
    if (prev.done) return;
    for (const item of { [Symbol.iterator]: () => iter }) {
        yield __tup([prev.value, item]);
        prev = { value: item, done: false };
    }
}

//# sourceMappingURL=itertools.js.map
