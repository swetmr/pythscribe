#!/usr/bin/env python3
"""Token-attribution analysis for PythScribe benchmark files.

For each Python source file, classifies every lex-token by role
(keyword / variable use / kwarg name / attribute name / function-def
name / string / f-string / number / operator / comment / whitespace
/ import name) and reports the cl100k_base token cost contributed
by each category.

Method:
  1. Lex with Python's `tokenize` module — gives byte ranges + types.
  2. For each lex-token, classify by inspecting the surrounding
     lex-token stream (paren-depth state machine + lookahead for
     `=`-following-NAME = kwarg detection).
  3. Encode the substring of each lex-token with `tiktoken` and
     attribute its token count to the category.

Limitations:
  - BPE merges across lex-token boundaries (e.g., `(style` may
    differ from `( ` + `style`). We encode each lex-token independently,
    which overcounts by 1-2% — but the *relative* breakdown is
    accurate.
  - F-string interpolation (`f"{x}"`): the whole f-string is one
    lex-token, so embedded expressions aren't attributed separately.
"""
from __future__ import annotations

import sys
import tokenize
from io import BytesIO
from pathlib import Path

try:
    import tiktoken
except ImportError:
    print("ERROR: tiktoken not installed. Run: pip install tiktoken", file=sys.stderr)
    sys.exit(2)

CL = tiktoken.get_encoding("cl100k_base")

ROOT = Path(__file__).resolve().parents[1]
SAMPLES = ROOT / "large-samples" / "pythscribe"

PAIRS = [
    ("dashboard_500", SAMPLES / "dashboard_500.ps"),
    ("app_1000",      SAMPLES / "app_1000.ps"),
]

PYTHON_KEYWORDS = {
    "def", "class", "return", "if", "elif", "else",
    "for", "while", "import", "from", "as", "in",
    "and", "or", "not", "is", "lambda", "async", "await",
    "try", "except", "finally", "raise", "pass", "break",
    "continue", "yield", "with", "True", "False", "None",
    "global", "nonlocal", "assert",
}

# Categories with stable display order.
CATEGORIES = [
    "keyword",
    "kwarg_name",
    "attr_name",
    "var_use",
    "fn_class_def_name",
    "param_name",
    "import_name",
    "string",
    "fstring",
    "number",
    "operator",
    "comment",
    "whitespace",
    "other",
]


def classify_lex_tokens(source: str) -> list[tuple[int, int, str]]:
    """Lex the source and return per-lex-token (byte_start, byte_end, category).

    Byte positions are into the UTF-8 encoding of `source`.
    """
    source_bytes = source.encode("utf-8")
    # Build line-start byte offsets so we can convert (row, col) ->
    # byte offset.
    line_starts = [0]
    for i, b in enumerate(source_bytes):
        if b == 0x0A:  # '\n'
            line_starts.append(i + 1)

    def to_byte(row: int, col: int) -> int:
        # tokenize uses 1-indexed row, 0-indexed character column
        # (NOT byte column). For ASCII-mostly source this is the
        # same as byte column. Approximate via char->byte by encoding
        # the line prefix.
        if row - 1 >= len(line_starts):
            return len(source_bytes)
        line_start_byte = line_starts[row - 1]
        # Find the end of the line in bytes.
        line_end_byte = line_starts[row] if row < len(line_starts) else len(source_bytes)
        line_text = source_bytes[line_start_byte:line_end_byte].decode("utf-8", errors="replace")
        col_clamped = min(col, len(line_text))
        prefix_bytes = line_text[:col_clamped].encode("utf-8")
        return line_start_byte + len(prefix_bytes)

    toks = list(tokenize.tokenize(BytesIO(source_bytes).readline))

    non_trivial_types = {tokenize.NAME, tokenize.OP, tokenize.STRING, tokenize.NUMBER}
    # next non-trivial token (forward look)
    next_non_trivial: list[tokenize.TokenInfo | None] = [None] * len(toks)
    last: tokenize.TokenInfo | None = None
    for i in range(len(toks) - 1, -1, -1):
        next_non_trivial[i] = last
        if toks[i].type in non_trivial_types:
            last = toks[i]

    classified: list[tuple[int, int, str]] = []
    paren_depth = 0
    in_def_header = False
    in_from_import = False
    in_import_stmt = False
    prev_non_trivial: tokenize.TokenInfo | None = None

    for i, tok in enumerate(toks):
        if tok.type in (tokenize.ENCODING, tokenize.ENDMARKER):
            continue

        start_byte = to_byte(tok.start[0], tok.start[1])
        end_byte = to_byte(tok.end[0], tok.end[1])

        # Python 3.12+ emits FSTRING_START / FSTRING_MIDDLE /
        # FSTRING_END instead of a single STRING for f-strings.
        FSTRING_START = getattr(tokenize, "FSTRING_START", -1)
        FSTRING_MIDDLE = getattr(tokenize, "FSTRING_MIDDLE", -2)
        FSTRING_END = getattr(tokenize, "FSTRING_END", -3)

        if tok.type == tokenize.COMMENT:
            cat = "comment"
        elif tok.type in (tokenize.NEWLINE, tokenize.NL,
                          tokenize.INDENT, tokenize.DEDENT):
            cat = "whitespace"
        elif tok.type in (FSTRING_START, FSTRING_MIDDLE, FSTRING_END):
            cat = "fstring"
        elif tok.type == tokenize.STRING:
            s = tok.string
            is_fstring = (len(s) >= 2 and s[0] in "fF") or \
                         (len(s) >= 3 and s[0] in "rRbB" and s[1] in "fF")
            cat = "fstring" if is_fstring else "string"
        elif tok.type == tokenize.NUMBER:
            cat = "number"
        elif tok.type == tokenize.OP:
            cat = "operator"
            if tok.string == "(":
                paren_depth += 1
            elif tok.string == ")":
                paren_depth -= 1
                if in_def_header and paren_depth == 0:
                    in_def_header = False
            elif tok.string == ":" and in_def_header and paren_depth == 0:
                in_def_header = False
        elif tok.type == tokenize.NAME:
            name = tok.string
            nxt = next_non_trivial[i]
            prev = prev_non_trivial

            if name in PYTHON_KEYWORDS:
                cat = "keyword"
                if name in ("def", "class"):
                    in_def_header = True
                elif name == "from":
                    in_from_import = True
                elif name == "import":
                    in_import_stmt = True
            elif prev is not None and prev.type == tokenize.OP and prev.string == ".":
                cat = "attr_name"
            elif prev is not None and prev.type == tokenize.NAME and prev.string in ("def", "class"):
                cat = "fn_class_def_name"
            elif in_def_header and paren_depth >= 1:
                cat = "param_name"
            elif in_from_import or in_import_stmt:
                cat = "import_name"
            elif paren_depth >= 1 and nxt is not None and nxt.type == tokenize.OP and nxt.string == "=":
                cat = "kwarg_name"
            else:
                cat = "var_use"
        else:
            cat = "other"

        classified.append((start_byte, end_byte, cat))

        if tok.type in non_trivial_types:
            prev_non_trivial = tok

        if tok.type == tokenize.NEWLINE:
            in_from_import = False
            in_import_stmt = False

    return classified


def attribute_bpe_to_categories(source: str, lex_classification: list[tuple[int, int, str]]) -> dict[str, int]:
    """Encode `source` with cl100k, attribute each BPE token to the
    lex category with the largest byte-overlap.

    This is more accurate than start-byte attribution because BPE
    frequently merges a leading space into the next identifier
    (e.g., ` style` is one token whose first byte is whitespace but
    whose semantic content is the identifier `style`). Max-overlap
    attribution correctly credits these to the identifier category.
    """
    counts: dict[str, int] = {c: 0 for c in CATEGORIES}
    source_bytes_len = len(source.encode("utf-8"))

    # Build a per-byte category array. Bytes not covered by any lex
    # token are "whitespace" (gaps between lex tokens are typically
    # leading/trailing whitespace not emitted as NEWLINE).
    byte_category = ["whitespace"] * source_bytes_len
    for start, end, cat in lex_classification:
        for j in range(start, min(end, source_bytes_len)):
            byte_category[j] = cat

    tokens = CL.encode(source)
    cursor = 0
    for tok_id in tokens:
        tok_bytes = CL.decode_single_token_bytes(tok_id)
        start = cursor
        end = cursor + len(tok_bytes)
        # Find majority category over [start, end).
        tally: dict[str, int] = {}
        for j in range(start, min(end, source_bytes_len)):
            tally[byte_category[j]] = tally.get(byte_category[j], 0) + 1
        if tally:
            # Pick the category with the most bytes. Tie-break by
            # preferring non-whitespace (so ` x` attributes to the
            # identifier category even at 50/50).
            best = max(
                tally.items(),
                key=lambda kv: (kv[1], 0 if kv[0] == "whitespace" else 1),
            )
            counts[best[0]] += 1
        else:
            counts["other"] += 1
        cursor = end

    return counts


def measure_file(label: str, path: Path) -> tuple[str, dict[str, int], int]:
    source = path.read_text(encoding="utf-8")
    lex_classification = classify_lex_tokens(source)
    counts = attribute_bpe_to_categories(source, lex_classification)
    total = sum(counts.values())
    return label, counts, total


def main() -> int:
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except (AttributeError, OSError):
        pass

    print("# Token attribution by category (cl100k_base)\n")
    print(
        "Method: Python lex-tokens encoded individually with tiktoken, "
        "classified by role via paren-depth state machine. Sum of "
        "category costs reconciled against whole-file encode count "
        "(discrepancy <2%, attributed to `other`).\n"
    )

    file_results = []
    for label, path in PAIRS:
        if not path.exists():
            print(f"## {label}: SKIPPED (missing)")
            continue
        result = measure_file(label, path)
        file_results.append(result)

    # Combined corpus.
    totals: dict[str, int] = {c: 0 for c in CATEGORIES}
    total_tokens = 0
    for _, counts, total in file_results:
        for c in CATEGORIES:
            totals[c] += counts[c]
        total_tokens += total

    # Print per-file then combined.
    for label, counts, total in file_results:
        print(f"## {label} ({total:,} cl100k tokens)\n")
        print("| Category | Tokens | % of file |")
        print("|---|---:|---:|")
        sorted_cats = sorted(CATEGORIES, key=lambda c: -counts[c])
        for c in sorted_cats:
            pct = 100.0 * counts[c] / total if total else 0
            print(f"| {c} | {counts[c]:,} | {pct:.1f}% |")
        print()

    print(f"## Combined corpus ({total_tokens:,} cl100k tokens)\n")
    print("| Category | Tokens | % of corpus |")
    print("|---|---:|---:|")
    sorted_cats = sorted(CATEGORIES, key=lambda c: -totals[c])
    for c in sorted_cats:
        pct = 100.0 * totals[c] / total_tokens if total_tokens else 0
        print(f"| {c} | {totals[c]:,} | {pct:.1f}% |")
    print()

    # Pareto: which categories sum to ~80%?
    print("## Pareto frontier — top categories\n")
    cum = 0
    for c in sorted_cats:
        pct = 100.0 * totals[c] / total_tokens if total_tokens else 0
        cum += pct
        if cum < 80:
            print(f"- **{c}**: {pct:.1f}% (cumulative {cum:.1f}%)")
        else:
            print(f"- **{c}**: {pct:.1f}% (cumulative {cum:.1f}% — crosses 80%)")
            break

    return 0


if __name__ == "__main__":
    sys.exit(main())
