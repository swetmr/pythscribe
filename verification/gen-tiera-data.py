#!/usr/bin/env python3
"""Generate verification/TierAData.lean from the committed Tier-A tables.

Sources of truth:
  crates/pyths_expand/src/presets.rs    :: PRESETS    (import presets)
  crates/pyths_expand/src/decorators.rs :: ALIASES    (decorator aliases)

Tier A is a per-LINE rewrite (lib.rs::expand_line): a line whose trimmed
body is exactly a preset marker expands to the canonical import; a line
whose trimmed body starts with a decorator alias expands that alias.
Both tables are finite and committed, so the Lean Tier-A instantiation is
generated from them and CI fails on drift (`--check`), exactly as for
gen-dict-data.py (strings.rs) and gen-kwarg-data.py (kwargs.rs).

Usage:  python verification/gen-tiera-data.py          # (re)write TierAData.lean
        python verification/gen-tiera-data.py --check  # exit 1 on drift
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PRESETS_RS = ROOT / "crates" / "pyths_expand" / "src" / "presets.rs"
DECORATORS_RS = ROOT / "crates" / "pyths_expand" / "src" / "decorators.rs"
OUT = ROOT / "verification" / "TierAData.lean"

PRESET_RE = re.compile(
    r'Preset\s*\{\s*marker:\s*"(?P<marker>(?:[^"\\]|\\.)*)",\s*'
    r'expansion:\s*"(?P<expansion>(?:[^"\\]|\\.)*)"\s*,?\s*\}'
)
DECO_RE = re.compile(
    r'DecoratorAlias\s*\{\s*alias:\s*"(?P<alias>(?:[^"\\]|\\.)*)",\s*'
    r'canonical:\s*"(?P<canon>(?:[^"\\]|\\.)*)"\s*,?\s*\}'
)


def rust_str_unescape(s: str) -> str:
    """Both tables are plain ASCII today. Stay exact and fail loud."""
    out = s.replace('\\"', '"')
    if "\\" in out.replace("\\\\", ""):
        raise SystemExit(f"unhandled escape in Tier-A table entry: {s!r}")
    return out


def lean_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def table(name: str, doc: str, entries: list[tuple[str, str]]) -> list[str]:
    lines = [
        f"/-- {doc} — {len(entries)} entries. -/",
        f"def {name} : List (String × String) := [",
    ]
    lines.append(",\n".join(f"  ({lean_str(a)}, {lean_str(b)})" for a, b in entries))
    lines.append("]")
    lines.append("")
    return lines


def main() -> int:
    psrc = PRESETS_RS.read_text(encoding="utf-8")
    dsrc = DECORATORS_RS.read_text(encoding="utf-8")

    presets = [
        (rust_str_unescape(m["marker"]), rust_str_unescape(m["expansion"]))
        for m in PRESET_RE.finditer(psrc)
    ]
    decos = [
        (rust_str_unescape(m["alias"]), rust_str_unescape(m["canon"]))
        for m in DECO_RE.finditer(dsrc)
    ]
    if not presets:
        raise SystemExit("no Preset entries found in presets.rs — regex drift?")
    if not decos:
        raise SystemExit("no DecoratorAlias entries found in decorators.rs — regex drift?")

    lines = [
        "/-",
        "  GENERATED FILE — do not edit by hand.",
        "",
        "  The committed Tier-A tables, generated from",
        "    crates/pyths_expand/src/presets.rs    :: PRESETS",
        "    crates/pyths_expand/src/decorators.rs :: ALIASES",
        "  by verification/gen-tiera-data.py. CI regenerates this file and",
        "  fails on any diff, so the Lean Tier-A instantiation always",
        "  quantifies over the SHIPPING tables.",
        "-/",
        "",
        "namespace PythExpandVerify",
        "",
    ]
    lines += table("committedPresets",
                   "The committed import-preset table (marker, canonicalImportLine)",
                   presets)
    lines += table("committedDecorators",
                   "The committed decorator-alias table (alias, canonicalDecorator)",
                   decos)
    lines.append("end PythExpandVerify")
    content = "\n".join(lines) + "\n"

    if "--check" in sys.argv:
        on_disk = OUT.read_text(encoding="utf-8") if OUT.exists() else ""
        if on_disk != content:
            print("TierAData.lean is stale — regenerate with: "
                  "python verification/gen-tiera-data.py")
            return 1
        print(f"TierAData.lean in sync ({len(presets)} presets, {len(decos)} decorators)")
        return 0

    OUT.write_text(content, encoding="utf-8", newline="\n")
    print(f"wrote {OUT} ({len(presets)} presets, {len(decos)} decorators)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
