// Unit tests for the Python-method runtime helpers added in Step 3 of the
// codegen-only Python-ism elimination work. Run with: `node --test`.
//
// These guard runtime semantics (the codegen unit tests only check emitted
// JS strings; this file checks behavior).

import { test } from "node:test";
import assert from "node:assert/strict";

import {
    pyStrJoin,
    pyStrSplit,
    pyStrTitle,
    pyStrCapitalize,
    pyStrFormat,
    pyListRemove,
    pyListCount,
    pyListClear,
    pyIndex,
    pyPop,
    pyDictGet,
    pyDictSetdefault,
    pyEnumerate,
    __pyEffect,
    ValueError,
    KeyError,
} from "./runtime.js";

test("pyStrJoin joins iter with separator", () => {
    assert.equal(pyStrJoin(", ", ["a", "b", "c"]), "a, b, c");
    assert.equal(pyStrJoin("", ["x", "y"]), "xy");
    assert.equal(pyStrJoin("-", []), "");
});

test("pyStrSplit no-arg splits on whitespace, drops empties", () => {
    assert.deepEqual(pyStrSplit("  a  b   c "), ["a", "b", "c"]);
});

test("pyStrSplit with sep keeps empty splits", () => {
    assert.deepEqual(pyStrSplit("a,,b", ","), ["a", "", "b"]);
});

test("pyStrTitle uppercases first letter of each word", () => {
    assert.equal(pyStrTitle("hello world"), "Hello World");
    assert.equal(pyStrTitle("foo-bar"), "Foo-Bar");
});

test("pyStrCapitalize: first upper, rest lower; empty stays empty", () => {
    assert.equal(pyStrCapitalize("hELLO"), "Hello");
    assert.equal(pyStrCapitalize(""), "");
});

test("pyStrFormat handles {}, {0}, and named keys", () => {
    assert.equal(pyStrFormat("hi {}", "alice"), "hi alice");
    assert.equal(pyStrFormat("{0}+{1}={1}+{0}", "a", "b"), "a+b=b+a");
    assert.equal(pyStrFormat("hello {name}", { name: "bob" }), "hello bob");
});

test("pyListRemove deletes first occurrence", () => {
    const xs = [1, 2, 3, 2];
    pyListRemove(xs, 2);
    assert.deepEqual(xs, [1, 3, 2]);
});

test("pyListRemove raises ValueError when absent", () => {
    assert.throws(() => pyListRemove([1, 2], 99), { name: "ValueError" });
});

test("pyListCount counts occurrences", () => {
    assert.equal(pyListCount([1, 2, 1, 1], 1), 3);
    assert.equal(pyListCount([], "x"), 0);
});

test("pyListClear empties array in place", () => {
    const xs = [1, 2, 3];
    pyListClear(xs);
    assert.deepEqual(xs, []);
});

test("pyIndex finds; throws ValueError if missing", () => {
    assert.equal(pyIndex([10, 20, 30], 20), 1);
    assert.equal(pyIndex("foobar", "bar"), 3);
    assert.throws(() => pyIndex([1, 2], 99), { name: "ValueError" });
});

test("pyPop on list, no-arg, pops from end", () => {
    const xs = [1, 2, 3];
    assert.equal(pyPop(xs), 3);
    assert.deepEqual(xs, [1, 2]);
});

test("pyPop on list, with index, removes from index", () => {
    const xs = [1, 2, 3];
    assert.equal(pyPop(xs, 0), 1);
    assert.deepEqual(xs, [2, 3]);
});

test("pyPop on list, negative index, counts from end", () => {
    const xs = [1, 2, 3];
    assert.equal(pyPop(xs, -1), 3);
    assert.deepEqual(xs, [1, 2]);
});

test("pyPop on dict removes key and returns value", () => {
    const d = { a: 1, b: 2 };
    assert.equal(pyPop(d, "a"), 1);
    assert.deepEqual(d, { b: 2 });
});

test("pyPop on dict with default returns default if missing", () => {
    assert.equal(pyPop({}, "missing", 99), 99);
});

test("pyPop on dict with no default raises KeyError", () => {
    assert.throws(() => pyPop({}, "missing"), { name: "KeyError" });
});

test("pyDictGet returns value if present, else default", () => {
    assert.equal(pyDictGet({ a: 1 }, "a", 99), 1);
    assert.equal(pyDictGet({ a: 1 }, "b", 99), 99);
    assert.equal(pyDictGet({ a: 1 }, "b"), undefined);
});

test("pyDictGet dispatches to .get() on Map/FormData/URLSearchParams (B-038)", () => {
    // Map — .get is a real method; `k in map` / `map[k]` would be wrong.
    const m = new Map([["a", 1]]);
    assert.equal(pyDictGet(m, "a", 99), 1);
    assert.equal(pyDictGet(m, "b", 99), 99);
    // URLSearchParams (FormData-shaped: .get returns null when missing).
    const p = new URLSearchParams("title=hello");
    assert.equal(pyDictGet(p, "title"), "hello");
    assert.equal(pyDictGet(p, "missing", "d"), "d");
    // A plain-object dict that happens to have a "get" key stays dict-indexed.
    assert.equal(pyDictGet({ get: 5, a: 1 }, "a"), 1);
    assert.equal(pyDictGet({ get: 5 }, "get"), 5);
});

test("pyDictGet on null receiver returns default (defensive)", () => {
    assert.equal(pyDictGet(null, "k", 0), 0);
});

test("pyDictSetdefault sets only when missing", () => {
    const d = { a: 1 };
    assert.equal(pyDictSetdefault(d, "a", 99), 1);   // existing
    assert.equal(pyDictSetdefault(d, "b", 99), 99);  // new
    assert.deepEqual(d, { a: 1, b: 99 });
});

// ============================================================
// Tests for the comprehensive method-table helpers (Phase 2).
// ============================================================

import {
    pyCount, pyClear, pyCopy, pyRemove,
    pyStrCenter, pyStrLjust, pyStrRjust, pyStrExpandtabs,
    pyStrPartition, pyStrRpartition, pyStrRsplit, pyStrSplitlines,
    pyStrSwapcase, pyStrTranslate, pyStrIsidentifier, pyStrIsprintable,
    pyStrIstitle, pyStrRindex,
    pyListSort, pyDictPopitem,
    pySetUnion, pySetIntersection, pySetDifference, pySetSymmetricDifference,
    pySetIntersectionUpdate, pySetDifferenceUpdate, pySetSymmetricDifferenceUpdate,
    pySetIsdisjoint, pySetIssubset, pySetIssuperset,
} from "./runtime.js";

test("pyCount on string counts substring occurrences", () => {
    assert.equal(pyCount("ababab", "ab"), 3);
    assert.equal(pyCount("hello", "x"), 0);
});

test("pyCount on list counts === matches", () => {
    assert.equal(pyCount([1, 2, 1, 1], 1), 3);
});

test("pyClear empties list/dict/set in place", () => {
    const xs = [1, 2]; pyClear(xs); assert.deepEqual(xs, []);
    const d = { a: 1 }; pyClear(d); assert.deepEqual(d, {});
    const s = new Set([1, 2]); pyClear(s); assert.equal(s.size, 0);
});

test("pyCopy makes shallow copies", () => {
    assert.deepEqual(pyCopy([1, 2]), [1, 2]);
    assert.deepEqual(pyCopy({ a: 1 }), { a: 1 });
    const s = new Set([1, 2]); const c = pyCopy(s); c.add(3);
    assert.equal(s.size, 2);
});

test("pyRemove removes from list or set", () => {
    const xs = [1, 2, 3]; pyRemove(xs, 2); assert.deepEqual(xs, [1, 3]);
    const s = new Set([1, 2]); pyRemove(s, 1); assert.equal(s.size, 1);
    assert.throws(() => pyRemove(new Set(), 99), { name: "KeyError" });
});

test("pyStrCenter pads both sides", () => {
    assert.equal(pyStrCenter("hi", 6), "  hi  ");
    // CPython tie-break: odd margin puts the extra pad on the LEFT
    // (left = marg/2 + (marg & width & 1)); "hi".center(5,"-") == "--hi-".
    assert.equal(pyStrCenter("hi", 5, "-"), "--hi-");
});

test("pyStrLjust / pyStrRjust pad to width", () => {
    assert.equal(pyStrLjust("hi", 5), "hi   ");
    assert.equal(pyStrRjust("hi", 5), "   hi");
    assert.equal(pyStrLjust("hi", 5, "-"), "hi---");
});

test("pyStrExpandtabs replaces tabs with spaces to next stop", () => {
    assert.equal(pyStrExpandtabs("a\tb"), "a       b");
    assert.equal(pyStrExpandtabs("a\tb", 4), "a   b");
});

test("pyStrPartition / pyStrRpartition split on first/last sep", () => {
    assert.deepEqual(pyStrPartition("a-b-c", "-"), ["a", "-", "b-c"]);
    assert.deepEqual(pyStrRpartition("a-b-c", "-"), ["a-b", "-", "c"]);
    assert.deepEqual(pyStrPartition("abc", "-"), ["abc", "", ""]);
    assert.deepEqual(pyStrRpartition("abc", "-"), ["", "", "abc"]);
});

test("pyStrSplitlines splits on universal newlines", () => {
    assert.deepEqual(pyStrSplitlines("a\nb\r\nc"), ["a", "b", "c"]);
    assert.deepEqual(pyStrSplitlines("a\nb", true), ["a\n", "b"]);
});

test("pyStrSwapcase inverts case", () => {
    assert.equal(pyStrSwapcase("AbC"), "aBc");
});

test("pyStrIsidentifier matches ASCII identifier rules", () => {
    assert.ok(pyStrIsidentifier("foo_bar"));
    assert.ok(!pyStrIsidentifier("9foo"));
    assert.ok(!pyStrIsidentifier(""));
});

test("pyStrIstitle detects title case", () => {
    assert.ok(pyStrIstitle("Hello World"));
    assert.ok(!pyStrIstitle("hello world"));
    assert.ok(!pyStrIstitle("HELLO"));
});

test("pyListSort sorts with optional key/reverse", () => {
    const xs = [3, 1, 2]; pyListSort(xs); assert.deepEqual(xs, [1, 2, 3]);
    const ys = [{ n: 3 }, { n: 1 }];
    pyListSort(ys, { key: o => o.n });
    assert.deepEqual(ys, [{ n: 1 }, { n: 3 }]);
});

test("pyDictPopitem removes last-inserted", () => {
    const d = { a: 1, b: 2 };
    assert.deepEqual(pyDictPopitem(d), ["b", 2]);
    assert.deepEqual(d, { a: 1 });
});

test("pySetUnion / pySetIntersection / pySetDifference / pySetSymmetricDifference", () => {
    const a = new Set([1, 2, 3]);
    const b = new Set([2, 3, 4]);
    assert.deepEqual([...pySetUnion(a, b)].sort(), [1, 2, 3, 4]);
    assert.deepEqual([...pySetIntersection(a, b)].sort(), [2, 3]);
    assert.deepEqual([...pySetDifference(a, b)].sort(), [1]);
    assert.deepEqual([...pySetSymmetricDifference(a, b)].sort(), [1, 4]);
});

test("set predicates", () => {
    const a = new Set([1, 2]);
    const b = new Set([2, 3]);
    const c = new Set([1, 2, 3]);
    assert.ok(!pySetIsdisjoint(a, b));
    assert.ok(pySetIsdisjoint(a, new Set([5])));
    assert.ok(pySetIssubset(a, c));
    assert.ok(pySetIssuperset(c, a));
});

import { pyNormalizeStyle, pyFormatSpec, pyFormatDynamic, pyUpdate } from "./runtime.js";

test("pyNormalizeStyle converts snake_case keys to camelCase", () => {
    assert.deepEqual(
        pyNormalizeStyle({ border_radius: "6px", padding: "8px" }),
        { borderRadius: "6px", padding: "8px" },
    );
});

test("pyNormalizeStyle preserves CSS custom props (-- prefix)", () => {
    assert.deepEqual(
        pyNormalizeStyle({ "--my-var": "red", padding: "1px" }),
        { "--my-var": "red", padding: "1px" },
    );
});

test("pyNormalizeStyle returns same ref when nothing to convert (fast path)", () => {
    const o = { padding: "1px", margin: "2px" };
    assert.equal(pyNormalizeStyle(o), o);
});

test("pyNormalizeStyle handles non-object passthrough", () => {
    assert.equal(pyNormalizeStyle(null), null);
    assert.equal(pyNormalizeStyle(undefined), undefined);
});

test("pyFormatSpec basic shapes", () => {
    assert.equal(pyFormatSpec(3.14, { precision: 2, type: "f" }), "3.14");
    assert.equal(pyFormatSpec(255, { type: "x", alt: true }), "0xff");
    assert.equal(pyFormatSpec("hi", { width: 5, align: ">", type: "s" }), "   hi");
    assert.equal(pyFormatSpec(7, { width: 4, zero: true, type: "d" }), "0007");
});

test("pyFormatDynamic parses the spec at runtime (#108)", () => {
    // Dynamic f-string specs: f"{v:{w}}" builds the spec string at
    // runtime; pyFormatDynamic parses it and delegates to pyFormatSpec.
    assert.equal(pyFormatDynamic(42, "8"), "      42");
    assert.equal(pyFormatDynamic(3.14159, ".3f"), "3.142");
    assert.equal(pyFormatDynamic(3.14159, "10.2f"), "      3.14");
    assert.equal(pyFormatDynamic("hi", "^6"), "  hi  ");
    assert.equal(pyFormatDynamic(-42, "05"), "-0042");
    assert.equal(pyFormatDynamic(255, "#x"), "0xff");
    assert.equal(pyFormatDynamic(42, ""), "42");
});

test("pyUpdate dispatches dict vs set", () => {
    const d = { a: 1 };
    pyUpdate(d, { b: 2 });
    assert.deepEqual(d, { a: 1, b: 2 });

    const s = new Set([1, 2]);
    pyUpdate(s, [3, 4]);
    assert.deepEqual([...s].sort(), [1, 2, 3, 4]);
});

// WB-6: a custom receiver with its OWN `update` must have it invoked for ANY
// arg count, including ZERO. The old loop over trailing args silently dropped
// a no-arg `obj.update()` (a user method colliding with dict.update).
test("pyUpdate forwards a no-arg call to a custom receiver's own update", () => {
    class Contents {
        constructor() { this.n = 0; }
        update(...args) { this.n += 1; this.lastArgc = args.length; }
    }
    const c = new Contents();
    pyUpdate(c);          // no-arg — must run (was dropped)
    pyUpdate(c);          // again
    assert.equal(c.n, 2);
    assert.equal(c.lastArgc, 0);
    // Single-arg custom receiver unchanged: forwarded once.
    const c2 = new Contents();
    pyUpdate(c2, { x: 1 });
    assert.equal(c2.n, 1);
    assert.equal(c2.lastArgc, 1);
});

// Sweep-A S2 finding: `enumerate(xs, start=1)` (keyword `start`) silently
// corrupted output. The codegen's universal calling convention wraps ALL
// keyword arguments into a single trailing options-object literal
// (`{start: 1}`), regardless of the callee's own parameter shape — that's
// exactly how `pySorted(iterable, { key, reverse } = {})` is designed to be
// called. `pyEnumerate`'s old signature was a *plain* `(iterable, start = 0)`
// positional parameter, so a keyword call passed the whole `{start: 1}`
// object where a bare number was expected, and `i = {start: 1}` corrupted
// every yielded index. Fixed to accept either shape: a bare number
// (positional call / `enumerate(xs, 1)`) or an options object (keyword call
// / `enumerate(xs, start=1)`).
test("pyEnumerate: bare positional start (positional-call shape)", () => {
    assert.deepEqual(pyEnumerate(["a", "b"], 1), [[1, "a"], [2, "b"]]);
});

test("pyEnumerate: default start is 0", () => {
    assert.deepEqual(pyEnumerate(["a", "b"]), [[0, "a"], [1, "b"]]);
});

test("pyEnumerate: keyword start (options-object shape from kwarg codegen)", () => {
    assert.deepEqual(pyEnumerate(["a", "b"], { start: 1 }), [[1, "a"], [2, "b"]]);
});

// ---------------------------------------------------------------------------
// Sweep-A fix batch (G) — runtime-helper behavior guards.
// ---------------------------------------------------------------------------

import {
    pyChr, pyOrd, pyMin, pyMax, pyFixed, pyDelItem,
    IndexError,
} from "./runtime.js";
import { pyStr } from "./operators.js";

test("#87 pyStrTitle lowercases the rest of each word (CPython semantics)", () => {
    assert.equal(pyStrTitle("QUX"), "Qux");
    assert.equal(pyStrTitle("hELLo wORLD"), "Hello World");
    // CPython word boundary = any non-cased char: "it's" -> "It'S"
    assert.equal(pyStrTitle("it's a test"), "It'S A Test");
    assert.equal(pyStrTitle("x1y2z"), "X1Y2Z");
});

test("#92 pyStrSplit/pyStrRsplit raise ValueError on empty separator", () => {
    assert.throws(() => pyStrSplit("ab", ""), /empty separator/);
    assert.throws(() => pyStrRsplit("ab", ""), /empty separator/);
});

test("#89 pyChr/pyOrd are code-point based with CPython errors", () => {
    assert.equal(pyChr(97), "a");
    assert.equal(pyOrd("a"), 97);
    assert.equal(pyChr(128512), "\u{1F600}");
    assert.equal(pyOrd("\u{1F600}"), 128512); // astral char is ONE char
    assert.throws(() => pyChr(0x110000), /chr\(\) arg not in range\(0x110000\)/);
    assert.throws(() => pyChr(-1), /chr\(\) arg not in range\(0x110000\)/);
    assert.throws(() => pyOrd("ab"), /expected a character, but string of length 2/);
});

test("#88 pyMin/pyMax: iterable form, scalar form, key=, default=", () => {
    assert.equal(pyMin([3, 1, 2]), 1);
    assert.equal(pyMax([3, 1, 2]), 3);
    assert.equal(pyMin(3, 1, 2), 1);
    assert.equal(pyMax("hello"), "o");
    assert.equal(pyMin(["aaa", "b", "cc"], { key: (s) => s.length }), "b");
    assert.equal(pyMax(["aaa", "b", "cc"], { key: (s) => s.length }), "aaa");
    assert.equal(pyMin([], { default: 5 }), 5);
    assert.throws(() => pyMin([]), /min\(\) iterable argument is empty/);
});

test("#86 pyFixed rounds exact ties half-to-even (unlike toFixed)", () => {
    assert.equal(pyFixed(1.625, 2), "1.62");   // toFixed gives "1.63"
    assert.equal(pyFixed(2.675, 2), "2.67");   // 2.67499... — not a tie
    assert.equal(pyFixed(0.375, 2), "0.38");
    assert.equal(pyFixed(2.5, 0), "2");
    assert.equal(pyFixed(-1.625, 2), "-1.62");
    assert.equal(pyFixed(3.1, 3), "3.100");
});

test("#101 pyDelItem: list splice, dict KeyError, negative index", () => {
    const xs = [1, 2, 3];
    pyDelItem(xs, 0);
    assert.deepEqual(xs, [2, 3]);
    pyDelItem(xs, -1);
    assert.deepEqual(xs, [2]);
    assert.throws(() => pyDelItem(xs, 5), IndexError);
    const d = { a: 1, b: 2 };
    pyDelItem(d, "a");
    assert.deepEqual(d, { b: 2 });
    assert.throws(() => pyDelItem(d, "zz"), KeyError);
});

test("#97 pyStr dispatches a user toString override (renamed __str__)", () => {
    class A { toString() { return "A-str"; } }
    assert.equal(pyStr(new A()), "A-str");
    // (The full Python-repr fallback for plain dicts lives in
    // runtime/src/operators.js and is covered by the differential corpus
    // g_dunder_str_* entries; this stale test copy only guards dispatch.)
});

// ── #83: Map-backed dicts (non-string keys) ────────────────────────────────
// PyDict canonicalization + the shape-dispatched dict helpers. Mirrors
// runtime/src/runtime.js; behavior is CPython's hash-equality rules.

test("#83 PyDict: int keys round-trip with type identity", async () => {
    const { PyDict } = await import("./runtime.js");
    const d = new PyDict([[1, "a"], [2, "b"]]);
    assert.deepEqual([...d.keys()], [1, 2]);
    assert.equal(d.get(1), "a");
    assert.equal(d.has("1"), false); // '1' (str) is a DIFFERENT key than 1 (int)
});

test("#83 PyDict: CPython key folding — True/1/1.0 are one key, first key object wins", async () => {
    const { PyDict } = await import("./runtime.js");
    const d = new PyDict();
    d.set(true, "t");
    d.set(1, "one");     // same key as True; value replaced, key stays True
    d.set(1.0, "float"); // still the same key
    assert.equal(d.size, 1);
    assert.deepEqual([...d.entries()], [[true, "float"]]);
    const e = new PyDict([[1, "a"]]);
    e.set(true, "b");    // 1 first → key displays as 1
    assert.deepEqual([...e.entries()], [[1, "b"]]);
});

test("#83 PyDict: tuple keys hash by structure; keys() returns the original tuple", async () => {
    const { PyDict } = await import("./runtime.js");
    const { pyTuple } = await import("./operators.js");
    const d = new PyDict();
    d.set(pyTuple(1, "x"), 5);
    assert.equal(d.get(pyTuple(1, "x")), 5);      // different tuple object, same structure
    assert.equal(d.get(pyTuple(1.0, "x")), 5);    // 1.0 folds with 1 inside tuples too
    assert.equal(d.has(pyTuple(1, "y")), false);
    const [k] = [...d.keys()];
    assert.deepEqual(k, [1, "x"]);                 // original tuple object, not the encoding
});

test("#83 PyDict: unhashable keys raise TypeError like CPython", async () => {
    const { PyDict } = await import("./runtime.js");
    const d = new PyDict();
    assert.throws(() => d.set([1, 2], "x"), /unhashable type: 'list'/);
    assert.throws(() => d.set({}, "x"), /unhashable type: 'dict'/);
    assert.throws(() => d.set(new Set(), "x"), /unhashable type: 'set'/);
});

test("#83 PyDict: default iteration yields KEYS (Python), values()/entries() intact", async () => {
    const { PyDict } = await import("./runtime.js");
    const d = new PyDict([[1, "a"], [2, "b"]]);
    assert.deepEqual([...d], [1, 2]);
    assert.deepEqual([...d.values()], ["a", "b"]);
    assert.deepEqual([...d.entries()], [[1, "a"], [2, "b"]]);
});

test("#83 pySetItem: shape dispatch (list index semantics / Map set / plain assign / proto-safe)", async () => {
    const { pySetItem, PyDict, IndexError } = await import("./runtime.js");
    const xs = [1, 2, 3];
    pySetItem(xs, -1, 9);
    assert.deepEqual(xs, [1, 2, 9]);
    assert.throws(() => pySetItem(xs, 3, 0), IndexError); // no silent JS hole-growing
    const d = new PyDict();
    pySetItem(d, true, "t");
    assert.deepEqual([...d.entries()], [[true, "t"]]);
    const o = {};
    pySetItem(o, "k", 1);
    assert.equal(o.k, 1);
    pySetItem(o, "__proto__", 7); // must create an own key, not mutate the prototype
    assert.equal(Object.getPrototypeOf(o), Object.prototype);
    assert.equal(Object.getOwnPropertyDescriptor(o, "__proto__").value, 7);
});

test("#83 pyDictKeys/pyDictValues/pyDictItems: both shapes; items are tuples", async () => {
    const { pyDictKeys, pyDictValues, pyDictItems, PyDict } = await import("./runtime.js");
    assert.deepEqual(pyDictKeys({ a: 1, b: 2 }), ["a", "b"]);
    assert.deepEqual(pyDictValues({ a: 1, b: 2 }), [1, 2]);
    const d = new PyDict([[1, "x"]]);
    assert.deepEqual(pyDictKeys(d), [1]);
    assert.deepEqual(pyDictValues(d), ["x"]);
    const items = pyDictItems(d);
    assert.deepEqual(items, [[1, "x"]]);
    assert.equal(items[0].__pytuple__, true); // repr(d.items()) shows (k, v)
    assert.equal(pyDictItems({ a: 1 })[0].__pytuple__, true);
});

test("#83 pyDictMerge: plain in → plain out; any Map-backed part → PyDict out; order kept", async () => {
    const { pyDictMerge, PyDict } = await import("./runtime.js");
    const plain = pyDictMerge({ a: 1 }, { b: 2 });
    assert.equal(Object.getPrototypeOf(plain), Object.prototype);
    assert.deepEqual(plain, { a: 1, b: 2 });
    const mixed = pyDictMerge({ s: 0 }, new PyDict([[1, "x"]]), { s: 9 });
    assert.ok(mixed instanceof PyDict);
    assert.deepEqual([...mixed.entries()], [["s", 9], [1, "x"]]);
});

test("#83 pyDict factory: dict() builtin — shape chosen by keys at runtime", async () => {
    const { pyDict, PyDict } = await import("./runtime.js");
    assert.deepEqual(pyDict(), {});
    const fromPairs = pyDict([[1, "a"]]);
    assert.ok(fromPairs instanceof PyDict);
    // all-string keys → plain object (the documented PyDict→JS escape hatch)
    const plain = pyDict(new PyDict([["a", 1]]));
    assert.equal(Object.getPrototypeOf(plain), Object.prototype);
    assert.deepEqual(plain, { a: 1 });
});

test("#83 pyGetItem: Map-backed dict reads + interop passthrough for non-plain objects", async () => {
    const { pyGetItem, PyDict, KeyError: KErr } = await import("./runtime.js");
    const d = new PyDict([[1, "a"]]);
    assert.equal(pyGetItem(d, 1), "a");
    assert.equal(pyGetItem(d, 1.0), "a");
    assert.throws(() => pyGetItem(d, "1"), (e) => e.name === "KeyError");
    // class instances keep native subscript semantics (no KeyError)
    class Wrapper { constructor() { this.x = 1; } }
    assert.equal(pyGetItem(new Wrapper(), "x"), 1);
    assert.equal(pyGetItem(new Wrapper(), "missing"), undefined);
});

test("#83 pyPop/pyDictSetdefault/pyDictPopitem/pyCopy/pyUpdate: Map-backed shapes", async () => {
    const { pyPop, pyDictSetdefault, pyDictPopitem, pyCopy, pyUpdate, PyDict } = await import("./runtime.js");
    const d = new PyDict([[1, "a"], [2, "b"]]);
    assert.equal(pyPop(d, 1), "a");
    assert.equal(pyPop(d, 99, "dflt"), "dflt");
    assert.equal(pyDictSetdefault(d, 3, "c"), "c");
    assert.equal(pyDictSetdefault(d, 2, "zz"), "b");
    const it = pyDictPopitem(d); // last-inserted
    assert.deepEqual(it, [3, "c"]);
    assert.equal(it.__pytuple__, true);
    const cp = pyCopy(d);
    assert.ok(cp instanceof PyDict);
    cp.set(7, "new");
    assert.equal(d.has(7), false);
    pyUpdate(d, new PyDict([[5, "e"]]), { s: 1 });
    assert.deepEqual([...d.keys()], [2, 5, "s"]);
    const plainTarget = {};
    pyUpdate(plainTarget, new PyDict([["k", "v"]]));
    assert.deepEqual(plainTarget, { k: "v" });
});

test("#83 pyEq: dict equality across shapes; key type identity respected", async () => {
    const { pyEq } = await import("./operators.js");
    const { PyDict } = await import("./runtime.js");
    assert.equal(pyEq(new PyDict([["a", 1]]), { a: 1 }), true);
    assert.equal(pyEq({ a: 1 }, new PyDict([["a", 1]])), true);
    assert.equal(pyEq(new PyDict([[1, "a"]]), { "1": "a" }), false); // int key ≠ str key
    assert.equal(pyEq(new PyDict([[1, "a"]]), new PyDict([[1.0, "a"]])), true);
});

// ── Pythonic-checks sweep: lazy pyZip + pySeq ────────────────────────

test("pythonic-checks pyZip: lazy with infinite iterators, tuple-marked rows", async () => {
    const { pyZip } = await import("./runtime.js");
    function* count() { let n = 0; while (true) yield n++; }
    const rows = [...pyZip(count(), "abc")];
    assert.equal(rows.length, 3);
    assert.deepEqual(rows[0], [0, "a"]);
    assert.equal(rows[0].__pytuple__, true);
});

test("pythonic-checks pyZip: one-shot; zip() empty; 3+ iterables", async () => {
    const { pyZip } = await import("./runtime.js");
    const z = pyZip([1, 2], "ab");
    assert.equal([...z].length, 2);
    assert.equal([...z].length, 0); // exhausted (CPython one-shot)
    assert.deepEqual([...pyZip()], []);
    assert.deepEqual([...pyZip([1], "a", [true])], [[1, "a", true]]);
});

test("pythonic-checks pyZip: strict=True raises CPython's ValueError", async () => {
    const { pyZip } = await import("./runtime.js");
    assert.deepEqual([...pyZip([1, 2], "ab", { strict: true })], [[1, "a"], [2, "b"]]);
    assert.throws(() => [...pyZip([1, 2], [1], { strict: true })],
        (e) => e.name === "ValueError" && e.message === "zip() argument 2 is shorter than argument 1");
    assert.throws(() => [...pyZip([1], [1, 2], { strict: true })],
        (e) => e.name === "ValueError" && e.message === "zip() argument 2 is longer than argument 1");
    assert.throws(() => [...pyZip([1, 2], [1, 2], [1], { strict: true })],
        (e) => e.name === "ValueError" && e.message === "zip() argument 3 is shorter than arguments 1-2");
});

test("pythonic-checks pySeq: materializes strings/generators/Maps/plain dicts", async () => {
    const { pySeq } = await import("./runtime.js");
    const arr = [1, 2];
    assert.equal(pySeq(arr), arr); // identity for arrays
    assert.deepEqual(pySeq("ab"), ["a", "b"]);
    function* g() { yield 1; yield 2; }
    assert.deepEqual(pySeq(g()), [1, 2]);
    assert.deepEqual(pySeq(new Map([["k", 1]])), ["k"]); // Python iterates dict KEYS
    assert.deepEqual(pySeq({ a: 1, b: 2 }), ["a", "b"]);
    assert.throws(() => pySeq(null), (e) => e.name === "TypeError");
});

// ═══════════════════════════════════════════════════════════════════════════
// Security regression suite — codex scan 2026-08-12 (report-2026-08-12.md).
// Reproducers: security-scan PoCs D-{7,8,11}
// ═══════════════════════════════════════════════════════════════════════════

// ── SEC-7 (CWE-1321) prototype pollution in the dict/kwargs write helpers ──
//
// `o[k] = v` with k === "__proto__" invokes the inherited
// Object.prototype.__proto__ SETTER: it reparents `o` instead of storing a
// key. Each test therefore asserts BOTH halves — the own data key exists
// (Python semantics: `"__proto__" in d` is True) and the receiver's prototype
// is unchanged (security: no inherited attacker state).

// Remote input arrives with an OWN "__proto__" data key. A `{__proto__: x}`
// object LITERAL would set the prototype instead, so build it via JSON.parse
// exactly the way a real payload does.
const attackerPayload = () => JSON.parse('{"__proto__": {"isAdmin": true}}');

function assertProtoSafe(o, key = "__proto__") {
    assert.ok(Object.prototype.hasOwnProperty.call(o, key),
        `expected an OWN "${key}" data key (Python semantics)`);
    assert.strictEqual(Object.getPrototypeOf(o), Object.prototype,
        "receiver prototype must be untouched (no pollution)");
    assert.strictEqual(o.isAdmin, undefined,
        "no attacker property may be inherited");
}

test("SEC-7 pyDictSetdefault: __proto__ is a data key, not a prototype write", () => {
    const d = {};
    const ret = pyDictSetdefault(d, "__proto__", { isAdmin: true });
    assert.deepEqual(ret, { isAdmin: true });
    assertProtoSafe(d);
    // second call must now see the key as present and NOT overwrite it
    assert.deepEqual(pyDictSetdefault(d, "__proto__", { other: 1 }), { isAdmin: true });
    // ordinary keys are unaffected
    assert.equal(pyDictSetdefault(d, "theme", "light"), "light");
    assert.equal(d.theme, "light");
});

test("SEC-7 pyUpdate: a JSON payload's __proto__ key cannot reparent the dict", async () => {
    const { pyUpdate } = await import("./runtime.js");
    const d = {};
    pyUpdate(d, attackerPayload());
    assertProtoSafe(d);
    // Map-backed source takes the other branch of pyUpdate
    const d2 = {};
    pyUpdate(d2, new Map([["__proto__", { isAdmin: true }]]));
    assertProtoSafe(d2);
    // normal merges still behave
    const d3 = { a: 1 };
    pyUpdate(d3, { b: 2 }, { c: 3 });
    assert.deepEqual(d3, { a: 1, b: 2, c: 3 });
});

test("SEC-7 __pyKwArgs: **kwargs cannot reparent the options object or **rest", async () => {
    const { __pyKwArgs } = await import("./runtime.js");
    // legacy path: no __pyparams__ metadata -> trailing options object
    const opts = __pyKwArgs(undefined, [], attackerPayload());
    assertProtoSafe(opts[opts.length - 1]);
    // **rest bucket of a Python-declared function
    function f(a) {}
    f.__pyparams__ = ["a"];
    f.__pykw__ = true;
    const args = __pyKwArgs(f, [1], attackerPayload());
    assert.equal(args[0], 1);
    assertProtoSafe(args[1]);
});

test("SEC-7 pySetItem / pyDictMerge keep __proto__ as data (centralized helper)", async () => {
    const { pySetItem, pyDictMerge, __pyDictWrite } = await import("./runtime.js");
    const d = {};
    pySetItem(d, "__proto__", { isAdmin: true });
    assertProtoSafe(d);
    assertProtoSafe(pyDictMerge({}, attackerPayload()));
    // the primitive itself
    const o = {};
    __pyDictWrite(o, "__proto__", { isAdmin: true });
    assertProtoSafe(o);
    __pyDictWrite(o, "x", 1);
    assert.equal(o.x, 1);
});

test("SEC-7 Object.prototype is never globally polluted by any dict helper", async () => {
    const { pyUpdate, pyDictMerge } = await import("./runtime.js");
    pyUpdate({}, attackerPayload());
    pyDictMerge({}, attackerPayload());
    pyDictSetdefault({}, "__proto__", { isAdmin: true });
    assert.strictEqual({}.isAdmin, undefined, "Object.prototype was polluted");
});

// ── SEC-11 (CWE-400) pyRange argument guards ──────────────────────────────
//
// pyRange materializes its whole result, so argument shapes CPython rejects
// used to become unbounded work: a non-finite bound made the fill loop
// infinite (hang with NO attacker-chosen size), and an explicit zero step
// silently became 1.

test("SEC-11 pyRange rejects non-finite bounds instead of looping forever", async () => {
    const { pyRange } = await import("./runtime.js");
    const isTypeError = (e) => e.name === "TypeError"
        && e.message === "'float' object cannot be interpreted as an integer";
    assert.throws(() => pyRange(Infinity), isTypeError);
    assert.throws(() => pyRange(-Infinity), isTypeError);
    assert.throws(() => pyRange(0, Infinity), isTypeError);
    assert.throws(() => pyRange(0, 10, Infinity), isTypeError);
    assert.throws(() => pyRange(NaN), isTypeError);
    assert.throws(() => pyRange(0, NaN), isTypeError);
});

test("SEC-11 pyRange rejects an explicit zero step (was silently treated as 1)", async () => {
    const { pyRange } = await import("./runtime.js");
    assert.throws(() => pyRange(0, 10, 0),
        (e) => e.name === "ValueError" && e.message === "range() arg 3 must not be zero");
    // `no step given` must stay distinguishable from `step = 0`
    assert.equal(pyRange(0, 5, undefined).length, 5);
    assert.equal(pyRange(0, 5, null).length, 5);
});

test("SEC-11 pyRange fails fast on a length no JS array could hold", async () => {
    const { pyRange } = await import("./runtime.js");
    assert.throws(() => pyRange(0, 1e10),
        (e) => e.name === "OverflowError" && e.message === "range() result has too many items");
});

test("SEC-11 pyRange guards do not regress ordinary ranges", async () => {
    const { pyRange } = await import("./runtime.js");
    assert.deepEqual(pyRange(5), [0, 1, 2, 3, 4]);
    assert.deepEqual(pyRange(2, 7), [2, 3, 4, 5, 6]);
    assert.deepEqual(pyRange(0, 10, 2), [0, 2, 4, 6, 8]);
    assert.deepEqual(pyRange(10, 0, -2), [10, 8, 6, 4, 2]);
    assert.deepEqual(pyRange(0), []);
    assert.deepEqual(pyRange(true), [0]); // bool ⊆ int
    assert.deepEqual(pyRange(5, 0), []);  // empty, not an error
});

// ── SEC-8 (CWE-116) cookie delimiter injection ────────────────────────────
//
// document.cookie is a SERIALIZED grammar: ";" separates the cookie from its
// attributes. Encoding only the value left name/path/SameSite as raw
// injection points. storage.js is imported dynamically so the document shim
// is installed first.

async function withCookieJar(fn) {
    const prev = globalThis.document;
    const jar = new Map();
    let last = "";
    globalThis.document = {
        set cookie(str) {
            last = str;
            const [head, ...attrs] = str.split(";").map((s) => s.trim());
            const eq = head.indexOf("=");
            jar.set(head.slice(0, eq), { value: head.slice(eq + 1), attrs });
        },
        get cookie() {
            return [...jar].map(([k, v]) => `${k}=${v.value}`).join("; ");
        },
    };
    try {
        const { cookies } = await import("../../../runtime/src/web/storage.js");
        return fn(cookies, jar, () => last);
    } finally {
        globalThis.document = prev;
    }
}

const isCookieValueError = (e) => e.name === "ValueError";

test("SEC-8 cookies.set rejects delimiters in the cookie NAME", async () => {
    await withCookieJar((cookies, jar) => {
        for (const bad of [
            "harmless=1; session",           // ";" -> forges a second pair
            "session=admin",                 // "=" -> retargets the cookie
            "a\r\nSet-Cookie: x=y",          // CRLF -> header splitting
            "has space",
            "",                              // empty is not a token
            "a,b", "a;b", 'a"b', "a\tb",
        ]) {
            assert.throws(() => cookies.set(bad, "v"), isCookieValueError,
                `name ${JSON.stringify(bad)} must be rejected`);
        }
        assert.equal(jar.size, 0, "no cookie may be written by a rejected call");
    });
});

test("SEC-8 cookies.set rejects delimiters in PATH and SameSite", async () => {
    await withCookieJar((cookies, jar) => {
        assert.throws(() => cookies.set("s", "v", { path: "/; Domain=evil.example" }), isCookieValueError);
        assert.throws(() => cookies.set("s", "v", { path: "/\r\nX: y" }), isCookieValueError);
        assert.throws(() => cookies.set("s", "v", { same_site: "None; Domain=evil.example" }), isCookieValueError);
        assert.throws(() => cookies.set("s", "v", { same_site: "Bogus" }), isCookieValueError);
        assert.throws(() => cookies.set("s", "v", { days: "1; Secure" }), isCookieValueError);
        assert.equal(jar.size, 0);
    });
});

test("SEC-8 cookies.delete validates the same fields as set", async () => {
    await withCookieJar((cookies, jar) => {
        assert.throws(() => cookies.delete("a; Domain=evil.example"), isCookieValueError);
        assert.throws(() => cookies.delete("s", { path: "/; Secure" }), isCookieValueError);
        assert.equal(jar.size, 0);
    });
});

test("SEC-8 cookies round-trip normally for legitimate input", async () => {
    await withCookieJar((cookies, jar, last) => {
        cookies.set("session", "abc def/=&", { days: 7, secure: true, same_site: "strict" });
        const rec = jar.get("session");
        assert.equal(rec.value, encodeURIComponent("abc def/=&"));
        assert.ok(rec.attrs.includes("path=/"));
        assert.ok(rec.attrs.includes("SameSite=Strict"), "same_site is normalized to canonical casing");
        assert.ok(rec.attrs.includes("Secure"));
        assert.ok(rec.attrs.some((a) => a.startsWith("expires=")));
        assert.equal(cookies.get("session"), "abc def/=&");
        assert.equal(cookies.has("session"), true);
        assert.equal(cookies.get("missing", "fallback"), "fallback");
        // token names with the odd-but-legal RFC 6265 punctuation still work
        cookies.set("a!#$%&'*+-.^_`|~9", "v", { path: "/sub/path" });
        assert.ok(jar.has("a!#$%&'*+-.^_`|~9"));
        assert.ok(last().includes("path=/sub/path"));
    });
});

// ── F7 (CVE-2026-15903 JS-path sibling) ────────────────────────────────────
// A `None`/invalid index must raise TypeError like CPython, not slip
// pyGetItem's type validation and silently return undefined -> None.
test("F7 pyGetItem: None/undefined index on a sequence raises TypeError", async () => {
    const { pyGetItem } = await import("./runtime.js");
    // list
    assert.throws(() => pyGetItem([10, 20, 30], null),
        (e) => e.name === "TypeError" && /list indices must be integers or slices, not NoneType/.test(e.message));
    assert.throws(() => pyGetItem([10, 20, 30], undefined),
        (e) => e.name === "TypeError" && /not NoneType/.test(e.message));
    // string
    assert.throws(() => pyGetItem("abc", null),
        (e) => e.name === "TypeError" && /string indices must be integers, not 'NoneType'/.test(e.message));
    assert.throws(() => pyGetItem("abc", undefined),
        (e) => e.name === "TypeError");
});

test("F7 pyGetItem: other non-integer index types on a sequence raise TypeError", async () => {
    const { pyGetItem } = await import("./runtime.js");
    assert.throws(() => pyGetItem([10, 20, 30], {}), (e) => e.name === "TypeError");
    assert.throws(() => pyGetItem([10, 20, 30], []), (e) => e.name === "TypeError" && /not list/.test(e.message));
    assert.throws(() => pyGetItem([10, 20, 30], Symbol("x")), (e) => e.name === "TypeError" && /not symbol/.test(e.message));
    assert.throws(() => pyGetItem("abc", {}), (e) => e.name === "TypeError");
});

test("F7 pyGetItem: valid indices and hashable dict keys are unaffected", async () => {
    const { pyGetItem } = await import("./runtime.js");
    // valid integer / negative / whole-float(B1) / string index all still work
    assert.equal(pyGetItem([10, 20, 30], 1), 20);
    assert.equal(pyGetItem([10, 20, 30], -1), 30);
    assert.equal(pyGetItem([10, 20, 30], 1.0), 20); // documented whole-float deviation B1
    assert.equal(pyGetItem("abc", 0), "a");
    // None is a legal *hashable* dict key — must stay legal (not a sequence).
    assert.equal(pyGetItem(new Map([[null, 7]]), null), 7);
});

// ── Round-2 review fixes (R1/R3/R4/R5/R7/R8 + __pyRangeArgs) ────────────────
test("R1 __pyDictWrite: a coercible key whose string form is __proto__ is data", async () => {
    const { __pyDictWrite, pyUpdate } = await import("./runtime.js");
    const o = {};
    __pyDictWrite(o, new String("__proto__"), { isAdmin: true });
    assert.ok(Object.prototype.hasOwnProperty.call(o, "__proto__"), "own data key created");
    assert.equal(o.isAdmin, undefined, "prototype NOT reparented");
    // via pyUpdate from a Map with a boxed-String key
    const d = {};
    pyUpdate(d, new Map([[new String("__proto__"), { isAdmin: true }]]));
    assert.ok(Object.prototype.hasOwnProperty.call(d, "__proto__"));
    assert.equal(d.isAdmin, undefined);
    // ordinary keys unaffected
    const e = {};
    __pyDictWrite(e, "x", 1); __pyDictWrite(e, 5, 2);
    assert.equal(e.x, 1); assert.equal(e[5], 2);
    // R1 (delta): a Symbol.toPrimitive that returns "__proto__" only on its
    // SECOND call must not slip through — `o[pk]` writes the already-coerced key.
    let n = 0;
    const k = { [Symbol.toPrimitive]() { return ++n === 1 ? "safe" : "__proto__"; } };
    const g = {};
    __pyDictWrite(g, k, { isAdmin: true });
    assert.equal(g.isAdmin, undefined, "double-coerce must not reparent the prototype");
    assert.ok(Object.prototype.hasOwnProperty.call(g, "safe"), "the effective key is stored as data");
});

test("R4 pyRange: coercible non-number bounds raise TypeError", async () => {
    const { pyRange } = await import("./runtime.js");
    assert.throws(() => pyRange("Infinity"), (e) => e.name === "TypeError");
    assert.throws(() => pyRange(null), (e) => e.name === "TypeError");
    assert.throws(() => pyRange(0, "5"), (e) => e.name === "TypeError");
});

test("R3 pyRange: BigInt bounds hit the oversized-result guard", async () => {
    const { pyRange } = await import("./runtime.js");
    assert.throws(() => pyRange(10n ** 100n), (e) => e.name === "OverflowError");
    assert.deepEqual(pyRange(3n), [0n, 1n, 2n]); // small bigint range materializes
});

test("R5 pyRange: near-2**53 Number ranges terminate (no non-progress hang)", async () => {
    const { pyRange } = await import("./runtime.js");
    const r = pyRange(9007199254740992, 9007199254740994); // 2**53 .. 2**53+2
    assert.equal(r.length, 2);
});

test("R7 pyRange: huge finite Number bounds do not false-OverflowError", async () => {
    const { pyRange } = await import("./runtime.js");
    const r = pyRange(Math.trunc(-1e308), Math.trunc(1e308), Math.trunc(1e308));
    assert.equal(r.length, 2); // stop-start overflows to Infinity in Number, not in BigInt
});

test("R8 pyGetItem: tuple/dict index type names are correct", async () => {
    const { pyGetItem } = await import("./runtime.js");
    const tup = [1]; tup.__pytuple__ = true;
    assert.throws(() => pyGetItem(tup, null),
        (e) => e.name === "TypeError" && /tuple indices must be integers or slices, not NoneType/.test(e.message));
    assert.throws(() => pyGetItem([1], {}),
        (e) => e.name === "TypeError" && /list indices must be integers or slices, not dict/.test(e.message));
});

test("R2 __pyRangeIter: shared lazy iterator — same guards, bool/bigint/2**53-safe", async () => {
    const { __pyRangeIter } = await import("./runtime.js");
    const take = (g, k) => { const a = []; for (const x of g) { a.push(x); if (a.length >= k) break; } return a; };
    assert.throws(() => [...__pyRangeIter(1, 0, 0)], (e) => e.name === "ValueError"); // zero step
    assert.throws(() => [...__pyRangeIter(0, Infinity, 1)], (e) => e.name === "TypeError"); // non-finite
    assert.throws(() => [...__pyRangeIter(0, "5", 1)], (e) => e.name === "TypeError"); // non-number
    assert.throws(() => [...__pyRangeIter(0, 2, 0.5)], (e) => e.name === "TypeError"); // float arg
    assert.deepEqual([...__pyRangeIter(true)], [0]); // bool normalized
    // near-2**53: values must be CORRECT (not just count) — promoted to BigInt
    // so 2**53+1 is exact instead of a duplicate 2**53.
    assert.deepEqual([...__pyRangeIter(9007199254740992, 9007199254740994)].map(String),
        ["9007199254740992", "9007199254740993"]);
    // small ranges stay Number-typed
    assert.deepEqual([...__pyRangeIter(3)], [0, 1, 2]);
    // bigint bounds: no Number/BigInt mix crash
    assert.deepEqual([...__pyRangeIter(9007199254740992n, 9007199254740994n)].map(String),
        ["9007199254740992", "9007199254740993"]);
    // lazy: a 10**100 range yields without materializing
    assert.deepEqual(take(__pyRangeIter(0n, 10n ** 100n, 1n), 3).map(String), ["0", "1", "2"]);
});

test("pyRange guards do not regress ordinary ranges (post round-2)", async () => {
    const { pyRange } = await import("./runtime.js");
    assert.deepEqual(pyRange(5), [0, 1, 2, 3, 4]);
    assert.deepEqual(pyRange(2, 10, 2), [2, 4, 6, 8]);
    assert.deepEqual(pyRange(10, 0, -1), [10, 9, 8, 7, 6, 5, 4, 3, 2, 1]);
    assert.deepEqual(pyRange(0, -5), []);
    assert.throws(() => pyRange(0, 10, 0), (e) => e.name === "ValueError");
    assert.throws(() => pyRange(Infinity), (e) => e.name === "TypeError");
});

// ── Round-4 delta fixes ────────────────────────────────────────────────────
test("delta: every plain-dict op coerces the key EXACTLY ONCE", async () => {
    const { pyDictGet, pyDictSetdefault, pyPop, pyContains } = await import("./runtime.js");
    const { __pyDictWrite } = await import("./runtime.js");
    // A Symbol.toPrimitive key that changes value across coercions would make a
    // probe and an access disagree if any op coerced twice.
    const makeKey = () => {
        let c = 0;
        const k = { [Symbol.toPrimitive]() { c++; return "kk"; } };
        return { k, count: () => c };
    };
    for (const [name, op] of [
        ["pyDictGet", (o, k) => pyDictGet(o, k, null)],
        ["pyDictSetdefault", (o, k) => pyDictSetdefault(o, k, 1)],
        ["pyPop", (o, k) => pyPop(o, k, null)],
        ["pyContains", (o, k) => pyContains(o, k)],
        ["__pyDictWrite", (o, k) => __pyDictWrite(o, k, 1)],
    ]) {
        const key = makeKey();
        op({}, key.k);
        assert.equal(key.count(), 1, `${name} must coerce the key exactly once (got ${key.count()})`);
    }
});

test("delta: setdefault with a re-coercing __proto__ key cannot pollute", async () => {
    const { pyDictSetdefault } = await import("./runtime.js");
    let n = 0;
    const k = { [Symbol.toPrimitive]() { return ++n === 1 ? "safe" : "__proto__"; } };
    const d = {};
    pyDictSetdefault(d, k, 7);
    assert.equal(d.isAdmin, undefined, "prototype must not be reparented");
    assert.ok(Object.prototype.hasOwnProperty.call(d, "safe"), "the single coerced key is stored as data");
    assert.ok(!Object.prototype.hasOwnProperty.call(d, "__proto__"), "no __proto__ key");
});

test("delta: tuple OUT-OF-RANGE message says 'tuple', not 'list'", async () => {
    const { pyGetItem } = await import("./runtime.js");
    const t = [10, 20, 30]; Object.defineProperty(t, "__pytuple__", { value: true });
    assert.throws(() => pyGetItem(t, 5),
        (e) => e.name === "IndexError" && e.message === "tuple index out of range");
    assert.throws(() => pyGetItem(t, 9007199254740992n), // huge bigint index
        (e) => e.name === "IndexError" && e.message === "tuple index out of range");
    // list still says "list"
    assert.throws(() => pyGetItem([1], 5),
        (e) => e.name === "IndexError" && e.message === "list index out of range");
});

test("delta: pyRange yields exact BigInt beyond 2**53 (no duplicates)", async () => {
    const { pyRange } = await import("./runtime.js");
    assert.deepEqual(pyRange(9007199254740992, 9007199254740994).map(String),
        ["9007199254740992", "9007199254740993"]);
    assert.deepEqual(pyRange(3), [0, 1, 2]); // small stays Number
});

// ── delta4 fixes ────────────────────────────────────────────────────────────
// 1) coerce-once invariant on ALL subscript paths (read/delete, not just
//    write); 2) Symbol keys preserved through __pyPropKey and dict
//    merge/update; 3) range BigInt promotion decided by INTERMEDIATE
//    arithmetic, not just endpoints.
import {
    pyGetItem as d4GetItem,
    pyDelItem as d4DelItem,
    pyDictMerge as d4DictMerge,
    pyUpdate as d4Update,
    __pyPropKey as d4PropKey,
    pyRange as d4Range,
    __pyRangeIter as d4RangeIter,
} from "./runtime.js";

/** A key whose Symbol.toPrimitive yields DIFFERENT values per coercion. */
function evilKey(first, rest) {
    let calls = 0;
    return { [Symbol.toPrimitive]() { return calls++ === 0 ? first : rest; } };
}

test("delta4: pyGetItem coerces the key exactly once (read cannot hit the wrong slot)", () => {
    const obj = { safe: 7, other: 9 };
    assert.equal(d4GetItem(obj, evilKey("safe", "other")), 7);
});

test("delta4: pyGetItem KeyError decided on the SAME coercion as the read", () => {
    // First coercion names a MISSING key: must throw KeyError, never return
    // the value of the second coercion's key.
    const obj = { present: 1 };
    assert.throws(() => d4GetItem(obj, evilKey("missing", "present")), (e) => e.name === "KeyError");
});

test("delta4: pyDelItem coerces the key exactly once (delete cannot remove the wrong key)", () => {
    const obj = { safe: 7, other: 9 };
    d4DelItem(obj, evilKey("safe", "other"));
    assert.deepEqual(Object.keys(obj), ["other"]);
    assert.equal(obj.other, 9);
});

test("delta4: __pyPropKey passes a Symbol.toPrimitive->Symbol through (native ToPropertyKey), not throw", () => {
    const s = Symbol("k");
    const key = { [Symbol.toPrimitive]() { return s; } };
    assert.equal(d4PropKey(key), s);
    // and it still coerces exactly once for the whole get/set path
    const obj = { [s]: 42 };
    assert.equal(d4GetItem(obj, key), 42);
});

test("delta4: pyDictMerge preserves Symbol-keyed entries (Reflect-own-keys walk)", () => {
    const s = Symbol("sym");
    const merged = d4DictMerge({ a: 1 }, { [s]: 7 });
    assert.equal(merged[s], 7);
    assert.equal(merged.a, 1);
});

test("delta4: pyUpdate preserves Symbol-keyed entries on plain and Map receivers", () => {
    const s = Symbol("sym");
    const plain = { a: 1 };
    d4Update(plain, { [s]: 7 });
    assert.equal(plain[s], 7);
    const m = new Map();
    d4Update(m, { [s]: 8 });
    assert.equal(m.get(s), 8);
});

test("delta4: range with SAFE endpoints but UNSAFE interior arithmetic yields exact values", () => {
    // start/stop/last are all within ±(2**53-1), but the Number loop's
    // intermediate i*step reaches ~1.8e16 — endpoint-only promotion produced
    // 4th value 1 (should be 2) and 6th value ...665 (should be ...664).
    const start = -9007199254740991, stop = 9007199254740991, step = 3002399751580331;
    const xs = d4Range(start, stop, step);
    // exact expectations (BigInt-promoted because the span exceeds 2**53-1):
    assert.equal(BigInt(xs[3]), 2n);
    assert.equal(BigInt(xs[5]), 6004799503160664n);
    // lazy iterator agrees with the materializing pyRange
    const it = d4RangeIter(start, stop, step);
    const lazy = [];
    for (const v of it) lazy.push(v);
    assert.deepEqual(lazy.map(BigInt), xs.map(BigInt));
});

test("delta4: safe-span ranges still take the Number fast path", () => {
    const xs = d4Range(0, 5, 1);
    assert.deepEqual(xs, [0, 1, 2, 3, 4]);
    assert.ok(xs.every((v) => typeof v === "number"));
    // endpoints near the safe boundary with a TINY span stay Numbers too
    const near = d4Range(9007199254740980, 9007199254740991, 3);
    assert.ok(near.every((v) => typeof v === "number"));
    assert.deepEqual(near, [9007199254740980, 9007199254740983, 9007199254740986, 9007199254740989]);
});

// ── delta4 round-6: Symbol keys uniform across ALL dict ops ────────────────
// Round 5 fixed merge/update only; a Symbol-keyed entry (raw-JS interop —
// Python itself cannot create one) then survived the merge but was invisible
// to len/keys/values/items/bool/eq/popitem/clear/iteration/repr/dict().
// One owned-keys helper (__pyOwnKeys) is now used by every plain-object
// dict op, so the ops agree with each other.
import {
    pyLen as r6Len,
    pyDict as r6Dict,
    PyDict as R6PyDict,
    pyDictKeys as r6Keys,
    pyDictValues as r6Values,
    pyDictItems as r6Items,
    pyDictPopitem as r6Popitem,
    pyClear as r6Clear,
    pySeq as r6Seq,
    pyForIter as r6ForIter,
} from "./runtime.js";
import { pyEq as r6Eq, pyRepr as r6Repr } from "../../../runtime/src/operators.js";
import { pyBool as r6Bool } from "../../../runtime/src/types.js";

test("r6: len/bool/keys/values/items see Symbol-keyed entries", () => {
    const sym = Symbol("s");
    const d = { a: 1, [sym]: 2 };
    assert.equal(r6Len(d), 2);
    assert.equal(r6Bool({ [sym]: 1 }), true);
    assert.deepEqual(r6Keys(d), ["a", sym]);
    assert.deepEqual(r6Values(d), [1, 2]);
    const items = r6Items(d);
    assert.deepEqual(items.map((p) => p[0]), ["a", sym]);
    assert.deepEqual(items.map((p) => p[1]), [1, 2]);
});

test("r6: iteration (pySeq/pyForIter) yields Symbol keys too", () => {
    const sym = Symbol("s");
    const d = { a: 1, [sym]: 2 };
    assert.deepEqual(r6Seq(d), ["a", sym]);
    assert.deepEqual([...r6ForIter(d)], ["a", sym]);
});

test("r6: pyEq distinguishes and matches Symbol-keyed entries", () => {
    const sym = Symbol("s");
    assert.equal(r6Eq({ a: 1, [sym]: 2 }, { a: 1, [sym]: 2 }), true);
    assert.equal(r6Eq({ a: 1, [sym]: 2 }, { a: 1 }), false);
    assert.equal(r6Eq({ a: 1 }, { a: 1, [sym]: 2 }), false);
    const other = Symbol("s"); // same description, DIFFERENT key
    assert.equal(r6Eq({ [sym]: 2 }, { [other]: 2 }), false);
});

test("r6: popitem pops a Symbol-keyed last entry; clear removes Symbol entries", () => {
    const sym = Symbol("s");
    const d = { a: 1, [sym]: 2 };
    const [k, v] = r6Popitem(d);
    assert.equal(k, sym);
    assert.equal(v, 2);
    const d2 = { a: 1, [sym]: 2 };
    r6Clear(d2);
    assert.equal(r6Len(d2), 0);
    assert.equal(d2[sym], undefined);
});

test("r6: dict()/PyDict conversion keeps Symbol entries; repr shows them", () => {
    const sym = Symbol("s");
    const src = { a: 1, [sym]: 2 };
    const viaFactory = r6Dict(src);
    // plain-object source with only string+symbol keys stays/converts with
    // both entries present, whichever backing shape the factory picks
    assert.equal(r6Len(viaFactory), 2);
    const viaCtor = new R6PyDict(src);
    assert.equal(viaCtor.size, 2);
    assert.equal(viaCtor.get(sym), 2);
    assert.ok(r6Repr(src).includes("Symbol(s): 2"));
});

// ── delta4 round-7: ** spread mapping keys must be strings ─────────────────
// CPython: f(**{1: 2}) / dict(**m) with a non-string key raises
// TypeError('keywords must be strings'). A plain-object spread silently
// DROPPED Symbol keys; a Map-backed spread let non-string keys flow into
// parameter binding with a wrong error.
import { __pyKwArgs as r7KwArgs, pyDict as r7DictF, PyDict as R7PyDict } from "./runtime.js";

test("r7: **spread with a Symbol key raises TypeError (was: silent drop)", () => {
    const sym = Symbol("s");
    const target = (...a) => a;
    target.__pyparams__ = ["a"];
    assert.throws(() => r7KwArgs(target, [], { a: 1, [sym]: 2 }),
        (e) => e.name === "TypeError" && /keywords must be strings/.test(e.message));
});

test("r7: **spread of a Map-backed dict with a non-string key raises TypeError", () => {
    const target = (...a) => a;
    target.__pyparams__ = ["a"];
    const m = new R7PyDict();
    m.set(1, "x");
    assert.throws(() => r7KwArgs(target, [], m),
        (e) => e.name === "TypeError" && /keywords must be strings/.test(e.message));
    // string-keyed mapping still binds fine
    const ok = new R7PyDict();
    ok.set("a", 7);
    assert.deepEqual(r7KwArgs(target, [], ok), [7]);
});

test("r7: dict(**m) with a Symbol key raises TypeError; string kwargs still work", () => {
    const sym = Symbol("s");
    assert.throws(() => r7DictF(null, { a: 1, [sym]: 2 }),
        (e) => e.name === "TypeError" && /keywords must be strings/.test(e.message));
    const d = r7DictF(null, { a: 1 });
    assert.equal(d.a ?? d.get?.("a"), 1);
});

// ── public #3: format / slice / ascii / vars builtins ───────────────────────

test("pyFormat: spec routes through the f-string engine", async () => {
    const { pyFormat } = await import("./runtime.js");
    assert.equal(pyFormat(3.14159, ".2f"), "3.14");
    assert.equal(pyFormat(255, "#06x"), "0x00ff");
    assert.equal(pyFormat("hi", ">4"), "  hi");
});

test("pyFormat: no/empty spec is str(); non-str spec raises TypeError", async () => {
    const { pyFormat } = await import("./runtime.js");
    assert.equal(pyFormat(42), "42");
    assert.equal(pyFormat(true, ""), "True");
    assert.equal(pyFormat(null), "None");
    assert.throws(() => pyFormat(1, 2), (e) => e.__name__ === "TypeError" || e.name === "TypeError" || /must be str/.test(e.message));
});

test("pySliceOf: CPython arg forms + pyGetItem dispatch", async () => {
    const { pySliceOf, pyGetItem } = await import("./runtime.js");
    assert.deepEqual(pyGetItem([1, 2, 3, 4], pySliceOf(1, 3)), [2, 3]);
    assert.equal(pyGetItem("hello", pySliceOf(3)), "hel");
    assert.deepEqual(pyGetItem([1, 2, 3, 4, 5], pySliceOf(null, null, -2)), [5, 3, 1]);
    const s = pySliceOf(1, 3);
    assert.equal(s.start, 1);
    assert.equal(s.stop, 3);
    assert.equal(s.step, null);
    assert.throws(() => pySliceOf(), /slice expected at least 1 argument/);
    assert.throws(() => pySliceOf(1, 2, 3, 4), /at most 3 arguments/);
});

test("pyGetItem: slice key on a dict is unhashable, like CPython", async () => {
    const { pySliceOf, pyGetItem } = await import("./runtime.js");
    assert.throws(() => pyGetItem({ a: 1 }, pySliceOf(1, 3)), /unhashable type: 'slice'/);
});

test("pyAscii: repr with non-ASCII escaped (CPython forms)", async () => {
    const { pyAscii } = await import("./runtime.js");
    assert.equal(pyAscii("café"), String.raw`'caf\xe9'`);
    assert.equal(pyAscii("héllo — 𝄞"), String.raw`'h\xe9llo \u` + String.raw`2014 \U0001d11e'`);
    assert.equal(pyAscii([1, "ü"]), String.raw`[1, '\xfc']`);
    assert.equal(pyAscii(42), "42");
});

test("pyVars: instance __dict__; non-instances raise TypeError", async () => {
    const { pyVars } = await import("./runtime.js");
    class P {}
    const p = new P();
    p.x = 1;
    p.y = "a";
    assert.deepEqual(pyVars(p), { x: 1, y: "a" });
    for (const bad of [{}, [1], 5, "s", null, new Map(), new Set()]) {
        assert.throws(() => pyVars(bad), /vars\(\) argument must have __dict__/);
    }
});

// WF-1: __pyEffect makes a use_effect callback's return safe as a React
// cleanup. React invokes a non-undefined, non-function return → crash; the
// wrapper coerces null/None/any-non-function to undefined and passes a real
// cleanup function through untouched.
test("__pyEffect coerces null/None return to undefined", () => {
    assert.equal(__pyEffect(() => null)(), undefined);
    assert.equal(__pyEffect(() => undefined)(), undefined);
    assert.equal(__pyEffect(() => {})(), undefined);
});

test("__pyEffect coerces non-function return to undefined", () => {
    assert.equal(__pyEffect(() => 5)(), undefined);
    assert.equal(__pyEffect(() => "x")(), undefined);
    assert.equal(__pyEffect(() => ({}))(), undefined);
});

test("__pyEffect passes a real cleanup function through and keeps it callable", () => {
    let ran = false;
    const cleanup = __pyEffect(() => () => { ran = true; })();
    assert.equal(typeof cleanup, "function");
    cleanup();
    assert.equal(ran, true);
});
