// PythScribe Runtime — Core helpers
// "Write Python. Ship to the Web."

// Round-4 sweep: exception construction needs Python's str()/repr() for
// its message and args-tuple handling. Deliberate import cycle with
// operators.js (which imports our exception classes) — both sides only
// dereference the bindings at call time, never during module evaluation,
// so ESM live bindings resolve it.
import { pyStr, pyRepr, pyTuple, pyEq, pyFormatFloat } from "./operators.js";

/**
 * Python-compatible range() generator.
 * range(stop), range(start, stop), range(start, stop, step)
 */
export function pyRange(startOrStop, stop, step) {
    // Python: bool ⊆ int, so True/False are valid range bounds (1/0).
    const __b = (v) => (typeof v === "boolean" ? (v ? 1 : 0) : v);
    startOrStop = __b(startOrStop); stop = __b(stop); step = __b(step);
    let start;
    if (stop === undefined) {
        start = 0;
        stop = startOrStop;
        step = 1;
    } else {
        start = startOrStop;
        step = step || 1;
    }
    const result = [];
    if (step > 0) {
        for (let i = start; i < stop; i += step) {
            result.push(i);
        }
    } else if (step < 0) {
        for (let i = start; i > stop; i += step) {
            result.push(i);
        }
    }
    return result;
}

/**
 * Python-compatible enumerate() generator.
 *
 * Sweep-A S2 fix: the codegen's calling convention wraps ANY keyword
 * argument into a single trailing options-object literal
 * (`enumerate(xs, start=1)` → `pyEnumerate(xs, {start: 1})`), while a
 * positional call (`enumerate(xs, 1)`) passes a bare number. Accept both
 * shapes rather than assuming `start` is always a plain number.
 */
export function pyEnumerate(iterable, startArg = 0) {
    const start = (startArg && typeof startArg === "object") ? (startArg.start ?? 0) : startArg;
    const result = [];
    let i = start;
    for (const item of iterable) {
        // Round-2 pythonic sweep: rows are TUPLES in Python —
        // repr(list(enumerate("ab"))) must show [(0, 'a'), ...].
        result.push(__markTuple([i, item]));
        i++;
    }
    return result;
}

/** Python-iteration iterator for any value: Maps/PyDicts yield KEYS,
 *  plain objects yield keys, everything else uses its own iterator. */
function __pyElemsIter(it) {
    if (it == null) throw new TypeError_("'NoneType' object is not iterable");
    if (it instanceof Map) return it.keys();
    if (typeof it[Symbol.iterator] === "function") return it[Symbol.iterator]();
    if (typeof it === "object") return Object.keys(it)[Symbol.iterator]();
    throw new TypeError_(`'${typeof it}' object is not iterable`);
}

/**
 * Python-compatible zip() — LAZY one-shot iterator (Pythonic-checks
 * sweep; previously eager, which looped forever on infinite iterators
 * like `zip(count(), "abc")` and allowed re-iteration). Yields
 * pyTuple-marked rows so repr shows `(a, b)`. Supports the 3.10+
 * `strict=True` keyword (arrives as the codegen's trailing
 * `{strict: ...}` options object) with CPython's ValueError messages.
 */
export function* pyZip(...iterables) {
    let strict = false;
    const last = iterables[iterables.length - 1];
    if (last !== null && typeof last === "object"
        && Object.getPrototypeOf(last) === Object.prototype
        && Object.prototype.hasOwnProperty.call(last, "strict")) {
        strict = !!last.strict;
        iterables = iterables.slice(0, -1);
    }
    if (iterables.length === 0) return;
    const iters = iterables.map(__pyElemsIter);
    while (true) {
        const row = [];
        for (let i = 0; i < iters.length; i++) {
            const r = iters[i].next();
            if (r.done) {
                if (strict) {
                    const plural = (k) => k === 1 ? "argument 1" : `arguments 1-${k}`;
                    if (i > 0) {
                        throw new ValueError(`zip() argument ${i + 1} is shorter than ${plural(i)}`);
                    }
                    for (let j = 1; j < iters.length; j++) {
                        if (!iters[j].next().done) {
                            throw new ValueError(`zip() argument ${j + 1} is longer than ${plural(j)}`);
                        }
                    }
                }
                return;
            }
            row.push(r.value);
        }
        yield __markTuple(row);
    }
}

/**
 * Python-compatible map(). Single-iterable form preserves the historical
 * array-returning behavior; the multi-iterable form `map(f, xs, ys)` gives
 * `f` one argument per iterable and stops at the shortest (CPython
 * semantics — the second iterable was previously ignored, feeding `f`
 * `undefined`).
 */
export function pyMap(fn, ...iterables) {
    if (iterables.length <= 1) return [...pyForIter(iterables[0])].map((x) => fn(x));
    const iters = iterables.map((it) => pyForIter(it)[Symbol.iterator]());
    const out = [];
    for (;;) {
        const row = [];
        for (const it of iters) {
            const r = it.next();
            if (r.done) return out;
            row.push(r.value);
        }
        out.push(fn(...row));
    }
}

/**
 * Python-compatible sorted().
 */
export function pySorted(iterable, { key, reverse } = {}) {
    // #275: `sorted(d)` sorts the dict KEYS (pyForIter maps a Map/plain-object
    // dict to its keys; arrays/sets/strings/generators pass through unchanged).
    const arr = [...pyForIter(iterable)];
    // Round-3 pythonic sweep: Python's sort is defined in terms of
    // `<` — instances with __lt__ (and dataclasses with order=True)
    // must sort by it, not by JS default comparison.
    const lt = (a, b) => {
        if (a !== null && typeof a?.__lt__ === "function") return !!a.__lt__(b);
        // #214: tuple/list keys (e.g. `key=lambda x: (-len(x), x)`) compare
        // lexicographically element-by-element, not by JS string coercion.
        if (Array.isArray(a) && Array.isArray(b)) {
            const n = Math.min(a.length, b.length);
            for (let i = 0; i < n; i++) {
                if (lt(a[i], b[i])) return true;
                if (lt(b[i], a[i])) return false;
            }
            return a.length < b.length;
        }
        return a < b;
    };
    const cmp = (a, b) => (lt(a, b) ? -1 : lt(b, a) ? 1 : 0);
    // #247: `reverse=True` must keep Python's STABLE order on ties. A
    // sort-then-`.reverse()` flips equal elements too; negate the comparator
    // instead so ties (cmp 0) stay in input order under JS's stable sort.
    const dir = reverse ? -1 : 1;
    if (key) {
        arr.sort((a, b) => dir * cmp(key(a), key(b)));
    } else {
        arr.sort((a, b) => dir * cmp(a, b));
    }
    return arr;
}

/**
 * Python-compatible reversed().
 */
export function pyReversed(iterable) {
    return [...iterable].reverse();
}

/**
 * Materialize any Python iterable into a real JS Array (identity for
 * arrays). Backs the comprehension fast path (`pySeq(it).map(...)`) for
 * iterables that are not provably arrays at compile time: strings
 * (code-point aware), generators / iterators (whose `.map` in Node 22+
 * is a lazy Iterator Helper, not an Array), Maps / PyDicts (Python
 * iterates KEYS), and plain-object dicts (keys). Pythonic-checks sweep.
 */
export function pySeq(it) {
    if (Array.isArray(it)) return it;
    if (it == null) throw new TypeError_("'NoneType' object is not iterable");
    if (typeof it === "string") return __hasSurrogate(it) ? [...it] : it.split("");
    if (it instanceof Map) return [...it.keys()]; // PyDict.keys() yields original key objects
    if (typeof it[Symbol.iterator] === "function") return [...it];
    if (typeof it === "object") return Object.keys(it); // plain-object dict → keys
    throw new TypeError_(`'${typeof it}' object is not iterable`);
}

/**
 * Python-compatible len().
 */
// F2: Python strings are sequences of Unicode code points, but JS strings
// are UTF-16 code units — an astral char (e.g. "😀") is length 2 in JS but 1
// in Python. `[...s]` iterates code points (surrogate-pair aware). Fast path:
// when no leading surrogate is present the two counts coincide, so plain
// `.length` / index / slice stay correct with zero allocation.
const __hasSurrogate = (s) => /[\uD800-\uDBFF]/.test(s);

// Wave-19 verification fix: Python string OFFSETS count code points; JS
// .indexOf/.lastIndexOf count UTF-16 code units, and the two provably
// diverge once an astral char precedes the match (verification wave 19,
// `smFindSub_ne_js16_astral`). These map between the two index spaces.
// Boundary (documented in the Lean model too): assumes offsets land on
// code-point boundaries — lone-surrogate needles are out of scope.
/** Code-point index → code-unit index (clamped to [0, s.length]). */
function __cpToUnit(s, cp) {
    let u = 0, c = 0;
    while (c < cp && u < s.length) {
        u += s.codePointAt(u) > 0xFFFF ? 2 : 1;
        c++;
    }
    return u;
}
/** Code-unit index (at a boundary) → code-point index. */
function __unitToCp(s, unit) {
    let u = 0, c = 0;
    while (u < unit) {
        u += s.codePointAt(u) > 0xFFFF ? 2 : 1;
        c++;
    }
    return c;
}
// Full CPython str.find/rfind/index semantics in CODE-POINT space.
// last=false → first occurrence (find/index); last=true → last (rfind/rindex).
// Returns a code-point index in [0, len] or -1. Honors start/end as code-point
// offsets (negative + clamped), the empty-needle rule, and lone-surrogate
// needles/haystacks (search runs in code-point space, so a surrogate half never
// matches an astral char). This is the shared engine for the four str methods.
function __pyStrFind(s, sub, start, end, last) {
    if (typeof sub !== "string") throw new TypeError_("must be str");
    // Fast path: no surrogates on either side → UTF-16 units == code points.
    if (!__hasSurrogate(s) && !__hasSurrogate(sub)) {
        const n = s.length;
        let lo = start == null ? 0 : (start < 0 ? Math.max(n + start, 0) : start);
        const hi = end == null ? n : (end < 0 ? Math.max(n + end, 0) : Math.min(end, n));
        if (sub.length === 0) { lo = Math.min(lo, n); return lo <= hi ? (last ? hi : lo) : -1; }
        if (last) {
            const from = hi - sub.length;
            if (from < lo) return -1;
            const i = s.lastIndexOf(sub, from);
            return i >= lo ? i : -1;
        }
        const i = s.indexOf(sub, lo);
        return (i >= 0 && i + sub.length <= hi) ? i : -1;
    }
    // Code-point path (surrogates present).
    const cpS = Array.from(s), cpSub = Array.from(sub), n = cpS.length, m = cpSub.length;
    let lo = start == null ? 0 : (start < 0 ? Math.max(n + start, 0) : start);
    const hi = end == null ? n : (end < 0 ? Math.max(n + end, 0) : Math.min(end, n));
    if (m === 0) { lo = Math.min(lo, n); return lo <= hi ? (last ? hi : lo) : -1; }
    if (last) {
        for (let i = hi - m; i >= lo; i--) {
            let k = 0; while (k < m && cpS[i + k] === cpSub[k]) k++;
            if (k === m) return i;
        }
        return -1;
    }
    for (let i = lo; i + m <= hi; i++) {
        let k = 0; while (k < m && cpS[i + k] === cpSub[k]) k++;
        if (k === m) return i;
    }
    return -1;
}

export function pyLen(obj) {
    if (obj == null) throw new TypeError("object of type 'NoneType' has no len()");
    if (typeof obj === "string") return __hasSurrogate(obj) ? [...obj].length : obj.length;
    if (Array.isArray(obj)) return obj.length;
    if (obj instanceof Set || obj instanceof Map) return obj.size;
    if (typeof obj.__len__ === "function") return obj.__len__();
    // #155: genexps are now real generator objects. CPython: len(genexp)
    // raises TypeError — do NOT fall through to Object.keys (which would
    // silently return 0).
    if (typeof obj.next === "function" && typeof obj[Symbol.iterator] === "function") {
        throw new TypeError("object of type 'generator' has no len()");
    }
    if (typeof obj.length === "number") return obj.length;
    if (typeof obj.size === "number") return obj.size;
    return Object.keys(obj).length;
}

/**
 * Python-compatible round().
 *
 * Matches CPython semantics:
 * - round(x): returns an integer (Number), rounding half to even ("banker's
 *   rounding") — round(0.5)===0, round(1.5)===2, round(2.5)===2.
 * - round(x, ndigits): rounds to ndigits decimal places (half to even),
 *   returning a float. Negative ndigits rounds to tens/hundreds/etc.
 *
 * JS's Math.round rounds half up and takes no ndigits arg, so it can't stand
 * in for Python's round directly.
 */
// #318: banker's rounding of a BigInt to a power of 10 (negative ndigits).
// k = -ndigits > 0; returns a BigInt. round(int, n) always returns an int.
function __roundBigNeg(x, k) {
    const p = 10n ** BigInt(k);
    const neg = x < 0n;
    const a = neg ? -x : x;
    const q = a / p;
    const r = a % p;
    const twice = r * 2n;
    let up;
    if (twice < p) up = false;
    else if (twice > p) up = true;
    else up = (q % 2n) === 1n; // exactly half → round to even
    const res = up ? (q + 1n) * p : q * p;
    return neg ? -res : res;
}

export function pyRound(x, ndigits) {
    // Python: bool ⊆ int — round(True) == 1, round(x, True) uses ndigits=1.
    if (typeof x === "boolean") x = x ? 1 : 0;
    if (typeof ndigits === "boolean") ndigits = ndigits ? 1 : 0;
    // #318 (a): round(int, ndigits) returns an int for ANY magnitude,
    // including BigInt-routed huge ints (was a spurious TypeError from
    // mixing BigInt with the Number path). nd>=0 is a no-op; nd<0 rounds.
    if (typeof x === "bigint") {
        const nd = ndigits == null ? 0 : Math.trunc(Number(ndigits));
        return nd >= 0 ? x : __roundBigNeg(x, -nd);
    }
    // #341: single-arg round() returns an int, so a non-finite input can't be
    // converted → ValueError (NaN) / OverflowError (±inf), like CPython. The
    // 2-arg form round(x, ndigits) returns a float, so nan/inf pass through.
    if (typeof x === "number" && !isFinite(x)) {
        if (ndigits == null) {
            if (Number.isNaN(x)) throw new ValueError("cannot convert float NaN to integer");
            throw new OverflowError("cannot convert float infinity to integer");
        }
        return x; // 2-arg form: nan/inf pass through
    }
    if (x == null || typeof x !== "number") {
        throw new TypeError("type cannot be interpreted as a number");
    }
    const nd = ndigits == null ? 0 : Math.trunc(ndigits);
    // #318 (b): extreme ndigits. A finite double can't gain precision from a
    // huge positive nd, so rounding is a no-op → return x (the old code hit
    // Math.pow(10, 400) = Infinity → NaN). A huge negative nd rounds every
    // finite value to a signed 0.0.
    const factor = Math.pow(10, nd);
    if (factor === 0) return x < 0 ? -0 : 0; // nd ≪ 0
    if (!isFinite(factor)) return x;          // nd ≫ 0
    const scaled = x * factor;
    if (!isFinite(scaled)) return x;          // scaled overflow → no-op
    // Round half to even.
    const floor = Math.floor(scaled);
    const diff = scaled - floor;
    let rounded;
    if (diff > 0.5) rounded = floor + 1;
    else if (diff < 0.5) rounded = floor;
    else rounded = floor % 2 === 0 ? floor : floor + 1; // exactly .5 → nearest even
    const result = rounded / factor;
    return result;
}

/**
 * Python-compatible iter().
 */
export function pyIter(obj) {
    if (obj == null) throw new TypeError("'NoneType' object is not iterable");
    if (typeof obj[Symbol.iterator] === "function") return obj[Symbol.iterator]();
    if (typeof obj.__iter__ === "function") return obj.__iter__();
    throw new TypeError(`'${typeof obj}' object is not iterable`);
}

/**
 * F5: Python-compatible next(). 1-arg `next(it)` advances a generator/iterator
 * and raises StopIteration when exhausted (previously `next` was undefined and
 * crashed). Requires a real iterator (`.next`), matching CPython which rejects
 * plain iterables with TypeError. The optional 2nd arg (`next(it, default)`)
 * is honored incidentally but stays officially unsupported (B-011).
 */
export function pyNext(it, ...rest) {
    if (it == null || typeof it.next !== "function") {
        throw new TypeError("object is not an iterator");
    }
    const r = it.next();
    if (r.done) {
        if (rest.length >= 1) return rest[0];
        // Round-4 sweep: a generator's `return value` rides the raised
        // StopIteration (e.value / e.args), like CPython.
        throw r.value === undefined ? new StopIteration() : new StopIteration(r.value);
    }
    return r.value;
}

/** True iff `g` is a native JS generator object (the shape compiled
 *  Python generator functions produce): next/return/throw + iterable. */
function __isJsGenerator(g) {
    return g != null
        && typeof g.next === "function"
        && typeof g.return === "function"
        && typeof g.throw === "function"
        && typeof g[Symbol.iterator] === "function";
}

/**
 * Round-4 sweep: Python `gen.send(value)`. JS generators take the value
 * through `.next(value)` instead — bridge the protocols: return the next
 * yielded value, raise StopIteration (carrying the return value) on
 * completion. Non-generator receivers with their own `.send` (WebSocket,
 * user classes) dispatch straight to it — interop preserved.
 */
export function pyGenSend(g, value) {
    if (__isJsGenerator(g)) {
        const r = g.next(value);
        if (r.done) {
            throw r.value === undefined ? new StopIteration() : new StopIteration(r.value);
        }
        return r.value;
    }
    if (g != null && typeof g.send === "function") return g.send(value);
    throw new AttributeError(`'${typeof g}' object has no attribute 'send'`);
}

/**
 * Python `gen.close()` — finish the generator (runs its finally blocks),
 * returns None. Non-generators with their own `.close` dispatch to it.
 */
export function pyGenClose(g) {
    if (__isJsGenerator(g)) {
        g.return(undefined);
        return null;
    }
    if (g != null && typeof g.close === "function") return g.close();
    throw new AttributeError(`'${typeof g}' object has no attribute 'close'`);
}

/**
 * Python `gen.throw(exc)` — raise `exc` inside the generator at the
 * paused yield. If the generator catches it and yields, that value is
 * returned; if it returns, StopIteration is raised; if it doesn't catch,
 * the exception propagates (native `.throw` already does the latter two
 * halves — we add the yielded-value/StopIteration mapping).
 */
export function pyGenThrow(g, exc) {
    if (__isJsGenerator(g)) {
        const r = g.throw(exc);
        if (r.done) {
            throw r.value === undefined ? new StopIteration() : new StopIteration(r.value);
        }
        return r.value;
    }
    if (g != null && typeof g.throw === "function") return g.throw(exc);
    throw new AttributeError(`'${typeof g}' object has no attribute 'throw'`);
}

/**
 * Python-compatible slice.
 * pySlice(obj, start, stop, step)
 */
export function pySlice(obj, start, stop, step) {
    // crit-9: a custom object with __getitem__ handles the slice itself — don't
    // force it through the array/string path (which returns [] for a
    // non-sequence). It receives a minimal slice object {start, stop, step}.
    if (obj != null && typeof obj !== "string" && !Array.isArray(obj) && typeof obj.__getitem__ === "function") {
        return obj.__getitem__({ start, stop, step, __pyslice__: true });
    }
    // crit-10: BigInt bounds/step (from huge int literals) can't mix with the
    // Number index arithmetic below; demote to Number. A huge step becomes a
    // large finite float, so the walk takes just the first element, per CPython
    // ([1,2][::10**100] == [1]).
    if (typeof start === "bigint") start = Number(start);
    if (typeof stop === "bigint") stop = Number(stop);
    if (typeof step === "bigint") step = Number(step);
    // F2: slice strings by code point when astral chars are present. `seq`
    // is the code-point array for surrogate-bearing strings, else the original
    // (a plain string indexes/measures identically when no surrogates exist).
    const isStr = typeof obj === "string";
    const seq = (isStr && __hasSurrogate(obj)) ? [...obj] : obj;
    const len = seq.length;
    if (step == null) step = 1;
    if (step === 0) throw new ValueError("slice step cannot be zero");

    // PBT-1: normalize exactly like CPython's slice.indices — out-of-range
    // start/stop clamp into [lower, upper], where the bounds depend on the
    // step sign (for a negative step the valid walk range is len-1 .. -1).
    // The old code clamped positive bounds to `len` for BOTH signs, so
    // `['a','b'][7::-1]` started at a nonexistent index and emitted
    // undefined→None padding instead of clamping to the last element.
    const lower = step < 0 ? -1 : 0;
    const upper = step < 0 ? len - 1 : len;

    if (start == null) start = step < 0 ? upper : lower;
    else if (start < 0) start = Math.max(start + len, lower);
    else start = Math.min(start, upper);

    if (stop == null) stop = step < 0 ? lower : upper;
    else if (stop < 0) stop = Math.max(stop + len, lower);
    else stop = Math.min(stop, upper);

    const result = [];
    if (step > 0) {
        for (let i = start; i < stop; i += step) result.push(seq[i]);
    } else {
        for (let i = start; i > stop; i += step) result.push(seq[i]);
    }
    if (isStr) return result.join("");
    // crit-9: slicing a tuple yields a tuple, not a list (preserve __pytuple__).
    if (Array.isArray(obj) && obj.__pytuple__) {
        Object.defineProperty(result, "__pytuple__", { value: true, enumerable: false });
    }
    return result;
}

// #219: slice ASSIGNMENT `l[a:b] = xs` / `l[::k] = xs`. A simple slice
// (step 1/None) splices in place and may resize the list (Python semantics);
// an extended slice (step != 1) assigns element-wise and requires the RHS to
// have exactly as many items as the slice selects. Index normalization mirrors
// pySlice so reads and writes agree.
export function pySetSlice(arr, start, stop, step, values) {
    const len = arr.length;
    const vals = [...values];
    if (step == null || step === 1) {
        let s = start == null ? 0 : start < 0 ? Math.max(0, len + start) : Math.min(start, len);
        let e = stop == null ? len : stop < 0 ? Math.max(0, len + stop) : Math.min(stop, len);
        if (e < s) e = s;
        arr.splice(s, e - s, ...vals);
        return;
    }
    if (step === 0) throw new ValueError("slice step cannot be zero");
    // PBT-1: same CPython slice.indices clamping as pySlice above, so an
    // out-of-range extended-slice write (e.g. `l[7::-1] = xs`) targets the
    // same clamped index set a read would select, instead of writing past
    // the end of the list.
    const lower = step < 0 ? -1 : 0;
    const upper = step < 0 ? len - 1 : len;
    if (start == null) start = step < 0 ? upper : lower;
    else if (start < 0) start = Math.max(start + len, lower);
    else start = Math.min(start, upper);
    if (stop == null) stop = step < 0 ? lower : upper;
    else if (stop < 0) stop = Math.max(stop + len, lower);
    else stop = Math.min(stop, upper);
    const idxs = [];
    if (step > 0) {
        for (let i = start; i < stop; i += step) idxs.push(i);
    } else {
        for (let i = start; i > stop; i += step) idxs.push(i);
    }
    if (idxs.length !== vals.length) {
        throw new ValueError(
            `attempt to assign sequence of size ${vals.length} to extended slice of size ${idxs.length}`,
        );
    }
    for (let i = 0; i < idxs.length; i++) arr[idxs[i]] = vals[i];
}

// #321: slice DELETE `del xs[a:b]` / `del xs[::k]`. Out-of-range bounds
// clamp per CPython slice.indices (a no-op when the clamped range is empty)
// instead of raising IndexError — the DELETE sibling of pySlice/pySetSlice.
// A simple slice (step 1/None) removes a contiguous run; an extended slice
// removes the selected indices (walk descending so earlier splices don't
// shift later targets).
export function pyDelSlice(arr, start, stop, step) {
    if (!Array.isArray(arr)) {
        throw new TypeError_("'" + (arr == null ? "NoneType" : typeof arr)
            + "' object does not support item deletion");
    }
    const len = arr.length;
    if (step == null || step === 1) {
        let s = start == null ? 0 : start < 0 ? Math.max(0, len + start) : Math.min(start, len);
        let e = stop == null ? len : stop < 0 ? Math.max(0, len + stop) : Math.min(stop, len);
        if (e < s) e = s;
        arr.splice(s, e - s);
        return;
    }
    if (step === 0) throw new ValueError("slice step cannot be zero");
    // Same CPython slice.indices clamping as pySlice/pySetSlice.
    const lower = step < 0 ? -1 : 0;
    const upper = step < 0 ? len - 1 : len;
    if (start == null) start = step < 0 ? upper : lower;
    else if (start < 0) start = Math.max(start + len, lower);
    else start = Math.min(start, upper);
    if (stop == null) stop = step < 0 ? lower : upper;
    else if (stop < 0) stop = Math.max(stop + len, lower);
    else stop = Math.min(stop, upper);
    const idxs = [];
    if (step > 0) {
        for (let i = start; i < stop; i += step) idxs.push(i);
    } else {
        for (let i = start; i > stop; i += step) idxs.push(i);
    }
    // Descending order so each splice leaves lower indices intact.
    idxs.sort((a, b) => b - a);
    for (const i of idxs) arr.splice(i, 1);
}

/**
 * Python-compatible contains check.
 */
export function pyContains(container, item) {
    if (container == null) throw new TypeError("argument of type 'NoneType' is not iterable");
    if (typeof container.__contains__ === "function") return container.__contains__(item);
    if (typeof container === "string") {
        // crit-18: `x in <str>` requires x to be a str in CPython (1 in "123"
        // raises TypeError); JS .includes would coerce the needle to "1".
        if (typeof item !== "string") throw new TypeError_("'in <string>' requires string as left operand");
        return container.includes(item);
    }
    if (Array.isArray(container)) {
        // Python membership is `x is item or x == item` — route through pyEq
        // (consults __eq__, compares tuples element-wise, and treats bool≡int
        // so `1 in [True, ...]` and tuple membership work). #241.
        return container.some((x) => pyEq(x, item));
    }
    if (container instanceof Set) {
        if (container.has(item)) return true;
        // #241: bool≡int — a set built with `1` won't `.has(true)`; fall back
        // to a pyEq scan for numeric/bool items (only on a miss).
        if (typeof item === "boolean" || typeof item === "number" || typeof item === "bigint") {
            for (const x of container) if (pyEq(x, item)) return true;
        }
        return false;
    }
    if (container instanceof Map) return container.has(item);
    // #155: genexps (and other iterators/iterables that aren't dicts) —
    // consume via the iterator protocol, same membership test as the
    // Array path above. Plain-object dicts have no Symbol.iterator, so
    // they still take the hasOwnProperty path below.
    if (typeof container[Symbol.iterator] === "function") {
        for (const x of container) {
            if (x === item
                || (x !== null && typeof x?.__eq__ === "function" && x.__eq__(item) === true)
                || (item !== null && typeof item?.__eq__ === "function" && item.__eq__(x) === true)) {
                return true;
            }
        }
        return false;
    }
    // Object dict — F3: use hasOwnProperty so inherited prototype members
    // (`hasOwnProperty`, `toString`, `constructor`, `__proto__`, ...) don't
    // spuriously report as keys the way JS `in` would.
    return Object.prototype.hasOwnProperty.call(container, item);
}

// ── Python exception hierarchy ─────────────────────────────────────────
// Round-4 sweep rebuild. Exceptions now carry CPython's introspection
// surface:
//   • `args`     — the constructor arguments as a tuple (`e.args`)
//   • `__name__` — set on each class below (and by the codegen for user
//                  classes) so `type(e).__name__` works
//   • message    — CPython's str(e): args[0] for one arg, repr of the
//                  args tuple for several, "" for none. KeyError alone
//                  reprs its single arg (`str(KeyError('k'))` → `"'k'"`).
// The classes also mirror CPython's hierarchy (KeyError/IndexError →
// LookupError, ZeroDivisionError/OverflowError → ArithmeticError) so
// `except LookupError` catches a KeyError via instanceof.
function __excStr(args) {
    if (args.length === 0) return "";
    if (args.length === 1) {
        const a = args[0];
        return typeof a === "string" ? a : pyStr(a);
    }
    return pyRepr(args); // args is tuple-marked → "('boom', 42)"
}

class Exception extends Error {
    constructor(...args) {
        const t = pyTuple(...args);
        super(__excStr(t));
        this.name = new.target.__name__ ?? new.target.name;
        this.args = t;
    }
}

class ValueError extends Exception {}
class TypeError_ extends Exception {}
class AttributeError extends Exception {}
class StopIteration extends Exception {
    constructor(...args) {
        super(...args);
        // CPython: StopIteration.value is the generator's return value —
        // args[0] when present, None otherwise (round-4 sweep).
        this.value = args.length > 0 ? args[0] : null;
    }
}
class StopAsyncIteration extends Exception {}
class RuntimeError extends Exception {}
class NotImplementedError extends RuntimeError {}
class LookupError extends Exception {}
class IndexError extends LookupError {}
class ArithmeticError extends Exception {}
class ZeroDivisionError extends ArithmeticError {}
class OverflowError extends ArithmeticError {}
// PBT-2: reading a for-loop target after a zero-iteration loop must raise
// (UnboundLocalError in a function, NameError at module scope), not yield
// None. The codegen initializes such hoisted targets to the __UNBOUND
// sentinel and routes reads through __pyChkLocal/__pyChkGlobal below.
class NameError extends Exception {}
class UnboundLocalError extends NameError {}

class KeyError extends LookupError {
    constructor(...args) {
        super(...args);
        // CPython quirk: str(KeyError(k)) is repr(k), not str(k).
        if (args.length === 1) this.message = pyRepr(args[0]);
    }
}

Exception.__name__ = "Exception";
ValueError.__name__ = "ValueError";
TypeError_.__name__ = "TypeError";
AttributeError.__name__ = "AttributeError";
StopIteration.__name__ = "StopIteration";
StopAsyncIteration.__name__ = "StopAsyncIteration";
RuntimeError.__name__ = "RuntimeError";
NotImplementedError.__name__ = "NotImplementedError";
LookupError.__name__ = "LookupError";
IndexError.__name__ = "IndexError";
ArithmeticError.__name__ = "ArithmeticError";
ZeroDivisionError.__name__ = "ZeroDivisionError";
OverflowError.__name__ = "OverflowError";
NameError.__name__ = "NameError";
UnboundLocalError.__name__ = "UnboundLocalError";
KeyError.__name__ = "KeyError";

// PBT-2: sentinel marking a hoisted for-loop target that no iteration ever
// assigned. Reads of such names route through the guards below; any real
// assignment (loop iteration or ordinary `=`) overwrites the sentinel.
const __UNBOUND = Symbol("unbound");

// PBT-2: function-scope read guard — CPython 3.11+ message shape.
function __pyChkLocal(v, name) {
    if (v === __UNBOUND) {
        throw new UnboundLocalError(
            `cannot access local variable '${name}' where it is not associated with a value`,
        );
    }
    return v;
}

// PBT-2: module-scope read guard — an unbound module name is a NameError.
function __pyChkGlobal(v, name) {
    if (v === __UNBOUND) throw new NameError(`name '${name}' is not defined`);
    return v;
}

// ── #166: value-aware type() ────────────────────────────────────────────
// The old lowering `obj?.constructor ?? typeof obj` gave Number/String/…
// whose `__name__` is undefined (and whose `.name` would be the JS name,
// not the Python one). Primitives map to singleton "type objects" carrying
// the CPython type name; class instances keep returning their constructor
// (compiled classes / runtime exceptions have `__name__` stamped on them).
//
// Documented limits (see PR #166 discussion):
// - JS has one number type: `type(5.0)` reports 'int' (Number.isInteger).
// - The singletons are not callable (`type(x)()` construction unsupported).
// - `type(fn)` distinguishes classes from functions by `class` source
//   detection, which is right for compiled Python classes.
class __PyTypeObj {
    constructor(name) {
        this.__name__ = name;
    }
    __repr__() {
        return `<class '${this.__name__}'>`;
    }
    __str__() {
        return `<class '${this.__name__}'>`;
    }
}
const __PyInt = new __PyTypeObj("int");
const __PyFloat = new __PyTypeObj("float");
const __PyBool = new __PyTypeObj("bool");
const __PyStr = new __PyTypeObj("str");
const __PyList = new __PyTypeObj("list");
const __PyTuple = new __PyTypeObj("tuple");
const __PySet = new __PyTypeObj("set");
const __PyDict = new __PyTypeObj("dict");
const __PyNoneType = new __PyTypeObj("NoneType");
const __PyTypeMeta = new __PyTypeObj("type");
const __PyFunction = new __PyTypeObj("function");

/**
 * Python-compatible type(). Value-aware: primitives return interned
 * type objects whose `__name__` is the CPython type name; `type(x) ==
 * type(y)` works by identity for same-category values; instances of
 * compiled/native classes return the constructor unchanged.
 */
export function pyType(v) {
    if (v === null || v === undefined) return __PyNoneType;
    switch (typeof v) {
        case "boolean": return __PyBool; // BEFORE number — bool is not int here
        case "number": return Number.isInteger(v) ? __PyInt : __PyFloat;
        case "bigint": return __PyInt;
        case "string": return __PyStr;
        case "function":
            // Compiled Python classes emit as `class X ...`; type(cls) is
            // CPython's metaclass `type`. Plain functions stay 'function'.
            return /^class[\s{]/.test(Function.prototype.toString.call(v))
                ? __PyTypeMeta
                : __PyFunction;
    }
    if (v instanceof __PyTypeObj) return __PyTypeMeta; // type(type(5)) → type
    if (Array.isArray(v)) return v.__pytuple__ ? __PyTuple : __PyList;
    if (v instanceof Set) return __PySet;
    if (v instanceof Map) return __PyDict; // Map-backed PyDict
    if (v instanceof Error) {
        // Runtime-raised builtins (pyGetItem's IndexError/KeyError, pyDiv's
        // ZeroDivisionError, native JS TypeError/ReferenceError, …) are plain
        // Error objects whose constructor is `Error` (or a native error type)
        // with no stamped `__name__` — returning that constructor makes
        // `type(e).__name__` read "Error". User/runtime exception CLASSES keep
        // their stamped `__name__` (custom `class X(Exception)`), so prefer the
        // constructor when it carries one. JS TDZ ReferenceError maps to
        // Python's UnboundLocalError (referenced-before-assignment) or NameError
        // (truly undefined) by message, matching CPython.
        const ec = v.constructor;
        if (ec && ec !== Error && ec.__name__) return ec;
        let n = v.name;
        if (n === "ReferenceError") {
            n = /before initialization/.test(v.message || "") ? "UnboundLocalError" : "NameError";
        }
        return new __PyTypeObj(n || "Exception");
    }
    const ctor = v.constructor;
    if (ctor && ctor !== Object) return ctor; // class instances (incl. exceptions)
    return __PyDict; // plain-object dict
}

/**
 * Round-4 sweep: bridge Python's async-iterator protocol
 * (`__aiter__`/`__anext__` + StopAsyncIteration) to JS's, so
 * `async for x in obj:` (lowered to `for await (const x of
 * __pyAsyncIter(obj))`) works over protocol classes. Native (a)sync
 * iterables — async generators, arrays — pass through untouched.
 */
export function __pyAsyncIter(obj) {
    if (obj == null) {
        throw new TypeError_("'NoneType' object is not an async iterable");
    }
    if (typeof obj[Symbol.asyncIterator] === "function"
        || typeof obj[Symbol.iterator] === "function") {
        return obj;
    }
    const it = typeof obj.__aiter__ === "function" ? obj.__aiter__() : obj;
    if (it == null || typeof it.__anext__ !== "function") return obj;
    return {
        [Symbol.asyncIterator]() {
            return {
                async next() {
                    try {
                        return { value: await it.__anext__(), done: false };
                    } catch (e) {
                        if (e instanceof StopAsyncIteration
                            || (e != null && e.name === "StopAsyncIteration")) {
                            return { value: undefined, done: true };
                        }
                        throw e;
                    }
                },
            };
        },
    };
}

/**
 * Python-compatible subscript read.
 *
 * Matches CPython error wording so a Python developer reading the
 * trace sees their own language's idioms, not JS's. Used by the
 * codegen for `a[i]` and `d[k]` reads when the receiver type is
 * inferred as list/dict/tuple.
 *
 *   pyGetItem([1,2,3], 5)      → IndexError: list index out of range
 *   pyGetItem({"a":1}, "b")    → KeyError: 'b'
 *   pyGetItem({"a":1}, 0)      → KeyError: 0  (CPython quotes strings, not numbers)
 *   pyGetItem("hi", 10)        → IndexError: string index out of range
 *   pyGetItem([1,2,3], -1)     → 3 (Python negative indexing)
 *   pyGetItem(null, 0)         → TypeError: 'NoneType' object is not subscriptable
 */
export function pyGetItem(obj, key) {
    if (obj == null) {
        throw new TypeError_("'NoneType' object is not subscriptable");
    }
    // Non-subscriptable primitives (int/float/bool). Without this guard a JS
    // number/bigint/boolean falls through to the interop passthrough below
    // (`Object.getPrototypeOf(5) !== Object.prototype`) and silently returns
    // `undefined` instead of raising — CPython raises TypeError. Found by the
    // lattice C4 shipping-binding (`5[0]`, `True[0]`, `(3.5)[0]`).
    if (typeof obj === "number" || typeof obj === "bigint" || typeof obj === "boolean") {
        const tn = typeof obj === "boolean" ? "bool"
            : ((typeof obj === "bigint" || Number.isInteger(obj)) ? "int" : "float");
        throw new TypeError_(`'${tn}' object is not subscriptable`);
    }
    // A set is not subscriptable (`{1,2}[0]` → TypeError). Sets route through
    // the helper now (cert::route), so guard here rather than fall through to
    // the interop passthrough (which would return `undefined`).
    if (obj instanceof Set) {
        throw new TypeError_("'set' object is not subscriptable");
    }
    // #258: bool ⊂ int, so `xs[True]` ≡ `xs[1]`; a bare bool key would coerce
    // to the string "true"/"false" and miss.
    if (typeof key === "boolean") key = key ? 1 : 0;
    // #344: a BigInt index (|index| beyond the Number range, e.g. from a huge
    // int literal) can't take part in the Number index arithmetic below
    // (`bigintIndex + length` is a TypeError). For a real sequence any such
    // index is out of range → IndexError, like CPython (not TypeError). A small
    // enough BigInt is demoted so normal indexing proceeds.
    if (typeof key === "bigint" && (typeof obj === "string" || Array.isArray(obj))) {
        if (key >= -9007199254740991n && key <= 9007199254740991n) {
            key = Number(key);
        } else {
            throw new IndexError(
                (typeof obj === "string" ? "string" : "list") + " index out of range");
        }
    }
    // crit-8: a non-integer numeric index on a sequence is a TypeError in
    // CPython ([10,20][1.5]). A whole-valued float (1.0) is an indistinguishable
    // JS Number here and falls under the documented whole-float deviation (B1).
    if ((typeof obj === "string" || Array.isArray(obj)) && typeof key === "number" && !Number.isInteger(key)) {
        throw new TypeError_((typeof obj === "string" ? "string" : "list") + " indices must be integers");
    }
    if (typeof obj === "string") {
        // A non-integer key (str, or a non-whole number) is a TypeError, not a
        // silent `undefined` (`"ab"["k"]`). Lattice C4 shipping-binding.
        if (typeof key === "string" || (typeof key === "number" && !Number.isInteger(key))) {
            throw new TypeError_("string indices must be integers");
        }
        // F2: index by code point when astral chars are present.
        if (__hasSurrogate(obj)) {
            const cps = [...obj];
            const n = cps.length;
            let i = key;
            if (i < 0) i += n;
            if (i < 0 || i >= n) throw new IndexError("string index out of range");
            return cps[i];
        }
        const n = obj.length;
        let i = key;
        if (i < 0) i += n;
        if (i < 0 || i >= n) throw new IndexError("string index out of range");
        return obj[i];
    }
    if (Array.isArray(obj)) {
        // A non-integer key (str, or a non-whole number) is a TypeError, not a
        // silent `undefined` (`[1,2]["k"]`, `[1,2][1.5]`). Lattice C4 binding.
        if (typeof key === "string" || (typeof key === "number" && !Number.isInteger(key))) {
            throw new TypeError_(
                "list indices must be integers or slices, not "
                + (typeof key === "string" ? "str" : "float"));
        }
        const n = obj.length;
        let i = key;
        if (i < 0) i += n;
        if (i < 0 || i >= n) throw new IndexError("list index out of range");
        return obj[i];
    }
    if (obj instanceof Map) {
        if (!obj.has(key)) {
            // CPython __missing__ protocol (dict subclasses): defaultdict
            // creates the default; Counter reads as 0. Pythonic-checks sweep.
            if (typeof obj.__missing__ === "function") return obj.__missing__(key);
            throw new KeyError(key);
        }
        return obj.get(key);
    }
    if (typeof obj.__getitem__ === "function") return obj.__getitem__(key);
    // Interop passthrough: objects that are NOT plain dicts (class
    // instances, DOM wrappers like CSSStyleDeclaration whose members live
    // on the prototype, library objects) keep native JS subscript
    // semantics — no own-key check, no KeyError. Only plain-prototype
    // objects get Python dict strictness below.
    {
        const proto = Object.getPrototypeOf(obj);
        if (proto !== Object.prototype && proto !== null) return obj[key];
    }
    // Plain object — treat as dict
    if (!Object.prototype.hasOwnProperty.call(obj, key)) {
        throw new KeyError(key);
    }
    return obj[key];
}

export { ValueError, IndexError, KeyError, TypeError_ as TypeError, AttributeError, StopIteration, StopAsyncIteration, ZeroDivisionError, Exception, OverflowError, RuntimeError, NotImplementedError, LookupError, ArithmeticError, NameError, UnboundLocalError };
// PBT-2: sentinel + read guards for possibly-unbound for-loop targets.
export { __UNBOUND, __pyChkLocal, __pyChkGlobal };

/**
 * Python `del obj[key]` (issue #101). Lists splice (Python removes the
 * slot — never a JS hole) with IndexError on out-of-range; dicts (plain
 * objects / Maps) delete the key with KeyError when absent; custom
 * classes dispatch `__delitem__`.
 */
export function pyDelItem(obj, key) {
    if (obj == null) {
        throw new TypeError_("'NoneType' object does not support item deletion");
    }
    if (Array.isArray(obj)) {
        const n = obj.length;
        let i = typeof key === "bigint" ? Number(key) : key;
        if (i < 0) i += n;
        if (!Number.isInteger(i) || i < 0 || i >= n) {
            throw new IndexError("list assignment index out of range");
        }
        obj.splice(i, 1);
        return;
    }
    if (obj instanceof Map) {
        if (!obj.delete(key)) {
            throw new KeyError(key);
        }
        return;
    }
    if (typeof obj.__delitem__ === "function") {
        obj.__delitem__(key);
        return;
    }
    if (!Object.prototype.hasOwnProperty.call(obj, key)) {
        throw new KeyError(key);
    }
    delete obj[key];
}

/**
 * Python `chr(n)` — code-point based (astral-safe), with CPython's range
 * check. Issue #89.
 */
export function pyChr(n) {
    const i = typeof n === "bigint" ? Number(n) : Math.trunc(Number(n));
    if (!Number.isFinite(i) || i < 0 || i >= 0x110000) {
        throw new ValueError("chr() arg not in range(0x110000)");
    }
    return String.fromCodePoint(i);
}

/**
 * Python `ord(s)` — accepts exactly one character (one Unicode code
 * point, so astral chars like "😀" count as length 1). Issue #89.
 */
export function pyOrd(s) {
    if (typeof s !== "string") {
        throw new TypeError_("ord() expected string of length 1");
    }
    const cps = [...s];
    if (cps.length !== 1) {
        throw new TypeError_(
            `ord() expected a character, but string of length ${cps.length} found`,
        );
    }
    return cps[0].codePointAt(0);
}

/**
 * Python `bin(n)` / `hex(n)` / `oct(n)` — integer → base-prefixed string with
 * Python's sign convention (`bin(-5) === "-0b101"`, `hex(255) === "0xff"`,
 * `oct(8) === "0o10"`). Accepts int-valued numbers and bigints; rejects
 * non-integers like CPython (`hex(1.5)` → TypeError). BigInt.toString gives
 * lowercase digits, matching CPython.
 */
function _pyRadix(n, prefix, radix, fn) {
    let big;
    if (typeof n === "bigint") {
        big = n;
    } else if (typeof n === "number" && Number.isInteger(n)) {
        big = BigInt(n);
    } else {
        throw new TypeError_(
            `${fn}() argument can't be interpreted as an integer`,
        );
    }
    const neg = big < 0n;
    const digits = (neg ? -big : big).toString(radix);
    return (neg ? "-" : "") + prefix + digits;
}
export function pyBin(n) {
    return _pyRadix(n, "0b", 2, "bin");
}
export function pyHex(n) {
    return _pyRadix(n, "0x", 16, "hex");
}
export function pyOct(n) {
    return _pyRadix(n, "0o", 8, "oct");
}

/**
 * Python `min()`/`max()` — both call forms (single iterable, or multiple
 * scalar args), plus the `key=` and `default=` kwargs (passed by the
 * codegen's universal kwargs-as-trailing-options-object convention).
 * Comparison is Python-faithful (`pyLtCmp` handles strings, BigInt/Number
 * mixes, and `__lt__`-bearing operands alike). Issue #88.
 */
function __minmax(name, wantGreater, args) {
    let key = null;
    let dflt;
    let hasDefault = false;
    if (args.length > 1) {
        const last = args[args.length - 1];
        if (last !== null && typeof last === "object"
            && Object.getPrototypeOf(last) === Object.prototype
            && (Object.prototype.hasOwnProperty.call(last, "key")
                || Object.prototype.hasOwnProperty.call(last, "default"))) {
            if (last.key != null) key = last.key;
            if (Object.prototype.hasOwnProperty.call(last, "default")) {
                dflt = last.default;
                hasDefault = true;
            }
            args = args.slice(0, -1);
        }
    }
    const items = args.length === 1 ? [...pyForIter(args[0])] : args;
    if (items.length === 0) {
        if (hasDefault) return dflt;
        throw new ValueError(`${name}() iterable argument is empty`);
    }
    let best = items[0];
    let bestKey = key ? key(best) : best;
    for (let i = 1; i < items.length; i++) {
        const k = key ? key(items[i]) : items[i];
        // Python keeps the FIRST occurrence on ties, so strictly-better only.
        const better = wantGreater
            ? (typeof k?.__gt__ === "function" ? k.__gt__(bestKey) : k > bestKey)
            : (typeof k?.__lt__ === "function" ? k.__lt__(bestKey) : k < bestKey);
        if (better) {
            best = items[i];
            bestKey = k;
        }
    }
    return best;
}

export function pyMin(...args) {
    return __minmax("min", false, args);
}

export function pyMax(...args) {
    return __minmax("max", true, args);
}

// =====================================================================
// Python-method runtime helpers.
//
// These back the Runtime and Hybrid entries in the codegen's
// `method_lowering` table (crates/pyths_codegen_js/src/method_lowering.rs).
// They exist so the codegen can emit clean JS without polluting the
// global Array/String/Object prototypes with Python-named methods.
// =====================================================================

// ----- string helpers -----

/** Python `sep.join(iter)` — note the arg order vs JS `arr.join(sep)`.
 * #301: a non-string receiver with its own .join (a JS Array — the
 * ubiquitous `arr.join(",")` idiom) dispatches to the native method;
 * the old unconditional form fed the ARRAY in as the separator. */
export function pyStrJoin(sep, iter) {
    if (typeof sep !== "string" && sep != null && typeof sep.join === "function") {
        return sep.join(iter);
    }
    return Array.from(iter).join(sep);
}

/** Python `s.split()` (whitespace) and `s.split(sep)`. Empty separator
 * raises ValueError like CPython (issue #92). */
export function pyStrSplit(s, sep, maxsplit) {
    // #301: non-string receivers with their own .split dispatch natively.
    if (typeof s !== "string" && s != null && typeof s.split === "function") {
        return s.split(sep, maxsplit);
    }
    // #213: accept the keyword forms `split(sep=..., maxsplit=...)` — which
    // the compiler lowers to a trailing options object — as well as the
    // positional `split(sep, maxsplit)`, and actually honor maxsplit.
    const optsFrom = (v) =>
        v !== null && typeof v === "object" && !Array.isArray(v);
    if (optsFrom(sep)) {
        if ("maxsplit" in sep) maxsplit = sep.maxsplit;
        sep = "sep" in sep ? sep.sep : undefined;
    } else if (optsFrom(maxsplit)) {
        maxsplit = "maxsplit" in maxsplit ? maxsplit.maxsplit : undefined;
    }
    const lim =
        maxsplit === undefined || maxsplit === null || maxsplit < 0
            ? Infinity
            : maxsplit;

    if (sep === undefined || sep === null) {
        // Whitespace split: collapse runs of whitespace, drop empties, and
        // (for a finite maxsplit) keep the unsplit remainder as the last part.
        const trimmed = s.replace(/^\s+/, "");
        if (trimmed === "") return [];
        if (lim === Infinity) return trimmed.split(/\s+/).filter(Boolean);
        const out = [];
        let i = 0;
        while (out.length < lim) {
            while (i < trimmed.length && /\s/.test(trimmed[i])) i++;
            if (i >= trimmed.length) break;
            let start = i;
            while (i < trimmed.length && !/\s/.test(trimmed[i])) i++;
            out.push(trimmed.slice(start, i));
        }
        while (i < trimmed.length && /\s/.test(trimmed[i])) i++;
        if (i < trimmed.length) out.push(trimmed.slice(i).replace(/\s+$/, ""));
        return out;
    }

    if (sep === "") throw new ValueError("empty separator");
    if (lim === Infinity) return s.split(sep);

    // Explicit separator with a finite maxsplit: at most `lim` splits.
    const out = [];
    let idx = 0;
    while (out.length < lim) {
        const next = s.indexOf(sep, idx);
        if (next === -1) break;
        out.push(s.slice(idx, next));
        idx = next + sep.length;
    }
    out.push(s.slice(idx));
    return out;
}

/** Python `s.title()` — first cased char of each "word" uppercased, the
 * REST of the word lowercased. CPython's word boundary is any non-cased
 * char, so `"it's".title()` is `"It'S"` (apostrophe restarts the word) —
 * issue #87. */
export function pyStrTitle(s) {
    let out = "";
    let prevCased = false;
    for (const ch of s) {
        const lo = ch.toLowerCase();
        const up = ch.toUpperCase();
        if (lo === up) {
            // Not a cased character — pass through, reset word state.
            out += ch;
            prevCased = false;
        } else {
            out += prevCased ? lo : up;
            prevCased = true;
        }
    }
    return out;
}

/** Python `s.capitalize()` — first letter upper, rest lower. */
export function pyStrCapitalize(s) {
    return s ? s[0].toUpperCase() + s.slice(1).toLowerCase() : s;
}

/** Python `s.strip(chars)` — strip any chars in the set from both ends. */
export function pyStrStrip(s, chars) {
    if (chars === undefined || chars === null) return s.replace(/^\s+|\s+$/g, "");
    return pyStrLstrip(pyStrRstrip(s, chars), chars);
}

/** Python `s.lstrip(chars)` — strip leading chars in set.
 * Wave-19 verification fix: iterate CODE POINTS — `new Set(chars)` holds
 * whole astral chars while `s[i]` is one UTF-16 unit, so the old unit-wise
 * walk never matched an astral strip char ('𝔸a𝔸'.strip('𝔸') was a no-op). */
export function pyStrLstrip(s, chars) {
    if (chars === undefined || chars === null) return s.replace(/^\s+/, "");
    const set = new Set(chars);
    let i = 0;
    while (i < s.length) {
        const ch = String.fromCodePoint(s.codePointAt(i));
        if (!set.has(ch)) break;
        i += ch.length;
    }
    return s.slice(i);
}

/** Python `s.rstrip(chars)` — strip trailing chars in set (code-point-aware,
 * surrogate-pair-safe backward walk — see pyStrLstrip). */
export function pyStrRstrip(s, chars) {
    if (chars === undefined || chars === null) return s.replace(/\s+$/, "");
    const set = new Set(chars);
    let end = s.length;
    while (end > 0) {
        let start = end - 1;
        const unit = s.charCodeAt(start);
        if (unit >= 0xDC00 && unit <= 0xDFFF && start > 0) {
            const lead = s.charCodeAt(start - 1);
            if (lead >= 0xD800 && lead <= 0xDBFF) start -= 1;
        }
        if (!set.has(s.slice(start, end))) break;
        end = start;
    }
    return s.slice(0, end);
}

/**
 * Exact fixed-point decimal expansion of a double with CPython's
 * round-half-to-even tie-breaking (issue #86).
 *
 * JS `toFixed` rounds exact ties away from zero (`(1.625).toFixed(2)` →
 * `"1.63"`), CPython rounds them to even (`f"{1.625:.2f}"` → `'1.62'`).
 * Both agree on non-tie cases because both operate on the exact binary
 * value of the double (`2.675` is really `2.67499...` → `2.67`). To get
 * the tie cases right we do the decimal expansion exactly: decompose the
 * double into `M * 2^E` (BigInt mantissa/exponent), compute
 * `M * 10^prec * 2^E` as an exact integer quotient + remainder, and
 * round half-to-even on the true remainder.
 *
 * `x` must be finite and non-negative; returns the digit string
 * (no sign).
 */
function __fixedHalfEven(x, prec) {
    const p = prec > 0 ? prec : 0;
    if (x === 0) return p > 0 ? "0." + "0".repeat(p) : "0";
    const dv = new DataView(new ArrayBuffer(8));
    dv.setFloat64(0, x);
    const hi = dv.getUint32(0);
    const lo = dv.getUint32(4);
    const expBits = (hi >>> 20) & 0x7ff;
    let mant = (BigInt(hi & 0xfffff) << 32n) | BigInt(lo);
    let e2;
    if (expBits === 0) {
        e2 = -1074n; // subnormal
    } else {
        mant |= 1n << 52n;
        e2 = BigInt(expBits) - 1075n;
    }
    const scaled = mant * 10n ** BigInt(p);
    let q;
    if (e2 >= 0n) {
        q = scaled << e2; // exact integer — nothing to round
    } else {
        const shift = -e2;
        q = scaled >> shift;
        const r = scaled & ((1n << shift) - 1n);
        const half = 1n << (shift - 1n);
        if (r > half || (r === half && (q & 1n) === 1n)) q += 1n;
    }
    let s = q.toString();
    if (p === 0) return s;
    if (s.length <= p) s = "0".repeat(p - s.length + 1) + s;
    return s.slice(0, s.length - p) + "." + s.slice(s.length - p);
}

/**
 * CPython-compatible `.Nf` fixed formatter — the direct-emission target
 * for f-string specs like `f"{x:.2f}"` (issue #86; replaces bare
 * `toFixed`, which rounds exact ties away from zero instead of to even).
 */
export function pyFixed(x, prec) {
    // pyFixed is the `.Nf` fast path — a numeric presentation type. Applied
    // to a str (e.g. after `!r`/`!s`: `f'{x!r:.3f}'`) CPython raises
    // ValueError rather than re-parsing the string as a number.
    if (typeof x === "string") {
        const e = new Error("Unknown format code 'f' for object of type 'str'");
        e.name = "ValueError";
        throw e;
    }
    const n = Number(x);
    if (Number.isNaN(n)) return "nan";
    if (n === Infinity) return "inf";
    if (n === -Infinity) return "-inf";
    const neg = n < 0 || Object.is(n, -0);
    const body = __fixedHalfEven(Math.abs(n), Math.trunc(prec));
    return neg ? "-" + body : body;
}

/**
 * PEP 3101 format-spec runtime — applies a parsed spec object to a
 * value. Used by f-string lowering for combinations the codegen
 * doesn't emit inline (any spec with fill/align/sign/grouping mix or
 * a non-default type character).
 *
 * `opts` shape (all optional):
 *   { fill, align, sign, alt, zero, width, grouping, precision, type }
 *   align: "<" | ">" | "^" | "="
 *   sign:  "+" | "-" | " "
 *   grouping: "," | "_"
 *   type:  "b"|"c"|"d"|"e"|"E"|"f"|"F"|"g"|"G"|"n"|"o"|"s"|"x"|"X"|"%"
 */
export function pyFormatSpec(value, opts, isFloat) {
    opts = opts || {};
    const ty = opts.type;
    // CPython: a numeric presentation type (f/e/g/d/x/b/o/n/c/%/…) applied to
    // a str raises `ValueError: Unknown format code 'X' for object of type
    // 'str'`. This matters after an `!r`/`!s` conversion — `f'{x!r:.3f}'`
    // formats the REPR string, not the underlying number, so the spec must
    // reject numeric codes instead of silently re-parsing the string.
    if (typeof value === "string" && ty != null && ty !== "s") {
        const e = new Error(`Unknown format code '${ty}' for object of type 'str'`);
        e.name = "ValueError";
        throw e;
    }
    let s;
    let isNumeric = false;
    let neg = false;

    // Group a digit string from the right (CPython: `,`/`_` every 3 for
    // decimal, `_` every 4 for b/o/x/X). Digit-agnostic (hex letters).
    const group = (str, size, sep) => {
        let out = "";
        for (let i = str.length; i > 0; i -= size) {
            const chunk = str.slice(Math.max(0, i - size), i);
            out = out ? chunk + sep + out : chunk;
        }
        return out;
    };

    if (ty === "s" || ty === undefined && typeof value === "string") {
        s = String(value);
        if (opts.precision != null) s = s.slice(0, opts.precision);
    } else if (ty === "b" || ty === "o" || ty === "x" || ty === "X" || ty === "d" || ty === "n" || ty === "c"
        || (ty === undefined && typeof value === "bigint")) {
        isNumeric = true;
        if (ty === "c") {
            s = String.fromCodePoint(Number(value));
        } else {
            // Keep BigInt ints exact (arbitrary precision) — never round
            // through Number.
            let n = typeof value === "bigint" ? value : Math.trunc(Number(value));
            neg = n < 0;
            if (neg) n = -n;
            const radix = ty === "b" ? 2 : ty === "o" ? 8 : (ty === "x" || ty === "X") ? 16 : 10;
            s = n.toString(radix);
            if (ty === "X") s = s.toUpperCase();
            // Grouping applies to the digits only; the #-prefix goes
            // OUTSIDE the grouped digits (0b1010_1010).
            if (opts.grouping) s = group(s, radix === 10 ? 3 : 4, opts.grouping);
            if (opts.alt) {
                if (radix === 2) s = "0b" + s;
                else if (radix === 8) s = "0o" + s;
                else if (radix === 16) s = (ty === "X" ? "0X" : "0x") + s;
            }
        }
    } else if (ty === "e" || ty === "E" || ty === "f" || ty === "F" || ty === "g" || ty === "G" || ty === "%" || ty === undefined) {
        isNumeric = true;
        let n = Number(value);
        if (ty === "%") n = n * 100;
        neg = n < 0 || Object.is(n, -0);
        n = Math.abs(n);
        const prec = opts.precision != null ? opts.precision : 6;
        if (ty === "e" || ty === "E") {
            s = n.toExponential(prec);
            // CPython zero-pads the exponent to at least 2 digits
            // (e+03, e-04). JS toExponential produces e+3 / e-4. Patch
            // by normalizing the trailing exponent.
            s = s.replace(/e([+-])(\d)$/, "e$10$2");
            if (ty === "E") s = s.toUpperCase();
        } else if (ty === "g" || ty === "G") {
            // CPython 'g': with precision p (default 6; 0 → 1), let exp be
            // the decimal exponent of the value rounded to p significant
            // digits. If -4 <= exp < p → fixed notation, else scientific;
            // trailing zeros stripped (unless '#'), exponent >= 2 digits.
            let p = prec;
            if (p === 0) p = 1;
            if (n === 0) {
                s = "0";
            } else if (!Number.isFinite(n)) {
                s = n === Infinity ? "inf" : "nan";
            } else {
                const m = /^(\d)(?:\.(\d+))?e([+-]\d+)$/.exec(n.toExponential(p - 1));
                const digits = m[1] + (m[2] || "");
                const exp10 = parseInt(m[3], 10);
                if (exp10 >= -4 && exp10 < p) {
                    if (exp10 >= 0) {
                        s = digits.length <= exp10 + 1
                            ? digits + "0".repeat(exp10 + 1 - digits.length)
                            : digits.slice(0, exp10 + 1) + "." + digits.slice(exp10 + 1);
                    } else {
                        s = "0." + "0".repeat(-exp10 - 1) + digits;
                    }
                    if (!opts.alt && s.includes(".")) s = s.replace(/\.?0+$/, "");
                } else {
                    let mant = opts.alt ? digits : digits.replace(/0+$/, "") || "0";
                    const mantStr = mant.length > 1 ? mant[0] + "." + mant.slice(1) : mant;
                    s = mantStr + "e" + (exp10 < 0 ? "-" : "+") + String(Math.abs(exp10)).padStart(2, "0");
                }
            }
            if (ty === "G") s = s.toUpperCase();
        } else if (ty === "%") {
            // Round-half-even on the exact double, like CPython (#86).
            s = __fixedHalfEven(n, opts.precision != null ? opts.precision : 6) + "%";
        } else if (ty === "f" || ty === "F" || opts.precision != null) {
            s = __fixedHalfEven(n, prec);
        } else if (isFloat) {
            // #347: no type char + a statically-float value → str(float): a
            // whole-valued float keeps its '.0' ('0.0', not the int '0'). n is
            // the absolute value (sign handled below), so use the positive
            // float rendering.
            s = pyFormatFloat(n);
        } else {
            s = String(n);
        }
        if ((ty === "f" || ty === "F" || ty === undefined) && opts.grouping) {
            // Insert separators in the integer part only.
            const dot = s.indexOf(".");
            const intPart = dot === -1 ? s : s.slice(0, dot);
            const fracPart = dot === -1 ? "" : s.slice(dot);
            s = group(intPart, 3, opts.grouping) + fracPart;
        }
    } else {
        s = String(value);
    }

    // Sign handling for numeric values
    let signStr = "";
    if (isNumeric) {
        if (neg) signStr = "-";
        else if (opts.sign === "+") signStr = "+";
        else if (opts.sign === " ") signStr = " ";
    }

    // Width / fill / align
    const width = opts.width || 0;
    if (width > 0) {
        const fill = opts.fill || (opts.zero && isNumeric ? "0" : " ");
        const align = opts.align || (opts.zero && isNumeric ? "=" : (isNumeric ? ">" : "<"));
        const total = signStr.length + s.length;
        if (total < width) {
            const need = width - total;
            if (align === "<") return signStr + s + fill.repeat(need);
            if (align === ">") return fill.repeat(need) + signStr + s;
            if (align === "^") {
                const left = Math.floor(need / 2);
                return fill.repeat(left) + signStr + s + fill.repeat(need - left);
            }
            if (align === "=") return signStr + fill.repeat(need) + s;
        }
    }
    return signStr + s;
}


// #108: dynamic f-string format specs — f"{v:{w}}" / f"{x:.{p}f}" build
// the spec string at RUNTIME, so it cannot be parsed at compile time.
// parseFormatSpec mirrors the compile-time parser
// (crates/pyths_parser/src/format_spec.rs); pyFormatDynamic parses the
// built spec and delegates to pyFormatSpec.
export function pyFormatDynamic(value, specStr) {
    return pyFormatSpec(value, parseFormatSpec(String(specStr)));
}

export function parseFormatSpec(s) {
    const opts = {};
    const chars = [...s];
    let i = 0;
    if (chars.length >= 2 && "<>=^".includes(chars[1])) {
        opts.fill = chars[0]; opts.align = chars[1]; i = 2;
    } else if (chars.length >= 1 && "<>=^".includes(chars[0])) {
        opts.align = chars[0]; i = 1;
    }
    if (i < chars.length && "+- ".includes(chars[i])) { opts.sign = chars[i]; i++; }
    if (i < chars.length && chars[i] === "#") { opts.alt = true; i++; }
    if (i < chars.length && chars[i] === "0") { opts.zero = true; i++; }
    let w = "";
    while (i < chars.length && /[0-9]/.test(chars[i])) { w += chars[i]; i++; }
    if (w) opts.width = parseInt(w, 10);
    if (i < chars.length && (chars[i] === "," || chars[i] === "_")) { opts.grouping = chars[i]; i++; }
    if (i < chars.length && chars[i] === ".") {
        i++;
        let p = "";
        while (i < chars.length && /[0-9]/.test(chars[i])) { p += chars[i]; i++; }
        if (p) opts.precision = parseInt(p, 10);
    }
    if (i < chars.length) { opts.type = chars[i]; i++; }
    return opts;
}

/** Python `s.format(...)` — full replacement-field support: `{{`/`}}` escapes,
 * auto/positional/named fields with `.attr`/`[key]` access, `!r`/`!s`/`!a`
 * conversions, and `:format_spec` (delegated to pyFormatSpec, the same engine
 * behind f-strings). Interpolated values render with Python str() semantics. */
export function pyStrFormat(s, ...args) {
    // #301: non-string receivers with their own .format (Intl.NumberFormat,
    // Intl.DateTimeFormat, user classes) dispatch natively.
    if (typeof s !== "string" && s != null && typeof s.format === "function") {
        return s.format(...args);
    }
    let auto = 0;
    // Named fields look up on a trailing kwargs object (the codegen lowers
    // `.format(name=v)` to a trailing object literal); positional/auto fields
    // index the positional args directly.
    const kwargs = args.length ? args[args.length - 1] : undefined;
    const resolveField = (fieldName) => {
        let i = 0;
        let base = "";
        while (i < fieldName.length && fieldName[i] !== "." && fieldName[i] !== "[") base += fieldName[i++];
        let val;
        if (base === "") val = args[auto++];
        else if (/^\d+$/.test(base)) val = args[Number(base)];
        else if (kwargs != null && typeof kwargs === "object" && base in kwargs) val = kwargs[base];
        else {
            const e = new Error("'" + base + "'");
            e.name = "KeyError";
            throw e;
        }
        // Trailing `.attr` / `[key]` accessors (CPython field-name grammar).
        while (i < fieldName.length) {
            if (fieldName[i] === ".") {
                i++;
                let attr = "";
                while (i < fieldName.length && fieldName[i] !== "." && fieldName[i] !== "[") attr += fieldName[i++];
                val = val == null ? undefined : val[attr];
            } else if (fieldName[i] === "[") {
                i++;
                let key = "";
                while (i < fieldName.length && fieldName[i] !== "]") key += fieldName[i++];
                if (i < fieldName.length) i++; // skip ']'
                const k = /^\d+$/.test(key) ? Number(key) : key;
                val = val == null ? undefined : (val instanceof Map ? val.get(k) : val[k]);
            } else break;
        }
        return val;
    };
    let out = "";
    let i = 0;
    const n = s.length;
    while (i < n) {
        const ch = s[i];
        if (ch === "{") {
            if (s[i + 1] === "{") { out += "{"; i += 2; continue; }
            // Consume up to the matching '}' (allow one nested level so dynamic
            // specs like `{:{}}` / `{:{w}}` survive to the spec parser).
            let depth = 1;
            let j = i + 1;
            while (j < n && depth > 0) {
                if (s[j] === "{") depth++;
                else if (s[j] === "}") { depth--; if (depth === 0) break; }
                j++;
            }
            const field = s.slice(i + 1, j);
            i = j + 1;
            // Split field into name, `!conversion`, and `:spec` at bracket depth 0.
            let bdepth = 0, colonIdx = -1, bangIdx = -1;
            for (let k = 0; k < field.length; k++) {
                const fc = field[k];
                if (fc === "[") bdepth++;
                else if (fc === "]") { if (bdepth > 0) bdepth--; }
                else if (bdepth === 0) {
                    if (fc === ":") { colonIdx = k; break; }
                    if (fc === "!" && bangIdx < 0) bangIdx = k;
                }
            }
            let namePart = colonIdx >= 0 ? field.slice(0, colonIdx) : field;
            let spec = colonIdx >= 0 ? field.slice(colonIdx + 1) : null;
            let conv = null;
            if (bangIdx >= 0 && (colonIdx < 0 || bangIdx < colonIdx)) {
                conv = namePart[bangIdx + 1];
                namePart = namePart.slice(0, bangIdx);
            }
            let val = resolveField(namePart);
            if (conv === "r") val = pyRepr(val);
            else if (conv === "s") val = pyStr(val);
            else if (conv === "a") val = pyRepr(val); // ascii(): non-ASCII escaping is a documented approximation
            if (spec != null && spec !== "") {
                // Resolve a nested (dynamic) spec's own fields first.
                if (spec.includes("{")) spec = spec.replace(/\{([^{}]*)\}/g, (_, k) => pyStr(resolveField(k)));
                out += pyFormatSpec(val, parseFormatSpec(spec));
            } else if (conv != null) {
                // `!r`/`!s`/`!a` already produced a string; an empty spec is str().
                out += val;
            } else {
                out += pyStr(val);
            }
        } else if (ch === "}") {
            if (s[i + 1] === "}") { out += "}"; i += 2; continue; }
            out += "}"; i++;
        } else {
            out += ch; i++;
        }
    }
    return out;
}

// ----- list helpers -----

/** Python `xs.remove(v)` — removes first occurrence; raises ValueError if absent. */
export function pyListRemove(xs, v) {
    const i = xs.indexOf(v);
    if (i < 0) throw new ValueError("list.remove(x): x not in list");
    xs.splice(i, 1);
}

/** #301: Python `xs.append(v)` — receiver-dispatched Hybrid fallback.
 * Provably-list receivers inline to `.push` at compile time; everything
 * else lands here: arrays get Python list.append (returns None), any
 * other receiver with its own native append (DOM ParentNode.append,
 * user classes) dispatches to it. */
export function pyAppend(xs, v) {
    if (Array.isArray(xs)) { xs.push(v); return; }
    if (xs != null && typeof xs.append === "function") return xs.append(v);
    throw new TypeError_(`object of type '${typeof xs}' has no append()`);
}

/** #301: Python `xs.extend(iterable)` — receiver-dispatched. */
export function pyExtend(xs, other) {
    if (Array.isArray(xs)) {
        for (const v of pyForIter(other)) xs.push(v);
        return;
    }
    if (xs != null && typeof xs.extend === "function") return xs.extend(other);
    throw new TypeError_(`object of type '${typeof xs}' has no extend()`);
}

/** #301: Python `xs.insert(i, v)` — receiver-dispatched. JS splice
 * clamps negative/overflow indices the same way CPython does. */
export function pyInsert(xs, i, v) {
    if (Array.isArray(xs)) { xs.splice(i, 0, v); return; }
    if (xs != null && typeof xs.insert === "function") return xs.insert(i, v);
    throw new TypeError_(`object of type '${typeof xs}' has no insert()`);
}

/** #301: Python `s.find(sub[, start[, end]])` for strings — full CPython
 * semantics including negative/clamped start/end (the old Rename→indexOf
 * ignored `end`). Non-string receivers with their own .find (JS
 * Array.prototype.find(callback)) dispatch natively. */
export function pyFind(s, sub, start, end) {
    if (typeof s === "string") return __pyStrFind(s, sub, start, end, false);
    if (s != null && typeof s.find === "function") return s.find(sub, start, end);
    throw new TypeError_(`object of type '${typeof s}' has no find()`);
}

/** #301: Python `s.discard(v)` — Set removes-if-present; non-Set
 * receivers with their own .discard dispatch natively. */
export function pyDiscard(s, v) {
    if (s instanceof Set) { s.delete(v); return; }
    if (s != null && typeof s.discard === "function") return s.discard(v);
    throw new TypeError_(`object of type '${typeof s}' has no discard()`);
}

/** Python `xs.count(v)` — number of occurrences. Hybrid fallback. */
export function pyListCount(xs, v) {
    let n = 0;
    for (const x of xs) if (pyEq(x, v)) n++; // #241: bool≡int, tuple/__eq__
    return n;
}

/** Python `xs.clear()` — Hybrid fallback for complex receivers. */
export function pyListClear(xs) {
    xs.length = 0;
}

// ----- combined index / pop (work on strings and lists) -----

/** Python `obj.index(v)` — works for str and list; raises ValueError if absent. */
export function pyIndex(obj, v, start, end) {
    // Custom receivers with their own index (deque, user classes).
    if (obj != null && typeof obj !== "string" && !Array.isArray(obj)
        && typeof obj.index === "function") {
        return obj.index(v, start, end);
    }
    if (typeof obj === "string") {
        // str.index(sub[, start[, end]]) — code-point offsets, ValueError if absent.
        const i = __pyStrFind(obj, v, start, end, false);
        if (i < 0) throw new ValueError(`substring not found`);
        return i;
    }
    // list/tuple.index(x[, start[, end]]) — value equality (bool≡int, tuple/__eq__).
    const n = obj.length;
    let lo = start == null ? 0 : (start < 0 ? Math.max(n + start, 0) : start);
    const hi = end == null ? n : (end < 0 ? Math.max(n + end, 0) : Math.min(end, n));
    for (let i = lo; i < hi; i++) if (pyEq(obj[i], v)) return i;
    throw new ValueError(`${JSON.stringify(v)} is not in list`);
}

/** Python `pop` — ambiguous between list (0 or 1 args) and dict (1 or 2 args). */
export function pyPop(obj, ...rest) {
    // Custom receivers with their own pop (deque, user classes) — but not
    // Map subclasses (dict-style pop below handles Counter etc.).
    if (obj != null && !Array.isArray(obj) && !(obj instanceof Map)
        && typeof obj.pop === "function") {
        return obj.pop(...rest);
    }
    if (Array.isArray(obj)) {
        // #346: CPython list.pop() bounds-checks — an empty list or an
        // out-of-range index raises IndexError (JS Array.pop()/splice() would
        // silently return undefined).
        const n = obj.length;
        if (n === 0) throw new IndexError("pop from empty list");
        let idx = rest.length === 0 ? -1 : rest[0];
        if (typeof idx === "boolean") idx = idx ? 1 : 0;
        if (typeof idx === "bigint") {
            if (idx >= -9007199254740991n && idx <= 9007199254740991n) idx = Number(idx);
            else throw new IndexError("pop index out of range");
        }
        if (idx < 0) idx += n;
        if (idx < 0 || idx >= n) throw new IndexError("pop index out of range");
        return obj.splice(idx, 1)[0];
    }
    // dict (Map-backed)
    if (obj instanceof Map) {
        const k = rest[0];
        if (obj.has(k)) {
            const v = obj.get(k);
            obj.delete(k);
            return v;
        }
        if (rest.length >= 2) return rest[1];
        throw new KeyError(k);
    }
    // dict (plain object)
    const k = rest[0];
    if (Object.prototype.hasOwnProperty.call(obj, k)) {
        const v = obj[k];
        delete obj[k];
        return v;
    }
    if (rest.length >= 2) return rest[1];
    throw new KeyError(k);
}

// ----- dict helpers -----

// ============================================================
// PyDict — Map-backed dict for non-string keys (issue #83).
//
// The hybrid representation: dict literals / comprehensions whose keys
// are all provably strings stay plain JS objects (full JS interop —
// React props, JSON, spread). Any literal/comprehension with a
// non-string (or not-provably-string) key compiles to `new PyDict(...)`
// instead, which preserves key type/identity through iteration & repr.
//
// Key canonicalization matches CPython hash-equality:
//   True/False fold with 1/0; 1.0 folds with 1 (same JS Number);
//   safe-range BigInts fold with Numbers; tuples hash by structure
//   (encoded to a sentinel string internally — original tuple objects
//   are kept in a side table so keys()/entries() return them).
// CPython keeps the FIRST-inserted key object; we mirror that for the
// shapes we can distinguish (True vs 1, tuples). NaN keys: JS Map
// folds every NaN into one key (SameValueZero) — CPython distinguishes
// different NaN objects by identity; documented best-effort divergence.
// ============================================================

const __TUPKEY = "\u0000pytuple\u0000";
let __objIdCounter = 0;
const __objIds = new WeakMap();
function __objId(o) {
    let id = __objIds.get(o);
    if (id === undefined) { id = ++__objIdCounter; __objIds.set(o, id); }
    return id;
}

function __encodeTupleKey(t) {
    let s = "(";
    for (const el of t) {
        if (el === null || el === undefined) s += "N;";
        else if (typeof el === "boolean") s += "n:" + (el ? 1 : 0) + ";";
        else if (typeof el === "number" || typeof el === "bigint") s += "n:" + String(el) + ";";
        else if (typeof el === "string") s += "s:" + el.length + ":" + el + ";";
        else if (Array.isArray(el) && el.__pytuple__) s += __encodeTupleKey(el) + ";";
        else if (Array.isArray(el)) throw new TypeError_("unhashable type: 'list'");
        else if (el instanceof Set) throw new TypeError_("unhashable type: 'set'");
        else if (el instanceof Map || __isPlainObj(el)) throw new TypeError_("unhashable type: 'dict'");
        else s += "o:" + __objId(el) + ";"; // class instances: identity, like CPython default __hash__
    }
    return s + ")";
}

const __isPlainObj = (x) => {
    if (x === null || typeof x !== "object") return false;
    const p = Object.getPrototypeOf(x);
    return p === Object.prototype || p === null;
};

/** Canonical Map key for a Python dict key (CPython hash-equality). */
function __pyKey(k) {
    if (typeof k === "boolean") return k ? 1 : 0;
    if (typeof k === "bigint") {
        // crit-16: fold with the equal Number when k is exactly representable
        // as a double, so int 2**53 and float 2.0**53 share one set/dict key
        // (CPython: {2**53, 2.0**53} has len 1). 2**53+1 is not representable
        // as a double → stays a distinct BigInt (CPython keeps them distinct).
        const n = Number(k);
        return (Number.isFinite(n) && BigInt(n) === k) ? n : k;
    }
    if (Array.isArray(k)) {
        if (k.__pytuple__) return __TUPKEY + __encodeTupleKey(k);
        throw new TypeError_("unhashable type: 'list'");
    }
    if (k instanceof Set) throw new TypeError_("unhashable type: 'set'");
    if (k instanceof Map || __isPlainObj(k)) throw new TypeError_("unhashable type: 'dict'");
    return k; // string | number (int/float fold naturally) | null | class instance (identity)
}

// PyDict -> Map(canonicalKey -> original key object) for keys whose
// canonical form differs displayably (True/False, tuples). WeakMap keeps
// the side table off the instance (no own props → JSON.stringify-safe).
const __pyKeyObjs = new WeakMap();

export class PyDict extends Map {
    constructor(src) {
        super();
        if (src != null) {
            if (src instanceof Map) {
                for (const [k, v] of src.entries()) this.set(k, v);
            } else if (typeof src[Symbol.iterator] === "function") {
                for (const [k, v] of src) this.set(k, v);
            } else {
                for (const k of Object.keys(src)) this.set(k, src[k]);
            }
        }
    }
    set(k, v) {
        const c = __pyKey(k);
        // CPython keeps the FIRST-inserted key object on re-assignment.
        if ((typeof k === "boolean" || Array.isArray(k)) && !super.has(c)) {
            let m = __pyKeyObjs.get(this);
            if (!m) { m = new Map(); __pyKeyObjs.set(this, m); }
            m.set(c, k);
        }
        super.set(c, v);
        return this;
    }
    get(k) { return super.get(__pyKey(k)); }
    has(k) { return super.has(__pyKey(k)); }
    delete(k) {
        const c = __pyKey(k);
        const m = __pyKeyObjs.get(this);
        if (m) m.delete(c);
        return super.delete(c);
    }
    clear() {
        const m = __pyKeyObjs.get(this);
        if (m) m.clear();
        super.clear();
    }
    __key(c) {
        const m = __pyKeyObjs.get(this);
        return (m && m.has(c)) ? m.get(c) : c;
    }
    *keys() { for (const c of super.keys()) yield this.__key(c); }
    *entries() { for (const [c, v] of super.entries()) yield [this.__key(c), v]; }
    // Python semantics: iter(dict) yields KEYS (list(d), [*d], for k in d).
    // Generic Map consumers must use .entries() explicitly.
    *[Symbol.iterator]() { yield* this.keys(); }
    forEach(fn, thisArg) { for (const [k, v] of this.entries()) fn.call(thisArg, v, k, this); }
}

/**
 * #297: canonicalizing set — CPython hash-equality for elements.
 * `hash(True) == hash(1) == hash(1.0)` so `{1, True, 1.0}` has ONE
 * element; tuples hash structurally so `(1, 2) in {(1, 2)}` is True.
 * Same `__pyKey` canonical form + first-inserted-original side table
 * (`__pyKeyObjs`, shared WeakMap — instances are distinct keys) as
 * PyDict. Storage holds canonical values; iteration yields the
 * original forms (CPython keeps the FIRST-inserted object: `{1, True}`
 * prints `{1}`, `{True, 1}` prints `{True}`).
 * Extends Set, so every existing `instanceof Set` shape dispatch
 * (pyLen, pyRepr, pyIn, operators, pyType) works unchanged.
 */
export class PySet extends Set {
    constructor(src) {
        super();
        if (src != null) for (const v of src) this.add(v);
    }
    add(v) {
        const c = __pyKey(v);
        if ((typeof v === "boolean" || Array.isArray(v)) && !super.has(c)) {
            let m = __pyKeyObjs.get(this);
            if (!m) { m = new Map(); __pyKeyObjs.set(this, m); }
            m.set(c, v);
        }
        super.add(c);
        return this;
    }
    has(v) { return super.has(__pyKey(v)); }
    delete(v) {
        const c = __pyKey(v);
        const m = __pyKeyObjs.get(this);
        if (m) m.delete(c);
        return super.delete(c);
    }
    clear() {
        const m = __pyKeyObjs.get(this);
        if (m) m.clear();
        super.clear();
    }
    __orig(c) {
        const m = __pyKeyObjs.get(this);
        return (m && m.has(c)) ? m.get(c) : c;
    }
    *values() { for (const c of super.values()) yield this.__orig(c); }
    keys() { return this.values(); }
    *entries() { for (const c of super.values()) { const o = this.__orig(c); yield [o, o]; } }
    *[Symbol.iterator]() { yield* this.values(); }
    forEach(fn, thisArg) { for (const v of this.values()) fn.call(thisArg, v, v, this); }
}

/** Python `set(iterable)` / `set()` / `frozenset(iterable)` builtin —
 * canonicalizing constructor; iterates dicts as KEYS via pyForIter.
 * (frozenset immutability is not enforced — documented deviation.) */
export function pySetOf(it) {
    if (it === undefined) return new PySet();
    return new PySet(pyForIter(it));
}

/** Non-enumerable tuple marker (local twin of operators.js pyTuple —
 * runtime.js cannot import operators.js without a cycle). */
function __markTuple(items) {
    Object.defineProperty(items, "__pytuple__", { value: true, enumerable: false });
    return items;
}

/** Python `dict()` builtin. Shape-chooses at runtime: all-string keys →
 * plain object (JS interop; also the documented PyDict→JS escape hatch),
 * any non-string key → PyDict. */
export function pyDict(src, kwargs) {
    const entries = [];
    if (src != null) {
        if (src instanceof Map) { for (const [k, v] of src.entries()) entries.push([k, v]); }
        // #284: mapping protocol (keys() + __getitem__) — ChainMap and other
        // non-Map mappings. CPython's dict() special-cases objects exposing
        // keys(); must precede the generic-iterable branch since such a mapping
        // also iterates (yielding KEYS, which the pair-unpack below would break).
        else if (typeof src.keys === "function" && typeof src.__getitem__ === "function") {
            for (const k of src.keys()) entries.push([k, src.__getitem__(k)]);
        }
        else if (typeof src[Symbol.iterator] === "function" && typeof src !== "string") {
            for (const pair of src) entries.push([pair[0], pair[1]]);
        } else if (typeof src === "object") {
            for (const k of Object.keys(src)) entries.push([k, src[k]]);
        } else {
            throw new TypeError_(`'${typeof src}' object is not iterable`);
        }
    }
    if (kwargs != null) for (const k of Object.keys(kwargs)) entries.push([k, kwargs[k]]);
    if (entries.every(([k]) => typeof k === "string")) {
        const out = {};
        for (const [k, v] of entries) {
            if (k === "__proto__") Object.defineProperty(out, k, { value: v, writable: true, enumerable: true, configurable: true });
            else out[k] = v;
        }
        return out;
    }
    return new PyDict(entries);
}

/**
 * Python subscript write `obj[key] = value` — shape-dispatched twin of
 * pyGetItem. Lists get Python index semantics (negative index,
 * IndexError out of range); Maps/PyDicts use .set (canonicalized);
 * `__setitem__` dispatches; plain objects assign (proto-safe); anything
 * else (DOM wrappers, class instances, interop) passes through natively.
 */
export function pySetItem(obj, key, value) {
    if (obj == null) throw new TypeError_("'NoneType' object does not support item assignment");
    if (Array.isArray(obj)) {
        // crit-7: tuples are immutable (they're __pytuple__-tagged arrays).
        if (obj.__pytuple__) throw new TypeError_("'tuple' object does not support item assignment");
        let i = typeof key === "boolean" ? (key ? 1 : 0) // #258: bool ⊂ int
            : typeof key === "bigint" ? Number(key) : key;
        if (typeof i === "number" && Number.isInteger(i)) {
            if (i < 0) i += obj.length;
            if (i < 0 || i >= obj.length) throw new IndexError("list assignment index out of range");
            obj[i] = value;
            return;
        }
        throw new TypeError_("list indices must be integers or slices");
    }
    // #297: hand PyDict the ORIGINAL key — its own __pyKey canonicalizes
    // (bool→0/1 included) AND records the first-inserted form for repr.
    // The old top-level bool pre-coercion (#258) destroyed that record
    // (`d[True] = x` printed `{1: x}`, CPython keeps `{True: x}`).
    if (obj instanceof Map) { obj.set(key, value); return; }
    if (typeof key === "boolean") key = key ? 1 : 0; // #258: bool ⊂ int (plain shapes)
    if (typeof obj.__setitem__ === "function") { obj.__setitem__(key, value); return; }
    if (__isPlainObj(obj) && key === "__proto__") {
        // F3 sibling: a computed `__proto__` write must create a real own
        // key, not mutate the prototype.
        Object.defineProperty(obj, "__proto__", { value, writable: true, enumerable: true, configurable: true });
        return;
    }
    obj[key] = value;
}

/** Python `d.keys()` (and bare dict iteration) — shape-dispatched, returns
 * an array. Map/PyDict/FormData/URLSearchParams use their real .keys();
 * plain objects use Object.keys. */
export function pyDictKeys(d) {
    if (d == null) throw new TypeError_("'NoneType' object is not iterable");
    if (!Array.isArray(d) && typeof d.keys === "function") return [...d.keys()];
    return Object.keys(d);
}

// #266: a Python method accessed as a VALUE (`g = d.get`, `key=d.get`) is a
// BOUND method — it carries its receiver. In JS a detached `d.get` loses `this`.
// Bind a native method to its receiver; synthesize a closure for a plain-object
// dict's methods (which are lowered, not real properties). Gated so a class
// instance's data field named like a dict method is returned as-is.
export function pyBoundMethod(obj, name) {
    if (obj == null) return obj && obj[name];
    if (typeof obj[name] === "function") return obj[name].bind(obj);
    const isDict =
        obj instanceof Map ||
        (typeof obj === "object" && !Array.isArray(obj) && Object.getPrototypeOf(obj) === Object.prototype);
    if (isDict) {
        switch (name) {
            case "get": return (k, d) => pyDictGet(obj, k, d);
            case "keys": return () => pyDictKeys(obj);
            case "values": return () => pyDictValues(obj);
            case "items": return () => pyDictItems(obj);
            case "pop": return (...a) => pyPop(obj, ...a);
        }
    }
    return obj[name];
}

// #239: iterate an operand of UNKNOWN static type in a `for x in it:` — Python
// iterates dict KEYS, list/tuple/set elements, and string code points. JS
// `for..of` handles arrays/strings/sets/generators, but a plain object (an
// untyped dict param) is not iterable at all, and a Map (PyDict) would yield
// [k,v] entries instead of keys. Route both dict shapes to their keys; pass
// everything else through untouched (fast path stays for..of over the value).
export function pyForIter(x) {
    if (x == null) throw new TypeError_("'NoneType' object is not iterable");
    if (typeof x === "string" || Array.isArray(x) || x instanceof Set) return x;
    if (x instanceof Map) return x.keys(); // dict keys (Counter/defaultdict too)
    if (typeof x[Symbol.iterator] === "function") return x; // generators / iterables
    // Async iterables (async generators) pass through untouched — `async for`
    // wraps the iterable in pyForIter then __pyAsyncIter, and an async gen has
    // no Symbol.iterator, so without this it fell to Object.keys() → [] (the
    // async-comprehension returned empty; #289 r4_a_async_comp).
    if (typeof x[Symbol.asyncIterator] === "function") return x;
    if (typeof x === "object") return Object.keys(x); // plain-object dict → keys
    return x;
}

/** Python `d.values()` — shape-dispatched, returns an array. */
export function pyDictValues(d) {
    if (d == null) throw new TypeError_("'NoneType' object is not iterable");
    if (!Array.isArray(d) && typeof d.values === "function") return [...d.values()];
    return Object.values(d);
}

/** Python `d.items()` — shape-dispatched; returns an array of (k, v)
 * tuples (pyTuple-marked so repr shows `(k, v)` like CPython). */
export function pyDictItems(d) {
    if (d == null) throw new TypeError_("'NoneType' object is not iterable");
    if (!Array.isArray(d) && typeof d.entries === "function") {
        return [...d.entries()].map((pair) => __markTuple([pair[0], pair[1]]));
    }
    return Object.entries(d).map((pair) => __markTuple(pair));
}

/** Dict-literal merge `{**a, "k": v, **b}` — shape-dispatched. Result is
 * a PyDict when any part is Map-backed, else a plain object (today's
 * behavior, proto-safe). Preserves left-to-right insertion order. */
/**
 * Round-2 pythonic sweep: Python keyword-argument binding for plain
 * user functions. Codegen attaches `fn.__pyparams__` (positional
 * parameter names, in order) and `fn.__pykw__` (truthy iff the
 * function has a **kwargs catch-all as its final JS parameter) at
 * definition time; call sites with keyword arguments route through
 * here. Functions WITHOUT metadata (JS interop, React components,
 * class methods) keep the legacy trailing-options-object convention.
 */
export function __pyCallKw(fn, pos, kw) {
    return fn(...__pyKwArgs(fn, pos, kw));
}

/**
 * Round-3 pythonic sweep: the argument-ARRAY builder behind Python
 * keyword binding, shared by plain calls (__pyCallKw), constructor
 * calls (`new Cls(...__pyKwArgs(Cls, pos, kw))`), and method calls
 * (`obj.m(...__pyKwArgs(obj.m, pos, kw))` — spreading at the call site
 * keeps `this`). `fn` supplies __pyparams__/__pykw__ metadata and a
 * name for error messages; without metadata the legacy trailing
 * options-object convention is preserved (JS interop, components).
 */
export function __pyKwArgs(fn, pos, kw) {
    const entries = kw instanceof Map ? Array.from(kw.entries()) : Object.entries(kw);
    const names = fn ? fn.__pyparams__ : undefined;
    if (!names) {
        const legacy = {};
        for (const [k, v] of entries) legacy[k] = v;
        return [...pos, legacy];
    }
    const fname = (fn && fn.name) || "function";
    const args = pos.slice();
    let rest = null;
    for (const [k, v] of entries) {
        const idx = names.indexOf(k);
        if (idx >= 0) {
            if (idx < pos.length) {
                throw new TypeError(`${fname}() got multiple values for argument '${k}'`);
            }
            args[idx] = v;
        } else if (fn.__pykw__) {
            (rest = rest || {})[k] = v;
        } else {
            throw new TypeError(`${fname}() got an unexpected keyword argument '${k}'`);
        }
    }
    if (fn.__pykw__) args[names.length] = rest || {};
    return args; // sparse holes spread as undefined -> JS defaults apply
}

export function pyDictMerge(...parts) {
    if (parts.some((p) => p instanceof Map)) {
        const out = new PyDict();
        for (const p of parts) {
            if (p == null) continue;
            if (p instanceof Map) { for (const [k, v] of p.entries()) out.set(k, v); }
            else { for (const k of Object.keys(p)) out.set(k, p[k]); }
        }
        return out;
    }
    const out = {};
    for (const p of parts) {
        if (p == null) continue;
        for (const k of Object.keys(p)) {
            if (k === "__proto__") Object.defineProperty(out, k, { value: p[k], writable: true, enumerable: true, configurable: true });
            else out[k] = p[k];
        }
    }
    return out;
}

/** Python `d.get(k)` / `d.get(k, default)` — Hybrid fallback. */
export function pyDictGet(d, k, defaultValue) {
    if (d instanceof Map) return d.has(k) ? d.get(k) : defaultValue;
    // #301: a non-dict receiver with its OWN native .get (FormData,
    // URLSearchParams, Headers, user classes) must dispatch to it — the
    // own-key probe below silently returned undefined for all of these.
    // Plain objects stay on the dict path: a dict key named "get" is data,
    // not a method. Missing-key result (null/undefined) honors the Python
    // default when one was given.
    if (d != null && !__isPlainObj(d) && typeof d.get === "function") {
        const r = d.get(k);
        return r == null && defaultValue !== undefined ? defaultValue : r;
    }
    // F3: own-key check so inherited prototype members (`hasOwnProperty`,
    // `toString`, `__proto__`, ...) don't masquerade as present keys.
    return (d != null && Object.prototype.hasOwnProperty.call(d, k)) ? d[k] : defaultValue;
}

/** Python `d.setdefault(k, default)` — sets if missing, returns current. */
export function pyDictSetdefault(d, k, defaultValue) {
    if (d instanceof Map) {
        if (d.has(k)) return d.get(k);
        d.set(k, defaultValue);
        return defaultValue;
    }
    if (Object.prototype.hasOwnProperty.call(d, k)) return d[k];
    d[k] = defaultValue;
    return defaultValue;
}

// ============================================================
// Multi-receiver helpers — dispatch on receiver type at runtime.
// Used for method names that exist on multiple Python types
// (count, clear, copy, remove) where the codegen has no static
// type info to pick a per-type lowering.
// ============================================================

/** Python `obj.count(v)` for str/list/tuple. */
export function pyCount(obj, v) {
    if (typeof obj === "string") {
        if (typeof v !== "string") return 0;
        // #327: the empty substring matches at every gap (before each char
        // and at the end) → len(s)+1, counted in code points (astral-safe).
        if (v.length === 0) {
            return (/[\uD800-\uDBFF]/.test(obj) ? [...obj].length : obj.length) + 1;
        }
        let n = 0, i = 0;
        while ((i = obj.indexOf(v, i)) !== -1) { n++; i += v.length; }
        return n;
    }
    if (Array.isArray(obj)) return pyListCount(obj, v);
    // Custom receivers with their own count (deque, user classes).
    if (obj != null && typeof obj.count === "function") return obj.count(v);
    throw new TypeError_(`object of type '${typeof obj}' has no count()`);
}

/** Python `obj.clear()` for list/dict/set. */
export function pyClear(obj) {
    if (Array.isArray(obj)) { obj.length = 0; return; }
    if (obj instanceof Set || obj instanceof Map) { obj.clear(); return; }
    // Custom receivers with their own clear (deque, user classes).
    if (obj != null && !__isPlainObj(obj) && typeof obj.clear === "function") { obj.clear(); return; }
    if (obj && typeof obj === "object") {
        for (const k of Object.keys(obj)) delete obj[k];
        return;
    }
    throw new TypeError_(`object of type '${typeof obj}' has no clear()`);
}

/** Python `obj.copy()` for list/dict/set. Shallow copy. */
export function pyCopy(obj) {
    if (Array.isArray(obj)) return obj.slice();
    // #297: subclass-preserving — a canonicalizing PySet copies to a PySet
    // (a plain interop Set stays plain).
    if (obj instanceof Set) return new (obj.constructor)(obj);
    // Custom receivers with their own copy (deque, Counter, OrderedDict,
    // defaultdict, user classes) — keeps the subclass type, like CPython.
    if (obj != null && typeof obj.copy === "function") return obj.copy();
    if (obj instanceof PyDict) return new PyDict(obj);
    if (obj instanceof Map) return new Map(obj.entries());
    if (obj && typeof obj === "object") return { ...obj };
    throw new TypeError_(`object of type '${typeof obj}' has no copy()`);
}

/**
 * Normalize a JSX `style` object by converting every snake_case key to
 * camelCase. Used by the codegen when a `style={...}` prop is bound
 * to a variable rather than a Dict literal — the literal case is
 * compile-time-rewritten in `emit_psx_element`, but variable-bound
 * style objects need runtime normalization since React silently
 * ignores unknown camelCase-only properties.
 *
 * Accepts:
 *   pyNormalizeStyle({ border_radius: "6px", padding: "8px" })
 *     → { borderRadius: "6px", padding: "8px" }
 *
 * Returns the same reference if no keys needed converting (cheap
 * fast-path), else returns a new object. Non-object inputs pass
 * through unchanged.
 */
export function pyNormalizeStyle(style) {
    if (style == null || typeof style !== "object") return style;
    let out = null;
    for (const k of Object.keys(style)) {
        if (k.indexOf("_") < 0) {
            // Already camelCase or no underscore — fast path.
            if (out !== null) out[k] = style[k];
            continue;
        }
        // Convert _X → X.toUpperCase() (generic snake→camel).
        // Preserve leading dashes (CSS custom props: --my-var) and
        // vendor prefixes (-webkit-foo) — those legitimately use `-`,
        // not `_`, so we only act on `_` separators.
        const camel = k.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
        if (out === null) {
            out = {};
            for (const prev of Object.keys(style)) {
                if (prev === k) break;
                out[prev] = style[prev];
            }
        }
        out[camel] = style[k];
    }
    return out === null ? style : out;
}

/** Python `obj.update(...others)` — dict merges, set unions. */
export function pyUpdate(obj, ...others) {
    if (obj instanceof Set) {
        for (const o of others) for (const v of o) obj.add(v);
        return;
    }
    // Custom receivers with their own update (Counter adds COUNTS, user
    // classes) — must win over the generic Map merge below.
    if (obj != null && !__isPlainObj(obj) && typeof obj.update === "function") {
        for (const o of others) obj.update(o);
        return;
    }
    if (obj instanceof Map) {
        // Explicit .entries(): PyDict's default iterator yields KEYS
        // (Python semantics), so implicit Map iteration would misfire.
        for (const o of others) {
            for (const [k, v] of (o instanceof Map ? o.entries() : Object.entries(o))) obj.set(k, v);
        }
        return;
    }
    if (obj && typeof obj === "object") {
        for (const o of others) {
            if (o instanceof Map) {
                // Plain-object receiver updated from a Map-backed dict:
                // keys pass through JS property coercion (the documented
                // plain-shape residual for non-string keys).
                for (const [k, v] of o.entries()) obj[k] = v;
            } else {
                Object.assign(obj, o);
            }
        }
        return;
    }
    throw new TypeError_(`object of type '${typeof obj}' has no update()`);
}

/** Python `obj.remove(v)` for list/set. List: first occurrence by ===.
 * Set: must be present; raises KeyError otherwise. */
export function pyRemove(obj, v) {
    if (Array.isArray(obj)) return pyListRemove(obj, v);
    if (obj instanceof Set) {
        if (!obj.has(v)) throw new KeyError(v);
        obj.delete(v);
        return;
    }
    // Custom receivers with their own remove (deque, user classes).
    if (obj != null && typeof obj.remove === "function") return obj.remove(v);
    throw new TypeError_(`object of type '${typeof obj}' has no remove()`);
}

// ============================================================
// String helpers (additional)
// ============================================================

/** Python `s.center(width, fillchar=" ")` — pad both sides to reach width. */
export function pyStrCenter(s, width, fillchar = " ") {
    const need = width - s.length;
    if (need <= 0) return s;
    // #328: CPython puts the odd extra pad on the LEFT when BOTH the margin
    // and the width are odd (`left = marg//2 + (marg & width & 1)`), not
    // always on the right.
    const left = Math.floor(need / 2) + (need & width & 1);
    const right = need - left;
    return fillchar.repeat(left) + s + fillchar.repeat(right);
}

/** Python `s.ljust(width, fillchar=" ")` — pad right to width. */
export function pyStrLjust(s, width, fillchar = " ") {
    return s.length >= width ? s : s + fillchar.repeat(width - s.length);
}

/** Python `s.rjust(width, fillchar=" ")` — pad left to width. */
export function pyStrRjust(s, width, fillchar = " ") {
    return s.length >= width ? s : fillchar.repeat(width - s.length) + s;
}

/** Python `s.expandtabs(tabsize=8)` — replace tabs with spaces. */
export function pyStrExpandtabs(s, tabsize = 8) {
    let out = "";
    let col = 0;
    for (const ch of s) {
        if (ch === "\t") {
            const fill = tabsize - (col % tabsize);
            out += " ".repeat(fill);
            col += fill;
        } else if (ch === "\n" || ch === "\r") {
            out += ch;
            col = 0;
        } else {
            out += ch;
            col++;
        }
    }
    return out;
}

/** Python `s.partition(sep)` — split into (before, sep, after) at first match. */
export function pyStrPartition(s, sep) {
    if (!sep) throw new ValueError("empty separator");
    const i = s.indexOf(sep);
    if (i === -1) return [s, "", ""];
    return [s.slice(0, i), sep, s.slice(i + sep.length)];
}

/** Python `s.rpartition(sep)` — same as partition but from the right. */
export function pyStrRpartition(s, sep) {
    if (!sep) throw new ValueError("empty separator");
    const i = s.lastIndexOf(sep);
    if (i === -1) return ["", "", s];
    return [s.slice(0, i), sep, s.slice(i + sep.length)];
}

/** Python `s.rsplit(sep, maxsplit=-1)`. Empty separator raises
 * ValueError like CPython (issue #92). */
export function pyStrRsplit(s, sep, maxsplit = -1) {
    if (sep === undefined) {
        return s.trim().split(/\s+/).filter(Boolean);
    }
    if (sep === "") throw new ValueError("empty separator");
    if (maxsplit < 0) return s.split(sep);
    const parts = s.split(sep);
    if (parts.length <= maxsplit + 1) return parts;
    const head = parts.slice(0, parts.length - maxsplit).join(sep);
    return [head, ...parts.slice(parts.length - maxsplit)];
}

/** Python `s.splitlines(keepends=false)` — split on universal newlines. */
export function pyStrSplitlines(s, keepends = false) {
    if (s.length === 0) return [];
    const out = [];
    let start = 0;
    let i = 0;
    while (i < s.length) {
        const ch = s[i];
        if (ch === "\n" || ch === "\r") {
            const end = i;
            let next = i + 1;
            if (ch === "\r" && s[i + 1] === "\n") next = i + 2;
            out.push(keepends ? s.slice(start, next) : s.slice(start, end));
            start = next;
            i = next;
        } else {
            i++;
        }
    }
    if (start < s.length) out.push(s.slice(start));
    return out;
}

/** Python `s.swapcase()` — invert case of each ASCII letter. */
export function pyStrSwapcase(s) {
    let out = "";
    for (const ch of s) {
        const lo = ch.toLowerCase();
        const up = ch.toUpperCase();
        out += ch === lo ? up : lo;
    }
    return out;
}

/** Python `s.translate(table)` — table is {codepoint: replacement|null}. */
export function pyStrTranslate(s, table) {
    if (!table) return s;
    let out = "";
    for (const ch of s) {
        const cp = ch.codePointAt(0);
        const entry = table.get ? table.get(cp) : table[cp];
        if (entry === undefined) out += ch;
        else if (entry === null) { /* delete */ }
        else if (typeof entry === "number") out += String.fromCodePoint(entry);
        else out += String(entry);
    }
    return out;
}

/** Python `s.isidentifier()` — valid Python identifier (ASCII subset). */
export function pyStrIsidentifier(s) {
    return /^[A-Za-z_][A-Za-z0-9_]*$/.test(s);
}

/** Python `s.isprintable()` — only printable + space. */
export function pyStrIsprintable(s) {
    if (s.length === 0) return true;
    return [...s].every(ch => {
        const cp = ch.charCodeAt(0);
        return cp === 0x20 || (cp > 0x20 && cp !== 0x7F);
    });
}

/** Python `s.istitle()` — every word starts upper, rest lower. */
// #237: runtime forms of `str.isupper()`/`str.islower()` — the inline specs
// reference the receiver three times and so need a simple receiver; on a
// complex one (`s[i].isupper()`) codegen falls back to these. Semantics match
// the inline forms exactly: has at least one cased char and all cased chars
// share the case.
// #242: `str.replace(old, new, count)` — JS `.replaceAll` ignores the optional
// count. Honor it: at most `count` replacements (count<0 / absent → all).
export function pyStrReplace(s, oldv, newv, count) {
    // #301: non-string receivers with their own .replace (DOMTokenList —
    // `el.classList.replace(a, b)` — user classes) dispatch natively.
    if (typeof s !== "string" && s != null && typeof s.replace === "function") {
        return s.replace(oldv, newv);
    }
    // Wave-15 F7: bool ⊂ int — a bool count is its int value (CPython:
    // 'ab'.replace('', '-', False) == 'ab'). Normalize BEFORE the limit
    // checks; strict `count === 0` below would miss JS `false`.
    if (typeof count === "boolean") count = count ? 1 : 0;
    const unlimited = count === undefined || count === null || count < 0;
    if (oldv === "" ) {
        // Empty pattern: Python inserts `new` between every CODE POINT and
        // at both ends; cap at `count` insertions when limited. Wave-15 F9
        // SHIPPING BUG: this must iterate by code point ([...s], the same
        // astral-correct iteration the other string helpers use) — the old
        // `s.split("")` walked UTF-16 code units, splitting astral surrogate
        // pairs ('𝔸'.replace('', '-') must be '-𝔸-', not '-\ud835-\udd38-').
        // A limited count of 0 means zero insertions (CPython:
        // 'ab'.replace('', '-', 0) == 'ab'; the old code emitted a leading
        // `new` unconditionally).
        if (!unlimited && count === 0) return s;
        const chars = [...s]; // code points, not code units
        let out = newv, n = 1;
        for (let i = 0; i < chars.length; i++) {
            out += chars[i];
            if (unlimited || n < count) { out += newv; n++; }
        }
        return out;
    }
    if (unlimited) return s.split(oldv).join(newv);
    let out = "", idx = 0, done = 0;
    while (done < count) {
        const next = s.indexOf(oldv, idx);
        if (next === -1) break;
        out += s.slice(idx, next) + newv;
        idx = next + oldv.length;
        done++;
    }
    return out + s.slice(idx);
}

export function pyStrIsupper(s) {
    return s === s.toUpperCase() && s !== s.toLowerCase();
}
export function pyStrIslower(s) {
    return s === s.toLowerCase() && s !== s.toUpperCase();
}

export function pyStrIstitle(s) {
    if (s.length === 0) return false;
    let prevCased = false;
    let hasUpper = false;
    for (const ch of s) {
        const isUpper = ch >= "A" && ch <= "Z";
        const isLower = ch >= "a" && ch <= "z";
        if (isUpper) {
            if (prevCased) return false;
            hasUpper = true;
            prevCased = true;
        } else if (isLower) {
            if (!prevCased) return false;
            prevCased = true;
        } else {
            prevCased = false;
        }
    }
    return hasUpper;
}

/** Python `s.rfind(sub)` — LAST occurrence as a CODE-POINT offset, -1 if
 * absent. Wave-19 verification fix: raw .lastIndexOf returns UTF-16
 * code-unit offsets ('𝔸x𝔸x'.rfind('x') must be 3, not 5). */
export function pyStrRfind(s, sub, start, end) {
    if (typeof s !== "string" && s != null && typeof s.rfind === "function") {
        return s.rfind(sub, start, end);
    }
    return __pyStrFind(s, sub, start, end, true);
}

/** Python `s.rindex(sub[, start[, end]])` — like rfind but raises ValueError if absent. */
export function pyStrRindex(s, sub, start, end) {
    const i = pyStrRfind(s, sub, start, end);
    if (i === -1) throw new ValueError(`substring not found`);
    return i;
}

// ============================================================
// List helpers (additional)
// ============================================================

/** Python `xs.sort(key=None, reverse=False)` — in-place stable sort. */
export function pyListSort(xs, opts = {}) {
    const { key, reverse = false } = opts;
    // #247: match pySorted — seq-aware lexicographic keys + stable reverse
    // (negate the comparator, never sort-then-reverse, so ties keep order).
    const lt = (a, b) => {
        if (a !== null && typeof a?.__lt__ === "function") return !!a.__lt__(b);
        if (Array.isArray(a) && Array.isArray(b)) {
            const n = Math.min(a.length, b.length);
            for (let i = 0; i < n; i++) {
                if (lt(a[i], b[i])) return true;
                if (lt(b[i], a[i])) return false;
            }
            return a.length < b.length;
        }
        return a < b;
    };
    const cmp = (a, b) => (lt(a, b) ? -1 : lt(b, a) ? 1 : 0);
    const dir = reverse ? -1 : 1;
    if (key) xs.sort((a, b) => dir * cmp(key(a), key(b)));
    else xs.sort((a, b) => dir * cmp(a, b));
}

// ============================================================
// Dict helpers (additional)
// ============================================================

/** Python `d.popitem()` — remove + return last-inserted (key, value)
 * tuple. Plain JS objects and Maps both preserve insertion order. */
export function pyDictPopitem(d, lastArg) {
    // OrderedDict (and user classes) implement popitem(last=...) themselves.
    if (d != null && typeof d.popitem === "function") {
        return d.popitem(lastArg === undefined ? true : lastArg);
    }
    if (d instanceof Map) {
        if (d.size === 0) throw new KeyError("popitem(): dictionary is empty");
        let last;
        for (const pair of d.entries()) last = pair;
        d.delete(last[0]);
        return __markTuple([last[0], last[1]]);
    }
    const keys = Object.keys(d);
    if (keys.length === 0) throw new KeyError("popitem(): dictionary is empty");
    const k = keys[keys.length - 1];
    const v = d[k];
    delete d[k];
    return __markTuple([k, v]);
}

// ============================================================
// Set helpers
// ============================================================

/** Python `s.union(...others)` — new set; receiver unchanged.
 * #297: results are canonicalizing PySets (bool/int/float hash identity,
 * structural tuple membership); iterable `others` are canonicalized by
 * PySet's own add. */
export function pySetUnion(s, ...others) {
    const out = new PySet(s);
    for (const o of others) for (const v of o) out.add(v);
    return out;
}

/** Python `s.intersection(...others)` — new set; receiver unchanged.
 * #297: canonical membership (bool/int/float identity, structural tuples)
 * AND CPython element provenance — the result holds the element objects of
 * the ITERATED operand: for set-vs-set the smaller side is iterated (the
 * OTHER on a size tie); a non-set iterable is always iterated and its
 * elements kept ({1, 2}.intersection([True]) == {True}). Others chain
 * pairwise like CPython's set_intersection_multi. */
export function pySetIntersection(s, ...others) {
    let cur = s;
    let owned = false;
    for (const o of others) {
        const out = new PySet();
        if (o instanceof Set) {
            const [small, big] = o.size > cur.size ? [cur, o] : [o, cur];
            const bigHas = big instanceof PySet ? big : new PySet(big);
            for (const v of small) if (bigHas.has(v)) out.add(v);
        } else {
            const curHas = cur instanceof PySet ? cur : new PySet(cur);
            for (const v of o) if (curHas.has(v)) out.add(v);
        }
        cur = out;
        owned = true;
    }
    return owned ? cur : new PySet(cur);
}

/** Python `s.difference(...others)`. */
export function pySetDifference(s, ...others) {
    const out = new PySet(s);
    for (const o of others) {
        for (const v of o) out.delete(v);
    }
    return out;
}

/** Python `s.symmetric_difference(other)`. */
export function pySetSymmetricDifference(s, other) {
    const o = other instanceof Set ? other : new PySet(other);
    const out = new PySet();
    for (const v of s) if (!o.has(v)) out.add(v);
    for (const v of o) if (!s.has(v)) out.add(v);
    return out;
}

/** Python `s.intersection_update(...others)` — in-place. */
export function pySetIntersectionUpdate(s, ...others) {
    const keep = pySetIntersection(s, ...others);
    s.clear();
    for (const v of keep) s.add(v);
}

/** Python `s.difference_update(...others)` — in-place. */
export function pySetDifferenceUpdate(s, ...others) {
    for (const o of others) for (const v of o) s.delete(v);
}

/** Python `s.symmetric_difference_update(other)` — in-place. */
export function pySetSymmetricDifferenceUpdate(s, other) {
    const out = pySetSymmetricDifference(s, other);
    s.clear();
    for (const v of out) s.add(v);
}

/** Python `s.isdisjoint(other)`. */
export function pySetIsdisjoint(s, other) {
    const o = other instanceof Set ? other : new PySet(other);
    for (const v of s) if (o.has(v)) return false;
    return true;
}

/** Python `s.issubset(other)`. */
export function pySetIssubset(s, other) {
    const o = other instanceof Set ? other : new PySet(other);
    for (const v of s) if (!o.has(v)) return false;
    return true;
}

/** Python `s.issuperset(other)`. */
export function pySetIssuperset(s, other) {
    const o = other instanceof Set ? other : new PySet(other);
    for (const v of o) if (!s.has(v)) return false;
    return true;
}
