#!/usr/bin/env python3
"""Audit each dictionary entry's actual cl100k token saving.

For each (alias, canonical), measures the cost of the alias used in
realistic context vs the canonical it expands to. Reports which
entries save tokens, which are neutral, and which regress.
"""
from __future__ import annotations

import sys
import tiktoken

CL = tiktoken.get_encoding("cl100k_base")
O200 = tiktoken.get_encoding("o200k_base")

# (alias_form_in_psc, canonical_form_after_expand, context_template, label)
# Context template uses {x} for substitution.
ENTRIES = [
    # CSS property KEYS — appear in `dict_key: dict_value` position
    ("$pad",  "\"padding\"",          "{{{x}: $p4}}", "pad"),
    ("$fs",   "\"font_size\"",        "{{{x}: $p1}}", "fs"),
    ("$col",  "\"color\"",            "{{{x}: $c1}}", "col"),
    ("$dsp",  "\"display\"",          "{{{x}: $flex}}", "dsp"),
    ("$brr",  "\"border_radius\"",    "{{{x}: $p5}}", "brr"),
    ("$bd",   "\"border\"",           "{{{x}: $br1}}", "bd"),
    ("$bg",   "\"background\"",       "{{{x}: $c2}}", "bg"),
    ("$mb",   "\"margin_bottom\"",    "{{{x}: $p4}}", "mb"),
    ("$mar",  "\"margin\"",           "{{{x}: 0}}", "mar"),
    ("$ta",   "\"text_align\"",       "{{{x}: center}}", "ta"),
    ("$wd",   "\"width\"",            "{{{x}: $pct}}", "wd"),
    ("$gtc",  "\"grid_template_columns\"", "{{{x}: \"...\"}}", "gtc"),
    ("$bb",   "\"border_bottom\"",    "{{{x}: $br1}}", "bb"),
    ("$mh",   "\"min_height\"",       "{{{x}: $vh}}", "mh"),
    ("$fw",   "\"font_weight\"",      "{{{x}: $w7}}", "fw"),
    ("$cur",  "\"cursor\"",           "{{{x}: pointer}}", "cur"),
    ("$ai",   "\"align_items\"",      "{{{x}: center}}", "ai"),
    ("$jc",   "\"justify_content\"",  "{{{x}: center}}", "jc"),
    ("$mw",   "\"max_width\"",        "{{{x}: $pct}}", "mw"),
    ("$mt",   "\"margin_top\"",       "{{{x}: $p4}}", "mt"),
    ("$blt",  "\"border_left\"",      "{{{x}: $br1}}", "blt"),
    ("$ff_k", "\"font_family\"",      "{{{x}: $ff}}", "ff_k"),
    ("$ws",   "\"white_space\"",      "{{{x}: $pwr}}", "ws"),
    ("$ls",   "\"list_style\"",       "{{{x}: none}}", "ls"),
    # CSS values
    ("$sbw",  "\"space-between\"",    "({x},)", "sbw"),
    ("$ib",   "\"inline-block\"",     "({x},)", "ib"),
    ("$pwr",  "\"pre-wrap\"",         "({x},)", "pwr"),
    ("$pct",  "\"100%\"",             "({x},)", "pct"),
    ("$vh",   "\"100vh\"",            "({x},)", "vh"),
    ("$w6",   "\"600\"",              "({x},)", "w6"),
    ("$w7",   "\"700\"",              "({x},)", "w7"),
]


def count(text: str, enc) -> int:
    return len(enc.encode(text))


def main() -> int:
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except (AttributeError, OSError):
        pass

    print("# Phase 2.11 dictionary audit\n")
    print("| Alias | Canonical | Alias cost | Canon cost | Δ cl100k | Δ o200k | Verdict |")
    print("|---|---|---:|---:|---:|---:|---|")

    save = []
    neutral = []
    regress = []
    for alias, canonical, ctx, label in ENTRIES:
        # NOTE: alias contains a literal `$` which the substitute pass
        # would expand. Here we just measure tokens of the .psc surface
        # form (with `$`) vs the canonical form (the expansion).
        a_text = ctx.format(x=alias)
        c_text = ctx.format(x=canonical)
        a_cl = count(a_text, CL)
        c_cl = count(c_text, CL)
        a_o = count(a_text, O200)
        c_o = count(c_text, O200)
        d_cl = c_cl - a_cl  # positive = alias saves
        d_o = c_o - a_o
        verdict = "save" if d_cl > 0 else ("neutral" if d_cl == 0 else "REGRESS")
        target = save if d_cl > 0 else (neutral if d_cl == 0 else regress)
        target.append((label, d_cl))
        # Truncate long canonicals for display.
        c_disp = canonical if len(canonical) < 30 else canonical[:27] + "..."
        print(f"| `{alias}` | `{c_disp}` | {a_cl} | {c_cl} | **{d_cl:+d}** | {d_o:+d} | {verdict} |")

    print()
    print(f"## Summary: {len(save)} save, {len(neutral)} neutral, {len(regress)} regress\n")
    if neutral or regress:
        print("**Candidates to prune** (zero-saving or regressing on cl100k):\n")
        for label, d in neutral + regress:
            print(f"- `${label}` ({d:+d} cl100k)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
