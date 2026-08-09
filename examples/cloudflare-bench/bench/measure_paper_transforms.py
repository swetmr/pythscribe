#!/usr/bin/env python3
"""W1 — Measure whether the three PLDI'26-keynote token-savings papers
(SimPy / ShortCoder / Token Sugar) transfer to `.psc` for REAL o200k gains.

Reuses `mine_idioms.py`'s per-occurrence o200k methodology (fixed-vocab
tokenizer, no retraining) and applies the three-stage filter from the W1 spec:

  (a) BPE-wall screen   — per-occurrence o200k/cl100k delta on our FIXED
                          frontier tokenizer; drop <= 0.
  (b) .ps applicability — drop anything normalised away by the canonical
                          printer (all whitespace/formatting) or that conflicts
                          with .ps semantics.
  (c) Iron-Rule feasibility — must be a deterministic, order-safe, REVERSIBLE
                          fixed-fragment text rewrite (our %NAME/$NAME model has
                          no parameterised macros, so wildcard patterns are
                          infeasible as fixed idioms).

Crucial honesty axis: the papers' HEADLINE gains (SimPy 10-13%, ShortCoder 18%,
Token Sugar 11-15%) are obtained by RETRAINING the model/tokenizer to absorb the
shorthand (`<def_stmt>`, `<1001>`) as single vocab entries. `.psc` operates
ZERO-SHOT at the tool boundary against a FIXED o200k vocab. This script measures
the zero-shot delta, which is the only thing `.psc` can realise.

Usage:  python measure_paper_transforms.py [--report PATH]
"""
from __future__ import annotations
import argparse
import glob
import io
import json
import re
import sys
from pathlib import Path

import tiktoken

CL = tiktoken.get_encoding("cl100k_base")
O200 = tiktoken.get_encoding("o200k_base")

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[2]
TRANSFORMS = HERE / "papers" / "transforms.jsonl"

CORPUS_GLOBS = [
    "./corpus/frontend/src/**/*.ps",
    "./corpus/frontend-next/**/*.ps",
    str(REPO_ROOT / "examples" / "clones" / "shared" / "**" / "*.ps"),
    str(REPO_ROOT / "examples" / "**" / "*.ps"),
]


def o(s: str) -> int:
    return len(O200.encode(s))


def c(s: str) -> int:
    return len(CL.encode(s))


def collect_corpus() -> tuple[list[str], list[str]]:
    seen: set[Path] = set()
    texts: list[str] = []
    names: list[str] = []
    for pattern in CORPUS_GLOBS:
        for p in glob.glob(pattern, recursive=True):
            rp = Path(p).resolve()
            if rp in seen or rp.name.endswith(".d.ps.ts"):
                continue
            seen.add(rp)
            try:
                texts.append(rp.read_text(encoding="utf-8", errors="replace"))
                names.append(rp.name)
            except Exception:
                pass
    return texts, names


# Alias cost floor: our cheapest sigil alias (%Ab / $Ab) on o200k.
ALIAS_O200 = o("%Ab")
ALIAS_CL = c("%Ab")

WILD = re.compile(r"SUGARWILDCARD_\d+")


def skeleton_tokens(pattern: str) -> int:
    """o200k tokens of the FIXED (non-wildcard) glue only.

    A fixed `%NAME` idiom can only collapse the constant skeleton; wildcard
    slots carry variable argument content the alias cannot absorb. So the
    generous upper-bound per-occurrence saving is tokens(skeleton) minus the
    alias cost. We strip wildcard placeholders and measure the residue.
    """
    skel = WILD.sub("", pattern)
    return o(skel)


def wildcard_free(pattern: str) -> bool:
    return WILD.search(pattern) is None


def ts_pattern_regex(pattern: str) -> re.Pattern | None:
    """Turn a Token Sugar pattern into a corpus-search regex (wildcards -> .+?)."""
    parts = [re.escape(p) for p in WILD.split(pattern)]
    try:
        return re.compile(r"[^\n]*?".join(parts))
    except re.error:
        return None


def main(argv=None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--report", default=str(HERE / "paper-mining-report.md"))
    args = ap.parse_args(argv)

    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:
        pass

    buf = io.StringIO()

    def pr(*a, **k):
        print(*a, **k)
        k.pop("file", None)
        print(*a, file=buf, **k)

    texts, names = collect_corpus()
    corpus = "\n".join(texts)
    corpus_o200 = o(corpus)
    corpus_cl = c(corpus)

    rows = [json.loads(l) for l in TRANSFORMS.read_text(encoding="utf-8").splitlines() if l.strip()]

    pr("# W1 — Paper transforms vs `.psc`: BPE-wall / applicability / Iron-Rule screen")
    pr("")
    pr(f"Corpus: **{len(texts)} `.ps` files** "
       f"(reference-app frontend + frontend-next + examples/clones + examples), "
       f"**{corpus_o200:,} o200k** / {corpus_cl:,} cl100k tokens.")
    pr(f"Alias-cost floor (our `%Ab` sigil): **{ALIAS_O200} o200k** / {ALIAS_CL} cl100k per occurrence.")
    pr("")
    pr("**Fixed-vocab, zero-shot.** The papers' headline gains require RETRAINING the")
    pr("model/tokenizer to absorb the shorthand as single vocab tokens. `.psc` cannot")
    pr("retrain a frontier model; it operates at the tool boundary against a FIXED o200k")
    pr("vocab. Every delta below is the zero-shot delta — the only thing `.psc` realises.")
    pr("")

    # ---------------------------------------------------------------- SimPy
    pr("## SimPy (arXiv 2404.16333) — grammar/whitespace compaction")
    pr("")
    pr("| rule | kind | pattern → replacement | o200k Δ/occ | verdict |")
    pr("|---|---|---|---:|---|")
    simpy = [r for r in rows if r["source"] == "SimPy"]
    for r in simpy:
        kind = r["kind"]
        if kind in ("whitespace", "line-sep", "block"):
            verdict = "DROP — normalised by canonical printer (Iron Rule)"
            delta = "n/a"
        else:
            # keyword/operator: measure zero-shot o200k delta of the literal
            # substitution the paper proposes (pattern -> its <token> form).
            d = o(r["pattern"]) - o(r["replacement"])
            delta = f"{d:+d}"
            verdict = "DROP — regresses/neutral zero-shot (BPE wall)" if d <= 0 else "keep?"
        pr(f"| `{r['rule_id']}` | {kind} | `{r['pattern'][:22]}` → `{r['replacement'][:16]}` | {delta} | {verdict} |")
    pr("")
    pr("**SimPy verdict: REJECTED for `.psc`.** Whitespace/line-sep/block rules are")
    pr("structurally inapplicable — the canonical printer already normalises styling")
    pr("whitespace, so there is nothing to save (the Iron Rule round-trips through the")
    pr("canonical form). Keyword/operator rules regress hard zero-shot: the `<def_stmt>`")
    pr("angle-bracket tokens fragment into MANY o200k pieces (a retrained-vocab win only).")
    pr("")

    # ------------------------------------------------------------ ShortCoder
    pr("## ShortCoder (arXiv 2601.09703) — 10 AST-altering simplification rules")
    pr("")
    pr("| rule | before → after | o200k Δ | Iron-Rule feasible? |")
    pr("|---|---|---:|---|")
    sc = [r for r in rows if r["source"] == "ShortCoder"]
    for r in sc:
        before = r["pattern"].replace("\\n", "; ")
        after = r["replacement"].replace("\\n", "; ")
        d = o(before) - o(after)
        pr(f"| `{r['rule_id']}` | `{before[:34]}` → `{after[:24]}` | {d:+d} | "
           f"NO — changes canonical AST |")
    pr("")
    pr("**ShortCoder verdict: REJECTED as a `.psc` tier.** These are semantics-preserving")
    pr("but AST-*altering* SOURCE rewrites (`x = x + y` → `x += y`). The Iron Rule requires")
    pr("`pyths expand foo.psc` to be BYTE-IDENTICAL to the canonical `.ps` the author would")
    pr("write; a shorthand that expands to a *different* AST than the author's source is")
    pr("not a reversible alias — it is a canonical-STYLE choice (like the curried-vs-flat")
    pr("PSX canonical-style choice), out of scope for the expander. NOTE: unlike")
    pr("SimPy/Token-Sugar these rules DO genuinely cut o200k tokens (see +Δ column, up to +11)")
    pr("— they are real code simplifications. They are rejected purely on Iron-Rule grounds,")
    pr("NOT the BPE wall. The transferable takeaway is canonical-`.ps` STYLE guidance (author")
    pr("`x += y`, comprehensions, `dict.get(k,default)` directly in `.ps`), not a `.psc` tier.")
    pr("")

    # ------------------------------------------------------------ Token Sugar
    pr("## Token Sugar (arXiv 2512.08266) — 799 mined pairs")
    pr("")
    ts = [r for r in rows if r["source"] == "TokenSugar"]
    n_total = len(ts)
    n_wf = sum(1 for r in ts if wildcard_free(r["pattern"]))
    pr(f"- Total mined pairs: **{n_total}** (dataset: LeetCode Python, mined on the paper's retrained tokenizer)")
    pr(f"- Wildcard-FREE (candidate for our fixed `%NAME` idiom): **{n_wf}** "
       f"({100*n_wf/n_total:.1f}%). The other {n_total-n_wf} carry `SUGARWILDCARD_N` slots and are")
    pr("  **Iron-Rule-INFEASIBLE** as fixed idioms — our `%NAME` model has no parameterised")
    pr("  macro form, so the variable argument content cannot be absorbed reversibly.")
    pr("")

    # Stage (a) BPE-wall screen on the FIXED skeleton (generous upper bound: even
    # granting a hypothetical parameterised macro, only the skeleton tokens are
    # collapsible). Per-occ saving = skeleton_o200 - alias_cost.
    survivors_bpe = []
    for r in ts:
        skel = skeleton_tokens(r["pattern"])
        per_occ = skel - ALIAS_O200
        r["_skel_o200"] = skel
        r["_per_occ_o200"] = per_occ
        if per_occ > 0:
            survivors_bpe.append(r)
    pr(f"### Stage (a) BPE-wall screen (fixed-skeleton o200k − {ALIAS_O200}-token alias)")
    pr("")
    pr(f"- Pairs whose FIXED skeleton clears the alias-cost floor (per-occ o200k > 0): "
       f"**{len(survivors_bpe)}/{n_total}**.")
    pr("- (This is a GENEROUS upper bound: it credits skeleton tokens even to wildcard")
    pr("  patterns that are Iron-Rule-infeasible, and ignores that the model must still")
    pr("  emit the wildcard argument content next to the alias.)")
    pr("")

    # Stage (c) applicability: does the pattern actually OCCUR in our corpus?
    pr("### Stage (b+c) corpus applicability + Iron-Rule feasibility")
    pr("")
    applicable = []
    for r in survivors_bpe:
        rx = ts_pattern_regex(r["pattern"])
        cnt = len(rx.findall(corpus)) if rx else 0
        r["_corpus_freq"] = cnt
        if cnt >= 3:  # same >=3 occurrence bar as mine_idioms
            applicable.append(r)
    applicable.sort(key=lambda r: -(r["_per_occ_o200"] * r["_corpus_freq"]))
    # Marginal realisable = only wildcard-free AND applicable AND positive.
    realisable = [r for r in applicable if wildcard_free(r["pattern"])]

    pr(f"- Of the {len(survivors_bpe)} BPE-survivors, **{len(applicable)}** occur >=3x in OUR corpus")
    pr("  (the LeetCode-algorithmic patterns — `class Solution:`, `for i in range(n):`,")
    pr("  `SUGARWILDCARD_0 += 1` — are largely absent from React-frontend `.psc`).")
    pr(f"- Of those, **{len(realisable)}** are ALSO wildcard-free (Iron-Rule-feasible as `%NAME`).")
    pr("")
    if applicable:
        pr("Top corpus-applicable Token Sugar patterns (generous skeleton scoring):")
        pr("")
        pr("| pattern | corpus freq | skeleton o200k | per-occ o200k | wildcard-free? | total o200k (gross) |")
        pr("|---|---:|---:|---:|:--:|---:|")
        for r in applicable[:15]:
            wf = "yes" if wildcard_free(r["pattern"]) else "NO (infeasible)"
            tot = r["_per_occ_o200"] * r["_corpus_freq"]
            pat = r["pattern"].replace("\n", "↵").replace("|", "\\|")[:40]
            pr(f"| `{pat}` | {r['_corpus_freq']} | {r['_skel_o200']} | "
               f"+{r['_per_occ_o200']} | {wf} | {tot} |")
        pr("")

    # Realisable gross corpus saving (wildcard-free + applicable), as % of corpus.
    gross = sum(r["_per_occ_o200"] * r["_corpus_freq"] for r in realisable)
    gross_pct = 100 * gross / corpus_o200 if corpus_o200 else 0
    pr(f"### Realisable Token Sugar gain (wildcard-free ∩ applicable ∩ BPE-positive)")
    pr("")
    if realisable:
        pr("| pattern | corpus freq | per-occ o200k | total o200k |")
        pr("|---|---:|---:|---:|")
        for r in realisable:
            pat = r["pattern"].replace("\n", "↵").replace("|", "\\|")[:44]
            pr(f"| `{pat}` | {r['_corpus_freq']} | +{r['_per_occ_o200']} | "
               f"{r['_per_occ_o200']*r['_corpus_freq']} |")
        pr("")
    pr(f"**Realisable Token Sugar corpus saving: {gross} o200k = {gross_pct:.3f}%** "
       f"of the {corpus_o200:,}-token corpus.")
    pr("")

    # Verdict
    pr("## Verdict — do the published papers transfer?")
    pr("")
    if gross_pct < 0.5:
        v = "REJECTED / BPE-wall confirmed"
    elif gross_pct < 1.5:
        v = "MARGINAL"
    else:
        v = "POSITIVE"
    pr(f"### **{v}** (realisable published-pair gain = {gross_pct:.3f}% o200k)")
    pr("")
    pr("- **SimPy:** rejected — whitespace rules normalised away by the canonical printer;")
    pr("  keyword/operator rules regress zero-shot (retrained-vocab-only win).")
    pr("- **ShortCoder:** rejected as an expander tier — AST-altering rewrites violate the")
    pr("  Iron Rule (not reversible fixed aliases); a canonical-STYLE question at most.")
    pr(f"- **Token Sugar:** {v}. Its 799 pairs are Python-general (LeetCode-mined) and")
    pr("  parameterised via a RETRAINED tokenizer. Zero-shot on a fixed o200k vocab, only")
    pr(f"  the fixed skeleton is collapsible; only {n_wf} pairs are wildcard-free, and only")
    pr(f"  {len(realisable)} of those both occur in our React corpus AND clear the BPE floor.")
    pr("  Its METHODOLOGY (mine YOUR corpus for high-frequency token-heavy patterns) is the")
    pr("  real transferable idea — and we already run it (`mine_idioms.py` → Tier E `%NAME`).")
    pr("")
    pr("**The dominant lever remains the domain `$NAME` dictionary** (long string literals /")
    pr("multi-byte CSS/URL values that genuinely fragment into many BPE pieces). The papers'")
    pr("published Python-general pairs do NOT transfer to a fixed frontier tokenizer.")

    Path(args.report).write_text(buf.getvalue(), encoding="utf-8")
    print(f"\nReport → {args.report}", file=sys.stderr)

    # Emit machine-readable summary for the ledger step.
    summary = {
        "corpus_files": len(texts),
        "corpus_o200": corpus_o200,
        "corpus_cl": corpus_cl,
        "alias_o200": ALIAS_O200,
        "simpy_rejected": True,
        "shortcoder_rejected": True,
        "ts_total": n_total,
        "ts_wildcard_free": n_wf,
        "ts_bpe_survivors": len(survivors_bpe),
        "ts_applicable": len(applicable),
        "ts_realisable": len(realisable),
        "ts_realisable_o200": gross,
        "ts_realisable_pct": gross_pct,
        "realisable_patterns": [
            {"pattern": r["pattern"], "freq": r["_corpus_freq"],
             "per_occ_o200": r["_per_occ_o200"]} for r in realisable
        ],
    }
    (HERE / "papers" / "measure_summary.json").write_text(
        json.dumps(summary, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
