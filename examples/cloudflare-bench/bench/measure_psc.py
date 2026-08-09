#!/usr/bin/env python3
"""Measure Phase 2 `.psc` (compressed) vs `.ps` (canonical) token cost.

Tokenizers measured directly:
  * cl100k_base — GPT-4 / GPT-3.5-Turbo
  * o200k_base  — GPT-4o / GPT-5

Tokenizers measured via heuristic (Anthropic's tokenizer isn't
downloadable; the published statement is "Claude produces ~10–20%
more tokens than GPT for identical text" — 65K vocab vs cl100k's
100K):
  * claude_low  = cl100k * 1.10   (lower bound)
  * claude_mid  = cl100k * 1.15   (midpoint estimate)
  * claude_high = cl100k * 1.20   (upper bound)

The proportional saving (saved %) is identical across the three
heuristics because they're constant multipliers; only the absolute
token counts shift. Hence we report `claude_mid` as the headline.

For a precise Claude measurement, replace the heuristic with calls
to Anthropic's `/v1/messages/count_tokens` endpoint (costs API
credits but otherwise free).

Requires `tiktoken`:  pip install tiktoken

Usage:
  python bench/measure_psc.py
"""
from __future__ import annotations

import sys
from pathlib import Path

try:
    import tiktoken
except ImportError:
    print("ERROR: tiktoken not installed. Run: pip install tiktoken", file=sys.stderr)
    sys.exit(2)

ROOT = Path(__file__).resolve().parents[1]
SAMPLES = ROOT / "large-samples" / "pythscribe"

PAIRS = [
    ("dashboard_500", SAMPLES / "dashboard_500.ps", SAMPLES / "dashboard_500.psc"),
    ("app_1000", SAMPLES / "app_1000.ps", SAMPLES / "app_1000.psc"),
]

ENCODERS = {
    "cl100k_base": tiktoken.get_encoding("cl100k_base"),
    "o200k_base": tiktoken.get_encoding("o200k_base"),
}

CLAUDE_HEURISTIC_MULTIPLIER = 1.15  # midpoint of Anthropic's
                                    # published 10-20% more-tokens
                                    # range vs cl100k.


def measure(text: str) -> dict[str, int]:
    result = {
        "bytes": len(text.encode("utf-8")),
        **{name: len(enc.encode(text)) for name, enc in ENCODERS.items()},
    }
    result["claude_mid"] = round(result["cl100k_base"] * CLAUDE_HEURISTIC_MULTIPLIER)
    return result


def pct(saved: int, original: int) -> str:
    if original == 0:
        return "n/a"
    return f"{100.0 * saved / original:+.1f}%"


METRICS = ("bytes", "cl100k_base", "o200k_base", "claude_mid")


def main() -> int:
    rows = []
    totals_ps = {k: 0 for k in METRICS}
    totals_psc = {k: 0 for k in METRICS}

    for label, ps_path, psc_path in PAIRS:
        if not ps_path.exists() or not psc_path.exists():
            print(f"## {label}: SKIPPED (missing file)")
            continue
        ps = measure(ps_path.read_text(encoding="utf-8"))
        psc = measure(psc_path.read_text(encoding="utf-8"))
        rows.append((label, ps, psc))
        for k in METRICS:
            totals_ps[k] += ps[k]
            totals_psc[k] += psc[k]

    print("# Phase 2 .psc vs .ps token-efficiency comparison\n")
    print(
        "| File | Metric | .ps | .psc | Saved | Saved % |\n"
        "|---|---|---:|---:|---:|---:|"
    )
    for label, ps, psc in rows:
        for metric in METRICS:
            saved = ps[metric] - psc[metric]
            print(
                f"| {label} | {metric} | "
                f"{ps[metric]:,} | {psc[metric]:,} | {saved:,} | "
                f"{pct(saved, ps[metric])} |"
            )
    print()
    print("## Combined corpus (weighted by size)\n")
    print(
        "| Metric | .ps total | .psc total | Saved | Saved % |\n"
        "|---|---:|---:|---:|---:|"
    )
    for metric in METRICS:
        saved = totals_ps[metric] - totals_psc[metric]
        print(
            f"| {metric} | {totals_ps[metric]:,} | {totals_psc[metric]:,} | "
            f"{saved:,} | {pct(saved, totals_ps[metric])} |"
        )
    print()
    print(
        f"_`claude_mid` is a heuristic estimate (cl100k * "
        f"{CLAUDE_HEURISTIC_MULTIPLIER}); proportional saving "
        f"is identical to cl100k since the multiplier is constant._"
    )

    return 0


if __name__ == "__main__":
    sys.exit(main())
