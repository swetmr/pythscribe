#!/usr/bin/env python3
"""coverage-gate.py -- change-impact coverage gate (v1: coarse whole-corpus boolean).

Greptile's core move imported into compiler testing: don't judge changed lines
in isolation -- know what tests them. Intersects the lines changed since the
merge-base with the differential-oracle coverage profile produced by
scripts/coverage-collect.py, and HARD-FAILS if any changed executable line in
a gated crate was executed by ZERO oracle-backed programs. That is exactly the
class that has shipped silent miscompiles (WASM nested-subscript scratch-local
clobber; B-064/B-065 family): changed emit path, zero exercising programs,
green gates.

Scoping doc: reference-app/scoping_coverage_gate.md. Local pre-merge checklist
slot: after `cargo test --workspace`, next to the planned pbt-gate.py.

Semantics (v1, deliberately coarse):
  - Coverage MUST be collected on the CANDIDATE tree (new lines don't exist
    in a base-tree profile). Run coverage-collect.py first, on this tree.
  - Changed lines = `git diff -U0 <merge-base(main,HEAD)> -- crates/`
    (working tree vs merge-base, so uncommitted candidate work is gated too),
    restricted to the gated crate set below, .rs files only.
  - Per changed line, from the lcov DA records:
      DA present, count > 0  -> COVERED  (executed under an oracle)
      DA present, count = 0  -> UNCOVERED -> hard FAIL unless waived
      no DA record           -> NOT-INSTRUMENTED -> warning only (comments,
                                blanks, uninstantiated generics, cfg'd-out
                                code; failing these would flood macro-heavy
                                Rust with false alarms)
  - Waivers: scripts/coverage-waivers.txt, KNOWN_DIVERGENCES-style reviewed
    ledger. The gap can shrink but never silently grow.

Known limitations (stated in the scoping doc, section 4): covered means
"executed while an oracle watched the output" -- NOT input-adequacy; the JS
runtime helpers are invisible to llvm-cov; line-diff is not semantic impact
tracing.

Usage
  python scripts/coverage-gate.py                       # gate HEAD+worktree
  python scripts/coverage-gate.py --diff-file d.patch   # gate a given diff
  python scripts/coverage-gate.py --base-ref origin/main
Exit codes: 0 pass, 1 UNCOVERED fail, 2 usage/infrastructure error.
"""

from __future__ import annotations

import argparse
import datetime
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_LCOV = REPO / "target" / "oracle-coverage.lcov"
DEFAULT_WAIVERS = REPO / "scripts" / "coverage-waivers.txt"

# Correctness-critical set (scoping doc section 1.4). Changes in excluded
# crates (cli, lsp, config, diagnostic, runtime-rust-side) don't ship
# miscompiles of user programs.
GATED_CRATES = {
    "pyths_codegen_js", "pyths_codegen_wasm", "pyths_expand", "pyths_hir",
    "pyths_types", "pyths_resolve", "pyths_parser", "pyths_lexer",
    "pyths_syntax", "pyths_print",
}

REMEDIATION = """\
Remediation contract (pick one):
  1. Add a corpus entry / targeted sweep that reaches this region
     (tests/differential/cpython_corpus.json, Livermore, S1/S2, or a
     pbt-ps sweep), re-run scripts/coverage-collect.py, re-gate; or
  2. Explicitly waive: add a line to scripts/coverage-waivers.txt
       crates/<crate>/src/<file>.rs:<start>[-<end>]  <justification>
     Waivers are a reviewed ledger -- the gap may shrink, never silently grow."""


def die(msg: str, code: int = 2) -> None:
    print(f"[coverage-gate] ERROR: {msg}", file=sys.stderr)
    sys.exit(code)


def git(*args: str) -> str:
    r = subprocess.run(["git", *args], cwd=REPO, capture_output=True,
                       text=True, encoding="utf-8", errors="replace")
    if r.returncode != 0:
        die(f"git {' '.join(args)} failed: {r.stderr.strip()}")
    return r.stdout


# --------------------------------------------------------------- diff parsing

def crate_of(path: str) -> str | None:
    m = re.match(r"crates/([^/]+)/", path)
    return m.group(1) if m else None


def parse_unified_diff(text: str) -> dict[str, set[int]]:
    """path (repo-relative, forward slashes) -> set of new-file line numbers
    that were added or modified."""
    changed: dict[str, set[int]] = {}
    cur: str | None = None
    new_line = 0
    for line in text.splitlines():
        if line.startswith("+++ "):
            p = line[4:].strip()
            if p.startswith("b/"):
                p = p[2:]
            cur = None if p == "/dev/null" else p.replace("\\", "/")
            continue
        if line.startswith("@@"):
            m = re.match(r"@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@", line)
            if not m:
                continue
            new_line = int(m.group(1))
            continue
        if cur is None:
            continue
        if line.startswith("+") and not line.startswith("+++"):
            changed.setdefault(cur, set()).add(new_line)
            new_line += 1
        elif line.startswith(" "):  # context (absent with -U0, tolerated)
            new_line += 1
        # '-' lines advance only the old file; nothing to gate (deleted code
        # has no candidate-tree line -- v1 semantics per the scoping doc).
    return changed


# --------------------------------------------------------------- lcov parsing

def norm_sf(path: str) -> str:
    """Normalize an lcov SF: path to repo-relative crates/... form."""
    p = path.replace("\\", "/")
    i = p.find("crates/")
    return p[i:] if i >= 0 else p


def parse_lcov(path: Path) -> dict[str, dict]:
    """norm path -> {'da': {line: count}, 'fn': [(start_line, name)]}"""
    files: dict[str, dict] = {}
    cur: dict | None = None
    with path.open(encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.rstrip("\n")
            if line.startswith("SF:"):
                cur = files.setdefault(norm_sf(line[3:]),
                                       {"da": {}, "fn": []})
            elif cur is not None and line.startswith("DA:"):
                parts = line[3:].split(",")
                try:
                    ln, cnt = int(parts[0]), int(parts[1])
                except (ValueError, IndexError):
                    continue
                # Merge duplicate records (templates/monomorphizations).
                cur["da"][ln] = max(cur["da"].get(ln, 0), cnt)
            elif cur is not None and line.startswith("FN:"):
                parts = line[3:].split(",", 1)
                try:
                    cur["fn"].append((int(parts[0]), parts[1]))
                except (ValueError, IndexError):
                    continue
            elif line == "end_of_record":
                cur = None
    for rec in files.values():
        rec["fn"].sort()
    return files


def demangle(name: str) -> str:
    """Best-effort readable form of a v0/legacy-mangled Rust symbol: pull the
    length-prefixed identifier segments and join the tail with '::'."""
    if not name.startswith(("_R", "_ZN")):
        return name
    s = re.sub(r"Cs[0-9A-Za-z]+?_", "", name)   # v0 crate-hash blocks
    s = re.sub(r"B[0-9A-Za-z]*_", "", s)        # v0 backrefs
    segs: list[str] = []
    i = 0
    while i < len(s):
        if s[i].isdigit():
            j = i
            while j < len(s) and s[j].isdigit():
                j += 1
            n = int(s[i:j])
            seg = s[j:j + n]
            if len(seg) == n and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", seg) \
                    and not re.fullmatch(r"h[0-9a-f]{16}", seg):
                segs.append(seg)
                i = j + n
                continue
            i = j
        else:
            i += 1
    return "::".join(segs[-3:]) if segs else name


def enclosing_fn(rec: dict, line: int) -> str:
    name = ""
    for start, fn_name in rec["fn"]:
        if start <= line:
            name = fn_name
        else:
            break
    return demangle(name)


# ------------------------------------------------------------------- waivers

def parse_waivers(path: Path) -> list[tuple[str, int, int, str]]:
    """Lines: crates/<...>.rs:<start>[-<end>]  justification"""
    waivers = []
    if not path.exists():
        return waivers
    for i, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = re.match(r"(\S+?):(\d+)(?:-(\d+))?\s+(.+)", line)
        if not m:
            print(f"[coverage-gate] WARNING: unparseable waiver at "
                  f"{path.name}:{i}: {line!r}")
            continue
        f, a, b, why = m.group(1).replace("\\", "/"), int(m.group(2)), \
            m.group(3), m.group(4)
        waivers.append((f, a, int(b) if b else a, why))
    return waivers


def waived(waivers, path: str, line: int) -> str | None:
    for f, a, b, why in waivers:
        if f == path and a <= line <= b:
            return why
    return None


# ----------------------------------------------------------------------- main

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--lcov", type=Path, default=DEFAULT_LCOV,
                    help="oracle-coverage lcov from coverage-collect.py")
    ap.add_argument("--diff-file", type=Path, default=None,
                    help="gate this unified diff instead of git "
                         "merge-base..worktree (testing / replay)")
    ap.add_argument("--base-ref", default="main",
                    help="merge-base partner for the diff (default: main)")
    ap.add_argument("--waivers", type=Path, default=DEFAULT_WAIVERS)
    args = ap.parse_args()

    if not args.lcov.exists():
        die(f"lcov not found: {args.lcov}\n  Collect on the CANDIDATE tree "
            f"first: python scripts/coverage-collect.py [--subset N]")
    age = datetime.datetime.now() - datetime.datetime.fromtimestamp(
        args.lcov.stat().st_mtime)
    print(f"[coverage-gate] lcov: {args.lcov} "
          f"(age {age.total_seconds() / 3600.0:.1f}h). Coverage must have "
          f"been collected on THIS candidate tree -- stale profiles judge "
          f"the wrong lines.")

    if args.diff_file:
        diff_text = args.diff_file.read_text(encoding="utf-8",
                                             errors="replace")
        print(f"[coverage-gate] diff source: {args.diff_file}")
    else:
        base = git("merge-base", args.base_ref, "HEAD").strip()
        diff_text = git("diff", "-U0", base, "--", "crates/")
        print(f"[coverage-gate] diff source: git diff -U0 {base[:12]} "
              f"(merge-base with {args.base_ref}) .. working tree, crates/")

    changed = {
        p: lines for p, lines in parse_unified_diff(diff_text).items()
        if p.endswith(".rs") and crate_of(p) in GATED_CRATES
    }
    n_lines = sum(len(v) for v in changed.values())
    if not changed:
        print("[coverage-gate] no changed lines in gated crates "
              f"({', '.join(sorted(GATED_CRATES))})")
        print("RESULT: PASS (vacuous -- nothing to gate)")
        return 0
    print(f"[coverage-gate] {n_lines} changed line(s) in "
          f"{len(changed)} gated file(s)")

    lcov = parse_lcov(args.lcov)
    waivers = parse_waivers(args.waivers)

    covered = uncovered = uninstrumented = waived_n = 0
    fail_msgs: list[str] = []
    warn_msgs: list[str] = []
    for path in sorted(changed):
        rec = lcov.get(path)
        crate = crate_of(path)
        if rec is None:
            warn_msgs.append(
                f"  NOT-INSTRUMENTED (whole file) {path} -- file absent from "
                f"the coverage profile; verify it is linked into the pyths "
                f"binary")
            uninstrumented += len(changed[path])
            continue
        for line in sorted(changed[path]):
            cnt = rec["da"].get(line)
            fn = enclosing_fn(rec, line)
            fn_s = f"  fn={fn}" if fn else ""
            if cnt is None:
                uninstrumented += 1
                warn_msgs.append(
                    f"  NOT-INSTRUMENTED {path}:{line}{fn_s} (comment/blank/"
                    f"generic-never-monomorphized/cfg'd-out)")
            elif cnt > 0:
                covered += 1
            else:
                why = waived(waivers, path, line)
                if why:
                    waived_n += 1
                    warn_msgs.append(
                        f"  WAIVED {path}:{line}{fn_s} -- {why}")
                else:
                    uncovered += 1
                    fail_msgs.append(
                        f"  UNCOVERED {path}:{line}  [{crate}]{fn_s} -- "
                        f"changed executable line, ZERO oracle-backed "
                        f"programs execute it")

    print(f"[coverage-gate] classification: {covered} covered, "
          f"{uncovered} UNCOVERED, {waived_n} waived, "
          f"{uninstrumented} not-instrumented (warn)")
    for m in warn_msgs:
        print(m)
    if uninstrumented and not uncovered:
        print("[coverage-gate] note: a merge on warnings alone is a "
              "conscious act -- uninstrumented lines are a real, bounded "
              "hole in the map.")
    if fail_msgs:
        print("\n".join(fail_msgs))
        print(f"\nRESULT: FAIL -- {uncovered} changed executable line(s) "
              f"with zero oracle-backed execution. This is the exact class "
              f"that shipped the WASM scratch-local-clobber silent "
              f"miscompile.")
        print(REMEDIATION)
        return 1
    print("RESULT: PASS -- every changed executable line in the gated "
          "crates is executed by at least one oracle-backed program.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
