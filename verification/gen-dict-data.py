#!/usr/bin/env python3
"""Generate verification/DictData.lean from the committed `$NAME` table.

Source of truth: crates/pyths_expand/src/strings.rs :: ALIASES.
Deterministic output — CI regenerates and `git diff --exit-code`s the
result, so the Lean development's concrete dictionary can never drift
from the shipping table (companion to the FNV manifest gate in
crates/pyths_expand/tests/gates.rs).

Usage:  python verification/gen-dict-data.py          # (re)write DictData.lean
        python verification/gen-dict-data.py --check  # exit 1 on drift
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STRINGS_RS = ROOT / "crates" / "pyths_expand" / "src" / "strings.rs"
OUT = ROOT / "verification" / "DictData.lean"

ENTRY_RE = re.compile(
    r'StringAlias\s*\{\s*alias:\s*"(?P<alias>[^"]+)",\s*canonical:\s*"(?P<canon>(?:[^"\\]|\\.)*)"\s*,?\s*\}'
)


def rust_str_unescape(s: str) -> str:
    # The table only uses \" escapes today; keep this exact and loud.
    out = s.replace('\\"', '"')
    if "\\" in out.replace("\\\\", ""):
        raise SystemExit(f"unhandled escape in canonical: {s!r}")
    return out


def lean_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main() -> int:
    src = STRINGS_RS.read_text(encoding="utf-8")
    entries = [(m["alias"], rust_str_unescape(m["canon"])) for m in ENTRY_RE.finditer(src)]
    if not entries:
        raise SystemExit("no StringAlias entries found — regex drift?")

    lines = [
        "/-",
        "  GENERATED FILE — do not edit by hand.",
        "",
        "  The committed `$NAME` dictionary, generated from",
        "  crates/pyths_expand/src/strings.rs :: ALIASES by",
        "  verification/gen-dict-data.py. CI regenerates this file and",
        "  fails on any diff, so the Lean development always quantifies",
        "  over the SHIPPING table.",
        "-/",
        "",
        "namespace PythExpandVerify",
        "",
        "/-- The committed alias table (alias, canonicalValue) — "
        f"{len(entries)} entries. -/",
        "def committedDict : List (String × String) := [",
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
            print("DictData.lean is stale — regenerate with: python verification/gen-dict-data.py")
            return 1
        print(f"DictData.lean in sync ({len(entries)} entries)")
        return 0

    OUT.write_text(content, encoding="utf-8", newline="\n")
    print(f"wrote {OUT} ({len(entries)} entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
