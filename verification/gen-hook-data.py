#!/usr/bin/env python3
"""Generate verification/HookData.lean from the committed hook-alias table.

Source of truth: crates/pyths_expand/src/hooks.rs :: ALIASES.

The `hooks` tier (Step 5 of expand_with_config, `hooks` in TIER_ORDER)
rewrites a hook shorthand ONLY when it is a free identifier in call
position — `us(` → `use_state(` — and never after a `.` (attribute
access) or inside an identifier. Exact companion of gen-dict-data.py /
gen-kwarg-data.py / gen-tiera-data.py; CI regenerates and diffs.

Usage:  python verification/gen-hook-data.py          # (re)write HookData.lean
        python verification/gen-hook-data.py --check  # exit 1 on drift
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HOOKS_RS = ROOT / "crates" / "pyths_expand" / "src" / "hooks.rs"
OUT = ROOT / "verification" / "HookData.lean"

ENTRY_RE = re.compile(
    r'HookAlias\s*\{\s*alias:\s*"(?P<alias>[^"]+)",\s*'
    r'canonical:\s*"(?P<canon>(?:[^"\\]|\\.)*)"\s*,?\s*\}'
)


def rust_str_unescape(s: str) -> str:
    out = s.replace('\\"', '"')
    if "\\" in out.replace("\\\\", ""):
        raise SystemExit(f"unhandled escape in canonical: {s!r}")
    return out


def lean_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main() -> int:
    src = HOOKS_RS.read_text(encoding="utf-8")
    entries = [(m["alias"], rust_str_unescape(m["canon"])) for m in ENTRY_RE.finditer(src)]
    if not entries:
        raise SystemExit("no HookAlias entries found — regex drift?")

    lines = [
        "/-",
        "  GENERATED FILE — do not edit by hand.",
        "",
        "  The committed hook-alias table, generated from",
        "  crates/pyths_expand/src/hooks.rs :: ALIASES by",
        "  verification/gen-hook-data.py. CI regenerates this file and",
        "  fails on any diff, so the Lean hooks-tier instantiation always",
        "  quantifies over the SHIPPING table.",
        "-/",
        "",
        "namespace PythExpandVerify",
        "",
        "/-- The committed hook-alias table (alias, canonicalHook) — "
        f"{len(entries)} entries. -/",
        "def committedHooks : List (String × String) := [",
    ]
    lines.append(",\n".join(f"  ({lean_str(a)}, {lean_str(c)})" for a, c in entries))
    lines.append("]")
    lines.append("")
    lines.append("end PythExpandVerify")
    content = "\n".join(lines) + "\n"

    if "--check" in sys.argv:
        on_disk = OUT.read_text(encoding="utf-8") if OUT.exists() else ""
        if on_disk != content:
            print("HookData.lean is stale — regenerate with: "
                  "python verification/gen-hook-data.py")
            return 1
        print(f"HookData.lean in sync ({len(entries)} entries)")
        return 0

    OUT.write_text(content, encoding="utf-8", newline="\n")
    print(f"wrote {OUT} ({len(entries)} entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
