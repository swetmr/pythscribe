// PythScribe Runtime — Core helpers
// "Write Python. Ship to the Web."

// Round-4 sweep: exception construction needs Python's str()/repr() for
// its message and args-tuple handling. Deliberate import cycle with
// operators.js (which imports our exception classes) — both sides only
// dereference the bindings at call time, never during module evaluation,
// so ESM live bindings resolve it.
import {
    pyStr, pyRepr, pyTuple, pyEq, pyFormatFloat,
    pyInt, pyFloat, pyListOf, pyTupleOf, __pyF,
    pyBytesOf, pyBytearrayOf,
    // F1 (v0.2.4): pySorted/pyListSort/__minmax consolidate their local
    // raw-`<` comparators onto THE comparison authority (cross-type
    // TypeError guard, bytes ordering, dunder + reflected dispatch).
    // Call-time-only dereference, so the import cycle stays safe.
    pyLt, pyGt,
    // F6-r2 (v0.2.4): round()'s ndigits validates through THE
    // interpreted-as-integer authority (__index__ protocol) instead of
    // Math.trunc(Number(...)) coercion.
    __pyAsIndexInt,
} from "./operators.js";
import { pyBool } from "./types.js";

/**
 * Python-compatible range() generator.
 * range(stop), range(start, stop), range(start, stop, step)
 *
 * SEC-11 (CWE-400) guards. This helper materializes the whole range into an
 * array (see the note at the end of the function), so two argument shapes
 * that CPython rejects outright used to become unbounded work here:
 *
 *   - a NON-FINITE bound (`Infinity`/`NaN`, e.g. from `float("inf")` or a
 *     `0/0`-style computation) turned the fill loop into an infinite push
 *     loop — a guaranteed hang/heap exhaustion needing NO attacker-chosen
 *     size at all. CPython: `TypeError: 'float' object cannot be
 *     interpreted as an integer`.
 *   - an explicit ZERO step fell through `step || 1` and silently became 1,
 *     so `range(0, 10, 0)` quietly yielded 10 items. CPython:
 *     `ValueError: range() arg 3 must not be zero`.
 *
 * Only NON-FINITE numbers are rejected, not all floats: JS has a single
 * number type, so rejecting every non-integer would break ordinary compiled
 * arithmetic. See `experiments/codex-security-scan/poc/D-11.md`.
 */
// R2/R3/R4/R5 ROOT FIX: ONE guard/normalize source shared by BOTH the
// materializing `pyRange` and the lazy `__pyRangeIter` the codegen's optimized
// `for i in range(...)` loop iterates — so the fast path can NEVER diverge from
// the canonical semantics (the previous hand-rolled C-loop did: BigInt/Number
// mix crash, 2**53 non-progress hang, bool rejected).
//
// Normalizes bool→int and the 1/2/3-arg shape, then guards: zero step
// (ValueError), non-Number/BigInt bound (TypeError), non-finite Number
// (TypeError), and — matching CPython, which rejects float args — a
// non-integer-valued Number (TypeError). After this, every bound is an
// integer-valued Number or a BigInt, so `BigInt(v)` is always exact.
function __pyRangeNorm(startOrStop, stop, step) {
    const __b = (v) => (typeof v === "boolean" ? (v ? 1 : 0) : v); // bool ⊆ int
    startOrStop = __b(startOrStop); stop = __b(stop); step = __b(step);
    let start;
    if (stop === undefined) { start = 0; stop = startOrStop; step = 1; }
    else {
        start = startOrStop;
        if (step === undefined || step === null) step = 1;
        else if (step === 0 || step === 0n) throw new ValueError("range() arg 3 must not be zero");
    }
    const __numOrBig = (v) => typeof v === "number" || typeof v === "bigint";
    // Option B: a boxed float (8.0) is now distinguishable — reject it the
    // way CPython rejects ANY float in range() (the old whole-float
    // deviation only remains for un-boxed natives that leaked in via JS).
    for (const v of [start, stop, step]) {
        if (v != null && v.__pyfloat__ === true) {
            throw new TypeError_("'float' object cannot be interpreted as an integer");
        }
    }
    if (!__numOrBig(start) || !__numOrBig(stop) || !__numOrBig(step)) {
        const bad = !__numOrBig(start) ? start : !__numOrBig(stop) ? stop : step;
        // #467: type name from the ONE value-model source, not ad-hoc typeof.
        throw new TypeError_(`'${__pyTypeName(bad)}' object cannot be interpreted as an integer`);
    }
    // Non-finite and non-integer Numbers both raise (CPython rejects float args;
    // integer-valued Numbers like 5.0 / -1e308 pass as the documented deviation).
    for (const v of [start, stop, step]) {
        if (typeof v === "number" && !Number.isInteger(v)) {
            throw new TypeError_("'float' object cannot be interpreted as an integer");
        }
    }
    return { start, stop, step };
}

/** BigInt length of range(start, stop, step); bounds are integer-valued. */
function __pyRangeLen(start, stop, step) {
    const bs = BigInt(start), bt = BigInt(stop), bp = BigInt(step);
    return bp > 0n
        ? (bt > bs ? (bt - bs + bp - 1n) / bp : 0n)
        : (bs > bt ? (bs - bt + (-bp) - 1n) / (-bp) : 0n);
}

const __MAX_SAFE_BIG = 9007199254740991n; // Number.MAX_SAFE_INTEGER as BigInt

/**
 * Should this range yield BigInt values? Yes when ANY bound is already BigInt,
 * or when the Number loop `start + i*step` could lose exactness ANYWHERE —
 * not just at the endpoints. The loop's INTERMEDIATE product `i*step` reaches
 * `|last - start|`, so even with both endpoints safe (e.g.
 * `range(-2**53+1, 2**53-1, 3002399751580331)`) a span beyond 2**53-1 makes
 * `i*step` inexact and yields WRONG interior values (delta4). Number-loop
 * soundness proof: with |start|, |last| and |last-start| all ≤ 2**53-1,
 * `i*step` is an exact integer product (|i*step| ≤ |last-start|) and
 * `start + i*step` is an exact sum (every produced value lies between start
 * and last) — so every yielded value is exact. All three checks run in
 * BigInt, so the decision itself cannot round. `bs`/`bp` are BigInt bounds.
 */
function __pyRangeUseBig(start, stop, step, bs, bp, len) {
    if (typeof start === "bigint" || typeof stop === "bigint" || typeof step === "bigint") return true;
    if (len === 0n) return false;
    const last = bs + (len - 1n) * bp;
    const abs = (x) => (x < 0n ? -x : x);
    return abs(bs) > __MAX_SAFE_BIG || abs(last) > __MAX_SAFE_BIG
        || abs(last - bs) > __MAX_SAFE_BIG;
}

/**
 * LAZY Python range iterator — the single source the codegen's optimized
 * `for i in range(...)` loop iterates, so a huge finite range never
 * materializes yet the guards + counted (2**53-safe, BigInt-safe) stepping are
 * exactly pyRange's. No length guard here: iteration is lazy.
 */
export function* __pyRangeIter(startOrStop, stop, step) {
    const n = __pyRangeNorm(startOrStop, stop, step);
    const len = __pyRangeLen(n.start, n.stop, n.step);
    const bs = BigInt(n.start), bp = BigInt(n.step);
    // Strength-reduced (salvaged from Option A 06f3c353): one add per
    // iteration (v += step) instead of a mul+add — this generator drives
    // every optimized `for i in range(...)`. Exactness: on the Number arm
    // every accumulated value IS a yielded value, and __pyRangeUseBig
    // already guarantees all yielded values are safe integers.
    if (__pyRangeUseBig(n.start, n.stop, n.step, bs, bp, len)) {
        let v = bs;
        for (let c = 0n; c < len; c++, v += bp) yield v; // exact beyond 2**53
    } else {
        const count = Number(len);
        let v = n.start;
        for (let i = 0; i < count; i++, v += n.step) yield v;
    }
}

/**
 * Python-compatible range() — MATERIALIZES into an array (with the >2**32-1
 * length guard). Shares __pyRangeNorm/__pyRangeLen with __pyRangeIter, so the
 * fast lazy path cannot diverge from this canonical one.
 */
export function pyRange(startOrStop, stop, step) {
    const n = __pyRangeNorm(startOrStop, stop, step);
    const len = __pyRangeLen(n.start, n.stop, n.step);
    if (len > 4294967295n) throw new OverflowError("range() result has too many items");
    const result = [];
    const bs = BigInt(n.start), bp = BigInt(n.step);
    if (__pyRangeUseBig(n.start, n.stop, n.step, bs, bp, len)) {
        for (let i = 0n; i < len; i++) result.push(bs + i * bp); // exact beyond 2**53
    } else {
        const count = Number(len);
        for (let i = 0; i < count; i++) result.push(n.start + i * n.step);
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
    // Option B: a boxed float is an object but NOT iterable — guard before
    // the plain-object branch or `for x in 8.0` silently iterates nothing.
    if (it.__pyfloat__ === true) throw new TypeError_("'float' object is not iterable");
    if (typeof it === "object") return __pyOwnKeys(it)[Symbol.iterator](); // r6: symbol keys iterate too
    // #467: CPython names the PYTHON type ('float', 'int', …), not the JS one.
    throw new TypeError_(`'${__pyTypeName(it)}' object is not iterable`);
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
    // F1 (v0.2.4): comparison routes through pyLt, THE comparison
    // authority — the old LOCAL raw-`<` comparator bypassed the
    // cross-type guard (sorted([1, 'a']) stayed silently permissive
    // where CPython raises TypeError) and the bytes byte-value arm.
    // pyLt already recurses on tuple/list elements (#214).
    const cmp = (a, b) => (pyLt(a, b) ? -1 : pyLt(b, a) ? 1 : 0);
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
    // Option B: a boxed float must not read as a 0-key plain-object dict.
    if (it.__pyfloat__ === true) throw new TypeError_("'float' object is not iterable");
    if (typeof it === "object") return __pyOwnKeys(it); // plain-object dict → keys (r6: symbols too)
    throw new TypeError_(`'${__pyTypeName(it)}' object is not iterable`); // #467
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
/** start/end of the find/index/count/startswith family: CPython slice-bound
 * semantics — int / None / __index__ only, TypeError otherwise. E3 r3: this
 * DELEGATES to __pySliceIndex (the ONE slice-component authority) so the
 * __index__ RESULT is validated too — a "__index__ returned non-int (type
 * str)" object raises CPython's TypeError instead of being Number()-coerced
 * to NaN (which silently returned -1). Only BigInt is narrowed here (these
 * bounds feed Number arithmetic). */
function __strSliceBound(x) {
    const v = __pySliceIndex(x);
    return typeof v === "bigint" ? Number(v) : v;
}

function __pyStrFind(s, sub, start, end, last, name) {
    if (typeof sub !== "string") {
        // E3 r3: CPython 3.14's argument-clinic wording carries the METHOD
        // name ("find() argument 1 must be str, not int"); the value None
        // prints as None, not NoneType.
        const shown = sub === null || sub === undefined ? "None" : __pyTypeName(sub);
        throw new TypeError_(`${name || "find"}() argument 1 must be str, not ${shown}`);
    }
    start = __strSliceBound(start);
    end = __strSliceBound(end);
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
    // Option B + fidelity: numeric primitives (and the boxed float) have no
    // len() — CPython raises; the old plain-object fallback returned 0.
    if (typeof obj === "number" || typeof obj === "bigint" || typeof obj === "boolean"
        || obj.__pyfloat__ === true) {
        throw new TypeError(`object of type '${__pyTypeName(obj)}' has no len()`); // #467
    }
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
    return __pyOwnKeys(obj).length; // r6: symbol-keyed entries count
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
// (exported for the inline-vs-package parity battery, F6)
export function __roundBigNeg(x, k) {
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
    // Option-B spike: unwrap a boxed float; the 2-arg form re-boxes below.
    const __wasF = x != null && x.__pyfloat__ === true;
    if (__wasF) x = x.valueOf();
    // F6-r2 (v0.2.4): ndigits validates through the __index__ protocol
    // (__pyAsIndexInt) — the old Math.trunc(Number(ndigits)) silently
    // coerced round(1.25, 1.5) and round(1.25, "1") where CPython raises
    // "'float'/'str' object cannot be interpreted as an integer". bool
    // (ndigits=True → 1), int of any magnitude, and __index__-bearing
    // objects are accepted; everything else raises CPython's TypeError.
    if (ndigits != null) ndigits = __pyAsIndexInt(ndigits);
    // #318 (a): round(int, ndigits) returns an int for ANY magnitude,
    // including BigInt-routed huge ints (was a spurious TypeError from
    // mixing BigInt with the Number path). nd>=0 is a no-op; nd<0 rounds.
    if (typeof x === "bigint") {
        const nd = ndigits == null ? 0 : ndigits;
        if (nd >= 0) return x; // Number and BigInt both compare correctly
        // F6-r2: a sufficiently-negative ndigits rounds every digit away —
        // CPython returns int 0. The old unconditional __roundBigNeg hit
        // V8's BigInt size cap (10n ** k → RangeError) for a huge |ndigits|.
        const digits = BigInt((x < 0n ? -x : x).toString().length);
        const k = typeof nd === "bigint" ? -nd : BigInt(-nd);
        if (k > digits) return 0;
        return __roundBigNeg(x, Number(k));
    }
    // #341: single-arg round() returns an int, so a non-finite input can't be
    // converted → ValueError (NaN) / OverflowError (±inf), like CPython. The
    // 2-arg form round(x, ndigits) returns a float, so nan/inf pass through.
    if (typeof x === "number" && !isFinite(x)) {
        if (ndigits == null) {
            if (Number.isNaN(x)) throw new ValueError("cannot convert float NaN to integer");
            throw new OverflowError("cannot convert float infinity to integer");
        }
        return __pyF(x); // 2-arg form: nan/inf pass through (a float)
    }
    if (x == null || typeof x !== "number") {
        throw new TypeError("type cannot be interpreted as a number");
    }
    // Option-B spike: the 2-arg form of a float input returns a float — box.
    const __reF = ndigits != null && (__wasF || !Number.isInteger(x));
    if (ndigits == null) {
        // 1-arg form: half-even on the double itself (exact — no scaling) → int.
        const floor = Math.floor(x);
        const diff = x - floor;
        if (diff > 0.5) return floor + 1;
        if (diff < 0.5) return floor;
        return floor % 2 === 0 ? floor : floor + 1; // exactly .5 → nearest even
    }
    // F6 (v0.2.4): the ndigits form must round the DECIMAL value of the
    // ORIGINAL double (CPython double_round → _Py_dg_dtoa mode 3): half-even
    // at the target digit of x's EXACT decimal expansion, then convert the
    // rounded decimal back to the nearest double. The old scale-multiply
    // (`x * 10^nd` then half-even) re-rounds in BINARY first, so any value
    // whose product misrounds diverged silently: round(0.05, 1) — exact
    // expansion 0.05000000000000000277… → CPython 0.1, but 0.05 * 10
    // lands exactly on 0.5 → half-even → old result 0.0. (round(0.145, 2)
    // is 0.14 BOTH ways — its exact expansion 0.14499999999999999001… is
    // below the half — so it is a non-witness; the r1 comment had it
    // reversed.) Class fix: exact BigInt decimal rounding (see
    // __pyRoundDecimal), no scaling.
    // F6-r2: ndigits is already a validated int (Number or BigInt). A BigInt
    // clamps into the Number band __pyRoundDecimal saturates at anyway
    // (≥1074 → x unchanged; ≤-309 → signed zero), so round(x, ±10**100)
    // returns CPython's value instead of overflowing.
    const nd = typeof ndigits === "bigint"
        ? (ndigits > 2000n ? 2000 : ndigits < -2000n ? -2000 : Number(ndigits))
        : ndigits;
    const result = x === 0 ? x : __pyRoundDecimal(x, nd); // ±0 passes through
    // CPython: a finite x whose ROUNDED value exceeds the double range raises
    // (round(1.7e308, -308) → 2e308) rather than returning inf.
    if (!isFinite(result)) {
        throw new OverflowError("rounded value too large to represent");
    }
    return __reF ? __pyF(result) : result;
}

// F6 (v0.2.4): exact decimal rounding of a finite nonzero double — the JS
// equivalent of CPython's double_round/_Py_dg_dtoa mode-3 path. Decomposes
// x into mant * 2^e (BigInt-exact), computes round-half-even of |x| * 10^nd
// as an exact integer quotient, and re-enters double land through the
// decimal string parse `Number("<q>e<-nd>")`, which ECMA-262 requires to be
// correctly rounded (its exponent stays far inside the ±(2^31) cap the spec
// allows engines to clamp at, so the parse is exact-nearest here).
function __pyRoundDecimal(x, nd) {
    // A double's exact decimal expansion has at most 1074 fractional digits
    // (min subnormal = 2^-1074): rounding at/past digit 1074 changes nothing.
    if (nd >= 1074) return x;
    // |x| ≤ 1.8e308 < 0.5 * 10^309: everything rounds to (signed) zero.
    if (nd <= -309) return x < 0 ? -0 : 0;
    const dv = new DataView(new ArrayBuffer(8));
    dv.setFloat64(0, x);
    const hi = dv.getUint32(0);
    const lo = dv.getUint32(4);
    const neg = (hi >>> 31) === 1;
    const be = (hi >>> 20) & 0x7ff; // biased exponent (finite ⇒ be < 0x7ff)
    let mant = (BigInt(hi & 0xfffff) << 32n) | BigInt(lo);
    let e;
    if (be === 0) {
        e = -1074; // subnormal
    } else {
        mant |= 1n << 52n;
        e = be - 1075;
    }
    // |x| = mant * 2^e, so |x| * 10^nd = num / den EXACTLY.
    const num = mant
        * (nd > 0 ? 10n ** BigInt(nd) : 1n)
        * (e > 0 ? 1n << BigInt(e) : 1n);
    const den = (nd < 0 ? 10n ** BigInt(-nd) : 1n)
        * (e < 0 ? 1n << BigInt(-e) : 1n);
    let q = num / den;
    const twice = (num % den) * 2n;
    if (twice > den || (twice === den && (q & 1n) === 1n)) q += 1n; // half-even
    const res = Number(q.toString() + "e" + -nd);
    return neg ? -res : res;
}

/**
 * Python-compatible iter().
 */
export function pyIter(obj) {
    if (obj == null) throw new TypeError("'NoneType' object is not iterable");
    if (typeof obj[Symbol.iterator] === "function") return obj[Symbol.iterator]();
    if (typeof obj.__iter__ === "function") return obj.__iter__();
    // Option B: name a boxed float correctly ('float', not 'object').
    if (obj.__pyfloat__ === true) throw new TypeError("'float' object is not iterable");
    throw new TypeError(`'${__pyTypeName(obj)}' object is not iterable`); // #467
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
    throw new AttributeError(`'${__pyTypeName(g)}' object has no attribute 'send'`); // #467
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
    throw new AttributeError(`'${__pyTypeName(g)}' object has no attribute 'close'`); // #467
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
    throw new AttributeError(`'${__pyTypeName(g)}' object has no attribute 'throw'`); // #467
}

/**
 * Python-compatible slice.
 * pySlice(obj, start, stop, step)
 */
// autotester extended_slices: a REAL slice object — CPython's
// slice(start, stop, step) with .indices(len) (the exact clamping rules),
// attribute access (missing bounds are None), repr, and ==. Handed to a
// custom __getitem__/__setitem__, and constructed in value position for
// tuple-of-slices subscripts (`a[1:2:3, 4:5:6]`).
export class PySlice {
    constructor(start, stop, step) {
        this.start = start === undefined ? null : start;
        this.stop = stop === undefined ? null : stop;
        this.step = step === undefined ? null : step;
        this.__pyslice__ = true;
    }
    indices(len) {
        const n = Number(len);
        let step = this.step === null ? 1 : Number(this.step);
        if (step === 0) throw new ValueError("slice step cannot be zero");
        const clamp = (i, neg) => {
            i = Number(i);
            if (i < 0) i += n;
            if (i < 0) return neg ? -1 : 0;
            if (i >= n) return neg ? n - 1 : n;
            return i;
        };
        const neg = step < 0;
        const start = this.start === null ? (neg ? n - 1 : 0) : clamp(this.start, neg);
        const stop = this.stop === null ? (neg ? -1 : n) : clamp(this.stop, neg);
        const out = [start, stop, step];
        Object.defineProperty(out, "__pytuple__", { value: true, enumerable: false });
        return out;
    }
    __repr__() {
        const r = (x) => (x === null ? "None" : pyRepr(x));
        return `slice(${r(this.start)}, ${r(this.stop)}, ${r(this.step)})`;
    }
    __eq__(o) {
        return o instanceof PySlice && pyEq(this.start, o.start)
            && pyEq(this.stop, o.stop) && pyEq(this.step, o.step);
    }
}
export function __pySliceObj(start, stop, step) {
    return new PySlice(start, stop, step);
}

// public #3: the slice() BUILTIN — CPython's argument forms:
// slice(stop) / slice(start, stop) / slice(start, stop, step). Returns the
// same PySlice object subscripts and custom __getitem__ receive; pyGetItem
// dispatches a PySlice key through pySlice, so `xs[slice(1, 3)]` ≡ `xs[1:3]`.
export function pySliceOf(...args) {
    if (args.length === 0) {
        throw new TypeError_("slice expected at least 1 argument, got 0");
    }
    if (args.length > 3) {
        throw new TypeError_(`slice expected at most 3 arguments, got ${args.length}`);
    }
    if (args.length === 1) return new PySlice(null, args[0], null);
    return new PySlice(args[0], args[1], args.length === 3 ? args[2] : null);
}

// F5 (v0.2.4): CPython's _PyEval_SliceIndex — a slice component must be an
// int, None, or expose __index__; EVERYTHING else (floats — boxed or native
// non-integer — strs, containers, None-typed misc) is a TypeError. The index
// arm has carried this guard since crit-8/F7; the slice arm silently accepted
// floats (`[1,2,3][0:2.0]` → [1,2] instead of TypeError). One validator, used
// by ALL THREE slice ops (get/set/del) so the class can't reopen per-arm.
// THEOREM-BLIND: the Lean slice theorems quantify over integer bounds, so
// this is a pure runtime guard in front of them — no Lean statement changes.
export function __pySliceIndex(v) {
    if (v == null) return null;                   // missing bound → None
    if (typeof v === "boolean") return v ? 1 : 0; // bool ⊂ int
    if (typeof v === "bigint") return v;          // huge int literal (callers demote)
    if (typeof v === "number") {
        if (Number.isInteger(v)) return v;        // int (whole floats are boxed, Option B)
    } else if (v.__pyfloat__ !== true && typeof v.__index__ === "function") {
        const r = v.__index__();                  // CPython __index__ protocol
        // F5-r2: a bool result is accepted as its int value — CPython 3.12
        // takes it (with a DeprecationWarning), it does not raise.
        if (typeof r === "boolean") return r ? 1 : 0;
        if (typeof r === "bigint") return r;
        if (typeof r === "number" && Number.isInteger(r)) return r;
        throw new TypeError_("__index__ returned non-int (type "
            + __pyTypeName(r) + ")");
    }
    throw new TypeError_(
        "slice indices must be integers or None or have an __index__ method");
}

export function pySlice(obj, start, stop, step) {
    // crit-9: a custom object with __getitem__ handles the slice itself — don't
    // force it through the array/string path (which returns [] for a
    // non-sequence). It receives a real PySlice (with .indices()) UNVALIDATED —
    // CPython only validates components when a built-in sequence consumes them.
    if (obj != null && typeof obj !== "string" && !Array.isArray(obj) && typeof obj.__getitem__ === "function") {
        return obj.__getitem__(new PySlice(start, stop, step));
    }
    // F5: validate in CPython's PySlice_Unpack order — step first (its zero
    // check precedes start/stop validation), then start, then stop.
    step = __pySliceIndex(step);
    if (step === 0 || step === 0n) throw new ValueError("slice step cannot be zero");
    start = __pySliceIndex(start);
    stop = __pySliceIndex(stop);
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
    // BYTES AUTHORITY: a bytes/bytearray slice READ yields the SAME kind
    // (CPython: b"abc"[1:] is bytes, bytearray(b"ab")[0:1] is a bytearray),
    // not the plain int list this path used to materialize.
    const __bk = __pyBytesKind(obj);
    if (__bk !== null) {
        return __bk === "bytearray" ? pyBytearrayOf(result) : pyBytesOf(result);
    }
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
    // autotester extended_slices: a custom __setitem__ object receives a
    // real PySlice, mirroring pySlice's crit-9 branch.
    if (arr != null && typeof arr !== "string" && !Array.isArray(arr) && typeof arr.__setitem__ === "function") {
        arr.__setitem__(new PySlice(start, stop, step), values);
        return;
    }
    // BYTES AUTHORITY (#455): a bytearray has __setitem__ and was routed
    // above; any bytes-like value reaching here is immutable — CPython's
    // TypeError, not the splice crash this path used to hit.
    if (__pyBytesKind(arr) !== null) {
        throw new TypeError_(`'${__pyBytesName(arr)}' object does not support item assignment`);
    }
    // F5 (v0.2.4): same int-or-None component guard as pySlice (write arm).
    step = __pySliceIndex(step);
    if (step === 0 || step === 0n) throw new ValueError("slice step cannot be zero");
    start = __pySliceIndex(start);
    stop = __pySliceIndex(stop);
    if (typeof start === "bigint") start = Number(start);
    if (typeof stop === "bigint") stop = Number(stop);
    if (typeof step === "bigint") step = Number(step);
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
    if (arr != null && !Array.isArray(arr) && typeof arr.__delitem__ === "function") {
        arr.__delitem__(new PySlice(start, stop, step));
        return;
    }
    if (!Array.isArray(arr)) {
        // BYTES AUTHORITY: name immutable bytes correctly (a bytearray has
        // __delitem__ and was routed above). #467: the ONE type-name source
        // covers null→NoneType and bytes/bytearray via pyType.
        throw new TypeError_(`'${__pyTypeName(arr)}' object does not support item deletion`);
    }
    // F5 (v0.2.4): same int-or-None component guard as pySlice (delete arm).
    step = __pySliceIndex(step);
    if (step === 0 || step === 0n) throw new ValueError("slice step cannot be zero");
    start = __pySliceIndex(start);
    stop = __pySliceIndex(stop);
    if (typeof start === "bigint") start = Number(start);
    if (typeof stop === "bigint") stop = Number(stop);
    if (typeof step === "bigint") step = Number(step);
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
    // CPython 3.14 reworded the containment TypeError to
    // "...is not a container or iterable" (3.12 said "...is not iterable").
    if (container == null) throw new TypeError("argument of type 'NoneType' is not a container or iterable");
    // Option B: a boxed float container must raise, not read as an empty dict.
    if (container.__pyfloat__ === true) {
        throw new TypeError_("argument of type 'float' is not a container or iterable");
    }
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
        if (typeof item === "boolean" || typeof item === "number" || typeof item === "bigint"
            || (item != null && item.__pyfloat__ === true)) {
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
    // FULL_SURFACE #2: a CLASS object (or plain function) with no
    // __contains__/__iter__ is NOT a container — CPython 3.14 raises
    // `TypeError: argument of type 'type' is not a container or iterable`
    // (Transcrypt's attr-name-membership behavior was the bug this replaces).
    if (typeof container === "function") {
        // #467: pyType classifies classes vs plain functions ('type'/'function').
        throw new TypeError_(`argument of type '${__pyTypeName(container)}' is not a container or iterable`);
    }
    // Object dict — F3: use hasOwnProperty so inherited prototype members
    // (`hasOwnProperty`, `toString`, `constructor`, `__proto__`, ...) don't
    // spuriously report as keys the way JS `in` would. Coerce ONCE (delta).
    // Only PLAIN-prototype objects are the dict representation (FULL_SURFACE
    // #2); a class INSTANCE falls through to the protocol probes below.
    const __cproto = Object.getPrototypeOf(container);
    if (__cproto === Object.prototype || __cproto === null) {
        const __cpk = __pyPropKey(item);
        const __cpresent = Object.prototype.hasOwnProperty.call(container, __cpk);
        // WB-20 analogue (see pyDictGet): perform the PLAIN property read
        // unconditionally — even on a miss — so a host read-trap (MobX
        // observable Proxy) registers a dependency on THIS key exactly as a
        // native `k in d` read path would. hasOwnProperty goes through
        // [[GetOwnProperty]], which fires no `get` at all on a missing key,
        // so an observer testing membership never subscribed and never
        // re-rendered when the key was later added. Reading a plain data
        // dict's absent slot is side-effect-free, so the extra read is
        // inert off the observable path.
        void container[__cpk];
        return __cpresent;
    }
    // Instance with an un-wired `__iter__` (the __pyClass path wires it to
    // Symbol.iterator, which was handled above — this is the fallback).
    // Accept both iterator shapes: JS `.next()` and Python `__next__()`.
    if (typeof container.__iter__ === "function") {
        const it = container.__iter__();
        if (it != null && typeof it.next === "function") {
            for (;;) {
                const r = it.next();
                if (r.done) return false;
                if (pyEq(r.value, item)) return true;
            }
        }
        if (it != null && typeof it.__next__ === "function") {
            for (;;) {
                let x;
                try {
                    x = it.__next__();
                } catch (e) {
                    if (e && e.name === "StopIteration") return false;
                    throw e;
                }
                if (pyEq(x, item)) return true;
            }
        }
    }
    // CPython's legacy sequence protocol: __getitem__(0), __getitem__(1), …
    // until IndexError.
    if (typeof container.__getitem__ === "function") {
        for (let i = 0; ; i++) {
            let x;
            try {
                x = container.__getitem__(i);
            } catch (e) {
                if (e instanceof IndexError || (e && e.name === "IndexError")) return false;
                throw e;
            }
            if (pyEq(x, item)) return true;
        }
    }
    throw new TypeError_(`argument of type '${__pyTypeName(container)}' is not a container or iterable`); // #467
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

// autotester exceptions: BaseException is the REAL root of CPython's
// hierarchy (user code legitimately writes `class Table(BaseException)` and
// `except BaseException` / `raise BaseException(...)`). It carries the same
// introspection surface; Exception extends it, so isinstance(e,
// BaseException) holds for every builtin/user exception.
class BaseException extends Error {
    static __name__ = "BaseException";
    constructor(...args) {
        const t = pyTuple(...args);
        super(__excStr(t));
        this.name = new.target.__name__ ?? new.target.name;
        this.args = t;
    }
}

class Exception extends BaseException {
    static __name__ = "Exception";
}

class ValueError extends Exception {
    static __name__ = "ValueError";
}
// autotester exceptions: raisable/catchable as a real class (the assert
// statement itself throws a name-tagged Error; both match by name).
class AssertionError extends Exception {
    static __name__ = "AssertionError";
}
class TypeError_ extends Exception {
    static __name__ = "TypeError";
    // E7 message-binding fix: node's uncaught-exception printer renders the
    // CONSTRUCTOR name, so an uncaught runtime TypeError printed
    // `TypeError_: msg` (the JS collision-avoidance class name) instead of
    // CPython's `TypeError: msg`. Re-point the class's `name` property at
    // the Python name (configurable, instanceof/catch unaffected). Lives
    // INSIDE the class body (static block) so the #170 inline extraction
    // carries it with the class slice — same reason the `__name__` stamps
    // are static fields. TypeError_ is the ONLY exception class whose JS
    // name differs from its `__name__`; message_shipped_binding.py pins
    // this terminal surface for the whole taxonomy.
    static {
        // `configurable: true` explicitly (review r2): a class's own `name`
        // is spec-default configurable, and redefining with only `value`
        // would preserve that — but the comment's claim is now enforced in
        // code, not inherited from property-descriptor trivia.
        Object.defineProperty(this, "name", { value: "TypeError", configurable: true });
    }
}
class AttributeError extends Exception {
    static __name__ = "AttributeError";
}
class StopIteration extends Exception {
    static __name__ = "StopIteration";
    constructor(...args) {
        super(...args);
        // CPython: StopIteration.value is the generator's return value —
        // args[0] when present, None otherwise (round-4 sweep).
        this.value = args.length > 0 ? args[0] : null;
    }
}
class StopAsyncIteration extends Exception {
    static __name__ = "StopAsyncIteration";
}
class RuntimeError extends Exception {
    static __name__ = "RuntimeError";
}
class NotImplementedError extends RuntimeError {
    static __name__ = "NotImplementedError";
}
class LookupError extends Exception {
    static __name__ = "LookupError";
}
class IndexError extends LookupError {
    static __name__ = "IndexError";
}
class ArithmeticError extends Exception {
    static __name__ = "ArithmeticError";
}
class ZeroDivisionError extends ArithmeticError {
    static __name__ = "ZeroDivisionError";
}
class OverflowError extends ArithmeticError {
    static __name__ = "OverflowError";
}
// PBT-2: reading a for-loop target after a zero-iteration loop must raise
// (UnboundLocalError in a function, NameError at module scope), not yield
// None. The codegen initializes such hoisted targets to the __UNBOUND
// sentinel and routes reads through __pyChkLocal/__pyChkGlobal below.
class NameError extends Exception {
    static __name__ = "NameError";
}
class UnboundLocalError extends NameError {
    static __name__ = "UnboundLocalError";
}

class KeyError extends LookupError {
    static __name__ = "KeyError";
    constructor(...args) {
        super(...args);
        // CPython quirk: str(KeyError(k)) is repr(k), not str(k).
        if (args.length === 1) this.message = pyRepr(args[0]);
    }
}

// The __name__ stamps live INSIDE each class as a static field (below) so
// the #170 inline extraction carries them with the class slice — the old
// standalone `X.__name__ = ...` statement block attached to whatever slice
// happened to precede it and got dropped when that slice wasn't needed
// (type(e).__name__ then leaked the JS class name 'TypeError_').

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

// #452 (cross-scope sentinels): closure read of an ENCLOSING-function local
// that no iteration/assignment ever bound — CPython raises NameError with
// the free-variable message (UnboundLocalError is only for the scope that
// owns the variable; a free variable is plain NameError — CPython 3.12).
function __pyChkFree(v, name) {
    if (v === __UNBOUND) {
        throw new NameError(
            `cannot access free variable '${name}' where it is not associated with a value in enclosing scope`,
        );
    }
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
// Callable interned TYPE objects (autotester classes): CPython's builtin
// type names are ONE object each — callable as a constructor AND usable as
// a first-class type (isinstance/issubclass second arg, `type(x) == int`
// by identity, repr `<class 'int'>`, stored in tuples of types). The
// `__pytype__` marker is what __pyIsInstance/__pyIsSubclass/pyType key on.
// pyType returns THESE singletons, so `type(5) is int` holds.
function __mkPyType(name, call) {
    Object.defineProperty(call, "name", { value: name, configurable: true });
    call.__name__ = name;
    call.__pytype__ = true;
    call.__repr__ = () => `<class '${name}'>`;
    call.__str__ = call.__repr__;
    return call;
}
// The interned type objects carry a real `__mro__` (CPython:
// `int.__mro__` → `(int, object)`, `bool.__mro__` → `(bool, int, object)`),
// so `int.__mro__[-1]` is `object` instead of a None-subscript TypeError.
// Each stamp sits DIRECTLY under its own singleton's declaration on purpose:
// the #170 extraction slicer attaches trailing statements to the preceding
// declaration's slice, so the stamp travels with its singleton into inline
// `pyths run` output (and the identifiers it references pull the other
// singletons in as transitive deps). `__pyTypeObject` is declared FIRST —
// a `const` does not hoist, so a stamp referencing it before its
// declaration would hit the TDZ at module init.
export const __pyTypeObject = __mkPyType("object", () => ({}));
__pyTypeObject.__mro__ = [__pyTypeObject];
export const __pyTypeInt = __mkPyType("int", (...a) => (a.length === 0 ? 0 : pyInt(...a)));
__pyTypeInt.__mro__ = [__pyTypeInt, __pyTypeObject];
export const __pyTypeFloat = __mkPyType("float", (...a) => (a.length === 0 ? 0 : pyFloat(...a)));
__pyTypeFloat.__mro__ = [__pyTypeFloat, __pyTypeObject];
export const __pyTypeBool = __mkPyType("bool", (...a) => (a.length === 0 ? false : pyBool(a[0])));
__pyTypeBool.__mro__ = [__pyTypeBool, __pyTypeInt, __pyTypeObject];
export const __pyTypeStr = __mkPyType("str", (...a) => (a.length === 0 ? "" : pyStr(...a)));
__pyTypeStr.__mro__ = [__pyTypeStr, __pyTypeObject];
export const __pyTypeList = __mkPyType("list", (it) => (it === undefined ? [] : pyListOf(it)));
__pyTypeList.__mro__ = [__pyTypeList, __pyTypeObject];
export const __pyTypeTuple = __mkPyType("tuple", (it) => pyTupleOf(it));
__pyTypeTuple.__mro__ = [__pyTypeTuple, __pyTypeObject];
export const __pyTypeSet = __mkPyType("set", (...a) => pySetOf(...a));
__pyTypeSet.__mro__ = [__pyTypeSet, __pyTypeObject];
export const __pyTypeFrozenset = __mkPyType("frozenset", (...a) => pyFrozensetOf(...a));
__pyTypeFrozenset.__mro__ = [__pyTypeFrozenset, __pyTypeObject];
export const __pyTypeDict = __mkPyType("dict", (...a) => pyDict(...a));
__pyTypeDict.__mro__ = [__pyTypeDict, __pyTypeObject];
// dict.fromkeys(iterable[, value]) — classmethod on the dict type; also
// reachable from an instance (d.fromkeys) via the pyBoundMethod switch.
export function __pyDictFromkeys(it, value) {
    const v = value === undefined ? null : value;
    const out = {};
    for (const k of pySeq(it)) __pyDictWrite(out, k, v);
    return out;
}
__pyTypeDict.fromkeys = __pyDictFromkeys;
// Method-table form: rt helpers receive the RECEIVER first; a classmethod
// ignores it (works from the type object or any instance).
export function pyDictFromkeys(_recv, it, value) {
    return __pyDictFromkeys(it, value);
}
// BYTES AUTHORITY: interned bytes/bytearray type singletons (CPython:
// `type(b'').__name__` is "bytes", `bytes.__mro__` is (bytes, object),
// bytearray is NOT a bytes subclass). Callable like the other type
// objects (`bytes(3)`, `bytearray(b'ab')` through a first-class value).
export const __pyTypeBytes = __mkPyType("bytes", (...a) => pyBytesOf(...a));
__pyTypeBytes.__mro__ = [__pyTypeBytes, __pyTypeObject];
export const __pyTypeBytearray = __mkPyType("bytearray", (...a) => pyBytearrayOf(...a));
__pyTypeBytearray.__mro__ = [__pyTypeBytearray, __pyTypeObject];
const __PyInt = __pyTypeInt;
const __PyFloat = __pyTypeFloat;
const __PyBool = __pyTypeBool;
const __PyStr = __pyTypeStr;
const __PyList = __pyTypeList;
const __PyTuple = __pyTypeTuple;
const __PySet = __pyTypeSet;
const __PyDict = __pyTypeDict;
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
    if (v.__pyfloat__ === true) return __PyFloat;
    switch (typeof v) {
        case "boolean": return __PyBool; // BEFORE number — bool is not int here
        case "number": return Number.isInteger(v) ? __PyInt : __PyFloat;
        case "bigint": return __PyInt;
        case "string": return __PyStr;
        case "function":
            // Interned callable type objects ARE types: type(int) → type.
            if (v.__pytype__) return __PyTypeMeta;
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
    // BYTES AUTHORITY (#456): bytes/bytearray classify through __pyBytesKind
    // and return the interned singletons — `type(b'').__name__` is "bytes"
    // (not the JS class name) and `type(x) == bytes` holds by identity.
    const __bk = __pyBytesKind(v);
    if (__bk !== null) return __bk === "bytearray" ? __pyTypeBytearray : __pyTypeBytes;
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
 * #463 (comprehension unification): CPython acquires `iter(outermost)` when
 * a GENEXP OBJECT IS CREATED — dis shows GET_ITER running before the genexp
 * function is called — which is observable with a side-effecting or throwing
 * `__iter__`. Compiled genexps route their outermost iterable through here
 * at creation time: the iterator is obtained NOW (Python iteration semantics
 * via __pyElemsIter — dict shapes yield keys, protocol classes go through
 * their bridged `__iter__`), and consumption hands back that same iterator,
 * so `next(g)` followed by `list(g)` resume ONE iterator, like CPython.
 */
export function __pyEagerIter(x) {
    const it = __pyElemsIter(x);
    return { [Symbol.iterator]() { return it; } };
}

/**
 * Async twin of __pyEagerIter — GET_AITER at async-genexp creation: the
 * async iterator is acquired NOW through the __pyAsyncIter protocol bridge
 * (a protocol object's `__aiter__` runs at creation, matching CPython). A
 * sync-only iterable keeps __pyAsyncIter's permissive passthrough (JS
 * for-await falls back to the sync protocol), acquired eagerly all the same.
 */
export function __pyEagerAIter(x) {
    const src = __pyAsyncIter(x);
    if (typeof src[Symbol.asyncIterator] === "function") {
        const it = src[Symbol.asyncIterator]();
        return { [Symbol.asyncIterator]() { return it; } };
    }
    return __pyEagerIter(src);
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
/**
 * Centralized Python container type-name for index/bounds diagnostics. ONE
 * source so every message ("tuple index out of range", "tuple indices must be
 * integers…") stays consistent and no path can hardcode "list" for a tuple.
 */
export function __pySeqName(obj) {
    if (typeof obj === "string") return "string";
    if (Array.isArray(obj)) return obj.__pytuple__ ? "tuple" : "list";
    return "sequence";
}

// ============================================================
// BYTES DISPATCH AUTHORITY (#455/#456/#457/#458 root fix)
// ============================================================
// ONE place recognizes "this value is bytes/bytearray" and classifies it.
// Every bytes-typed decision routes through __pyBytesKind — truthiness
// (pyBool), type()/`__name__` (pyType → the interned __pyTypeBytes /
// __pyTypeBytearray singletons), isinstance (__pyIsInstance, both copies),
// write/delete immutability arms (pySetItem/pySetSlice/pyDelSlice), and
// diagnostics (__pyBytesName) — so a future bytes op cannot invent its own
// recognition and drift. The METHOD surface has one home too: the PyBytes/
// PyByteArray prototypes in operators.js, reached by both dispatch surfaces
// through the uniform "receiver's own method wins" rule (direct calls via
// the method-table runtime helpers, extraction via pyBoundMethod's
// prototype-bind branch).
//
// Deliberately duck-typed (instanceof Uint8Array + the mutator surface)
// rather than `instanceof PyBytes/PyByteArray`: referencing the classes
// from here would pull them into every #170-sliced program, and a raw
// interop Uint8Array should classify as bytes as well.

/** "bytes" | "bytearray" for a bytes-like value, null for anything else.
 *  THE recognizer — a bytearray exposes the mutating `append`, bytes does
 *  not. */
export function __pyBytesKind(v) {
    if (!(v instanceof Uint8Array)) return null;
    return typeof v.append === "function" ? "bytearray" : "bytes";
}

/** "bytearray" vs immutable "bytes" — the authority's name surface for
 * error messages and repr (callers already know the value is bytes-like). */
export function __pyBytesName(obj) {
    return __pyBytesKind(obj) ?? "bytes";
}

/**
 * Python `type(key).__name__` for a subscript KEY, for the CPython
 * "indices must be integers or slices, not X" diagnostics on the list /
 * bytearray write and delete paths. ONE source so every path renders the
 * key's type name identically (mirrors the inline logic in pyGetItem).
 */
export function __pyIndexTypeName(key) {
    if (key === null || key === undefined) return "NoneType";
    if (key.__pyfloat__ === true) return "float"; // Option B boxed float
    switch (typeof key) {
        case "boolean": return "bool";
        case "bigint": return "int";
        case "number": return Number.isInteger(key) ? "int" : "float";
        case "string": return "str";
        case "symbol": return "symbol";
        case "function": return "function";
    }
    if (Array.isArray(key)) return key.__pytuple__ ? "tuple" : "list";
    if (key instanceof Map) return "dict";
    if (key instanceof Set) return "set";
    if (key instanceof Uint8Array) return __pyBytesName(key);
    return "dict"; // a plain-object key reads as a dict (R8, matches pyGetItem)
}

export function pyGetItem(obj, key) {
    if (obj == null) {
        throw new TypeError_("'NoneType' object is not subscriptable");
    }
    // public #3: a real slice OBJECT as the key — `xs[slice(1, 3)]` must
    // behave exactly like `xs[1:3]`. Route sequences (and custom
    // __getitem__ receivers, which get handed the PySlice itself) through
    // pySlice; a dict key of type slice is unhashable, like CPython.
    if (key instanceof PySlice) {
        if (typeof obj === "string" || Array.isArray(obj)
            || typeof obj.__getitem__ === "function") {
            return pySlice(obj, key.start, key.stop, key.step);
        }
        throw new TypeError_("unhashable type: 'slice'");
    }
    // Non-subscriptable primitives (int/float/bool). Without this guard a JS
    // number/bigint/boolean falls through to the interop passthrough below
    // (`Object.getPrototypeOf(5) !== Object.prototype`) and silently returns
    // `undefined` instead of raising — CPython raises TypeError. Found by the
    // lattice C4 shipping-binding (`5[0]`, `True[0]`, `(3.5)[0]`).
    if (typeof obj === "number" || typeof obj === "bigint" || typeof obj === "boolean"
        || obj.__pyfloat__ === true) {
        throw new TypeError_(`'${__pyTypeName(obj)}' object is not subscriptable`); // #467
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
            throw new IndexError(__pySeqName(obj) + " index out of range");
        }
    }
    // crit-8: a non-integer numeric index on a sequence is a TypeError in
    // CPython ([10,20][1.5]). A whole-valued float (1.0) is an indistinguishable
    // JS Number here and falls under the documented whole-float deviation (B1).
    if ((typeof obj === "string" || Array.isArray(obj)) && typeof key === "number" && !Number.isInteger(key)) {
        // R8: report tuple/list/string per the actual sequence type.
        const on = __pySeqName(obj);
        throw new TypeError_(on === "string"
            ? "string indices must be integers"
            : `${on} indices must be integers or slices, not float`);
    }
    // F7 (CVE-2026-15903 JS-path sibling): an index that is neither a number
    // nor a string — None/undefined, a dict/list/set, a symbol — slips every
    // check below (`null < 0` and `null >= n` are both false), so `obj[key]`
    // silently returns undefined → None instead of CPython's TypeError. bool→int
    // and in-range bigint were normalized above; str keys keep their dedicated
    // messages in the per-type branches. See
    // experiments/codex-security-scan/poc/A2-f7.md.
    if ((typeof obj === "string" || Array.isArray(obj)) && typeof key !== "number" && typeof key !== "string") {
        const tn = key === null || key === undefined ? "NoneType"
            : key.__pyfloat__ === true ? "float" // Option B boxed float
            : Array.isArray(key) ? (key.__pytuple__ ? "tuple" : "list")
            : key instanceof Map ? "dict"
            : key instanceof Set ? "set"
            : typeof key === "object" ? "dict" // R8: a plain-object key is a dict
            : typeof key;
        const on = __pySeqName(obj);
        throw new TypeError_(on === "string"
            ? `string indices must be integers, not '${tn}'`
            : `${on} indices must be integers or slices, not ${tn}`);
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
                __pySeqName(obj) + " indices must be integers or slices, not "
                + (typeof key === "string" ? "str" : "float"));
        }
        const n = obj.length;
        let i = key;
        if (i < 0) i += n;
        if (i < 0 || i >= n) throw new IndexError(__pySeqName(obj) + " index out of range");
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
    // Plain object — treat as dict. Coerce the key EXACTLY ONCE (delta4):
    // `hasOwnProperty.call(obj, key)` runs ToPropertyKey(key) and `obj[key]`
    // would run it AGAIN, so a `Symbol.toPrimitive` returning "safe" then
    // "other" made the presence-check and the read disagree — the read
    // returned the WRONG slot. Same invariant as the write path (pySetItem
    // → __pyDictWrite): every subscript op computes __pyPropKey once.
    const pk = __pyPropKey(key);
    if (!Object.prototype.hasOwnProperty.call(obj, pk)) {
        throw new KeyError(key);
    }
    return obj[pk];
}

export { AssertionError, ValueError, IndexError, KeyError, TypeError_ as TypeError, AttributeError, StopIteration, StopAsyncIteration, ZeroDivisionError, BaseException, Exception, OverflowError, RuntimeError, NotImplementedError, LookupError, ArithmeticError, NameError, UnboundLocalError };
// PBT-2: sentinel + read guards for possibly-unbound for-loop targets.
export { __UNBOUND, __pyChkLocal, __pyChkGlobal, __pyChkFree };

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
        // F7: a tuple is immutable — `del t[i]` is a TypeError in CPython, not
        // the silent splice this path used to perform.
        if (obj.__pytuple__) {
            throw new TypeError_("'tuple' object doesn't support item deletion");
        }
        // F7: a non-integer index TYPE is a TypeError (CPython), not the
        // IndexError "... out of range" this path raised for EVERY non-integer
        // key (wrong error KIND — C4 axis). bool ⊂ int; an in-range bigint is a
        // valid integer index; only a genuinely out-of-range integer is an
        // IndexError.
        let i = typeof key === "boolean" ? (key ? 1 : 0)
              : typeof key === "bigint" ? Number(key) : key;
        if (typeof i !== "number" || !Number.isInteger(i)) {
            throw new TypeError_(
                "list indices must be integers or slices, not " + __pyIndexTypeName(key));
        }
        const n = obj.length;
        if (i < 0) i += n;
        if (i < 0 || i >= n) {
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
    // F7: a non-subscriptable / immutable object does NOT support item
    // deletion — a TypeError in CPython, NOT the KeyError the plain-object
    // property path below would raise (`del (5)[0]`, `del "abc"[0]`,
    // `del {1,2}[0]`, `del b"AB"[0]`). Ints/floats/bools are not
    // subscriptable; strings and bytes are immutable — all render
    // "'<type>' object doesn't support item deletion". (A MUTABLE bytearray
    // was already handled by the `__delitem__` branch above, so any Uint8Array
    // reaching here is immutable bytes.)
    if (typeof obj === "number" || typeof obj === "bigint" || typeof obj === "boolean"
        || typeof obj === "string" || obj instanceof Set || obj instanceof Uint8Array
        || obj.__pyfloat__ === true) {
        throw new TypeError_(`'${__pyTypeName(obj)}' object doesn't support item deletion`); // #467
    }
    // Coerce ONCE (delta4) — same invariant as pyGetItem/pySetItem: a
    // double-coercing key must not make the presence-check pass on one
    // property and the delete remove a DIFFERENT one.
    const pk = __pyPropKey(key);
    if (!Object.prototype.hasOwnProperty.call(obj, pk)) {
        throw new KeyError(key);
    }
    delete obj[pk];
}

/**
 * Python `chr(n)` — code-point based (astral-safe), with CPython's range
 * check. Issue #89.
 */
export function pyChr(n) {
    // E7/F7 kind fix, r2: route the receiver through THE "interpreted as an
    // integer" authority (__pyAsIndexInt — CPython's PyNumber_Index): bool ⊆
    // int, int/BigInt pass, an object with a valid __index__ is ACCEPTED
    // (chr(I()) where I().__index__() == 65 is 'A' in CPython), and every
    // other receiver raises CPython's exact TypeError (incl. "__index__
    // returned non-int"). r1 re-derived the check locally and rejected
    // __index__ receivers — the same class as F3; do NOT re-derive here.
    // Range: CPython 3.14 raises ValueError for ANY out-of-range int, incl.
    // beyond-ssize_t magnitudes (probed: chr(2**70) → ValueError, not the
    // pre-3.14 OverflowError), which the BigInt → Number funnel preserves.
    const v = __pyAsIndexInt(n);
    const i = typeof v === "bigint" ? Number(v) : v;
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
    if (args.length >= 1) {
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
    // autotester module_builtin: `max()` / `max(default=5)` are TypeError in
    // CPython ("expected at least 1 argument"), not the empty-iterable
    // ValueError. (The options object was stripped above; a bare
    // `max({'default': …})` dict-iteration remains ambiguous by design —
    // the long-standing kwargs-object call convention.)
    if (args.length === 0) {
        throw new TypeError_(`${name} expected at least 1 argument, got 0`);
    }
    // CPython: default= is only valid with a single iterable argument.
    if (hasDefault && args.length > 1) {
        throw new TypeError_(
            `Cannot specify a default for ${name}() with multiple positional arguments`,
        );
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
        // F1 (v0.2.4): route through pyGt/pyLt (THE comparison authority) —
        // the local dunder-or-raw-`>` fallback bypassed the cross-type
        // TypeError guard (min(3, 'a') silently returned 3) and the
        // reflected-dunder + bytes/sequence arms.
        const better = wantGreater ? pyGt(k, bestKey) : pyLt(k, bestKey);
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

// === BEGIN GENERATED UNICODE TABLES (scripts/gen_unicode_tables.py; oracle 3.14.7) ===
// str.isdecimal(): all Nd per the oracle's Unicode tables.
const __RE_DECIMAL = /^[\u0030-\u0039\u0660-\u0669\u06f0-\u06f9\u07c0-\u07c9\u0966-\u096f\u09e6-\u09ef\u0a66-\u0a6f\u0ae6-\u0aef\u0b66-\u0b6f\u0be6-\u0bef\u0c66-\u0c6f\u0ce6-\u0cef\u0d66-\u0d6f\u0de6-\u0def\u0e50-\u0e59\u0ed0-\u0ed9\u0f20-\u0f29\u1040-\u1049\u1090-\u1099\u17e0-\u17e9\u1810-\u1819\u1946-\u194f\u19d0-\u19d9\u1a80-\u1a89\u1a90-\u1a99\u1b50-\u1b59\u1bb0-\u1bb9\u1c40-\u1c49\u1c50-\u1c59\ua620-\ua629\ua8d0-\ua8d9\ua900-\ua909\ua9d0-\ua9d9\ua9f0-\ua9f9\uaa50-\uaa59\uabf0-\uabf9\uff10-\uff19\u{104a0}-\u{104a9}\u{10d30}-\u{10d39}\u{10d40}-\u{10d49}\u{11066}-\u{1106f}\u{110f0}-\u{110f9}\u{11136}-\u{1113f}\u{111d0}-\u{111d9}\u{112f0}-\u{112f9}\u{11450}-\u{11459}\u{114d0}-\u{114d9}\u{11650}-\u{11659}\u{116c0}-\u{116c9}\u{116d0}-\u{116e3}\u{11730}-\u{11739}\u{118e0}-\u{118e9}\u{11950}-\u{11959}\u{11bf0}-\u{11bf9}\u{11c50}-\u{11c59}\u{11d50}-\u{11d59}\u{11da0}-\u{11da9}\u{11f50}-\u{11f59}\u{16130}-\u{16139}\u{16a60}-\u{16a69}\u{16ac0}-\u{16ac9}\u{16b50}-\u{16b59}\u{16d70}-\u{16d79}\u{1ccf0}-\u{1ccf9}\u{1d7ce}-\u{1d7ff}\u{1e140}-\u{1e149}\u{1e2f0}-\u{1e2f9}\u{1e4f0}-\u{1e4f9}\u{1e5f1}-\u{1e5fa}\u{1e950}-\u{1e959}\u{1fbf0}-\u{1fbf9}]+$/u;
// str.isdigit(): decimal + Numeric_Type=Digit.
const __RE_DIGIT = /^[\u0030-\u0039\u00b2\u00b3\u00b9\u0660-\u0669\u06f0-\u06f9\u07c0-\u07c9\u0966-\u096f\u09e6-\u09ef\u0a66-\u0a6f\u0ae6-\u0aef\u0b66-\u0b6f\u0be6-\u0bef\u0c66-\u0c6f\u0ce6-\u0cef\u0d66-\u0d6f\u0de6-\u0def\u0e50-\u0e59\u0ed0-\u0ed9\u0f20-\u0f29\u1040-\u1049\u1090-\u1099\u1369-\u1371\u17e0-\u17e9\u1810-\u1819\u1946-\u194f\u19d0-\u19da\u1a80-\u1a89\u1a90-\u1a99\u1b50-\u1b59\u1bb0-\u1bb9\u1c40-\u1c49\u1c50-\u1c59\u2070\u2074-\u2079\u2080-\u2089\u2460-\u2468\u2474-\u247c\u2488-\u2490\u24ea\u24f5-\u24fd\u24ff\u2776-\u277e\u2780-\u2788\u278a-\u2792\ua620-\ua629\ua8d0-\ua8d9\ua900-\ua909\ua9d0-\ua9d9\ua9f0-\ua9f9\uaa50-\uaa59\uabf0-\uabf9\uff10-\uff19\u{104a0}-\u{104a9}\u{10a40}-\u{10a43}\u{10d30}-\u{10d39}\u{10d40}-\u{10d49}\u{10e60}-\u{10e68}\u{11052}-\u{1105a}\u{11066}-\u{1106f}\u{110f0}-\u{110f9}\u{11136}-\u{1113f}\u{111d0}-\u{111d9}\u{112f0}-\u{112f9}\u{11450}-\u{11459}\u{114d0}-\u{114d9}\u{11650}-\u{11659}\u{116c0}-\u{116c9}\u{116d0}-\u{116e3}\u{11730}-\u{11739}\u{118e0}-\u{118e9}\u{11950}-\u{11959}\u{11bf0}-\u{11bf9}\u{11c50}-\u{11c59}\u{11d50}-\u{11d59}\u{11da0}-\u{11da9}\u{11f50}-\u{11f59}\u{16130}-\u{16139}\u{16a60}-\u{16a69}\u{16ac0}-\u{16ac9}\u{16b50}-\u{16b59}\u{16d70}-\u{16d79}\u{1ccf0}-\u{1ccf9}\u{1d7ce}-\u{1d7ff}\u{1e140}-\u{1e149}\u{1e2f0}-\u{1e2f9}\u{1e4f0}-\u{1e4f9}\u{1e5f1}-\u{1e5fa}\u{1e950}-\u{1e959}\u{1f100}-\u{1f10a}\u{1fbf0}-\u{1fbf9}]+$/u;
// str.isnumeric(): digit + Numeric_Type=Numeric (incl. CJK numerals, fractions).
const __RE_NUMERIC = /^[\u0030-\u0039\u00b2\u00b3\u00b9\u00bc-\u00be\u0660-\u0669\u06f0-\u06f9\u07c0-\u07c9\u0966-\u096f\u09e6-\u09ef\u09f4-\u09f9\u0a66-\u0a6f\u0ae6-\u0aef\u0b66-\u0b6f\u0b72-\u0b77\u0be6-\u0bf2\u0c66-\u0c6f\u0c78-\u0c7e\u0ce6-\u0cef\u0d58-\u0d5e\u0d66-\u0d78\u0de6-\u0def\u0e50-\u0e59\u0ed0-\u0ed9\u0f20-\u0f33\u1040-\u1049\u1090-\u1099\u1369-\u137c\u16ee-\u16f0\u17e0-\u17e9\u17f0-\u17f9\u1810-\u1819\u1946-\u194f\u19d0-\u19da\u1a80-\u1a89\u1a90-\u1a99\u1b50-\u1b59\u1bb0-\u1bb9\u1c40-\u1c49\u1c50-\u1c59\u2070\u2074-\u2079\u2080-\u2089\u2150-\u2182\u2185-\u2189\u2460-\u249b\u24ea-\u24ff\u2776-\u2793\u2cfd\u3007\u3021-\u3029\u3038-\u303a\u3192-\u3195\u3220-\u3229\u3248-\u324f\u3251-\u325f\u3280-\u3289\u32b1-\u32bf\u3405\u3483\u382a\u3b4d\u4e00\u4e03\u4e07\u4e09\u4e24\u4e5d\u4e8c\u4e94\u4e96\u4eac\u4ebf\u4ec0\u4edf\u4ee8\u4f0d\u4f70\u4fe9\u5006\u5104\u5146\u5169\u516b\u516d\u5341\u5343-\u5345\u534c\u53c1-\u53c4\u56db\u58f1\u58f9\u5e7a\u5efe\u5eff\u5f0c-\u5f0e\u5f10\u62d0\u62fe\u634c\u67d2\u6d1e\u6f06\u7396\u767e\u7695\u79ed\u8086\u842c\u8cae\u8cb3\u8d30\u920e\u94a9\u9621\u9646\u964c\u9678\u96f6\ua620-\ua629\ua6e6-\ua6ef\ua830-\ua835\ua8d0-\ua8d9\ua900-\ua909\ua9d0-\ua9d9\ua9f0-\ua9f9\uaa50-\uaa59\uabf0-\uabf9\uf96b\uf973\uf978\uf9b2\uf9d1\uf9d3\uf9fd\uff10-\uff19\u{10107}-\u{10133}\u{10140}-\u{10178}\u{1018a}\u{1018b}\u{102e1}-\u{102fb}\u{10320}-\u{10323}\u{10341}\u{1034a}\u{103d1}-\u{103d5}\u{104a0}-\u{104a9}\u{10858}-\u{1085f}\u{10879}-\u{1087f}\u{108a7}-\u{108af}\u{108fb}-\u{108ff}\u{10916}-\u{1091b}\u{109bc}\u{109bd}\u{109c0}-\u{109cf}\u{109d2}-\u{109ff}\u{10a40}-\u{10a48}\u{10a7d}\u{10a7e}\u{10a9d}-\u{10a9f}\u{10aeb}-\u{10aef}\u{10b58}-\u{10b5f}\u{10b78}-\u{10b7f}\u{10ba9}-\u{10baf}\u{10cfa}-\u{10cff}\u{10d30}-\u{10d39}\u{10d40}-\u{10d49}\u{10e60}-\u{10e7e}\u{10f1d}-\u{10f26}\u{10f51}-\u{10f54}\u{10fc5}-\u{10fcb}\u{11052}-\u{1106f}\u{110f0}-\u{110f9}\u{11136}-\u{1113f}\u{111d0}-\u{111d9}\u{111e1}-\u{111f4}\u{112f0}-\u{112f9}\u{11450}-\u{11459}\u{114d0}-\u{114d9}\u{11650}-\u{11659}\u{116c0}-\u{116c9}\u{116d0}-\u{116e3}\u{11730}-\u{1173b}\u{118e0}-\u{118f2}\u{11950}-\u{11959}\u{11bf0}-\u{11bf9}\u{11c50}-\u{11c6c}\u{11d50}-\u{11d59}\u{11da0}-\u{11da9}\u{11f50}-\u{11f59}\u{11fc0}-\u{11fd4}\u{12400}-\u{1246e}\u{16130}-\u{16139}\u{16a60}-\u{16a69}\u{16ac0}-\u{16ac9}\u{16b50}-\u{16b59}\u{16b5b}-\u{16b61}\u{16d70}-\u{16d79}\u{16e80}-\u{16e96}\u{1ccf0}-\u{1ccf9}\u{1d2c0}-\u{1d2d3}\u{1d2e0}-\u{1d2f3}\u{1d360}-\u{1d378}\u{1d7ce}-\u{1d7ff}\u{1e140}-\u{1e149}\u{1e2f0}-\u{1e2f9}\u{1e4f0}-\u{1e4f9}\u{1e5f1}-\u{1e5fa}\u{1e8c7}-\u{1e8cf}\u{1e950}-\u{1e959}\u{1ec71}-\u{1ecab}\u{1ecad}-\u{1ecaf}\u{1ecb1}-\u{1ecb4}\u{1ed01}-\u{1ed2d}\u{1ed2f}-\u{1ed3d}\u{1f100}-\u{1f10c}\u{1fbf0}-\u{1fbf9}\u{20001}\u{20064}\u{200e2}\u{20121}\u{2092a}\u{20983}\u{2098c}\u{2099c}\u{20aea}\u{20afd}\u{20b19}\u{22390}\u{22998}\u{23b1b}\u{2626d}\u{2f890}]+$/u;
// str.casefold(): code points whose full case fold differs from lower() —
// the residual applied on top of toLowerCase (~ lower()).
const __CASEFOLD_MAP = new Map([
    [0xb5, "\u03bc"],
    [0xdf, "ss"],
    [0x149, "\u02bcn"],
    [0x17f, "s"],
    [0x1f0, "j\u030c"],
    [0x345, "\u03b9"],
    [0x390, "\u03b9\u0308\u0301"],
    [0x3b0, "\u03c5\u0308\u0301"],
    [0x3c2, "\u03c3"],
    [0x3d0, "\u03b2"],
    [0x3d1, "\u03b8"],
    [0x3d5, "\u03c6"],
    [0x3d6, "\u03c0"],
    [0x3f0, "\u03ba"],
    [0x3f1, "\u03c1"],
    [0x3f5, "\u03b5"],
    [0x587, "\u0565\u0582"],
    [0x13a0, "\u13a0"],
    [0x13a1, "\u13a1"],
    [0x13a2, "\u13a2"],
    [0x13a3, "\u13a3"],
    [0x13a4, "\u13a4"],
    [0x13a5, "\u13a5"],
    [0x13a6, "\u13a6"],
    [0x13a7, "\u13a7"],
    [0x13a8, "\u13a8"],
    [0x13a9, "\u13a9"],
    [0x13aa, "\u13aa"],
    [0x13ab, "\u13ab"],
    [0x13ac, "\u13ac"],
    [0x13ad, "\u13ad"],
    [0x13ae, "\u13ae"],
    [0x13af, "\u13af"],
    [0x13b0, "\u13b0"],
    [0x13b1, "\u13b1"],
    [0x13b2, "\u13b2"],
    [0x13b3, "\u13b3"],
    [0x13b4, "\u13b4"],
    [0x13b5, "\u13b5"],
    [0x13b6, "\u13b6"],
    [0x13b7, "\u13b7"],
    [0x13b8, "\u13b8"],
    [0x13b9, "\u13b9"],
    [0x13ba, "\u13ba"],
    [0x13bb, "\u13bb"],
    [0x13bc, "\u13bc"],
    [0x13bd, "\u13bd"],
    [0x13be, "\u13be"],
    [0x13bf, "\u13bf"],
    [0x13c0, "\u13c0"],
    [0x13c1, "\u13c1"],
    [0x13c2, "\u13c2"],
    [0x13c3, "\u13c3"],
    [0x13c4, "\u13c4"],
    [0x13c5, "\u13c5"],
    [0x13c6, "\u13c6"],
    [0x13c7, "\u13c7"],
    [0x13c8, "\u13c8"],
    [0x13c9, "\u13c9"],
    [0x13ca, "\u13ca"],
    [0x13cb, "\u13cb"],
    [0x13cc, "\u13cc"],
    [0x13cd, "\u13cd"],
    [0x13ce, "\u13ce"],
    [0x13cf, "\u13cf"],
    [0x13d0, "\u13d0"],
    [0x13d1, "\u13d1"],
    [0x13d2, "\u13d2"],
    [0x13d3, "\u13d3"],
    [0x13d4, "\u13d4"],
    [0x13d5, "\u13d5"],
    [0x13d6, "\u13d6"],
    [0x13d7, "\u13d7"],
    [0x13d8, "\u13d8"],
    [0x13d9, "\u13d9"],
    [0x13da, "\u13da"],
    [0x13db, "\u13db"],
    [0x13dc, "\u13dc"],
    [0x13dd, "\u13dd"],
    [0x13de, "\u13de"],
    [0x13df, "\u13df"],
    [0x13e0, "\u13e0"],
    [0x13e1, "\u13e1"],
    [0x13e2, "\u13e2"],
    [0x13e3, "\u13e3"],
    [0x13e4, "\u13e4"],
    [0x13e5, "\u13e5"],
    [0x13e6, "\u13e6"],
    [0x13e7, "\u13e7"],
    [0x13e8, "\u13e8"],
    [0x13e9, "\u13e9"],
    [0x13ea, "\u13ea"],
    [0x13eb, "\u13eb"],
    [0x13ec, "\u13ec"],
    [0x13ed, "\u13ed"],
    [0x13ee, "\u13ee"],
    [0x13ef, "\u13ef"],
    [0x13f0, "\u13f0"],
    [0x13f1, "\u13f1"],
    [0x13f2, "\u13f2"],
    [0x13f3, "\u13f3"],
    [0x13f4, "\u13f4"],
    [0x13f5, "\u13f5"],
    [0x13f8, "\u13f0"],
    [0x13f9, "\u13f1"],
    [0x13fa, "\u13f2"],
    [0x13fb, "\u13f3"],
    [0x13fc, "\u13f4"],
    [0x13fd, "\u13f5"],
    [0x1c80, "\u0432"],
    [0x1c81, "\u0434"],
    [0x1c82, "\u043e"],
    [0x1c83, "\u0441"],
    [0x1c84, "\u0442"],
    [0x1c85, "\u0442"],
    [0x1c86, "\u044a"],
    [0x1c87, "\u0463"],
    [0x1c88, "\ua64b"],
    [0x1e96, "h\u0331"],
    [0x1e97, "t\u0308"],
    [0x1e98, "w\u030a"],
    [0x1e99, "y\u030a"],
    [0x1e9a, "a\u02be"],
    [0x1e9b, "\u1e61"],
    [0x1e9e, "ss"],
    [0x1f50, "\u03c5\u0313"],
    [0x1f52, "\u03c5\u0313\u0300"],
    [0x1f54, "\u03c5\u0313\u0301"],
    [0x1f56, "\u03c5\u0313\u0342"],
    [0x1f80, "\u1f00\u03b9"],
    [0x1f81, "\u1f01\u03b9"],
    [0x1f82, "\u1f02\u03b9"],
    [0x1f83, "\u1f03\u03b9"],
    [0x1f84, "\u1f04\u03b9"],
    [0x1f85, "\u1f05\u03b9"],
    [0x1f86, "\u1f06\u03b9"],
    [0x1f87, "\u1f07\u03b9"],
    [0x1f88, "\u1f00\u03b9"],
    [0x1f89, "\u1f01\u03b9"],
    [0x1f8a, "\u1f02\u03b9"],
    [0x1f8b, "\u1f03\u03b9"],
    [0x1f8c, "\u1f04\u03b9"],
    [0x1f8d, "\u1f05\u03b9"],
    [0x1f8e, "\u1f06\u03b9"],
    [0x1f8f, "\u1f07\u03b9"],
    [0x1f90, "\u1f20\u03b9"],
    [0x1f91, "\u1f21\u03b9"],
    [0x1f92, "\u1f22\u03b9"],
    [0x1f93, "\u1f23\u03b9"],
    [0x1f94, "\u1f24\u03b9"],
    [0x1f95, "\u1f25\u03b9"],
    [0x1f96, "\u1f26\u03b9"],
    [0x1f97, "\u1f27\u03b9"],
    [0x1f98, "\u1f20\u03b9"],
    [0x1f99, "\u1f21\u03b9"],
    [0x1f9a, "\u1f22\u03b9"],
    [0x1f9b, "\u1f23\u03b9"],
    [0x1f9c, "\u1f24\u03b9"],
    [0x1f9d, "\u1f25\u03b9"],
    [0x1f9e, "\u1f26\u03b9"],
    [0x1f9f, "\u1f27\u03b9"],
    [0x1fa0, "\u1f60\u03b9"],
    [0x1fa1, "\u1f61\u03b9"],
    [0x1fa2, "\u1f62\u03b9"],
    [0x1fa3, "\u1f63\u03b9"],
    [0x1fa4, "\u1f64\u03b9"],
    [0x1fa5, "\u1f65\u03b9"],
    [0x1fa6, "\u1f66\u03b9"],
    [0x1fa7, "\u1f67\u03b9"],
    [0x1fa8, "\u1f60\u03b9"],
    [0x1fa9, "\u1f61\u03b9"],
    [0x1faa, "\u1f62\u03b9"],
    [0x1fab, "\u1f63\u03b9"],
    [0x1fac, "\u1f64\u03b9"],
    [0x1fad, "\u1f65\u03b9"],
    [0x1fae, "\u1f66\u03b9"],
    [0x1faf, "\u1f67\u03b9"],
    [0x1fb2, "\u1f70\u03b9"],
    [0x1fb3, "\u03b1\u03b9"],
    [0x1fb4, "\u03ac\u03b9"],
    [0x1fb6, "\u03b1\u0342"],
    [0x1fb7, "\u03b1\u0342\u03b9"],
    [0x1fbc, "\u03b1\u03b9"],
    [0x1fbe, "\u03b9"],
    [0x1fc2, "\u1f74\u03b9"],
    [0x1fc3, "\u03b7\u03b9"],
    [0x1fc4, "\u03ae\u03b9"],
    [0x1fc6, "\u03b7\u0342"],
    [0x1fc7, "\u03b7\u0342\u03b9"],
    [0x1fcc, "\u03b7\u03b9"],
    [0x1fd2, "\u03b9\u0308\u0300"],
    [0x1fd3, "\u03b9\u0308\u0301"],
    [0x1fd6, "\u03b9\u0342"],
    [0x1fd7, "\u03b9\u0308\u0342"],
    [0x1fe2, "\u03c5\u0308\u0300"],
    [0x1fe3, "\u03c5\u0308\u0301"],
    [0x1fe4, "\u03c1\u0313"],
    [0x1fe6, "\u03c5\u0342"],
    [0x1fe7, "\u03c5\u0308\u0342"],
    [0x1ff2, "\u1f7c\u03b9"],
    [0x1ff3, "\u03c9\u03b9"],
    [0x1ff4, "\u03ce\u03b9"],
    [0x1ff6, "\u03c9\u0342"],
    [0x1ff7, "\u03c9\u0342\u03b9"],
    [0x1ffc, "\u03c9\u03b9"],
    [0xab70, "\u13a0"],
    [0xab71, "\u13a1"],
    [0xab72, "\u13a2"],
    [0xab73, "\u13a3"],
    [0xab74, "\u13a4"],
    [0xab75, "\u13a5"],
    [0xab76, "\u13a6"],
    [0xab77, "\u13a7"],
    [0xab78, "\u13a8"],
    [0xab79, "\u13a9"],
    [0xab7a, "\u13aa"],
    [0xab7b, "\u13ab"],
    [0xab7c, "\u13ac"],
    [0xab7d, "\u13ad"],
    [0xab7e, "\u13ae"],
    [0xab7f, "\u13af"],
    [0xab80, "\u13b0"],
    [0xab81, "\u13b1"],
    [0xab82, "\u13b2"],
    [0xab83, "\u13b3"],
    [0xab84, "\u13b4"],
    [0xab85, "\u13b5"],
    [0xab86, "\u13b6"],
    [0xab87, "\u13b7"],
    [0xab88, "\u13b8"],
    [0xab89, "\u13b9"],
    [0xab8a, "\u13ba"],
    [0xab8b, "\u13bb"],
    [0xab8c, "\u13bc"],
    [0xab8d, "\u13bd"],
    [0xab8e, "\u13be"],
    [0xab8f, "\u13bf"],
    [0xab90, "\u13c0"],
    [0xab91, "\u13c1"],
    [0xab92, "\u13c2"],
    [0xab93, "\u13c3"],
    [0xab94, "\u13c4"],
    [0xab95, "\u13c5"],
    [0xab96, "\u13c6"],
    [0xab97, "\u13c7"],
    [0xab98, "\u13c8"],
    [0xab99, "\u13c9"],
    [0xab9a, "\u13ca"],
    [0xab9b, "\u13cb"],
    [0xab9c, "\u13cc"],
    [0xab9d, "\u13cd"],
    [0xab9e, "\u13ce"],
    [0xab9f, "\u13cf"],
    [0xaba0, "\u13d0"],
    [0xaba1, "\u13d1"],
    [0xaba2, "\u13d2"],
    [0xaba3, "\u13d3"],
    [0xaba4, "\u13d4"],
    [0xaba5, "\u13d5"],
    [0xaba6, "\u13d6"],
    [0xaba7, "\u13d7"],
    [0xaba8, "\u13d8"],
    [0xaba9, "\u13d9"],
    [0xabaa, "\u13da"],
    [0xabab, "\u13db"],
    [0xabac, "\u13dc"],
    [0xabad, "\u13dd"],
    [0xabae, "\u13de"],
    [0xabaf, "\u13df"],
    [0xabb0, "\u13e0"],
    [0xabb1, "\u13e1"],
    [0xabb2, "\u13e2"],
    [0xabb3, "\u13e3"],
    [0xabb4, "\u13e4"],
    [0xabb5, "\u13e5"],
    [0xabb6, "\u13e6"],
    [0xabb7, "\u13e7"],
    [0xabb8, "\u13e8"],
    [0xabb9, "\u13e9"],
    [0xabba, "\u13ea"],
    [0xabbb, "\u13eb"],
    [0xabbc, "\u13ec"],
    [0xabbd, "\u13ed"],
    [0xabbe, "\u13ee"],
    [0xabbf, "\u13ef"],
    [0xfb00, "ff"],
    [0xfb01, "fi"],
    [0xfb02, "fl"],
    [0xfb03, "ffi"],
    [0xfb04, "ffl"],
    [0xfb05, "st"],
    [0xfb06, "st"],
    [0xfb13, "\u0574\u0576"],
    [0xfb14, "\u0574\u0565"],
    [0xfb15, "\u0574\u056b"],
    [0xfb16, "\u057e\u0576"],
    [0xfb17, "\u0574\u056d"],
]);
// Titlecase-first mapping (capitalize()/title() word starts) where it
// differs from upper() — digraphs and ligature expansions.
const __TITLE_MAP = new Map([
    [0xdf, "Ss"],
    [0x1c4, "\u01c5"],
    [0x1c5, "\u01c5"],
    [0x1c6, "\u01c5"],
    [0x1c7, "\u01c8"],
    [0x1c8, "\u01c8"],
    [0x1c9, "\u01c8"],
    [0x1ca, "\u01cb"],
    [0x1cb, "\u01cb"],
    [0x1cc, "\u01cb"],
    [0x1f1, "\u01f2"],
    [0x1f2, "\u01f2"],
    [0x1f3, "\u01f2"],
    [0x587, "\u0535\u0582"],
    [0x10d0, "\u10d0"],
    [0x10d1, "\u10d1"],
    [0x10d2, "\u10d2"],
    [0x10d3, "\u10d3"],
    [0x10d4, "\u10d4"],
    [0x10d5, "\u10d5"],
    [0x10d6, "\u10d6"],
    [0x10d7, "\u10d7"],
    [0x10d8, "\u10d8"],
    [0x10d9, "\u10d9"],
    [0x10da, "\u10da"],
    [0x10db, "\u10db"],
    [0x10dc, "\u10dc"],
    [0x10dd, "\u10dd"],
    [0x10de, "\u10de"],
    [0x10df, "\u10df"],
    [0x10e0, "\u10e0"],
    [0x10e1, "\u10e1"],
    [0x10e2, "\u10e2"],
    [0x10e3, "\u10e3"],
    [0x10e4, "\u10e4"],
    [0x10e5, "\u10e5"],
    [0x10e6, "\u10e6"],
    [0x10e7, "\u10e7"],
    [0x10e8, "\u10e8"],
    [0x10e9, "\u10e9"],
    [0x10ea, "\u10ea"],
    [0x10eb, "\u10eb"],
    [0x10ec, "\u10ec"],
    [0x10ed, "\u10ed"],
    [0x10ee, "\u10ee"],
    [0x10ef, "\u10ef"],
    [0x10f0, "\u10f0"],
    [0x10f1, "\u10f1"],
    [0x10f2, "\u10f2"],
    [0x10f3, "\u10f3"],
    [0x10f4, "\u10f4"],
    [0x10f5, "\u10f5"],
    [0x10f6, "\u10f6"],
    [0x10f7, "\u10f7"],
    [0x10f8, "\u10f8"],
    [0x10f9, "\u10f9"],
    [0x10fa, "\u10fa"],
    [0x10fd, "\u10fd"],
    [0x10fe, "\u10fe"],
    [0x10ff, "\u10ff"],
    [0x1f80, "\u1f88"],
    [0x1f81, "\u1f89"],
    [0x1f82, "\u1f8a"],
    [0x1f83, "\u1f8b"],
    [0x1f84, "\u1f8c"],
    [0x1f85, "\u1f8d"],
    [0x1f86, "\u1f8e"],
    [0x1f87, "\u1f8f"],
    [0x1f88, "\u1f88"],
    [0x1f89, "\u1f89"],
    [0x1f8a, "\u1f8a"],
    [0x1f8b, "\u1f8b"],
    [0x1f8c, "\u1f8c"],
    [0x1f8d, "\u1f8d"],
    [0x1f8e, "\u1f8e"],
    [0x1f8f, "\u1f8f"],
    [0x1f90, "\u1f98"],
    [0x1f91, "\u1f99"],
    [0x1f92, "\u1f9a"],
    [0x1f93, "\u1f9b"],
    [0x1f94, "\u1f9c"],
    [0x1f95, "\u1f9d"],
    [0x1f96, "\u1f9e"],
    [0x1f97, "\u1f9f"],
    [0x1f98, "\u1f98"],
    [0x1f99, "\u1f99"],
    [0x1f9a, "\u1f9a"],
    [0x1f9b, "\u1f9b"],
    [0x1f9c, "\u1f9c"],
    [0x1f9d, "\u1f9d"],
    [0x1f9e, "\u1f9e"],
    [0x1f9f, "\u1f9f"],
    [0x1fa0, "\u1fa8"],
    [0x1fa1, "\u1fa9"],
    [0x1fa2, "\u1faa"],
    [0x1fa3, "\u1fab"],
    [0x1fa4, "\u1fac"],
    [0x1fa5, "\u1fad"],
    [0x1fa6, "\u1fae"],
    [0x1fa7, "\u1faf"],
    [0x1fa8, "\u1fa8"],
    [0x1fa9, "\u1fa9"],
    [0x1faa, "\u1faa"],
    [0x1fab, "\u1fab"],
    [0x1fac, "\u1fac"],
    [0x1fad, "\u1fad"],
    [0x1fae, "\u1fae"],
    [0x1faf, "\u1faf"],
    [0x1fb2, "\u1fba\u0345"],
    [0x1fb3, "\u1fbc"],
    [0x1fb4, "\u0386\u0345"],
    [0x1fb7, "\u0391\u0342\u0345"],
    [0x1fbc, "\u1fbc"],
    [0x1fc2, "\u1fca\u0345"],
    [0x1fc3, "\u1fcc"],
    [0x1fc4, "\u0389\u0345"],
    [0x1fc7, "\u0397\u0342\u0345"],
    [0x1fcc, "\u1fcc"],
    [0x1ff2, "\u1ffa\u0345"],
    [0x1ff3, "\u1ffc"],
    [0x1ff4, "\u038f\u0345"],
    [0x1ff7, "\u03a9\u0342\u0345"],
    [0x1ffc, "\u1ffc"],
    [0xfb00, "Ff"],
    [0xfb01, "Fi"],
    [0xfb02, "Fl"],
    [0xfb03, "Ffi"],
    [0xfb04, "Ffl"],
    [0xfb05, "St"],
    [0xfb06, "St"],
    [0xfb13, "\u0544\u0576"],
    [0xfb14, "\u0544\u0565"],
    [0xfb15, "\u0544\u056b"],
    [0xfb16, "\u054e\u0576"],
    [0xfb17, "\u0544\u056d"],
]);
// === END GENERATED UNICODE TABLES ===

// Python str whitespace (str.isspace / split / strip default): differs from
// JS \s — includes \x1c-\x1f and \x85, EXCLUDES the BOM \ufeff.
const __PY_WS_CC = " \\t\\n\\r\\v\\f\\u001c-\\u001f\\u0085\\u00a0\\u1680\\u2000-\\u200a\\u2028\\u2029\\u202f\\u205f\\u3000";
const __PY_WS_RE = new RegExp("[" + __PY_WS_CC + "]");
const __PY_WS_LEAD = new RegExp("^[" + __PY_WS_CC + "]+");
const __PY_WS_TRAIL = new RegExp("[" + __PY_WS_CC + "]+$");
const __PY_WS_RUN = new RegExp("[" + __PY_WS_CC + "]+", "g");
// str.isprintable(): CPython rejects Cc/Cf/Cs/Co/Cn/Zl/Zp/Zs except space.
const __RE_UNPRINTABLE = /[\p{Cc}\p{Cf}\p{Cs}\p{Co}\p{Cn}\p{Zl}\p{Zp}\p{Zs}]/u;

/** Python `s.join(iterable)` — any iterable of str; a non-str item raises
 * CPython's TypeError with the item's index. */
export function pyStrJoin(sep, iter) {
    if (typeof sep !== "string" && sep != null && typeof sep.join === "function") {
        return sep.join(iter);
    }
    const parts = [];
    let idx = 0;
    for (const item of pyForIter(iter)) {
        if (typeof item !== "string") {
            throw new TypeError_(
                `sequence item ${idx}: expected str instance, ${__pyTypeName(item)} found`,
            );
        }
        parts.push(item);
        idx++;
    }
    return parts.join(sep);
}

/** Python `s.split()` (whitespace runs) and `s.split(sep[, maxsplit])` —
 * full CPython spec: keyword forms, maxsplit in both modes, the PYTHON
 * whitespace set (`\x1c–\x1f`, `\x85`, Zs — NOT JS `\s`, which adds
 * `﻿`), empty separator ValueError, sep-type TypeError. */
export function pyStrSplit(s, sep, maxsplit) {
    // #301: non-string receivers with their own .split dispatch natively.
    if (typeof s !== "string" && s != null && typeof s.split === "function") {
        return s.split(sep, maxsplit);
    }
    // #213: accept the keyword forms `split(sep=..., maxsplit=...)` — which
    // the compiler lowers to a trailing options object — as well as the
    // positional `split(sep, maxsplit)`, and actually honor maxsplit.
    // E3 r3: the bag must CARRY one of the keyword names — a bare object is
    // a positional value (an __index__-bearing maxsplit was swallowed as an
    // empty bag and silently ignored; a dict sep must reach the "must be
    // str or None, not dict" TypeError).
    const optsFrom = (v) =>
        v !== null && typeof v === "object" && !Array.isArray(v) && v.__pyfloat__ !== true
        && typeof v.__index__ !== "function"
        && ("sep" in v || "maxsplit" in v);
    if (optsFrom(sep)) {
        if ("maxsplit" in sep) maxsplit = sep.maxsplit;
        sep = "sep" in sep ? sep.sep : undefined;
    } else if (optsFrom(maxsplit)) {
        maxsplit = "maxsplit" in maxsplit ? maxsplit.maxsplit : undefined;
    }
    if (maxsplit !== undefined && maxsplit !== null) maxsplit = __strIndexArg(maxsplit);
    const lim =
        maxsplit === undefined || maxsplit === null || maxsplit < 0
            ? Infinity
            : maxsplit;

    if (sep === undefined || sep === null) {
        // Whitespace split: collapse runs of whitespace, drop empties, and
        // (for a finite maxsplit) keep the unsplit remainder as the last part.
        const trimmed = s.replace(__PY_WS_LEAD, "");
        if (trimmed === "") return [];
        if (lim === Infinity) {
            return trimmed.replace(__PY_WS_TRAIL, "").split(__PY_WS_RUN);
        }
        const out = [];
        let i = 0;
        while (out.length < lim) {
            while (i < trimmed.length && __PY_WS_RE.test(trimmed[i])) i++;
            if (i >= trimmed.length) break;
            let start = i;
            while (i < trimmed.length && !__PY_WS_RE.test(trimmed[i])) i++;
            out.push(trimmed.slice(start, i));
        }
        while (i < trimmed.length && __PY_WS_RE.test(trimmed[i])) i++;
        // E3 r3: the unsplit REMAINDER keeps its trailing whitespace —
        // CPython '  a b  '.split(None, 0) == ['a b  '] (only the leading
        // run before the remainder is consumed; nothing is trimmed off the
        // right).
        if (i < trimmed.length) out.push(trimmed.slice(i));
        return out;
    }

    if (typeof sep !== "string") {
        throw new TypeError_(`must be str or None, not ${__pyTypeName(sep)}`);
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

/** Python `s.title()` — the first cased char of each word TITLECASED
 * (digraphs like ǆ → ǅ via __TITLE_MAP), the rest of the word lowercased.
 * CPython's word boundary is any non-cased char (`"it's".title()` is
 * `"It'S"` — issue #87); "cased" is the Unicode Cased property (covers
 * Other_Lowercase modifiers, unlike a lower!=upper probe). */
export function pyStrTitle(s) {
    if (typeof s !== "string" && s != null && typeof s.title === "function") {
        return s.title();
    }
    let out = "";
    let prevCased = false;
    for (const ch of s) {
        if (!/\p{Cased}/u.test(ch)) {
            out += ch;
            prevCased = false;
            continue;
        }
        if (prevCased) {
            out += ch.toLowerCase();
        } else {
            const t = __TITLE_MAP.get(ch.codePointAt(0));
            out += t !== undefined ? t : ch.toUpperCase();
        }
        prevCased = true;
    }
    return out;
}

/** Python `s.capitalize()` — first character TITLECASED (3.8+ semantics:
 * ǆ → ǅ, ß → Ss), the rest lowercased. Code-point-aware. */
export function pyStrCapitalize(s) {
    if (typeof s !== "string" && s != null && typeof s.capitalize === "function") {
        return s.capitalize();
    }
    if (!s) return s;
    const cps = [...s];
    const t = __TITLE_MAP.get(cps[0].codePointAt(0));
    return (t !== undefined ? t : cps[0].toUpperCase()) + cps.slice(1).join("").toLowerCase();
}

/** Python `s.strip(chars)` — strip any chars in the set from both ends;
 * default = the PYTHON whitespace set (not JS `\s`). */
export function pyStrStrip(s, chars) {
    if (chars === undefined || chars === null) {
        return s.replace(__PY_WS_LEAD, "").replace(__PY_WS_TRAIL, "");
    }
    if (typeof chars !== "string") {
        throw new TypeError_("strip arg must be None or str");
    }
    return pyStrLstrip(pyStrRstrip(s, chars), chars);
}

/** Python `s.lstrip(chars)` — strip leading chars in set.
 * Wave-19 verification fix: iterate CODE POINTS — `new Set(chars)` holds
 * whole astral chars while `s[i]` is one UTF-16 unit, so the old unit-wise
 * walk never matched an astral strip char ('𝔸a𝔸'.strip('𝔸') was a no-op). */
export function pyStrLstrip(s, chars) {
    if (chars === undefined || chars === null) return s.replace(__PY_WS_LEAD, "");
    if (typeof chars !== "string") {
        throw new TypeError_("lstrip arg must be None or str");
    }
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
    if (chars === undefined || chars === null) return s.replace(__PY_WS_TRAIL, "");
    if (typeof chars !== "string") {
        throw new TypeError_("rstrip arg must be None or str");
    }
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
    // Option B: a boxed (integer-valued) float unwraps ONCE at entry, and its
    // floatness is remembered — `f"{8.0:>6}"` renders "   8.0", not "     8".
    if (value != null && value.__pyfloat__ === true) {
        isFloat = true;
        value = value.valueOf();
    }
    const __e = (msg) => { throw new ValueError(msg); };
    // Is the spec completely empty (no formatting effect)? `raw` is carried
    // for the __format__ protocol and does not count.
    const emptySpec = opts.fill == null && opts.align == null && opts.sign == null
        && !opts.z && !opts.alt && !opts.zero && opts.width == null
        && opts.grouping == null && opts.precision == null
        && opts.fracGrouping == null && opts.type == null;

    let ty = opts.type;
    let v = value;
    let kind; // "str" | "int" | "float"
    if (typeof v === "string") kind = "str";
    else if (typeof v === "bigint") kind = "int";
    else if (typeof v === "boolean") {
        // bool.__format__: an EMPTY spec is str(bool) ('True'); any non-empty
        // spec routes through the int value (format(True, '>6') == '     1').
        if (emptySpec) return v ? "True" : "False";
        v = v ? 1 : 0;
        kind = "int";
    } else if (typeof v === "number") {
        kind = isFloat || !Number.isInteger(v) ? "float" : "int";
    } else {
        // Not str/int/float/bool: CPython dispatches type(v).__format__.
        // A user-defined __format__ receives the RAW spec string; otherwise
        // object.__format__ is str(v) for an EMPTY spec and a TypeError for
        // any non-empty one (`format([1], '>8')` raises).
        if (v != null && typeof v.__format__ === "function") {
            return v.__format__(opts.raw != null ? String(opts.raw) : "");
        }
        if (emptySpec) return pyStr(v);
        throw new TypeError_(
            `unsupported format string passed to ${__pyTypeName(value)}.__format__`,
        );
    }
    const tyName = kind === "float" ? "float"
        : typeof value === "boolean" ? "bool"
        : kind; // "str" / "int"

    // Grouping × TYPE compatibility — a GLOBAL check that fires before the
    // per-kind code checks, for every value kind (CPython: format('x', ',x')
    // raises "Cannot specify ',' with 'x'", not the str unknown-code error).
    // ',' groups d/e/E/f/F/g/G/%/none; '_' additionally b/o/x/X; neither
    // works with 'c', 'n' or 's'.
    if (opts.grouping && ty !== undefined) {
        const g = opts.grouping;
        if ((g === "," && "bcnosxX".includes(ty)) || (g === "_" && "cns".includes(ty))) {
            __e(`Cannot specify '${g}' with '${ty}'.`);
        }
    }

    // ----- str-typed values: only 's' (or no type); most flags illegal -----
    if (kind === "str") {
        if (opts.grouping && (ty === undefined || ty === "s")) {
            __e(`Cannot specify '${opts.grouping}' with 's'.`);
        }
        if (ty !== undefined && ty !== "s") {
            __e(`Unknown format code '${ty}' for object of type 'str'`);
        }
        if (opts.sign === " ") __e("Space not allowed in string format specifier");
        if (opts.sign) __e("Sign not allowed in string format specifier");
        if (opts.align === "=") __e("'=' alignment not allowed in string format specifier");
        if (opts.alt) __e("Alternate form (#) not allowed in string format specifier");
        if (opts.z) __e("Negative zero coercion (z) not allowed in string format specifier");
        // Precision truncates by CODE POINTS (astral-safe), like all width math.
        let body = v;
        if (opts.precision != null) body = [...v].slice(0, opts.precision).join("");
        return __specPad("", body, opts, false);
    }

    const INT_CODES = "bcdnoxX";
    const FLOAT_CODES = "eEfFgGn%";
    if (ty !== undefined && !INT_CODES.includes(ty) && !FLOAT_CODES.includes(ty)) {
        // 's' (or another non-numeric code) applied to a numeric value.
        __e(`Unknown format code '${ty}' for object of type '${tyName}'`);
    }
    if (kind === "float" && ty !== undefined && ty !== "n" && INT_CODES.includes(ty)) {
        __e(`Unknown format code '${ty}' for object of type '${tyName}'`);
    }
    const intPresentation = kind === "int" && (ty === undefined || INT_CODES.includes(ty));
    if (opts.z && intPresentation) {
        __e("Negative zero coercion (z) not allowed in integer format specifier");
    }
    if (opts.precision != null && intPresentation) {
        __e("Precision not allowed in integer format specifier");
    }

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

    let digits = "";     // ungrouped integer-part digits (numeric paths)
    let tail = "";       // fraction / exponent / '%' suffix
    let neg = false;
    let prefixStr = "";  // '#' base prefix — padded AFTER it, like the sign
    let groupSize = 3;
    let isNumeric = true;

    if (intPresentation) {
        // ----- integer presentations ------------------------------------
        if (ty === "c") {
            if (opts.sign) __e("Sign not allowed with integer format specifier 'c'");
            if (opts.alt) __e("Alternate form (#) not allowed with integer format specifier 'c'");
            const big = typeof v === "bigint" ? v : BigInt(Math.trunc(Number(v)));
            if (big > 0x7fffffffffffffffn || big < -0x8000000000000000n) {
                throw new OverflowError("Python int too large to convert to C long");
            }
            const n = Number(big);
            if (n < 0 || n >= 0x110000) {
                throw new OverflowError("%c arg not in range(0x110000)");
            }
            digits = String.fromCodePoint(n);
        } else {
            // Keep BigInt ints exact (arbitrary precision) — never round
            // through Number.
            let n = typeof v === "bigint" ? v : Math.trunc(Number(v));
            neg = n < 0;
            if (neg) n = -n;
            const radix = ty === "b" ? 2 : ty === "o" ? 8 : (ty === "x" || ty === "X") ? 16 : 10;
            digits = n.toString(radix);
            if (ty === "X") digits = digits.toUpperCase();
            groupSize = radix === 10 ? 3 : 4;
            // autotester string_format: the '#' prefix sits BETWEEN the sign
            // and any '='/zero padding ('{:#08b}'.format(-15) → '-0b01111',
            // not '-0000b1111'), so it is carried separately (prefixStr joins
            // signStr below) instead of being glued onto the digits.
            if (opts.alt) {
                if (radix === 2) prefixStr = "0b";
                else if (radix === 8) prefixStr = "0o";
                else if (radix === 16) prefixStr = ty === "X" ? "0X" : "0x";
            }
        }
    } else {
        // ----- float presentations (int values convert through Number) ---
        let n = Number(v);
        if (ty === "%") n = n * 100;
        neg = n < 0 || Object.is(n, -0);
        n = Math.abs(n);
        const prec = opts.precision != null ? opts.precision : 6;
        let s;
        if (!Number.isFinite(n)) {
            // Non-finite floats format as inf/nan in every float presentation
            // type; E/F/G uppercase; '%' keeps its suffix.
            s = Number.isNaN(n) ? "nan" : "inf";
            if (ty === "E" || ty === "F" || ty === "G") s = s.toUpperCase();
            if (ty === "%") s += "%";
        } else if (ty === "e" || ty === "E") {
            const r = __sigRound(n, prec + 1);
            const mant = prec > 0
                ? r.digits[0] + "." + r.digits.slice(1)
                : r.digits[0] + (opts.alt ? "." : "");
            s = mant + "e" + (r.exp10 < 0 ? "-" : "+")
                + String(Math.abs(r.exp10)).padStart(2, "0");
            if (ty === "E") s = s.toUpperCase();
        } else if (ty === undefined && opts.precision == null) {
            // No type, no precision: str(float) — shortest round-trip repr.
            s = pyFormatFloat(n);
        } else if (ty === "g" || ty === "G" || ty === "n" || ty === undefined) {
            // CPython 'g' (and 'n' = locale-aware 'g'; PythScribe formats
            // with no locale — documented deviation): with precision p
            // (default 6; 0 → 1), round to p significant digits (HALF-EVEN,
            // like CPython — not toExponential's half-away); exp in
            // [-4, p) → fixed else scientific; trailing zeros stripped
            // unless '#'; exponent ≥ 2 digits.
            //
            // NO type char WITH a precision is the None presentation type —
            // like 'g', except fixed notation keeps at least one digit past
            // the decimal point, and when adding it would exceed the
            // precision's significant-digit budget, scientific wins.
            const noneType = ty === undefined;
            let p = prec;
            if (p === 0) p = 1;
            const r = n === 0 ? { digits: "0".repeat(p), exp10: 0 } : __sigRound(n, p);
            const ds = r.digits;
            const exp10 = r.exp10;
            const sci = () => {
                const mant = opts.alt ? ds : ds.replace(/0+$/, "") || "0";
                let mantStr = mant.length > 1 ? mant[0] + "." + mant.slice(1) : mant;
                if (opts.alt && !mantStr.includes(".")) mantStr += ".";
                return mantStr + "e" + (exp10 < 0 ? "-" : "+")
                    + String(Math.abs(exp10)).padStart(2, "0");
            };
            if (exp10 >= -4 && exp10 < p) {
                if (exp10 >= 0) {
                    s = ds.length <= exp10 + 1
                        ? ds + "0".repeat(exp10 + 1 - ds.length)
                        : ds.slice(0, exp10 + 1) + "." + ds.slice(exp10 + 1);
                } else {
                    s = "0." + "0".repeat(-exp10 - 1) + ds;
                }
                if (!opts.alt && s.includes(".")) s = s.replace(/\.?0+$/, "");
                if (opts.alt && !s.includes(".")) s += ".";
                if (noneType && !s.includes(".")) {
                    // ≥1 digit past the decimal point; if that exceeds the
                    // significant-digit budget, go scientific.
                    s = exp10 + 2 > p ? sci() : s + ".0";
                }
            } else {
                s = sci();
            }
            if (ty === "G") s = s.toUpperCase();
        } else if (ty === "%") {
            // Round-half-even on the exact double, like CPython (#86).
            s = __fixedHalfEven(n, opts.precision != null ? opts.precision : 6) + "%";
            if (opts.alt && !s.includes(".")) s = s.replace("%", ".%");
        } else {
            // 'f' / 'F'
            s = __fixedHalfEven(n, prec);
            // '#' on a float presentation type forces the decimal point
            // ('{:#.0f}'.format(-1552) → '-1552.').
            if (opts.alt && !s.includes(".")) s += ".";
            if (ty === "F") s = s.toUpperCase();
        }
        // PEP 682 'z': coerce a result that rounds to negative zero.
        if (opts.z && neg && !/[1-9a-fA-F]/.test(s.replace(/[eE][+-]\d+/, ""))) {
            neg = false;
        }
        // 3.14 fractional grouping: group the FRACTION digits in 3s,
        // left-to-right from the decimal point ('.123456791' →
        // '.123_456_791'); exponent/'%' suffixes untouched. Ignored for
        // str values (CPython: format('ab', '.3,s') is 'ab').
        if (opts.fracGrouping) {
            const fsep = opts.fracGrouping;
            s = s.replace(/\.([0-9]+)/, (_m, frac) =>
                "." + frac.replace(/([0-9]{3})(?=[0-9])/g, "$1" + fsep));
        }
        // Split into integer digits + tail so grouping/zero-pad see digits.
        const dm = /^([0-9]*)([\s\S]*)$/.exec(s);
        digits = dm[1];
        tail = dm[2];
    }

    // Sign handling for numeric values. The '#' base prefix joins the sign
    // area so '='/zero padding lands between '0b'/'0x' and the digits.
    let signStr = "";
    if (ty !== "c") {
        if (neg) signStr = "-";
        else if (opts.sign === "+") signStr = "+";
        else if (opts.sign === " ") signStr = " ";
    }
    signStr += prefixStr;

    const sep = opts.grouping;
    let body = sep && digits ? group(digits, groupSize, sep) : digits;
    body += tail;

    // Zero-padding with grouping: the fill zeros PARTICIPATE in grouping
    // ('{:011,}'.format(1234) → '000,001,234'; may overshoot the width by
    // one rather than lead with a separator, like CPython).
    const width = opts.width || 0;
    const zeroFill = isNumeric && (opts.fill == null || opts.fill === "0")
        && (opts.zero || opts.align === "=");
    if (sep && zeroFill && width > 0 && digits) {
        let d = digits;
        while ([...signStr].length + [...(group(d, groupSize, sep) + tail)].length < width) {
            d = "0" + d;
        }
        body = group(d, groupSize, sep) + tail;
        return signStr + body;
    }

    return __specPad(signStr, body, opts, isNumeric);
}

/** |n| rounded to `p` significant digits, HALF-EVEN (CPython's rounding for
 * e/g/none presentations; JS toExponential rounds half-away-from-zero).
 * Returns { digits: <p digit chars>, exp10 } for finite n > 0. */
function __sigRound(n, p) {
    const m = /^(\d)(?:\.(\d+))?e([+-]\d+)$/.exec(n.toExponential(Math.min(99, Math.max(p + 2, 50))));
    let ds = m[1] + (m[2] || "");
    let exp10 = parseInt(m[3], 10);
    if (ds.length > p) {
        const cut = ds.slice(0, p);
        const next = ds[p];
        const rest = ds.slice(p + 1).replace(/0+$/, "");
        let up = false;
        if (next > "5" || (next === "5" && rest !== "")) up = true;
        else if (next === "5" && rest === "") {
            up = (cut.charCodeAt(p - 1) - 48) % 2 === 1;
        }
        const arr = cut.split("");
        if (up) {
            let i = p - 1;
            for (;;) {
                if (i < 0) {
                    arr.unshift("1");
                    arr.pop();
                    exp10 += 1;
                    break;
                }
                if (arr[i] === "9") {
                    arr[i] = "0";
                    i -= 1;
                } else {
                    arr[i] = String.fromCharCode(arr[i].charCodeAt(0) + 1);
                    break;
                }
            }
        }
        ds = arr.join("");
    } else {
        ds = ds.padEnd(p, "0");
    }
    return { digits: ds, exp10 };
}

/** Width/fill/align application — all lengths in CODE POINTS (astral-safe). */
function __specPad(signStr, s, opts, isNumeric) {
    const width = opts.width || 0;
    if (width > 0) {
        const total = [...signStr].length + [...s].length;
        if (total < width) {
            // '0' means fill='0' for strings too (align stays '<'); for
            // numerics it additionally implies '=' alignment.
            const fill = opts.fill != null ? opts.fill : (opts.zero ? "0" : " ");
            const align = opts.align || (opts.zero && isNumeric ? "=" : isNumeric ? ">" : "<");
            const need = width - total;
            if (align === "<") return signStr + s + fill.repeat(need);
            if (align === ">") return fill.repeat(need) + signStr + s;
            if (align === "^") {
                const left = Math.floor(need / 2);
                return fill.repeat(left) + signStr + s + fill.repeat(need - left);
            }
            return signStr + fill.repeat(need) + s; // '='
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
    // A user-defined __format__ receives the RAW spec string (CPython
    // format(v, spec) protocol) — '{:custom_format}'.format(b).
    if (value !== null && value !== undefined && typeof value.__format__ === "function") {
        return value.__format__(String(specStr));
    }
    return pyFormatSpec(value, parseFormatSpec(String(specStr), value));
}

// public #3: the format(value[, format_spec]) BUILTIN — the same engine as
// f-strings / str.format (pyFormatDynamic → pyFormatSpec), including the
// __format__ protocol. format(x) with a missing/empty spec is str(x) unless
// the value defines __format__ (CPython: format(v) ≡ type(v).__format__(v, '')
// and object.__format__ with an empty spec is str).
export function pyFormat(value, spec) {
    if (spec === undefined || spec === null) spec = "";
    if (typeof spec !== "string") {
        throw new TypeError_(
            `format() argument 2 must be str, not ${__pyTypeName(spec)}`,
        );
    }
    if (spec === ""
        && !(value !== null && value !== undefined
            && typeof value.__format__ === "function")) {
        return pyStr(value);
    }
    return pyFormatDynamic(value, spec);
}

/** PEP 3101 format-spec mini-language parser (the RUNTIME twin of
 * crates/pyths_parser/src/format_spec.rs::parse — both are pinned to
 * tests/fixtures/format_spec_grammar.json; keep them in lockstep).
 * Grammar: [[fill]align][sign]["z"]["#"]["0"][width][grouping]["." prec][type]
 * Invalid input RAISES the CPython error (never a silent no-op — the #108
 * class): "Unknown format code" for a single unrecognized type char,
 * "Invalid format specifier" for structural garbage, "Format specifier
 * missing precision" for a bare '.', "Cannot specify both ',' and '_'." for
 * double grouping. When `forValue` is a value whose type formats through
 * object.__format__ (not str/int/float/bool), every one of these is
 * CPython's TypeError ("unsupported format string passed to ...") instead. */
export function parseFormatSpec(s, forValue) {
    const hasValue = arguments.length >= 2;
    const objFamily = hasValue
        && !(typeof forValue === "string" || typeof forValue === "number"
            || typeof forValue === "bigint" || typeof forValue === "boolean"
            || (forValue != null && forValue.__pyfloat__ === true));
    const raiseFor = (mkValueError) => {
        if (objFamily) {
            throw new TypeError_(
                `unsupported format string passed to ${__pyTypeName(forValue)}.__format__`,
            );
        }
        throw mkValueError();
    };
    const bad = () => raiseFor(() => new ValueError(hasValue
        ? `Invalid format specifier '${s}' for object of type '${__pyTypeName(forValue)}'`
        : `Invalid format specifier '${s}'`));
    const opts = { raw: s };
    const chars = [...s];
    let i = 0;
    if (chars.length >= 2 && "<>=^".includes(chars[1])) {
        opts.fill = chars[0]; opts.align = chars[1]; i = 2;
    } else if (chars.length >= 1 && "<>=^".includes(chars[0])) {
        opts.align = chars[0]; i = 1;
    }
    if (i < chars.length && "+- ".includes(chars[i])) { opts.sign = chars[i]; i++; }
    if (i < chars.length && chars[i] === "z") { opts.z = true; i++; }
    if (i < chars.length && chars[i] === "#") { opts.alt = true; i++; }
    if (i < chars.length && chars[i] === "0") { opts.zero = true; i++; }
    // E3 r3: CPython bounds width/precision digit runs by PY_SSIZE_T_MAX
    // during the parse — beyond it is "Too many decimal digits in format
    // string" (ValueError), NOT a silently-lossy number. Mirrored by the
    // static parser in crates/pyths_parser/src/format_spec.rs, which routes
    // any >u32 digit run to this runtime parser so BOTH reject identically.
    const tooManyDigits = (ds) => {
        if (ds.length >= 19 && BigInt(ds) > 9223372036854775807n) {
            raiseFor(() => new ValueError("Too many decimal digits in format string"));
        }
    };
    let w = "";
    while (i < chars.length && chars[i] >= "0" && chars[i] <= "9") { w += chars[i]; i++; }
    if (w) { tooManyDigits(w); opts.width = parseInt(w, 10); }
    if (i < chars.length && (chars[i] === "," || chars[i] === "_")) {
        opts.grouping = chars[i];
        i++;
        if (i < chars.length && (chars[i] === "," || chars[i] === "_")) {
            if (chars[i] !== opts.grouping) {
                raiseFor(() => new ValueError("Cannot specify both ',' and '_'."));
            }
            bad();
        }
    }
    if (i < chars.length && chars[i] === ".") {
        i++;
        let p = "";
        while (i < chars.length && chars[i] >= "0" && chars[i] <= "9") { p += chars[i]; i++; }
        // 3.14: '.' with NO digits is legal when a FRACTIONAL grouping char
        // follows ('{:.,f}' -> default precision, grouped fraction); bare
        // '.'/'.q' stays the missing-precision error.
        if (!p && !(i < chars.length && (chars[i] === "," || chars[i] === "_"))) {
            raiseFor(() => new ValueError("Format specifier missing precision"));
        }
        if (p) { tooManyDigits(p); opts.precision = parseInt(p, 10); }
        // 3.14: an optional grouping char AFTER the precision groups the
        // FRACTIONAL digits ('{:,.9_f}' → '123,456,789.123_456_791').
        if (i < chars.length && (chars[i] === "," || chars[i] === "_")) {
            opts.fracGrouping = chars[i];
            i++;
            if (i < chars.length && (chars[i] === "," || chars[i] === "_")) {
                // a DIFFERENT second grouping char -> the both-error; the
                // SAME char repeated is structural garbage (Invalid).
                if (chars[i] !== opts.fracGrouping) {
                    raiseFor(() => new ValueError("Cannot specify both ',' and '_'."));
                }
                bad();
            }
        }
    }
    if (i < chars.length) {
        if (i === chars.length - 1) {
            // A single trailing char is always TAKEN as the presentation
            // type; an unrecognized one raises "Unknown format code" (the
            // per-value renderer re-checks known codes against the value).
            if (!"bcdeEfFgGnosxX%".includes(chars[i])) {
                const c = chars[i];
                raiseFor(() => new ValueError(hasValue
                    ? `Unknown format code '${c}' for object of type '${__pyTypeName(forValue)}'`
                    : `Unknown format code '${c}'`));
            }
            opts.type = chars[i];
            i++;
        } else {
            bad(); // more than one trailing character
        }
    }
    return opts;
}

/** Python `s.format(...)` — full replacement-field support: `{{`/`}}` escapes,
 * auto/positional/named fields with `.attr`/`[key]` access, `!r`/`!s`/`!a`
 * conversions, `:format_spec` (delegated to pyFormatSpec, the same engine
 * behind f-strings), and the CPython auto↔manual numbering errors. */
export function pyStrFormat(s, ...args) {
    // #301: non-string receivers with their own .format (Intl.NumberFormat,
    // Intl.DateTimeFormat, user classes) dispatch natively.
    if (typeof s !== "string" && s != null && typeof s.format === "function") {
        return s.format(...args);
    }
    // Named fields look up on a trailing kwargs object (the codegen lowers
    // `.format(name=v)` to a trailing object literal); positional/auto fields
    // index the positional args directly.
    const kwargs = args.length ? args[args.length - 1] : undefined;
    return __pyStrFormatImpl(s, args, kwargs);
}

function __pyStrFormatImpl(s, args, kwargs) {
    let auto = 0;
    let manual = false;
    const mapHas = (m, k) => (m instanceof Map ? m.has(k) : m != null && typeof m === "object" && k in m);
    const mapGet = (m, k) => (m instanceof Map ? m.get(k) : m[k]);
    const resolveField = (fieldName) => {
        let i = 0;
        let base = "";
        while (i < fieldName.length && fieldName[i] !== "." && fieldName[i] !== "[") base += fieldName[i++];
        let val;
        if (base === "") {
            if (manual) {
                throw new ValueError(
                    "cannot switch from manual field specification to automatic field numbering",
                );
            }
            if (auto >= args.length) {
                throw new IndexError(`Replacement index ${auto} out of range for positional args tuple`);
            }
            val = args[auto++];
        } else if (/^\d+$/.test(base)) {
            if (auto > 0) {
                throw new ValueError(
                    "cannot switch from automatic field numbering to manual field specification",
                );
            }
            manual = true;
            const bi = Number(base);
            if (bi >= args.length) {
                throw new IndexError(`Replacement index ${bi} out of range for positional args tuple`);
            }
            val = args[bi];
        } else if (mapHas(kwargs, base)) {
            val = mapGet(kwargs, base);
        } else {
            throw new KeyError(base);
        }
        // Trailing `.attr` / `[key]` accessors (CPython field-name grammar).
        while (i < fieldName.length) {
            if (fieldName[i] === ".") {
                i++;
                let attr = "";
                while (i < fieldName.length && fieldName[i] !== "." && fieldName[i] !== "[") attr += fieldName[i++];
                const parent = val;
                val = val == null ? undefined : val[attr];
                // complex .real/.imag are Python FLOATS - re-box so str()
                // renders 3.0, not 3 (the compiled attr-read path does the
                // same re-tagging at its own surface).
                if (typeof val === "number" && parent != null && parent.__pycomplex__ === true
                    && (attr === "real" || attr === "imag")) {
                    val = __pyF(val);
                }
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
            if (depth !== 0) throw new ValueError("Single '{' encountered in format string");
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
                conv = namePart.slice(bangIdx + 1);
                namePart = namePart.slice(0, bangIdx);
                if (conv !== "r" && conv !== "s" && conv !== "a") {
                    throw new ValueError(`Unknown conversion specifier ${conv}`);
                }
            }
            let val = resolveField(namePart);
            if (conv === "r") val = pyRepr(val);
            else if (conv === "s") val = pyStr(val);
            else if (conv === "a") val = pyAscii(val); // public #3: real ascii() (was a repr approximation)
            if (spec != null && spec !== "") {
                // Resolve a nested (dynamic) spec's own fields first.
                if (spec.includes("{")) spec = spec.replace(/\{([^{}]*)\}/g, (_, k) => pyStr(resolveField(k)));
                if (val !== null && val !== undefined && typeof val.__format__ === "function") {
                    out += val.__format__(spec); // CPython __format__ protocol
                } else {
                    out += pyFormatSpec(val, parseFormatSpec(spec, val));
                }
            } else if (conv != null) {
                // `!r`/`!s`/`!a` already produced a string; an empty spec is str().
                out += val;
            } else {
                out += pyStr(val);
            }
        } else if (ch === "}") {
            if (s[i + 1] === "}") { out += "}"; i += 2; continue; }
            throw new ValueError("Single '}' encountered in format string");
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
    throw new TypeError_(`object of type '${__pyTypeName(xs)}' has no append()`); // #467
}

/** #301: Python `xs.extend(iterable)` — receiver-dispatched. */
export function pyExtend(xs, other) {
    if (Array.isArray(xs)) {
        for (const v of pyForIter(other)) xs.push(v);
        return;
    }
    if (xs != null && typeof xs.extend === "function") return xs.extend(other);
    throw new TypeError_(`object of type '${__pyTypeName(xs)}' has no extend()`); // #467
}

/** #301: Python `xs.insert(i, v)` — receiver-dispatched. JS splice
 * clamps negative/overflow indices the same way CPython does. */
export function pyInsert(xs, i, v) {
    if (Array.isArray(xs)) { xs.splice(i, 0, v); return; }
    if (xs != null && typeof xs.insert === "function") return xs.insert(i, v);
    throw new TypeError_(`object of type '${__pyTypeName(xs)}' has no insert()`); // #467
}

/** #301: Python `s.find(sub[, start[, end]])` for strings — full CPython
 * semantics including negative/clamped start/end (the old Rename→indexOf
 * ignored `end`). Non-string receivers with their own .find (JS
 * Array.prototype.find(callback)) dispatch natively. */
export function pyFind(s, sub, start, end) {
    if (typeof s === "string") return __pyStrFind(s, sub, start, end, false, "find");
    if (s != null && typeof s.find === "function") return s.find(sub, start, end);
    throw new TypeError_(`object of type '${__pyTypeName(s)}' has no find()`); // #467
}

/** #301: Python `s.discard(v)` — Set removes-if-present; non-Set
 * receivers with their own .discard dispatch natively. */
export function pyDiscard(s, v) {
    if (s instanceof Set) { s.delete(v); return; }
    if (s != null && typeof s.discard === "function") return s.discard(v);
    throw new TypeError_(`object of type '${__pyTypeName(s)}' has no discard()`); // #467
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
        const i = __pyStrFind(obj, v, start, end, false, "index");
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
    // Item 3 (0.2.2 hold): only DICT-shaped receivers may reach the dict-pop
    // fallthrough at the bottom. None, sets, and non-subscriptable/immutable
    // primitives (int/float/bool/str/bytes) used to fall into it and raise
    // KeyError (or crash) — the wrong KIND (C4): CPython raises
    // AttributeError for receivers without a pop, and set.pop() is a real
    // method. (A bytearray never reaches this guard: PyByteArray has its own
    // pop, dispatched by the receiver branch below.)
    if (obj == null) {
        throw new AttributeError("'NoneType' object has no attribute 'pop'");
    }
    // Custom receivers with their own pop (deque, user classes) — but not
    // Map subclasses (dict-style pop below handles Counter etc.).
    if (!Array.isArray(obj) && !(obj instanceof Map)
        && typeof obj.pop === "function") {
        return obj.pop(...rest);
    }
    // set.pop() — remove and return an arbitrary element (first in insertion
    // order here); empty set → KeyError; any argument → TypeError (CPython).
    if (obj instanceof Set) {
        if (rest.length > 0) {
            throw new TypeError_(`set.pop() takes no arguments (${rest.length} given)`);
        }
        for (const v of obj) {
            obj.delete(v);
            return v;
        }
        throw new KeyError("pop from an empty set");
    }
    if (typeof obj === "number" || typeof obj === "bigint" || typeof obj === "boolean"
        || typeof obj === "string" || obj instanceof Uint8Array
        || obj.__pyfloat__ === true) {
        const tn = typeof obj === "string" ? "str"
            : typeof obj === "boolean" ? "bool"
            : obj.__pyfloat__ === true ? "float"
            : obj instanceof Uint8Array ? __pyBytesName(obj)
            : (typeof obj === "bigint" || Number.isInteger(obj)) ? "int" : "float";
        throw new AttributeError(`'${tn}' object has no attribute 'pop'`);
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
    // dict (plain object) — coerce ONCE (delta): pk for probe, read, delete.
    const k = rest[0];
    const pk = __pyPropKey(k);
    if (Object.prototype.hasOwnProperty.call(obj, pk)) {
        const v = obj[pk];
        delete obj[pk];
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
        // Option B: a boxed float folds to its numeric value — (8,) and
        // (8.0,) are the SAME dict/set key in CPython (hash(8)==hash(8.0)).
        else if (el != null && el.__pyfloat__ === true) s += "n:" + String(el.valueOf()) + ";";
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
    // Option-B spike: a boxed float folds to its numeric value, so
    // {1.0: v}[1] and {1: v}[1.0] hit like CPython (hash(1) == hash(1.0)).
    if (k != null && k.__pyfloat__ === true) return k.valueOf();
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
                // r6: symbols survive dict() conversion
                for (const k of __pyOwnKeys(src)) this.set(k, src[k]);
            }
        }
    }
    set(k, v) {
        const c = __pyKey(k);
        // CPython keeps the FIRST-inserted key object on re-assignment.
        if ((typeof k === "boolean" || Array.isArray(k) || (k != null && k.__pyfloat__ === true)) && !super.has(c)) {
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
        if ((typeof v === "boolean" || Array.isArray(v) || (v != null && v.__pyfloat__ === true)) && !super.has(c)) {
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

/** Python `set(iterable)` / `set()` builtin — canonicalizing constructor;
 * iterates dicts as KEYS via pyForIter. */
export function pySetOf(it) {
    if (it === undefined) return new PySet();
    return new PySet(pyForIter(it));
}

/** Python `frozenset(iterable)` builtin. Same canonicalizing PySet storage,
 * plus a non-enumerable `__pyfrozen__` BRAND so the in-place aug-assign
 * helpers (pyIBitOr/pyIBitAnd/pyIBitXor/pyISub) can tell it from `set` and
 * REBIND instead of mutating — CPython's frozenset `|=`/`&=`/`-=`/`^=` is
 * alias-safe (`b = a; a &= s` leaves `b` untouched, `a is b` False).
 * Full immutability (blocking .add/.discard/…) is still NOT enforced — the
 * pre-existing documented deviation; the brand only closes the silent
 * aliasing-corruption class. */
export function pyFrozensetOf(it) {
    const s = pySetOf(it);
    Object.defineProperty(s, "__pyfrozen__", { value: true, enumerable: false });
    return s;
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
        else if (typeof src[Symbol.iterator] === "function") {
            // autotester dictionaries: CPython validates every update-sequence
            // element — a non-pair raises the numbered ValueError (this is how
            // dict('asdf') fails: element #0 = 'a' has length 1), a
            // non-sequence element raises TypeError. Strings take this branch
            // too (dict(['ab', 'cd']) == {'a':'b','c':'d'} is legal CPython).
            let i = 0;
            for (const pair of src) {
                let seq = pair;
                if (typeof pair !== "string" && !Array.isArray(pair)) {
                    if (
                        pair !== null &&
                        typeof pair === "object" &&
                        typeof pair[Symbol.iterator] === "function"
                    ) {
                        seq = [...pair];
                    } else {
                        throw new TypeError_(
                            `cannot convert dictionary update sequence element #${i} to a sequence`,
                        );
                    }
                }
                if (seq.length !== 2) {
                    throw new ValueError(
                        `dictionary update sequence element #${i} has length ${seq.length}; 2 is required`,
                    );
                }
                entries.push([seq[0], seq[1]]);
                i += 1;
            }
        } else if (typeof src === "object") {
            for (const k of __pyOwnKeys(src)) entries.push([k, src[k]]); // r6: symbols survive
        } else {
            throw new TypeError_(`'${__pyTypeName(src)}' object is not iterable`); // #467
        }
    }
    if (kwargs != null) {
        // r7: dict(**m) keyword names must be strings (CPython TypeError) —
        // a Symbol key now raises instead of being silently dropped.
        for (const k of (kwargs instanceof Map ? kwargs.keys() : __pyOwnKeys(kwargs))) {
            if (typeof k !== "string") throw new TypeError_("keywords must be strings");
        }
        for (const k of Object.keys(kwargs)) entries.push([k, kwargs[k]]);
    }
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
 * SEC-7 (CWE-1321) — the single proto-safe plain-dict write primitive.
 *
 * `o[k] = v` with `k === "__proto__"` does NOT store a key: it invokes the
 * inherited `Object.prototype.__proto__` SETTER, which silently reparents
 * `o`. That is both
 *   (a) a prototype-pollution primitive — `o` then inherits every property of
 *       an attacker-supplied object, so a later `if (opts.isAdmin)` in any JS
 *       consumer reads attacker data; and
 *   (b) a Python-semantics break — in CPython `"__proto__"` is an ordinary
 *       string key, so `d["__proto__"] = v; "__proto__" in d` is `True`.
 *
 * `Object.defineProperty` creates a real own data property in every case
 * (plain, null-prototype, or exotic receiver), so this is a total function —
 * no `__isPlainObj` gate is needed and none is wanted: the gate is exactly
 * what a hostile receiver would try to slip past.
 *
 * EVERY plain-object dict/kwargs write in this file must route through here.
 * See `experiments/codex-security-scan/poc/D-7.md` for the reproducer.
 */
/**
 * Coerce a dict key to its effective property key EXACTLY ONCE. Every
 * plain-object dict op (write/get/setdefault/pop/contains) must compute this
 * one and reuse it, so a `Symbol.toPrimitive` key that returns different values
 * on successive coercions cannot make the presence-check and the access
 * disagree. Symbols pass through (they are valid keys and `String(sym)` throws).
 */
export function __pyPropKey(k) {
    if (typeof k === "symbol") return k;
    if ((typeof k === "object" && k !== null) || typeof k === "function") {
        // delta4: full ToPropertyKey. `String(k)` runs ToPrimitive(k, string)
        // and THROWS when that yields a Symbol — but a Symbol result is a
        // valid property key that native `o[k]` would use. Evaluating the key
        // in a computed-property position applies the exact spec coercion
        // EXACTLY ONCE (Symbol.toPrimitive → valueOf → toString), symbols
        // passing through instead of throwing.
        return Reflect.ownKeys({ [k]: 0 })[0];
    }
    return String(k);
}

/**
 * Own ENUMERABLE keys of a dict-shaped object — strings AND symbols.
 * `Object.keys` silently DROPS Symbol keys (so a Symbol-keyed entry vanished
 * through dict merge/update); bare `Reflect.ownKeys` would ADD non-enumerable
 * ones. This matches Object.assign's source-key selection, which is Python's
 * "every key of the dict". delta4.
 */
export function __pyOwnKeys(o) {
    const out = Object.keys(o);
    for (const s of Object.getOwnPropertySymbols(o)) {
        const d = Object.getOwnPropertyDescriptor(o, s);
        if (d && d.enumerable) out.push(s);
    }
    return out;
}

export function __pyDictWrite(o, k, v) {
    // R1: `o[k] = v` applies ToPropertyKey(k) FIRST, so a key whose *string*
    // form is "__proto__" — a boxed `new String("__proto__")`, a Map key, an
    // object with that toString — invokes the inherited setter too. Compare the
    // coerced key, not the raw one. (Symbols can never be "__proto__".)
    const pk = __pyPropKey(k);
    if (pk === "__proto__") {
        Object.defineProperty(o, "__proto__", {
            value: v, writable: true, enumerable: true, configurable: true,
        });
        return;
    }
    // R1: write the ALREADY-COERCED key. `o[k] = v` would run ToPropertyKey(k)
    // a SECOND time, so a `Symbol.toPrimitive` that returns "__proto__" only on
    // its second call would still hit the setter. `o[pk]` cannot re-coerce.
    o[pk] = v;
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
        // 2c: derive the container name from the ONE source (__pySeqName —
        // always "list" here, since tuples were rejected above) and carry the
        // ", not <type>" suffix CPython prints, instead of the truncated
        // hardcoded "list indices must be integers or slices".
        throw new TypeError_(
            `${__pySeqName(obj)} indices must be integers or slices, not ${__pyIndexTypeName(key)}`);
    }
    // #297: hand PyDict the ORIGINAL key — its own __pyKey canonicalizes
    // (bool→0/1 included) AND records the first-inserted form for repr.
    // The old top-level bool pre-coercion (#258) destroyed that record
    // (`d[True] = x` printed `{1: x}`, CPython keeps `{True: x}`).
    if (obj instanceof Map) { obj.set(key, value); return; }
    // BYTES AUTHORITY: immutable bytes reject item assignment (CPython
    // TypeError — the old fallthrough to __pyDictWrite silently WROTE into
    // the Uint8Array). A bytearray carries the __setitem__ protocol method
    // (PyByteArray, operators.js) and dispatches below with byte validation.
    if (__pyBytesKind(obj) === "bytes") {
        throw new TypeError_("'bytes' object does not support item assignment");
    }
    if (typeof key === "boolean") key = key ? 1 : 0; // #258: bool ⊂ int (plain shapes)
    if (typeof obj.__setitem__ === "function") { obj.__setitem__(key, value); return; }
    // F3 sibling / SEC-7: a computed `__proto__` write must create a real own
    // key, not mutate the prototype. Centralized in __pyDictWrite.
    __pyDictWrite(obj, key, value);
}

/** Python `d.keys()` (and bare dict iteration) — shape-dispatched, returns
 * an array. Map/PyDict/FormData/URLSearchParams use their real .keys();
 * plain objects use Object.keys. */
export function pyDictKeys(d) {
    if (d == null) throw new TypeError_("'NoneType' object is not iterable");
    if (!Array.isArray(d) && typeof d.keys === "function") return [...d.keys()];
    return __pyOwnKeys(d); // r6: symbol keys listed too
}

// #266: a Python method accessed as a VALUE (`g = d.get`, `key=d.get`) is a
// BOUND method — it carries its receiver. In JS a detached `d.get` loses `this`.
// Bind a native method to its receiver; synthesize a closure for a plain-object
// dict's methods (which are lowered, not real properties). Gated so a class
// instance's data field named like a dict method is returned as-is.
// S1 (bound-method identity): CPython bound methods compare EQUAL when they
// wrap the same function and the same receiver (`a.f == a.f` is True) while
// staying distinct objects (`a.f is a.f` is False — a fresh wrapper per
// access). Stamp the wrapper with its (func, self) pair so pyEq can
// recognize equality; the stamps are non-enumerable so dir()/keys stay clean.
function __pyBind(func, self) {
    const bound = func.bind(self);
    Object.defineProperty(bound, "__pyboundfunc__", { value: func });
    Object.defineProperty(bound, "__pyboundself__", { value: self });
    return bound;
}

// `strict` (optional, codegen-driven): 1 when the compiler proved the
// receiver is a Python DICT (a plain-object dict literal/local). At runtime
// a plain object is indistinguishable from a JS-interop object (React
// props, DOM options), so the absent-attribute AttributeError below only
// fires for plain objects when the codegen vouches for the dict typing;
// brand-carrying containers (Array/Map/Set) don't need the flag.
export function pyBoundMethod(obj, name, strict) {
    // autotester: None.attr is AttributeError in CPython, not a silent null
    // (this helper is now the general value-position attribute-read path).
    if (obj == null) {
        throw new AttributeError(`'NoneType' object has no attribute '${name}'`);
    }
    // WB-16: `.constructor` is a CLASS reference, not a method. The
    // function-binding branch below would `.bind(obj)` it and return a wrapper
    // whose `.name` is "bound <Class>" (and which cannot be `new`-ed) —
    // breaking the standard `obj.constructor.name` reflection idiom
    // (`root.classList.add(this.constructor.name)`, fromJSON dispatch). A class
    // constructor is never a bound method, so return it RAW.
    if (name === "constructor") return obj.constructor;
    // Python numeric attribute protocol on primitives: int/float/bool have
    // .real/.imag/.conjugate() (autotester complex_numbers).
    if (typeof obj === "number" || typeof obj === "bigint" || typeof obj === "boolean"
        || obj.__pyfloat__ === true) {
        // Option B: float attributes keep floatness — (8.0).real is 8.0 and
        // float .imag is 0.0 (CPython), while int .imag stays int 0.
        const isF = obj.__pyfloat__ === true
            || (typeof obj === "number" && !Number.isInteger(obj));
        switch (name) {
            case "real": return typeof obj === "boolean" ? (obj ? 1 : 0) : obj;
            case "imag": return isF ? __pyF(0) : 0;
            case "conjugate": return () => (typeof obj === "boolean" ? (obj ? 1 : 0) : obj);
        }
        // Error-kind batch, review round 2: INT and BOOL also carry the
        // Rational protocol — `.numerator` is the value itself (bool ⊂ int:
        // True→1, False→0) and `.denominator` is 1. FLOAT does NOT
        // (CPython: `(2.5).numerator` raises AttributeError), so these arms
        // are gated on !isF and a float receiver falls through to the
        // missing-attribute guard below. Without this, that guard
        // over-raised on CPython-valid `(5).numerator`.
        if (!isF) {
            switch (name) {
                case "numerator": return typeof obj === "boolean" ? (obj ? 1 : 0) : obj;
                case "denominator": return 1;
            }
        }
    }
    // Option B: complex components are FLOATS in CPython — (8+0j).real is
    // 8.0, .imag is 0.0. PyComplex stores raw natives internally (its
    // arithmetic/repr depend on that), so the float tag is applied here at
    // the Python attribute-read surface. Brand-based: covers both runtime
    // copies' PyComplex (each carries __pycomplex__).
    if (obj.__pycomplex__ === true && (name === "real" || name === "imag")) {
        return __pyF(obj[name]);
    }
    // B1: read the attribute ONCE — a @property getter must fire exactly
    // once per access (the old typeof-check + return pair ran it twice).
    const v = obj[name];
    if (typeof v === "function") {
        // S3: a function stored as INSTANCE DATA (an own property, e.g.
        // `self.cb = freefn`) is not a method — CPython does not bind
        // instance-dict functions, so `obj.cb is freefn` stays True.
        // Real methods live on the prototype (not own) and are bound.
        if (Object.hasOwn(obj, name)) return v;
        return __pyBind(v, obj);
    }
    // autotester docstrings: `C.g` — a method reached through the CLASS
    // object is the (unbound) prototype function in Python 3.
    if (v === undefined && typeof obj === "function" && obj.prototype
        && typeof obj.prototype[name] === "function" && name !== "constructor") {
        return obj.prototype[name];
    }
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
            case "fromkeys": return (it, v2) => __pyDictFromkeys(it, v2);
            // Error-kind round 3: the rest of CPython's dict surface, so a
            // plain-object dict's method REFERENCE resolves (a Map receiver
            // reaches here only for the non-JS-member names — its JS clear()
            // binds in the function branch above with equal semantics).
            case "update": return (...o) => pyUpdate(obj, ...o);
            case "setdefault": return (k, d) => pyDictSetdefault(obj, k, d);
            case "popitem": return (last) => pyDictPopitem(obj, last);
            case "clear": return () => pyClear(obj);
            case "copy": return () => pyCopy(obj);
        }
    }
    // Error-kind round 3 (corpus deep-close): NATIVE CONTAINER method
    // references resolve through the same runtime helpers the call-position
    // lowering uses, so `m = xs.append; m(3)` works and — critically — the
    // absent-attribute guard below cannot false-positive on a real Python
    // method. Python-only names (no JS member) are listed; JS-backed members
    // (list .pop/.sort/.reverse, set .add/.clear) already bound above.
    if (Array.isArray(obj)) {
        switch (name) {
            case "index": return (x, s, e) => pyIndex(obj, x, s, e);
            case "count": return (x) => pyCount(obj, x);
        }
        // tuple has ONLY count/index — `(1,).append` must fall through to
        // the AttributeError guard, like CPython.
        if (obj.__pytuple__ !== true) {
            switch (name) {
                case "append": return (x) => pyAppend(obj, x);
                case "extend": return (it) => pyExtend(obj, it);
                case "insert": return (i, x) => pyInsert(obj, i, x);
                case "remove": return (x) => pyRemove(obj, x);
                case "clear": return () => pyClear(obj);
                case "copy": return () => pyCopy(obj);
            }
        }
    }
    if (obj instanceof Set) {
        switch (name) {
            case "discard": return (x) => pyDiscard(obj, x);
            case "remove": return (x) => pyRemove(obj, x);
            case "pop": return (...a) => pyPop(obj, ...a);
            case "copy": return () => pyCopy(obj);
            case "update": return (...o) => pyUpdate(obj, ...o);
            case "union": return (...o) => pySetUnion(obj, ...o);
            case "intersection": return (...o) => pySetIntersection(obj, ...o);
            case "difference": return (...o) => pySetDifference(obj, ...o);
            case "symmetric_difference": return (o) => pySetSymmetricDifference(obj, o);
            case "intersection_update": return (...o) => pySetIntersectionUpdate(obj, ...o);
            case "difference_update": return (...o) => pySetDifferenceUpdate(obj, ...o);
            case "symmetric_difference_update": return (o) => pySetSymmetricDifferenceUpdate(obj, o);
            case "isdisjoint": return (o) => pySetIsdisjoint(obj, o);
            case "issubset": return (o) => pySetIssubset(obj, o);
            case "issuperset": return (o) => pySetIssuperset(obj, o);
        }
    }
    // Error-kind class (#471/#472/#473 batch + round-3 corpus): a receiver
    // with a genuinely-absent attribute must raise CPython's AttributeError —
    // previously the read silently yielded `undefined` (printed as None), a
    // silent wrong value on the error path. Type name via __pyTypeName
    // (#469 — the ONE source). Scope:
    //   * primitives — int/bool/str + the float box (None arm is at the top);
    //   * native containers by BRAND — Array (list/tuple), Map (dict),
    //     Set/PySet (set); their real methods all resolved above, and
    //     JS-backed/expando members pass the `in` check;
    //   * plain-object dicts ONLY under the codegen `strict` flag (receiver
    //     statically proven Dict) — an unproven plain object keeps the
    //     undefined pass-through because it may be a JS-interop object
    //     (React props), where absent-optional reads are legitimate.
    // User class instances and other objects keep the pass-through (the
    // hasattr idiom and interop reads must keep working).
    if (v === undefined
        && (typeof obj === "number" || typeof obj === "bigint" || typeof obj === "boolean"
            || typeof obj === "string" || obj.__pyfloat__ === true
            || Array.isArray(obj) || obj instanceof Map || obj instanceof Set
            || (strict === 1 && isDict))
        && !(name in Object(obj))) {
        throw new AttributeError(`'${__pyTypeName(obj)}' object has no attribute '${name}'`);
    }
    return v;
}

// autotester callable_test: calling through a plain VARIABLE — the value
// may be a real function (fast path) or an instance of a class defining
// __call__ (CPython callable objects). The codegen routes Name-callee
// calls here only when the name is a local variable (not a known def/
// class/builtin), so direct function calls stay raw.
export function __pyCall(f, args) {
    if (typeof f === "function") {
        // A CLASS reached through a variable (decorated class, class passed
        // as an argument) constructs with `new`. Compiled classes carry
        // __mro__; the source sniff covers native/dataclass classes.
        if (f.__mro__ !== undefined
            || (f.prototype !== undefined
                && /^class[\s{(]/.test(Function.prototype.toString.call(f)))) {
            return new f(...args);
        }
        return f(...args);
    }
    if (f !== null && f !== undefined && typeof f.__call__ === "function") {
        return f.__call__(...args);
    }
    throw new TypeError_(`'${__pyTypeName(f)}' object is not callable`);
}

// autotester local_classes: an attribute CALL whose attribute is a class —
// `a.B(9)` / `b.C(10)` reaching a NESTED class through an instance — must
// construct with `new` (a plain call throws "Class constructor cannot be
// invoked without 'new'"). A method attribute keeps its receiver. The
// codegen routes an attribute call here only when the attribute name is a
// known class name, so the common method-call path stays raw. One property
// read (getters fire once).
export function __pyAttrCall(obj, name, args) {
    if (obj == null) {
        throw new AttributeError(`'NoneType' object has no attribute '${name}'`);
    }
    const v = obj[name];
    if (typeof v === "function") {
        if (/^class[\s{(]/.test(Function.prototype.toString.call(v))) {
            return new v(...args);
        }
        return v.apply(obj, args);
    }
    if (v === undefined && !(name in Object(obj))) {
        throw new AttributeError(
            `'${__pyTypeName(obj)}' object has no attribute '${name}'`,
        );
    }
    return v(...args);
}

// ── autotester attribs_by_name: getattr/setattr/hasattr/delattr ────────────
// #467: THE ONE value-model type-name source for runtime error messages.
// CPython names the value's PYTHON type in diagnostics ("'float' object is
// not iterable"), while JS `typeof` names the JS representation ('number',
// 'object', …). Every "'X' object is not/has no/does not support …" message
// routes through here so the name is right by construction: pyType covers
// primitives (boxing-aware: an unboxed non-integer Number IS a float),
// bigint→int, bytes/bytearray, containers, and compiled classes; fall back
// to the JS typeof only for values pyType cannot name.
export function __pyTypeName(o) {
    if (o === null || o === undefined) return "NoneType";
    const t = pyType(o);
    return (t && (t.__name__ || t.name)) || typeof o;
}
const __ATTR_MISSING = Symbol("attr-missing");
function __attrLookup(obj, name) {
    if (obj === null || obj === undefined) return __ATTR_MISSING;
    // WB-16 (residual): `.constructor` is a CLASS reference, not a method —
    // return it RAW so `getattr(o, "constructor").name` is the bare class name.
    // pyBoundMethod already special-cases this; the getattr/hasattr path
    // (this helper) did NOT, so the bind branch below wrapped it into a
    // function whose `.name` is "bound <Class>" (and which cannot be `new`-ed),
    // breaking the standard reflection idiom via getattr.
    if (name === "constructor") return obj.constructor;
    const v = obj[name];
    if (v === undefined && !(name in Object(obj))) return __ATTR_MISSING;
    // Bound-method semantics: a function attribute read by name carries its
    // receiver, matching direct `obj.m` access (pyBoundMethod discipline —
    // same own-property split: instance-data functions stay unbound, and the
    // wrapper carries the (func, self) identity stamps for pyEq).
    if (typeof v === "function" && !Object.hasOwn(obj, name)) return __pyBind(v, obj);
    return v;
}

/** Python `getattr(obj, name[, default])`. */
export function pyGetattr(obj, name, ...dflt) {
    const v = __attrLookup(obj, name);
    if (v !== __ATTR_MISSING) return v;
    if (dflt.length > 0) return dflt[0];
    throw new AttributeError(
        `'${__pyTypeName(obj)}' object has no attribute '${name}'`,
    );
}

/** Python `setattr(obj, name, value)` — returns None. */
export function pySetattr(obj, name, value) {
    if (obj === null || obj === undefined) {
        throw new AttributeError(
            `'NoneType' object has no attribute '${name}'`,
        );
    }
    obj[name] = value;
    return null;
}

/** Python `hasattr(obj, name)`. */
export function pyHasattr(obj, name) {
    return __attrLookup(obj, name) !== __ATTR_MISSING;
}

/** Python `delattr(obj, name)` — AttributeError when absent, like CPython. */
export function pyDelattr(obj, name) {
    if (obj === null || obj === undefined || !(name in Object(obj))) {
        throw new AttributeError(
            `'${__pyTypeName(obj)}' object has no attribute '${name}'`,
        );
    }
    delete obj[name];
    return null;
}

// autotester general_functions: `dir()`. Approximates CPython's contract on
// the compiled object model: for a CLASS, the own attributes of the
// constructor chain + prototype methods; for an INSTANCE, its own attributes
// plus everything its class chain contributes; sorted, deduplicated. JS
// structural noise (length/name/prototype/...) is excluded; dunder-ish
// compiler metadata (__mro__, __pyparams__, ...) survives like CPython's own
// dunders and is filtered the same way user code filters them.
const __DIR_NOISE = new Set([
    "length", "name", "prototype", "constructor", "arguments", "caller", "toString",
]);
export function pyDir(x) {
    if (x === null || x === undefined) return [];
    const names = new Set();
    const add = (o) => {
        for (const k of Object.getOwnPropertyNames(o)) {
            if (!__DIR_NOISE.has(k)) names.add(k);
        }
    };
    if (typeof x === "function") {
        let c = x;
        while (c && c !== Function.prototype) {
            add(c);
            if (c.prototype) add(c.prototype);
            c = Object.getPrototypeOf(c);
        }
    } else if (typeof x === "object") {
        add(x);
        let p = Object.getPrototypeOf(x);
        while (p && p !== Object.prototype) {
            add(p);
            if (p.constructor && p.constructor !== Object) add(p.constructor);
            p = Object.getPrototypeOf(p);
        }
    } else {
        const p = Object.getPrototypeOf(Object(x));
        if (p) add(p);
    }
    return [...names].sort();
}

// public #3: ascii(x) — repr() with every non-ASCII character escaped
// (\xNN for U+0080..U+00FF, \uNNNN to U+FFFF, \UNNNNNNNN above), CPython
// semantics. Iterating by code point keeps surrogate pairs one escape.
export function pyAscii(x) {
    let out = "";
    for (const ch of pyRepr(x)) {
        const cp = ch.codePointAt(0);
        if (cp < 0x80) { out += ch; continue; }
        const hex = cp.toString(16);
        if (cp <= 0xff) out += "\\x" + hex.padStart(2, "0");
        else if (cp <= 0xffff) out += "\\u" + hex.padStart(4, "0");
        else out += "\\U" + hex.padStart(8, "0");
    }
    return out;
}

// public #3: vars(obj) — the instance __dict__ as a dict (own enumerable
// data attributes). On the compiled object model instance attributes are
// own JS properties, while methods and class attributes live on the
// prototype chain — so "own props minus compiler markers" is exactly
// CPython's instance __dict__. Non-instances (primitives, dicts, lists,
// sets, None) raise TypeError like CPython. The zero-arg form
// (vars() ≡ locals()) is rejected at compile time.
export function pyVars(obj) {
    const isInstance = obj !== null && obj !== undefined
        && typeof obj === "object"
        && !Array.isArray(obj)
        && !(obj instanceof Map)
        && !(obj instanceof Set)
        && Object.getPrototypeOf(obj) !== Object.prototype;
    if (!isInstance) {
        throw new TypeError_("vars() argument must have __dict__ attribute");
    }
    const out = {};
    for (const k of Object.keys(obj)) {
        if (k.startsWith("__py")) continue; // compiler markers, not user attrs
        out[k] = obj[k];
    }
    return out;
}

/** Python `callable(x)` — functions/classes, or instances with __call__. */
export function pyCallable(x) {
    return (
        typeof x === "function" ||
        (x !== null && x !== undefined && typeof x.__call__ === "function")
    );
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
    // #467 / Option B parity: a boxed float is an object but NOT iterable —
    // the same guard __pyElemsIter/pySeq/pyIter carry; without it
    // `for x in 8.0` silently iterated the box's (zero) keys.
    if (x.__pyfloat__ === true) throw new TypeError_("'float' object is not iterable");
    if (typeof x === "object") return __pyOwnKeys(x); // plain-object dict → keys (r6: symbols too)
    // #467: a non-iterable primitive used to pass through to for..of's
    // JS-shaped native TypeError ("... is not iterable" naming the VALUE);
    // raise CPython's message with the Python type name instead.
    throw new TypeError_(`'${__pyTypeName(x)}' object is not iterable`);
}

/** Python `d.values()` — shape-dispatched, returns an array. */
export function pyDictValues(d) {
    if (d == null) throw new TypeError_("'NoneType' object is not iterable");
    if (!Array.isArray(d) && typeof d.values === "function") return [...d.values()];
    return __pyOwnKeys(d).map((k) => d[k]); // r6: symbol-keyed values included
}

/** Python `d.items()` — shape-dispatched; returns an array of (k, v)
 * tuples (pyTuple-marked so repr shows `(k, v)` like CPython). */
export function pyDictItems(d) {
    if (d == null) throw new TypeError_("'NoneType' object is not iterable");
    // WB-6: a USER receiver with its own `items` method (user classes,
    // mapping-likes) dispatches to it — the old `.entries`-keyed dispatch
    // silently snapshotted the ATTRIBUTE dict instead. Containers never
    // define `items` (Array/Map/PyDict/plain objects have no such method;
    // FormData/URLSearchParams are `.entries`-native), so the container
    // paths below are unreachable from here.
    if (!__isPlainObj(d) && typeof d.items === "function") return d.items();
    if (!Array.isArray(d) && typeof d.entries === "function") {
        return [...d.entries()].map((pair) => __markTuple([pair[0], pair[1]]));
    }
    return __pyOwnKeys(d).map((k) => __markTuple([k, d[k]])); // r6: symbol-keyed items included
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
    // autotester operator_overloading: a CPython callable OBJECT (instance
    // of a class defining __call__) invoked with keywords — dispatch to the
    // prototype's __call__ (which carries the __pyparams__/__pykw__
    // metadata) with the instance as `this`.
    if (typeof fn !== "function" && fn !== null && fn !== undefined
        && typeof fn.__call__ === "function") {
        const m = fn.__call__;
        return m.call(fn, ...__pyKwArgs(m, pos, kw));
    }
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
    // r7: CPython — a `**` mapping's keyword names MUST be strings
    // (f(**{1: 2}) raises TypeError). Previously a plain-object spread
    // silently DROPPED Symbol keys (Object.entries is string-only) and a
    // Map-backed spread let non-string keys flow into parameter binding
    // with a wrong error. Validate BEFORE binding, like CPython.
    for (const k of (kw instanceof Map ? kw.keys() : __pyOwnKeys(kw))) {
        if (typeof k !== "string") throw new TypeError_("keywords must be strings");
    }
    const entries = kw instanceof Map ? Array.from(kw.entries()) : Object.entries(kw);
    // autotester arguments: for `new Cls(...)` the keyword metadata lives on
    // the cooperative __init__ prototype method, not the class object.
    let meta = fn;
    if (fn && !fn.__pyparams__ && fn.prototype) {
        const mro = fn.__mro__ || [fn];
        for (const c of mro) {
            if (c && c.prototype
                && Object.prototype.hasOwnProperty.call(c.prototype, "__init__")
                && c.prototype.__init__.__pyparams__) {
                meta = c.prototype.__init__;
                break;
            }
        }
    }
    const names = meta ? meta.__pyparams__ : undefined;
    if (!names) {
        const legacy = {};
        // SEC-7: `f(**remote)` must not let a "__proto__" key reparent the
        // options object handed to a JS/React consumer.
        for (const [k, v] of entries) __pyDictWrite(legacy, k, v);
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
        } else if (meta.__pykw__) {
            __pyDictWrite(rest = rest || {}, k, v); // SEC-7
        } else {
            throw new TypeError(`${fname}() got an unexpected keyword argument '${k}'`);
        }
    }
    if (meta.__pykw__) {
        // autotester arguments/decorators: a VARIADIC signature (`*args` —
        // fn.__pyva__) has no fixed keyword slot; the keyword channel
        // travels as a Symbol-marked trailing carrier that the callee's
        // prologue pops (__pyTakeKw). Fixed-arity keeps the names.length slot.
        // The carrier must land BEYOND every named slot (not merely at the
        // end of `args`): with fewer positionals than named params, a bare
        // push would let a named param swallow the carrier (g(1, n=5) put it
        // in `y`). Sparse holes spread as undefined → JS defaults apply.
        if (meta.__pyva__) {
            args[Math.max(args.length, names.length)] = __pyMarkKw(rest || {});
        } else {
            args[names.length] = rest || {};
        }
    }
    return args; // sparse holes spread as undefined -> JS defaults apply
}

// ── autotester arguments/decorators: varargs keyword channel ──────────────
// JS cannot declare parameters after a rest param, so `def f(*args, m, n=1,
// **kwargs)` compiles to `function f(...args)` plus a prologue:
//   const kwargs = __pyTakeKw(args);              // pops the marked carrier
//   let m = __pyKwPop(kwargs, "m", "f");          // kw-only, required
//   let n = __pyKwPop(kwargs, "n", "f", 1);       // kw-only, defaulted
//   __pyNoExtraKw(kwargs, "f");                   // only when no **kwargs
// Call sites with keywords append __pyMarkKw(rest) via __pyKwArgs above;
// plain positional calls carry no marker, so the prologue sees {}.
const __PYKW_MARK = Symbol("pyths.kwargs");
export function __pyMarkKw(obj) {
    Object.defineProperty(obj, __PYKW_MARK, { value: true, enumerable: false });
    return obj;
}
export function __pyTakeKw(args) {
    const last = args.length > 0 ? args[args.length - 1] : undefined;
    if (last !== null && typeof last === "object" && last[__PYKW_MARK] === true) {
        args.pop();
        return last;
    }
    return {};
}
// S2: a `*args` rest parameter is a TUPLE in Python — `type(args)` is
// tuple, `isinstance(args, tuple)` is True, repr is `(1, 2)`. The JS rest
// array is freshly allocated per call, so marking it in place is safe.
export function __pyMarkTuple(a) {
    if (Array.isArray(a) && !a.__pytuple__) {
        Object.defineProperty(a, "__pytuple__", { value: true, enumerable: false });
    }
    return a;
}

export function __pyKwPop(kw, name, fname, ...dflt) {
    if (Object.prototype.hasOwnProperty.call(kw, name)) {
        const v = kw[name];
        delete kw[name];
        return v;
    }
    if (dflt.length > 0) return dflt[0];
    throw new TypeError_(
        `${fname}() missing 1 required keyword-only argument: '${name}'`,
    );
}
export function __pyNoExtraKw(kw, fname, allowed) {
    // Called BEFORE the kw-only pops (CPython reports an unexpected keyword
    // ahead of a missing keyword-only one), so `allowed` lists the kw-only
    // names that are legitimately still present at this point.
    for (const k of Object.keys(kw)) {
        if (allowed === undefined || !allowed.includes(k)) {
            throw new TypeError_(`${fname}() got an unexpected keyword argument '${k}'`);
        }
    }
}

// autotester method_and_class_decorators: apply a USER decorator to an
// instance method. Compiled methods carry `self` as JS `this`, but a Python
// decorator expects a plain function whose FIRST parameter is self
// (`def deco(f): def inner(*args): ... f(*args)`). Bridge both shapes:
//   as_fn : python-shaped view of the original (self as first arg)
//   dec(as_fn) : whatever the user decorator builds
//   returned  : method-shaped — forwards `this` as the first argument
// so `a.m(3)` reaches inner as args=(a, 3), exactly like CPython.
export function __pyDecorateMethod(dec, orig) {
    const as_fn = function (self, ...a) {
        return orig.apply(self, a);
    };
    // Carry keyword-binding metadata (self prepended) so keyword calls
    // through the decorator still bind by name.
    if (orig.__pyparams__) {
        as_fn.__pyparams__ = ["self", ...orig.__pyparams__];
        if (orig.__pykw__) as_fn.__pykw__ = true;
        if (orig.__pyva__) as_fn.__pyva__ = true;
    }
    // The decorator itself may be a callable INSTANCE (`@adeco(t=1)` — a
    // class whose __call__ returns the wrapper).
    const w = typeof dec === "function" ? dec(as_fn) : dec.__call__(as_fn);
    return function (...a) {
        return w(this, ...a);
    };
}

// autotester method_and_class_decorators: decorate a @classmethod — the
// decorator's wrapper signature is (cls, ...); thread the class the way
// __pyDecorateMethod threads self. `this` is the class for Cls.m(...)
// calls; instance-alias calls fall back to the defining class.
export function __pyDecorateClassMethod(dec, orig, cls) {
    const as_fn = function (c, ...a) { return orig.apply(c, a); };
    if (orig.__pyparams__) {
        as_fn.__pyparams__ = ["cls", ...orig.__pyparams__];
        if (orig.__pykw__) as_fn.__pykw__ = true;
        if (orig.__pyva__) as_fn.__pyva__ = true;
    }
    const w = typeof dec === "function" ? dec(as_fn) : dec.__call__(as_fn);
    return function (...a) {
        const c = typeof this === "function" ? this : cls;
        return w(c, ...a);
    };
}

// Metadata attacher for function EXPRESSIONS (lambdas) — statements attach
// __pyparams__/__pykw__/__pyva__ as post-declaration assignments, but a
// lambda has no name to assign onto; wrap instead.
export function __pyFnMeta(fn, names, pykw, pyva) {
    fn.__pyparams__ = names;
    if (pykw) fn.__pykw__ = true;
    if (pyva) fn.__pyva__ = true;
    return fn;
}

export function pyDictMerge(...parts) {
    if (parts.some((p) => p instanceof Map)) {
        const out = new PyDict();
        for (const p of parts) {
            if (p == null) continue;
            if (p instanceof Map) { for (const [k, v] of p.entries()) out.set(k, v); }
            // delta4: __pyOwnKeys, not Object.keys — Symbol-keyed entries survive.
            else { for (const k of __pyOwnKeys(p)) out.set(k, p[k]); }
        }
        return out;
    }
    const out = {};
    for (const p of parts) {
        if (p == null) continue;
        // SEC-7: centralized in __pyDictWrite (same semantics as before).
        // delta4: __pyOwnKeys, not Object.keys — Symbol-keyed entries survive.
        for (const k of __pyOwnKeys(p)) __pyDictWrite(out, k, p[k]);
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
    // Coerce ONCE (delta): use pk in both the probe and the read.
    if (d == null) return defaultValue;
    const pk = __pyPropKey(k);
    const present = Object.prototype.hasOwnProperty.call(d, pk);
    // WB-20: perform the PLAIN property read UNCONDITIONALLY — even when the
    // key is absent — so a host read-trap fires and registers a dependency on
    // THIS key, exactly as native `d[k]` and the subscript helper `pyGetItem`
    // (line ~1354) do. When `d` is a MobX-observable object (a Proxy whose
    // prototype is Object.prototype, so `__isPlainObj` sends it down this dict
    // path), MobX tracks reads through the Proxy's `get` trap. The old code
    // guarded the read behind `hasOwnProperty` — which goes through the
    // `[[GetOwnProperty]]`/`has` machinery, not `get`, and short-circuits with
    // NO `get` at all on a missing key — so an `observer` component reading
    // MobX state via `dict.get(k)` never subscribed, and never re-rendered when
    // the key was later added. Doing `d[pk]` here (then gating on `present`)
    // registers the dependency for the absent-key case too, matching the golden
    // native-bracket read. Gating the RESULT on own-property presence preserves
    // Python dict semantics: missing/inherited keys → default; an own key → its
    // value, including a genuine `undefined`. Reading a plain data dict's
    // absent/inherited slot is side-effect-free (no getters), so the extra read
    // is inert off the observable path.
    const v = d[pk];
    return present ? v : defaultValue;
}

/** Python `d.setdefault(k, default)` — sets if missing, returns current. */
export function pyDictSetdefault(d, k, defaultValue) {
    // WB-6: user receivers with their own setdefault (user classes,
    // Map/dict subclasses overriding it) dispatch to it — checked FIRST so
    // an override wins, like pyDictPopitem/pyUpdate. Plain Maps/objects
    // have no `setdefault` method and keep the container paths below.
    if (d != null && !__isPlainObj(d) && typeof d.setdefault === "function") {
        return d.setdefault(k, defaultValue);
    }
    if (d instanceof Map) {
        if (d.has(k)) return d.get(k);
        d.set(k, defaultValue);
        return defaultValue;
    }
    // Coerce ONCE (delta): pk drives the probe, the read, AND the write.
    const pk = __pyPropKey(k);
    if (Object.prototype.hasOwnProperty.call(d, pk)) return d[pk];
    __pyDictWrite(d, pk, defaultValue); // SEC-7: `d["__proto__"]` is a data key
    return defaultValue;
}

// ============================================================
// Multi-receiver helpers — dispatch on receiver type at runtime.
// Used for method names that exist on multiple Python types
// (count, clear, copy, remove) where the codegen has no static
// type info to pick a per-type lowering.
// ============================================================

/** Python `obj.count(v[, start[, end]])` for str/list/tuple/bytes. */
export function pyCount(obj, v, start, end) {
    if (typeof obj === "string") {
        // E3 r3: CPython's argument-clinic check — a non-str needle is a
        // TypeError (the value `None` prints as None, not NoneType), never
        // a silent 0.
        if (typeof v !== "string") {
            const shown = v === null || v === undefined ? "None" : __pyTypeName(v);
            throw new TypeError_(`count() argument 1 must be str, not ${shown}`);
        }
        // E3: full spec — start/end are code-point slice bounds. Clamp like
        // a slice, take the segment, count within it.
        let s = obj;
        start = __strSliceBound(start);
        end = __strSliceBound(end);
        if (start !== null || end !== null) {
            const cps = [...obj];
            const n0 = cps.length;
            let st = start === null ? 0 : start;
            let en = end === null ? n0 : end;
            if (st < 0) st = Math.max(0, n0 + st);
            if (en < 0) en = Math.max(0, n0 + en);
            if (en > n0) en = n0;
            if (st > en) return 0;
            s = cps.slice(st, en).join("");
        }
        // #327: the empty substring matches at every gap (before each char
        // and at the end) → len(segment)+1, counted in code points.
        if (v.length === 0) {
            return (/[\uD800-\uDBFF]/.test(s) ? [...s].length : s.length) + 1;
        }
        // E3 r3: when either side carries surrogates the scan runs in
        // CODE-POINT space (same discipline as __pyStrFind) — a lone
        // surrogate needle must never match half of an astral pair
        // ('a😀b'.count('\ud83d') is 0 in CPython, but JS indexOf finds the
        // pair's high half). Non-overlapping, like the fast path.
        if (__hasSurrogate(s) || __hasSurrogate(v)) {
            const hs = [...s], nd = [...v], m = nd.length;
            let n = 0, i = 0;
            while (i + m <= hs.length) {
                let k = 0;
                while (k < m && hs[i + k] === nd[k]) k++;
                if (k === m) { n++; i += m; } else { i++; }
            }
            return n;
        }
        let n = 0, i = 0;
        while ((i = s.indexOf(v, i)) !== -1) { n++; i += v.length; }
        return n;
    }
    if (Array.isArray(obj)) return pyListCount(obj, v);
    // Custom receivers with their own count (bytes/bytearray via the
    // PyBytes prototype — the bytes query engine — deque, user classes).
    // Forward start/end so bytes.count's optional args survive dispatch.
    if (obj != null && typeof obj.count === "function") return obj.count(v, start, end);
    throw new TypeError_(`object of type '${__pyTypeName(obj)}' has no count()`); // #467
}

/** Python `obj.clear()` for list/dict/set. */
export function pyClear(obj) {
    if (Array.isArray(obj)) { obj.length = 0; return; }
    if (obj instanceof Set || obj instanceof Map) { obj.clear(); return; }
    // Custom receivers with their own clear (deque, user classes).
    // WB-6 round 2: propagate the user method's return value.
    if (obj != null && !__isPlainObj(obj) && typeof obj.clear === "function") { return obj.clear(); }
    if (obj && typeof obj === "object") {
        for (const k of __pyOwnKeys(obj)) delete obj[k]; // r6: symbols cleared too
        return;
    }
    throw new TypeError_(`object of type '${__pyTypeName(obj)}' has no clear()`); // #467
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
    throw new TypeError_(`object of type '${__pyTypeName(obj)}' has no copy()`); // #467
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
    // Option B: a boxed (integer-valued) float style value must unbox to a
    // native Number here — React appends the "px" unit ONLY when
    // `typeof value === "number"`, so a box would silently drop the unit
    // (`padding: 10.0` rendering without "px"). Key conversion: _X →
    // X.toUpperCase() (generic snake→camel), preserving leading dashes
    // (CSS custom props: --my-var) and vendor prefixes (-webkit-foo) —
    // those legitimately use `-`, not `_`, so we only act on `_`.
    const unboxV = (v) => (v != null && v.__pyfloat__ === true ? v.valueOf() : v);
    let out = null;
    const keys = Object.keys(style);
    for (let i = 0; i < keys.length; i++) {
        const k = keys[i];
        const camel = k.indexOf("_") < 0
            ? k
            : k.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
        const v = unboxV(style[k]);
        if (out === null && (camel !== k || v !== style[k])) {
            // Copy-on-write: materialize only when something changes.
            out = {};
            for (let j = 0; j < i; j++) out[keys[j]] = unboxV(style[keys[j]]);
        }
        if (out !== null) out[camel] = v;
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
        // WB-6: forward ALL args (zero or more) in ONE call so a NO-ARG user
        // `.update()` is honored. The old `for (const o of others)` loop
        // silently DROPPED a no-arg call — a user method named `update`
        // collides with dict.update, whose loop assumes >=1 arg. Single-arg
        // Counter/user calls are unchanged: obj.update(...[x]) === obj.update(x).
        // WB-6 round 2: PROPAGATE the user method's return value (Python
        // returns it; the old bare call+return read as None).
        return obj.update(...others);
    }
    if (obj instanceof Map) {
        // Explicit .entries(): PyDict's default iterator yields KEYS
        // (Python semantics), so implicit Map iteration would misfire.
        // delta4: __pyOwnKeys, not Object.entries — Symbol-keyed entries survive.
        for (const o of others) {
            if (o instanceof Map) { for (const [k, v] of o.entries()) obj.set(k, v); }
            else if (__pyUpdatePairs(o)) { for (const p of o) { __pyUpdatePairShape(p); obj.set(p[0], p[1]); } } // WB-6
            else { for (const k of __pyOwnKeys(o)) obj.set(k, o[k]); }
        }
        return;
    }
    if (obj && typeof obj === "object") {
        for (const o of others) {
            if (o instanceof Map) {
                // Plain-object receiver updated from a Map-backed dict:
                // keys pass through JS property coercion (the documented
                // plain-shape residual for non-string keys).
                for (const [k, v] of o.entries()) __pyDictWrite(obj, k, v);
            } else if (__pyUpdatePairs(o)) {
                // WB-6: CPython `dict.update` accepts an ITERABLE OF PAIRS
                // (`d.update([("k", "v")])`) — the own-keys walk turned it
                // into `{'0': ('k', 'v')}`.
                for (const p of o) { __pyUpdatePairShape(p); __pyDictWrite(obj, p[0], p[1]); }
            } else {
                // SEC-7: NOT Object.assign — it re-[[Set]]s each key, so an
                // own "__proto__" data key on `o` (exactly what JSON.parse
                // produces from remote input) reparents `obj`. Mirrors
                // pyDictMerge's __pyOwnKeys walk (delta4: symbols survive).
                for (const k of __pyOwnKeys(o)) __pyDictWrite(obj, k, o[k]);
            }
        }
        return;
    }
    throw new TypeError_(`object of type '${__pyTypeName(obj)}' has no update()`); // #467
}

/** WB-6: is a `dict.update` ARGUMENT the iterable-of-pairs form? CPython's
 * rule: a mapping (has `keys`) merges by keys; any other iterable is
 * consumed as (k, v) pairs. JS wrinkles: Arrays and Sets carry a native
 * `.keys` METHOD, so they are matched explicitly as pairs first; strings
 * are excluded (CPython raises — the legacy own-keys walk keeps that shape
 * out of the pairs path); Maps are the mapping branch upstream. */
function __pyUpdatePairs(o) {
    if (Array.isArray(o) || o instanceof Set) return true;
    return (
        o != null
        && typeof o !== "string"
        && typeof o[Symbol.iterator] === "function"
        && typeof o.keys !== "function"
    );
}

/** WB-6: each element of a pairs-form update must be a length-2 sequence
 * (CPython: "dictionary update sequence element has length N; 2 is
 * required"). */
function __pyUpdatePairShape(p) {
    if (!Array.isArray(p) || p.length !== 2) {
        const n = Array.isArray(p) ? p.length : 1;
        throw new ValueError(`dictionary update sequence element has length ${n}; 2 is required`);
    }
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
    throw new TypeError_(`object of type '${__pyTypeName(obj)}' has no remove()`); // #467
}

// ============================================================
// String helpers (additional)
// ============================================================

/** Python `s.center(width, fillchar=" ")` — code-point width; CPython puts
 * the odd extra pad on the LEFT when both the margin and width are odd
 * (#328: `left = marg//2 + (marg & width & 1)`). */
export function pyStrCenter(s, width, fillchar = " ") {
    width = __strIndexArg(width);
    __strFillcharCheck(fillchar);
    const len = [...s].length;
    const need = width - len;
    if (need <= 0) return s;
    const left = Math.floor(need / 2) + (need & width & 1);
    const right = need - left;
    return fillchar.repeat(left) + s + fillchar.repeat(right);
}

/** Python `s.ljust(width, fillchar=" ")` — pad right to width (code points). */
export function pyStrLjust(s, width, fillchar = " ") {
    width = __strIndexArg(width);
    __strFillcharCheck(fillchar);
    const len = [...s].length;
    return len >= width ? s : s + fillchar.repeat(width - len);
}

/** Python `s.rjust(width, fillchar=" ")` — pad left to width (code points). */
export function pyStrRjust(s, width, fillchar = " ") {
    width = __strIndexArg(width);
    __strFillcharCheck(fillchar);
    const len = [...s].length;
    return len >= width ? s : fillchar.repeat(width - len) + s;
}

/** Python `s.expandtabs(tabsize=8)` — full spec: keyword form, tabsize 0
 * (removes tabs), column reset on \n and \r. */
export function pyStrExpandtabs(s, tabsize = 8) {
    if (tabsize !== null && typeof tabsize === "object" && tabsize.__pyfloat__ !== true) {
        tabsize = tabsize.tabsize !== undefined ? tabsize.tabsize : 8;
    }
    tabsize = __strIndexArg(tabsize);
    let out = "";
    let col = 0;
    for (const ch of s) {
        if (ch === "\t") {
            if (tabsize > 0) {
                const fill = tabsize - (col % tabsize);
                out += " ".repeat(fill);
                col += fill;
            }
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
    // autotester: CPython returns a TUPLE (type-repr parity — the marked
    // array via pyTuple, not a bare list).
    if (!sep) throw new ValueError("empty separator");
    const i = s.indexOf(sep);
    if (i === -1) return pyTuple(s, "", "");
    return pyTuple(s.slice(0, i), sep, s.slice(i + sep.length));
}

/** Python `s.rpartition(sep)` — same as partition but from the right. */
export function pyStrRpartition(s, sep) {
    if (!sep) throw new ValueError("empty separator");
    const i = s.lastIndexOf(sep);
    if (i === -1) return pyTuple("", "", s);
    return pyTuple(s.slice(0, i), sep, s.slice(i + sep.length));
}

/** Python `s.rsplit([sep[, maxsplit]])` — full CPython spec: keyword forms,
 * whitespace mode honors maxsplit FROM THE RIGHT, empty separator raises
 * ValueError (issue #92). */
export function pyStrRsplit(s, sep, maxsplit) {
    if (typeof s !== "string" && s != null && typeof s.rsplit === "function") {
        return s.rsplit(sep, maxsplit);
    }
    // E3 r3: same bag discrimination as pyStrSplit (a bag must carry a
    // keyword name; __index__-bearing objects are positional maxsplits).
    const optsFrom = (v) =>
        v !== null && typeof v === "object" && !Array.isArray(v) && v.__pyfloat__ !== true
        && typeof v.__index__ !== "function"
        && ("sep" in v || "maxsplit" in v);
    if (optsFrom(sep)) {
        if ("maxsplit" in sep) maxsplit = sep.maxsplit;
        sep = "sep" in sep ? sep.sep : undefined;
    } else if (optsFrom(maxsplit)) {
        maxsplit = "maxsplit" in maxsplit ? maxsplit.maxsplit : undefined;
    }
    if (maxsplit !== undefined && maxsplit !== null) maxsplit = __strIndexArg(maxsplit);
    const lim =
        maxsplit === undefined || maxsplit === null || maxsplit < 0
            ? Infinity
            : maxsplit;

    if (sep === undefined || sep === null) {
        const trimmed = s.replace(__PY_WS_TRAIL, "");
        if (trimmed.replace(__PY_WS_LEAD, "") === "") return [];
        if (lim === Infinity) {
            return trimmed.replace(__PY_WS_LEAD, "").split(__PY_WS_RUN);
        }
        // maxsplit applies from the RIGHT: collect words right-to-left.
        const out = [];
        let i = trimmed.length;
        while (out.length < lim) {
            while (i > 0 && __PY_WS_RE.test(trimmed[i - 1])) i--;
            if (i <= 0) break;
            let end = i;
            while (i > 0 && !__PY_WS_RE.test(trimmed[i - 1])) i--;
            out.unshift(trimmed.slice(i, end));
        }
        while (i > 0 && __PY_WS_RE.test(trimmed[i - 1])) i--;
        // E3 r3: mirror of split — the unsplit remainder keeps its LEADING
        // whitespace ('  a b  '.rsplit(None, 0) == ['  a b'] in CPython).
        if (i > 0) out.unshift(trimmed.slice(0, i));
        return out;
    }
    if (typeof sep !== "string") {
        throw new TypeError_(`must be str or None, not ${__pyTypeName(sep)}`);
    }
    if (sep === "") throw new ValueError("empty separator");
    if (lim === Infinity) return s.split(sep);
    const parts = s.split(sep);
    if (parts.length <= lim) return parts;
    const head = parts.slice(0, parts.length - lim).join(sep);
    return [head, ...parts.slice(parts.length - lim)];
}

/** Python `s.splitlines(keepends=False)` — the FULL CPython line-boundary
 * set: \n \r \r\n \v \f \x1c \x1d \x1e \x85 U+2028 U+2029. */
export function pyStrSplitlines(s, keepends = false) {
    if (keepends !== null && typeof keepends === "object" && keepends.__pyfloat__ !== true) {
        keepends = keepends.keepends !== undefined ? keepends.keepends : false;
    }
    if (s.length === 0) return [];
    const isBound = (ch) =>
        ch === "\n" || ch === "\r" || ch === "\v" || ch === "\f"
        || ch === "\x1c" || ch === "\x1d" || ch === "\x1e" || ch === "\x85"
        || ch === "\u2028" || ch === "\u2029";
    const out = [];
    let start = 0;
    let i = 0;
    while (i < s.length) {
        const ch = s[i];
        if (isBound(ch)) {
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
    if (typeof s !== "string" && s != null && typeof s.swapcase === "function") {
        return s.swapcase();
    }
    // E3 r2 (oracle-swept 0x0..0x10FFFF): CPython swaps ONLY Lowercase→
    // upper() and Uppercase→lower(); titlecase digraphs (ǅ ǈ ǋ ǲ,
    // ᾈ-ῼ) and uncased chars pass through UNCHANGED — the old
    // lower/upper toggle mangled all 31 Lt code points.
    let out = "";
    for (const ch of s) {
        if (/\p{Lowercase}/u.test(ch)) out += ch.toUpperCase();
        else if (/\p{Uppercase}/u.test(ch)) out += ch.toLowerCase();
        else out += ch;
    }
    return out;
}

/** Python `s.translate(table)` — table maps CODE POINTS to str, code point
 * int, or None (delete). E3 r2: mapping RESULTS are validated like CPython —
 * a non-int/str/None entry raises TypeError ("character mapping must return
 * integer, None or str"), an out-of-range int ValueError; BigInt keys are
 * looked up EXACTLY (no lossy Number round-trip). */
export function pyStrTranslate(s, table) {
    if (typeof s !== "string" && s != null && typeof s.translate === "function") {
        return s.translate(table);
    }
    // E3 r3: the table is ANY subscriptable — CPython does `table[ord(ch)]`
    // per char and keeps the char on LookupError. dict/Map, str, list/tuple,
    // and custom-__getitem__ mappings all work; a NON-subscriptable table
    // (None, int, float, …) raises "'NoneType' object is not subscriptable"
    // LAZILY at the first lookup — ''.translate(None) is '' because no
    // lookup ever happens. (The old `if (!table) return s` short-circuit
    // silently passed None/0 through.)
    let lookup;
    if (table instanceof Map) {
        lookup = (cp) => {
            if (table.has(cp)) return table.get(cp);
            const bk = BigInt(cp);
            if (table.has(bk)) return table.get(bk);
            return undefined;
        };
    } else if (typeof table === "string") {
        // str table: indexed by CODE POINT; out-of-range is CPython's
        // IndexError → keep the char.
        const tcp = __hasSurrogate(table) ? [...table] : table;
        lookup = (cp) => (cp < tcp.length ? tcp[cp] : undefined);
    } else if (Array.isArray(table)) {
        // list/tuple table: positional; out-of-range IndexError → keep.
        lookup = (cp) => (cp < table.length ? table[cp] : undefined);
    } else if (table != null && typeof table.__getitem__ === "function") {
        // Custom mapping: LookupError (KeyError/IndexError) → keep the
        // char; any OTHER exception propagates, as in CPython.
        lookup = (cp) => {
            try {
                return table.__getitem__(cp);
            } catch (e) {
                if (e instanceof LookupError) return undefined;
                throw e;
            }
        };
    } else if (table !== null && typeof table === "object" && table.__pyfloat__ !== true) {
        lookup = (cp) => table[cp];
    } else {
        lookup = () => {
            throw new TypeError_(`'${__pyTypeName(table)}' object is not subscriptable`);
        };
    }
    let out = "";
    for (const ch of s) {
        let entry = lookup(ch.codePointAt(0));
        if (entry === undefined) { out += ch; continue; }
        if (entry === null) continue; // delete
        if (typeof entry === "string") { out += entry; continue; }
        // bool ⊂ int: {97: True} maps to '\x01' in CPython, not a TypeError.
        if (typeof entry === "boolean") entry = entry ? 1 : 0;
        if (typeof entry === "number" || typeof entry === "bigint") {
            if (typeof entry === "number" && !Number.isInteger(entry)) {
                throw new TypeError_("character mapping must return integer, None or str");
            }
            const cp2 = typeof entry === "bigint" ? entry : BigInt(entry);
            if (cp2 < 0n || cp2 >= 0x110000n) {
                throw new ValueError("character mapping must be in range(0x110000)");
            }
            out += String.fromCodePoint(Number(cp2));
            continue;
        }
        throw new TypeError_("character mapping must return integer, None or str");
    }
    return out;
}

/** Python `s.isidentifier()` — Unicode identifier per ID_Start/ID_Continue
 * (CPython uses XID_*; the ID_* JS properties differ on ~40 exotic points). */
export function pyStrIsidentifier(s) {
    return /^[\p{XID_Start}_][\p{XID_Continue}]*$/u.test(s);
}

/** Python `s.isprintable()` — false iff any char is Cc/Cf/Cs/Co/Cn/Zl/Zp/Zs
 * other than ASCII space. */
export function pyStrIsprintable(s) {
    for (const ch of s) {
        if (ch !== " " && __RE_UNPRINTABLE.test(ch)) return false;
    }
    return true;
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
    // E3 r2: full arg validation — CPython messages, and count through the
    // interpreted-as-integer protocol (bool ⊂ int; float → TypeError).
    if (typeof oldv !== "string") {
        throw new TypeError_(`replace() argument 1 must be str, not ${__pyTypeName(oldv)}`);
    }
    if (typeof newv !== "string") {
        throw new TypeError_(`replace() argument 2 must be str, not ${__pyTypeName(newv)}`);
    }
    if (count !== undefined && count !== null) count = __strIndexArg(count);
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

/**
 * WB-18 — RUNTIME dispatcher for `.replace(...)` on a (possibly-)Python-str
 * receiver. Compile time cannot know a VARIABLE's type, so the WB-10 syntactic
 * check (inline `RegExp(...)` first arg / function-literal second arg) missed a
 * regex held in a variable and mis-routed it to `pyStrReplace` (Python
 * str.replace — which never applies the regex). Decide on the RUNTIME type of
 * the arguments so inline, variable, and any-expression regex all behave
 * identically:
 *   - a RegExp pattern OR a function replacer ⇒ JS `String.prototype.replace`
 *     (regex match, capture groups, `$1` backrefs, function replacer);
 *   - two plain strings ⇒ Python `str.replace` (replace-ALL, honoring `count`).
 * Non-string receivers with their own `.replace` (DOMTokenList, user classes)
 * fall through to pyStrReplace, which already dispatches them natively.
 */
export function pyStrReplaceSmart(s, a, b, count) {
    if (a instanceof RegExp || typeof b === "function") {
        return s.replace(a, b);
    }
    return pyStrReplace(s, a, b, count);
}

export function pyStrIsupper(s) {
    if (typeof s !== "string" && s != null && typeof s.isupper === "function") {
        return s.isupper();
    }
    // CPython: at least one cased char, no lowercase, no titlecase.
    // \p{Uppercase}/\p{Lowercase} are the binary properties (cover
    // Other_Uppercase Roman numerals / Other_Lowercase modifiers).
    return /\p{Uppercase}/u.test(s) && !/[\p{Lowercase}\p{Lt}]/u.test(s);
}
export function pyStrIslower(s) {
    if (typeof s !== "string" && s != null && typeof s.islower === "function") {
        return s.islower();
    }
    return /\p{Lowercase}/u.test(s) && !/[\p{Uppercase}\p{Lt}]/u.test(s);
}

/** Python `s.istitle()` — upper/titlecase may not follow a cased char,
 * lowercase must follow one; needs at least one cased char. */
export function pyStrIstitle(s) {
    if (typeof s !== "string" && s != null && typeof s.istitle === "function") {
        return s.istitle();
    }
    let prevCased = false;
    let hasCased = false;
    for (const ch of s) {
        const isUpper = /[\p{Uppercase}\p{Lt}]/u.test(ch);
        const isLower = !isUpper && /\p{Lowercase}/u.test(ch);
        if (isUpper) {
            if (prevCased) return false;
            hasCased = true;
            prevCased = true;
        } else if (isLower) {
            if (!prevCased) return false;
            hasCased = true;
            prevCased = true;
        } else {
            prevCased = false;
        }
    }
    return hasCased;
}

/** Python `s.startswith(prefix[, start[, end]])` — full CPython spec:
 * `prefix` may be a str OR a tuple of strs; start/end are CODE-POINT
 * indices with negative-index clamping (slice rules). The old lowering was
 * a bare JS .startsWith rename that ignored all of that. */
function __pyStrStartsEnds(s, fix, start, end, atEnd) {
    const name = atEnd ? "endswith" : "startswith";
    // E3 r3: slice bounds convert FIRST (CPython parses start/end before it
    // types the affix — 'abc'.startswith(1, 2.5) is the slice-indices error).
    start = __strSliceBound(start);
    end = __strSliceBound(end);
    // Then CPython's affix taxonomy — str or a TUPLE of str only. A list
    // (or any other type) is "first arg must be str or a tuple of str, not
    // list"; a non-str INSIDE a tuple is "tuple for startswith must only
    // contain str, not int", checked lazily item-by-item so an earlier
    // matching element still short-circuits to True
    // ('abc'.startswith(('a', 1)) is True in CPython).
    let cands;
    if (typeof fix === "string") {
        cands = [fix];
    } else if (Array.isArray(fix) && fix.__pytuple__) {
        cands = fix;
    } else {
        throw new TypeError_(
            `${name} first arg must be str or a tuple of str, not ${__pyTypeName(fix)}`,
        );
    }
    const cps = Array.from(s);
    const n = cps.length;
    let st = start === null ? 0 : start;
    let en = end === null ? n : end;
    if (st < 0) st = Math.max(0, n + st);
    if (en < 0) en = Math.max(0, n + en);
    if (en > n) en = n;
    if (st > en) return false;
    const seg = cps.slice(st, en).join("");
    for (const p of cands) {
        if (typeof p !== "string") {
            throw new TypeError_(
                `tuple for ${name} must only contain str, not ${__pyTypeName(p)}`,
            );
        }
        if (atEnd ? seg.endsWith(p) : seg.startsWith(p)) return true;
    }
    return false;
}
export function pyStrStartswith(s, prefix, start, end) {
    if (typeof s !== "string" && s != null && typeof s.startswith === "function") {
        return s.startswith(prefix, start, end);
    }
    return __pyStrStartsEnds(s, prefix, start, end, false);
}
export function pyStrEndswith(s, suffix, start, end) {
    if (typeof s !== "string" && s != null && typeof s.endswith === "function") {
        return s.endswith(suffix, start, end);
    }
    return __pyStrStartsEnds(s, suffix, start, end, true);
}

/** Python `s.rfind(sub)` — LAST occurrence as a CODE-POINT offset, -1 if
 * absent. Wave-19 verification fix: raw .lastIndexOf returns UTF-16
 * code-unit offsets ('𝔸x𝔸x'.rfind('x') must be 3, not 5). */
export function pyStrRfind(s, sub, start, end) {
    if (typeof s !== "string" && s != null && typeof s.rfind === "function") {
        return s.rfind(sub, start, end);
    }
    return __pyStrFind(s, sub, start, end, true, "rfind");
}

/** Python `s.rindex(sub[, start[, end]])` — like rfind but raises ValueError if absent. */
export function pyStrRindex(s, sub, start, end) {
    // Receiver's own rindex wins (bytes query engine, user classes) — so
    // bytes keep their CPython "subsection not found" wording.
    if (typeof s !== "string" && s != null && typeof s.rindex === "function") {
        return s.rindex(sub, start, end);
    }
    const i = __pyStrFind(s, sub, start, end, true, "rindex");
    if (i === -1) throw new ValueError(`substring not found`);
    return i;
}

// ============================================================
// E3 — the CPython text authority: full-spec str-method helpers
// ============================================================

/** Python `s.casefold()` — FULL Unicode case folding: toLowerCase (≈ lower())
 * plus the generated residual where the full fold differs (ß→ss, ﬁ→fi, µ→μ). */
export function pyStrCasefold(s) {
    if (typeof s !== "string" && s != null && typeof s.casefold === "function") {
        return s.casefold();
    }
    let out = "";
    for (const ch of s) {
        const f = __CASEFOLD_MAP.get(ch.codePointAt(0));
        out += f !== undefined ? f : ch.toLowerCase();
    }
    return out;
}

/** Python `s.isdecimal()` — Nd only (generated from the oracle CPython). */
export function pyStrIsdecimal(s) {
    if (typeof s !== "string" && s != null && typeof s.isdecimal === "function") {
        return s.isdecimal();
    }
    return __RE_DECIMAL.test(s);
}

/** Python `s.isdigit()` — decimal + Numeric_Type=Digit ('²'.isdigit()). */
export function pyStrIsdigit(s) {
    if (typeof s !== "string" && s != null && typeof s.isdigit === "function") {
        return s.isdigit();
    }
    return __RE_DIGIT.test(s);
}

/** Python `s.isnumeric()` — digit + Numeric_Type=Numeric ('½', '十'). */
export function pyStrIsnumeric(s) {
    if (typeof s !== "string" && s != null && typeof s.isnumeric === "function") {
        return s.isnumeric();
    }
    return __RE_NUMERIC.test(s);
}

/** Python `s.isascii()` — runtime twin of the inline spec (complex
 * receivers + bytes dispatch). */
export function pyStrIsascii(s) {
    if (typeof s !== "string" && s != null && typeof s.isascii === "function") {
        return s.isascii();
    }
    return [...s].every((c) => c.charCodeAt(0) < 128);
}

/** Python `s.isalpha()` — Unicode category L (runtime twin). */
export function pyStrIsalpha(s) {
    if (typeof s !== "string" && s != null && typeof s.isalpha === "function") {
        return s.isalpha();
    }
    return /^\p{L}+$/u.test(s);
}

/** Python `s.isspace()` — the PYTHON whitespace set (runtime twin). */
export function pyStrIsspace(s) {
    if (typeof s !== "string" && s != null && typeof s.isspace === "function") {
        return s.isspace();
    }
    return s.length > 0 && new RegExp("^[" + __PY_WS_CC + "]+$").test(s);
}

/** Python `s.removeprefix(p)` — THE lowering for every receiver shape
 * (E3 r2: the inline arm was deleted — it double-evaluated the argument
 * and silently coerced non-str prefixes). */
export function pyStrRemoveprefix(s, p) {
    if (typeof s !== "string" && s != null && typeof s.removeprefix === "function") {
        return s.removeprefix(p);
    }
    if (typeof p !== "string") {
        throw new TypeError_(`removeprefix() argument must be str, not ${__pyTypeName(p)}`);
    }
    return s.startsWith(p) ? s.slice(p.length) : s;
}

/** Python `s.removesuffix(p)` — THE lowering for every receiver shape. */
export function pyStrRemovesuffix(s, p) {
    if (typeof s !== "string" && s != null && typeof s.removesuffix === "function") {
        return s.removesuffix(p);
    }
    if (typeof p !== "string") {
        throw new TypeError_(`removesuffix() argument must be str, not ${__pyTypeName(p)}`);
    }
    return p.length > 0 && s.endsWith(p) ? s.slice(0, s.length - p.length) : s;
}

/** Python `s.isalnum()` — every char alphabetic OR decimal/digit/numeric. */
export function pyStrIsalnum(s) {
    if (typeof s !== "string" && s != null && typeof s.isalnum === "function") {
        return s.isalnum();
    }
    if (s.length === 0) return false;
    for (const ch of s) {
        if (!/\p{L}/u.test(ch) && !__RE_NUMERIC.test(ch) && !__RE_DIGIT.test(ch)
            && !__RE_DECIMAL.test(ch)) {
            return false;
        }
    }
    return true;
}

/** Python `s.zfill(width)` — sign-aware zero fill ('-5'.zfill(4) → '-005'),
 * code-point width. */
export function pyStrZfill(s, width) {
    if (typeof s !== "string" && s != null && typeof s.zfill === "function") {
        return s.zfill(width);
    }
    width = __strIndexArg(width);
    const len = [...s].length;
    if (len >= width) return s;
    const sign = s[0] === "+" || s[0] === "-" ? s[0] : "";
    return sign + "0".repeat(width - len) + s.slice(sign.length);
}

/** `width`/`maxsplit`/`count`/`tabsize`-style int argument. E3 r3: DELEGATES
 * to __pyAsIndexInt — THE "interpreted as an integer" authority (CPython's
 * __index__ protocol) — so an object exposing a valid __index__ is ACCEPTED
 * (CPython takes it for maxsplit/count/width via the Py_ssize_t converter)
 * and a bad __index__ result raises "__index__ returned non-int (type X)".
 * bool ⊂ int, floats raise CPython's TypeError, BigInt narrows to Number
 * (these args feed Number arithmetic). */
function __strIndexArg(x) {
    const v = __pyAsIndexInt(x);
    return typeof v === "bigint" ? Number(v) : v;
}

function __strFillcharCheck(fillchar) {
    if (typeof fillchar !== "string" || [...fillchar].length !== 1) {
        throw new TypeError_("The fill character must be exactly one character long");
    }
}

/** UnicodeEncodeError — a ValueError subclass, as in CPython. */
class UnicodeEncodeError extends ValueError {
    static __name__ = "UnicodeEncodeError";
}
export { UnicodeEncodeError };

/** Python `s.encode(encoding='utf-8', errors='strict')` — the common codecs
 * (utf-8 / ascii / latin-1 + standard aliases) with the FULL CPython error
 * taxonomy (E3 r2):
 *   - encoding/errors must be str → TypeError ("encode() argument
 *     'encoding' must be str, not None");
 *   - unknown codec → LookupError("unknown encoding: X");
 *   - unknown ERROR HANDLER → LookupError("unknown error handler name 'X'"),
 *     raised LAZILY at the first failing char (CPython validates on use);
 *   - strict failures report the CONSECUTIVE RUN: one char is
 *     "character '\xe9' in position 1", a run is
 *     "characters in position 0-1". UnicodeEncodeError ⊂ ValueError. */
export function pyStrEncode(s, encoding = "utf-8", errors = "strict") {
    // E3 r3: the kwargs bag must CARRY a keyword name (same class as the
    // split/rsplit bag fix) — a bare object positional encoding must reach
    // the "must be str, not dict" TypeError below, not default to utf-8.
    if (encoding !== null && typeof encoding === "object" && encoding.__pyfloat__ !== true
        && ("encoding" in encoding || "errors" in encoding)) {
        const kw = encoding;
        encoding = kw.encoding !== undefined ? kw.encoding : "utf-8";
        errors = kw.errors !== undefined ? kw.errors : "strict";
    } else if (errors !== null && typeof errors === "object" && errors.__pyfloat__ !== true
        && "errors" in errors) {
        errors = errors.errors !== undefined ? errors.errors : "strict";
    }
    // CPython's argument-clinic message prints the VALUE `None`, not the
    // type name NoneType.
    const argName = (v) => (v === null || v === undefined ? "None" : __pyTypeName(v));
    if (typeof encoding !== "string") {
        throw new TypeError_(`encode() argument 'encoding' must be str, not ${argName(encoding)}`);
    }
    if (typeof errors !== "string") {
        throw new TypeError_(`encode() argument 'errors' must be str, not ${argName(errors)}`);
    }
    const enc = encoding.toLowerCase().replace(/[\s_]/g, "-");
    let codec;
    if (["utf-8", "utf8", "u8", "utf"].includes(enc)) codec = "utf-8";
    else if (["ascii", "us-ascii", "646"].includes(enc)) codec = "ascii";
    else if (["latin-1", "latin1", "latin", "l1", "iso-8859-1", "iso8859-1", "8859", "cp819"].includes(enc)) codec = "latin-1";
    else throw new LookupError(`unknown encoding: ${encoding}`);

    const escCh = (cp) =>
        cp < 0x100 ? "\\x" + cp.toString(16).padStart(2, "0")
            : cp < 0x10000 ? "\\u" + cp.toString(16).padStart(4, "0")
            : "\\U" + cp.toString(16).padStart(8, "0");
    const cps = [...s];
    const bad = (cp) => {
        const lone = cp >= 0xd800 && cp <= 0xdfff;
        if (codec === "utf-8") return lone;
        if (codec === "ascii") return cp >= 0x80 || lone;
        return cp >= 0x100; // latin-1
    };
    // E3 r3: "surrogates not allowed" is the UTF-8 reason ONLY. For the
    // range codecs CPython reports the RANGE even for a lone surrogate:
    // chr(0xD800).encode('ascii') → "ordinal not in range(128)" and
    // .encode('latin-1') → "ordinal not in range(256)" (verified on the
    // pinned 3.14.7 oracle).
    const reason = () => {
        if (codec === "utf-8") return "surrogates not allowed";
        return codec === "ascii" ? "ordinal not in range(128)" : "ordinal not in range(256)";
    };
    const bytes = [];
    for (let pos = 0; pos < cps.length; pos++) {
        const cp = cps[pos].codePointAt(0);
        if (bad(cp)) {
            if (errors === "ignore") continue;
            if (errors === "replace") { bytes.push(0x3f); continue; }
            if (errors !== "strict") {
                // CPython validates the handler name lazily, on first use.
                throw new LookupError(`unknown error handler name '${errors}'`);
            }
            // strict: report the whole CONSECUTIVE failing run.
            let runEnd = pos;
            while (runEnd + 1 < cps.length && bad(cps[runEnd + 1].codePointAt(0))) runEnd++;
            const where = runEnd === pos
                ? `character '${escCh(cp)}' in position ${pos}`
                : `characters in position ${pos}-${runEnd}`;
            throw new UnicodeEncodeError(
                `'${codec}' codec can't encode ${where}: ${reason(cp)}`,
            );
        }
        if (codec === "utf-8" && cp >= 0x80) {
            if (cp < 0x800) {
                bytes.push(0xc0 | (cp >> 6), 0x80 | (cp & 0x3f));
            } else if (cp < 0x10000) {
                bytes.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3f), 0x80 | (cp & 0x3f));
            } else {
                bytes.push(
                    0xf0 | (cp >> 18), 0x80 | ((cp >> 12) & 0x3f),
                    0x80 | ((cp >> 6) & 0x3f), 0x80 | (cp & 0x3f),
                );
            }
        } else {
            bytes.push(cp);
        }
    }
    return pyBytesOf(bytes);
}

/** Python `str.maketrans(x[, y[, z]])` — builds the translate table.
 * `recv` is the receiver the method-call lowering passes (the str type
 * object or a str instance) and is ignored. */
export function pyStrMaketrans(recv, x, y, z) {
    const table = new Map();
    if (y === undefined) {
        const isMapLike = x instanceof Map
            || (x !== null && typeof x === "object" && !Array.isArray(x) && x.__pyfloat__ !== true);
        if (!isMapLike) {
            throw new TypeError_("if you give only one argument to maketrans it must be a dict");
        }
        const entries = x instanceof Map ? x.entries() : Object.entries(x);
        for (const [k, v] of entries) {
            let key;
            if (typeof k === "string") {
                const kc = [...k];
                if (kc.length !== 1) {
                    throw new ValueError("string keys in translate table must be of length 1");
                }
                key = k.codePointAt(0);
            } else if (typeof k === "bigint" || typeof k === "number") {
                key = Number(k);
            } else {
                throw new TypeError_("keys in translate table must be strings or integers");
            }
            table.set(key, typeof v === "bigint" ? Number(v) : v);
        }
        return table;
    }
    if (typeof x !== "string" || typeof y !== "string") {
        throw new TypeError_("maketrans arguments must be strings");
    }
    const xa = [...x];
    const ya = [...y];
    if (xa.length !== ya.length) {
        throw new ValueError("the first two maketrans arguments must have equal length");
    }
    for (let i = 0; i < xa.length; i++) table.set(xa[i].codePointAt(0), ya[i]);
    if (z !== undefined && z !== null) {
        if (typeof z !== "string") {
            throw new TypeError_("maketrans arguments must be strings");
        }
        for (const ch of z) table.set(ch.codePointAt(0), null);
    }
    return table;
}

/** Python `s.format_map(mapping)` — like `s.format(**mapping)` but uses the
 * mapping object directly (no copy); missing keys raise KeyError. */
export function pyStrFormatMap(s, mapping) {
    return __pyStrFormatImpl(s, [], mapping);
}

/** Python printf-style `%` formatting (`fmt % args`) — the full mini-language:
 * %s %r %a %d %i %u %o %x %X %e %E %f %F %g %G %c %%, flags `-+ #0`,
 * width/precision incl. `*`, tuple / single-value / `%(name)s` mapping forms.
 * Numeric rendering routes through pyFormatSpec (ONE render engine). */
export function pyStrMod(fmt, args) {
    const isTuple = Array.isArray(args) && args.__pytuple__;
    const isMap = !isTuple
        && (args instanceof Map
            || (args !== null && typeof args === "object" && !Array.isArray(args)
                && args.__pyfloat__ !== true
                && Object.getPrototypeOf(args) === Object.prototype));
    const values = isTuple ? [...args] : [args];
    let vi = 0;
    const nextVal = () => {
        // A mapping also serves as the single positional value (CPython:
        // '%s' % {'a': 1} prints the dict).
        if (vi >= values.length) {
            throw new TypeError_("not enough arguments for format string");
        }
        return values[vi++];
    };
    const mapGet = (k) => {
        if (args instanceof Map) {
            if (!args.has(k)) throw new KeyError(k);
            return args.get(k);
        }
        if (!(k in args)) throw new KeyError(k);
        return args[k];
    };
    let out = "";
    let i = 0;
    const n = fmt.length;
    while (i < n) {
        const c = fmt[i];
        if (c !== "%") { out += c; i++; continue; }
        i++;
        if (fmt[i] === "%") { out += "%"; i++; continue; }
        // %(name)…
        let mapKey = null;
        if (fmt[i] === "(") {
            let depth = 1;
            let j = i + 1;
            let key = "";
            while (j < n) {
                if (fmt[j] === "(") depth++;
                else if (fmt[j] === ")") { depth--; if (depth === 0) break; }
                key += fmt[j];
                j++;
            }
            if (depth !== 0) throw new ValueError("incomplete format key");
            if (!isMap) throw new TypeError_("format requires a mapping");
            mapKey = key;
            i = j + 1;
        }
        const flags = { minus: false, plus: false, space: false, alt: false, zero: false };
        for (;;) {
            const f = fmt[i];
            if (f === "-") flags.minus = true;
            else if (f === "+") flags.plus = true;
            else if (f === " ") flags.space = true;
            else if (f === "#") flags.alt = true;
            else if (f === "0") flags.zero = true;
            else break;
            i++;
        }
        let width = null;
        if (fmt[i] === "*") {
            width = __strIndexArg(nextVal());
            if (width < 0) { flags.minus = true; width = -width; }
            i++;
        } else {
            let w = "";
            while (fmt[i] >= "0" && fmt[i] <= "9") { w += fmt[i]; i++; }
            // E3 r3: CPython bounds printf width/precision by PY_SSIZE_T_MAX
            // at parse time ("width too big" / "precision too big"), never a
            // silently-lossy JS number.
            if (w.length >= 19 && BigInt(w) > 9223372036854775807n) {
                throw new ValueError("width too big");
            }
            if (w) width = parseInt(w, 10);
        }
        let prec = null;
        if (fmt[i] === ".") {
            i++;
            if (fmt[i] === "*") {
                prec = __strIndexArg(nextVal());
                if (prec < 0) prec = 0;
                i++;
            } else {
                let p = "";
                while (fmt[i] >= "0" && fmt[i] <= "9") { p += fmt[i]; i++; }
                if (p.length >= 19 && BigInt(p) > 9223372036854775807n) {
                    throw new ValueError("precision too big");
                }
                prec = p ? parseInt(p, 10) : 0;
            }
        }
        while (fmt[i] === "h" || fmt[i] === "l" || fmt[i] === "L") i++;
        const conv = fmt[i];
        if (conv === undefined) throw new ValueError("incomplete format");
        i++;
        const val = mapKey !== null ? mapGet(mapKey) : nextVal();
        out += __printfOne(conv, val, flags, width, prec, i - 1);
    }
    if (!isMap && vi < values.length) {
        throw new TypeError_("not all arguments converted during string formatting");
    }
    return out;
}

function __printfOne(conv, val, flags, width, prec, atIndex) {
    const padStr = (body) => {
        const len = [...body].length;
        if (width == null || len >= width) return body;
        const pad = " ".repeat(width - len);
        return flags.minus ? body + pad : pad + body;
    };
    if (conv === "s" || conv === "r" || conv === "a") {
        let body = conv === "s" ? pyStr(val) : conv === "r" ? pyRepr(val) : pyAscii(val);
        if (prec != null) body = [...body].slice(0, prec).join("");
        return padStr(body);
    }
    if (conv === "c") {
        // E3 r3: %c does its OWN range check — CPython's %c reports
        // "%c arg not in range(0x110000)" for ANY out-of-range int (2**100
        // included; verified on the pinned 3.14.7 oracle), unlike
        // format(x, 'c') whose C-long conversion trips first. bool counts
        // as its int value ('%c' % True → '\x01'); an __index__-bearing
        // object is accepted ('%c' % I(65) → 'A').
        let cv = val;
        if (typeof cv === "boolean") cv = cv ? 1 : 0;
        if (typeof cv === "string") {
            if ([...cv].length !== 1) {
                throw new TypeError_("%c requires an int or a unicode character, not str");
            }
            return padStr(cv);
        }
        let isIntVal = typeof cv === "bigint"
            || (typeof cv === "number" && Number.isInteger(cv)
                && !(val != null && val.__pyfloat__ === true));
        if (!isIntVal && val != null && val.__pyfloat__ !== true
            && typeof val.__index__ === "function") {
            try {
                cv = __pyAsIndexInt(val);
                isIntVal = true;
            } catch (e) {
                isIntVal = false;
            }
        }
        if (!isIntVal) {
            throw new TypeError_(`%c requires an int or a unicode character, not ${__pyTypeName(val)}`);
        }
        const cnum = typeof cv === "bigint" ? cv : BigInt(cv);
        if (cnum < 0n || cnum >= 0x110000n) {
            throw new OverflowError("%c arg not in range(0x110000)");
        }
        const opts = { type: "c" };
        if (width != null) {
            opts.width = width;
            opts.align = flags.minus ? "<" : ">";
        }
        return pyFormatSpec(Number(cnum), opts);
    }
    if ("diu".includes(conv) || "oxX".includes(conv)) {
        // E3 r2: integer conversions render through pyFormatSpec (ONE digit/
        // sign/prefix/zero-pad engine — exactly like the float conversions
        // below); only printf's min-digit precision is assembled here.
        let iv = val;
        if (typeof iv === "boolean") iv = iv ? 1 : 0;
        if (iv != null && iv.__pyfloat__ === true) iv = iv.valueOf();
        if ("oxX".includes(conv)) {
            // E3 r3: %o/%x/%X take int OR an __index__-bearing object
            // (PyNumber_Index); ANY protocol failure — no __index__, or a
            // bad __index__ result — is replaced by CPython's
            // "%x format: an integer is required, not X".
            let isIntVal = typeof iv === "bigint"
                || (typeof iv === "number" && Number.isInteger(iv)
                    && !(val != null && val.__pyfloat__ === true));
            if (!isIntVal && val != null && val.__pyfloat__ !== true
                && typeof val.__index__ === "function") {
                try {
                    iv = __pyAsIndexInt(val);
                    isIntVal = true;
                } catch (e) {
                    isIntVal = false;
                }
            }
            if (!isIntVal) {
                throw new TypeError_(`%${conv} format: an integer is required, not ${__pyTypeName(val)}`);
            }
        } else {
            // E3 r3: %d/%i/%u take real numbers AND objects converting via
            // __index__ or __int__ (PyNumber_Long); any conversion failure
            // is CPython's "%d format: a real number is required, not X".
            if (typeof iv !== "bigint" && typeof iv !== "number") {
                let converted = false;
                if (val != null && val.__pyfloat__ !== true) {
                    for (const proto of ["__int__", "__index__"]) {
                        if (typeof val[proto] === "function") {
                            let r;
                            try {
                                r = val[proto]();
                            } catch (e) {
                                break; // conversion failed → the %d TypeError
                            }
                            if (typeof r === "boolean") { iv = r ? 1 : 0; converted = true; }
                            else if (typeof r === "bigint") { iv = r; converted = true; }
                            else if (typeof r === "number" && Number.isInteger(r)) { iv = r; converted = true; }
                            break;
                        }
                    }
                }
                if (!converted) {
                    throw new TypeError_(`%${conv} format: a real number is required, not ${__pyTypeName(val)}`);
                }
            }
            if (typeof iv === "number" && !Number.isInteger(iv)) {
                if (Number.isNaN(iv)) {
                    throw new ValueError("cannot convert float NaN to integer");
                }
                if (!Number.isFinite(iv)) {
                    throw new OverflowError("cannot convert float infinity to integer");
                }
                iv = Math.trunc(iv);
            }
            // E3 r3: a float beyond 2**53 must convert to its EXACT integer
            // value ('%d' % 1e100 prints all 101 digits in CPython) —
            // BigInt(double) is exact for integer-valued doubles; going
            // through the Number render path would print "1e+100".
            if (typeof iv === "number" && !Number.isSafeInteger(iv)) {
                iv = BigInt(iv);
            }
        }
        const ty = conv === "o" || conv === "x" || conv === "X" ? conv : "d";
        if (prec == null) {
            const opts = { type: ty };
            if (flags.plus) opts.sign = "+";
            else if (flags.space) opts.sign = " ";
            if (flags.alt && ty !== "d") opts.alt = true;
            if (width != null) {
                opts.width = width;
                if (flags.minus) opts.align = "<";
                else if (flags.zero) opts.zero = true;
            }
            return pyFormatSpec(iv, opts);
        }
        // printf precision = MINIMUM digit count (no engine equivalent —
        // the mini-language forbids precision on ints): bare engine digits,
        // zero-extended, then sign/prefix/width assembled.
        const neg = iv < 0;
        let digits = pyFormatSpec(neg ? -iv : iv, { type: ty });
        if (digits.length < prec) digits = "0".repeat(prec - digits.length) + digits;
        let prefix = "";
        if (flags.alt) {
            if (ty === "o") prefix = "0o";
            else if (ty === "x") prefix = "0x";
            else if (ty === "X") prefix = "0X";
        }
        const sign = neg ? "-" : flags.plus ? "+" : flags.space ? " " : "";
        // E3 r3: the 0-flag still zero-fills to WIDTH when a precision is
        // present — '%08.3d' % 7 is '00000007' in CPython (the r2 code
        // dropped the flag and space-padded), with the zeros between the
        // sign/prefix and the digits ('%08.3d' % -7 → '-0000007',
        // '%#08.3x' % 255 → '0x0000ff'). '-' (left-justify) beats '0'.
        if (flags.zero && !flags.minus && width != null) {
            const assembled = sign.length + prefix.length + digits.length;
            if (assembled < width) {
                digits = "0".repeat(width - assembled) + digits;
            }
        }
        return padStr(sign + prefix + digits);
    }
    if ("eEfFgG".includes(conv)) {
        let fv = val;
        if (typeof fv === "boolean") fv = fv ? 1 : 0;
        if (fv != null && fv.__pyfloat__ === true) fv = fv.valueOf();
        if (typeof fv === "bigint") fv = Number(fv);
        if (typeof fv !== "number") {
            throw new TypeError_(`%${conv} format: a real number is required, not ${__pyTypeName(val)}`);
        }
        const opts = { type: conv, precision: prec == null ? 6 : prec };
        if (flags.plus) opts.sign = "+";
        else if (flags.space) opts.sign = " ";
        if (flags.alt) opts.alt = true;
        if (width != null) {
            opts.width = width;
            if (flags.minus) opts.align = "<";
            else if (flags.zero) opts.zero = true;
        }
        return pyFormatSpec(fv, opts, true);
    }
    throw new ValueError(
        `unsupported format character '${conv}' (0x${conv.codePointAt(0).toString(16)}) at index ${atIndex}`,
    );
}

/** Python `print(*args, sep=..., end=..., flush=...)` — the kwargs form.
 * The codegen lowers `print` WITH keyword arguments to this helper with the
 * kwargs object FIRST (unambiguous — a dict positional arg can never be
 * mistaken for kwargs). `file` is not supported (no sys.stdout object);
 * `flush` is accepted and ignored (console I/O is unbuffered). */
export function pyPrintKw(kw, ...args) {
    let sep = kw.sep === undefined || kw.sep === null ? " " : kw.sep;
    let end = kw.end === undefined || kw.end === null ? "\n" : kw.end;
    if (typeof sep !== "string") {
        throw new TypeError_(`sep must be None or a string, not ${__pyTypeName(sep)}`);
    }
    if (typeof end !== "string") {
        throw new TypeError_(`end must be None or a string, not ${__pyTypeName(end)}`);
    }
    const body = args.map(pyStr).join(sep);
    if (end === "\n") {
        console.log(body);
        return;
    }
    if (typeof process !== "undefined" && process.stdout
        && typeof process.stdout.write === "function") {
        process.stdout.write(body + end);
    } else {
        // Browser/worker hosts have no raw stdout; console.log appends a
        // newline — best-effort.
        console.log(body + end);
    }
}

// ============================================================
// List helpers (additional)
// ============================================================

/** Python `xs.sort(key=None, reverse=False)` — in-place stable sort. */
export function pyListSort(xs, opts = {}) {
    const { key, reverse = false } = opts;
    // #247: match pySorted — seq-aware lexicographic keys + stable reverse
    // (negate the comparator, never sort-then-reverse, so ties keep order).
    // F1 (v0.2.4): comparison routes through pyLt (see pySorted) — the
    // local raw-`<` copy bypassed the cross-type TypeError guard.
    const cmp = (a, b) => (pyLt(a, b) ? -1 : pyLt(b, a) ? 1 : 0);
    const dir = reverse ? -1 : 1;
    if (key) xs.sort((a, b) => dir * cmp(key(a), key(b)));
    else xs.sort((a, b) => dir * cmp(a, b));
}

// ============================================================
// React effect helper
// ============================================================

/** WF-1: wrap a use_effect/use_layout_effect/use_insertion_effect callback so
 * its return is safe as a React cleanup. React stores an effect callback's
 * return value as its cleanup ("destroy") and invokes it verbatim; it accepts
 * ONLY `undefined` or a function. A Python effect ending in `return None`
 * compiles to `return null`, and because `null !== undefined` React would call
 * it → "TypeError: destroy is not a function", crashing the component. Coerce
 * any non-function return (null, None, a number, …) to `undefined`; a real
 * cleanup function is passed through untouched, so unmount/re-run cleanup still
 * runs. Effect args (React passes none today) are forwarded for forward-compat. */
export function __pyEffect(fn) {
    return (...args) => {
        const cleanup = fn(...args);
        return typeof cleanup === "function" ? cleanup : undefined;
    };
}

/** WF-1 (spread form): argument-list splitter for a cleanup-wrapped hook
 * called with a SPREAD — `use_effect(*args)` / `use_sync_external_store(
 * *args)`. The compile-time wrap can't reach inside a spread (the old
 * emission wrapped the WHOLE spread — `useEffect(__pyEffect(...args))` —
 * which swallowed the deps array, so the effect re-ran every render). This
 * wraps ONLY the runtime-resolved FIRST argument (the effect/subscribe
 * callback) and passes the rest (deps / getSnapshot / …) through:
 * `useEffect(...__pyEffectArgs(...args))`. */
export function __pyEffectArgs(...a) {
    if (a.length > 0 && typeof a[0] === "function") a[0] = __pyEffect(a[0]);
    return a;
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
    const keys = __pyOwnKeys(d); // r6: a symbol-keyed last entry pops correctly
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

//# sourceMappingURL=runtime.js.map
