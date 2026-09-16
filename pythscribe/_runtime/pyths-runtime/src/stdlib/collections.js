// PythScribe standard library: collections module

import { KeyError, IndexError, ValueError, PyDict } from "../runtime.js";
import { pyRepr } from "../operators.js";

// Non-enumerable tuple marker (local twin of operators.js pyTuple).
function __tup(items) {
    Object.defineProperty(items, "__pytuple__", { value: true, enumerable: false });
    return items;
}

// Codegen kwargs convention: keyword args arrive as a single trailing
// plain-object literal.
function __isKwargs(x, ...keys) {
    return x !== null && typeof x === "object"
        && Object.getPrototypeOf(x) === Object.prototype
        && keys.some((k) => Object.prototype.hasOwnProperty.call(x, k));
}

// #277: extend PyDict (not native Map) so non-primitive keys — tuples,
// bool-vs-int folding — canonicalize like a plain `{}` dict. Native Map keys
// tuple arrays by identity, so `Counter()[(1,2)] += 1` never matched itself.
export class Counter extends PyDict {
    constructor(iterable) {
        super();
        if (iterable) this.update(iterable);
    }

    // pyGetItem dispatches __missing__ on Map subclasses: a missing
    // Counter key reads as 0 (and is NOT inserted), like CPython.
    __missing__() { return 0; }

    most_common(n) {
        // Stable sort by count desc — insertion order breaks ties (CPython).
        const sorted = [...this.entries()].sort((a, b) => b[1] - a[1])
            .map((pair) => __tup(pair));
        return (n === undefined || n === null) ? sorted : sorted.slice(0, n);
    }

    elements() {
        const self = this;
        return (function* () {
            // #277: PyDict's default iterator yields KEYS — use .entries().
            for (const [elem, count] of self.entries()) {
                for (let i = 0; i < count; i++) yield elem;
            }
        })();
    }

    update(iterable) {
        // Map/PyDict first: Maps are iterable, so the generic-iterable
        // branch would otherwise swallow them (and PyDict's default
        // iterator yields KEYS — use .entries() explicitly). CPython
        // Counter.update(mapping) adds COUNTS, not elements.
        if (iterable instanceof Map) {
            for (const [key, value] of iterable.entries()) {
                this.set(key, (this.get(key) || 0) + value);
            }
        } else if (typeof iterable === "string" || Array.isArray(iterable) || iterable[Symbol.iterator]) {
            for (const item of iterable) {
                this.set(item, (this.get(item) || 0) + 1);
            }
        } else if (typeof iterable === "object") {
            for (const [key, value] of Object.entries(iterable)) {
                this.set(key, (this.get(key) || 0) + value);
            }
        }
    }

    subtract(iterable) {
        if (iterable instanceof Map) {
            for (const [key, value] of iterable.entries()) {
                this.set(key, (this.get(key) || 0) - value);
            }
        } else if (typeof iterable === "string" || Array.isArray(iterable) || (iterable && iterable[Symbol.iterator])) {
            for (const item of iterable) {
                this.set(item, (this.get(item) || 0) - 1);
            }
        } else if (typeof iterable === "object" && iterable !== null) {
            for (const [key, value] of Object.entries(iterable)) {
                this.set(key, (this.get(key) || 0) - value);
            }
        }
    }

    total() {
        let sum = 0;
        for (const count of this.values()) sum += count;
        return sum;
    }

    copy() { return new Counter(this); }

    // Counter arithmetic (CPython: results keep only positive counts;
    // iteration order = self's keys first, then other-only keys).
    __add__(other) {
        const result = new Counter();
        for (const [k, v] of this.entries()) {
            const nv = v + (other.has(k) ? other.get(k) : 0);
            if (nv > 0) result.set(k, nv);
        }
        for (const [k, v] of other.entries()) {
            if (!this.has(k) && v > 0) result.set(k, v);
        }
        return result;
    }

    __sub__(other) {
        const result = new Counter();
        for (const [k, v] of this.entries()) {
            const nv = v - (other.has(k) ? other.get(k) : 0);
            if (nv > 0) result.set(k, nv);
        }
        for (const [k, v] of other.entries()) {
            if (!this.has(k) && v < 0) result.set(k, -v);
        }
        return result;
    }

    __and__(other) {
        const result = new Counter();
        for (const [k, v] of this.entries()) {
            const ov = other.has(k) ? other.get(k) : 0;
            const nv = v < ov ? v : ov;
            if (nv > 0) result.set(k, nv);
        }
        return result;
    }

    __or__(other) {
        const result = new Counter();
        for (const [k, v] of this.entries()) {
            const ov = other.has(k) ? other.get(k) : 0;
            const nv = v > ov ? v : ov;
            if (nv > 0) result.set(k, nv);
        }
        for (const [k, v] of other.entries()) {
            if (!this.has(k) && v > 0) result.set(k, v);
        }
        return result;
    }

    __repr__() {
        if (this.size === 0) return "Counter()";
        const parts = this.most_common().map(([k, v]) => `${pyRepr(k)}: ${pyRepr(v)}`);
        return `Counter({${parts.join(", ")}})`;
    }
}

class DefaultDict extends PyDict {
    constructor(default_factory, entries) {
        super();
        this.default_factory = default_factory || null;
        if (entries) {
            if (entries instanceof Map) {
                for (const [key, value] of entries.entries()) this.set(key, value);
            } else if (entries[Symbol.iterator]) {
                for (const [key, value] of entries) this.set(key, value);
            } else {
                for (const key of Object.keys(entries)) this.set(key, entries[key]);
            }
        }
    }

    // pyGetItem __missing__ protocol: d[missing] creates + returns the
    // default (CPython). NOTE: plain .get(missing) does NOT create —
    // dict.get semantics are preserved (pyDictGet uses has/get).
    __missing__(key) {
        if (this.default_factory === null) {
            throw new KeyError(typeof key === "string" ? `'${key}'` : String(key));
        }
        const value = this.default_factory();
        this.set(key, value);
        return value;
    }

    copy() { return new DefaultDict(this.default_factory, this); }

    // #257: CPython repr is `defaultdict(<factory>, {..})`, not a plain dict.
    // The factory's name is recovered by probing its result type (pure for the
    // builtin factories int/list/dict/set/str — the common case); a factory
    // with side effects would run once here (repr only). `float` reads as
    // `int` (a whole float is the same JS number — the documented D1 residual).
    __factoryName() {
        try {
            const v = this.default_factory();
            if (typeof v === "boolean") return "bool";
            if (typeof v === "number") return Number.isInteger(v) ? "int" : "float";
            if (typeof v === "string") return "str";
            if (Array.isArray(v)) return "list";
            if (v instanceof Set) return "set";
            if (v instanceof Map || (v && typeof v === "object")) return "dict";
        } catch (_e) { /* fall through */ }
        return "function";
    }
    __repr__() {
        const f = this.default_factory === null ? "None" : `<class '${this.__factoryName()}'>`;
        const parts = [];
        for (const [k, v] of this.entries()) parts.push(`${pyRepr(k)}: ${pyRepr(v)}`);
        return `defaultdict(${f}, {${parts.join(", ")}})`;
    }
}

// Python spells the constructor lowercase; the codegen's `new`-insertion
// heuristic is capitalization-based, so expose a plain factory function.
export function defaultdict(default_factory, entries) {
    return new DefaultDict(default_factory, entries);
}
defaultdict.class = DefaultDict;

class Deque {
    constructor(iterable, maxlen) {
        if (__isKwargs(maxlen, "maxlen")) maxlen = maxlen.maxlen;
        this._data = iterable ? [...iterable] : [];
        this.maxlen = maxlen === undefined || maxlen === null ? null : maxlen;
        if (this.maxlen !== null) {
            while (this._data.length > this.maxlen) this._data.shift();
        }
    }

    append(x) {
        this._data.push(x);
        if (this.maxlen !== null && this._data.length > this.maxlen) this._data.shift();
    }

    // The codegen lowers Python `.append(x)` to JS `.push(x)` and
    // `.extend(xs)` to `.push(...xs)` (list idiom) — alias push so deques
    // survive that lowering, including maxlen trimming.
    push(...items) {
        for (const x of items) this.append(x);
    }

    appendleft(x) {
        this._data.unshift(x);
        if (this.maxlen !== null && this._data.length > this.maxlen) this._data.pop();
    }

    pop() {
        if (this._data.length === 0) throw new IndexError("pop from an empty deque");
        return this._data.pop();
    }

    popleft() {
        if (this._data.length === 0) throw new IndexError("pop from an empty deque");
        return this._data.shift();
    }

    extend(iterable) {
        for (const item of iterable) this.append(item);
    }

    extendleft(iterable) {
        for (const item of iterable) this.appendleft(item);
    }

    rotate(n = 1) {
        if (this._data.length === 0) return;
        n = n % this._data.length;
        if (n > 0) {
            const tail = this._data.splice(-n);
            this._data.unshift(...tail);
        } else if (n < 0) {
            const head = this._data.splice(0, -n);
            this._data.push(...head);
        }
    }

    clear() { this._data = []; }

    count(x) {
        return this._data.filter(item => item === x).length;
    }

    index(x, start = 0, stop) {
        stop = stop === undefined ? this._data.length : stop;
        for (let i = start; i < stop; i++) {
            if (this._data[i] === x) return i;
        }
        throw new ValueError(`${pyRepr(x)} is not in deque`);
    }

    remove(x) {
        const idx = this._data.indexOf(x);
        if (idx === -1) throw new ValueError("deque.remove(x): x not in deque");
        this._data.splice(idx, 1);
    }

    reverse() { this._data.reverse(); }

    copy() { return new Deque(this._data, this.maxlen); }

    __getitem__(i) {
        const n = this._data.length;
        if (i < 0) i += n;
        if (i < 0 || i >= n) throw new IndexError("deque index out of range");
        return this._data[i];
    }

    __setitem__(i, v) {
        const n = this._data.length;
        if (i < 0) i += n;
        if (i < 0 || i >= n) throw new IndexError("deque index out of range");
        this._data[i] = v;
    }

    __contains__(x) { return this._data.includes(x); }

    // #271: `bool(deque())` / `if q:` — pyBool consults __len__, so an empty
    // deque was truthy without it (a deque is otherwise a generic object).
    __len__() { return this._data.length; }

    get length() { return this._data.length; }

    *[Symbol.iterator]() { yield* this._data; }

    __repr__() {
        const body = `[${this._data.map(pyRepr).join(", ")}]`;
        return this.maxlen !== null ? `deque(${body}, maxlen=${this.maxlen})` : `deque(${body})`;
    }
}

// Lowercase factory — see defaultdict note above.
export function deque(iterable, maxlen) {
    return new Deque(iterable, maxlen);
}
deque.class = Deque;

export class OrderedDict extends PyDict {
    constructor(entries) {
        super();
        if (entries) {
            if (entries instanceof Map) {
                for (const [key, value] of entries.entries()) this.set(key, value);
            } else if (entries[Symbol.iterator]) {
                for (const [key, value] of entries) this.set(key, value);
            } else {
                for (const key of Object.keys(entries)) this.set(key, entries[key]);
            }
        }
    }

    move_to_end(key, last = true) {
        if (__isKwargs(last, "last")) last = last.last;
        if (!this.has(key)) {
            throw new KeyError(typeof key === "string" ? `'${key}'` : String(key));
        }
        const value = this.get(key);
        this.delete(key);
        if (last) {
            this.set(key, value);
        } else {
            const entries = [...this.entries()];
            this.clear();
            this.set(key, value);
            for (const [k, v] of entries) this.set(k, v);
        }
    }

    popitem(last = true) {
        if (__isKwargs(last, "last")) last = last.last;
        if (this.size === 0) throw new KeyError("'dictionary is empty'");
        const entries = [...this.entries()];
        const [key, value] = last ? entries[entries.length - 1] : entries[0];
        this.delete(key);
        return __tup([key, value]);
    }

    copy() { return new OrderedDict(this); }

    __repr__() {
        // CPython 3.12 repr: OrderedDict({'b': 2, 'a': 1})
        if (this.size === 0) return "OrderedDict()";
        const parts = [];
        for (const [k, v] of this.entries()) parts.push(`${pyRepr(k)}: ${pyRepr(v)}`);
        return `OrderedDict({${parts.join(", ")}})`;
    }
}

// Generic accessors over a single underlying mapping. A ChainMap's maps may
// be plain-object dicts (`{'a': 1}` — string keys) or Map-backed PyDicts
// (non-string keys); handle both shapes uniformly.
function __cmHas(m, k) {
    return (m instanceof Map) ? m.has(k) : Object.prototype.hasOwnProperty.call(m, k);
}
function __cmGet(m, k) {
    return (m instanceof Map) ? m.get(k) : m[k];
}
function __cmKeys(m) {
    return (m instanceof Map) ? [...m.keys()] : Object.keys(m);
}

// #284: collections.ChainMap — a single view over multiple mappings. Lookups
// search the maps left-to-right (FIRST map wins for a value); mutations act on
// the first map only. CPython builds its key view by updating a fresh dict from
// `reversed(self.maps)` (`dict.fromkeys`), so a key's POSITION is set by the
// last (right-most) map that contains it while its VALUE comes from the first
// (left-most) — verified against CPython 3.12. Not a Map subclass: it is a view,
// not a dict, and mutation must land in maps[0], not a merged copy.
export class ChainMap {
    constructor(...maps) {
        // CPython: `ChainMap()` with no args wraps a single empty dict.
        this.maps = maps.length === 0 ? [{}] : maps;
    }

    __getitem__(key) {
        for (const m of this.maps) {
            if (__cmHas(m, key)) return __cmGet(m, key);
        }
        throw new KeyError(key);
    }

    get(key, defaultValue = null) {
        for (const m of this.maps) {
            if (__cmHas(m, key)) return __cmGet(m, key);
        }
        return defaultValue;
    }

    __contains__(key) {
        return this.maps.some((m) => __cmHas(m, key));
    }

    // Unique keys in CPython view order (right-most map first-seen — see class
    // note). Backs keys()/values()/items()/__iter__/__len__.
    __keyList() {
        const seen = new Set();
        const order = [];
        for (let i = this.maps.length - 1; i >= 0; i--) {
            for (const k of __cmKeys(this.maps[i])) {
                if (!seen.has(k)) { seen.add(k); order.push(k); }
            }
        }
        return order;
    }

    keys() { return this.__keyList(); }
    values() { return this.__keyList().map((k) => this.__getitem__(k)); }
    items() { return this.__keyList().map((k) => __tup([k, this.__getitem__(k)])); }
    // pyDictItems lowering probes `.entries()`; mirror items() so `dict()` and
    // `.items()` both yield tuple-marked pairs regardless of dispatch path.
    entries() { return this.items(); }

    __len__() { return this.__keyList().length; }

    __iter__() { return this[Symbol.iterator](); }
    *[Symbol.iterator]() { yield* this.__keyList(); }

    // Mutating ops target the FIRST map only (CPython).
    __setitem__(key, value) {
        const m = this.maps[0];
        if (m instanceof Map) m.set(key, value); else m[key] = value;
    }
    __delitem__(key) {
        const m = this.maps[0];
        if (__cmHas(m, key)) {
            if (m instanceof Map) m.delete(key); else delete m[key];
            return;
        }
        throw new KeyError(`Key not found in the first mapping: ${pyRepr(key)}`);
    }

    // new_child(m=None): new ChainMap with `m` (or a fresh empty dict) prepended.
    new_child(m) {
        return new ChainMap(m === undefined || m === null ? {} : m, ...this.maps);
    }
    // parents: a ChainMap over maps[1:] (skips the first map).
    get parents() {
        return new ChainMap(...this.maps.slice(1));
    }

    __repr__() {
        return `ChainMap(${this.maps.map(pyRepr).join(", ")})`;
    }
    toString() { return this.__repr__(); }
}

export function namedtuple(name, fields) {
    if (typeof fields === "string") {
        fields = fields.split(/[\s,]+/).filter(Boolean);
    }
    class NT extends Array {
        constructor(...args) {
            super(...args);
            for (let i = 0; i < fields.length; i++) {
                Object.defineProperty(this, fields[i], {
                    get: () => this[i],
                    set: (v) => { this[i] = v; },
                    enumerable: false,
                });
            }
        }
        __repr__() {
            const parts = fields.map((f, i) => `${f}=${pyRepr(this[i])}`).join(", ");
            return `${name}(${parts})`;
        }
        toString() { return this.__repr__(); }
        _asdict() {
            const obj = {};
            fields.forEach((f, i) => { obj[f] = this[i]; });
            return obj;
        }
        _replace(kwargs) {
            const values = [...this];
            for (const [key, value] of Object.entries(kwargs)) {
                const idx = fields.indexOf(key);
                if (idx === -1) throw new ValueError(`Got unexpected field names: ['${key}']`);
                values[idx] = value;
            }
            return new NT(...values);
        }
    }
    NT._fields = __tup([...fields]);
    NT._name = name;
    return NT;
}

//# sourceMappingURL=collections.js.map
