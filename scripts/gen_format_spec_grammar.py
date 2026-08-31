#!/usr/bin/env python3
"""E3: generate tests/fixtures/format_spec_grammar.json — THE format-spec
mini-language matrix that pins BOTH parsers (Rust
crates/pyths_parser/src/format_spec.rs::parse and the runtime's
parseFormatSpec) and the render engine (pyFormatSpec) to the CPython oracle.

Each row: {"spec", "parse": opts|null, "cases": {label: {"ok": str}|{"err": str}}}
  - `parse` is the canonical opts object per the grammar
    [[fill]align][sign]["z"]["#"]["0"][width][grouping]["." prec][type]
    (null = grammatically invalid). Both shipped parsers must agree with it.
  - `cases` come from LIVE CPython `format(value, spec)` per value label —
    the render differential (crates/pyths_runtime/js/format_diff_test.mjs
    replays them; the Rust side checks `parse`).

Regenerate with the pinned oracle:  py -3.14 scripts/gen_format_spec_grammar.py
"""
import json
import sys
from pathlib import Path

VALUES = {
    "int": 4660,
    "negint": -42,
    "bigint": 18446744073709551617,
    "float": 1234.5678,
    "negfloat": -0.0625,
    "wholefloat": 8.0,
    "negzero": -0.0,
    "inf": float("inf"),
    "nan": float("nan"),
    "str": "héllo",
    "bool": True,
    "none": None,
    "list": [1],
}

SPECS = [
    # bare types
    "", "s", "d", "f", "F", "e", "E", "g", "G", "%", "b", "o", "x", "X", "c", "n",
    # width / align / fill
    "10", "<10", ">10", "^10", "=10", "*<10", "*>10", "*^10", "0=10",
    " ^7", "x<5", "0>5", "05", "010", "0", "1", "3d", "20f",
    "<<5", ">>5", "^^5", "=<5", " <5", "0^6", "z>5", "{<6", "}>6",
    # sign
    "+", "-", " ", "+d", "-d", " d", "+.2f", " .0f", "+010d", " 08.2f",
    "+x", " b", "+s",
    # z (PEP 682)
    "z", "z.1f", "z.2e", "zd", "z10.3f", "+z.1f", "z.0%", "zs",
    # alternate form
    "#", "#b", "#o", "#x", "#X", "#c", "#d", "#.0f", "#.0e", "#.3g", "#g",
    "#010b", "#010x", "#s",
    # zero-pad + grouping interactions
    "07.2f", "08d", "011,", "013_.2f", "0,", "08,d", "015,.2f", "#014_x",
    # grouping
    ",", "_", ",d", "_d", ",x", "_x", ",b", "_b", ",o", "_o", ",c", ",s",
    ",e", ",f", ",g", ",%", "_%", ",.2f", "10,", ",n", "_n", ",_", "_,",
    # precision
    ".0f", ".2f", ".7f", ".0e", ".3e", ".0g", ".3g", ".10g", ".2", ".5",
    ".0", ".2s", ".2d", ".2x", ".2c", ".2%", ".", ".f", "8.3", "10.2s",
    # '=' alignment
    "=d", "=8.2f", "=s", "0=8d",
    # c-type specials
    "5c", "<5c", "+c", ",.0c",
    # invalid / garbage
    "q", "ff", "dd", "2f2", "d10", "+-", "++", "--5d", "#z5d", "z#5d",
    # 3.14 fractional grouping
    ",.9_f", ".6,f", ".3_%", ".7,e", ".8,g", ".3,s", ".10,", "z.4,f",
    "05.6,f", ".3,d", ",._3f", ".,3f",
    ".,", "._f", ".,d", ".,s", ".3,_f", ".3,q", ".3,,f",
    # kitchen-sink combos
    "+#012_.3e", "*^+#020,.3f", "=^10.4f", "==10", "0=+10,.1f", " >#16_X",
]


def mini_parse(s):
    """The canonical grammar — the third, generator-side statement of it.
    Returns the opts dict or None."""
    chars = list(s)
    opts = {}
    i = 0
    if len(chars) >= 2 and chars[1] in "<>=^":
        opts["fill"] = chars[0]
        opts["align"] = chars[1]
        i = 2
    elif len(chars) >= 1 and chars[0] in "<>=^":
        opts["align"] = chars[0]
        i = 1
    if i < len(chars) and chars[i] in "+- ":
        opts["sign"] = chars[i]
        i += 1
    if i < len(chars) and chars[i] == "z":
        opts["z"] = True
        i += 1
    if i < len(chars) and chars[i] == "#":
        opts["alt"] = True
        i += 1
    if i < len(chars) and chars[i] == "0":
        opts["zero"] = True
        i += 1
    w = ""
    while i < len(chars) and chars[i].isascii() and chars[i].isdigit():
        w += chars[i]
        i += 1
    if w:
        opts["width"] = int(w)
    if i < len(chars) and chars[i] in ",_":
        opts["grouping"] = chars[i]
        i += 1
    if i < len(chars) and chars[i] == ".":
        i += 1
        p = ""
        while i < len(chars) and chars[i].isascii() and chars[i].isdigit():
            p += chars[i]
            i += 1
        if not p and not (i < len(chars) and chars[i] in ",_"):
            return None  # missing precision (no digits, no frac grouping)
        if p:
            opts["precision"] = int(p)
        # 3.14: optional FRACTIONAL grouping char after the precision.
        if i < len(chars) and chars[i] in ",_":
            opts["fracGrouping"] = chars[i]
            i += 1
    if i < len(chars):
        if chars[i] not in "bcdeEfFgGnosxX%":
            return None
        opts["type"] = chars[i]
        i += 1
    if i != len(chars):
        return None
    return opts


def main():
    assert sys.version_info[:2] >= (3, 12), "run with the pinned oracle"
    rows = []
    for spec in SPECS:
        cases = {}
        for label, v in VALUES.items():
            try:
                cases[label] = {"ok": format(v, spec)}
            except BaseException as e:  # noqa: BLE001 — capture kind+message
                cases[label] = {"err": type(e).__name__ + ": " + str(e)}
        rows.append({"spec": spec, "parse": mini_parse(spec), "cases": cases})
    out = {
        "_comment": [
            "GENERATED by scripts/gen_format_spec_grammar.py against the",
            "pinned CPython oracle (%s). Consumed by" % sys.version.split()[0],
            "crates/pyths_parser/tests/format_spec_grammar.rs (parser parity)",
            "and crates/pyths_runtime/js/format_diff_test.mjs (render",
            "differential). Regenerate, never hand-edit.",
        ],
        "oracle": sys.version.split()[0],
        "rows": rows,
    }
    dest = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "format_spec_grammar.json"
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(json.dumps(out, ensure_ascii=True, indent=1) + "\n", encoding="utf-8")
    print("wrote", dest, len(rows), "rows x", len(VALUES), "values")


if __name__ == "__main__":
    main()
