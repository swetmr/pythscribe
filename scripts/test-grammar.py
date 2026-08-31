#!/usr/bin/env python3
"""scripts/test-grammar.py — corpus gate for the formal Lark grammars.

Validates grammar/pyths.lark (.ps) and grammar/psc.lark (.psc overlay)
against every syntax corpus in the repo. The hand-written parser in
crates/pyths_parser is AUTHORITATIVE; these grammars are best-effort and
this gate is their only correctness contract (see grammar/README.md).

Corpora:
  1. every tracked .ps file  -> parsed with pyths.lark
  2. every tracked .psc file -> parsed with psc.lark. psc.lark now covers the
     ENTIRE compressed surface (15/15 tracked files parse directly). A file
     psc.lark cannot parse falls back to `pyths expand <file>` with the OUTPUT
     parsed by pyths.lark — the fallback is retained as a safety net but is
     currently unused (the PSX `#id` shortcut, its only former user, is gone).
  3. all entries of tests/differential/cpython_corpus.json
     (_setup + expr joined into a module) -> pyths.lark
  4. the clone pairs in examples/clones/shared/*/ -> the .ps side with
     pyths.lark AND the expanded .psc side (pyths expand) with pyths.lark

Style mirrors scripts/test-readme.mjs: exclusion lists with paper-trail
comments, per-corpus summary counts, nonzero exit on unexcluded failure.

Usage:  pip install lark && python scripts/test-grammar.py
"""

import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXE = ".exe" if sys.platform == "win32" else ""
PYTHS = os.environ.get("PYTHS_BIN", os.path.join(ROOT, "target", "release", f"pyths{EXE}"))

# ---------------------------------------------------------------------------
# Exclusions — the paper trail.
# ---------------------------------------------------------------------------

# .ps files the grammar must NOT be expected to parse. Every entry here is
# rejected by `pyths check` itself (verified 2026-07-05) — the grammar
# failing on them is agreement with pyths_parser, not a gap.
PS_EXCLUSIONS = {
    # Deliberately-invalid negative fixtures (error-recovery tests). Each
    # exists in five copies (tests/fixtures + 4 fuzz seed dirs).
    "tests/fixtures/error_bad_indent.ps": "negative fixture: bad dedent (pyths check rejects)",
    "tests/fixtures/error_missing_colon.ps": "negative fixture: missing ':' (pyths check rejects)",
    "tests/fixtures/error_multiple.ps": "negative fixture: '$'/'@' junk chars (pyths check rejects)",
    "fuzz/seed_corpus/fuzz_check/error_bad_indent.ps": "negative fixture (copy of tests/fixtures)",
    "fuzz/seed_corpus/fuzz_check/error_missing_colon.ps": "negative fixture (copy of tests/fixtures)",
    "fuzz/seed_corpus/fuzz_check/error_multiple.ps": "negative fixture (copy of tests/fixtures)",
    "fuzz/seed_corpus/fuzz_expand/error_bad_indent.ps": "negative fixture (copy of tests/fixtures)",
    "fuzz/seed_corpus/fuzz_expand/error_missing_colon.ps": "negative fixture (copy of tests/fixtures)",
    "fuzz/seed_corpus/fuzz_expand/error_multiple.ps": "negative fixture (copy of tests/fixtures)",
    "fuzz/seed_corpus/fuzz_lexer/error_bad_indent.ps": "negative fixture (copy of tests/fixtures)",
    "fuzz/seed_corpus/fuzz_lexer/error_missing_colon.ps": "negative fixture (copy of tests/fixtures)",
    "fuzz/seed_corpus/fuzz_lexer/error_multiple.ps": "negative fixture (copy of tests/fixtures)",
    "fuzz/seed_corpus/fuzz_parser/error_bad_indent.ps": "negative fixture (copy of tests/fixtures)",
    "fuzz/seed_corpus/fuzz_parser/error_missing_colon.ps": "negative fixture (copy of tests/fixtures)",
    "fuzz/seed_corpus/fuzz_parser/error_multiple.ps": "negative fixture (copy of tests/fixtures)",
    # Compressed (.psc-shaped) content under a .ps extension: starts with the
    # `W*` preset line. `pyths check tests/b029_worker.ps` rejects it too;
    # the B-029 test expands it first. Not canonical .ps.
    "tests/b029_worker.ps": "compressed content in .ps extension (W* preset; pyths check rejects)",
    # E7 testlet the AUTHORITATIVE PARSER ITSELF rejects (`pyths check`:
    # "Expected ), found =" — the `class C(metaclass=M)` keyword base).
    # The grammar failing on it is parser agreement, verified 2026-08-31.
    "tests/conformance/autotester/testlets/metaclasses.ps":
        "parser rejects it too (class keyword base `metaclass=M`; verified 2026-08-31)",
}

# E7 vendored-testlet prefix exclusion — RETIRED 2026-08-31. The grammar gap
# it acknowledged (lambda varargs, imaginary literals, byte strings,
# underscored numerics, extended slices, semicolons, matmul, for-iter
# testlists, return testlists, continuation blank-lines) was closed in
# grammar/pyths.lark (E7 sync); all parser-valid testlets now parse and are
# HARD-gated like the rest of the corpus. The one parser-REJECTED testlet
# (metaclasses.ps) moved to PS_EXCLUSIONS above with its honest reason.

# .psc files where BOTH psc.lark AND the expand-fallback are expected to
# fail. Empty today — psc.lark parses all 15 tracked .psc files DIRECTLY;
# the expand-fallback is not exercised by any current file.
PSC_EXCLUSIONS = {}

# Differential-corpus entry ids expected to fail. Empty — 580/580 parse.
DIFFERENTIAL_EXCLUSIONS = {}

# Pinned boundary witnesses (corpus 5 below): (label, source, must_accept).
# Every entry's verdict was measured against `pyths check --syntax-only`
# (2026-08-31, E7 grammar sync + r1 review round). PAIRED accept+reject per
# family — the reject side proves the gate can fail (anti-vacuity).
# NOTE: try_parse appends a trailing "\n", so a witness written without one
# exercises the "last line of the file" form of its class.
PINNED_WITNESSES = [
    # continuation `\` + blank/comment lines (r1 BLOCKER-1 + N2 class)
    ("cont-blank-then-content", "x = 1 + \\\n\n    2", True),
    ("cont-comment-then-content", "x = 1 + \\\n# c\n    2", True),
    ("cont-blank-to-eof", "x = 1\\\n", True),
    ("cont-bare-eof", "x = 1\\", True),
    ("cont-space-before-newline", "x = 1 + \\ \n    2", False),
    # imaginary literals
    ("imag-float", "x = 2.5j", True),
    ("imag-hex", "x = 0x1j", False),
    # byte strings
    ("bytes-basic", "x = b'a' b'b'", True),
    ("bytes-raw-prefix", "x = rb'a'", False),
    # underscore after base prefix
    ("hex-underscore", "x = 0x_ff_ff_ff", True),
    ("hex-double-underscore", "x = 0x__ff", False),
    # dot-hanging floats
    ("float-trailing-dot", "x = 2. + .5e3", True),
    ("float-dot-exp-only", "x = .e3", False),
    # lambda star params (free order)
    ("lambda-free-order", "f = lambda **kw, x, *a: x", True),
    ("lambda-bare-star", "f = lambda *, k: k", False),
    # subscript tuples + star slice bounds (r1 BLOCKER-2; r2 NIT-A)
    ("subscript-slice-tuple", "y = a[1:2:3, 4:5:6]", True),
    ("subscript-star-slice-bound", "y = a[:*b, 1]", True),
    ("subscript-star-item", "y = a[1:2, *b]", True),
    ("subscript-star-plain", "y = a[*b]", True),
    ("subscript-empty-item", "y = a[1:2,,3]", False),
    # semicolons
    ("semi-chain-compound", "x = 1; y = 2; if x: pass", True),
    ("semi-lone", ";", False),
    # matmul
    ("matmul", "m5 @= m4 @ m3", True),
    # for/return testlists; raise stays single-value (r1 SF-2)
    ("for-iter-tuple", "for x in a, *b,: pass", True),
    ("return-testlist", "def f(): return *a, 1,", True),
    ("raise-star", "raise *a from x", True),
    ("raise-testlist", "raise 1, 2", False),
]

# ---------------------------------------------------------------------------


def run_utf8(cmd, **kw):
    # Explicit UTF-8: on Windows, text=True decodes with the ANSI code page
    # (cp1252) and chokes on UTF-8 bytes in expanded .psc output.
    return subprocess.run(
        cmd, capture_output=True, encoding="utf-8", errors="replace", cwd=ROOT, **kw
    )


def git_ls(pattern):
    out = run_utf8(["git", "ls-files", pattern], check=True)
    return [line for line in out.stdout.splitlines() if line.strip()]


def build_parsers():
    try:
        import lark
        from lark.indenter import PythonIndenter
    except ImportError:
        print("FATAL: lark not installed — pip install lark", file=sys.stderr)
        sys.exit(2)

    def load(path):
        return lark.Lark.open(
            os.path.join(ROOT, path),
            parser="lalr",
            postlex=PythonIndenter(),
            start="file_input",
            maybe_placeholders=False,
        )

    return load("grammar/pyths.lark"), load("grammar/psc.lark")


def try_parse(parser, source):
    """Returns None on success, first-line error string on failure."""
    try:
        # .ps statements are newline-terminated; guarantee a trailing one.
        parser.parse(source + "\n")
        return None
    except Exception as e:  # lark raises several exception types
        return str(e).split("\n")[0][:200]


def ensure_binary():
    if os.path.exists(PYTHS):
        return True
    print(f"note: {PYTHS} missing — building (cargo build --release --workspace -j 4) ...")
    r = subprocess.run(
        ["cargo", "build", "--release", "--workspace", "-j", "4"], cwd=ROOT
    )
    return r.returncode == 0 and os.path.exists(PYTHS)


def expand(path):
    """pyths expand <path> -> (canonical_source | None, err)."""
    r = run_utf8([PYTHS, "expand", path])
    if r.returncode != 0:
        return None, (r.stderr or r.stdout).strip().split("\n")[0][:200]
    return r.stdout, None


def main():
    ps_parser, psc_parser = build_parsers()
    failures = []  # (corpus, name, error) — unexcluded only
    excluded_hits = []  # (corpus, name, reason)

    # ---- corpus 1: tracked .ps files ------------------------------------
    ps_files = git_ls("*.ps")
    ps_ok = 0
    for f in ps_files:
        src = open(os.path.join(ROOT, f), encoding="utf-8").read()
        err = try_parse(ps_parser, src)
        if err is None:
            if f in PS_EXCLUSIONS:
                # An exclusion that now parses is stale — flag it loudly.
                failures.append(("ps", f, "STALE EXCLUSION: file now parses; remove it"))
            else:
                ps_ok += 1
        elif f in PS_EXCLUSIONS:
            excluded_hits.append(("ps", f, PS_EXCLUSIONS[f]))
        else:
            failures.append(("ps", f, err))

    # ---- corpus 2: tracked .psc files -----------------------------------
    psc_files = git_ls("*.psc")
    psc_direct = psc_via_expand = 0
    have_bin = ensure_binary()
    for f in psc_files:
        src = open(os.path.join(ROOT, f), encoding="utf-8").read()
        err = try_parse(psc_parser, src)
        if err is None:
            psc_direct += 1
            continue
        # Safety-net fallback: expand to canonical .ps and parse the output
        # with pyths.lark. Currently unused — psc.lark covers every file.
        if not have_bin:
            failures.append(("psc", f, f"psc.lark: {err} (and pyths binary unavailable for fallback)"))
            continue
        expanded, xerr = expand(f)
        if expanded is None:
            failures.append(("psc", f, f"psc.lark: {err}; expand failed: {xerr}"))
            continue
        err2 = try_parse(ps_parser, expanded)
        if err2 is None:
            psc_via_expand += 1
        elif f in PSC_EXCLUSIONS:
            excluded_hits.append(("psc", f, PSC_EXCLUSIONS[f]))
        else:
            failures.append(("psc", f, f"psc.lark: {err}; post-expansion: {err2}"))

    # ---- corpus 3: CPython differential corpus --------------------------
    corpus_path = os.path.join(ROOT, "tests", "differential", "cpython_corpus.json")
    entries = json.load(open(corpus_path, encoding="utf-8"))
    diff_ok = 0
    for e in entries:
        setup = e.get("_setup", "")
        src = (setup + "\n" if setup else "") + e["expr"]
        err = try_parse(ps_parser, src)
        if err is None:
            diff_ok += 1
        elif e["id"] in DIFFERENTIAL_EXCLUSIONS:
            excluded_hits.append(("differential", e["id"], DIFFERENTIAL_EXCLUSIONS[e["id"]]))
        else:
            failures.append(("differential", e["id"], err))

    # ---- corpus 4: clone pairs (.ps + expanded .psc) ---------------------
    clones_dir = os.path.join(ROOT, "examples", "clones", "shared")
    clone_ok = clone_total = 0
    for d in sorted(os.listdir(clones_dir)):
        full = os.path.join(clones_dir, d)
        if not os.path.isdir(full):
            continue
        ps = [x for x in os.listdir(full) if x.endswith(".ps")]
        psc = [x for x in os.listdir(full) if x.endswith(".psc")]
        if not ps or not psc:
            continue
        clone_total += 1
        rel_ps = f"examples/clones/shared/{d}/{ps[0]}"
        rel_psc = f"examples/clones/shared/{d}/{psc[0]}"
        pair_ok = True
        err = try_parse(ps_parser, open(os.path.join(ROOT, rel_ps), encoding="utf-8").read())
        if err is not None:
            failures.append(("clones", rel_ps, err))
            pair_ok = False
        if have_bin:
            expanded, xerr = expand(rel_psc)
            if expanded is None:
                failures.append(("clones", rel_psc, f"expand failed: {xerr}"))
                pair_ok = False
            else:
                err2 = try_parse(ps_parser, expanded)
                if err2 is not None:
                    failures.append(("clones", rel_psc, f"post-expansion: {err2}"))
                    pair_ok = False
        else:
            failures.append(("clones", rel_psc, "pyths binary unavailable — cannot expand"))
            pair_ok = False
        if pair_ok:
            clone_ok += 1

    # ---- corpus 5: pinned boundary witnesses (paired negative controls) ---
    # Anti-vacuity discipline (CLAUDE-d'): every measured accept/reject
    # boundary from the 2026-08-31 E7 grammar sync ships a PINNED witness
    # driven through the SHIPPED grammar, paired accept+reject per family,
    # so a silent regression of the class goes RED here (the r1 review's
    # BLOCKER-1 — the continuation ignore eating the statement-final
    # newline at EOF — was invisible to the corpus gates precisely because
    # no tracked file ends in a continuation; these witnesses close that
    # blind spot). Each verdict below was measured against
    # `pyths check --syntax-only` on 2026-08-31; if the PARSER moves, the
    # witness must be re-measured, not deleted.
    pin_ok = 0
    for label, src, must_accept in PINNED_WITNESSES:
        err = try_parse(ps_parser, src)
        good = (err is None) if must_accept else (err is not None)
        if good:
            pin_ok += 1
        else:
            want = "accept" if must_accept else "reject"
            failures.append(("pinned", label,
                             f"grammar must {want} {src!r} (parser-verified "
                             f"boundary); got {err or 'accept'}"))

    # ---- summary ----------------------------------------------------------
    print()
    print("grammar corpus gate — summary")
    print(f"  .ps files        : {ps_ok}/{len(ps_files)} parsed "
          f"({len([x for x in excluded_hits if x[0] == 'ps'])} excluded)")
    print(f"  .psc files       : {psc_direct + psc_via_expand}/{len(psc_files)} "
          f"({psc_direct} via psc.lark, {psc_via_expand} via expand-fallback)")
    print(f"  differential     : {diff_ok}/{len(entries)} entries parsed")
    print(f"  clone pairs      : {clone_ok}/{clone_total} pairs (.ps + expanded .psc)")
    print(f"  pinned witnesses : {pin_ok}/{len(PINNED_WITNESSES)} boundary controls")
    if excluded_hits:
        print(f"\n  excluded ({len(excluded_hits)}):")
        for corpus, name, reason in excluded_hits:
            print(f"    [{corpus}] {name} — {reason}")
    if failures:
        print(f"\nFAILURES ({len(failures)} unexcluded):", file=sys.stderr)
        for corpus, name, err in failures:
            print(f"  [{corpus}] {name}\n      {err}", file=sys.stderr)
        sys.exit(1)
    print("\nOK — no unexcluded failures")


if __name__ == "__main__":
    main()
