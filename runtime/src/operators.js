// PythScribe Runtime — Operator dispatch
// Implements Python-compatible operator overloading via dunder methods.

import {
    ZeroDivisionError, ValueError, OverflowError, TypeError as TypeError_,
    NotImplementedError,
    pySetUnion, pySetIntersection, pySetDifference, pySetSymmetricDifference,
    pyDictMerge, pySeq,
} from "./runtime.js";

// #322: CPython type name of an arithmetic operand, for the
// "unsupported operand type(s)" TypeError message.
function __opTypeName(v) {
    if (v === null || v === undefined) return "NoneType";
    switch (typeof v) {
        case "boolean": return "bool";
        case "number": return Number.isInteger(v) ? "int" : "float";
        case "bigint": return "int";
        case "string": return "str";
    }
    if (Array.isArray(v)) return v.__pytuple__ ? "tuple" : "list";
    if (v instanceof Set) return "set";
    if (v instanceof Map) return "dict";
    const c = v && v.constructor;
    return (c && (c.__name__ || c.name)) || "object";
}

// #322: `None + x` (typically a defaulted `dict.get(missing)` feeding
// arithmetic) must raise TypeError like CPython — not coerce to NaN via
// JS `undefined + 1`. Called only on the SLOW path (after the numeric
// fast path and dunder dispatch), so the hot path is untouched and a
// custom class whose `__add__`/etc. accepts None still wins.
function __arithNoneGuard(op, a, b) {
    if (a == null || b == null) {
        throw new TypeError_(
            `unsupported operand type(s) for ${op}: '${__opTypeName(a)}' and '${__opTypeName(b)}'`,
        );
    }
}

// Wave-15 F9 (cat-C, NARROWED): the `-`/`/`/`//`/`%`/`**` slow paths must
// accept ONLY numeric operands — int/float/bool (bool ⊂ int), i.e. JS
// number/bigint/boolean. The old fallbacks coerced numeric STRINGS and
// singleton ARRAYS through Number()/BigInt() (`"4" // 2` → 2, `[4] // 2` →
// 2, `"4" - 2` → 2), where CPython raises TypeError. Called AFTER dunder
// dispatch and after the legitimate non-numeric overloads (set ops), so
// user classes and valid Python surfaces are untouched. Subsumes
// __arithNoneGuard on these paths (__opTypeName(None) is 'NoneType').
// Mirrors the #320/#343 __reqBitInt discipline for the bitwise family.
// SCOPE (wave-15 narrow): `+` (pyAdd) and `*` (pyMul) are NOT guarded —
// their remaining type gaps (string/list coercion on `+`, non-int
// replication counts on `*`, reflected dunders, printf-style `%`) are the
// arithmetic-operator type-safety workstream (C3/C4), not wave 15.
const __arithNumOk = (x) =>
    typeof x === "number" || typeof x === "bigint" || typeof x === "boolean";
function __reqArithNum(op, a, b) {
    if (!__arithNumOk(a) || !__arithNumOk(b)) {
        throw new TypeError_(
            `unsupported operand type(s) for ${op}: '${__opTypeName(a)}' and '${__opTypeName(b)}'`,
        );
    }
}
// Wave-15 F4: bool ⊂ int — when the OTHER operand is also numeric/bool,
// coerce a bool operand to its int value (True→1, False→0) BEFORE any
// arithmetic, so bool-with-BigInt takes the exact __intBin path
// (`True + 9007199254740993` must be 9007199254740994 — raw JS would
// throw "Cannot mix BigInt" on `+` and round through Number() on the
// other slow paths). Non-numeric partners are untouched, so dunder
// dispatch and the (deferred, workstream) str/list behavior of `+`/`*`
// are exactly as before.
const __boolNum = (x) => (x ? 1 : 0);

/**
 * F4: Python-compatible float() conversion.
 *
 * `Number("inf")` yields NaN, but Python's `float("inf")` is real Infinity.
 * Python accepts `inf`, `infinity`, `nan` (case-insensitive) with an optional
 * sign and surrounding whitespace, and raises ValueError on any other
 * non-numeric string (JS `Number("")`/`Number("0x10")` would silently return
 * 0/16). Numeric inputs pass straight through `Number`.
 */
export function pyFloat(x) {
    if (typeof x === "boolean") return x ? 1 : 0;
    if (typeof x === "number") return x;
    // #319: float(huge_int) overflows to OverflowError in CPython, not inf.
    if (typeof x === "bigint") return __reqNum(x);
    if (typeof x === "string") {
        const t = x.trim();
        const m = /^([+-]?)(inf|infinity|nan)$/i.exec(t);
        if (m) {
            if (m[2].toLowerCase() === "nan") return NaN;
            return m[1] === "-" ? -Infinity : Infinity;
        }
        // #342: PEP 515 underscore digit separators — allowed only BETWEEN two
        // digits (no leading/trailing/double, none adjacent to '.'/'e'/sign).
        // Validate, then strip before Number().
        let t2 = t;
        if (t.indexOf("_") !== -1) {
            const isDig = (c) => c >= 48 && c <= 57;
            for (let i = 0; i < t.length; i++) {
                if (t.charCodeAt(i) === 95 /* _ */
                    && !(isDig(t.charCodeAt(i - 1)) && isDig(t.charCodeAt(i + 1)))) {
                    throw new ValueError(`could not convert string to float: ${pyRepr(x)}`);
                }
            }
            t2 = t.replace(/_/g, "");
        }
        // Reject empty / non-numeric strings the way CPython does, rather
        // than inheriting JS's lenient Number() coercions.
        if (t2 === "" || !/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(t2)) {
            throw new ValueError(`could not convert string to float: ${pyRepr(x)}`);
        }
        return Number(t2);
    }
    return Number(x);
}

/**
 * Python-compatible int() conversion (issue #82).
 *
 * `Math.trunc(Number("abc"))` silently returns NaN; CPython raises
 * `ValueError: invalid literal for int() with base 10: 'abc'`. String
 * validation follows CPython: surrounding whitespace and a sign are
 * allowed, underscores are allowed BETWEEN digits, everything else is a
 * ValueError. Large numeric strings stay exact (BigInt-backed) and are
 * demoted to Number when they fit the safe-integer range — matching the
 * hybrid int representation used everywhere else. Floats truncate toward
 * zero; NaN/Infinity raise like CPython.
 */
export function pyInt(x, base) {
    if (typeof base === "object" && base !== null) base = base.base;
    if (typeof x === "boolean") return x ? 1 : 0;
    if (typeof x === "bigint") return x;
    if (typeof x === "number") {
        if (Number.isNaN(x)) throw new ValueError("cannot convert float NaN to integer");
        if (!Number.isFinite(x)) throw new OverflowError("cannot convert float infinity to integer");
        return Math.trunc(x);
    }
    if (typeof x === "string" || x instanceof String) {
        const b = base == null ? 10 : Number(base);
        const t = x.trim();
        const m = /^([+-]?)([0-9a-zA-Z_]+)$/.exec(t);
        const bad = () => new ValueError(
            `invalid literal for int() with base ${b}: ${pyRepr(String(x))}`);
        if (!m) throw bad();
        const body = m[2];
        // Underscores only BETWEEN digits (no leading/trailing/double).
        if (/^_|_$|__/.test(body)) throw bad();
        const digits = body.replace(/_/g, "");
        const digitRe = b === 10 ? /^[0-9]+$/
            : b === 16 ? /^[0-9a-fA-F]+$/
            : b === 8 ? /^[0-7]+$/
            : b === 2 ? /^[01]+$/
            : null;
        if (digitRe) {
            if (!digitRe.test(digits)) throw bad();
            const prefix = b === 16 ? "0x" : b === 8 ? "0o" : b === 2 ? "0b" : "";
            const big = BigInt((m[1] === "-" ? "-" : "") + prefix + digits);
            return big >= -9007199254740991n && big <= 9007199254740991n ? Number(big) : big;
        }
        // Uncommon bases: parseInt with per-digit validation.
        const v = parseInt((m[1] === "-" ? "-" : "") + digits, b);
        if (Number.isNaN(v) || digits.split("").some(d => Number.isNaN(parseInt(d, b)))) throw bad();
        return v;
    }
    if (x != null && typeof x.__int__ === "function") return x.__int__();
    if (x == null) throw new TypeError("int() argument must be a string, a bytes-like object or a real number, not 'NoneType'");
    return Math.trunc(Number(x));
}

/**
 * Python `divmod(a, b)` → tuple `(a // b, a % b)` with CPython's
 * floor-division/sign-of-divisor semantics and error messages
 * (issue #90).
 */
export function pyDivmod(a, b) {
    if ((__isFloat(a) || __isFloat(b)) && Number(b) === 0) {
        throw new ZeroDivisionError("float divmod()");
    }
    return pyTuple(pyFloorDiv(a, b), pyMod(a, b));
}

/**
 * Python `sum(iterable, start=0)` — honors the positional AND keyword
 * `start` forms (issue #94; previously both were silently discarded).
 * Uses pyAdd so BigInt-promoted ints stay exact and list-start sums
 * (`sum(xss, [])`) concatenate like CPython.
 */
export function pySum(iterable, startArg) {
    let start = 0;
    if (startArg !== undefined) {
        // Keyword form arrives as the codegen's trailing options object
        // `{start: v}`; positional form arrives bare. A plain object with
        // an own `start` key is the kwargs wrapper (a dict start value is
        // not summable in Python anyway).
        if (startArg !== null && typeof startArg === "object"
            && Object.getPrototypeOf(startArg) === Object.prototype
            && Object.prototype.hasOwnProperty.call(startArg, "start")) {
            start = startArg.start;
        } else {
            start = startArg;
        }
    }
    let acc = start;
    for (const item of iterable) acc = pyAdd(acc, item);
    return acc;
}

/**
 * Python `|` — set union, dict merge (PEP 584), or int bitwise-or
 * (BigInt-aware). Issue #93: sets previously fell into JS numeric
 * coercion (`Number(Set)` → NaN → 0).
 */
// JS bitwise ops truncate Numbers to 32 bits; Python ints are arbitrary
// precision. Fast-path the in-range case, BigInt everything else.
const __fits32 = (x) =>
    typeof x === "number" && Number.isInteger(x) && Math.abs(x) <= 0x7fffffff;

// #320: a bitwise operator requires int operands. A non-integer number (or any
// non-int value with no dunder) must raise TypeError like CPython, not leak the
// JS RangeError from BigInt(0.2) / SyntaxError from BigInt("x"). bool ⊆ int.
const __bitIntOk = (x) =>
    typeof x === "bigint" || typeof x === "boolean"
    || (typeof x === "number" && Number.isInteger(x));
// #343: `fctx` is a codegen bitmask (1=left operand statically float, 2=right,
// 3=both). A whole-valued float (`3.0`) is the same JS number as int `3` at
// runtime, so __bitIntOk can't reject it — CPython rejects ANY float in a
// bitwise position with TypeError. When fctx marks an operand float, raise and
// report its type as 'float' regardless of value.
function __reqBitInt(op, a, b, fctx) {
    fctx = fctx || 0;
    if (fctx || !__bitIntOk(a) || !__bitIntOk(b)) {
        const an = (fctx & 1) ? "float" : __opTypeName(a);
        const bn = (fctx & 2) ? "float" : __opTypeName(b);
        throw new TypeError_(
            `unsupported operand type(s) for ${op}: '${an}' and '${bn}'`,
        );
    }
}

export function pyBitOr(a, b, fctx) {
    if (a instanceof Set && b instanceof Set) return pySetUnion(a, b);
    if (a != null && typeof a.__or__ === "function") return a.__or__(b);
    // Dict merge `d | e` — both shapes, including mixed (#83). Result is
    // PyDict when either side is Map-backed, else a plain object.
    if ((a instanceof Map || __isPlainObject(a)) && (b instanceof Map || __isPlainObject(b))) {
        return pyDictMerge(a, b);
    }
    __reqBitInt("|", a, b, fctx); // #320/#343
    if (__fits32(a) && __fits32(b)) return a | b;
    return __norm(__toBig(a) | __toBig(b));
}

/** Python `&` — set intersection or int bitwise-and (issue #93). */
export function pyBitAnd(a, b, fctx) {
    if (a instanceof Set && b instanceof Set) return pySetIntersection(a, b);
    if (a != null && typeof a.__and__ === "function") return a.__and__(b);
    __reqBitInt("&", a, b, fctx); // #320/#343
    if (__fits32(a) && __fits32(b)) return a & b;
    return __norm(__toBig(a) & __toBig(b));
}

/** Python `^` — set symmetric difference or int bitwise-xor (issue #93). */
export function pyBitXor(a, b, fctx) {
    if (a instanceof Set && b instanceof Set) return pySetSymmetricDifference(a, b);
    if (a != null && typeof a.__xor__ === "function") return a.__xor__(b);
    __reqBitInt("^", a, b, fctx); // #320/#343
    if (__fits32(a) && __fits32(b)) return a ^ b;
    return __norm(__toBig(a) ^ __toBig(b));
}

/**
 * Python unary `~` — arbitrary-precision bitwise inversion, `~x == -x - 1`
 * on the infinite two's-complement view (wave-14 F9). Raw JS `~` does
 * ToInt32, so `~(2**40)` compiled to `-1` instead of CPython's
 * `-1099511627777` (ints through 2^53-1 emit as JS Numbers). Same
 * #320/#343 int-operand discipline as the binary bitwise helpers, with
 * CPython's unary message shape (`bad operand type for unary ~: 'float'`);
 * `fctx` is truthy when the codegen knows the operand is statically float
 * (a whole-valued float is an indistinguishable JS number at runtime).
 */
export function pyBitNot(a, fctx) {
    if (a != null && typeof a.__invert__ === "function") return a.__invert__();
    if (fctx || !__bitIntOk(a)) {
        const an = fctx ? "float" : __opTypeName(a);
        throw new TypeError_(`bad operand type for unary ~: '${an}'`);
    }
    if (__fits32(a)) return ~a;
    return __norm(-__toBig(a) - 1n);
}

// #249: Python `<<`/`>>` are arbitrary-precision (`1 << 99` is a 100-bit int)
// and never modulo the shift count — but JS `<<`/`>>` truncate to 32 bits and
// take the count mod 32. Always compute in BigInt then normalize back to a
// Number when it fits. BigInt `>>` is arithmetic (floors toward -inf), matching
// Python (`-8 >> 1 == -4`).
export function pyShiftLeft(a, b, fctx) {
    if (a != null && typeof a.__lshift__ === "function") return a.__lshift__(b);
    __reqBitInt("<<", a, b, fctx); // #320/#343
    return __norm(__toBig(a) << __toBig(b));
}
export function pyShiftRight(a, b, fctx) {
    if (a != null && typeof a.__rshift__ === "function") return a.__rshift__(b);
    __reqBitInt(">>", a, b, fctx); // #320/#343
    return __norm(__toBig(a) >> __toBig(b));
}

const __isPlainObject = (x) =>
    x !== null && typeof x === "object" && Object.getPrototypeOf(x) === Object.prototype;

/**
 * Python equality check.
 *
 * Order of dispatch:
 *   1. JS reference / primitive equality (`===`).
 *   2. null/undefined — Python None compares equal only with None.
 *   3. User-defined `__eq__` on either operand.
 *   4. Element-wise compare for arrays / tuples (recursive).
 *   5. Key-by-key compare for plain dicts (recursive).
 *   6. Set-membership compare for `Set` / `Map`.
 *   7. Fallback `===`.
 *
 * Matches Python `==` semantics for list/dict/set/tuple values that
 * the JS `===` operator would otherwise compare by reference only.
 */
export function pyEq(a, b) {
    if (a === b) return true;
    if (a == null || b == null) return a == b;
    // Numeric cross-representation: BigInt int vs Number float/int, plus bool
    // (Python `bool` ⊂ `int`, so `True == 1`, `False == 0`). JS loose `==`
    // matches across all three (5n == 5, 5n == 5.0, true == 1, false == 0).
    const numish = (x) =>
        typeof x === "bigint" || typeof x === "number" || typeof x === "boolean";
    if (numish(a) && numish(b)) {
        return a == b;
    }
    if (typeof a.__eq__ === "function") return a.__eq__(b);
    if (typeof b.__eq__ === "function") return b.__eq__(a);
    // Arrays / tuples: element-wise, length-aware. A list never equals a tuple
    // (crit-13): both are JS arrays, distinguished only by the __pytuple__ tag.
    if (Array.isArray(a) && Array.isArray(b)) {
        if (!!a.__pytuple__ !== !!b.__pytuple__) return false;
        if (a.length !== b.length) return false;
        for (let i = 0; i < a.length; i++) {
            if (!pyEq(a[i], b[i])) return false;
        }
        return true;
    }
    // Sets: equal-size + every element in b.
    if (a instanceof Set && b instanceof Set) {
        if (a.size !== b.size) return false;
        for (const x of a) {
            if (!b.has(x)) return false;
        }
        return true;
    }
    // Dicts — both shapes (plain object | Map/PyDict), including
    // cross-shape (#83): equal-size + key-by-key value compare.
    // Explicit .entries(): PyDict's default iterator yields KEYS.
    {
        const aMap = a instanceof Map, bMap = b instanceof Map;
        const aDict = aMap || __isPlainObject(a);
        const bDict = bMap || __isPlainObject(b);
        if (aDict && bDict) {
            const aLen = aMap ? a.size : Object.keys(a).length;
            const bLen = bMap ? b.size : Object.keys(b).length;
            if (aLen !== bLen) return false;
            for (const [k, v] of (aMap ? a.entries() : Object.entries(a))) {
                let bHas, bv;
                if (bMap) { bHas = b.has(k); bv = b.get(k); }
                else if (typeof k !== "string") return false; // plain dicts hold only string keys
                else { bHas = Object.prototype.hasOwnProperty.call(b, k); bv = b[k]; }
                if (!bHas || !pyEq(v, bv)) return false;
            }
            return true;
        }
    }
    return a === b;
}

// ── Arbitrary-precision int support (hybrid) ──────────────────────────
//
// A Python `int` is represented as a JS `Number` while it fits in the
// safe-integer range, and promoted to a `BigInt` once it would overflow
// 2**53 — so small ints stay fast/native (array indices, loop counters)
// and only genuinely-large values pay the BigInt cost. A real `float`
// (non-integer-valued Number) always uses float math.
//
// `__intBin` computes an integer binary op exactly: it tries native
// Number math and, only if the result is no longer a safe integer,
// recomputes from the (still-exact) inputs in BigInt. Results are
// normalized back to Number when they fit, so BigInt only persists for
// values that truly require it.
const MAX_SAFE = 9007199254740991n;
// #319: a JS number is a Python float when it is non-integral OR its
// magnitude exceeds the safe-integer range — a whole-valued number that big
// can only be a float, because pyths represents large ints as BigInt (never
// a Number). This routes `1e308 * 1e10` through float arithmetic (→ inf, like
// CPython) instead of the __intBin BigInt path (which would overflow-raise).
const __isFloat = (x) => typeof x === "number" && !Number.isSafeInteger(x);
// #319: coerce an operand to a JS float for float-context arithmetic. A BigInt
// too large to represent as a double raises OverflowError, matching CPython's
// "int too large to convert to float" (the int→float conversion, distinct from
// float-arithmetic overflow which yields inf).
const __reqNum = (x) => {
    if (typeof x === "bigint") {
        const n = Number(x);
        if (!isFinite(n)) throw new OverflowError("int too large to convert to float");
        return n;
    }
    return Number(x);
};
const __toBig = (x) => (typeof x === "bigint" ? x : BigInt(x));
const __norm = (big) =>
    big >= -MAX_SAFE && big <= MAX_SAFE ? Number(big) : big;

const __numeric = (x) => typeof x === "number" || typeof x === "bigint";

function __intBin(a, b, numOp, bigOp) {
    if (typeof a === "number" && typeof b === "number") {
        const r = numOp(a, b);
        if (Number.isSafeInteger(r)) return r; // common fast path
        return __norm(bigOp(BigInt(a), BigInt(b))); // overflow → exact BigInt
    }
    return __norm(bigOp(__toBig(a), __toBig(b)));
}

/**
 * Python addition with operator overloading.
 */
export function pyAdd(a, b, fctx) {
    // Wave-15 F4: bool ⊂ int (see __boolNum) — exact bool+BigInt arithmetic.
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    // Fast path: pure numeric operands skip the dunder/string lookups.
    if (__numeric(a) && __numeric(b)) {
        // #319: float context (fctx) coerces a BigInt operand to float — too
        // large → OverflowError, like CPython `(10**400) + 1.0`.
        if (fctx || __isFloat(a) || __isFloat(b)) return __reqNum(a) + __reqNum(b);
        return __intBin(a, b, (x, y) => x + y, (x, y) => x + y);
    }
    if (a != null && typeof a.__add__ === "function") return a.__add__(b);
    if (b != null && typeof b.__radd__ === "function") return b.__radd__(a);
    // Python sequence concat (JS `+` would string-coerce arrays). list+list and
    // tuple+tuple concatenate (preserving tuple-ness); mixing list and tuple is
    // a TypeError, like CPython (crit-13). Needed by pySum(xss, []) (#94).
    if (Array.isArray(a) && Array.isArray(b)) {
        const at = !!a.__pytuple__, bt = !!b.__pytuple__;
        if (at !== bt) {
            throw new TypeError_(`can only concatenate ${at ? "tuple" : "list"} (not "${bt ? "tuple" : "list"}") to ${at ? "tuple" : "list"}`);
        }
        const r = [...a, ...b];
        if (at) Object.defineProperty(r, "__pytuple__", { value: true, enumerable: false });
        return r;
    }
    __arithNoneGuard("+", a, b); // #322: None operand → TypeError, not NaN
    // crit-12: `str + non-str` (or vice-versa) is a TypeError in Python, not JS
    // coercion ("a" + 1 → "a1"). Only str+str concatenates.
    if ((typeof a === "string") !== (typeof b === "string")) {
        throw new TypeError_('can only concatenate str to str');
    }
    return a + b; // str+str concat / other non-numeric
}

/**
 * Python subtraction with operator overloading.
 */
export function pySub(a, b) {
    // Wave-15 F4: bool ⊂ int (see __boolNum) — exact bool+BigInt arithmetic.
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if (__numeric(a) && __numeric(b)) {
        if (__isFloat(a) || __isFloat(b)) return Number(a) - Number(b);
        return __intBin(a, b, (x, y) => x - y, (x, y) => x - y);
    }
    // Issue #93: `s1 - s2` on sets is Python set difference.
    if (a instanceof Set && b instanceof Set) return pySetDifference(a, b);
    if (a != null && typeof a.__sub__ === "function") return a.__sub__(b);
    if (b != null && typeof b.__rsub__ === "function") return b.__rsub__(a);
    __reqArithNum("-", a, b); // #322 + wave-15 F9 (str/list no longer coerce)
    return Number(a) - Number(b); // defensive: guard admits only num/bool, F4 coerced bools above
}

/**
 * Python multiplication with operator overloading.
 */
export function pyMul(a, b, fctx) {
    // Fast path for pure numeric operands (string/list replication below).
    if (__numeric(a) && __numeric(b)) {
        // #319: float context coerces a BigInt operand to float — too large →
        // OverflowError, like CPython `(10**400) * 1.0`.
        if (fctx || __isFloat(a) || __isFloat(b)) return __reqNum(a) * __reqNum(b);
        return __intBin(a, b, (x, y) => x * y, (x, y) => x * y);
    }
    if (a != null && typeof a.__mul__ === "function") return a.__mul__(b);
    if (b != null && typeof b.__rmul__ === "function") return b.__rmul__(a);
    // Python string * int replication (count may be Number or BigInt)
    if (typeof a === "string" && (typeof b === "number" || typeof b === "bigint")) return a.repeat(Number(b));
    if ((typeof a === "number" || typeof a === "bigint") && typeof b === "string") return b.repeat(Number(a));
    // Python list * int replication
    if (Array.isArray(a) && (typeof b === "number" || typeof b === "bigint")) {
        const result = [];
        const n = Number(b);
        for (let i = 0; i < n; i++) result.push(...a);
        return result;
    }
    __arithNoneGuard("*", a, b); // #322
    if (__isFloat(a) || __isFloat(b)) return Number(a) * Number(b);
    return __intBin(a, b, (x, y) => x * y, (x, y) => x * y);
}

/**
 * Python division — true division always returns float.
 */
export function pyDiv(a, b, floatDiv) {
    // Wave-15 F4: bool ⊂ int (see __boolNum) — exact bool+BigInt arithmetic.
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if ((!__numeric(a) || !__numeric(b)) && a != null && typeof a.__truediv__ === "function") return a.__truediv__(b);
    __reqArithNum("/", a, b); // #322 + wave-15 F9 (str/list no longer coerce)
    // #319: divisor converts to float too — a too-large BigInt → OverflowError.
    const bn = __reqNum(b);
    if (bn === 0) {
        // F4: CPython distinguishes `1/0` ("division by zero") from
        // `1.0/0.0` ("float division by zero"). `floatDiv` is set by codegen
        // when an operand is a statically-known float; `__isFloat` catches
        // non-integer runtime floats. Whole-valued float literals (`2.0`)
        // compile to the same JS number as an int, so the compile-time flag
        // is what disambiguates those.
        throw new ZeroDivisionError(
            (floatDiv || __isFloat(a) || __isFloat(b)) ? "float division by zero" : "division by zero",
        );
    }
    // #319: true division always produces a float, so a BigInt operand must
    // convert to float — too large → OverflowError (CPython `(10**400)/1.0`).
    return __reqNum(a) / bn;
}

/**
 * Python floor division //.
 */
export function pyFloorDiv(a, b) {
    // Wave-15 F4: bool ⊂ int (see __boolNum) — exact bool+BigInt arithmetic.
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if ((!__numeric(a) || !__numeric(b)) && a != null && typeof a.__floordiv__ === "function") return a.__floordiv__(b);
    // Wave-15 F9 SHIPPING BUG: the old None-only guard let numeric strings
    // and singleton arrays fall into the BigInt fallback (`"4".strip("x") //
    // 2` → 2, `"4".split(",") // 2` → 2); CPython raises TypeError. The Lean
    // wave-15 model (`SgVal.asArith`: gstr/glist → none) was already correct
    // — this makes the shipped helper match it.
    __reqArithNum("//", a, b); // #322 + wave-15 F9
    if (__isFloat(a) || __isFloat(b)) {
        const x = Number(a), y = Number(b);
        if (y === 0) throw new ZeroDivisionError("float floor division by zero");
        // CPython float_divmod: floor toward -inf, correcting fmod's sign, so
        // 1.0 // 0.1 == 9.0 (not Math.floor(1.0/0.1) == 10). JS `%` is C fmod.
        const mod = x % y;
        let div = (x - mod) / y;
        if (mod !== 0 && (y < 0) !== (mod < 0)) div -= 1;
        let fd = Math.floor(div);
        if (div - fd > 0.5) fd += 1;
        return fd;
    }
    if (Number(b) === 0) throw new ZeroDivisionError("integer division or modulo by zero");
    // Python floors toward -inf; BigInt `/` truncates toward zero.
    return __intBin(
        a, b,
        (x, y) => Math.floor(x / y),
        (x, y) => {
            let q = x / y;
            if (x % y !== 0n && (x < 0n) !== (y < 0n)) q -= 1n;
            return q;
        },
    );
}

/**
 * Python modulo % with correct negative handling (sign of divisor).
 */
export function pyMod(a, b) {
    // Wave-15 F4: bool ⊂ int (see __boolNum) — exact bool+BigInt arithmetic.
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if ((!__numeric(a) || !__numeric(b)) && a != null && typeof a.__mod__ === "function") return a.__mod__(b);
    // Wave-15 (narrowed): `%` with a string LEFT operand is printf-style
    // formatting in Python — a surface PythScribe does NOT implement (known
    // limitation; use f-strings). Raise an HONEST unsupported-feature error
    // (NotImplementedError), never a counterfeit CPython TypeError and never
    // the old silent BigInt coercion (`"4" % 2` → 0) / raw JS SyntaxError
    // crash (`"%d" % 5`). Implementing `%`-format (or matching CPython's
    // exact TypeError split) is the arithmetic-type-safety workstream.
    if (typeof a === "string") {
        throw new NotImplementedError(
            "printf-style %-formatting is not supported by PythScribe; use an f-string",
        );
    }
    __reqArithNum("%", a, b); // #322 + wave-15 F9 (str/list no longer coerce)
    if (__isFloat(a) || __isFloat(b)) {
        const bf = Number(b);
        if (bf === 0) throw new ZeroDivisionError("float modulo by zero");
        return ((Number(a) % bf) + bf) % bf;
    }
    if (Number(b) === 0) throw new ZeroDivisionError("integer division or modulo by zero");
    return __intBin(a, b, (x, y) => ((x % y) + y) % y, (x, y) => ((x % y) + y) % y);
}

/**
 * Python power **.
 */
export function pyPow(a, b, fctx) {
    // Wave-15 F4: bool ⊂ int (see __boolNum) — exact bool+BigInt arithmetic.
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if (__numeric(a) && __numeric(b)) {
        // Float context (a statically-known float operand — the codegen
        // passes fctx since a whole-valued `10.0` is an indistinguishable
        // JS number at runtime), a runtime float, or a negative exponent →
        // float result. #319: float ** overflow raises OverflowError in
        // CPython (unlike float * / + overflow, which yields inf).
        if (fctx || __isFloat(a) || __isFloat(b) || (typeof b === "bigint" ? b < 0n : b < 0)) {
            const r = __reqNum(a) ** __reqNum(b);
            if (!isFinite(r) && isFinite(__reqNum(a)) && isFinite(__reqNum(b))) {
                throw new OverflowError("(34, 'Result too large')");
            }
            return r;
        }
        return __intBin(a, b, (x, y) => x ** y, (x, y) => x ** y);
    }
    if (a != null && typeof a.__pow__ === "function") return a.__pow__(b);
    // Wave-15 F9 — CPython's `**` message names both spellings.
    __reqArithNum("** or pow()", a, b); // #322 + wave-15 F9
    return Number(a) ** Number(b); // defensive: guard admits only num/bool, F4 coerced bools above
}

/**
 * Python less-than comparison. Dispatches `__lt__` on the left operand,
 * falling back to the reflected `__gt__` on the right operand (Python's
 * reflected-comparison rule — e.g. `5 < Decimal('3')` dispatches
 * `Decimal('3').__gt__(5)`), before falling back to bare `<`.
 */
// #214: Python compares tuples/lists lexicographically element-by-element
// (`(-6, "s") < (-2, "x")`), but JS `<` on arrays coerces to a joined string
// ("-6,s" < "-2,x"). Route array<->array comparison through this recursive
// helper (recurses via pyLt so nested tuples and mixed element types work).
// Shorter sequence is smaller when it is a prefix of the longer.
function __seqLt(a, b) {
    const n = Math.min(a.length, b.length);
    for (let i = 0; i < n; i++) {
        if (pyLt(a[i], b[i])) return true;
        if (pyLt(b[i], a[i])) return false;
    }
    return a.length < b.length;
}

export function pyLt(a, b) {
    if (a != null && typeof a.__lt__ === "function") return a.__lt__(b);
    if (b != null && typeof b.__gt__ === "function") return b.__gt__(a);
    if (Array.isArray(a) && Array.isArray(b)) return __seqLt(a, b);
    return a < b;
}

/**
 * Python less-than-or-equal comparison (see `pyLt` for dispatch order).
 */
export function pyLe(a, b) {
    if (a != null && typeof a.__le__ === "function") return a.__le__(b);
    if (b != null && typeof b.__ge__ === "function") return b.__ge__(a);
    if (Array.isArray(a) && Array.isArray(b)) return !__seqLt(b, a);
    return a <= b;
}

/**
 * Python greater-than comparison (see `pyLt` for dispatch order).
 */
export function pyGt(a, b) {
    if (a != null && typeof a.__gt__ === "function") return a.__gt__(b);
    if (b != null && typeof b.__lt__ === "function") return b.__lt__(a);
    if (Array.isArray(a) && Array.isArray(b)) return __seqLt(b, a);
    return a > b;
}

/**
 * Python greater-than-or-equal comparison (see `pyLt` for dispatch order).
 */
export function pyGe(a, b) {
    if (a != null && typeof a.__ge__ === "function") return a.__ge__(b);
    if (b != null && typeof b.__le__ === "function") return b.__le__(a);
    if (Array.isArray(a) && Array.isArray(b)) return !__seqLt(a, b);
    return a >= b;
}

/**
 * Python not-equal comparison.
 */
export function pyNe(a, b) {
    if (typeof a.__ne__ === "function") return a.__ne__(b);
    return !pyEq(a, b);
}

/**
 * Python `abs()`. Dispatches `__abs__` when defined (Decimal/Fraction
 * return their own type, not a coerced float) — otherwise falls back to
 * plain `Math.abs`, unchanged from before this helper existed.
 */
export function pyAbs(x) {
    if (x != null && typeof x.__abs__ === "function") return x.__abs__();
    // Python: abs() of an int of any magnitude stays an exact int; JS
    // Math.abs throws on BigInt ("Cannot convert a BigInt value to a number").
    if (typeof x === "bigint") return x < 0n ? -x : x;
    return Math.abs(x);
}

/**
 * Python negative unary operator.
 */
export function pyNeg(a) {
    if (a != null && typeof a.__neg__ === "function") return a.__neg__();
    if (typeof a === "bigint") return __norm(-a);
    return -a;
}

/**
 * Python `print`. `sep.join(str(a) for a in args)` (Python default
 * `sep=" "`), followed by a newline — i.e. every arg is converted via
 * [`pyStr`] (Python semantics: bools/None/floats/containers all format
 * the CPython way, not JS's `console.log` inspector formatting) before
 * being joined and written out.
 */
export function pyPrint(...args) {
    console.log(args.map(pyStr).join(" "));
}

/**
 * Python `str()`. Top-level strings pass through unquoted (Python's
 * `str("hi") == "hi"`, no quotes) — everything else (bool/None/numbers/
 * containers, and custom classes without their own `__str__`) delegates
 * to [`pyRepr`], which is exactly Python's own fallback rule: `str(x)`
 * prefers `x.__str__()`, then falls back to `repr(x)` — and for
 * containers `str(list) == repr(list)` in CPython (nested elements are
 * always shown via repr(), e.g. nested strings stay quoted).
 */
export function pyStr(obj) {
    if (typeof obj === "string") return obj;
    if (obj != null && typeof obj.__str__ === "function") return obj.__str__();
    // F4: an exception's str() is its message (the single arg), not a dict
    // dump of its enumerable own props. Without this, `print(e)` / `str(e)`
    // over a caught error rendered `{'name': 'ZeroDivisionError'}`.
    if (obj instanceof Error) return obj.message != null ? String(obj.message) : "";
    // Issue #97: the codegen renames a user-defined `__str__` to
    // `toString()` (Python-dunder → JS-idiom key transformation), so the
    // `__str__` probe above never sees it. A toString that is NOT one of
    // the natives (Object/Array/Set/Map inherit Object.prototype's or
    // Array.prototype's) is a user override — Python's str() would have
    // dispatched to it. Numbers/bigints/booleans keep the pyRepr path
    // (their native toString is JS formatting, not Python's).
    if (obj !== null && typeof obj === "object" && !Array.isArray(obj)
        && typeof obj.toString === "function"
        && obj.toString !== Object.prototype.toString) {
        return obj.toString();
    }
    return pyRepr(obj);
}

/**
 * CPython-compatible shortest-round-trip float repr.
 *
 * PythScribe floats are plain JS `number`s (ints are BigInt-backed — see
 * pyRepr below), so every `number` reaching this function is a Python
 * float. CPython's `repr(float)` is the shortest decimal string that
 * round-trips back to the same double — the same class of algorithm
 * `Number.prototype.toExponential()` (no argument) already implements in
 * V8 — but CPython formats that digit string differently:
 *   - whole numbers get a trailing `.0` (JS: `"1"`, Python: `"1.0"`)
 *   - the fixed/scientific-notation switchover threshold differs (Python:
 *     scientific when the decimal-point position `decpt` is `<= -4` or
 *     `> 16`; JS switches much later, at `< -6` or `>= 21`)
 *   - the scientific exponent is always signed and zero-padded to 2
 *     digits (Python: `1e+16`, `5e-07`; JS: `1e+16`, `5e-7`)
 *
 * `toExponential()`'s digit string is the canonical shortest-round-trip
 * representation (a mathematically unique value for a given double), so
 * reusing it here — rather than re-implementing dtoa — guarantees the
 * same digits CPython's own shortest-repr algorithm would produce; only
 * the *formatting* (fixed vs. scientific, `.0`, exponent padding) needs
 * to be redone Python-style.
 */
export function pyFormatFloat(n) {
    // #319: a float-typed result can arrive as a BigInt when a whole-valued
    // float operation fell through the __intBin path (e.g. `10.0 ** 308`).
    // Convert to a double: finite → float-format ("1e+308"); too large →
    // OverflowError, matching CPython's int→float conversion limit.
    if (typeof n === "bigint") {
        const f = Number(n);
        if (!isFinite(f)) throw new OverflowError("int too large to convert to float");
        n = f;
    }
    if (Number.isNaN(n)) return "nan";
    if (n === Infinity) return "inf";
    if (n === -Infinity) return "-inf";
    const negative = n < 0 || Object.is(n, -0);
    const abs = Math.abs(n);
    if (abs === 0) return negative ? "-0.0" : "0.0";

    const m = /^(\d)(?:\.(\d+))?e([+-]\d+)$/.exec(abs.toExponential());
    const digits = m[1] + (m[2] || "");
    const exponent = parseInt(m[3], 10);
    const decpt = exponent + 1; // CPython's format_float_short "decpt"

    let out;
    if (decpt <= -4 || decpt > 16) {
        const mantissa = digits.length > 1 ? `${digits[0]}.${digits.slice(1)}` : digits;
        const sign = exponent < 0 ? "-" : "+";
        const expDigits = String(Math.abs(exponent)).padStart(2, "0");
        out = `${mantissa}e${sign}${expDigits}`;
    } else if (decpt <= 0) {
        out = `0.${"0".repeat(-decpt)}${digits}`;
    } else if (decpt >= digits.length) {
        out = `${digits}${"0".repeat(decpt - digits.length)}.0`;
    } else {
        out = `${digits.slice(0, decpt)}.${digits.slice(decpt)}`;
    }
    return negative ? `-${out}` : out;
}

/**
 * Construct a Python tuple value: a real JS `Array` (so indexing,
 * `.length`, iteration, spread, `Array.isArray`, and `pyEq`'s
 * element-wise compare all keep working unchanged) stamped with a
 * non-enumerable `.__pytuple__` marker so [`pyRepr`]/[`pyStr`] can print
 * `(a, b)` instead of `[a, b]`. Non-enumerable means the marker is
 * invisible to `Object.keys`, `for...in`, `JSON.stringify` (which for
 * arrays only ever walks index keys regardless of enumerability), and
 * object-spread — so it can't leak into equality, serialization, or
 * iteration. It does NOT survive operations that build a fresh array
 * (concatenation via spread, `.slice`, comprehensions) — a tuple derived
 * that way currently prints as a list. That mirrors how the codebase
 * already treats lists (no new gap) and is the accepted, documented
 * scope of the A4 fix: literal tuple construction, not every derived
 * tuple value.
 */
export function pyTuple(...items) {
    Object.defineProperty(items, "__pytuple__", { value: true, enumerable: false });
    return items;
}

/**
 * CPython-compatible repr. Recurses into containers.
 *
 *   repr(None)       → 'None'
 *   repr(True)       → 'True'
 *   repr(False)      → 'False'
 *   repr(3)          → '3'
 *   repr(3.14)       → '3.14'
 *   repr(1.0)        → '1.0'
 *   repr('hi')       → "'hi'"     (single quotes; switches to double if single inside)
 *   repr([1,2])      → '[1, 2]'
 *   repr({'a': 1})   → "{'a': 1}"
 *   repr({1, 2})     → '{1, 2}'   (Set)
 *   repr((1, 2))     → '(1, 2)'   (Tuple — distinguished by .__pytuple__ marker, see pyTuple)
 */
/**
 * #110: `tuple(iterable)` / first-class `tuple` — a FACTORY over pyTuple
 * (the PyTuple class constructor cannot be invoked without `new`, and
 * tuple values are marked arrays, not class instances).
 */
export function pyTupleOf(iterable) {
    // #348 family (eager-spread overflow): the old `pyTuple(...[...iterable])`
    // spreads the iterable into FUNCTION ARGUMENTS, which blows the call stack
    // (`RangeError: Maximum call stack size exceeded`) on large inputs —
    // `tuple(large_list)` overflowed at ~10^5 elements (EvalPerf Mbpp/587).
    // Build the marked array by iterative consumption instead; no spread.
    if (iterable === undefined) return pyTuple();
    let items;
    if (Array.isArray(iterable)) {
        items = iterable.slice();
    } else if (
        iterable !== null &&
        typeof iterable === "object" &&
        typeof iterable[Symbol.iterator] !== "function" &&
        Object.getPrototypeOf(iterable) === Object.prototype
    ) {
        // Plain-prototype object = the all-string-key dict representation; it
        // has no Symbol.iterator, so `[...iterable]` would throw. CPython's
        // `tuple(d)` yields the dict's KEYS.
        items = Object.keys(iterable);
    } else {
        items = [...iterable];
    }
    Object.defineProperty(items, "__pytuple__", { value: true, enumerable: false });
    return items;
}

// `list(iterable)` — a fresh mutable list. Routes through pySeq (which yields
// KEYS for plain-object / Map-backed dicts, code points for strings, and
// materializes ranges / generators / sets) and always COPIES so the result is
// independent of the source. Plain `Array.from` returned `[]` for plain-object
// dicts (no Symbol.iterator) and entries — not keys — for Maps.
export function pyListOf(iterable) {
    if (iterable === undefined) return [];
    const seq = pySeq(iterable);
    // pySeq returns the input array itself for arrays (and marked tuples); a
    // list must be an independent, unmarked array, so copy in that case.
    return seq === iterable ? seq.slice() : seq;
}

// #329: assigned non-printable code-point ranges [lo,hi,lo,hi,...] — the
// Unicode "Other"/"Separator" categories (Cc/Cf/Cs/Co/Zl/Zp/Zs, ASCII space
// excluded). CPython repr escapes exactly the code points `str.isprintable()`
// rejects; this table is an exact match for all ASSIGNED code points
// (unassigned Cn, which CPython also escapes, is version-fragile and omitted).
const __NP_RANGES = [
    0x0, 0x1f, 0x7f, 0xa0, 0xad, 0xad, 0x600, 0x605, 0x61c, 0x61c, 0x6dd, 0x6dd,
    0x70f, 0x70f, 0x890, 0x891, 0x8e2, 0x8e2, 0x1680, 0x1680, 0x180e, 0x180e,
    0x2000, 0x200f, 0x2028, 0x202f, 0x205f, 0x2064, 0x2066, 0x206f, 0x3000, 0x3000,
    0xd800, 0xf8ff, 0xfeff, 0xfeff, 0xfff9, 0xfffb, 0x110bd, 0x110bd, 0x110cd, 0x110cd,
    0x13430, 0x1343f, 0x1bca0, 0x1bca3, 0x1d173, 0x1d17a, 0xe0001, 0xe0001,
    0xe0020, 0xe007f, 0xf0000, 0xffffd, 0x100000, 0x10fffd,
];
function __cpNonPrintable(cp) {
    let lo = 0, hi = __NP_RANGES.length / 2 - 1;
    while (lo <= hi) {
        const mid = (lo + hi) >> 1;
        const a = __NP_RANGES[mid * 2], b = __NP_RANGES[mid * 2 + 1];
        if (cp < a) hi = mid - 1;
        else if (cp > b) lo = mid + 1;
        else return true;
    }
    return false;
}

export function pyRepr(obj) {
    if (obj === null || obj === undefined) return "None";
    if (typeof obj === "boolean") return obj ? "True" : "False";
    // Round-3 pythonic sweep: a user-defined `__repr__` wins (class
    // instances previously repr'd as their enumerable-own-props dict).
    // Containers (Array/Map/Set/plain dicts) never define __repr__, so
    // the probe is safe ahead of the container branches.
    if (typeof obj === "object" && typeof obj.__repr__ === "function") {
        return obj.__repr__();
    }
    if (typeof obj === "number") {
        // PythScribe ints are usually BigInt-backed (see pyRepr's final
        // String(obj) fallback for bigint) — EXCEPT small ints (abs <=
        // Number.MAX_SAFE_INTEGER) and safe-range BigInt-arithmetic
        // results, which are demoted to plain `number` for performance
        // (see codegen's ExprKind::IntLiteral / __norm()). That means a
        // plain `number` that is an exact integer *within* the safe range
        // is genuinely ambiguous at this point — it could be a small
        // Python int (no trailing `.0`) or a whole-number Python float
        // (`1.0` — wants `.0`) — the two compile to the identical JS
        // value and there is no runtime tag to tell them apart.
        //
        // Outside the safe-integer range, a real Python int would have
        // stayed BigInt, so a huge plain `number` here is unambiguously a
        // float — and any non-integer value is unambiguously a float too
        // (Python ints are always exact). Both go through pyFormatFloat.
        //
        // For the ambiguous case, default to int-like display (no `.0`)
        // — ints are overwhelmingly more common, and this preserves prior
        // (pre-A4) behavior for the common case. The specific case this
        // leaves unfixed — a whole-number float value reaching pyRepr
        // through an untyped channel (nested in a list/dict/tuple
        // literal, returned from an unannotated function, etc.) — is a
        // documented, accepted residual gap of the A4 fix. Where the
        // float-ness IS statically known at compile time (a literal like
        // `1.0`, a `float`-annotated variable, `/` division, ...), the
        // *compiler* resolves the ambiguity upstream — see the A4 note
        // above emit_call's print/str/repr handling and the f-string
        // FStringPart::Expr case in crates/pyths_codegen_js/src/emit.rs
        // — by pre-formatting with pyFormatFloat at the call site, so
        // this ambiguous branch is never reached for those cases.
        if (Number.isInteger(obj) && Math.abs(obj) <= Number.MAX_SAFE_INTEGER) {
            return String(obj);
        }
        return pyFormatFloat(obj);
    }
    if (typeof obj === "string") {
        // CPython repr: prefer single quotes; use double if string
        // contains single but no double; escape both as backslash-quote
        // if both present. Since #95 string literals hold their DECODED
        // characters, repr must re-escape them the way CPython does:
        // backslash, \t \n \r by name, other control chars as \xNN.
        let body = "";
        for (const ch of obj) {
            if (ch === "\\") body += "\\\\";
            else if (ch === "\t") body += "\\t";
            else if (ch === "\n") body += "\\n";
            else if (ch === "\r") body += "\\r";
            else {
                const cp = ch.codePointAt(0);
                // #329: escape any non-printable code point (CPython repr
                // escapes everything `str.isprintable()` rejects — Cc/Cf/Cs/
                // Co/Zl/Zp/Zs, except ASCII space), not just the ASCII control
                // set. \xNN for <0x100, \uNNNN for <0x10000, else \UNNNNNNNN.
                if (__cpNonPrintable(cp)) {
                    body += cp < 0x100 ? "\\x" + cp.toString(16).padStart(2, "0")
                        : cp < 0x10000 ? "\\u" + cp.toString(16).padStart(4, "0")
                        : "\\U" + cp.toString(16).padStart(8, "0");
                } else {
                    body += ch;
                }
            }
        }
        if (!body.includes("'")) return `'${body}'`;
        if (!body.includes('"')) return `"${body}"`;
        return `'${body.replace(/'/g, "\\'")}'`;
    }
    // A user-supplied __repr__ wins over the container branches below —
    // stdlib types built on JS containers (Counter/OrderedDict extend Map,
    // namedtuple extends Array, deque) print CPython-style through it.
    // Pythonic-checks sweep; plain arrays/Maps/Sets have no __repr__.
    if (typeof obj.__repr__ === "function") return obj.__repr__();
    if (typeof obj.__pytuple__ !== "undefined" || (Array.isArray(obj) && obj.__pytuple__)) {
        const inner = obj.map(pyRepr).join(", ");
        return obj.length === 1 ? `(${inner},)` : `(${inner})`;
    }
    if (Array.isArray(obj)) {
        return `[${obj.map(pyRepr).join(", ")}]`;
    }
    if (obj instanceof Set) {
        if (obj.size === 0) return "set()";
        return `{${[...obj].map(pyRepr).join(", ")}}`;
    }
    if (obj instanceof Map) {
        // Explicit .entries(): PyDict's default iterator yields KEYS.
        const parts = [];
        for (const [k, v] of obj.entries()) parts.push(`${pyRepr(k)}: ${pyRepr(v)}`);
        return `{${parts.join(", ")}}`;
    }
    if (typeof obj.__repr__ === "function") return obj.__repr__();
    // F4: CPython repr of an exception is `Name('message')`, not a dict of
    // its enumerable own props. Must precede the plain-object branch (an
    // Error's `name` is an own enumerable prop and would otherwise leak).
    if (obj instanceof Error) {
        // Round-4 sweep: prefer the class's `__name__` (set for runtime and
        // compiled user exception classes) over `obj.name`, which a subclass
        // inherits from its base; and render from `args` when present so
        // multi-arg exceptions repr as `MyErr('boom', 42)` like CPython.
        const nm = obj.constructor?.__name__ ?? obj.name;
        if (Array.isArray(obj.args)) {
            return `${nm}(${obj.args.map((a) => pyRepr(a)).join(", ")})`;
        }
        const msg = obj.message;
        return `${nm}(${msg != null && msg !== "" ? pyRepr(String(msg)) : ""})`;
    }
    if (typeof obj === "object") {
        // Plain object → dict repr
        const parts = [];
        for (const k of Object.keys(obj)) {
            parts.push(`${pyRepr(k)}: ${pyRepr(obj[k])}`);
        }
        return `{${parts.join(", ")}}`;
    }
    return String(obj);
}
