#!/usr/bin/env python3
"""Generate verification/KwargData.lean from the committed Tier-B kwarg table.

Source of truth: crates/pyths_expand/src/kwargs.rs :: ALIASES.
Deterministic output — CI regenerates and `git diff --exit-code`s the
result, so the Lean development's concrete Tier-B table can never drift
from the shipping table. Exact companion of gen-dict-data.py (which does
the same for the `$NAME` dictionary in strings.rs), and of the FNV
manifest gate in crates/pyths_expand/tests/gates.rs, which pins the same
table from the Rust side — so neither side can move alone.

Usage:  python verification/gen-kwarg-data.py          # (re)write KwargData.lean
        python verification/gen-kwarg-data.py --check  # exit 1 on drift
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
KWARGS_RS = ROOT / "crates" / "pyths_expand" / "src" / "kwargs.rs"
OUT = ROOT / "verification" / "KwargData.lean"

ENTRY_RE = re.compile(
    r'KwargAlias\s*\{\s*alias:\s*"(?P<alias>[^"]+)",\s*canonical:\s*"(?P<canon>(?:[^"\\]|\\.)*)"\s*,?\s*\}'
)


def rust_str_unescape(s: str) -> str:
    # The kwarg table is plain identifiers today; keep this exact and loud.
    out = s.replace('\\"', '"')
    if "\\" in out.replace("\\\\", ""):
        raise SystemExit(f"unhandled escape in canonical: {s!r}")
    return out


def lean_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main() -> int:
    src = KWARGS_RS.read_text(encoding="utf-8")
    entries = [(m["alias"], rust_str_unescape(m["canon"])) for m in ENTRY_RE.finditer(src)]
    if not entries:
        raise SystemExit("no KwargAlias entries found — regex drift?")

    lines = [
        "/-",
        "  GENERATED FILE — do not edit by hand.",
        "",
        "  The committed Tier-B kwarg-alias table, generated from",
        "  crates/pyths_expand/src/kwargs.rs :: ALIASES by",
        "  verification/gen-kwarg-data.py. CI regenerates this file and",
        "  fails on any diff, so the Lean Tier-B instantiation always",
        "  quantifies over the SHIPPING table.",
        "-/",
        "",
        "namespace PythExpandVerify",
        "",
        "/-- The committed kwarg-alias table (alias, canonicalKwarg) — "
        f"{len(entries)} entries. -/",
        "def committedKwargs : List (String × String) := [",
    ]
    body = ",\n".join(f"  ({lean_str(a)}, {lean_str(c)})" for a, c in entries)
    lines.append(body)
    lines.append("]")
    lines.append("")
    lines.append("end PythExpandVerify")
    content = "\n".join(lines) + "\n"

    if "--check" in sys.argv:
        on_disk = OUT.read_text(encoding="utf-8") if OUT.exists() else ""
        if on_disk != content:
            print("KwargData.lean is stale — regenerate with: python verification/gen-kwarg-data.py")
            return 1
        print(f"KwargData.lean in sync ({len(entries)} entries)")
        return 0

    OUT.write_text(content, encoding="utf-8", newline="\n")
    print(f"wrote {OUT} ({len(entries)} entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
