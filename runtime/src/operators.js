// PythScribe Runtime — Operator dispatch
// Implements Python-compatible operator overloading via dunder methods.

import {
    ZeroDivisionError, ValueError, OverflowError, IndexError, TypeError as TypeError_,
    NotImplementedError,
    pySetUnion, pySetIntersection, pySetDifference, pySetSymmetricDifference,
    pyFrozensetOf,
    pyDictMerge, pyUpdate, pySeq, __pyOwnKeys, PySlice, __pyBytesKind, __pyTypeName,
    // F5-r2 (v0.2.4): THE slice-component validator (int/None/__index__ →
    // TypeError otherwise) — the bytearray slice write/delete paths validate
    // through it BEFORE any mutation, same guard as list get/set/del.
    // Call-time-only dereference keeps the deliberate import cycle safe.
    __pySliceIndex,
    // E3: printf-style `%` formatting engine (str % args). Call-time-only
    // dereference (same deliberate cycle as above).
    pyStrMod,
} from "./runtime.js";

// #469 (was #322): the operand-type name in every operator error message
// comes from __pyTypeName (runtime.js) — THE ONE value-model type-name
// source, backed by pyType. The old local __opTypeName copy here lacked the
// function / class / plain-object arms, so operand messages leaked JS class
// names ('Function' for a function AND for a class, 'Object' for a
// plain-object dict) while iteration messages — already routed through
// __pyTypeName (#467) — said 'function'/'type'/'dict' for the SAME value.
// One computer ⇒ every message agrees by construction; a new value form
// added to pyType can never drift between operand/bit/iteration paths.

// ═══ THE BINARY-OP OPERAND-TYPE AUTHORITY (E2-lite; #466 class) ═══
//
// Terminal decision for a binary op whose operand pair matched NO valid
// combination: ALWAYS throws the CPython exception for the pair. Every
// binary-op helper's fall-through ends here instead of a raw JS operator
// (pyAdd's old `return a + b` string-coerced b"abc" + 1 into the silent
// value "97,98,991" — #466), so a future op cannot silently fall through.
// Reuses B's __pyBytesKind (bytes detection) and __pyTypeName (#469 — the
// ONE type-name source); messages are CPython 3.12-exact:
//   * `+` with a bytes-like LEFT   → "can't concat X to bytes|bytearray"
//   * `+` with a str LEFT          → 'can only concatenate str (not "X") to str'
//   * `+` with a list/tuple LEFT   → 'can only concatenate list (not "X") to list'
//   * `*` with a sequence operand  → "can't multiply sequence by non-int of type 'X'"
//                                    (X = the NON-sequence side's type — CPython
//                                    names the offending count operand)
//   * anything else                → "unsupported operand type(s) for OP: 'X' and 'Y'"
// Subsumes the old __arithNoneGuard (#322): __pyTypeName(None) is 'NoneType',
// so None operands land in the generic branch with the identical message.
export function __binOpTypeError(op, a, b) {
    if (op === "+") {
        const ak = __pyBytesKind(a);
        if (ak !== null) {
            throw new TypeError_(`can't concat ${__pyTypeName(b)} to ${ak}`);
        }
        if (typeof a === "string") {
            throw new TypeError_(`can only concatenate str (not "${__pyTypeName(b)}") to str`);
        }
        if (Array.isArray(a)) {
            const an = a.__pytuple__ ? "tuple" : "list";
            throw new TypeError_(`can only concatenate ${an} (not "${__pyTypeName(b)}") to ${an}`);
        }
    } else if (op === "*") {
        const seq = (v) => typeof v === "string" || Array.isArray(v) || __pyBytesKind(v) !== null;
        if (seq(a)) {
            throw new TypeError_(`can't multiply sequence by non-int of type '${__pyTypeName(b)}'`);
        }
        if (seq(b)) {
            throw new TypeError_(`can't multiply sequence by non-int of type '${__pyTypeName(a)}'`);
        }
    }
    throw new TypeError_(
        `unsupported operand type(s) for ${op}: '${__pyTypeName(a)}' and '${__pyTypeName(b)}'`,
    );
}

// E2 (#466 class): sequence-replication count — CPython admits int and
// bool (bool ⊂ int) as `seq * n` counts, NEVER float (2.5 silently
// truncated through JS `.repeat`; a boxed 2.0 fell through entirely) or
// any other type. ONE rule for bytes/str/list alike; null = "not a valid
// count" (the caller falls through to __binOpTypeError's non-int message).
export const __mulRepCount = (v) => {
    if (typeof v === "boolean") return v ? 1 : 0;
    if (typeof v === "bigint") {
        // #471: CPython bounds the count to an index-sized (Py_ssize_t)
        // integer — `[1] * (10**30)` raises OverflowError, never a JS
        // RangeError ("Invalid array length") from the replication loop.
        if (v > 9223372036854775807n || v < -9223372036854775808n) {
            throw new OverflowError("cannot fit 'int' into an index-sized integer");
        }
        return Number(v);
    }
    if (typeof v === "number" && Number.isInteger(v)) {
        // #471: same index-size bound for a Number-typed integer count
        // (defensive — the hybrid int model keeps ints this large as
        // BigInt, but an interop caller can hand a huge Number). 2**63 is
        // exactly representable as f64; 2**63-1 is not.
        if (v >= 9223372036854775808 || v < -9223372036854775808) {
            throw new OverflowError("cannot fit 'int' into an index-sized integer");
        }
        return v;
    }
    return null;
};

// Wave-15 F9 (cat-C, NARROWED): the `-`/`/`/`//`/`%`/`**` slow paths must
// accept ONLY numeric operands — int/float/bool (bool ⊂ int), i.e. JS
// number/bigint/boolean. The old fallbacks coerced numeric STRINGS and
// singleton ARRAYS through Number()/BigInt() (`"4" // 2` → 2, `[4] // 2` →
// 2, `"4" - 2` → 2), where CPython raises TypeError. Called AFTER dunder
// dispatch and after the legitimate non-numeric overloads (set ops), so
// user classes and valid Python surfaces are untouched. Handles None too
// (__pyTypeName(None) is 'NoneType'). Mirrors the #320/#343 __reqBitInt
// discipline for the bitwise family. E2 (#466): the throw is delegated to
// __binOpTypeError, the ONE binary-op operand-type authority, so every
// helper renders identical CPython messages; `+` (pyAdd) and `*` (pyMul)
// route their own fall-throughs to the same authority (their valid
// non-numeric surfaces — concat, replication — are checked in-helper).
const __arithNumOk = (x) =>
    typeof x === "number" || typeof x === "bigint" || typeof x === "boolean"
    || (x != null && x.__pyfloat__ === true);
function __reqArithNum(op, a, b) {
    if (!__arithNumOk(a) || !__arithNumOk(b)) __binOpTypeError(op, a, b);
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

// F3-r2 (v0.2.4): THE "interpreted as an integer" validator — CPython's
// __index__ protocol (PyNumber_Index), used wherever a value must BE an int
// (int() base, round() ndigits, __index__-bearing receivers). Replaces the
// r1 __numProtoOk valueOf heuristic, which accepted ANY overridden-valueOf
// object (a JS-coercion test, not Python's protocol). Accepts: bool (⊂ int),
// int (integer Number of any magnitude / BigInt), or an object exposing
// __index__ whose result is an int (bool result accepted — CPython 3.12
// takes it with a DeprecationWarning). A float — native non-integer, or
// boxed — a str, None, or a protocol-less object raises CPython's exact
// TypeError; a bad __index__ result raises "__index__ returned non-int".
export function __pyAsIndexInt(v) {
    if (typeof v === "boolean") return v ? 1 : 0;
    if (typeof v === "bigint") return v;
    if (typeof v === "number") {
        if (Number.isInteger(v)) return v;
        throw new TypeError_("'float' object cannot be interpreted as an integer");
    }
    if (v != null && v.__pyfloat__ !== true && typeof v.__index__ === "function") {
        const r = v.__index__();
        if (typeof r === "boolean") return r ? 1 : 0;
        if (typeof r === "bigint") return r;
        if (typeof r === "number" && Number.isInteger(r)) return r;
        throw new TypeError_(`__index__ returned non-int (type ${__pyTypeName(r)})`);
    }
    throw new TypeError_(`'${__pyTypeName(v)}' object cannot be interpreted as an integer`);
}

// F3 (v0.2.4): bytes-like receivers are legal for int()/float()
// (int(b'12') → 12, float(b'1.5') → 1.5) — decode as ASCII/latin-1 and
// take the string path. Returns null for non-bytes; otherwise the decoded
// string plus the CPython b'…' repr for error messages.
function __bytesArgDecode(x) {
    if (__pyBytesKind(x) === null) return null;
    let s = "";
    for (let i = 0; i < x.length; i++) s += String.fromCharCode(x[i]);
    return { s, repr: pyRepr(x) };
}

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
    if (typeof x === "boolean") return __pyF(x ? 1 : 0);
    if (typeof x === "number") return __pyF(x);
    if (x != null && x.__pyfloat__ === true) return x;
    // #319: float(huge_int) overflows to OverflowError in CPython, not inf.
    if (typeof x === "bigint") return __pyF(__reqNum(x));
    // F3 (v0.2.4): bytes-like → decode and parse like a string, with the
    // b'…' repr preserved in the ValueError message.
    let __bytesRepr = null;
    const __dec = __bytesArgDecode(x);
    if (__dec !== null) {
        __bytesRepr = __dec.repr;
        x = __dec.s;
    }
    if (typeof x === "string") {
        // r2 should-fix: a BYTES receiver only strips ASCII whitespace —
        // JS trim() also removes U+00A0/U+2028/… so float(b'\xa01.5')
        // silently parsed where CPython raises ValueError. (A str receiver
        // keeps trim(): Python's int/float accept any str whitespace,
        // '\xa0'.isspace() is True.)
        const t = __bytesRepr !== null
            ? x.replace(/^[\t\n\v\f\r ]+|[\t\n\v\f\r ]+$/g, "")
            : x.trim();
        const m = /^([+-]?)(inf|infinity|nan)$/i.exec(t);
        if (m) {
            if (m[2].toLowerCase() === "nan") return __pyF(NaN);
            return __pyF(m[1] === "-" ? -Infinity : Infinity);
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
                    throw new ValueError(`could not convert string to float: ${__bytesRepr ?? pyRepr(x)}`);
                }
            }
            t2 = t.replace(/_/g, "");
        }
        // Reject empty / non-numeric strings the way CPython does, rather
        // than inheriting JS's lenient Number() coercions.
        if (t2 === "" || !/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(t2)) {
            throw new ValueError(`could not convert string to float: ${__bytesRepr ?? pyRepr(x)}`);
        }
        return __pyF(Number(t2));
    }
    // F3-r2 (v0.2.4): the Python numeric protocol, not a valueOf heuristic —
    // PyNumber_Float order: __float__ first, then __index__. The result of
    // __float__ must BE a float (boxed, or a non-integer native Number —
    // an integer-valued native Number is an int in this value model), else
    // CPython's "T.__float__ returned non-float (type R)". Decimal/Fraction
    // implement __float__/__int__ (stdlib); a bare-valueOf object (the r1
    // __numProtoOk allowance) now raises the receiver TypeError like any
    // other protocol-less object.
    if (x != null && typeof x.__float__ === "function") {
        const r = x.__float__();
        if (r != null && r.__pyfloat__ === true) return r;
        if (typeof r === "number" && !Number.isInteger(r)) return __pyF(r);
        throw new TypeError_(
            `${__pyTypeName(x)}.__float__ returned non-float (type ${__pyTypeName(r)})`,
        );
    }
    if (x != null && x.__pyfloat__ !== true && typeof x.__index__ === "function") {
        // float(idx_obj) → the integer via __index__, as a float. __reqNum
        // keeps CPython's OverflowError for a BigInt beyond the double range.
        return __pyF(__reqNum(__pyAsIndexInt(x)));
    }
    throw new TypeError_(
        `float() argument must be a string or a real number, not '${__pyTypeName(x)}'`,
    );
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
    // Keyword form arrives as the codegen's trailing options object
    // `{base: v}`. F3-r2: unwrap ONLY the plain-object wrapper with an own
    // `base` key — the old unconditional `base = base.base` destroyed a
    // boxed-float or __index__-bearing base object (silently turning it
    // into base 10 instead of CPython's TypeError).
    if (base !== null && typeof base === "object"
        && Object.getPrototypeOf(base) === Object.prototype
        && Object.prototype.hasOwnProperty.call(base, "base")) {
        base = base.base;
    }
    // F3-r2 (v0.2.4): explicit-base form — CPython validates the BASE first
    // (via __index__: int('101', 2.0) → "'float' object cannot be interpreted
    // as an integer", int('101', 99) → the base-range ValueError), THEN
    // requires a str/bytes receiver (int(7, 2) → "int() can't convert
    // non-string with explicit base"; the old code ran the numeric branch
    // before ever looking at base, so int(7, 2) silently returned 7).
    let b = 10;
    if (base !== undefined) {
        b = __pyAsIndexInt(base);
        if (typeof b === "bigint") b = Number(b);
        if (b !== 0 && (b < 2 || b > 36)) {
            throw new ValueError("int() base must be >= 2 and <= 36, or 0");
        }
        if (typeof x !== "string" && !(x instanceof String) && __pyBytesKind(x) === null) {
            throw new TypeError_("int() can't convert non-string with explicit base");
        }
    }
    if (typeof x === "boolean") return x ? 1 : 0;
    if (typeof x === "bigint") return x;
    // Option-B spike: unwrap a boxed float before the number branch.
    if (x != null && x.__pyfloat__ === true) x = x.valueOf();
    // F3 (v0.2.4): bytes-like → decode and parse like a string
    // (int(b'12') → 12, int(b'ff', 16) → 255), with the b'…' repr
    // preserved in the ValueError message.
    let __bytesRepr = null;
    const __dec = __bytesArgDecode(x);
    if (__dec !== null) {
        __bytesRepr = __dec.repr;
        x = __dec.s;
    }
    if (typeof x === "number") {
        if (Number.isNaN(x)) throw new ValueError("cannot convert float NaN to integer");
        if (!Number.isFinite(x)) throw new OverflowError("cannot convert float infinity to integer");
        // Authority: canonical int form — Number iff safe, else exact BigInt
        // (int(1e300) is CPython's exact huge int, not an unsafe Number).
        const t = Math.trunc(x);
        return Number.isSafeInteger(t) ? t : BigInt(t);
    }
    if (typeof x === "string" || x instanceof String) {
        // r2 should-fix: bytes strip only ASCII whitespace (see pyFloat) —
        // int(b'\xa012') raises like CPython; int('\xa012') still parses.
        const t = __bytesRepr !== null
            ? x.replace(/^[\t\n\v\f\r ]+|[\t\n\v\f\r ]+$/g, "")
            : x.trim();
        const m = /^([+-]?)([0-9a-zA-Z_]+)$/.exec(t);
        const bad = () => new ValueError(
            `invalid literal for int() with base ${b}: ${__bytesRepr ?? pyRepr(String(x))}`);
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
    // F3-r2 (v0.2.4): the Python numeric protocol, not a valueOf heuristic —
    // PyNumber_Long order: __int__ first (validated: the result must BE an
    // int — an __int__ returning 1.5 raises "__int__ returned non-int (type
    // float)"; a bool result is accepted as its int value, bool ⊂ int), then
    // __index__ (validated by __pyAsIndexInt). Decimal/Fraction implement
    // exact BigInt-backed __int__ (stdlib) — no more 2^53 precision loss
    // through Number(). A bare-valueOf object (the r1 __numProtoOk
    // allowance) raises the receiver TypeError like any protocol-less value.
    if (x != null && typeof x.__int__ === "function") {
        const r = x.__int__();
        if (typeof r === "boolean") return r ? 1 : 0;
        if (typeof r === "bigint") return r;
        if (typeof r === "number" && Number.isInteger(r)
            && (r == null || r.__pyfloat__ !== true)) {
            return r;
        }
        throw new TypeError_(`__int__ returned non-int (type ${__pyTypeName(r)})`);
    }
    if (x != null && x.__pyfloat__ !== true && typeof x.__index__ === "function") {
        return __pyAsIndexInt(x);
    }
    throw new TypeError_(
        `int() argument must be a string, a bytes-like object or a real number, not '${__pyTypeName(x)}'`,
    );
}

/**
 * Python `divmod(a, b)` → tuple `(a // b, a % b)` with CPython's
 * floor-division/sign-of-divisor semantics and error messages
 * (issue #90).
 */
export function pyDivmod(a, b) {
    if ((__isFloat(a) || __isFloat(b)) && Number(b) === 0) {
        // CPython 3.14 unified every ZeroDivisionError from a division/modulo
        // to the single message "division by zero" (was "float divmod()").
        throw new ZeroDivisionError("division by zero");
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
        const an = (fctx & 1) ? "float" : __pyTypeName(a);
        const bn = (fctx & 2) ? "float" : __pyTypeName(b);
        throw new TypeError_(
            `unsupported operand type(s) for ${op}: '${an}' and '${bn}'`,
        );
    }
}

export function pyBitOr(a, b, fctx) {
    if (a instanceof Set && b instanceof Set) return pySetUnion(a, b);
    if (a != null && typeof a.__or__ === "function") return a.__or__(b);
    if (b != null && typeof b.__ror__ === "function") return b.__ror__(a);
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
    if (b != null && typeof b.__rand__ === "function") return b.__rand__(a);
    __reqBitInt("&", a, b, fctx); // #320/#343
    if (__fits32(a) && __fits32(b)) return a & b;
    return __norm(__toBig(a) & __toBig(b));
}

/** Python `^` — set symmetric difference or int bitwise-xor (issue #93). */
export function pyBitXor(a, b, fctx) {
    if (a instanceof Set && b instanceof Set) return pySetSymmetricDifference(a, b);
    if (a != null && typeof a.__xor__ === "function") return a.__xor__(b);
    if (b != null && typeof b.__rxor__ === "function") return b.__rxor__(a);
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
        const an = fctx ? "float" : __pyTypeName(a);
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
    if (b != null && typeof b.__rlshift__ === "function") return b.__rlshift__(a);
    __reqBitInt("<<", a, b, fctx); // #320/#343
    // CPython raises for a negative count (`1 << -1`); JS BigInt `<<` with a
    // negative count silently shifts the OTHER way (1n << -1n === 0n).
    const bl = __toBig(b);
    if (bl < 0n) throw new ValueError("negative shift count");
    return __norm(__toBig(a) << bl);
}
export function pyShiftRight(a, b, fctx) {
    if (a != null && typeof a.__rshift__ === "function") return a.__rshift__(b);
    if (b != null && typeof b.__rrshift__ === "function") return b.__rrshift__(a);
    __reqBitInt(">>", a, b, fctx); // #320/#343
    // CPython raises for a negative count (`8 >> -2`); see pyShiftLeft.
    const br = __toBig(b);
    if (br < 0n) throw new ValueError("negative shift count");
    return __norm(__toBig(a) >> br);
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
    // Option-B spike: boxed floats compare by value (8.0 == 8 → True;
    // two distinct boxes of the same value are equal).
    if (a.__pyfloat__ === true) a = a.valueOf();
    if (b.__pyfloat__ === true) b = b.valueOf();
    if (a === b) return true;
    // Numeric cross-representation: BigInt int vs Number float/int, plus bool
    // (Python `bool` ⊂ `int`, so `True == 1`, `False == 0`). JS loose `==`
    // matches across all three (5n == 5, 5n == 5.0, true == 1, false == 0).
    const numish = (x) =>
        typeof x === "bigint" || typeof x === "number" || typeof x === "boolean";
    if (numish(a) && numish(b)) {
        return a == b;
    }
    // Bound-method wrappers (S1): CPython methods compare equal when they
    // wrap the same __func__ and the same __self__ (`a.f == a.f` → True even
    // though each access makes a fresh wrapper, so `a.f is a.f` stays False).
    // A bound wrapper never equals a plain function or anything else.
    if (typeof a === "function" && typeof b === "function") {
        const fa = a.__pyboundfunc__ ?? a;
        const fb = b.__pyboundfunc__ ?? b;
        return fa === fb && (a.__pyboundself__ ?? null) === (b.__pyboundself__ ?? null);
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
    // bytes/bytearray: CONTENT equality, cross-type like CPython
    // (`b'a' == bytearray(b'a')` is True; item 5 of the 0.2.2 hold — the old
    // fallthrough to `a === b` made ALL distinct bytes objects unequal, even
    // `b'ab' == b'ab'`). A bytes-like never equals a str/list/int (falls
    // through to the shape checks below, ultimately `a === b` → false).
    if (a instanceof Uint8Array && b instanceof Uint8Array) {
        if (a.length !== b.length) return false;
        for (let i = 0; i < a.length; i++) {
            if (a[i] !== b[i]) return false;
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
            // r6: __pyOwnKeys everywhere — a dict differing only in a
            // Symbol-keyed entry must compare UNEQUAL, and two dicts with the
            // same Symbol entry must compare equal.
            const aLen = aMap ? a.size : __pyOwnKeys(a).length;
            const bLen = bMap ? b.size : __pyOwnKeys(b).length;
            if (aLen !== bLen) return false;
            for (const [k, v] of (aMap ? a.entries() : __pyOwnKeys(a).map((k) => [k, a[k]]))) {
                let bHas, bv;
                if (bMap) { bHas = b.has(k); bv = b.get(k); }
                else if (typeof k !== "string" && typeof k !== "symbol") return false; // plain dicts hold string/symbol keys
                else { bHas = Object.prototype.hasOwnProperty.call(b, k); bv = b[k]; }
                if (!bHas || !pyEq(v, bv)) return false;
            }
            return true;
        }
    }
    return a === b;
}

// ═══ THE NUMERIC VALUE-BOUNDARY AUTHORITY (#460/#461/#464, umbrella #38) ═══
//
// PythScribe's runtime carries FOUR coexisting numeric value forms:
//   1. Number-as-int   — a Python int within the safe-integer range;
//   2. BigInt-as-int   — a Python int beyond 2**53-1 (exact, hybrid);
//   3. Number-as-float — a Python float with a non-integer value (3.14),
//      plus NaN/±Infinity;
//   4. PyFloat box     — a Python float with an integer value (8.0), branded
//      __pyfloat__ so it stays distinguishable from int 8 (Option B).
//
// SEMANTICS are CPython's (arbitrary-precision int; float distinct from
// int); the REPRESENTATION strategy is the Pyodide-style number↔bigint
// hybrid. This block is the ONE place that decides representation:
//
//   * classification      — __isFloat (float ⇔ brand or non-integer Number;
//                           an integer-valued Number is ALWAYS an int here:
//                           in-`.ps` float values that are integer-valued
//                           are boxed at creation by __pyF, so an unboxed
//                           integer Number can only be an int — including
//                           inbound native-JS values past 2**53, which the
//                           old !isSafeInteger classifier mislabeled float
//                           and rounded, #464);
//   * promotion/demotion  — __intBin (int ops promote to BigInt whenever an
//                           operand or result leaves the safe range, #460)
//                           and __norm (demote BigInt back to Number when
//                           safe);
//   * int→float coercion  — __reqNum (all four forms → native double; a
//                           BigInt beyond double range raises OverflowError
//                           like CPython) — this is also the coerce-on-mix
//                           path: a BigInt meeting a float NEVER reaches a
//                           raw JS operator, so "Cannot mix BigInt" cannot
//                           be thrown by Python-level arithmetic (#461);
//   * float box/unbox     — __pyF (box iff integer-valued) and __pyJs
//                           (unbox at native-JS sinks).
//
// No op site — runtime helper, stdlib module, codegen fast path, or WASM
// glue — may re-decide any of this locally: stdlib modules import these
// helpers (see stdlib/math.js), the codegen's inline runtime mirrors this
// block verbatim (PY_ARITH_JS in crates/pyths_codegen_js/src/emit.rs,
// gated by inline_runtime_parity.rs), and the WASM glue's __i64ToJs/
// __f64Box are the same __norm/__pyF rules by definition. The guard for
// all of this is runtime/src/numeric_boundary.test.mjs (the CPython-
// verified 4-form × op matrix) plus the single-authority freeze in
// inline_runtime_parity.rs.
const MAX_SAFE = 9007199254740991n;
// Float ⇔ __pyfloat__ brand OR non-integer Number (NaN/±Infinity included:
// Number.isInteger rejects them). An integer-valued Number of ANY magnitude
// is an int — exactness for the unsafe ones is restored by __intBin's
// BigInt promotion (#460/#464). (The old `!Number.isSafeInteger` rule
// pre-dates Option-B boxing; once integer-valued floats carry the brand,
// classifying unsafe integer Numbers as float only corrupted inbound ints.)
const __isFloat = (x) =>
    (typeof x === "number" && !Number.isInteger(x))
    || (x != null && x.__pyfloat__ === true);
// Coerce any numeric form to a native double for float-context arithmetic
// and float-only native sinks. A BigInt too large to represent as a double
// raises OverflowError, matching CPython's "int too large to convert to
// float" (the int→float conversion, distinct from float-arithmetic
// overflow which yields inf). #319.
const __reqNum = (x) => {
    if (typeof x === "bigint") {
        const n = Number(x);
        if (!isFinite(n)) throw new OverflowError("int too large to convert to float");
        return n;
    }
    if (x != null && x.__pyfloat__ === true) return x.valueOf();
    return Number(x);
};
const __toBig = (x) => (typeof x === "bigint" ? x : BigInt(x));
const __norm = (big) =>
    big >= -MAX_SAFE && big <= MAX_SAFE ? Number(big) : big;
// Exported for stdlib modules (math.js, …) and the parity battery: route
// EVERY numeric boundary through the authority instead of local ad-hoc
// copies. (__intBin is exported below, after its definition.)
export { __reqNum, __toBig, __norm, __isFloat };

const __numeric = (x) =>
    typeof x === "number" || typeof x === "bigint"
    || (x != null && x.__pyfloat__ === true);

// Option B (minimal): boxed Python float for INTEGER-VALUED floats only.
// A non-integer float (3.14) stays a native Number — it is already
// distinguishable from an int by `Number.isInteger`. Only the ambiguous
// case (8.0 vs 8) is boxed. Extends Number so native JS numeric APIs
// (Math.sin, toFixed, ToNumber coercion at the WASM boundary) keep
// working via the [[NumberData]] slot / valueOf, while the __pyfloat__
// brand makes an integer-valued float (8.0) distinguishable from int 8.
export class PyFloat extends Number {
    get __pyfloat__() { return true; }
    __repr__() { return pyFormatFloat(this.valueOf()); }
    __str__() { return pyFormatFloat(this.valueOf()); }
    __bool__() { return this.valueOf() !== 0; }
    __neg__() { return new PyFloat(-this.valueOf()); }
    __pos__() { return this; }
    __abs__() { return new PyFloat(Math.abs(this.valueOf())); }
}
// THE single float-box authority: box iff integer-valued (8.0, -0.0, 1e300);
// NaN/Infinity/3.14 stay native (Number.isInteger is false for all three).
export const __pyF = (v) => (Number.isInteger(v) ? new PyFloat(v) : v);
// THE single unbox authority for native-JS boundaries (typeof-dispatching
// sinks: Array(n), React style values, DOM/native call args). A non-boxed
// value passes through untouched.
export const __pyJs = (v) => (v != null && v.__pyfloat__ === true ? v.valueOf() : v);

// Exact integer binary op (authority: promotion). Native Number math only
// when BOTH operands AND the result are safe integers; everything else —
// a BigInt operand, an inbound unsafe-integer Number (#460/#464), or a
// result crossing 2**53 — recomputes exactly in BigInt and __norm-demotes.
function __intBin(a, b, numOp, bigOp) {
    if (typeof a === "number" && typeof b === "number"
        && Number.isSafeInteger(a) && Number.isSafeInteger(b)) {
        const r = numOp(a, b);
        if (Number.isSafeInteger(r)) return r; // common fast path
        return __norm(bigOp(BigInt(a), BigInt(b))); // overflow → exact BigInt
    }
    return __norm(bigOp(__toBig(a), __toBig(b)));
}
export { __intBin };

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
        if (fctx || __isFloat(a) || __isFloat(b)) return __pyF(__reqNum(a) + __reqNum(b));
        return __intBin(a, b, (x, y) => x + y, (x, y) => x + y);
    }
    if (a != null && typeof a.__add__ === "function") return a.__add__(b);
    if (b != null && typeof b.__radd__ === "function") return b.__radd__(a);
    // bytes + bytes -> concat (autotester byte_arrays).
    if (a instanceof Uint8Array && b instanceof Uint8Array) {
        // Item 5 (0.2.2 hold): the result TYPE follows the LEFT operand, like
        // CPython — bytearray + bytes-like -> bytearray, bytes + bytearray ->
        // bytes. `ba += b"…"` lowers to `ba = pyAdd(ba, …)`, so this is what
        // keeps a bytearray a bytearray (the old always-PyBytes result
        // silently REBOUND it to immutable bytes: wrong repr, and every later
        // mutator — pop/append/… — fell into the dict path, e.g. KeyError).
        // Duck-typed via the bytearray mutator surface so a bytes-only
        // program never references the bytearray class (extraction-safe).
        if (typeof a.copy === "function" && typeof a.extend === "function") {
            const out = a.copy();
            out.extend(b);
            return out;
        }
        const out = new PyBytes(a.length + b.length);
        out.set(a, 0); out.set(b, a.length);
        return out;
    }
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
    // crit-12: only str+str concatenates — never JS coercion ("a" + 1 → "a1").
    if (typeof a === "string" && typeof b === "string") return a + b;
    // E2 (#466): no valid combination matched — the operand-type authority
    // decides the CPython TypeError. (The old raw `return a + b` string-
    // coerced the leftovers: b"abc" + 1 → "97,98,991", [1] + 1 → "11",
    // set/dict + set/dict → "[object …][object …]". Also subsumes #322's
    // None guard and upgrades crit-12's message to CPython's exact
    // 'can only concatenate str (not "X") to str' form.)
    __binOpTypeError("+", a, b);
}

/**
 * Python subtraction with operator overloading.
 */
export function pySub(a, b) {
    // Wave-15 F4: bool ⊂ int (see __boolNum) — exact bool+BigInt arithmetic.
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if (__numeric(a) && __numeric(b)) {
        if (__isFloat(a) || __isFloat(b)) return __pyF(__reqNum(a) - __reqNum(b));
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
// PEP 465 `a @ b` (matmul): pure dunder dispatch — CPython defines no
// builtin operands for `@`, so anything without __matmul__/__rmatmul__
// raises the CPython-shaped TypeError.
export function pyMatMul(a, b) {
    if (a != null && typeof a.__matmul__ === "function") {
        const r = a.__matmul__(b);
        if (r !== undefined) return r;
    }
    if (b != null && typeof b.__rmatmul__ === "function") {
        const r = b.__rmatmul__(a);
        if (r !== undefined) return r;
    }
    throw new TypeError_(
        `unsupported operand type(s) for @: '${__pyTypeName(a)}' and '${__pyTypeName(b)}'`,
    );
}

// In-place matmul `a @= b`: CPython tries __imatmul__ on the target first
// and only falls back to the binary `__matmul__`/`__rmatmul__` protocol when
// the type defines no in-place hook. The aug-assign codegen lowers
// `n @= m` to `n = pyIMatMul(n, m)`, so a defined __imatmul__ (which may
// mutate in place and return self) wins over __matmul__.
export function pyIMatMul(a, b) {
    if (a != null && typeof a.__imatmul__ === "function") {
        const r = a.__imatmul__(b);
        if (r !== undefined) return r;
    }
    return pyMatMul(a, b);
}

// ═══ CPython in-place augmented-assign protocol (aliasing soundness) ═══
//
// `x OP= v` lowers to `x = pyIOp(x, v)`. For a MUTABLE builtin target the
// in-place helper mutates the SAME object and returns it — the rebind is a
// no-op and every alias observes the change (`y = x; x += [5]` leaves
// `x is y` True, like CPython's __iadd__/__ior__/... slots). For immutables
// (int/float/str/tuple/bigint/...) there is no in-place slot: fall back to
// the binary value helper, which returns a fresh object (a genuine rebind —
// exactly CPython's fallback). User classes get their __iOP__ dunder
// dispatched first (may mutate in place and return self), then the binary
// protocol — the same shape as pyIMatMul above.
//
// Deliberately NOT in-place here (documented residuals, matching the binary
// helpers' surface): bytearray `+=`/`*=` still rebinds via pyAdd/pyMul (the
// result keeps its bytearray-ness but not its identity), and `list += <non-
// list/tuple iterable>` keeps the binary concat TypeError rather than
// CPython's extend-any-iterable.

/** Python `+=` — list extends in place (accepts a list or tuple rhs, both
 * concat-legal in CPython's list.__iadd__ ⊂ extend); immutables rebind. */
export function pyIAdd(a, b, fctx) {
    if (a != null && typeof a.__iadd__ === "function") {
        const r = a.__iadd__(b);
        if (r !== undefined) return r;
    }
    if (Array.isArray(a) && !a.__pytuple__ && Array.isArray(b)) {
        for (const v of b) a.push(v);
        return a;
    }
    return pyAdd(a, b, fctx);
}

/** Python `-=` — set difference_update in place; numbers rebind. */
export function pyISub(a, b) {
    if (a != null && typeof a.__isub__ === "function") {
        const r = a.__isub__(b);
        if (r !== undefined) return r;
    }
    if (a instanceof Set && b instanceof Set) {
        // frozenset (branded) REBINDS — and the rebound result is RE-BRANDED
        // frozen, so a SECOND in-place op on it also rebinds (CPython's
        // frozenset OP= yields a frozenset; an unbranded result would make
        // op #2 mutate in place and re-corrupt aliases). Only a real `set`
        // mutates in place.
        if (a.__pyfrozen__ === true) return pyFrozensetOf(pySub(a, b));
        for (const v of b) a.delete(v);
        return a;
    }
    return pySub(a, b);
}

/** Python `*=` — list repeats in place (int/bool count, CPython
 * list.__imul__); everything else rebinds via pyMul. */
export function pyIMul(a, b, fctx) {
    if (a != null && typeof a.__imul__ === "function") {
        const r = a.__imul__(b);
        if (r !== undefined) return r;
    }
    if (Array.isArray(a) && !a.__pytuple__) {
        const n = __mulRepCount(b);
        if (n !== null) {
            const src = a.slice();
            a.length = 0;
            for (let i = 0; i < n; i++) for (const v of src) a.push(v);
            return a;
        }
    }
    return pyMul(a, b, fctx);
}

/** Python `|=` — set in-place union / dict update (dict.__ior__); ints
 * rebind via pyBitOr. */
export function pyIBitOr(a, b, fctx) {
    if (a != null && typeof a.__ior__ === "function") {
        const r = a.__ior__(b);
        if (r !== undefined) return r;
    }
    if (a instanceof Set && b instanceof Set) {
        // frozenset (branded) rebinds, RE-BRANDED frozen — see pyISub.
        if (a.__pyfrozen__ === true) return pyFrozensetOf(pyBitOr(a, b));
        for (const v of b) a.add(v);
        return a;
    }
    // Same dict-shape admission as pyBitOr's merge branch (#83): both sides
    // Map-backed or plain-object dicts — but update the LEFT one in place.
    if ((a instanceof Map || __isPlainObject(a)) && (b instanceof Map || __isPlainObject(b))) {
        pyUpdate(a, b);
        return a;
    }
    return pyBitOr(a, b, fctx);
}

/** Python `&=` — set intersection_update in place; ints rebind. */
export function pyIBitAnd(a, b, fctx) {
    if (a != null && typeof a.__iand__ === "function") {
        const r = a.__iand__(b);
        if (r !== undefined) return r;
    }
    if (a instanceof Set && b instanceof Set) {
        // frozenset (branded) rebinds, RE-BRANDED frozen — see pyISub.
        if (a.__pyfrozen__ === true) return pyFrozensetOf(pyBitAnd(a, b));
        const keep = pySetIntersection(a, b);
        a.clear();
        for (const v of keep) a.add(v);
        return a;
    }
    return pyBitAnd(a, b, fctx);
}

/** Python `^=` — set symmetric_difference_update in place; ints rebind. */
export function pyIBitXor(a, b, fctx) {
    if (a != null && typeof a.__ixor__ === "function") {
        const r = a.__ixor__(b);
        if (r !== undefined) return r;
    }
    if (a instanceof Set && b instanceof Set) {
        // frozenset (branded) rebinds, RE-BRANDED frozen — see pyISub.
        if (a.__pyfrozen__ === true) return pyFrozensetOf(pyBitXor(a, b));
        const out = pySetSymmetricDifference(a, b);
        a.clear();
        for (const v of out) a.add(v);
        return a;
    }
    return pyBitXor(a, b, fctx);
}

export function pyMul(a, b, fctx) {
    // Wave-15 F4: bool ⊂ int (see __boolNum) — exact bool·BigInt arithmetic.
    // (E2: pyMul previously lacked this preamble; bool·int only worked by
    // falling into the tail's BigInt(true) coercion, and bool with a
    // sequence crashed with a raw JS SyntaxError instead of replicating.)
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    // Fast path for pure numeric operands (string/list replication below).
    if (__numeric(a) && __numeric(b)) {
        // #319: float context coerces a BigInt operand to float — too large →
        // OverflowError, like CPython `(10**400) * 1.0`.
        if (fctx || __isFloat(a) || __isFloat(b)) return __pyF(__reqNum(a) * __reqNum(b));
        return __intBin(a, b, (x, y) => x * y, (x, y) => x * y);
    }
    if (a != null && typeof a.__mul__ === "function") return a.__mul__(b);
    if (b != null && typeof b.__rmul__ === "function") return b.__rmul__(a);
    // E2 (#466 class): sequence replication — bytes/str/list/tuple times an
    // INT (or bool) count, exactly one side a sequence. The count validates
    // through __mulRepCount (ONE rule); any non-int count or seq·seq pair
    // falls through to the operand-type authority, which raises CPython's
    // "can't multiply sequence by non-int of type 'X'". (The old branches
    // admitted ANY JS number as count: b"ab" * 2.5 → RangeError crash,
    // "ab" * 2.5 → silent "abab" via JS repeat-truncation, [1] * 2.5 →
    // silent 3-element list; "ab" * -1 crashed with RangeError instead of
    // returning "".)
    {
        const aSeq = typeof a === "string" || Array.isArray(a) || a instanceof Uint8Array;
        const bSeq = typeof b === "string" || Array.isArray(b) || b instanceof Uint8Array;
        if (aSeq !== bSeq) {
            const n = __mulRepCount(aSeq ? b : a);
            if (n !== null) {
                const s = aSeq ? a : b;
                if (s instanceof Uint8Array) return __bytesRepeat(s, n);
                if (typeof s === "string") return s.repeat(Math.max(0, n));
                const result = [];
                for (let i = 0; i < n; i++) result.push(...s);
                // tuple * int stays a tuple, like CPython (mirrors pyAdd's
                // tuple-concat marker handling).
                if (s.__pytuple__) Object.defineProperty(result, "__pytuple__", { value: true, enumerable: false });
                return result;
            }
        }
    }
    // E2 (#466): no valid combination matched — the operand-type authority
    // decides the CPython TypeError. (The old tail fell into __intBin, so
    // "a" * "b" crashed with a raw JS SyntaxError from BigInt("a") and
    // set * int with "[object Set]"-flavored garbage; subsumes #322.)
    __binOpTypeError("*", a, b);
}

/**
 * Python division — true division always returns float.
 *
 * (The old third parameter `floatDiv` — codegen's static-float hint, which
 * only ever selected 3.12's "float division by zero" wording — was removed
 * with the CPython 3.14 oracle bump: the message is unified and the result
 * is ALWAYS float, so the hint carried no remaining semantics. A stale
 * 3-arg call site is harmless in JS but should be cleaned up.)
 */
export function pyDiv(a, b) {
    // Wave-15 F4: bool ⊂ int (see __boolNum) — exact bool+BigInt arithmetic.
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if ((!__numeric(a) || !__numeric(b)) && a != null && typeof a.__truediv__ === "function") return a.__truediv__(b);
    if ((!__numeric(a) || !__numeric(b)) && b != null && typeof b.__rtruediv__ === "function") return b.__rtruediv__(a);
    __reqArithNum("/", a, b); // #322 + wave-15 F9 (str/list no longer coerce)
    // #319: divisor converts to float too — a too-large BigInt → OverflowError.
    const bn = __reqNum(b);
    if (bn === 0) {
        // CPython 3.14 ORACLE: the old float/int distinction (F4 — `1/0`
        // "division by zero" vs `1.0/0.0` "float division by zero") was
        // removed upstream; every ZeroDivisionError message is now the single
        // "division by zero" (which is why the floatDiv hint parameter is gone).
        throw new ZeroDivisionError("division by zero");
    }
    // #319: true division always produces a float, so a BigInt operand must
    // convert to float — too large → OverflowError (CPython `(10**400)/1.0`).
    return __pyF(__reqNum(a) / bn);
}

/**
 * Python floor division //.
 */
export function pyFloorDiv(a, b) {
    // Wave-15 F4: bool ⊂ int (see __boolNum) — exact bool+BigInt arithmetic.
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if ((!__numeric(a) || !__numeric(b)) && a != null && typeof a.__floordiv__ === "function") return a.__floordiv__(b);
    if ((!__numeric(a) || !__numeric(b)) && b != null && typeof b.__rfloordiv__ === "function") return b.__rfloordiv__(a);
    // Wave-15 F9 SHIPPING BUG: the old None-only guard let numeric strings
    // and singleton arrays fall into the BigInt fallback (`"4".strip("x") //
    // 2` → 2, `"4".split(",") // 2` → 2); CPython raises TypeError. The Lean
    // wave-15 model (`SgVal.asArith`: gstr/glist → none) was already correct
    // — this makes the shipped helper match it.
    __reqArithNum("//", a, b); // #322 + wave-15 F9
    if (__isFloat(a) || __isFloat(b)) {
        const x = Number(a), y = Number(b);
        if (y === 0) throw new ZeroDivisionError("division by zero"); // CPython 3.14: unified (was "float floor division by zero")
        // CPython float_divmod: floor toward -inf, correcting fmod's sign, so
        // 1.0 // 0.1 == 9.0 (not Math.floor(1.0/0.1) == 10). JS `%` is C fmod.
        const mod = x % y;
        let div = (x - mod) / y;
        if (mod !== 0 && (y < 0) !== (mod < 0)) div -= 1;
        let fd = Math.floor(div);
        if (div - fd > 0.5) fd += 1;
        return __pyF(fd);
    }
    if (Number(b) === 0) throw new ZeroDivisionError("division by zero"); // CPython 3.14: unified (was "integer division or modulo by zero")
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
    if ((!__numeric(a) || !__numeric(b)) && b != null && typeof b.__rmod__ === "function") return b.__rmod__(a);
    // E3 (was wave-15 narrowed): `%` with a string LEFT operand is
    // printf-style formatting — now implemented to the CPython spec by
    // pyStrMod (runtime.js), the same render engine as f-strings/format().
    // (The old honest NotImplementedError is retired; `"4" % 2` raises
    // CPython's "unsupported format character" ValueError family or
    // formats, exactly as the oracle does.)
    if (typeof a === "string") {
        return pyStrMod(a, b);
    }
    __reqArithNum("%", a, b); // #322 + wave-15 F9 (str/list no longer coerce)
    if (__isFloat(a) || __isFloat(b)) {
        const bf = __reqNum(b);
        if (bf === 0) throw new ZeroDivisionError("division by zero"); // CPython 3.14: unified (was "float modulo by zero")
        // Sign-of-divisor correction WITHOUT the `(+y)%y` re-mod: at a huge
        // divisor, `(2.5 % 2**53) + 2**53` rounds and the final `%` returns
        // a corrupted residue (guard-matrix catch). Add the divisor only
        // when the sign actually needs correcting — no intermediate can
        // then leave the exact range. Matches CPython float_mod.
        let m = __reqNum(a) % bf;
        if (m !== 0 && (m < 0) !== (bf < 0)) m += bf;
        return __pyF(m);
    }
    // CPython 3.14 ORACLE: every division/modulo ZeroDivisionError message was
    // unified to "division by zero" (3.12 said "integer modulo by zero" for
    // `%` and "integer division or modulo by zero" for `//`/divmod).
    if (Number(b) === 0) throw new ZeroDivisionError("division by zero");
    // Same sign-correction form for ints: `((x % y) + y) % y` overflowed
    // 2**53 in the INTERMEDIATE (x%y + y can reach 2y) and rounded before
    // the final `%`, silently corrupting results near MAX_SAFE.
    const __modFix = (x, y) => {
        let m = x % y;
        if (m !== 0 && (m < 0) !== (y < 0)) m += y;
        return m;
    };
    return __intBin(a, b, __modFix, (x, y) => {
        let m = x % y;
        if (m !== 0n && (m < 0n) !== (y < 0n)) m += y;
        return m;
    });
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
            const an = __reqNum(a), bn = __reqNum(b);
            // CPython: zero (int or float, -0.0 included) to a negative power
            // is ZeroDivisionError — the old raw `0 ** -1` produced Infinity,
            // which the overflow guard mislabeled OverflowError. CPython 3.14
            // reworded the message to "zero to a negative power" (3.12 said
            // "0.0 cannot be raised to a negative power").
            if (an === 0 && bn < 0) {
                throw new ZeroDivisionError("zero to a negative power");
            }
            const r = an ** bn;
            if (!isFinite(r) && isFinite(an) && isFinite(bn)) {
                throw new OverflowError("(34, 'Result too large')");
            }
            return __pyF(r);
        }
        return __intBin(a, b, (x, y) => x ** y, (x, y) => x ** y);
    }
    if (a != null && typeof a.__pow__ === "function") return a.__pow__(b);
    if (b != null && typeof b.__rpow__ === "function") return b.__rpow__(a);
    // Wave-15 F9 — CPython's `**` message names both spellings.
    __reqArithNum("** or pow()", a, b); // #322 + wave-15 F9
    return Number(a) ** Number(b); // defensive: guard admits only num/bool, F4 coerced bools above
}

// autotester: modular inverse via extended Euclid, for pow(a, -e, m).
// CPython raises ValueError when the base is not invertible (gcd != 1).
function __modInverse(a, m) {
    const absM = m < 0n ? -m : m;
    let [old_r, r] = [((a % absM) + absM) % absM, absM];
    let [old_s, s] = [1n, 0n];
    while (r !== 0n) {
        const q = old_r / r;
        [old_r, r] = [r, old_r - q * r];
        [old_s, s] = [s, old_s - q * s];
    }
    if (old_r !== 1n) {
        throw new ValueError("base is not invertible for the given modulus");
    }
    return ((old_s % absM) + absM) % absM;
}

/**
 * Python BUILTIN `pow()`. The 2-arg form (and the explicit `pow(a, b, None)`)
 * delegates to the `**` operator helper above; the 3-arg form is modular
 * exponentiation with CPython's rules: all three arguments must be integers
 * (TypeError otherwise), the modulus must be nonzero (ValueError), a negative
 * exponent requires an invertible base (ValueError), and the result carries
 * the SIGN of the modulus. Distinct from pyPow because the operator helper's
 * third parameter is the internal float-context flag — mapping the builtin
 * onto it silently ignored the modulus (pow(2, 10, 1000) → 1024, want 24).
 */
export function pyPowBuiltin(a, b, m) {
    if (m === undefined || m === null) return pyPow(a, b);
    const asBig = (v) => {
        if (typeof v === "boolean") return v ? 1n : 0n;
        if (typeof v === "bigint") return v;
        if (typeof v === "number" && Number.isSafeInteger(v)) return BigInt(v);
        throw new TypeError_(
            "pow() 3rd argument not allowed unless all arguments are integers",
        );
    };
    const mod = asBig(m);
    let base = asBig(a);
    let exp = asBig(b);
    if (mod === 0n) throw new ValueError("pow() 3rd argument cannot be 0");
    const absMod = mod < 0n ? -mod : mod;
    if (exp < 0n) {
        base = __modInverse(base, mod);
        exp = -exp;
    }
    // Right-to-left square-and-multiply over BigInt (exact at any size).
    let result = 1n % absMod;
    base = ((base % absMod) + absMod) % absMod;
    while (exp > 0n) {
        if (exp & 1n) result = (result * base) % absMod;
        base = (base * base) % absMod;
        exp >>= 1n;
    }
    if (mod < 0n && result !== 0n) result += mod; // sign follows the modulus
    return __norm(result);
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


// Python set ordering: `a <= b` is the subset test (`<` proper subset).
function __setLe(a, b) {
    for (const x of a) { if (!b.has(x)) return false; }
    return true;
}

// F1 (v0.2.4): bytes/bytearray order lexicographically BY BYTE VALUE, like
// CPython (b"\x02" < b"\x10"). The old raw-`<` fallback compared the
// comma-joined decimal toString ("2" vs "16" → wrong answer, silently).
function __bytesLt(a, b) {
    const n = Math.min(a.length, b.length);
    for (let i = 0; i < n; i++) {
        if (a[i] !== b[i]) return a[i] < b[i];
    }
    return a.length < b.length;
}

// F1 (v0.2.4): THE cross-type ordering guard. CPython's rich comparisons
// raise TypeError when the operands are not order-compatible (`1 < 'a'`,
// `None < 1`, `[1] < 1`, `{} < {}`, unordered class instances); the old
// final fallback leaked JS `<` coercion and returned a silent-wrong
// boolean. Runs AFTER dunder dispatch and after the legitimate typed arms
// (set subset, sequence lexicographic, bytes), so it only fires where
// CPython itself would raise. Ordering is legal between:
//   * any two numerics — int/float/bool/boxed-float mix freely
//     (__arithNumOk is the same numeric-operand authority the arithmetic
//     guards use), and
//   * two strings.
// Everything else raises with CPython's message shape. pyLt is the ONE
// comparison authority — heapq/bisect and the __seqLt element recursion
// already route here, so the guard covers those derived surfaces in one
// place. (runtime.js's pySorted/pyListSort/__minmax still carry local
// raw-`<` comparators and must be consolidated onto pyLt/pyGt — tracked
// as the runtime.js half of this F1 class fix.)
function __cmpTypeGuard(op, a, b) {
    if (__arithNumOk(a) && __arithNumOk(b)) return;
    if (typeof a === "string" && typeof b === "string") return;
    throw new TypeError_(
        `'${op}' not supported between instances of `
        + `'${__pyTypeName(a)}' and '${__pyTypeName(b)}'`,
    );
}

// F1-r2 (v0.2.4): list and tuple are NOT order-compatible — both compile to
// JS arrays (distinguished only by the __pytuple__ brand), so the bare
// Array.isArray pair-check silently ordered `[1] < (2,)` where CPython
// raises TypeError. Same-kind pairs (list↔list, tuple↔tuple) still order
// lexicographically; a mixed pair falls through to __cmpTypeGuard, which
// renders CPython's exact 'list'/'tuple' operand names via __pyTypeName.
const __seqKindOk = (a, b) => !!a.__pytuple__ === !!b.__pytuple__;

// r2 should-fix: CPython's rich-comparison protocol tries the RIGHT
// operand's reflected method FIRST when type(b) is a strict subclass of
// type(a) AND overrides the reflected method (a different implementation
// than a's — an inherited copy keeps the normal left-first order). The r1
// dispatch was unconditionally left-first, so `A() < B()` for B(A)
// overriding __gt__ called A.__lt__ where CPython calls B.__gt__.
const __reflFirst = (a, b, aMeth, bMeth) =>
    a !== null && b !== null
    && typeof a === "object" && typeof b === "object"
    && typeof bMeth === "function" && bMeth !== aMeth
    && a.constructor && b.constructor && a.constructor !== b.constructor
    && b instanceof a.constructor;

export function pyLt(a, b) {
    if (a instanceof Set && b instanceof Set) return a.size < b.size && __setLe(a, b);
    if (__reflFirst(a, b, a?.__gt__, b?.__gt__)) return b.__gt__(a);
    if (a != null && typeof a.__lt__ === "function") return a.__lt__(b);
    if (b != null && typeof b.__gt__ === "function") return b.__gt__(a);
    if (Array.isArray(a) && Array.isArray(b) && __seqKindOk(a, b)) return __seqLt(a, b);
    if (a instanceof Uint8Array && b instanceof Uint8Array) return __bytesLt(a, b);
    __cmpTypeGuard("<", a, b);
    return a < b;
}

/**
 * Python less-than-or-equal comparison (see `pyLt` for dispatch order).
 */
export function pyLe(a, b) {
    if (a instanceof Set && b instanceof Set) return __setLe(a, b);
    if (__reflFirst(a, b, a?.__ge__, b?.__ge__)) return b.__ge__(a);
    if (a != null && typeof a.__le__ === "function") return a.__le__(b);
    if (b != null && typeof b.__ge__ === "function") return b.__ge__(a);
    if (Array.isArray(a) && Array.isArray(b) && __seqKindOk(a, b)) return !__seqLt(b, a);
    if (a instanceof Uint8Array && b instanceof Uint8Array) return !__bytesLt(b, a);
    __cmpTypeGuard("<=", a, b);
    return a <= b;
}

/**
 * Python greater-than comparison (see `pyLt` for dispatch order).
 */
export function pyGt(a, b) {
    if (a instanceof Set && b instanceof Set) return b.size < a.size && __setLe(b, a);
    if (__reflFirst(a, b, a?.__lt__, b?.__lt__)) return b.__lt__(a);
    if (a != null && typeof a.__gt__ === "function") return a.__gt__(b);
    if (b != null && typeof b.__lt__ === "function") return b.__lt__(a);
    if (Array.isArray(a) && Array.isArray(b) && __seqKindOk(a, b)) return __seqLt(b, a);
    if (a instanceof Uint8Array && b instanceof Uint8Array) return __bytesLt(b, a);
    __cmpTypeGuard(">", a, b);
    return a > b;
}

/**
 * Python greater-than-or-equal comparison (see `pyLt` for dispatch order).
 */
export function pyGe(a, b) {
    if (a instanceof Set && b instanceof Set) return __setLe(b, a);
    if (__reflFirst(a, b, a?.__le__, b?.__le__)) return b.__le__(a);
    if (a != null && typeof a.__ge__ === "function") return a.__ge__(b);
    if (b != null && typeof b.__le__ === "function") return b.__le__(a);
    if (Array.isArray(a) && Array.isArray(b) && __seqKindOk(a, b)) return !__seqLt(a, b);
    if (a instanceof Uint8Array && b instanceof Uint8Array) return !__bytesLt(a, b);
    __cmpTypeGuard(">=", a, b);
    return a >= b;
}

/**
 * Python not-equal comparison.
 */
export function pyNe(a, b) {
    if (typeof a.__ne__ === "function") return a.__ne__(b);
    return !pyEq(a, b);
}

// F4 (v0.2.4): THE unary operand-type guard. The wave-15 arithmetic guard
// was binary-only — unary minus/plus and abs() never entered its dispatch,
// so `-'a'` → NaN, `-[1]` → -1, `abs(None)` → 0 leaked silent-wrong JS
// coercions where CPython raises TypeError. Runs AFTER dunder dispatch
// (__neg__/__pos__/__abs__ — Decimal/Fraction/PyFloat/PyComplex keep their
// own arms) — a numeric operand (number/bigint/bool, __arithNumOk) passes;
// everything else raises CPython's exact message shape
// ("bad operand type for unary -: 'str'" / "bad operand type for abs(): 'NoneType'").
function __unaryTypeGuard(op, a) {
    if (__arithNumOk(a)) return;
    throw new TypeError_(`bad operand type for ${op}: '${__pyTypeName(a)}'`);
}

/**
 * Python `abs()`. Dispatches `__abs__` when defined (Decimal/Fraction
 * return their own type, not a coerced float) — otherwise falls back to
 * plain `Math.abs`, guarded on numeric operands (F4).
 */
export function pyAbs(x) {
    if (x != null && typeof x.__abs__ === "function") return x.__abs__();
    // Python: abs() of an int of any magnitude stays an exact int; JS
    // Math.abs throws on BigInt ("Cannot convert a BigInt value to a number").
    if (typeof x === "bigint") return x < 0n ? -x : x;
    __unaryTypeGuard("abs()", x);
    return Math.abs(x);
}

/**
 * Python negative unary operator.
 */
export function pyNeg(a) {
    if (a != null && typeof a.__neg__ === "function") return a.__neg__();
    if (typeof a === "bigint") return __norm(-a);
    __unaryTypeGuard("unary -", a);
    return -a;
}

/**
 * Python positive unary operator. Authority (#38): raw JS `+x` throws
 * "Cannot convert a BigInt value to a number" on a large int — `+int` is
 * the identity in Python, at any magnitude. Dispatches `__pos__` (boxed
 * floats return themselves; Decimal keeps its type), keeps int forms
 * unchanged, and raises CPython's TypeError for non-numerics (F4 — the
 * old ToNumber fallback returned NaN for `+'a'`).
 */
export function pyPos(a) {
    if (a != null && typeof a.__pos__ === "function") return a.__pos__();
    if (typeof a === "bigint" || (typeof a === "number" && Number.isInteger(a))) return a;
    if (typeof a === "boolean") return a ? 1 : 0;
    __unaryTypeGuard("unary +", a);
    return +a;
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
    if (obj != null && obj.__pyfloat__ === true) return pyFormatFloat(obj.valueOf());
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
    // Option-B spike: unwrap a boxed float (preserves -0 via valueOf).
    if (n != null && n.__pyfloat__ === true) n = n.valueOf();
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
        items = __pyOwnKeys(iterable); // r6: symbol keys included
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

// FULL_SURFACE #3: CPython's default object repr shows an identity address
// (`<__main__.Foo object at 0x7f9c1a2b3c40>`). JS has no addresses, so hand
// out a stable-per-object SYNTHETIC one (same object → same address for its
// lifetime; distinct live objects → distinct addresses), formatted like a
// CPython pointer (lowercase hex, 12 digits).
const __pyAddrMap = new WeakMap();
let __pyAddrNext = 0x7f6c00000000;
function __pyObjAddr(obj) {
    let a = __pyAddrMap.get(obj);
    if (a === undefined) {
        a = __pyAddrNext;
        __pyAddrNext += 0x40;
        __pyAddrMap.set(obj, a);
    }
    return a.toString(16).padStart(12, "0");
}

// E3: in-flight container repr tracking for the self-reference placeholder.
const __reprSeen = new Set();

export function pyRepr(obj) {
    if (obj === null || obj === undefined) return "None";
    if (typeof obj === "boolean") return obj ? "True" : "False";
    // Option-B spike: any __pyfloat__-branded box (incl. the WASM glue's
    // local brand class) reprs as a Python float.
    if (obj.__pyfloat__ === true) return pyFormatFloat(obj.valueOf());
    // Functions: interned builtin type objects and compiled classes repr as
    // CPython classes (`<class 'int'>` / `<class 'A'>`); plain functions get
    // the CPython function form (the address is synthetic — JS has none).
    if (typeof obj === "function") {
        if (obj.__pytype__) return `<class '${obj.__name__}'>`;
        if (/^class[\s{(]/.test(Function.prototype.toString.call(obj))) {
            return `<class '${obj.__name__ || obj.name}'>`;
        }
        return `<function ${obj.__name__ || obj.name || "<lambda>"} at 0x000000000000>`;
    }
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
        // Authority (#464): an integer-valued Number PAST the safe range is
        // an int too (it can only arrive from a native-JS boundary — in-`.ps`
        // large ints are BigInt and integer-valued floats are boxed). Print
        // it exactly as an int (String() would switch to exponent form at
        // 1e21; CPython int repr never does).
        if (Number.isInteger(obj)) {
            return BigInt(obj).toString();
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
    // E3: self-referential containers print CPython's placeholder instead
    // of recursing forever (list [...], tuple (...), dict {...}).
    if (__reprSeen.has(obj)) {
        if (obj.__pytuple__) return "(...)";
        if (Array.isArray(obj)) return "[...]";
        return "{...}";
    }
    if (typeof obj.__pytuple__ !== "undefined" || (Array.isArray(obj) && obj.__pytuple__)) {
        __reprSeen.add(obj);
        try {
            const inner = obj.map(pyRepr).join(", ");
            return obj.length === 1 ? `(${inner},)` : `(${inner})`;
        } finally { __reprSeen.delete(obj); }
    }
    if (Array.isArray(obj)) {
        __reprSeen.add(obj);
        try {
            return `[${obj.map(pyRepr).join(", ")}]`;
        } finally { __reprSeen.delete(obj); }
    }
    if (obj instanceof Set) {
        if (obj.size === 0) return "set()";
        __reprSeen.add(obj);
        try {
            return `{${[...obj].map(pyRepr).join(", ")}}`;
        } finally { __reprSeen.delete(obj); }
    }
    if (obj instanceof Map) {
        // Explicit .entries(): PyDict's default iterator yields KEYS.
        __reprSeen.add(obj);
        try {
            const parts = [];
            for (const [k, v] of obj.entries()) parts.push(`${pyRepr(k)}: ${pyRepr(v)}`);
            return `{${parts.join(", ")}}`;
        } finally { __reprSeen.delete(obj); }
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
        // FULL_SURFACE #3: only PLAIN-prototype objects are the dict
        // representation. A class INSTANCE with no __repr__/__str__ gets
        // CPython's default `<module.Class object at 0x…>` (it used to fall
        // into the dict branch and print `{}`).
        const proto = Object.getPrototypeOf(obj);
        if (proto !== Object.prototype && proto !== null) {
            const ctor = obj.constructor;
            const nm = (ctor && (ctor.__name__ || ctor.name)) || "object";
            const mod = (ctor && ctor.__module__) || "__main__";
            return `<${mod}.${nm} object at 0x${__pyObjAddr(obj)}>`;
        }
        // Plain object → dict repr
        __reprSeen.add(obj);
        try {
            const parts = [];
            for (const k of __pyOwnKeys(obj)) { // r6: symbol entries shown, not hidden
                parts.push(`${pyRepr(k)}: ${pyRepr(obj[k])}`);
            }
            return `{${parts.join(", ")}}`;
        } finally { __reprSeen.delete(obj); }
    }
    return String(obj);
}

// ============================================================
// Complex numbers (#283 / delta4)
// ============================================================
// Minimal runtime `complex` type — the PACKAGE twin of the codegen's
// inlined PyComplex block (crates/pyths_codegen_js/src/emit.rs). The
// emitter lowers an imaginary literal `1j` to `pyComplex(0, 1)` and
// imports it from the runtime package, so BOTH entry points must
// export it (previously only the inline `pyths run` path had it —
// any compiled module using a complex literal failed ESM
// instantiation). Construction from real+imag, .real/.imag,
// + - * (mixed int/float/complex via __toComplex), abs() →
// hypot (a float), ==, and a CPython-matching repr. `cmath` and
// complex `/`, `**` are out of scope. Each dunder returns a fresh
// PyComplex; pyAdd/pySub/pyMul dispatch here through __add__ etc.

function __complexFmtPart(x, forceSign) {
    let s;
    if (!isFinite(x)) { s = Number.isNaN(x) ? "nan" : (x > 0 ? "inf" : "-inf"); }
    else if (Number.isInteger(x)) { s = (Object.is(x, -0) ? "-0" : String(x)); }
    else { s = pyFormatFloat(x); }
    if (forceSign && s[0] !== "-") s = "+" + s;
    return s;
}
function __complexRepr(re, im) {
    // Pure-imaginary (real is +0.0): print just the imaginary part, no parens.
    if (re === 0 && !Object.is(re, -0)) return __complexFmtPart(im, false) + "j";
    return "(" + __complexFmtPart(re, false) + __complexFmtPart(im, true) + "j)";
}
function __toComplex(o) {
    if (o instanceof PyComplex) return o;
    if (typeof o === "number") return new PyComplex(o, 0);
    // Option B: a boxed (integer-valued) float coerces by value — this is
    // THE complex-coercion authority behind complex() and every PyComplex
    // dunder, so one unbox here covers complex(8.0), 8.0+2j, abs(3.0+4.0j).
    // (Mirrors the cmath.__c fix; keep both coercion authorities in sync.)
    if (o != null && o.__pyfloat__ === true) return new PyComplex(o.valueOf(), 0);
    if (typeof o === "bigint") return new PyComplex(Number(o), 0);
    if (typeof o === "boolean") return new PyComplex(o ? 1 : 0, 0);
    // Cross-copy interop: a complex-shaped object from the OTHER runtime
    // copy (package stdlib cmath vs the inline program runtime) converts.
    if (o !== null && typeof o === "object" && typeof o.real === "number" && typeof o.imag === "number") return new PyComplex(o.real, o.imag);
    return null;
}
export class PyComplex {
    constructor(re, im) {
        // Option B: a boxed (integer-valued) float component unboxes ONCE
        // here — the constructor is the single entry for complex(), the
        // dunders (via __toComplex) and cross-copy coercion, so internals
        // (repr's `re === 0` check, the arithmetic) always see natives.
        this.real = re != null && re.__pyfloat__ === true ? re.valueOf() : re;
        this.imag = im != null && im.__pyfloat__ === true ? im.valueOf() : im;
    }
    // Brand for the attribute-read path: complex .real/.imag are FLOATS in
    // CPython (pyBoundMethod re-tags them via __pyF on the way out).
    get __pycomplex__() { return true; }
    __add__(o) { const c = __toComplex(o); return c ? new PyComplex(this.real + c.real, this.imag + c.imag) : undefined; }
    __radd__(o) { const c = __toComplex(o); return c ? new PyComplex(c.real + this.real, c.imag + this.imag) : undefined; }
    __sub__(o) { const c = __toComplex(o); return c ? new PyComplex(this.real - c.real, this.imag - c.imag) : undefined; }
    __rsub__(o) { const c = __toComplex(o); return c ? new PyComplex(c.real - this.real, c.imag - this.imag) : undefined; }
    __mul__(o) { const c = __toComplex(o); return c ? new PyComplex(this.real * c.real - this.imag * c.imag, this.real * c.imag + this.imag * c.real) : undefined; }
    __rmul__(o) { const c = __toComplex(o); return c ? new PyComplex(c.real * this.real - c.imag * this.imag, c.real * this.imag + c.imag * this.real) : undefined; }
    __neg__() { return new PyComplex(-this.real, -this.imag); }
    __pos__() { return new PyComplex(this.real, this.imag); }
    // CPython: abs(complex) is a FLOAT — abs(3.0+4.0j) is 5.0, not int 5.
    __abs__() { return __pyF(Math.hypot(this.real, this.imag)); }
    __eq__(o) { const c = __toComplex(o); return c ? (this.real === c.real && this.imag === c.imag) : false; }
    conjugate() { return new PyComplex(this.real, -this.imag); }
    __truediv__(o) {
        const c = __toComplex(o); if (!c) return undefined;
        const d = c.real * c.real + c.imag * c.imag;
        return new PyComplex((this.real * c.real + this.imag * c.imag) / d, (this.imag * c.real - this.real * c.imag) / d);
    }
    __rtruediv__(o) { const c = __toComplex(o); return c ? c.__truediv__(this) : undefined; }
    // z ** w = exp(w * ln z) — CPython returns complex for complex bases.
    __pow__(o) {
        const c = __toComplex(o); if (!c) return undefined;
        const r = Math.hypot(this.real, this.imag);
        if (r === 0) return new PyComplex(0, 0);
        const lnRe = Math.log(r), lnIm = Math.atan2(this.imag, this.real);
        const eRe = c.real * lnRe - c.imag * lnIm;
        const eIm = c.real * lnIm + c.imag * lnRe;
        const m = Math.exp(eRe);
        return new PyComplex(m * Math.cos(eIm), m * Math.sin(eIm));
    }
    __rpow__(o) { const c = __toComplex(o); return c ? c.__pow__(this) : undefined; }
    __repr__() { return __complexRepr(this.real, this.imag); }
    __str__() { return __complexRepr(this.real, this.imag); }
}
export function pyComplex(re, im) { return new PyComplex(re, im); }

// ============================================================
// Bytes (autotester byte_arrays)
// ============================================================
// `bytes`/`bytearray` are Uint8Array subclasses: iteration yields ints,
// indexing yields ints, and the arithmetic helpers (pyAdd/pyMul) implement
// + (concat) and * (repeat). bytearray shares the representation; its
// mutability is element-level (Uint8Array writes), matching the common
// usage. repr is CPython's b'...' form.
function __bytesRepeat(src, n) {
    const count = n < 0 ? 0 : n;
    // Item 5 (0.2.2 hold): bytearray * n -> bytearray (CPython result type
    // follows the bytes-like operand; `ba *= n` must stay a bytearray).
    // Duck-typed via the mutator surface (extraction-safe — a bytes-only
    // program never references the bytearray class).
    if (typeof src.copy === "function" && typeof src.clear === "function"
        && typeof src.extend === "function") {
        const out = src.copy();
        out.clear();
        for (let i = 0; i < count; i++) out.extend(src);
        return out;
    }
    const out = new PyBytes(src.length * count);
    for (let i = 0; i < count; i++) out.set(src, i * src.length);
    return out;
}

// ------------------------------------------------------------------
// Bytes query engine — the ONE implementation of the bytes/bytearray
// method surface (#458 root fix). The methods live on the PyBytes
// PROTOTYPE (bytearray inherits), so BOTH dispatch surfaces reach the
// SAME code by construction:
//   * direct calls: the method-table Multi/Str runtime helpers
//     (pyCount/pyFind/pyIndex/pyStrStartswith/…) all follow the uniform
//     "the receiver's own method wins" rule and land here;
//   * value-position extraction (`m = b.count; m(x)`): pyBoundMethod's
//     prototype-function branch binds the SAME method.
// A future bytes method added here is picked up by both surfaces with no
// dispatch wiring. Semantics + messages are CPython 3.12-exact.
// ------------------------------------------------------------------

/** sub/needle argument of count/find/index/…: a bytes-like object or an
 *  int in range(0, 256) (bool ⊂ int). CPython-exact errors: a non-integer
 *  non-bytes TYPE is the "argument should be integer or bytes-like
 *  object" TypeError; an out-of-range int a ValueError (__byteVal). */
function __bytesNeedle(v) {
    if (v instanceof Uint8Array) return v;
    if (typeof v === "boolean" || typeof v === "bigint"
        || (typeof v === "number" && Number.isInteger(v))) {
        return Uint8Array.of(__byteVal(v));
    }
    throw new TypeError_(
        `argument should be integer or bytes-like object, not '${__pyTypeName(v)}'`);
}

/** CPython slice-rule clamp of optional start/end over a length-n bytes
 *  value (negative indices count from the end; missing → full range). */
function __bytesStartEnd(n, start, end) {
    start = __bytesIdx(start);
    end = __bytesIdx(end);
    let st = start === undefined || start === null ? 0 : Number(start);
    let en = end === undefined || end === null ? n : Number(end);
    if (st < 0) st = Math.max(0, n + st);
    if (en < 0) en = Math.max(0, n + en);
    if (en > n) en = n;
    return [st, en];
}
// bool ⊂ int for start/end indices (b.count(x, True) is start=1).
function __bytesIdx(v) { return typeof v === "boolean" ? (v ? 1 : 0) : v; }

function __bytesMatchAt(h, needle, i) {
    for (let k = 0; k < needle.length; k++) {
        if (h[i + k] !== needle[k]) return false;
    }
    return true;
}

/** Shared find/rfind core: first (or last) match offset within the
 *  clamped [start, end) window, -1 when absent. Empty needle matches at
 *  the window edge like CPython (b"abc".find(b"", 3) is 3; start past
 *  the end, or an inverted window, is -1). */
function __bytesFind(h, sub, start, end, last) {
    const needle = __bytesNeedle(sub);
    const n = h.length;
    let [st, en] = __bytesStartEnd(n, start, end);
    if (st > n) return -1;
    if (needle.length === 0) {
        if (st > en) return -1;
        return last ? en : st;
    }
    const limit = en - needle.length;
    if (last) {
        for (let i = limit; i >= st; i--) {
            if (__bytesMatchAt(h, needle, i)) return i;
        }
    } else {
        for (let i = st; i <= limit; i++) {
            if (__bytesMatchAt(h, needle, i)) return i;
        }
    }
    return -1;
}

/** startswith/endswith core — prefix must be bytes-like or a tuple of
 *  bytes-like (CPython-exact messages), never an int. */
function __bytesStartsEnds(h, fix, start, end, atEnd, meth) {
    const cands = Array.isArray(fix) ? fix : [fix];
    if (!Array.isArray(fix) && !(fix instanceof Uint8Array)) {
        throw new TypeError_(
            `${meth} first arg must be bytes or a tuple of bytes, not ${__pyTypeName(fix)}`);
    }
    const n = h.length;
    const [st, en] = __bytesStartEnd(n, start, end);
    if (st > n) return false;
    for (const p of cands) {
        if (!(p instanceof Uint8Array)) {
            throw new TypeError_(
                `a bytes-like object is required, not '${__pyTypeName(p)}'`);
        }
        const m = p.length;
        const at = atEnd ? en - m : st;
        if (at < st || at + m > en) continue;
        if (__bytesMatchAt(h, p, at)) return true;
    }
    return false;
}

export class PyBytes extends Uint8Array {
    __repr__() {
        let body = "";
        for (const b of this) {
            if (b === 0x5c) body += "\\\\";
            else if (b === 0x27) body += "\\'";
            else if (b === 0x09) body += "\\t";
            else if (b === 0x0a) body += "\\n";
            else if (b === 0x0d) body += "\\r";
            else if (b >= 0x20 && b <= 0x7e) body += String.fromCharCode(b);
            else body += "\\x" + b.toString(16).padStart(2, "0");
        }
        return `b'${body}'`;
    }
    __str__() { return this.__repr__(); }

    /** bytes.count(sub[, start[, end]]) — non-overlapping occurrences of a
     *  bytes-like or single-int needle; empty needle counts the gaps
     *  (CPython: b"abc".count(b"") is 4). */
    count(sub, start, end) {
        const needle = __bytesNeedle(sub);
        const n = this.length;
        const [st, en] = __bytesStartEnd(n, start, end);
        if (st > n) return 0;
        if (needle.length === 0) return st > en ? 0 : en - st + 1;
        let c = 0;
        const limit = en - needle.length;
        let i = st;
        while (i <= limit) {
            if (__bytesMatchAt(this, needle, i)) { c++; i += needle.length; }
            else i++;
        }
        return c;
    }

    /** bytes.find/rfind(sub[, start[, end]]) — offset or -1. */
    find(sub, start, end) { return __bytesFind(this, sub, start, end, false); }
    rfind(sub, start, end) { return __bytesFind(this, sub, start, end, true); }

    /** bytes.index/rindex — like find/rfind but ValueError when absent
     *  (CPython's bytes wording: "subsection not found"). */
    index(sub, start, end) {
        const i = __bytesFind(this, sub, start, end, false);
        if (i < 0) throw new ValueError("subsection not found");
        return i;
    }
    rindex(sub, start, end) {
        const i = __bytesFind(this, sub, start, end, true);
        if (i < 0) throw new ValueError("subsection not found");
        return i;
    }

    /** bytes.startswith/endswith(prefix[, start[, end]]) — bytes-like or
     *  tuple of bytes-like. */
    startswith(prefix, start, end) {
        return __bytesStartsEnds(this, prefix, start, end, false, "startswith");
    }
    endswith(suffix, start, end) {
        return __bytesStartsEnds(this, suffix, start, end, true, "endswith");
    }
}

/** Bytes literal constructor: `b'...'` lowers to pyBytes([..raw bytes..]). */
export function pyBytes(byteValues) {
    return PyBytes.from(byteValues);
}

// ------------------------------------------------------------------
// bytearray — the MUTABLE, growable sibling of bytes.
// ------------------------------------------------------------------
// A `PyByteArray` is a `PyBytes` (→ Uint8Array) subclass, so every
// `instanceof Uint8Array` operation — concat/repeat (pyAdd/pyMul), element
// indexing/iteration, and the emitted inline runtime — treats it exactly like
// bytes. Its data lives in a length-TRACKING view over a RESIZABLE
// ArrayBuffer: resizing the buffer grows/shrinks the SAME object every
// reference sees, matching CPython's in-place mutation. repr is the
// `bytearray(b'...')` form. The mutating methods below are dispatched to
// automatically by the receiver-fallback in pyAppend/pyExtend/pyInsert/pyPop/
// pyRemove/pyClear (which probe `typeof recv.<method> === "function"`), so no
// change to those helpers is needed.

// Resizable ArrayBuffers (Node >= 20 / modern engines) are what make in-place
// growth possible. Feature-detected once; on an engine without them a
// bytearray still reads and repr's correctly and only the mutators fail — with
// a clear message rather than a cryptic one.
const __RESIZABLE_BUFFERS = (() => {
    try {
        return typeof new ArrayBuffer(0, { maxByteLength: 1 }).resize === "function";
    } catch {
        return false;
    }
})();
// `maxByteLength` only RESERVES address space (committed lazily on resize), so
// a large per-bytearray growth headroom is effectively free — measured: 5000
// buffers with a 1 GiB max ≈ 39 MB RSS. 256 MiB of headroom covers every
// realistic bytearray while staying well inside V8's per-buffer limit; growth
// beyond it fails loud (never silently), like a memory limit.
const __BYTEARRAY_HEADROOM = 1 << 28;

/** Coerce a Python value to a byte (int in range(0, 256)); CPython-exact
 *  errors: a non-int TYPE is a TypeError, an out-of-range int a ValueError. */
function __byteVal(v) {
    const n = typeof v === "boolean" ? (v ? 1 : 0)
        : typeof v === "bigint" ? Number(v) : v;
    if (typeof n !== "number" || !Number.isInteger(n)) {
        throw new TypeError_(
            `'${__pyTypeName(v)}' object cannot be interpreted as an integer`);
    }
    if (n < 0 || n > 255) throw new ValueError("byte must be in range(0, 256)");
    return n;
}

/** Coerce a Python value to an integer index for insert/pop. */
function __intIndex(i) {
    const n = typeof i === "boolean" ? (i ? 1 : 0)
        : typeof i === "bigint" ? Number(i) : i;
    if (typeof n !== "number" || !Number.isInteger(n)) {
        throw new TypeError_(
            `'${__pyTypeName(i)}' object cannot be interpreted as an integer`);
    }
    return n;
}

export class PyByteArray extends PyBytes {
    __repr__() {
        // Reuse the byte-escaping of PyBytes (b'...') and wrap it.
        return `bytearray(${super.__repr__()})`;
    }
    __str__() { return this.__repr__(); }

    /** Resize the backing resizable buffer to `n` bytes; the length-tracking
     *  view's `.length` follows. Fails closed with a clear message on an
     *  engine without resizable buffers or past the reserved headroom. */
    __resize(n) {
        const buf = this.buffer;
        if (typeof buf.resize !== "function") {
            throw new TypeError_(
                "bytearray in-place growth requires a runtime with resizable "
                + "ArrayBuffers (Node.js >= 20)");
        }
        if (n > buf.maxByteLength) {
            throw new OverflowError(
                "bytearray exceeded its maximum in-place capacity of "
                + `${buf.maxByteLength} bytes`);
        }
        buf.resize(n);
    }

    /** bytearray.append(int) — grow by one byte in place. */
    append(v) {
        const b = __byteVal(v);
        const n = this.length;
        this.__resize(n + 1);
        this[n] = b;
    }

    /** bytearray.extend(iterable) — a bytes-like extends by its raw bytes; any
     *  other iterable must yield ints in range(0, 256). */
    extend(iter) {
        let src;
        if (iter instanceof Uint8Array) {
            src = iter;
        } else {
            src = [];
            for (const x of pySeq(iter)) src.push(__byteVal(x));
        }
        const n = this.length;
        this.__resize(n + src.length);
        this.set(src, n);
    }

    /** bytearray.insert(i, int) — CPython clamps the index into [0, len]. */
    insert(i, v) {
        const b = __byteVal(v);
        const n = this.length;
        let idx = __intIndex(i);
        if (idx < 0) idx += n;
        if (idx < 0) idx = 0;
        if (idx > n) idx = n;
        this.__resize(n + 1);
        this.copyWithin(idx + 1, idx, n); // shift [idx, n) right by one
        this[idx] = b;
    }

    /** bytearray.pop([i]) — default last; bounds-checked like CPython. */
    pop(i) {
        const n = this.length;
        if (n === 0) throw new IndexError("pop from empty bytearray");
        let idx = i === undefined ? -1 : __intIndex(i);
        if (idx < 0) idx += n;
        if (idx < 0 || idx >= n) throw new IndexError("pop index out of range");
        const val = this[idx];
        this.copyWithin(idx, idx + 1, n); // shift tail left by one
        this.__resize(n - 1);
        return val;
    }

    /** bytearray.remove(int) — remove the first matching byte; ValueError if
     *  absent, matching CPython. */
    remove(v) {
        const b = __byteVal(v);
        const n = this.length;
        const idx = this.indexOf(b);
        if (idx < 0) throw new ValueError("value not found in bytearray");
        this.copyWithin(idx, idx + 1, n);
        this.__resize(n - 1);
    }

    /** bytearray.clear() — truncate to empty in place. */
    clear() { this.__resize(0); }

    /** ba[i] = v / ba[a:b:c] = xs — the bytearray WRITE protocol surface.
     *  pySetItem and pySetSlice dispatch here through their uniform
     *  `__setitem__` protocol branches (the same route user classes take),
     *  so bytearray writes are byte-validated (#455's class): a non-integer
     *  index TYPE is a TypeError, out-of-range index an IndexError, a
     *  non-byte value the __byteVal TypeError/ValueError — CPython-exact. */
    __setitem__(key, value) {
        if (key instanceof PySlice) {
            __byteArraySetSlice(this, key.start, key.stop, key.step, value);
            return;
        }
        const n = this.length;
        let idx = typeof key === "boolean" ? (key ? 1 : 0)
            : typeof key === "bigint" ? Number(key) : key;
        if (typeof idx !== "number" || !Number.isInteger(idx)) {
            throw new TypeError_(
                "bytearray indices must be integers or slices, not " + __pyTypeName(key));
        }
        if (idx < 0) idx += n;
        if (idx < 0 || idx >= n) throw new IndexError("bytearray index out of range");
        this[idx] = __byteVal(value);
    }

    /** del ba[i] / del ba[a:b:c] — remove byte(s) in place. pyDelItem and
     *  pyDelSlice dispatch here via their `__delitem__` branches (F7: a
     *  non-integer index TYPE is a TypeError, an out-of-range integer an
     *  IndexError). */
    __delitem__(key) {
        if (key instanceof PySlice) {
            __byteArrayDelSlice(this, key.start, key.stop, key.step);
            return;
        }
        const n = this.length;
        let idx = typeof key === "boolean" ? (key ? 1 : 0)
            : typeof key === "bigint" ? Number(key) : key;
        if (typeof idx !== "number" || !Number.isInteger(idx)) {
            throw new TypeError_(
                "bytearray indices must be integers or slices, not " + __pyTypeName(key));
        }
        if (idx < 0) idx += n;
        if (idx < 0 || idx >= n) throw new IndexError("bytearray index out of range");
        this.copyWithin(idx, idx + 1, n);
        this.__resize(n - 1);
    }

    /** bytearray.copy() — a fresh, independently-mutable bytearray. */
    copy() {
        const out = __newByteArray(this.length);
        out.set(this);
        return out;
    }
}

/** RHS of a bytearray slice assignment, coerced ONCE (before any mutation,
 *  like CPython validates first): a bytes-like object is used byte-wise; a
 *  str or non-iterable is CPython's "can assign only bytes, buffers, or
 *  iterables of ints in range(0, 256)" TypeError; any other iterable must
 *  yield ints in range(0, 256) (per-element __byteVal errors). A source
 *  sharing the destination's buffer (ba[1:3] = ba) is snapshotted. */
function __byteArraySrc(values, arr) {
    if (values instanceof Uint8Array) {
        return values.buffer === arr.buffer ? Uint8Array.from(values) : values;
    }
    if (typeof values === "string" || values == null
        || typeof values[Symbol.iterator] !== "function") {
        throw new TypeError_(
            "can assign only bytes, buffers, or iterables of ints in range(0, 256)");
    }
    const tmp = [];
    for (const x of values) tmp.push(__byteVal(x));
    return Uint8Array.from(tmp);
}

/** CPython slice.indices clamp shared by the bytearray slice write/delete
 *  paths — the same normalization pySetSlice/pyDelSlice use for lists, so
 *  bytearray reads and writes select identical index sets. */
function __byteArraySliceBounds(len, start, stop, step) {
    start = start == null ? null : Number(start);
    stop = stop == null ? null : Number(stop);
    const lower = step < 0 ? -1 : 0;
    const upper = step < 0 ? len - 1 : len;
    if (start == null) start = step < 0 ? upper : lower;
    else if (start < 0) start = Math.max(start + len, lower);
    else start = Math.min(start, upper);
    if (stop == null) stop = step < 0 ? lower : upper;
    else if (stop < 0) stop = Math.max(stop + len, lower);
    else stop = Math.min(stop, upper);
    return [start, stop];
}

/** ba[a:b:c] = xs — in-place, resizing slice assignment (#455). A simple
 *  slice (step 1/None) replaces the range and may grow/shrink the SAME
 *  object every alias sees; an extended slice assigns element-wise and
 *  requires an exactly-matching source length, CPython-exact message. */
function __byteArraySetSlice(arr, start, stop, step, values) {
    // F5-r2 (v0.2.4): validate the slice COMPONENTS before any mutation or
    // source coercion (CPython order: PySlice indices first, then the RHS).
    // The old path went straight to Number(...) coercion, so a string or
    // boxed-float bound silently selected a wrong range on a WRITE
    // (ba['0':2] = b'x' mutated; CPython raises the slice-indices TypeError
    // and leaves the bytearray untouched). Same validator as list get/set/del
    // (__pySliceIndex); BigInt bounds demote like pySetSlice.
    step = __pySliceIndex(step);
    if (step === 0 || step === 0n) throw new ValueError("slice step cannot be zero");
    start = __pySliceIndex(start);
    stop = __pySliceIndex(stop);
    if (typeof start === "bigint") start = Number(start);
    if (typeof stop === "bigint") stop = Number(stop);
    if (typeof step === "bigint") step = Number(step);
    const src = __byteArraySrc(values, arr);
    const n = arr.length;
    if (step == null || step === 1) {
        let s = start == null ? 0 : start < 0 ? Math.max(0, n + start) : Math.min(start, n);
        let e = stop == null ? n : stop < 0 ? Math.max(0, n + stop) : Math.min(stop, n);
        if (e < s) e = s;
        const removed = e - s;
        const m = src.length;
        if (m > removed) {
            arr.__resize(n + m - removed);
            arr.copyWithin(s + m, e, n);
        } else if (m < removed) {
            arr.copyWithin(s + m, e, n);
            arr.__resize(n - removed + m);
        }
        arr.set(src, s);
        return;
    }
    if (step === 0) throw new ValueError("slice step cannot be zero");
    const [st, sp] = __byteArraySliceBounds(n, start, stop, step);
    const idxs = [];
    if (step > 0) {
        for (let i = st; i < sp; i += step) idxs.push(i);
    } else {
        for (let i = st; i > sp; i += step) idxs.push(i);
    }
    if (idxs.length !== src.length) {
        throw new ValueError(
            `attempt to assign bytes of size ${src.length} to extended slice of size ${idxs.length}`);
    }
    for (let i = 0; i < idxs.length; i++) arr[idxs[i]] = src[i];
}

/** del ba[a:b:c] — in-place slice deletion (the DELETE sibling of
 *  __byteArraySetSlice; out-of-range bounds clamp to a no-op like
 *  CPython slice.indices). */
function __byteArrayDelSlice(arr, start, stop, step) {
    // F5-r2 (v0.2.4): same pre-mutation component validation as
    // __byteArraySetSlice (del ba[0:'2'] must raise, not coerce).
    step = __pySliceIndex(step);
    if (step === 0 || step === 0n) throw new ValueError("slice step cannot be zero");
    start = __pySliceIndex(start);
    stop = __pySliceIndex(stop);
    if (typeof start === "bigint") start = Number(start);
    if (typeof stop === "bigint") stop = Number(stop);
    if (typeof step === "bigint") step = Number(step);
    const n = arr.length;
    if (step == null || step === 1) {
        let s = start == null ? 0 : start < 0 ? Math.max(0, n + start) : Math.min(start, n);
        let e = stop == null ? n : stop < 0 ? Math.max(0, n + stop) : Math.min(stop, n);
        if (e < s) e = s;
        arr.copyWithin(s, e, n);
        arr.__resize(n - (e - s));
        return;
    }
    if (step === 0) throw new ValueError("slice step cannot be zero");
    const [st, sp] = __byteArraySliceBounds(n, start, stop, step);
    const kill = new Set();
    if (step > 0) {
        for (let i = st; i < sp; i += step) kill.add(i);
    } else {
        for (let i = st; i > sp; i += step) kill.add(i);
    }
    let w = 0;
    for (let r = 0; r < n; r++) {
        if (!kill.has(r)) arr[w++] = arr[r];
    }
    arr.__resize(w);
}

/** Build an empty, growable bytearray of `len` zero bytes (data is filled by
 *  the caller). Uses a resizable buffer where available; falls back to a fixed
 *  buffer otherwise (reads/repr work, mutators throw a clear message). */
function __newByteArray(len) {
    if (__RESIZABLE_BUFFERS) {
        const buf = new ArrayBuffer(len, { maxByteLength: len + __BYTEARRAY_HEADROOM });
        return new PyByteArray(buf); // length-tracking (no explicit length)
    }
    return new PyByteArray(len);
}

// The `bytes(...)` / `bytearray(...)` BUILTINS: zero-arg -> b'', int n ->
// n zero bytes, str + encoding -> encoded text (utf-8 family), iterable of
// ints -> those bytes, bytes -> copy.
export function pyBytesOf(src, encoding) {
    if (src === undefined) return new PyBytes(0);
    if (typeof src === "string") {
        if (encoding === undefined) {
            throw new TypeError_("string argument without an encoding");
        }
        const enc = String(encoding).toLowerCase().replace(/[-_]/g, "");
        if (enc !== "utf8" && enc !== "ascii" && enc !== "latin1") {
            const e = new Error(`unknown encoding: ${encoding}`); e.name = "LookupError"; throw e;
        }
        if (enc === "latin1") {
            return PyBytes.from([...src].map((c) => c.codePointAt(0) & 0xff));
        }
        return PyBytes.from(new TextEncoder().encode(src));
    }
    if (typeof src === "number" || typeof src === "bigint") {
        return new PyBytes(Number(src));
    }
    return PyBytes.from(src, (v) => Number(v));
}
export function pyBytearrayOf(src, encoding) {
    // Same argument handling as bytes(), but the result is a MUTABLE, growable
    // PyByteArray (distinct repr + in-place mutators), not immutable bytes.
    const bytes = pyBytesOf(src, encoding);
    const ba = __newByteArray(bytes.length);
    ba.set(bytes);
    return ba;
}

// autotester complex_numbers: `x.conjugate()` on ANY Python number —
// ints/floats/bools are their own conjugate (CPython int.conjugate()).
export function pyConjugate(recv) {
    if (typeof recv === "number" || typeof recv === "bigint" || typeof recv === "boolean") return recv;
    if (recv !== null && recv !== undefined && typeof recv.conjugate === "function") return recv.conjugate();
    const e = new Error(`'${__pyTypeName(recv)}' object has no attribute 'conjugate'`);
    e.name = "AttributeError";
    throw e;
}

// autotester callable_test/complex_numbers: the BUILTIN `complex()`
// constructor — zero-arg, one numeric/complex arg, the two-arg form
// (complex(a, b) = a + b*1j, either possibly complex), and the common
// string forms CPython accepts ('2j', '1+2j', '-j', '(1.5-3j)', 'inf+nanj').
export function pyComplexOf(re, im) {
    if (re === undefined) return new PyComplex(0, 0);
    if (typeof re === "string") {
        if (im !== undefined) {
            throw new TypeError_("complex() can't take second arg if first is a string");
        }
        const t = re.trim().replace(/^\((.*)\)$/s, "$1").trim();
        const NUM = "(?:\\d+(?:\\.\\d*)?|\\.\\d+)(?:[eE][+-]?\\d+)?|[iI][nN][fF](?:[iI][nN][iI][tT][yY])?|[nN][aA][nN]";
        const toF = (str) => {
            const sign = str[0] === "-" ? -1 : 1;
            const body = str.replace(/^[+-]/, "").toLowerCase();
            if (body.startsWith("inf")) return sign * Infinity;
            if (body === "nan") return NaN; // CPython keeps nan unsigned-ish; sign is ignored on parse output repr
            return sign * parseFloat(body);
        };
        let m = new RegExp(`^([+-]?(?:${NUM}))([+-](?:${NUM})?)[jJ]$`).exec(t);
        if (m) {
            const imBody = m[2].length === 1 ? m[2] + "1" : m[2];
            return new PyComplex(toF(m[1]), toF(imBody));
        }
        m = new RegExp(`^([+-]?(?:${NUM})?)[jJ]$`).exec(t);
        if (m) {
            const body = m[1] === "" || m[1] === "+" || m[1] === "-" ? m[1] + "1" : m[1];
            return new PyComplex(0, toF(body));
        }
        m = new RegExp(`^([+-]?(?:${NUM}))$`).exec(t);
        if (m) return new PyComplex(toF(m[1]), 0);
        throw new ValueError("complex() arg is a malformed string");
    }
    const a = __toComplex(re);
    const b = __toComplex(im === undefined ? 0 : im);
    if (!a || !b) {
        throw new TypeError_("complex() argument must be a string or a number");
    }
    // complex(a, b) = a + b*1j — both halves may themselves be complex.
    return new PyComplex(a.real - b.imag, a.imag + b.real);
}

//# sourceMappingURL=operators.js.map
