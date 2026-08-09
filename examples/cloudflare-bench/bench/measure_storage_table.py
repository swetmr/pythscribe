#!/usr/bin/env python3
"""Storage table for the idiomatic corpus: .tsx vs .ps vs .psc, o200k + cl100k.

Extends the committed `measure_psc.py` methodology (Python `tiktoken`, text-mode
read) to the React `.tsx` side, so a single tool produces every number in the
paper's storage table:

  * `.tsx` / `.ps` / `.psc` token counts (per file and combined),
  * the `.psc` -> `.ps` increment (how much MORE the canonical form costs),
  * `.ps` and `.psc` savings vs the `.tsx` React equivalent.

Combined figures are size-weighted (sum of counts), matching `measure_psc.py`.

Usage:  python bench/measure_storage_table.py        (from examples/cloudflare-bench)
Needs:  pip install tiktoken
"""

from pathlib import Path

import tiktoken

ROOT = Path(__file__).resolve().parents[1]
PS_DIR = ROOT / "large-samples" / "pythscribe"
TSX_DIR = ROOT / "large-samples" / "react-equivalent"

# (label, .tsx file, .ps/.psc stem)
PAIRS = [
    ("dashboard_500", "Dashboard500.tsx", "dashboard_500"),
    ("app_1000", "App1000.tsx", "app_1000"),
]

ENCODINGS = ["o200k_base", "cl100k_base"]


def count(enc, path: Path) -> int:
    return len(enc.encode(path.read_text(encoding="utf-8")))


def pct(new: int, base: int) -> float:
    """Percent saved going from `base` down to `new` (positive = smaller)."""
    return (base - new) / base * 100.0


def main() -> None:
    for enc_name in ENCODINGS:
        enc = tiktoken.get_encoding(enc_name)
        print(f"\n## {enc_name}\n")
        print("| File | .tsx | .ps | .psc | .psc->.ps increment | .ps vs .tsx | .psc vs .tsx |")
        print("|---|---:|---:|---:|---:|---:|---:|")

        totals = {"tsx": 0, "ps": 0, "psc": 0}
        for label, tsx_name, stem in PAIRS:
            tsx = count(enc, TSX_DIR / tsx_name)
            ps = count(enc, PS_DIR / f"{stem}.ps")
            psc = count(enc, PS_DIR / f"{stem}.psc")
            totals["tsx"] += tsx
            totals["ps"] += ps
            totals["psc"] += psc
            print(
                f"| {label} | {tsx:,} | {ps:,} | {psc:,} | "
                f"+{pct(psc, ps):.1f}% | +{pct(ps, tsx):.1f}% | +{pct(psc, tsx):.1f}% |"
            )

        t, p, c = totals["tsx"], totals["ps"], totals["psc"]
        print(
            f"| **combined** | **{t:,}** | **{p:,}** | **{c:,}** | "
            f"**+{pct(c, p):.1f}%** | **+{pct(p, t):.1f}%** | **+{pct(c, t):.1f}%** |"
        )

    print(
        "\n_`.psc->.ps increment` = how many more tokens the canonical `.ps` form "
        "costs than the compressed `.psc`; `X vs .tsx` = tokens saved against the "
        "React+TS equivalent. Combined rows are size-weighted._"
    )


if __name__ == "__main__":
    main()
