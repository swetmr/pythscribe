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
