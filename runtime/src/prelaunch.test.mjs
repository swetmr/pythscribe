// Runtime-helper coverage for the pre-launch probe fix batch (F2–F6).
// Mirrors the differential corpus but exercises the runtime/src helpers
// directly (the differential suite goes through `pyths compile`).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
    pyLen, pyGetItem, pySlice, pyContains, pyNext, pyIter,
    StopIteration, ZeroDivisionError,
} from "./runtime.js";
import { pyFloat, pyDiv, pyStr, pyRepr } from "./operators.js";
import { pyDictGet } from "./runtime.js";

// ---- F2: astral (surrogate-pair) strings ----
test("pyLen counts code points, not UTF-16 units", () => {
    assert.equal(pyLen("😀"), 1);
    assert.equal(pyLen("a😀b"), 3);
    assert.equal(pyLen("abc"), 3); // fast path unaffected
});
test("pyGetItem indexes strings by code point incl. negatives", () => {
    assert.equal(pyGetItem("a😀b", 1), "😀");
    assert.equal(pyGetItem("a😀b", -1), "b");
    assert.equal(pyGetItem("a😀b", 0), "a");
    assert.throws(() => pyGetItem("a😀b", 5), /string index out of range/);
});
test("pySlice slices strings by code point", () => {
    assert.equal(pySlice("a😀b", 1, 2, null), "😀");
    assert.equal(pySlice("a😀b", null, null, -1), "b😀a");
    assert.equal(pySlice("abc", 0, 2, null), "ab"); // fast path
});

// ---- F3: prototype-safe dicts ----
test("pyContains uses own-property semantics", () => {
    assert.equal(pyContains({}, "hasOwnProperty"), false);
    assert.equal(pyContains({ a: 1 }, "toString"), false);
    assert.equal(pyContains({ a: 1 }, "constructor"), false);
    assert.equal(pyContains({ x: 1 }, "x"), true);
});
test("pyDictGet ignores inherited members", () => {
    assert.equal(pyDictGet({}, "constructor", "def"), "def");
    assert.equal(pyDictGet({ a: 1 }, "a", "def"), 1);
});

// ---- WB-20: pyDictGet does a proxy-trappable read (MobX reactivity) ----
// A dict `.get(k)` lowered to pyDictGet must go through a plain `d[k]` property
// GET so a host observable Proxy's `get` trap fires and registers a dependency
// — including for an ABSENT key (the MobX case where an observer first reads a
// not-yet-populated slot, then must re-render once it is added). The old
// hasOwnProperty-guarded read short-circuited on a missing key and never
// tripped the `get` trap. This test simulates MobX's tracking with a Proxy
// that records the keys read through `get`, without depending on mobx.
test("pyDictGet reads through the Proxy get trap (present AND absent keys)", () => {
    const gets = [];
    const target = { a: 1 };
    const p = new Proxy(target, {
        get(t, key, recv) { gets.push(key); return Reflect.get(t, key, recv); },
    });
    // Present key: value returned AND the get trap saw it.
    assert.equal(pyDictGet(p, "a", "def"), 1);
    assert.ok(gets.includes("a"), "present key must be read via the get trap");
    // Absent key: default returned, but the get trap STILL fired for it so a
    // reactive host subscribes and re-renders when the key is later added.
    gets.length = 0;
    assert.equal(pyDictGet(p, "missing", "def"), "def");
    assert.ok(gets.includes("missing"), "absent key must still trip the get trap (WB-20)");
});

// Semantics preserved alongside the WB-20 fix.
test("pyDictGet preserves dict semantics (missing/inherited/own-undefined)", () => {
    assert.equal(pyDictGet({ a: 1 }, "b", "def"), "def");   // missing → default
    assert.equal(pyDictGet({ a: 1 }, "b"), undefined);       // missing, no default
    assert.equal(pyDictGet({}, "toString", "def"), "def");   // inherited → default
    assert.equal(pyDictGet({}, "hasOwnProperty", 7), 7);     // inherited → default
    // Own key holding a genuine `undefined` returns undefined, not the default
    // (own-property presence, not value-truthiness, decides).
    assert.equal(pyDictGet({ x: undefined }, "x", "def"), undefined);
    // Map-backed dict path unchanged.
    const m = new Map([["k", 5]]);
    assert.equal(pyDictGet(m, "k", "def"), 5);
    assert.equal(pyDictGet(m, "nope", "def"), "def");
});

// ---- F4: float fidelity ----
test("pyFloat maps inf/-inf/nan case-insensitively", () => {
    assert.equal(pyFloat("inf"), Infinity);
    assert.equal(pyFloat("-inf"), -Infinity);
    assert.equal(pyFloat("  Infinity  "), Infinity);
    assert.equal(pyFloat("INF"), Infinity);
    assert.ok(Number.isNaN(pyFloat("nan")));
    assert.equal(pyFloat("3.5"), 3.5);
    // Option B: float(2) is integer-valued -> boxed (brand + native value).
    assert.equal(pyFloat(2).__pyfloat__, true);
    assert.equal(Number(pyFloat(2)), 2);
    assert.throws(() => pyFloat("abc"), /could not convert string to float/);
});
test("pyDiv distinguishes float vs int division by zero", () => {
    assert.throws(() => pyDiv(1, 0), /^ZeroDivisionError|division by zero/);
    try { pyDiv(1, 0); } catch (e) { assert.equal(e.message, "division by zero"); }
    try { pyDiv(1, 0, true); } catch (e) { assert.equal(e.message, "float division by zero"); }
    try { pyDiv(1.5, 0); } catch (e) { assert.equal(e.message, "float division by zero"); }
});
test("exceptions stringify to message, not a dict dump", () => {
    const e = new ZeroDivisionError("float division by zero");
    assert.equal(pyStr(e), "float division by zero");
    assert.equal(pyRepr(e), "ZeroDivisionError('float division by zero')");
});

// ---- F5: next() over generators ----
test("pyNext advances a generator and raises StopIteration", () => {
    function* g() { yield 1; yield 2; }
    const it = g();
    assert.equal(pyNext(it), 1);
    assert.equal(pyNext(it), 2);
    assert.throws(() => pyNext(it), (err) => err.name === "StopIteration");
    // list() over a generator (Array.from path) is the codegen's list(g()).
    assert.deepEqual([...g()], [1, 2]);
    // pyIter still yields an iterator for for-loops.
    assert.equal(typeof pyIter(g()).next, "function");
});

// ---- Sweep-A fix batch (G) ----
import {
    pyStrTitle, pyStrSplit, pyChr, pyOrd, pyBin, pyHex, pyOct, pyMin, pyMax, pyFixed, pyDelItem,
    ValueError as VE,
} from "./runtime.js";
import {
    pyInt, pyDivmod, pySum, pyBitOr, pyBitAnd, pyBitXor, pySub,
    pyLt, pyLe, pyGt, pyGe, pyEq, pyShiftLeft, pyShiftRight,
} from "./operators.js";
import { pySorted, pySetSlice, pyListCount, pyStrReplace, pyListSort } from "./runtime.js";
import { __pyIsInstance } from "./classes.js";
import { copy as copyShallow, deepcopy } from "./stdlib/copy.js";
import * as pyString from "./stdlib/string.js";
import { Random } from "./stdlib/random.js";
import { pyBoundMethod } from "./runtime.js";
import * as heapq from "./stdlib/heapq.js";
import * as bisect from "./stdlib/bisect.js";
import { cmp_to_key } from "./stdlib/functools.js";

test("#266 pyBoundMethod binds dict .get to its receiver", () => {
    const m = new Map([["a", 1], ["b", 2]]);
    const g = pyBoundMethod(m, "get");
    assert.equal(g("b"), 2);
    // plain-object dict: synthesize the closure
    const g2 = pyBoundMethod({ x: 5 }, "get");
    assert.equal(g2("x"), 5);
    assert.equal(g2("z", 9), 9);
});

test("#229 heapq: tuple-priority heap + heapify/nlargest/nsmallest", () => {
    const h = [];
    heapq.heappush(h, [3, "c"]);
    heapq.heappush(h, [1, "a"]);
    heapq.heappush(h, [2, "b"]);
    assert.deepEqual(heapq.heappop(h), [1, "a"]); // lexicographic tuple order
    assert.deepEqual(heapq.heappop(h), [2, "b"]);
    const xs = [5, 1, 8, 3, 9, 2];
    heapq.heapify(xs);
    const drained = [];
    while (xs.length) drained.push(heapq.heappop(xs));
    assert.deepEqual(drained, [1, 2, 3, 5, 8, 9]);
    assert.deepEqual(heapq.nlargest(3, [5, 1, 8, 3, 9, 2]), [9, 8, 5]);
    assert.deepEqual(heapq.nsmallest(2, [5, 1, 8, 3, 9, 2]), [1, 2]);
});

test("#234 heapq.merge merges sorted inputs (with key/reverse)", () => {
    assert.deepEqual([...heapq.merge([1, 4, 7], [2, 5, 8], [3, 6, 9])], [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    assert.deepEqual([...heapq.merge([1, 2], [3], { reverse: false })], [1, 2, 3]);
    assert.deepEqual(
        [...heapq.merge(["bb", "cccc"], ["a", "ddd"], { key: (s) => s.length })],
        ["a", "bb", "ddd", "cccc"],
    );
});

test("#229 bisect: left/right search + insort", () => {
    const a = [1, 3, 5, 7];
    assert.equal(bisect.bisect_left(a, 4), 2);
    assert.equal(bisect.bisect_right(a, 5), 3);
    assert.equal(bisect.bisect_left(a, 5), 2);
    bisect.insort(a, 4);
    assert.deepEqual(a, [1, 3, 4, 5, 7]);
});

test("#223 copy.deepcopy isolates nested mutations; copy.copy is shallow", () => {
    const a = [[1, 2], [3, 4]];
    const b = deepcopy(a);
    b[0][0] = 99;
    assert.deepEqual(a, [[1, 2], [3, 4]]);
    assert.deepEqual(b, [[99, 2], [3, 4]]);
    const c = copyShallow([1, 2, 3]);
    assert.deepEqual(c, [1, 2, 3]);
    // shallow copy shares the inner reference
    const d = [[1], [2]];
    const e = copyShallow(d);
    e[0].push(9);
    assert.deepEqual(d[0], [1, 9]);
});

test("#223 string constants match CPython", () => {
    assert.equal(pyString.ascii_lowercase, "abcdefghijklmnopqrstuvwxyz");
    assert.equal(pyString.ascii_letters, pyString.ascii_lowercase + pyString.ascii_uppercase);
    assert.equal(pyString.digits, "0123456789");
    assert.equal(pyString.punctuation, "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~");
});

test("#223 random.Random(seed) is seedable, reproducible, in-range", () => {
    const r1 = new Random(42);
    const s1 = Array.from({ length: 20 }, () => r1.randint(1, 6));
    const r2 = new Random(42);
    const s2 = Array.from({ length: 20 }, () => r2.randint(1, 6));
    assert.deepEqual(s1, s2, "same seed → same sequence");
    assert.ok(s1.every((v) => v >= 1 && v <= 6), "randint respects bounds");
    // a different seed diverges
    const s3 = Array.from({ length: 20 }, () => new Random(7).randint(1, 100));
    assert.notDeepEqual(s1, s3);
});

test("#242 str.replace honors the count argument", () => {
    assert.equal(pyStrReplace("aaa", "a", "", 1), "aa");
    assert.equal(pyStrReplace("banana", "a", "X", 2), "bXnXna");
    assert.equal(pyStrReplace("hello", "l", "L"), "heLLo");        // no count = all
    assert.equal(pyStrReplace("aaa", "a", "b", 0), "aaa");         // count 0 = none
});

test("#219 pySetSlice: simple slice splices (resizes), extended slice element-wise", () => {
    let a = [1, 2, 3, 4, 5];
    pySetSlice(a, 1, 3, null, [20, 30, 40]); // grows
    assert.deepEqual(a, [1, 20, 30, 40, 4, 5]);
    let b = [1, 2, 3, 4, 5];
    pySetSlice(b, 1, 4, null, [9]); // shrinks
    assert.deepEqual(b, [1, 9, 5]);
    let c = [0, 1, 2, 3, 4, 5];
    pySetSlice(c, null, null, 3, [10, 20]); // c[::3] = [10,20]
    assert.deepEqual(c, [10, 1, 2, 20, 4, 5]);
    // extended-slice length mismatch throws like CPython
    assert.throws(() => pySetSlice([0, 1, 2, 3], null, null, 2, [9]), /extended slice/);
});

test("#213 str.split honors sep=/maxsplit kwargs and positional maxsplit", () => {
    assert.deepEqual(pyStrSplit("a.b.c", { sep: "." }), ["a", "b", "c"]);
    assert.deepEqual(pyStrSplit("a.b.c", ".", 1), ["a", "b.c"]);
    assert.deepEqual(pyStrSplit("x.y.z", ".", { maxsplit: 1 }), ["x", "y.z"]);
    assert.deepEqual(pyStrSplit("a b  c d", undefined, 2), ["a", "b", "c d"]);
    assert.deepEqual(pyStrSplit("  lots  of space "), ["lots", "of", "space"]);
    assert.deepEqual(pyStrSplit("", ","), [""]);
});

test("#249 shifts are arbitrary-precision, not 32-bit", () => {
    assert.equal(pyShiftLeft(1, 99), 633825300114114700748351602688n);
    assert.equal(pyShiftLeft(1, 3), 8);            // small stays a Number
    assert.equal(pyShiftRight(1024, 2), 256);
    assert.equal(pyShiftRight(-8, 1), -4);         // arithmetic (floor)
});

test("#264 cmp_to_key sorts by the comparator, not the raw value", () => {
    const cmp = (x, y) => x.length - y.length;
    const key = cmp_to_key(cmp);
    assert.deepEqual(pySorted(["bb","a","ccc"], { key }), ["a","bb","ccc"]);
    // a comparator that disagrees with natural order is honored
    const rev = cmp_to_key((x, y) => y - x);
    assert.deepEqual(pySorted([1,3,2], { key: rev }), [3,2,1]);
});

test("#247 sorted/list.sort reverse=True is stable on ties", () => {
    const data = [["a",6],["r",6],["e",9],["p",3]];
    assert.deepEqual(pySorted(data, { key: (x)=>x[1], reverse: true }),
        [["e",9],["a",6],["r",6],["p",3]]);   // ties a,r keep input order
    const xs = [["a",6],["r",6],["e",9]];
    pyListSort(xs, { key: (x)=>x[1], reverse: true });
    assert.deepEqual(xs, [["e",9],["a",6],["r",6]]);
});

test("#214 tuple/list comparison is lexicographic, not JS string coercion", () => {
    assert.equal(pyLt([-6, "s"], [-2, "x"]), true);   // '-6,s' < '-2,x' would be false
    assert.equal(pyLt([1, 2], [1, 2, 0]), true);      // prefix is smaller
    assert.equal(pyLt([2], [1, 9]), false);
    assert.equal(pyLe([1, 2], [1, 2]), true);
    assert.equal(pyGt([3, 1], [2, 9]), true);
    assert.equal(pyGe([1, 2], [1, 2]), true);
    assert.deepEqual(
        pySorted([[2, "b"], [2, "a"], [1, "z"]]),
        [[1, "z"], [2, "a"], [2, "b"]],
    );
    // tuple key with a negated first element sorts descending on it
    assert.deepEqual(
        pySorted(["ab", "a", "abc"], { key: (x) => [-x.length, x] }),
        ["abc", "ab", "a"],
    );
});

test("#258 bool subscript index coerces to int", () => {
    assert.equal(pyGetItem([10, 20, 30], true), 20);   // xs[True] == xs[1]
    assert.equal(pyGetItem([10, 20, 30], false), 10);  // xs[False] == xs[0]
    assert.equal(pyGetItem("abc", true), "b");
});

test("#241 bool is int in equality / membership / count", () => {
    assert.equal(pyEq(true, 1), true);
    assert.equal(pyEq(false, 0), true);
    assert.equal(pyEq(true, 2), false);
    assert.equal(pyEq(1n, true), true);       // bigint int vs bool
    assert.equal(pyContains([true, 2, 3], 1), true);
    assert.equal(pyContains([0, 2], false), true);
    assert.equal(pyListCount([1, 2, 3, 1], true), 2);  // True counts as 1
    // set membership fallback
    assert.equal(pyContains(new Set([1, 2, 3]), true), true);
});

test("#215 isinstance(True, int) is True (bool subclass of int)", () => {
    assert.equal(__pyIsInstance(true, "int"), true);
    assert.equal(__pyIsInstance(false, "int"), true);
    assert.equal(__pyIsInstance(true, "bool"), true);
    assert.equal(__pyIsInstance(true, "float"), false);
    assert.equal(__pyIsInstance(5, "int"), true);
    assert.equal(__pyIsInstance(2.5, "int"), false);
});

test("#82 pyInt validates strings with CPython ValueError", () => {
    assert.equal(pyInt(" 42 "), 42);
    assert.equal(pyInt("1_000"), 1000);
    assert.equal(pyInt("07"), 7);
    assert.equal(pyInt("ff", 16), 255);
    assert.throws(() => pyInt("abc"), /invalid literal for int\(\) with base 10: 'abc'/);
    assert.throws(() => pyInt(""), /invalid literal for int\(\)/);
    assert.throws(() => pyInt(NaN), /cannot convert float NaN to integer/);
    assert.throws(() => pyInt(Infinity), /cannot convert float infinity to integer/);
});

test("#206 pyBin/pyHex/pyOct match CPython incl. sign + zero", () => {
    assert.equal(pyBin(5), "0b101");
    assert.equal(pyBin(-5), "-0b101");
    assert.equal(pyBin(0), "0b0");
    assert.equal(pyHex(255), "0xff");
    assert.equal(pyHex(-42), "-0x2a");
    assert.equal(pyOct(8), "0o10");
    assert.equal(pyOct(-8), "-0o10");
    // bigint domain (ids past 2^53) round-trips exactly
    assert.equal(pyHex(0x1fffffffffffffn + 1n), "0x20000000000000");
    // non-integers are rejected like CPython
    assert.throws(() => pyHex(1.5), /can't be interpreted as an integer/);
});

test("#90 pyDivmod floor semantics + errors", () => {
    assert.deepEqual([...pyDivmod(-7, 3)], [-3, 2]);
    assert.deepEqual([...pyDivmod(7, -3)], [-3, -2]);
    assert.throws(() => pyDivmod(7, 0), /integer division or modulo by zero/);
    assert.throws(() => pyDivmod(7.5, 0), /float divmod\(\)/);
});

test("#94 pySum honors positional and keyword start", () => {
    assert.equal(pySum([1, 2, 3]), 6);
    assert.equal(pySum([1, 2, 3], 10), 16);
    assert.equal(pySum([1, 2, 3], { start: 10 }), 16);
    assert.deepEqual(pySum([[1], [2]], []), [1, 2]);
});

test("#93 set/dict operators via pyBitOr/pyBitAnd/pyBitXor/pySub", () => {
    assert.deepEqual([...pyBitOr(new Set([1, 2, 3]), new Set([3, 4]))], [1, 2, 3, 4]);
    assert.deepEqual([...pyBitAnd(new Set([1, 2, 3]), new Set([2, 3]))], [2, 3]);
    assert.deepEqual([...pySub(new Set([1, 2, 3]), new Set([2]))], [1, 3]);
    assert.deepEqual([...pyBitXor(new Set([1, 2, 3]), new Set([2, 4]))], [1, 3, 4]);
    assert.deepEqual(pyBitOr({ a: 1 }, { b: 2 }), { a: 1, b: 2 });
    assert.equal(pyBitOr(5, 3), 7);
    assert.equal(pyBitAnd(6, 3), 2);
    assert.equal(pyBitXor(6, 3), 5);
    // >32-bit ints must not truncate (JS bitwise is 32-bit).
    assert.equal(pyBitOr(2 ** 40, 1), 2 ** 40 + 1);
});

test("#95 pyRepr re-escapes control characters like CPython", () => {
    assert.equal(pyRepr("a\tb"), "'a\\tb'");
    assert.equal(pyRepr("x\ny"), "'x\\ny'");
    assert.equal(pyRepr("a\\b"), "'a\\\\b'");
    assert.equal(pyRepr("\x07"), "'\\x07'");
});

test("G runtime extras: title/split/chr/ord/min/max/fixed/delitem", () => {
    assert.equal(pyStrTitle("it's"), "It'S");
    assert.throws(() => pyStrSplit("ab", ""), VE);
    assert.equal(pyOrd(pyChr(955)), 955);
    assert.equal(pyMin(["aaa", "b"], { key: (s) => s.length }), "b");
    assert.equal(pyMax([3, 1]), 3);
    assert.equal(pyFixed(1.625, 2), "1.62");
    const xs = [1, 2, 3];
    pyDelItem(xs, -1);
    assert.deepEqual(xs, [1, 2]);
});

test("#277 tuple/bool keys canonicalize in Counter/defaultdict (extend PyDict)", async () => {
    const { Counter, defaultdict } = await import("./stdlib/collections.js");
    const { pyTuple } = await import("./operators.js");
    const t = (a, b) => pyTuple(a, b);
    const d = defaultdict(() => 0);
    d.set(t(1, 3), (d.get(t(1, 3)) || 0) + 1);
    d.set(t(1, 3), (d.get(t(1, 3)) || 0) + 1);
    assert.equal(d.get(t(1, 3)), 2);          // same tuple key matched
    assert.equal(d.has(t(1, 3)), true);
    // copy keeps the subclass (__missing__/factory) + canonical keys
    const c = d.copy();
    assert.equal(c.get(t(1, 3)), 2);
    assert.equal(typeof c.__missing__, "function");
    const cnt = new Counter();
    cnt.set(t(2, 5), 3);
    assert.equal(cnt.get(t(2, 5)), 3);
});

test("#275 sorted(dict) sorts the KEYS, not entries", async () => {
    const { pySorted } = await import("./runtime.js");
    assert.deepEqual(pySorted(new Map([[3, 1], [1, 2], [2, 9]])), [1, 2, 3]);
    // other iterables unchanged
    assert.deepEqual(pySorted([3, 1, 2]), [1, 2, 3]);
    assert.deepEqual(pySorted(new Set([5, 3, 8])), [3, 5, 8]);
    assert.deepEqual(pySorted("cba"), ["a", "b", "c"]);
});

test("#271 deque truthiness: empty deque is falsy (pyBool consults __len__)", async () => {
    const { pyBool } = await import("./types.js");
    const { deque } = await import("./stdlib/collections.js");
    const q = deque();
    assert.equal(pyBool(q), false);
    assert.equal(typeof q.__len__, "function");
    assert.equal(q.__len__(), 0);
    q.append(1);
    assert.equal(pyBool(q), true);
});


// ---- Wave-19 verification fix batch: CODE-POINT offsets for the string
// method surface (verification/PythExpandVerify.lean wave 19 predicted the
// bug class: naive UTF-16 offsets provably diverge from CPython once an
// astral char precedes the match — smFindSub_ne_js16_astral). ----
test("wave19: pyFind/pyIndex return code-point offsets on astral strings", async () => {
    const { pyFind, pyIndex } = await import("./runtime.js");
    assert.equal(pyFind("\u{1D538}x", "x"), 1);          // CPython 1, not UTF-16 2
    assert.equal(pyFind("\u{1D538}abc", "bc"), 2);
    assert.equal(pyFind("abcbc", "bc"), 1);              // first occurrence
    assert.equal(pyFind("abc", ""), 0);
    assert.equal(pyFind("abc", "zq"), -1);
    assert.equal(pyIndex("\u{1D538}abc", "bc"), 2);
    // CPython: str.index raises ValueError("substring not found") — the old
    // /is not in string/ regex never matched CPython (stale since the A1
    // full-spec rewrite of find/index landed the real message). delta4.
    assert.throws(() => pyIndex("abc", "zq"), /substring not found/);
    // start/end are code-point offsets too
    assert.equal(pyFind("\u{1D538}\u{1D538}x", "x", 1), 2);
    assert.equal(pyFind("\u{1D538}abcabc", "b", -3), 5);
    assert.equal(pyFind("hello", "l", 0, 3), 2);         // fast path unaffected
});
test("wave19: pyStrRfind/pyStrRindex return code-point offsets", async () => {
    const { pyStrRfind, pyStrRindex } = await import("./runtime.js");
    assert.equal(pyStrRfind("\u{1D538}x\u{1D538}x", "x"), 3);  // CPython 3, not 5
    assert.equal(pyStrRfind("aXbXc", "X"), 3);                 // fast path
    assert.equal(pyStrRfind("abc", "zq"), -1);
    assert.equal(pyStrRindex("\u{1D538}x\u{1D538}x", "x"), 3);
    assert.throws(() => pyStrRindex("abc", "zq"), /substring not found/);
});
test("wave19: strip family handles astral chars in the strip set", async () => {
    const { pyStrStrip, pyStrLstrip, pyStrRstrip } = await import("./runtime.js");
    assert.equal(pyStrStrip("\u{1D538}a\u{1D538}", "\u{1D538}"), "a");
    assert.equal(pyStrLstrip("\u{1D538}a\u{1D538}", "\u{1D538}"), "a\u{1D538}");
    assert.equal(pyStrRstrip("\u{1D538}a\u{1D538}", "\u{1D538}"), "\u{1D538}a");
    assert.equal(pyStrStrip("x\u{1D538}hix\u{1D538}", "x\u{1D538}"), "hi");
    assert.equal(pyStrStrip("xxhixx", "x"), "hi");       // fast path unaffected
    assert.equal(pyStrStrip("  hi  "), "hi");            // no-chars arm unaffected
});

// ---- FULL_SURFACE #2: `in` on a non-container raises TypeError ----
test("pyContains: class object raises TypeError (not attr membership)", () => {
    class T { }
    T.__name__ = "T";
    T.a = 3; // static attr — Transcrypt would report 'a' in T as True
    assert.throws(() => pyContains(T, "a"), (e) =>
        e.name === "TypeError" && e.message === "argument of type 'type' is not iterable");
});

test("pyContains: instance without __contains__/__iter__ raises TypeError", () => {
    class Foo { }
    Foo.__name__ = "Foo";
    assert.throws(() => pyContains(new Foo(), "x"), (e) =>
        e.name === "TypeError" && e.message === "argument of type 'Foo' is not iterable");
});

test("pyContains: instance protocols still honored", () => {
    class C { __contains__(x) { return x === 42; } }
    assert.equal(pyContains(new C(), 42), true);
    assert.equal(pyContains(new C(), 1), false);
    // legacy sequence protocol: __getitem__(0..) until IndexError
    class Seq {
        __getitem__(i) {
            if (i > 2) { const e = new Error("x"); e.name = "IndexError"; throw e; }
            return i * 10;
        }
    }
    assert.equal(pyContains(new Seq(), 20), true);
    assert.equal(pyContains(new Seq(), 99), false);
});

test("pyContains: containers unaffected by the non-container guard", () => {
    assert.equal(pyContains([1, 2, 3], 2), true);
    assert.equal(pyContains("hello", "ell"), true);
    assert.equal(pyContains({ a: 1 }, "a"), true);       // plain-object dict
    assert.equal(pyContains(new Set([1]), 1), true);
    assert.equal(pyContains(new Map([["k", 1]]), "k"), true);
});

// ---- FULL_SURFACE #3: default object repr/str ----
test("pyRepr/pyStr: default object repr is <module.Class object at 0x…>", () => {
    class Foo { }
    Foo.__name__ = "Foo";
    const f = new Foo();
    assert.match(pyRepr(f), /^<__main__\.Foo object at 0x[0-9a-f]+>$/);
    assert.equal(pyStr(f), pyRepr(f));                  // CPython: str falls back to repr
    assert.equal(pyRepr(f), pyRepr(f));                 // stable per object
    assert.notEqual(pyRepr(new Foo()), pyRepr(f));      // distinct objects differ
});

test("pyRepr: user __repr__ and container reprs unchanged", () => {
    class Bar { __repr__() { return "Bar()"; } }
    assert.equal(pyRepr(new Bar()), "Bar()");
    assert.equal(pyRepr({ a: 1 }), "{'a': 1}");
    assert.equal(pyRepr([1, "x"]), "[1, 'x']");
    assert.equal(pyRepr(3), "3");
});
