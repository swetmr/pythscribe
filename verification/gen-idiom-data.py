#!/usr/bin/env python3
"""Generate verification/IdiomData.lean from the committed Tier-E fixture.

Source of truth: verification/idiom-table.toml (`[expand.idioms]`).

UNLIKE the other tiers, Tier E has no compiler-side table to generate from:
`idioms::substitute_with_map` takes its map from the user's pyths.toml and
is EMPTY by default. So the object pinned here is the SCANNER, not a
shipped table — see the header of idiom-table.toml. The same fixture is fed
to the real `pyths expand` by diff_harness.py --tier idioms, so the Lean
Tier-E model and the Rust Tier-E scanner are differentially compared over
one identical table.

Usage:  python verification/gen-idiom-data.py          # (re)write IdiomData.lean
        python verification/gen-idiom-data.py --check  # exit 1 on drift
"""
from __future__ import annotations

import sys
from pathlib import Path

try:
    import tomllib  # Python >= 3.11
except ModuleNotFoundError:  # pragma: no cover
    raise SystemExit("gen-idiom-data.py needs Python >= 3.11 (tomllib)")

ROOT = Path(__file__).resolve().parent.parent
FIXTURE = ROOT / "verification" / "idiom-table.toml"
OUT = ROOT / "verification" / "IdiomData.lean"


def lean_str(s: str) -> str:
    return (
        '"'
        + s.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
        + '"'
    )


def main() -> int:
    # Normalize line endings BEFORE parsing. The fixture's multi-line literal
    # strings (''' … ''') preserve their content byte-for-byte, so on a Windows
    # checkout (core.autocrlf) a raw binary read would put `\r\n` inside every
    # multi-line fragment and this generator's output would differ from CI's.
    # Read as text + strip CR so the generated file is checkout-independent.
    # (gates.rs normalizes the same way before hashing the fixture.)
    raw = FIXTURE.read_text(encoding="utf-8").replace("\r\n", "\n")
    data = tomllib.loads(raw)
    idioms = data.get("expand", {}).get("idioms", {})
    if not idioms:
        raise SystemExit("no [expand.idioms] entries in idiom-table.toml")

    # `pyths` builds a HashMap, whose iteration order is irrelevant: the
    # scanner does a single `map.get(name)` per sigil. The Lean model uses an
    # association list with first-match-wins `elookup`, so ANY deterministic
    # order is faithful PROVIDED the keys are unique — TOML already
    # guarantees that (duplicate keys are a parse error). Sort for a stable,
    # regenerable file.
    entries = sorted(idioms.items())

    lines = [
        "/-",
        "  GENERATED FILE — do not edit by hand.",
        "",
        "  The committed Tier-E idiom fixture, generated from",
        "  verification/idiom-table.toml by verification/gen-idiom-data.py.",
        "  CI regenerates this file and fails on any diff.",
        "",
        "  NOTE (honest scoping): Tier E has no compiler-side table — the",
        "  `%NAME` map is supplied by the user's pyths.toml and is empty by",
        "  default. This fixture is therefore a TEST TABLE, not a shipped",
        "  one; what the Tier-E proofs and the differential pin is the",
        "  SCANNER (idioms.rs::substitute_with_map), which is shipped. The",
        "  Lean theorems are stated for an ARBITRARY table; this fixture is",
        "  what the differential harness feeds to both sides.",
        "-/",
        "",
        "namespace PythExpandVerify",
        "",
        "/-- The committed Tier-E idiom fixture (name, canonicalFragment) — "
        f"{len(entries)} entries. -/",
        "def committedIdioms : List (String × String) := [",
    ]
    lines.append(",\n".join(f"  ({lean_str(k)}, {lean_str(v)})" for k, v in entries))
    lines.append("]")
    lines.append("")
    lines.append("end PythExpandVerify")
    content = "\n".join(lines) + "\n"

    if "--check" in sys.argv:
        on_disk = OUT.read_text(encoding="utf-8") if OUT.exists() else ""
        if on_disk != content:
            print("IdiomData.lean is stale — regenerate with: "
                  "python verification/gen-idiom-data.py")
            return 1
        print(f"IdiomData.lean in sync ({len(entries)} entries)")
        return 0

    OUT.write_text(content, encoding="utf-8", newline="\n")
    print(f"wrote {OUT} ({len(entries)} entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
