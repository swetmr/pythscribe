// Tests for pyths-runtime/core — the Worker-safe subpath export.
// Run with: node --test runtime/src/core.test.mjs
//
// Guards two properties:
//  1. Functional — pyAdd/pyLen/etc. produce correct Python-semantics output.
//  2. DOM-free   — core.js and every file it transitively imports contain
//                  zero references to dom.js, react.js, document, or window.
//
// Fix: B-030 — pyths-runtime not Worker/tree-shake-safe for numeric-only output.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import {
    pyAdd, pySub, pyMul, pyDiv, pyFloorDiv, pyMod, pyPow,
    pyLt, pyLe, pyGt, pyGe, pyNe, pyNeg, pyEq,
    pyStr, pyRepr, pyPrint, pyTuple, pyFormatFloat,
    pyLen, pyRange, pyContains, pyGetItem,
    pyBool,
    ValueError, ZeroDivisionError,
    PyDict, PySet, PyTuple,
    PyObject, __pyClass, __pyF,
} from "./core.js";
import { Decimal } from "./stdlib/decimal.js";
import { Fraction } from "./stdlib/fractions.js";

// ── 1. Numeric operators ───────────────────────────────────────────────────

test("pyAdd: int + int, float + float, string concat", () => {
    assert.equal(pyAdd(1, 2), 3);
    // Option B: an integer-valued float RESULT is boxed (PyFloat brand) so
    // 4.0 reprs as a float; its numeric value is native via valueOf().
    const r = pyAdd(1.5, 2.5);
    assert.equal(r.__pyfloat__, true);
    assert.equal(Number(r), 4);
    assert.equal(pyAdd("a", "b"), "ab");
});

test("pySub / pyMul / pyDiv / pyMod / pyPow / pyFloorDiv", () => {
    assert.equal(pySub(10, 3), 7);
    assert.equal(pyMul(3, 4), 12);
    assert.equal(pyDiv(7, 2), 3.5);
    assert.equal(pyMod(10, 3), 1);
    assert.equal(pyPow(2, 10), 1024);
    assert.equal(pyFloorDiv(7, 2), 3);
});

test("pyMul: BigInt operands normalize back to Number when in safe range", () => {
    // __norm() converts BigInt back to Number when the result fits in safe-int range.
    assert.equal(pyMul(2n, 3n), 6);
    // Large BigInt stays BigInt.
    const big = 2n ** 54n;
    assert.equal(typeof pyMul(big, 2n), "bigint");
});

test("pyDiv: division by zero throws ZeroDivisionError", () => {
    assert.throws(() => pyDiv(1, 0), ZeroDivisionError);
});

test("pyFloorDiv: floors toward -inf (Python semantics)", () => {
    assert.equal(pyFloorDiv(-7, 2), -4);  // Python: -7 // 2 == -4
});

test("pyMod: result has sign of divisor (Python semantics)", () => {
    assert.equal(pyMod(-1, 3), 2);  // Python: -1 % 3 == 2
});

// ── 2. Comparison operators ────────────────────────────────────────────────

test("comparison operators", () => {
    assert.equal(pyLt(1, 2), true);
    assert.equal(pyLe(2, 2), true);
    assert.equal(pyGt(3, 2), true);
    assert.equal(pyGe(2, 2), true);
    assert.equal(pyNe(1, 2), true);
    assert.equal(pyEq(1, 1), true);
    assert.equal(pyNeg(-5), 5);
});

test("pyEq: list element-wise equality", () => {
    assert.equal(pyEq([1, 2], [1, 2]), true);
    assert.equal(pyEq([1, 2], [1, 3]), false);
});

// ── 3. String representation ───────────────────────────────────────────────

test("pyStr: None, bool, custom __str__", () => {
    assert.equal(pyStr(null), "None");
    assert.equal(pyStr(true), "True");
    assert.equal(pyStr(false), "False");
    // 42n: PythScribe ints are BigInt-backed at runtime (see A4 note on
    // pyRepr's number branch) — a bare JS `number` here would mean float.
    assert.equal(pyStr(42n), "42");
});

test("pyRepr: strings use single quotes; repr nested list", () => {
    assert.equal(pyRepr("hello"), "'hello'");
    assert.equal(pyRepr([1n, 2n]), "[1, 2]");
    assert.equal(pyRepr(null), "None");
});

// ── 4. Collection builtins ─────────────────────────────────────────────────

test("pyLen: arrays, strings, Set, Map, plain object", () => {
    assert.equal(pyLen([1, 2]), 2);
    assert.equal(pyLen("abc"), 3);
    assert.equal(pyLen(new Set([1, 2, 3])), 3);
    assert.equal(pyLen(new Map([["a", 1]])), 1);
    assert.equal(pyLen({ x: 1, y: 2 }), 2);
});

test("pyRange: stop-only, start+stop", () => {
    assert.deepEqual(pyRange(3), [0, 1, 2]);
    assert.deepEqual(pyRange(1, 4), [1, 2, 3]);
    assert.deepEqual(pyRange(0, 10, 2), [0, 2, 4, 6, 8]);
});

test("pyContains: array and string", () => {
    assert.equal(pyContains([1, 2, 3], 2), true);
    assert.equal(pyContains([1, 2, 3], 5), false);
    assert.equal(pyContains("hello", "ell"), true);
});

test("pyGetItem: list by index, dict by key", () => {
    assert.equal(pyGetItem([10, 20, 30], 1), 20);
    assert.equal(pyGetItem({ a: 1 }, "a"), 1);
});

// ── 5. Python bool / container types ─────────────────────────────────────

test("pyBool: Python truthiness rules", () => {
    assert.equal(pyBool(0), false);
    assert.equal(pyBool(""), false);
    assert.equal(pyBool([]), false);
    assert.equal(pyBool({}), false);
    assert.equal(pyBool([1]), true);
    assert.equal(pyBool("x"), true);
    assert.equal(pyBool(1), true);
});

test("ValueError and ZeroDivisionError are throwable", () => {
    assert.throws(() => { throw new ValueError("bad value"); }, ValueError);
    assert.throws(() => pyDiv(5, 0), ZeroDivisionError);
});

test("PyDict basic operations", () => {
    const d = new PyDict([["a", 1], ["b", 2]]);
    assert.equal(d.get("a"), 1);
    assert.equal(d.get("b"), 2);
});

test("PyObject + __pyClass: MRO wired, __init__ dispatched correctly", () => {
    // __pyClass sets Foo.__mro__ = [Foo, PyObject] directly on Foo.
    // Without it, Foo.__mro__ resolves through the prototype chain to
    // PyObject.__mro__ = [PyObject], which picks the wrong (empty) __init__.
    class Foo extends PyObject {
        __init__(x) { this.x = x; }
    }
    __pyClass(Foo, [PyObject]);
    const foo = new Foo(42);
    assert.equal(foo.x, 42);
    assert.ok(foo instanceof PyObject);
});

// ── 6. DOM-free static check ───────────────────────────────────────────────
// Verifies that core.js and its entire transitive import graph contain
// zero references to dom.js, react.js, document, or window.

test("core.js transitive imports: no dom/react/document/window references", () => {
    const __dirname = dirname(fileURLToPath(import.meta.url));

    // Direct imports of core.js (the full transitive set — none go further).
    const filesToCheck = [
        "core.js",
        "operators.js",
        "runtime.js",
        "types.js",
        "classes.js",
    ];

    // These patterns must NOT appear as actual import specifiers / property
    // accesses in core.js or its transitive deps.
    // Note: bare-word occurrence (e.g., in a comment like "no dom.js refs")
    // is intentionally allowed; we check for actual import expressions and
    // property-access usage patterns.
    const forbidden = [
        // Catches: from "./dom.js"  import "./dom.js"
        { pattern: /from\s+["']\.\/dom\.js["']|import\s+["']\.\/dom\.js["']/, label: "dom.js import expression" },
        // Catches: from "./react.js"  import "./react.js"
        { pattern: /from\s+["']\.\/react\.js["']|import\s+["']\.\/react\.js["']/, label: "react.js import expression" },
        // Catches property access: document.querySelector(...)  document.createElement(...)
        // (requires a word char after the dot to avoid matching "document." at sentence end)
        { pattern: /\bdocument\.\w/, label: "document global property access" },
        // Catches property access: window.location  window.addEventListener(...)
        { pattern: /\bwindow\.\w/, label: "window global property access" },
    ];

    for (const file of filesToCheck) {
        const src = readFileSync(join(__dirname, file), "utf8");
        for (const { pattern, label } of forbidden) {
            assert.ok(
                !pattern.test(src),
                `B-030: ${file} must not contain '${label}' (Worker-safe core)`
            );
        }
    }
});

// ── 7. Print/str/repr fidelity (A4) ─────────────────────────────────────────
// PythScribe claims "Python semantics, not JavaScript's" for print()/str()/
// repr(). These guard the exact byte-for-byte CPython output for bools,
// None, floats, and container reprs — verified against `python -c ...` in
// tests/differential/cpython_corpus.json; these are the fast-iteration unit
// tests for the same functions.

function captured(fn) {
    const orig = console.log;
    let out = "";
    console.log = (...args) => { out += args.join(" ") + "\n"; };
    try { fn(); } finally { console.log = orig; }
    return out;
}

test("pyRepr: bool/None (not JS true/false/null)", () => {
    assert.equal(pyRepr(true), "True");
    assert.equal(pyRepr(false), "False");
    assert.equal(pyRepr(null), "None");
    assert.equal(pyRepr(undefined), "None");
});

test("pyFormatFloat: integral floats get a .0 suffix (CPython repr(1.0) == '1.0')", () => {
    // pyFormatFloat always treats its input as a float — it's the
    // building block the compiler calls once float-ness is established
    // (statically, at the call site) or that pyRepr calls once a number
    // is *unambiguously* a float (see the next test for pyRepr's own
    // number branch, which additionally has to guess for ambiguous
    // whole-number values — pyFormatFloat itself never guesses).
    assert.equal(pyFormatFloat(1.0), "1.0");
    assert.equal(pyFormatFloat(-1.0), "-1.0");
    assert.equal(pyFormatFloat(0.0), "0.0");
    assert.equal(pyFormatFloat(-0.0), "-0.0");
    assert.equal(pyFormatFloat(100.0), "100.0");
});

test("pyRepr: ambiguous whole-number float — ints and whole floats compile to the " +
     "identical JS number, so pyRepr must guess; documents the accepted default", () => {
    // A4 known limitation (see the long comment on pyRepr's number branch
    // in runtime/src/operators.js): a plain JS `number` that is an exact
    // integer within the safe-integer range is genuinely ambiguous —
    // could be a small Python int (e.g. `1`) or a whole-number Python
    // float (e.g. `1.0`) reaching pyRepr through an untyped channel
    // (nested in a list/dict/tuple literal, an unannotated function
    // return, ...). pyRepr defaults to int-like display (no `.0`) for
    // this ambiguous case — ints are the overwhelmingly common case, and
    // this matches pre-A4 behavior for what was already working
    // correctly (plain int repr). The specific verified-broken case from
    // the bug report — `print(1.0)`, a *direct* literal argument whose
    // float-ness IS statically knowable — is fixed at the compiler level
    // instead (see crates/pyths_codegen_js/src/emit.rs's A4 notes, and
    // the differential-corpus / codegen-test coverage for it), not here.
    assert.equal(pyRepr(1.0), "1");
});

test("pyRepr: non-integral floats unaffected", () => {
    assert.equal(pyRepr(3.14), "3.14");
    assert.equal(pyRepr(-2.5), "-2.5");
});

test("pyRepr: scientific notation floats match CPython e+NN / e-NN format", () => {
    // Value-boundary authority (#38/#464): an integer-valued float carries
    // the PyFloat box (that's its runtime form — codegen boxes 1e16 at
    // creation); a RAW integer-valued Number is the inbound-int form and
    // reprs as an int. Non-integer floats stay native.
    assert.equal(pyRepr(__pyF(1e16)), "1e+16");
    assert.equal(pyRepr(1e-5), "1e-05");
    assert.equal(pyRepr(5e-7), "5e-07");
    // 1.5e300 is integer-VALUED at double precision, so its float form is
    // the box too (the codegen boxes any float literal with fract() == 0).
    assert.equal(pyRepr(__pyF(1.5e300)), "1.5e+300");
    // CPython keeps 16-digit whole floats in fixed notation (decpt==16 is
    // the boundary, not scientific until decpt>16).
    assert.equal(pyRepr(__pyF(9999999999999998.0)), "9999999999999998.0");
    // The raw (unboxed) forms are ints — exact digits, no exponent (#464).
    assert.equal(pyRepr(1e16), "10000000000000000");
    assert.equal(pyRepr(9999999999999998), "9999999999999998");
});

test("pyRepr: list literals — Python True/None/quotes, not JS formatting", () => {
    assert.equal(pyRepr([true, null]), "[True, None]");
    assert.equal(pyRepr(["x", "y"]), "['x', 'y']");
    assert.equal(pyRepr([]), "[]");
});

test("pyRepr: plain-object dict — single-quoted keys, CPython spacing", () => {
    assert.equal(pyRepr({ a: 1n }), "{'a': 1}");
    assert.equal(pyRepr({}), "{}");
});

test("pyRepr: Set and Map still work (unaffected by this fix)", () => {
    assert.equal(pyRepr(new Set()), "set()");
    assert.equal(pyRepr(new Set([1n, 2n])), "{1, 2}");
    assert.equal(pyRepr(new Map([["a", 1n]])), "{'a': 1}");
});

test("pyTuple: marks a real array (Array.isArray, .length, indexing, iteration all work)", () => {
    const t = pyTuple(1, 2, 3);
    assert.ok(Array.isArray(t));
    assert.equal(t.length, 3);
    assert.equal(t[0], 1);
    assert.deepEqual([...t], [1, 2, 3]);
});

test("pyTuple: marker is non-enumerable — invisible to JSON.stringify/Object.keys/spread", () => {
    const t = pyTuple(1, 2);
    assert.equal(JSON.stringify(t), "[1,2]");
    assert.deepEqual(Object.keys(t), ["0", "1"]);
    const spread = { ...[9] }; // sanity: object-spread of array indexes only
    assert.deepEqual(spread, { 0: 9 });
    assert.deepEqual([...t], [1, 2]); // array-spread carries no extra props
});

test("pyTuple: pyEq element-wise compare unaffected by the marker", () => {
    assert.equal(pyEq(pyTuple(1, 2), pyTuple(1, 2)), true);
    assert.equal(pyEq(pyTuple(1, 2), pyTuple(1, 3)), false);
});

test("pyRepr: tuples print as (a, b), not [a, b] — singleton and empty forms", () => {
    assert.equal(pyRepr(pyTuple(1n, 2n)), "(1, 2)");
    assert.equal(pyRepr(pyTuple(1n)), "(1,)");
    assert.equal(pyRepr(pyTuple()), "()");
});

test("pyRepr: nested containers — list-of-dict, dict-of-list, tuple-of-tuple", () => {
    assert.equal(pyRepr([{ a: 1n }]), "[{'a': 1}]");
    assert.equal(pyRepr({ a: [1n, 2n] }), "{'a': [1, 2]}");
    assert.equal(pyRepr(pyTuple(pyTuple(1n, 2n), pyTuple(3n, 4n))), "((1, 2), (3, 4))");
});

test("pyStr: bool/None match Python str(), not JS String()", () => {
    assert.equal(pyStr(true), "True");
    assert.equal(pyStr(false), "False");
    assert.equal(pyStr(null), "None");
});

test("pyStr: top-level string prints unquoted; nested string in a container is quoted", () => {
    assert.equal(pyStr("hello"), "hello");
    assert.equal(pyStr(["hello"]), "['hello']");
});

test("pyStr: containers use repr() semantics (str(list) == repr(list) in Python)", () => {
    assert.equal(pyStr([true, null, "x"]), "[True, None, 'x']");
    assert.equal(pyStr({ a: 1n }), "{'a': 1}");
    assert.equal(pyStr(pyTuple(1n, 2n)), "(1, 2)");
});

test("pyStr: non-integral float formats via pyFormatFloat (unambiguous)", () => {
    assert.equal(pyStr(3.5), "3.5");
});

test("pyStr: ambiguous whole-number float — same documented default as pyRepr " +
     "(see pyRepr's ambiguous-whole-number-float test above)", () => {
    // The real fix for `str(1.0) == '1.0'` (a direct literal, statically
    // known to be float) lives at the compiler level — see
    // test_str_and_repr_whole_float_literal_use_pyformatfloat in
    // crates/pyths_codegen_js/src/emit.rs, and the cpython_corpus.json /
    // differential-run.mjs entries for `str(1.0)` — not in this raw
    // function call, which has no compile-time context to draw on.
    assert.equal(pyStr(1.0), "1");
});

test("pyStr/pyRepr: Decimal and Fraction __str__/__repr__ dispatch unaffected", () => {
    const d = new Decimal("3.14");
    assert.equal(pyStr(d), "3.14");
    assert.equal(pyRepr(d), "Decimal('3.14')");
    const f = new Fraction(1, 2);
    assert.equal(pyStr(f), "1/2");
    assert.equal(pyRepr(f), "Fraction(1, 2)");
});

test("pyPrint: bool/None/float/containers/tuple — CPython-faithful output", () => {
    assert.equal(captured(() => pyPrint(true)), "True\n");
    assert.equal(captured(() => pyPrint(null)), "None\n");
    // 1.5: unambiguous float (non-integer) — pyFormatFloat's job.
    // print(1.0) itself (a whole-number literal) is fixed at the
    // compiler level — see the pyStr/pyRepr ambiguous-float tests above.
    assert.equal(captured(() => pyPrint(1.5)), "1.5\n");
    assert.equal(captured(() => pyPrint([true, null])), "[True, None]\n");
    assert.equal(captured(() => pyPrint({ a: 1n })), "{'a': 1}\n");
    assert.equal(captured(() => pyPrint(["x", "y"])), "['x', 'y']\n");
    assert.equal(captured(() => pyPrint(pyTuple(1n, 2n))), "(1, 2)\n");
});

test("pyPrint: plain string still prints unquoted (existing behavior, no regression)", () => {
    assert.equal(captured(() => pyPrint("plain")), "plain\n");
});

test("pyPrint: multiple args space-joined (Python print(a, b) default sep)", () => {
    assert.equal(captured(() => pyPrint(1n, "a", true)), "1 a True\n");
});

// ---- FULL_SURFACE #4: random.seed() — deterministic within PythScribe ----
// BY DESIGN the sequence does NOT match CPython (mulberry32, not Mersenne
// Twister — see docs/known-limitations.md); the contract is determinism:
// same seed → same sequence, different seed → different sequence.
test("random.seed gives reproducible sequences", async () => {
    const random = await import("./stdlib/random.js");
    random.seed(0);
    const a = [random.random(), random.random(), random.randint(1, 100),
               random.choice([1, 2, 3, 4, 5]), random.uniform(0, 10)];
    random.seed(0);
    const b = [random.random(), random.random(), random.randint(1, 100),
               random.choice([1, 2, 3, 4, 5]), random.uniform(0, 10)];
    assert.deepEqual(a, b);
    random.seed(1);
    assert.notEqual(random.random(), a[0]);
    // shuffle: deterministic under a seed AND still a permutation
    const arr = [1, 2, 3, 4, 5, 6];
    random.seed(7);
    const s1 = random.shuffle([...arr]);
    random.seed(7);
    const s2 = random.shuffle([...arr]);
    assert.deepEqual(s1, s2);
    assert.deepEqual([...s1].sort((x, y) => x - y), arr);
    // module functions and the Random class share the algorithm
    const r1 = new random.Random(5), r2 = new random.Random(5);
    assert.equal(r1.random(), r2.random());
    // values stay in range
    random.seed(123);
    for (let i = 0; i < 50; i++) {
        const v = random.random();
        assert.ok(v >= 0 && v < 1);
        const n = random.randint(3, 9);
        assert.ok(n >= 3 && n <= 9);
    }
});
