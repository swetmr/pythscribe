// Unit tests for the `re` stdlib module — Python replacement-template
// semantics in re.sub / re.subn (PBT Lane A, MBPP+ tasks 427/748).
// Behavior parity against real CPython is additionally covered by the
// differential corpus (tests/differential/cpython_corpus.json, re_*
// entries). Every expected value here was produced by CPython 3.11.
//
// Run with: node --test runtime/src/stdlib/re.test.mjs

import { test } from "node:test";
import assert from "node:assert/strict";

import * as re from "./re.js";

test("sub: numbered backreferences \\1 \\2 \\3", () => {
    assert.equal(
        re.sub(String.raw`(\d{4})-(\d{2})-(\d{2})`, String.raw`\3-\2-\1`, "2026-01-02"),
        "02-01-2026",
    );
    assert.equal(
        re.sub(String.raw`(\w)([A-Z])`, String.raw`\1 \2`, "PythonProgrammingExamples"),
        "Python Programming Examples",
    );
});

test("sub: \\g<number> form", () => {
    assert.equal(
        re.sub(String.raw`(\w+) (\w+)`, String.raw`\g<2> \g<1>`, "hello world"),
        "world hello",
    );
    assert.equal(re.sub("a", String.raw`[\g<0>]`, "banana"), "b[a]n[a]n[a]");
});

test("sub: literal $ is not JS-special", () => {
    assert.equal(re.sub("n", "$&", "banana"), "ba$&a$&a");
    assert.equal(re.sub("n", "$$", "banana"), "ba$$a$$a");
});

test("sub: escapes in template (\\n, \\t, \\\\, octal)", () => {
    assert.equal(re.sub("x", String.raw`\n`, "axb"), "a\nb");
    assert.equal(re.sub("x", String.raw`\t`, "axb"), "a\tb");
    assert.equal(re.sub("x", "\\\\", "axb"), "a\\b");
    assert.equal(re.sub("x", String.raw`\101`, "axb"), "aAb"); // octal 101 = 'A'
    assert.equal(re.sub("x", String.raw`\0`, "axb"), "a\x00b");
});

test("sub: unknown ASCII-letter escape raises like CPython", () => {
    assert.throws(() => re.sub("x", String.raw`\q`, "axb"), /bad escape/);
});

test("sub: non-letter unknown escape kept literally", () => {
    assert.equal(re.sub("x", String.raw`\-`, "axb"), "a\\-b");
});

test("sub: unmatched (but valid) group substitutes empty string", () => {
    // CPython >= 3.5: re.sub('(a)|(b)', r'X\2Y', 'a') -> 'XY'
    assert.equal(re.sub("(a)|(b)", String.raw`X\2Y`, "a"), "XY");
});

test("sub: invalid group reference raises", () => {
    assert.throws(() => re.sub("(a)", String.raw`\2`, "a"), /invalid group reference/);
});

test("sub: count semantics", () => {
    assert.equal(re.sub("a", "b", "banana", 2), "bbnbna");
    assert.equal(re.sub("a", "b", "banana", 0), "bbnbnb"); // 0 = all
    assert.equal(re.sub("a", "b", "banana", 100), "bbnbnb");
});

test("sub: zero-width matches advance like CPython 3.7+", () => {
    assert.equal(re.sub("x*", "-", "axbc"), "-a--b-c-");
    assert.equal(re.sub("", "-", "abc"), "-a-b-c-");
});

test("sub: function replacement receives a Match", () => {
    assert.equal(
        re.sub(String.raw`\d+`, (m) => String(Number(m.group(0)) * 2), "a3b10"),
        "a6b20",
    );
    // Backslashes in a function's return value are NOT template-processed.
    assert.equal(re.sub("x", () => String.raw`\1`, "axb"), "a\\1b");
});

test("subn returns (result, n) as a tuple-marked pair", () => {
    const r = re.subn("a", "b", "banana");
    assert.equal(r[0], "bbnbnb");
    assert.equal(r[1], 3);
    assert.equal(r.__pytuple__, true);
    const r2 = re.subn("a", "b", "banana", 2);
    assert.equal(r2[0], "bbnbna");
    assert.equal(r2[1], 2);
    const r3 = re.subn("z", "b", "banana");
    assert.equal(r3[0], "banana");
    assert.equal(r3[1], 0);
});

test("subn: backreferences work through the shared engine", () => {
    const r = re.subn(String.raw`(\w)([A-Z])`, String.raw`\1 \2`, "aBcD");
    assert.equal(r[0], "a Bc D");
    assert.equal(r[1], 2);
});

// ---- Match introspection & repr (PBT Lane A, MBPP+ 737/787/794/607) ----

test("Match repr matches CPython shape", () => {
    const m = re.search("n+", "banana");
    assert.equal(m.__repr__(), "<re.Match object; span=(2, 3), match='n'>");
    assert.equal(String(m), m.__repr__());
});

test("Match repr uses Python string repr for the matched text", () => {
    const m = re.search("'.*'", "say 'hi' now");
    assert.equal(m.__repr__(), "<re.Match object; span=(4, 8), match=\"'hi'\">");
});

test("Match group(a, b) returns tuple; groups() tuple-marked", () => {
    const m = re.match(String.raw`(\w+) (\w+)`, "hello world extra");
    const g = m.group(1, 2);
    assert.deepEqual([...g], ["hello", "world"]);
    assert.equal(g.__pytuple__, true);
    const gs = m.groups();
    assert.deepEqual([...gs], ["hello", "world"]);
    assert.equal(gs.__pytuple__, true);
});

test("Match span/start/end work for subgroups via 'd' flag", () => {
    const m = re.match(String.raw`(\w+) (\w+)`, "hello world extra");
    assert.deepEqual([...m.span(1)], [0, 5]);
    assert.deepEqual([...m.span(2)], [6, 11]);
    assert.equal(m.span(1).__pytuple__, true);
    assert.equal(m.start(2), 6);
    assert.equal(m.end(1), 5);
});

test("Match unmatched group: span (-1,-1), group None, groups partial", () => {
    const m = re.search("(a)|(b)", "a");
    assert.deepEqual([...m.span(2)], [-1, -1]);
    assert.equal(m.group(2), null);
    assert.deepEqual([...m.groups()], ["a", null]);
});

test("Match nonexistent group raises IndexError-like", () => {
    const m = re.search("(a)|(b)", "a");
    assert.throws(() => m.group(3), /no such group/);
    assert.throws(() => m.span(3), /no such group/);
});

test("Match.re.pattern exposes the original pattern (Mbpp/607 shape)", () => {
    const m = re.search("fox", "The quick brown fox");
    assert.equal(m.re.pattern, "fox");
    assert.equal(m.start(), 16);
    assert.equal(m.end(), 19);
});

test("findall: multi-group tuples, unmatched group ''", () => {
    const r = re.findall("(a)|(b)", "ab");
    assert.deepEqual(r.map(t => [...t]), [["a", ""], ["", "b"]]);
    assert.equal(r[0].__pytuple__, true);
});

test("findall/finditer terminate on zero-width matches", () => {
    assert.deepEqual(re.findall("x*", "axb"), ["", "x", "", ""]);
    const spans = [...re.finditer("x*", "axb")].map(m => [...m.span()]);
    assert.deepEqual(spans, [[0, 0], [1, 2], [2, 2], [3, 3]]);
});

// ---- Compiled Pattern interop + named groups (PBT Lane A, BigCodeBench/1108) ----

test("module functions accept a compiled Pattern (BCB/1108 shape)", () => {
    const regex = re.compile(String.raw`^(?:http|ftp)s?://\S+$`, re.IGNORECASE);
    assert.notEqual(re.match(regex, "HTTP://google.com"), null);
    assert.equal(re.match(regex, "hi"), null);
    assert.notEqual(re.search(regex, "https://x.y"), null);
    assert.equal(re.sub(re.compile("a"), "b", "banana"), "bbnbnb");
    assert.deepEqual(re.findall(re.compile("[ab]"), "abc"), ["a", "b"]);
    assert.deepEqual(re.split(re.compile("-"), "a-b-c"), ["a", "b", "c"]);
});

test("Pattern object surface", () => {
    const p = re.compile("fox");
    assert.equal(p.pattern, "fox");
    assert.equal(p.__repr__(), "re.compile('fox')");
    const m = p.search("The quick brown fox");
    assert.equal(m.re.pattern, "fox");
    assert.equal(m.start(), 16);
    assert.equal(m.end(), 19);
    assert.equal(p.match("foxy").group(0), "fox");
    assert.equal(p.fullmatch("fox").group(0), "fox");
    assert.equal(p.sub("X", "fox fox"), "X X");
    const sn = p.subn("X", "fox fox");
    assert.deepEqual([...sn], ["X X", 2]);
    assert.deepEqual(p.findall("fox fox"), ["fox", "fox"]);
});

test("compile() is idempotent and preserves original pattern text", () => {
    const p = re.compile("(?P<x>a)");
    assert.equal(p.pattern, "(?P<x>a)");
    assert.equal(re.compile(p), p);
    assert.equal(p.search("a").re.pattern, "(?P<x>a)");
});

test("m.re is a Pattern for module-level string-pattern calls too", () => {
    const m = re.search("n+", "banana");
    assert.equal(m.re.pattern, "n+");
    assert.equal(m.re instanceof re.Pattern, true);
});

test("(?P<name>...) named groups work end-to-end", () => {
    const m = re.search(String.raw`(?P<first>\w+) (?P<second>\w+)`, "hello world");
    assert.equal(m.group("first"), "hello");
    assert.equal(m.group("second"), "world");
    assert.deepEqual(m.groupdict(), { first: "hello", second: "world" });
    assert.deepEqual([...m.span("second")], [6, 11]);
});

test("\g<name> template references named groups", () => {
    assert.equal(
        re.sub(String.raw`(?P<a>\w+) (?P<b>\w+)`, String.raw`\g<b> \g<a>`, "hello world"),
        "world hello",
    );
});

test("(?P=name) named backreference in pattern", () => {
    assert.equal(re.search("(?P<ch>.)(?P=ch)", "abbc").group(0), "bb");
});
