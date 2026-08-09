#!/usr/bin/env python3
"""Audit each .psc alias for actual token savings.

For each (alias, canonical) pair, measures the cl100k_base and
o200k_base token cost of the alias and the canonical form in a
realistic surrounding context. Substitutions where Δtok ≤ 0 are
flagged — they consume engineering complexity for zero gain and
should be reconsidered.
"""
from __future__ import annotations

import sys

try:
    import tiktoken
except ImportError:
    print("ERROR: tiktoken not installed. Run: pip install tiktoken", file=sys.stderr)
    sys.exit(2)

CL = tiktoken.get_encoding("cl100k_base")
O200 = tiktoken.get_encoding("o200k_base")

# (alias, canonical, kind)
SUBSTITUTIONS = [
    # Presets (whole-line)
    ("R*", "from pyths.react import component, use_state, use_effect, use_callback, use_memo", "preset"),
    ("A*", "from pyths.asyncio import gather, sleep", "preset"),
    ("T*", "from dataclasses import dataclass", "preset"),
    ("T+", "from dataclasses import dataclass, Field", "preset"),
    ("D*", "from pyths.dom import query, query_all, get_element_by_id, set_text, get_text, add_event_listener", "preset"),
    ("W*", "from pyths.web import handler, Response", "preset"),
    # Decorators
    ("@c", "@component", "decorator"),
    ("@d", "@dataclass", "decorator"),
    ("@v", "@validator", "decorator"),
    ("@h", "@handler", "decorator"),
    ("@k", "@check", "decorator"),
    # Kwargs (kwarg-position only; measure in `(NAME=` context)
    ("st", "style", "kwarg"),
    ("cn", "class_name", "kwarg"),
    ("cl", "className", "kwarg"),
    ("oc", "on_click", "kwarg"),
    ("oh", "on_change", "kwarg"),
    ("os", "on_submit", "kwarg"),
    ("oa", "on_acknowledge", "kwarg"),
    ("ph", "placeholder", "kwarg"),
    ("dis", "disabled", "kwarg"),
]


def count(text: str) -> tuple[int, int]:
    return len(CL.encode(text)), len(O200.encode(text))


KWARG_CONTEXTS = [
    "({name}=value)",
    ", {name}=value",
    ",\n    {name}=value",
]
DECORATOR_CONTEXTS = [
    "\n{name}\ndef f():",
    "\n@check\nclass C:\n    {name}\n    def m(self):",  # method-level
]
PRESET_CONTEXTS = [
    "{name}\nx = 1\n",  # standalone whole-line, as in `.psc`
]


def measure(alias: str, canonical: str, kind: str) -> dict:
    if kind == "kwarg":
        templates = KWARG_CONTEXTS
    elif kind == "decorator":
        templates = DECORATOR_CONTEXTS
    else:
        templates = PRESET_CONTEXTS

    a_cl_sum = a_o_sum = c_cl_sum = c_o_sum = 0
    for t in templates:
        a_text = t.format(name=alias)
        c_text = t.format(name=canonical)
        a_cl, a_o = count(a_text)
        c_cl, c_o = count(c_text)
        a_cl_sum += a_cl
        a_o_sum += a_o
        c_cl_sum += c_cl
        c_o_sum += c_o

    return {
        "alias": alias,
        "canonical": canonical,
        "kind": kind,
        "a_cl": a_cl_sum,
        "c_cl": c_cl_sum,
        "saved_cl": c_cl_sum - a_cl_sum,
        "a_o": a_o_sum,
        "c_o": c_o_sum,
        "saved_o": c_o_sum - a_o_sum,
        "contexts": len(templates),
    }


def main() -> int:
    # Windows console fallback: force UTF-8 if available.
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except (AttributeError, OSError):
        pass

    print("# Phase 2.3 alias audit -- per-substitution token deltas\n")
    print(
        "| Kind | Alias | Canonical | cl100k dTok | o200k dTok | Verdict |\n"
        "|---|---|---|---:|---:|---|"
    )
    for alias, canonical, kind in SUBSTITUTIONS:
        m = measure(alias, canonical, kind)
        if m["saved_cl"] > 0:
            verdict = "save"
        elif m["saved_cl"] == 0:
            verdict = "neutral"
        else:
            verdict = "REGRESS"
        c_disp = canonical if len(canonical) < 60 else canonical[:57] + "..."
        print(
            f"| {kind} | `{alias}` | `{c_disp}` | "
            f"{m['saved_cl']:+d} | {m['saved_o']:+d} | {verdict} |"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
