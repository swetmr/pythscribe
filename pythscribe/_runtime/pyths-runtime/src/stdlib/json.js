// PythScribe standard library: json module

import { pyFormatFloat } from "../operators.js";

// CPython json key coercion: int/float/bool/None keys serialize as their
// Python string forms (`{1: 'a'}` → `{"1": "a"}`, True → "true",
// None → "null"); anything else raises TypeError. Applies to Map-backed
// dicts (#83) — plain-object dicts already hold string keys.
function __jsonKey(k) {
    if (typeof k === "string") return k;
    if (typeof k === "boolean") return k ? "true" : "false";
    if (typeof k === "bigint") return k.toString();
    // Option B: a boxed (integer-valued) float key serializes with its
    // float repr — CPython json.dumps({8.0: 'a'}) → '{"8.0": "a"}'.
    if (k != null && k.__pyfloat__ === true) return pyFormatFloat(k.valueOf());
    if (typeof k === "number") {
        if (Number.isInteger(k) && Math.abs(k) <= Number.MAX_SAFE_INTEGER) return String(k);
        return pyFormatFloat(k);
    }
    if (k === null || k === undefined) return "null";
    const name = Array.isArray(k) ? "tuple" : (k instanceof Map ? "dict" : (k instanceof Set ? "set" : typeof k));
    throw new TypeError(`keys must be str, int, float, bool or None, not ${name}`);
}

// Private-use sentinel (U+E000) — not escaped by JSON.stringify and absent
// from normal data, so a tagged BigInt round-trips to bare digits without
// risk of stripping the quotes off a legitimate all-digit string value.
const __BIGINT_TAG = "";
// Option B: the same sentinel also carries boxed-float reprs ("8.0",
// "1e+300"), so the pattern admits a fraction/exponent tail.
const __BIGINT_RE = /"(-?(?:\d+(?:\.\d+)?(?:e[+-]?\d+)?))"/g;

const __typeErr = (msg) => { const e = new Error(msg); e.name = "TypeError"; return e; };

// #299: a value CPython's encoder cannot serialize natively - anything that
// is not str/int/float/bool/None/list/tuple/dict. In JS terms: an object
// that is neither an Array (list/tuple), a Map (PyDict), nor a plain-proto
// object (plain dict). Sets, datetimes, class instances all route through
// the encoder's default() hook.
function __needsDefault(v) {
    if (v === null || typeof v !== "object") return false;
    if (Array.isArray(v) || v instanceof Map) return false;
    const p = Object.getPrototypeOf(v);
    return p !== Object.prototype && p !== null;
}

/**
 * #299: json.JSONEncoder - subclassable with an overridable `default(obj)`
 * (BigCodeBench/464 shape: `class MyEncoder(json.JSONEncoder)` consumed by
 * `json.dumps(x, cls=MyEncoder)`). The base default() raises TypeError like
 * CPython; encode() serializes with this encoder. Minimal faithful subset -
 * the constructor accepts CPython's keyword options object and honors
 * `default=` / `indent=` / `sort_keys=`.
 */
export class JSONEncoder {
    constructor(kw) {
        if (kw != null && typeof kw === "object") {
            if (typeof kw.default === "function") this.default = kw.default;
            if (kw.indent !== undefined) this.indent = kw.indent;
            if (kw.sort_keys !== undefined) this.sort_keys = kw.sort_keys;
        }
    }
    default(o) {
        const name =
            o === null || o === undefined ? "NoneType"
            : (o.constructor && (o.constructor.__name__ ?? o.constructor.name)) || typeof o;
        throw __typeErr(`Object of type ${name} is not JSON serializable`);
    }
    encode(o) {
        return __dumps(o, { indent: this.indent, sort_keys: this.sort_keys ?? false }, this);
    }
}

function __dumps(obj, { indent, sort_keys = false, separators } = {}, enc) {
    const replacer = function (key, value) {
        // #299: with an encoder in play, decide on the RAW holder value -
        // a toJSON() transform must not preempt the Python default() hook
        // (CPython has no toJSON; default() is the only escape).
        if (enc) {
            const raw = this === undefined ? value : this[key];
            if (__needsDefault(raw)) {
                // CPython calls default() once per object; the substitute is
                // serialized normally (stringify recurses into it, so nested
                // Maps/BigInts inside still hit this replacer). A substitute
                // that is itself unserializable raises from the recursive
                // replacer call on ITS holder - matching CPython's TypeError.
                value = enc.default(raw);
                if (typeof value !== "bigint" && !(value instanceof Map)) return value;
            }
        }
        // Arbitrary-precision ints arrive as BigInt; JSON.stringify throws
        // on BigInt, and Python emits them as unquoted integer literals.
        if (typeof value === "bigint") {
            return __BIGINT_TAG + value.toString() + __BIGINT_TAG;
        }
        // Option B: a boxed (integer-valued) float must serialize with its
        // Python float repr — CPython json.dumps(8.0) → "8.0", not "8".
        // JSON.stringify would ToNumber the box to bare digits, so tag its
        // repr through the same sentinel channel as BigInt.
        if (value != null && value.__pyfloat__ === true) {
            return __BIGINT_TAG + pyFormatFloat(value.valueOf()) + __BIGINT_TAG;
        }
        // Map-backed dicts (#83) — coerce keys the CPython way, then let
        // JSON.stringify recurse into the resulting (null-proto) object.
        // Explicit .entries(): PyDict's default iterator yields KEYS.
        if (value instanceof Map) {
            const out = Object.create(null);
            for (const [k, v] of value.entries()) out[__jsonKey(k)] = v;
            if (sort_keys) {
                const sorted = Object.create(null);
                for (const k of Object.keys(out).sort()) sorted[k] = out[k];
                return sorted;
            }
            return out;
        }
        // Recursive key sort (the old top-level-only allowlist was lossy).
        if (sort_keys && value && typeof value === "object" && !Array.isArray(value)) {
            const sorted = {};
            for (const k of Object.keys(value).sort()) sorted[k] = value[k];
            return sorted;
        }
        return value;
    };
    // #299/BCB-464: CPython's default separators are (", ", ": ") — compact
    // JSON.stringify output ({"a":1}) diverges byte-for-byte. Serialize in
    // indent mode (which gives the `": "` key separator for free) and
    // collapse the formatting newlines: raw newlines can NEVER occur inside
    // JSON string content (stringify escapes them as \n), so the collapse is
    // safe. `separators=(",", ":")` (the common compact form) keeps the raw
    // compact stringify; other custom separators fall back to the default
    // formatting (documented minimal subset).
    const compactPy = indent === undefined || indent === null;
    const wantCompactSeps =
        separators != null && separators[0] === "," && separators[1] === ":";
    let json = JSON.stringify(
        obj,
        replacer,
        compactPy ? (wantCompactSeps ? undefined : 1) : indent
    );
    if (json === undefined) return json;
    if (compactPy && !wantCompactSeps) {
        json = json.replace(/,\n\s*/g, ", ").replace(/\n\s*/g, "");
    }
    // Unwrap tagged BigInts → unquoted integer literals.
    return json.replace(__BIGINT_RE, "$1");
}

export function dumps(obj, kw = {}) {
    const { cls, default: dflt } = kw;
    let enc = null;
    // #299: `cls=` names an encoder class - instantiate it and use its
    // default() for non-serializable objects (CPython semantics). A bare
    // `default=` callable wraps into a base encoder the way CPython's
    // dumps(default=fn) does.
    if (cls != null) enc = new cls(dflt !== undefined ? { default: dflt } : undefined);
    else if (typeof dflt === "function") enc = new JSONEncoder({ default: dflt });
    return __dumps(obj, kw, enc);
}

export function loads(s) {
    return JSON.parse(s);
}

export function dump(obj, file) {
    throw new Error("json.dump() requires file I/O — use json.dumps() in browser");
}

export function load(file) {
    throw new Error("json.load() requires file I/O — use json.loads() in browser");
}

//# sourceMappingURL=json.js.map
