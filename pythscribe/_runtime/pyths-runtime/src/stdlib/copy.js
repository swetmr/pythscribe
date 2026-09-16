// PythScribe stdlib — Python `copy` module.
//
// `copy(x)` is a shallow copy; `deepcopy(x)` recursively copies, sharing
// structure only where Python does (via a memo that also makes cyclic
// structures terminate). Primitives / strings / immutables are returned as-is.
// Tuples are arrays carrying a non-enumerable `__pytuple__` marker; it is
// preserved so a copied tuple still reports as a tuple.

function markTuple(src, dst) {
    if (Array.isArray(src) && src.__pytuple__) {
        Object.defineProperty(dst, "__pytuple__", { value: true, enumerable: false });
    }
    return dst;
}

export function copy(x) {
    if (Array.isArray(x)) return markTuple(x, x.slice());
    if (x instanceof Set) return new Set(x);
    if (x instanceof Map) return new Map(x);
    if (x !== null && typeof x === "object" && Object.getPrototypeOf(x) === Object.prototype) {
        return { ...x };
    }
    return x;
}

export function deepcopy(x, memo) {
    if (x === null || typeof x !== "object") return x;
    memo = memo || new Map();
    if (memo.has(x)) return memo.get(x);
    if (Array.isArray(x)) {
        const r = [];
        markTuple(x, r);
        memo.set(x, r);
        for (const v of x) r.push(deepcopy(v, memo));
        return r;
    }
    if (x instanceof Set) {
        const r = new Set();
        memo.set(x, r);
        for (const v of x) r.add(deepcopy(v, memo));
        return r;
    }
    if (x instanceof Map) {
        const r = new Map();
        memo.set(x, r);
        for (const [k, v] of x) r.set(deepcopy(k, memo), deepcopy(v, memo));
        return r;
    }
    if (Object.getPrototypeOf(x) === Object.prototype) {
        const r = {};
        memo.set(x, r);
        for (const k of Object.keys(x)) r[k] = deepcopy(x[k], memo);
        return r;
    }
    // Class instances / exotic objects: best-effort passthrough (Python would
    // dispatch __deepcopy__/__copy__; not modeled here).
    return x;
}

//# sourceMappingURL=copy.js.map
