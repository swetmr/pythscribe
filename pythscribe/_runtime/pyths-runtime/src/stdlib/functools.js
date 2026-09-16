// PythScribe standard library: functools module

// #478: partial must honor keyword arguments (both construction-time and
// call-time). Keywords travel through the codegen's marked kwargs carrier;
// __pyTakeKw pops it, __pyCallKw re-binds them against the wrapped callable.
import { __pyCallKw, __pyTakeKw } from "../runtime.js";

export function reduce(func, iterable, initializer) {
    const iter = iterable[Symbol.iterator]();
    let accumulator;
    if (initializer !== undefined) {
        accumulator = initializer;
    } else {
        const first = iter.next();
        if (first.done) throw new TypeError("reduce() of empty iterable with no initial value");
        accumulator = first.value;
    }
    for (const item of { [Symbol.iterator]: () => iter }) {
        accumulator = func(accumulator, item);
    }
    return accumulator;
}

export function partial(func, ...args) {
    // functools.partial(func, /, *args, **keywords): capture construction-time
    // keywords (arriving via the marked kwargs carrier — see partial's own
    // metadata below), leaving only the frozen positional prefix in `args`.
    const keywords = __pyTakeKw(args);
    const hasCtorKw = Object.keys(keywords).length > 0;
    const p = function (...moreArgs) {
        // Call-time keywords override construction-time ones, exactly like
        // CPython's `func(*args, *fargs, **{**keywords, **fkeywords})`.
        const callKw = __pyTakeKw(moreArgs);
        const allPos = [...args, ...moreArgs];
        // Fast/interop path: no keywords anywhere → call positionally,
        // preserving the original behavior (a metadata-less JS callee must
        // not receive a spurious trailing options object).
        if (!hasCtorKw && Object.keys(callKw).length === 0) {
            return func(...allPos);
        }
        // Re-bind keywords against the wrapped callable's parameters.
        return __pyCallKw(func, allPos, { ...keywords, ...callKw });
    };
    // Route BOTH construction and call keywords through the marked variadic
    // kwargs carrier rather than the legacy trailing-object shape.
    p.__pyparams__ = [];
    p.__pyva__ = true;
    p.__pykw__ = true;
    return p;
}
// partial(func, *args, **keywords): declare `func` positional + a variadic
// keyword channel so its OWN construction keywords route through the marked
// carrier (__pyTakeKw above) instead of the legacy trailing-object shape.
partial.__pyparams__ = ["func"];
partial.__pyva__ = true;
partial.__pykw__ = true;

export function lru_cache(maxsize = 128) {
    return function (func) {
        const cache = new Map();
        function wrapper(...args) {
            const key = JSON.stringify(args);
            if (cache.has(key)) {
                const value = cache.get(key);
                cache.delete(key);
                cache.set(key, value);
                return value;
            }
            const result = func(...args);
            cache.set(key, result);
            if (maxsize !== null && cache.size > maxsize) {
                const oldest = cache.keys().next().value;
                cache.delete(oldest);
            }
            return result;
        }
        wrapper.cache_info = () => ({ maxsize, currsize: cache.size });
        wrapper.cache_clear = () => cache.clear();
        return wrapper;
    };
}

export function cache(func) {
    return lru_cache(null)(func);
}

export function wraps(wrapped) {
    return function (wrapper) {
        wrapper.__wrapped__ = wrapped;
        wrapper.__name__ = wrapped.name;
        return wrapper;
    };
}

export function cmp_to_key(mycmp) {
    // #264: the key object must compare via `mycmp`, not its wrapped value.
    // pySorted/pyLt dispatch the __lt__ etc. dunders, so define them (the old
    // Symbol.toPrimitive/valueOf fallback silently sorted by the raw value and
    // ignored a non-trivial comparator).
    class K {
        constructor(obj) { this.obj = obj; }
        __lt__(other) { return mycmp(this.obj, other.obj) < 0; }
        __le__(other) { return mycmp(this.obj, other.obj) <= 0; }
        __gt__(other) { return mycmp(this.obj, other.obj) > 0; }
        __ge__(other) { return mycmp(this.obj, other.obj) >= 0; }
        __eq__(other) { return mycmp(this.obj, other.obj) === 0; }
    }
    return (obj) => new K(obj);
}

export function total_ordering(cls) {
    const methods = Object.getOwnPropertyNames(cls.prototype);
    if (methods.includes("__lt__")) {
        if (!methods.includes("__le__"))
            cls.prototype.__le__ = function (other) { return this.__lt__(other) || this.__eq__(other); };
        if (!methods.includes("__gt__"))
            cls.prototype.__gt__ = function (other) { return !this.__lt__(other) && !this.__eq__(other); };
        if (!methods.includes("__ge__"))
            cls.prototype.__ge__ = function (other) { return !this.__lt__(other); };
    } else if (methods.includes("__gt__")) {
        if (!methods.includes("__ge__"))
            cls.prototype.__ge__ = function (other) { return this.__gt__(other) || this.__eq__(other); };
        if (!methods.includes("__lt__"))
            cls.prototype.__lt__ = function (other) { return !this.__gt__(other) && !this.__eq__(other); };
        if (!methods.includes("__le__"))
            cls.prototype.__le__ = function (other) { return !this.__gt__(other); };
    }
    return cls;
}

//# sourceMappingURL=functools.js.map
