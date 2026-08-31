#!/usr/bin/env python3
"""E3: generate the Unicode tables embedded in runtime/src/runtime.js.

The CPython interpreter running this script IS the authority (run it with
the pinned oracle — docs/python-oracle-policy.md): every table is computed
by asking CPython itself, so the emitted JS reproduces the oracle's
str.casefold / str.capitalize / str.title / str.isdecimal / str.isdigit /
str.isnumeric behavior exactly (same Unicode version as the oracle).

Usage:  py -3.14 scripts/gen_unicode_tables.py
Prints the generated JS block to stdout; paste it between the
`// === BEGIN GENERATED UNICODE TABLES` / `// === END GENERATED` markers
in runtime/src/runtime.js (or redirect and splice).
"""
import sys

MAX_CP = 0x110000


def esc(cp: int) -> str:
    if cp <= 0xFFFF:
        return "\\u%04x" % cp
    return "\\u{%x}" % cp


def ranges(cps):
    cps = sorted(cps)
    out = []
    i = 0
    while i < len(cps):
        j = i
        while j + 1 < len(cps) and cps[j + 1] == cps[j] + 1:
            j += 1
        out.append((cps[i], cps[j]))
        i = j + 1
    return out


def char_class(cps) -> str:
    parts = []
    for a, b in ranges(cps):
        if a == b:
            parts.append(esc(a))
        elif b == a + 1:
            parts.append(esc(a) + esc(b))
        else:
            parts.append(esc(a) + "-" + esc(b))
    return "".join(parts)


def js_str(s: str) -> str:
    body = s.replace("\\", "\\\\").replace('"', '\\"')
    return '"' + "".join(c if 0x20 <= ord(c) < 0x7F else "".join("\\u%04x" % u for u in _units(c)) for c in body) + '"'


def _units(c):
    n = ord(c)
    if n <= 0xFFFF:
        return [n]
    n -= 0x10000
    return [0xD800 + (n >> 10), 0xDC00 + (n & 0x3FF)]


def main():
    decimal, digit, numeric = [], [], []
    fold_map = {}   # cp -> casefold string, where casefold != lower (JS toLowerCase ~ lower)
    title_map = {}  # cp -> titlecase-of-char, where it differs from upper(ch)
    for cp in range(MAX_CP):
        if 0xD800 <= cp <= 0xDFFF:
            continue
        ch = chr(cp)
        if ch.isdecimal():
            decimal.append(cp)
        if ch.isdigit():
            digit.append(cp)
        if ch.isnumeric():
            numeric.append(cp)
        f = ch.casefold()
        if f != ch.lower():
            fold_map[cp] = f
        # per-char titlecase = str.title() on the single char when it is cased;
        # capitalize()'s first-char mapping is title-case too.
        t = ch.title()
        # only meaningful for cased letters; title() of an uncased char is itself
        if t != ch.upper():
            title_map[cp] = t

    w = sys.stdout.write
    w("// === BEGIN GENERATED UNICODE TABLES (scripts/gen_unicode_tables.py; oracle %s) ===\n"
      % (".".join(map(str, sys.version_info[:3]))))
    w("// str.isdecimal(): all Nd per the oracle's Unicode tables.\n")
    w("const __RE_DECIMAL = /^[%s]+$/u;\n" % char_class(decimal))
    w("// str.isdigit(): decimal + Numeric_Type=Digit.\n")
    w("const __RE_DIGIT = /^[%s]+$/u;\n" % char_class(digit))
    w("// str.isnumeric(): digit + Numeric_Type=Numeric (incl. CJK numerals, fractions).\n")
    w("const __RE_NUMERIC = /^[%s]+$/u;\n" % char_class(numeric))
    w("// str.casefold(): code points whose full case fold differs from lower() —\n")
    w("// the residual applied on top of toLowerCase (~ lower()).\n")
    w("const __CASEFOLD_MAP = new Map([\n")
    for cp, f in sorted(fold_map.items()):
        w("    [0x%x, %s],\n" % (cp, js_str(f)))
    w("]);\n")
    w("// Titlecase-first mapping (capitalize()/title() word starts) where it\n")
    w("// differs from upper() — digraphs and ligature expansions.\n")
    w("const __TITLE_MAP = new Map([\n")
    for cp, t in sorted(title_map.items()):
        w("    [0x%x, %s],\n" % (cp, js_str(t)))
    w("]);\n")
    w("// === END GENERATED UNICODE TABLES ===\n")


if __name__ == "__main__":
    main()
