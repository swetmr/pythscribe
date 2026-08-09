#!/usr/bin/env python3
"""W1 — Normalise the three PLDI'26-keynote token-savings papers into one
transform table: `transforms.jsonl`.

Sources
-------
  * SimPy       (arXiv 2404.16333) — AI-oriented Python grammar. Keyword/
                operator/block-delimiter/whitespace compaction; AST-preserving.
  * ShortCoder  (arXiv 2601.09703) — 10 AST-preserving source simplification
                rules (aug-assign, comprehension conversion, ternary, etc.).
  * Token Sugar (arXiv 2512.08266, ASE'25) — 799 mined reversible
                (code-pattern -> shorthand) pairs. Artifact fetched from
                github.com/v587su/TokenSugar (mined on LeetCode Python).

Each row of transforms.jsonl:
  {source, rule_id, pattern, replacement, kind, note}

`kind` is one of:
  keyword | operator | block | whitespace | line-sep   (SimPy — grammar-level)
  rewrite                                                (ShortCoder — AST-altering)
  sugar-stmt | sugar-stmt-head | sugar-expr             (Token Sugar — reversible)

The `replacement` for Token Sugar rows is a synthesised sigil alias (our
`%NAME` Tier-E form) — the paper itself uses a retrained `<ID>` token, which
we cannot reproduce zero-shot; we substitute the cheapest fixed-vocab sigil
alias so the BPE-wall screen is apples-to-apples with our expander.
"""
from __future__ import annotations
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
TS_JSON = HERE / "tokensugar_mined_sugars.json"
OUT = HERE / "transforms.jsonl"

rows: list[dict] = []

# ---------------------------------------------------------------------------
# 1. SimPy — grammar-level compaction rules (from arXiv 2404.16333 §Grammar).
#    These are AST-preserving grammar token substitutions + whitespace removal.
#    We record them as (pattern -> replacement) at the surface level so the
#    applicability filter can reason about them; SimPy's true form retrains the
#    tokenizer to add <def_stmt> etc. as single vocab entries.
# ---------------------------------------------------------------------------
SIMPY = [
    ("simpy-kw-def",    "def ",          "<def_stmt>",   "keyword",
     "compound-stmt keyword -> single grammar token"),
    ("simpy-kw-class",  "class ",        "<class_stmt>", "keyword", ""),
    ("simpy-kw-if",     "if ",           "<if_stmt>",    "keyword", ""),
    ("simpy-kw-for",    "for ",          "<for_stmt>",   "keyword", ""),
    ("simpy-kw-while",  "while ",        "<while_stmt>", "keyword", ""),
    ("simpy-kw-with",   "with ",         "<with_stmt>",  "keyword", ""),
    ("simpy-kw-try",    "try:",          "<try_stmt>",   "keyword", ""),
    ("simpy-kw-return", "return ",       "<return_stmt>","keyword", ""),
    ("simpy-op-ge",     ">=",            "<ge>",         "operator", ""),
    ("simpy-op-arrow",  "->",            "<arrow>",      "operator", ""),
    ("simpy-colon-def", "def NAME(params):", "<def_stmt> NAME params", "block",
     "drop parens + colon around def params"),
    ("simpy-colon-if",  "if EXPR:",      "<if_stmt> EXPR", "block",
     "drop trailing colon on compound stmt"),
    ("simpy-block",     "NEWLINE INDENT ... DEDENT", "<block_start> ... <block_end>", "block",
     "3 block terminals -> 2"),
    ("simpy-ws-indent", "<indent/newline/continuation>", "<elided>", "whitespace",
     "eliminate line breaks, indents, line-continuation symbols"),
    ("simpy-linesep",   "stmt NEWLINE",  "stmt [<line_sep>]", "line-sep",
     "mandatory NEWLINE -> optional line separator"),
]
for rid, pat, rep, kind, note in SIMPY:
    rows.append({"source": "SimPy", "rule_id": rid, "pattern": pat,
                 "replacement": rep, "kind": kind, "note": note})

# ---------------------------------------------------------------------------
# 2. ShortCoder — 10 AST-preserving simplification rules (arXiv 2601.09703).
#    These are source->source rewrites that CHANGE the surface AST shape
#    (e.g. x=x+1 -> x+=1). Recorded as (before -> after) rewrites.
# ---------------------------------------------------------------------------
SHORTCODER = [
    ("sc-01-multi-assign", "a = 5\\nb = 5",            "a = b = 5"),
    ("sc-02-return-parens", "return (x + y)",          "return x + y"),
    ("sc-03-aug-assign",   "x = x + y",                "x += y"),
    ("sc-04-ternary",      "if c:\\n x = a\\nelse:\\n x = b", "x = a if c else b"),
    ("sc-05-elif-chain",   "nested if-else",           "elif chain"),
    ("sc-06-listcomp",     "result = []\\nfor i in items:\\n result.append(i*2)",
                            "result = [i*2 for i in items]"),
    ("sc-07-multi-del",    "del a\\ndel b",            "del a, b"),
    ("sc-08-dict-get",     "if k in d:\\n v = d[k]\\nelse:\\n v = default",
                            "v = d.get(k, default)"),
    ("sc-09-str-format",   '"str" + var + "end"',      '"str{0}end".format(var)'),
    ("sc-10-with-open",    "f = open(p)\\ndata = f.read()\\nf.close()",
                            "with open(p) as f:\\n data = f.read()"),
]
for rid, before, after in SHORTCODER:
    rows.append({"source": "ShortCoder", "rule_id": rid, "pattern": before,
                 "replacement": after, "kind": "rewrite",
                 "note": "AST-altering source simplification"})

# ---------------------------------------------------------------------------
# 3. Token Sugar — 799 mined pairs (fetched artifact). Pattern text uses
#    SUGARWILDCARD_N placeholders; `reward` is per-occurrence saving on THEIR
#    retrained tokenizer. We keep the raw pattern + reward + freq for the
#    applicability/BPE-wall screen. `replacement` is a synthesised `%NAME`
#    sigil alias (our Tier-E form) so the fixed-vocab o200k screen is fair.
# ---------------------------------------------------------------------------
ts = json.loads(TS_JSON.read_text(encoding="utf-8"))
TYPE_MAP = {"stmt": "sugar-stmt", "stmt_head": "sugar-stmt-head", "expr": "sugar-expr"}
for i, s in enumerate(ts["sugar"]):
    rows.append({
        "source": "TokenSugar",
        "rule_id": f"ts-{i:03d}-{s['id'][:8]}",
        "pattern": s["code"],
        "replacement": f"%TS{i}",           # our sigil-alias stand-in
        "kind": TYPE_MAP.get(s["type"], "sugar"),
        "note": f"reward={s['reward']} freq={s['freq']} (LeetCode-mined, their tokenizer)",
        "ts_reward": s["reward"],
        "ts_freq": s["freq"],
    })

OUT.write_text("\n".join(json.dumps(r, ensure_ascii=False) for r in rows) + "\n",
               encoding="utf-8")
print(f"Wrote {len(rows)} transform rows to {OUT}")
print(f"  SimPy:      {sum(1 for r in rows if r['source']=='SimPy')}")
print(f"  ShortCoder: {sum(1 for r in rows if r['source']=='ShortCoder')}")
print(f"  TokenSugar: {sum(1 for r in rows if r['source']=='TokenSugar')}")
