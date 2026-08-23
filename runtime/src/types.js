// PythScribe Runtime — Type classes
// Implements Python-compatible container types.

import { __pyOwnKeys, __pyBytesKind } from "./runtime.js"; // r6: symbol-aware dict emptiness; bytes authority

/**
 * Python truthiness check.
 * Empty containers, 0, null, undefined, empty string, false → falsy.
 */
export function pyBool(x) {
    if (x == null) return false;
    if (typeof x === "boolean") return x;
    if (typeof x === "number") return x !== 0;
    if (typeof x === "string") return x.length > 0;
    if (Array.isArray(x)) return x.length > 0;
    if (x instanceof Set || x instanceof Map) return x.size > 0;
    // #457: bytes/bytearray truthiness is emptiness — routed through the
    // bytes dispatch authority (bool(b"") is False, like CPython).
    if (__pyBytesKind(x) !== null) return x.length > 0;
    if (typeof x.__bool__ === "function") return x.__bool__();
    if (typeof x.__len__ === "function") return x.__len__() > 0;
    // Plain dict literal: `{}` is truthy in JS but falsy in Python.
    if (typeof x === "object" && Object.getPrototypeOf(x) === Object.prototype) {
        return __pyOwnKeys(x).length > 0; // r6: a symbol-only dict is truthy
    }
    return true;
}

// #348: any()/all() must consume the iterable LAZILY and short-circuit on the
// first truthy/falsy element — Python semantics. The old codegen lowered these
// to `[...iter].some/.every`, which fully materialises the iterable first: that
// breaks short-circuit side-effect ordering, loses early-exit asymptotics
// (O(n²) product on Mbpp/414 vs CPython's O(1) exit), and OOMs on large or
// unbounded generators (HumanEval/0 spreads a ~10^12-element nested-for
// generator). Mirrors the lazy pyNext/pySum treatment (the #155 pattern).
// Truthiness is Python truthiness (pyBool) — `any([[]])` is False, not JS's
// truthy empty array.
export function pyAny(iterable) {
    for (const item of iterable) {
        if (pyBool(item)) return true;
    }
    return false;
}
export function pyAll(iterable) {
    for (const item of iterable) {
        if (!pyBool(item)) return false;
    }
    return true;
}

// #273: Python `and`/`or` short-circuit on PYTHON truthiness and return the
// deciding OPERAND (not a coerced bool). Raw JS `a && b` diverges when `a` is
// Python-falsy but JS-truthy — an empty list/dict/set/deque or any `__len__`/
// `__bool__` object (`while stack and stack[-1]: ...` would evaluate `stack[-1]`
// on an empty stack). The right operand is a thunk so it stays lazily evaluated.
export function pyAnd(a, b) { return pyBool(a) ? b() : a; }
export function pyOr(a, b) { return pyBool(a) ? a : b(); }

// Python dict — Map-backed with CPython key canonicalization (#83).
// The real implementation lives in runtime.js (it needs the Python
// exception classes, which would be an import cycle from here);
// re-exported under the historical name for back-compat.
export { PyDict } from "./runtime.js";

// #297: Python set — canonicalizing Set subclass (bool/int/float hash
// identity, structural tuple membership, first-inserted-original repr).
// The real implementation lives in runtime.js (shares PyDict's __pyKey
// infrastructure); re-exported under the historical name, replacing the
// old non-canonicalizing wrapper class.
export { PySet } from "./runtime.js";

export class PyTuple {
    constructor(iterable) {
        this._items = Object.freeze([...(iterable || [])]);
    }

    __getitem__(index) {
        if (index < 0) index = this._items.length + index;
        if (index < 0 || index >= this._items.length) {
            throw new Error("IndexError: tuple index out of range");
        }
        return this._items[index];
    }

    __contains__(item) {
        return this._items.includes(item);
    }

    __len__() {
        return this._items.length;
    }

    __bool__() {
        return this._items.length > 0;
    }

    get length() {
        return this._items.length;
    }

    [Symbol.iterator]() {
        return this._items[Symbol.iterator]();
    }

    toString() {
        const items = this._items.map(x => JSON.stringify(x)).join(", ");
        return `(${items})`;
    }
}

//# sourceMappingURL=types.js.map
