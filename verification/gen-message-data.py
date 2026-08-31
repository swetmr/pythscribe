#!/usr/bin/env python3
"""Generate verification/MessageData.lean from the canonical message table.

Source of truth: verification/message-table.json (`templates` section) — the
E7 exception-message layer. Deterministic output; CI regenerates and
`git diff --exit-code`s the result (the gen-dict-data.py drift-gate pattern),
so the Lean C1C3C4 message model can never drift from the table that the
shipped-binding differential (verification/message_shipped_binding.py) pins
against the REAL `pyths` binary and the CPython oracle.

The forcing chain this closes (the 3.14 oracle-bump lesson): a runtime
message change → message_shipped_binding.py goes red (pyths != table) →
the table is updated → this gate goes red until MessageData.lean is
regenerated → C1C3C4Outcome.lean's #guard pins re-evaluate against the new
literal. The Lean gate can no longer stay green while asserting an obsolete
message.

Usage:  python verification/gen-message-data.py          # (re)write MessageData.lean
        python verification/gen-message-data.py --check  # exit 1 on drift
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
TABLE = HERE / "message-table.json"
OUT = HERE / "MessageData.lean"

PLACEHOLDER = re.compile(r"\{([a-z]+)\}")


def camel(name: str) -> str:
    parts = name.split("_")
    return parts[0] + "".join(p.capitalize() for p in parts[1:])


def lean_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def template_def(name: str, template: str) -> str:
    """Render one template as a Lean def: placeholders become String params
    (in first-appearance order); the body is the exact concatenation."""
    params: list[str] = []
    for m in PLACEHOLDER.finditer(template):
        if m.group(1) not in params:
            params.append(m.group(1))
    # Split the template into literal / placeholder pieces.
    pieces: list[str] = []
    pos = 0
    for m in PLACEHOLDER.finditer(template):
        if m.start() > pos:
            pieces.append(lean_str(template[pos:m.start()]))
        pieces.append(m.group(1))
        pos = m.end()
    if pos < len(template):
        pieces.append(lean_str(template[pos:]))
    body = " ++ ".join(pieces) if pieces else '""'
    if params:
        sig = " ".join(f"({p} : String)" for p in params)
        return f"def {camel(name)} {sig} : String :=\n  {body}"
    return f"def {camel(name)} : String := {body}"


def generate() -> str:
    table = json.loads(TABLE.read_text(encoding="utf-8"))
    templates: dict[str, str] = table["templates"]
    lines = [
        "/-",
        "  GENERATED FILE — do not edit by hand.",
        "",
        "  The canonical exception-message layer, generated from",
        "  verification/message-table.json by verification/gen-message-data.py.",
        "  CI regenerates this file and fails on any diff, so the C1C3C4",
        "  message model always states the SAME literals the shipped-binding",
        "  differential (message_shipped_binding.py) pins against the real",
        "  `pyths` binary and the CPython oracle"
        f" ({table.get('oracle', 'unpinned')}).",
        "-/",
        "",
        "namespace MessageData",
        "",
    ]
    for name, template in templates.items():
        lines.append(f"/-- `{name}`: {json.dumps(template)} -/")
        lines.append(template_def(name, template))
        lines.append("")
    lines.append("end MessageData")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    text = generate()
    if "--check" in sys.argv[1:]:
        current = OUT.read_text(encoding="utf-8") if OUT.exists() else ""
        # Line-ending-insensitive: git's eol conversion must not fake drift.
        if current.replace("\r\n", "\n") != text:
            sys.stderr.write(
                "MessageData.lean is out of date with message-table.json.\n"
                "Run: python verification/gen-message-data.py\n")
            return 1
        print("MessageData.lean is in sync with message-table.json.")
        return 0
    OUT.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
