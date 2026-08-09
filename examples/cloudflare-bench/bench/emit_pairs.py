#!/usr/bin/env python3
"""W1 by-product — emit an Iron-Rule-verified {ps, psc} pair corpus.

Walks tracked `.psc` files, runs `pyths expand` for the canonical `.ps` side,
and (when a sibling `.ps` exists) runs `pyths expand --verify` to confirm the
round-trip Iron Rule. Only verified pairs are emitted.

Output: examples/cloudflare-bench/corpus/ps_psc_pairs.jsonl
  {ps, psc, origin, tiers, verified}
"""
from __future__ import annotations
import json
import re
import subprocess
import sys
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
except Exception:
    pass

RUN = dict(capture_output=True, text=True, encoding="utf-8", errors="replace")

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[2]
PYTHS = REPO_ROOT / "target" / "release" / "pyths.exe"
OUT_DIR = REPO_ROOT / "examples" / "cloudflare-bench" / "corpus"
OUT = OUT_DIR / "ps_psc_pairs.jsonl"


def tracked_psc() -> list[Path]:
    out = subprocess.run(["git", "ls-files", "*.psc"], cwd=REPO_ROOT,
                         capture_output=True, text=True).stdout
    return [REPO_ROOT / line.strip() for line in out.splitlines() if line.strip()]


def detect_tiers(psc: str) -> list[str]:
    tiers = set()
    if re.search(r"^\s*[RTADWG]\*|^\s*R\+|^\s*T\+", psc, re.M):
        tiers.add("A")
    if re.search(r"@[cdvhk]\b", psc):
        tiers.add("A")
    if re.search(r"\b(us|ue|um|uc|ur|ux)\(", psc) or re.search(r"\b(oc|oh|os|oa|cn|cl|st|ph|dis)=", psc):
        tiers.add("B")
    if "$" in psc:
        tiers.add("Dict")
    if re.search(r"%[A-Za-z]", psc):
        tiers.add("E")
    return sorted(tiers)


def main() -> int:
    if not PYTHS.exists():
        print(f"ERROR: {PYTHS} not found; build with cargo build --release", file=sys.stderr)
        return 2
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    pairs = []
    for psc_path in tracked_psc():
        psc_text = psc_path.read_text(encoding="utf-8", errors="replace")
        # Expand for the canonical side.
        r = subprocess.run([str(PYTHS), "expand", str(psc_path)],
                           cwd=psc_path.parent, **RUN)
        if r.returncode != 0:
            print(f"  expand FAIL {psc_path.name}: {r.stderr.strip()[:80]}", file=sys.stderr)
            continue
        ps_text = r.stdout
        sibling = psc_path.with_suffix(".ps")
        verified = False
        if sibling.exists():
            v = subprocess.run([str(PYTHS), "expand", str(psc_path), "--verify", "--quiet"],
                               cwd=psc_path.parent, **RUN)
            verified = v.returncode == 0
            if not verified:
                print(f"  verify FAIL {psc_path.name}: {v.stderr.strip()[:80]}", file=sys.stderr)
                continue  # Iron-Rule-verified pairs only
        else:
            # No sibling .ps: expansion is deterministic but not round-trip-checkable.
            # Skip to honour "Iron-Rule-verified pairs only".
            continue
        rel = psc_path.relative_to(REPO_ROOT).as_posix()
        pairs.append({
            "ps": ps_text,
            "psc": psc_text,
            "origin": rel,
            "tiers": detect_tiers(psc_text),
            "verified": verified,
        })

    OUT.write_text("\n".join(json.dumps(p, ensure_ascii=False) for p in pairs) + "\n",
                   encoding="utf-8")
    print(f"Wrote {len(pairs)} Iron-Rule-verified pairs → {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
