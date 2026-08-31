// E3 r3 — string-method fidelity witnesses for the codex r2 blocker batch.
// Every expectation below is a VERBATIM transcript of the pinned CPython
// 3.14.7 oracle (`py -3.14`) on the same expression — recorded in the r3
// fix session and re-checkable with the one-liners in each section header.
// Classes exercised:
//   * __index__-protocol authority (slice bounds, maxsplit/count, %x/%d/%c)
//   * lone-surrogate / code-point handling (count, encode)
//   * argument-type + error-taxonomy (count, startswith/endswith, translate,
//     encode, printf ints)
//   * width/precision digit-run bounds (format-spec + printf parsers)
import { test } from "node:test";
import assert from "node:assert/strict";
import {
    pyCount, pyFind, pyIndex, pyStrRindex, pyStrStartswith, pyStrEndswith,
    pyStrSplit, pyStrRsplit, pyStrReplace, pyStrTranslate, pyStrEncode,
    pyStrMod, pyFormatDynamic, LookupError,
} from "./runtime.js";

const raises = (fn, errName, msg) => {
    let threw = null;
    try { fn(); } catch (e) { threw = e; }
    assert.ok(threw !== null, "expected a raise, got a value");
    const got = threw.constructor.__name__ || threw.constructor.name;
    assert.equal(got, errName, `error class: got ${got} (${threw.message})`);
    assert.equal(threw.message, msg);
};

const BadI = () => ({ __index__: () => "1" });
const I = (v) => ({ __index__: () => v });
const tup = (...xs) => {
    Object.defineProperty(xs, "__pytuple__", { value: true, enumerable: false });
    return xs;
};

// ---- blocker 2: slice-bound __index__ RESULT validated, not coerced ------
// py -3.14: "abc".find("a", BadI()) → TypeError: __index__ returned non-int (type str)
test("slice bounds validate the __index__ result (find/startswith/count)", () => {
    raises(() => pyFind("abc", "a", BadI()), "TypeError",
        "__index__ returned non-int (type str)");
    raises(() => pyStrStartswith("abc", "a", BadI()), "TypeError",
        "__index__ returned non-int (type str)");
    raises(() => pyCount("abc", "a", BadI()), "TypeError",
        "__index__ returned non-int (type str)");
    assert.equal(pyFind("abc", "a", I(1)), -1); // a valid __index__ is accepted
    raises(() => pyFind("abc", "a", 2.5), "TypeError",
        "slice indices must be integers or None or have an __index__ method");
});

// ---- blocker 3: count() arg typing + lone-surrogate code-point scan ------
// py -3.14: "abc".count(1) → TypeError: count() argument 1 must be str, not int
//           "a😀b".count(chr(0xD83D)) → 0 ; "a\ud83db".count(chr(0xD83D)) → 1
test("count(): CPython arg taxonomy and code-point matching", () => {
    raises(() => pyCount("abc", 1), "TypeError",
        "count() argument 1 must be str, not int");
    raises(() => pyCount("abc", null), "TypeError",
        "count() argument 1 must be str, not None");
    assert.equal(pyCount("a\u{1F600}b", "\uD83D"), 0); // half-pair never matches
    assert.equal(pyCount("a\uD83Db", "\uD83D"), 1);    // real lone surrogate does
    assert.equal(pyCount("a\u{1F600}b\u{1F600}", "\u{1F600}"), 2);
});

// py -3.14 (3.14 wording): "abc".find(1) → find() argument 1 must be str, not int
test("find/index/rindex carry the 3.14 method-name message", () => {
    raises(() => pyFind("abc", 1), "TypeError", "find() argument 1 must be str, not int");
    raises(() => pyIndex("abc", 1), "TypeError", "index() argument 1 must be str, not int");
    raises(() => pyStrRindex("abc", 1), "TypeError", "rindex() argument 1 must be str, not int");
});

// ---- blocker 4: startswith/endswith affix taxonomy -----------------------
// py -3.14: 'abc'.startswith(['a']) → startswith first arg must be str or a
// tuple of str, not list ; ('a', 1) short-circuits True ; ('x', 1) raises.
test("startswith/endswith: str-or-tuple-of-str, short-circuit preserved", () => {
    raises(() => pyStrStartswith("abc", 1), "TypeError",
        "startswith first arg must be str or a tuple of str, not int");
    raises(() => pyStrStartswith("abc", ["a"]), "TypeError",
        "startswith first arg must be str or a tuple of str, not list");
    raises(() => pyStrStartswith("abc", tup("x", 1)), "TypeError",
        "tuple for startswith must only contain str, not int");
    assert.equal(pyStrStartswith("abc", tup("a", 1)), true); // valid short-circuit
    assert.equal(pyStrStartswith("abc", tup()), false);
    raises(() => pyStrEndswith("abc", ["c"]), "TypeError",
        "endswith first arg must be str or a tuple of str, not list");
    raises(() => pyStrEndswith("abc", tup("x", 1)), "TypeError",
        "tuple for endswith must only contain str, not int");
    // CPython converts start/end BEFORE typing the affix.
    raises(() => pyStrStartswith("abc", 1, 2.5), "TypeError",
        "slice indices must be integers or None or have an __index__ method");
});

// ---- blocker 5: whitespace-split maxsplit remainder + __index__ ----------
// py -3.14: '  a b  '.split(None, 0) → ['a b  '] ; rsplit → ['  a b']
test("split/rsplit whitespace remainder keeps its outer whitespace", () => {
    assert.deepEqual(pyStrSplit("  a b  ", null, 0), ["a b  "]);
    assert.deepEqual(pyStrRsplit("  a b  ", null, 0), ["  a b"]);
    assert.deepEqual(pyStrSplit("  a b  c  ", null, 1), ["a", "b  c  "]);
    assert.deepEqual(pyStrRsplit("  a b  c  ", null, 1), ["  a b", "c"]);
    assert.deepEqual(pyStrSplit("  a b  ", null, false), ["a b  "]); // bool ⊂ int
});
test("maxsplit/count go through the __index__ authority", () => {
    assert.deepEqual(pyStrSplit("a b c", null, I(1)), ["a", "b c"]);
    assert.deepEqual(pyStrRsplit("a b c", null, I(1)), ["a b", "c"]);
    assert.deepEqual(pyStrSplit("a-b-c", "-", I(1)), ["a", "b-c"]);
    assert.equal(pyStrReplace("aaa", "a", "b", I(1)), "baa");
    raises(() => pyStrSplit("a b", null, BadI()), "TypeError",
        "__index__ returned non-int (type str)");
    raises(() => pyStrSplit("a b", null, 1.5), "TypeError",
        "'float' object cannot be interpreted as an integer");
});

// ---- blocker 6: translate takes ANY subscriptable, CPython taxonomy ------
// py -3.14: "a".translate(None) → TypeError: 'NoneType' object is not
// subscriptable ; "".translate(None) → '' ; {97: True} → '\x01'.
test("translate: subscriptable protocol + lazy non-subscriptable error", () => {
    raises(() => pyStrTranslate("a", null), "TypeError",
        "'NoneType' object is not subscriptable");
    assert.equal(pyStrTranslate("", null), ""); // no lookup → no raise
    raises(() => pyStrTranslate("a", 0), "TypeError",
        "'int' object is not subscriptable");
    raises(() => pyStrTranslate("a", true), "TypeError",
        "'bool' object is not subscriptable");
    assert.equal(pyStrTranslate("a", { 97: true }), "\x01"); // bool ⊂ int
    const getitem = {
        __getitem__: (k) => {
            if (k === 97) return "Z";
            throw new LookupError(String(k));
        },
    };
    assert.equal(pyStrTranslate("ab", getitem), "Zb"); // LookupError keeps char
    assert.equal(pyStrTranslate("abc", "XYZ"), "abc"); // str table: IndexError keeps
    assert.equal(pyStrTranslate("\x01", ["X", "q"]), "q"); // list table positional
    assert.equal(pyStrTranslate("ab", new Map()), "ab");
    raises(() => pyStrTranslate("a", { 97: 0x110000 }), "ValueError",
        "character mapping must be in range(0x110000)");
});

// ---- blocker 7: ASCII/latin-1 encode reason for lone surrogates ----------
// py -3.14: chr(0xD800).encode('ascii') → ... ordinal not in range(128)
test("encode: range codecs report the RANGE for lone surrogates", () => {
    raises(() => pyStrEncode("\uD800", "ascii"), "UnicodeEncodeError",
        "'ascii' codec can't encode character '\\ud800' in position 0: ordinal not in range(128)");
    raises(() => pyStrEncode("\uD800", "latin-1"), "UnicodeEncodeError",
        "'latin-1' codec can't encode character '\\ud800' in position 0: ordinal not in range(256)");
    raises(() => pyStrEncode("\uD800", "utf-8"), "UnicodeEncodeError",
        "'utf-8' codec can't encode character '\\ud800' in position 0: surrogates not allowed");
});

// ---- blocker 8: printf integer conversions -------------------------------
// py -3.14: '%08.3d' % 7 → '00000007' ; '%#08.3x' % 255 → '0x0000ff' ;
// '%x' % I(15) → 'f' ; '%d' % 1e100 → the exact 101-digit integer ;
// '%c' % 2**100 → OverflowError: %c arg not in range(0x110000).
test("printf: 0-flag with precision zero-fills to width", () => {
    assert.equal(pyStrMod("%08.3d", 7), "00000007");
    assert.equal(pyStrMod("%-8.3d", 7), "007     ");
    assert.equal(pyStrMod("%08.3d", -7), "-0000007");
    assert.equal(pyStrMod("%#08.3x", 255), "0x0000ff");
    assert.equal(pyStrMod("%#8.3x", 255), "   0x0ff");
    assert.equal(pyStrMod("%+08.3d", 7), "+0000007");
    assert.equal(pyStrMod("%#08.3o", 7), "0o000007");
    assert.equal(pyStrMod("%.5d", -7), "-00007");
});
test("printf: %x/%d integer protocols and messages", () => {
    assert.equal(pyStrMod("%x", I(15)), "f");
    raises(() => pyStrMod("%x", 1.5), "TypeError",
        "%x format: an integer is required, not float");
    raises(() => pyStrMod("%x", BadI()), "TypeError",
        "%x format: an integer is required, not dict");
    assert.equal(pyStrMod("%d", { __int__: () => 42 }), "42");
    assert.equal(pyStrMod("%d", I(15)), "15");
    raises(() => pyStrMod("%d", "s"), "TypeError",
        "%d format: a real number is required, not str");
    assert.equal(pyStrMod("%d", -2.5), "-2");
    assert.equal(
        pyStrMod("%d", 1e100),
        "1" + "0000000000000000159028911097599180468360808563945281389781327557747838772170381060813469985856815104",
    );
});
test("printf: %c range check matches CPython for any out-of-range int", () => {
    raises(() => pyStrMod("%c", 2n ** 100n), "OverflowError", "%c arg not in range(0x110000)");
    raises(() => pyStrMod("%c", -1), "OverflowError", "%c arg not in range(0x110000)");
    raises(() => pyStrMod("%c", 0x110000), "OverflowError", "%c arg not in range(0x110000)");
    assert.equal(pyStrMod("%c", I(65)), "A");
    assert.equal(pyStrMod("%c", true), "\x01");
});

// ---- blocker 9: width/precision digit-run bounds -------------------------
// py -3.14: '%999999999999999999999d' % 1 → ValueError: width too big ;
// format(1, '999999999999999999999d') → Too many decimal digits in format string
test("width/precision digit runs past PY_SSIZE_T_MAX raise, never truncate", () => {
    raises(() => pyStrMod("%999999999999999999999d", 1), "ValueError", "width too big");
    raises(() => pyStrMod("%.999999999999999999999d", 1), "ValueError", "precision too big");
    raises(() => pyFormatDynamic(1, "999999999999999999999d"), "ValueError",
        "Too many decimal digits in format string");
    raises(() => pyFormatDynamic(1.5, ".999999999999999999999f"), "ValueError",
        "Too many decimal digits in format string");
    raises(() => pyFormatDynamic(1, "9223372036854775808d"), "ValueError",
        "Too many decimal digits in format string");
});
