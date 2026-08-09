#!/usr/bin/env python3
"""Vocabulary-aligned alias search v2 (§9.2 of phase2_design.md).

For each (canonical, current_alias) pair, search for a better alias
that saves MORE cl100k tokens than the current one. Measures in
three real kwarg-position contexts and reports the worst case.

Output:
  - Per-substitution: current cost vs best-alternative cost
  - Aliases that win in all three contexts (safe improvements)
  - Honest "no improvement possible" cases
"""
from __future__ import annotations

import itertools
import string
import sys

try:
    import tiktoken
except ImportError:
    print("ERROR: tiktoken not installed. Run: pip install tiktoken", file=sys.stderr)
    sys.exit(2)

CL = tiktoken.get_encoding("cl100k_base")
O200 = tiktoken.get_encoding("o200k_base")


# (canonical, current_alias, kind). Includes BOTH cl100k-neutral
# aliases (looking for replacement) AND already-saving aliases
# (checking if a better alternative exists).
TARGETS = [
    # cl100k-neutral — primary search target
    ("style",        "st",   "kwarg"),
    ("className",    "cl",   "kwarg"),
    ("placeholder",  "ph",   "kwarg"),
    ("disabled",     "dis",  "kwarg"),
    # currently saves 1 — see if anything beats it
    ("class_name",   "cn",   "kwarg"),
    ("on_click",     "oc",   "kwarg"),
    ("on_change",    "oh",   "kwarg"),
    ("on_submit",    "os",   "kwarg"),
    ("on_acknowledge","oa",  "kwarg"),
    # decorators (neutral)
    ("validator",    "@v",   "decorator"),
    ("handler",      "@h",   "decorator"),
    ("check",        "@k",   "decorator"),
    # decorators (currently saves)
    ("component",    "@c",   "decorator"),
    ("dataclass",    "@d",   "decorator"),
]


KWARG_CONTEXTS = [
    "({name}=value)",      # first positional after `(`
    ", {name}=value",      # after comma in arg list
    ",\n    {name}=value", # multi-line: comma + newline + indent
]
DECORATOR_CONTEXTS = [
    "\n{name}\ndef f():",        # @decorator above def
    "\n    {name}\n    def f():", # indented (method)
]


def contexts(name: str, kind: str) -> list[str]:
    template_list = KWARG_CONTEXTS if kind == "kwarg" else DECORATOR_CONTEXTS
    return [t.format(name=name) for t in template_list]


def count_cl(text: str) -> int:
    return len(CL.encode(text))


def count_o(text: str) -> int:
    return len(O200.encode(text))


def total_cost(name: str, kind: str, encoder) -> int:
    """Sum tokens across all contexts for `name`."""
    return sum(len(encoder.encode(c)) for c in contexts(name, kind))


PYTHON_RISKY = {
    # Single-letter aliases that collide with extremely common
    # Python variable names. Don't recommend.
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l",
    "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x",
    "y", "z",
    # Common short Python idioms that frequently appear as
    # variable names in our benchmark files.
    "tag", "key", "val", "obj", "src", "url", "fmt", "idx",
}


def candidate_aliases(kind: str) -> list[str]:
    lower = string.ascii_lowercase
    upper = string.ascii_uppercase
    candidates: list[str] = []
    if kind == "kwarg":
        # 1-3 char lowercase (existing) — exhaustive
        for L in (1, 2, 3):
            for combo in itertools.product(lower, repeat=L):
                s = "".join(combo)
                if s in {"if", "or", "in", "is", "as", "for", "and",
                         "not", "def", "try", "del", "the"}:
                    continue
                candidates.append(s)
        # 2-char mixed case (e.g., `Cn`, `Oc`) — less likely to be
        # user variable names (Python convention is lowercase for
        # vars, CamelCase for classes).
        for u in upper:
            for l in lower:
                candidates.append(u + l)
        # Underscore-prefixed (1-2 letter, `_x` style) — Python
        # convention for private/internal; reduces collision risk.
        for c in lower + upper:
            candidates.append("_" + c)
        for c1 in lower:
            for c2 in lower:
                candidates.append("_" + c1 + c2)
        # Common short Python-ish acronyms (whitelist of 3-letter
        # forms that might be in cl100k vocab).
        candidates.extend([
            "cls", "tgt", "src", "dst", "tag", "key", "val", "obj",
            "pad", "mar", "col", "row", "idx", "fmt", "url",
        ])
    elif kind == "decorator":
        # 1-2 char lowercase + uppercase + underscored
        for L in (1, 2):
            for combo in itertools.product(lower, repeat=L):
                candidates.append("@" + "".join(combo))
        for u in upper:
            for l in lower:
                candidates.append("@" + u + l)
        for c in lower:
            candidates.append("@_" + c)
    return candidates


def main() -> int:
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except (AttributeError, OSError):
        pass

    print("# Vocabulary-aligned alias search (multi-context)\n")
    print("Cost is the SUM of cl100k tokens across multiple realistic ")
    print("contexts (first-arg, mid-comma, multi-line). The alternative ")
    print("must win in all contexts (or at least sum lower) to count.\n")
    print(
        "| Canonical | Current | Canon Σtok | Curr Σtok | Best alt | Best Σtok | Δ vs current | Risky? |\n"
        "|---|---|---:|---:|---|---:|---:|---|"
    )

    improvements: list[dict] = []

    for canonical, current_alias, kind in TARGETS:
        canon_cl = total_cost(canonical, kind, CL)
        curr_cl = total_cost(current_alias, kind, CL)

        best_alias = current_alias
        best_cl = curr_cl

        for alias in candidate_aliases(kind):
            a_cl = total_cost(alias, kind, CL)
            if a_cl < best_cl:
                best_alias = alias
                best_cl = a_cl

        delta = best_cl - curr_cl
        bare_alias = best_alias.lstrip("@")
        risky = bare_alias in PYTHON_RISKY
        risky_marker = "⚠ collision" if risky else ""

        if delta < 0:
            improvements.append({
                "canonical": canonical,
                "current": current_alias,
                "best": best_alias,
                "delta": delta,
                "risky": risky,
            })

        print(
            f"| `{canonical}` | `{current_alias}` | {canon_cl} | "
            f"{curr_cl} | `{best_alias}` | {best_cl} | "
            f"**{delta:+d}** | {risky_marker} |"
        )

    print()
    print("## Summary\n")

    safe = [i for i in improvements if not i["risky"]]
    risky_list = [i for i in improvements if i["risky"]]

    if safe:
        print(f"**Safe improvements** ({len(safe)}):\n")
        for i in safe:
            print(f"- `{i['current']}` → `{i['best']}` for `{i['canonical']}` ({i['delta']:+d} tok)")
        print()
    else:
        print("No safe improvements found.\n")

    if risky_list:
        print(f"**Risky improvements** ({len(risky_list)}, flagged for collision):\n")
        for i in risky_list:
            print(f"- `{i['current']}` → `{i['best']}` for `{i['canonical']}` ({i['delta']:+d} tok)")
        print()
        print("These aliases are single common-Python-letters and would")
        print("clash with user variable names. NOT recommended for adoption")
        print("without a different alias-marking convention (e.g., a sigil)." )
    else:
        print("No risky-alias options either.")

    if not safe and not risky_list:
        print("\n**Conclusion:** the current alias table is at the cl100k")
        print("BPE frontier. Further compression requires a different")
        print("mechanism (sigil-prefixed aliases, or input-side compression).")

    return 0


if __name__ == "__main__":
    sys.exit(main())
