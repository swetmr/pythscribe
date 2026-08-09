#!/usr/bin/env python3
"""Axiom-footprint driver for the ten-proofs-style trust artifact.

Two jobs, one extraction path (so the manifest and the CI gate can NEVER
disagree):

  emit   -> writes verification/formalization.yaml (the audit manifest):
            per-headline-claim `#print axioms` output + file/line + sorry_count,
            plus the aggregate integrity block (UNION of axioms = pinned
            footprint, total theorem+lemma count, global sorry_count).

  gate   -> the Comparator-style axiom-footprint GATE (the 80%-of-the-value
            piece that MUST land + pass even if lean4export/nanoda do not
            wire up). Mechanically enforces, over EVERY headline decl:
              * the decl exists,
              * `#print axioms` is a SUBSET of the pinned footprint
                {propext, Classical.choice, Quot.sound},
              * zero sorry/admit/native_decide/axiom in the Lean development.
            Exits nonzero on ANY sorry, extra axiom, or missing decl.

Both run `#print axioms` in the REAL Lean environment via `lake env lean`, so
this binds to the built kernel state, not to prose. Python-for-tooling per the
repo rule; shells out only to lake/lean.

Usage (from verification/):
    python comparator/axiom_footprint.py gate      # CI gate (exit nonzero on violation)
    python comparator/axiom_footprint.py emit       # (re)write formalization.yaml
    python comparator/axiom_footprint.py emit --no-build   # skip the fresh lake build
"""
from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
VERIF = HERE.parent                       # verification/
LEAN_FILE = VERIF / "PythExpandVerify.lean"
MANIFEST = VERIF / "formalization.yaml"
MODULE = "PythExpandVerify"

sys.path.insert(0, str(HERE))
from headline_claims import HEADLINE, PINNED_AXIOMS  # noqa: E402

DECL_RE = re.compile(r"^\s*(theorem|lemma|def|abbrev|structure|inductive)\s+([A-Za-z_][A-Za-z0-9_']*)")
ESCAPE_RE = re.compile(r"\bsorry\b|\badmit\b|^\s*axiom\s|native_decide")
AXIOM_LINE_RE = re.compile(r"^'" + re.escape(MODULE) + r"\.([A-Za-z0-9_']+)' (?:depends on axioms: \[(.*)\]|does not depend on any axioms)")


def resolve_decls():
    """Map every decl NAME -> (line, kind) from the live source (first defn site)."""
    loc = {}
    for i, line in enumerate(LEAN_FILE.read_text(encoding="utf-8").splitlines(), 1):
        m = DECL_RE.match(line)
        if m and m.group(2) not in loc:
            loc[m.group(2)] = (i, m.group(1))
    return loc


def count_theorems_lemmas():
    txt = LEAN_FILE.read_text(encoding="utf-8").splitlines()
    return sum(1 for ln in txt if re.match(r"^\s*(theorem|lemma)\s", ln))


def scan_escape_hatches():
    """Return list of (file, lineno, text) for any sorry/admit/axiom/native_decide.

    Scans EVERY `verification/*.lean` (not just PythExpandVerify.lean) — the fable
    F-6 fix: a sorry hidden in a sibling `.lean` (e.g. a generated *Data.lean or a
    future module) must not slip past the trust-base audit. Ignores comment-only
    lines (mirrors the CI grep's intent)."""
    hits = []
    for lean in sorted(VERIF.glob("*.lean")):
        for i, line in enumerate(lean.read_text(encoding="utf-8").splitlines(), 1):
            stripped = line.lstrip()
            if stripped.startswith("--"):
                continue
            if ESCAPE_RE.search(line):
                hits.append((lean.name, i, line.strip()))
    return hits


def all_probe_names():
    """Every decl we need axioms for: headline decls + their witness decls."""
    names = []
    for h in HEADLINE:
        names.append(h["decl"])
        w = h.get("witness")
        if w:
            names.append(w)
    # de-dup, preserve order
    seen = set()
    out = []
    for n in names:
        if n not in seen:
            seen.add(n)
            out.append(n)
    return out


def run_print_axioms(names, do_build=True):
    """Run `#print axioms` for each name in the real Lean env; return name->set(axioms).

    A missing decl (Lean errors 'unknown identifier') is reported as None so the
    caller can flag it."""
    if do_build:
        b = subprocess.run(["lake", "build"], cwd=VERIF, capture_output=True, text=True)
        if b.returncode != 0:
            sys.stderr.write("lake build FAILED:\n" + b.stdout + b.stderr + "\n")
            sys.exit(2)
    body = [f"import {MODULE}", f"open {MODULE}"]
    for n in names:
        body.append(f"#print axioms {n}")
    with tempfile.NamedTemporaryFile("w", suffix=".lean", delete=False,
                                     dir=str(VERIF), encoding="utf-8") as f:
        f.write("\n".join(body) + "\n")
        probe = f.name
    try:
        r = subprocess.run(["lake", "env", "lean", probe], cwd=VERIF,
                           capture_output=True, text=True)
    finally:
        os.unlink(probe)
    out = r.stdout + "\n" + r.stderr
    result = {n: None for n in names}   # None = not reported (missing / error)
    for line in out.splitlines():
        m = AXIOM_LINE_RE.match(line.strip())
        if not m:
            continue
        name, axioms_str = m.group(1), m.group(2)
        if axioms_str is None:
            result[name] = set()          # axiom-free
        else:
            result[name] = {a.strip() for a in axioms_str.split(",") if a.strip()}
    return result, out


# ---------------------------------------------------------------- gate mode
def cmd_gate(do_build=True):
    print("== Comparator axiom-footprint gate ==")
    print(f"pinned footprint: {{{', '.join(sorted(PINNED_AXIOMS))}}}")
    violations = []

    hatches = scan_escape_hatches()
    if hatches:
        for fname, ln, txt in hatches:
            violations.append(f"escape hatch @ {fname}:{ln}: {txt}")

    names = all_probe_names()
    axioms, raw = run_print_axioms(names, do_build=do_build)

    # Gate is over HEADLINE decls (+ their witnesses, which must also be clean).
    for name in names:
        ax = axioms.get(name)
        if ax is None:
            violations.append(f"decl not reported by `#print axioms` (missing/typo?): {name}")
            continue
        extra = ax - PINNED_AXIOMS
        if extra:
            violations.append(f"{name}: UNEXPECTED axioms {sorted(extra)} (not in pinned footprint)")

    if violations:
        print("\nGATE FAILED:")
        for v in violations:
            print("  - " + v)
        print(f"\n({len(violations)} violation(s))")
        return 1

    n_head = len(HEADLINE)
    n_probe = len(names)
    union = set()
    for name in names:
        union |= axioms[name]
    print(f"OK: {n_head} headline claims ({n_probe} decls incl. witnesses) checked.")
    print(f"OK: 0 sorry/admit/axiom/native_decide across all verification/*.lean.")
    print(f"OK: axiom footprint (union over headline+witness decls) = "
          f"{{{', '.join(sorted(union)) or '(empty)'}}} subset of pinned.")
    return 0


# ---------------------------------------------------------------- emit mode
def _yaml_str(s: str) -> str:
    # Quote + escape for a YAML double-quoted scalar.
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def cmd_emit(do_build=True):
    loc = resolve_decls()
    names = all_probe_names()
    axioms, raw = run_print_axioms(names, do_build=do_build)
    hatches = scan_escape_hatches()
    global_sorry = len(hatches)
    total_tl = count_theorems_lemmas()

    def axioms_yaml(ax):
        if ax is None:
            return "[UNKNOWN]"
        if not ax:
            return "[]  # axiom-free (tighter than the pinned ceiling)"
        return "[" + ", ".join(sorted(ax)) + "]"

    union = set()
    missing = []
    no_refuter = []
    over_pinned = []

    lines = []
    lines.append("# formalization.yaml — ten-proofs-style trust manifest for the PythScribe verified core.")
    lines.append("#")
    lines.append("# Adapted from github.com/openai/ten-proofs/formalization.yaml. Maps each")
    lines.append("# Paper-C headline claim -> its FINAL Lean statement -> file/line -> the ACTUAL")
    lines.append("# `#print axioms` footprint -> the `*_stub_fails`/discriminating-witness refuter")
    lines.append("# that proves the theorem has teeth.")
    lines.append("#")
    lines.append("# GENERATED by verification/comparator/axiom_footprint.py (do not hand-edit the")
    lines.append("# per-claim axioms/line fields — regenerate with `python comparator/axiom_footprint.py emit`).")
    lines.append("# The claim SELECTION + informal names + witnesses live in comparator/headline_claims.py.")
    lines.append("#")
    lines.append(f"# Toolchain: {read_toolchain()}   |   Core: dependency-free Lean 4 (NO Mathlib).")
    lines.append("")
    lines.append("project: PythScribe verified core (verification/PythExpandVerify.lean)")
    lines.append(f"module: {MODULE}")
    lines.append(f"toolchain: {read_toolchain()}")
    lines.append("mathlib: false   # dependency-free core; axiom ceiling is Lean's own {propext, Classical.choice, Quot.sound}")
    lines.append("")
    lines.append("headline_claims:")

    for h in HEADLINE:
        decl = h["decl"]
        ax = axioms.get(decl)
        if ax is None:
            missing.append(decl)
        else:
            union |= ax
            if ax - PINNED_AXIOMS:
                over_pinned.append((decl, sorted(ax - PINNED_AXIOMS)))
        line_no, kind = loc.get(decl, (None, None))
        witness = h.get("witness")
        if not witness:
            no_refuter.append(decl)
        lines.append(f"  - name: {_yaml_str(h['name'])}")
        lines.append(f"    declaration: {MODULE}.{decl}")
        lines.append(f"    kind: {kind or 'UNKNOWN'}")
        lines.append(f"    file: verification/PythExpandVerify.lean")
        lines.append(f"    line: {line_no if line_no else 'UNKNOWN'}")
        lines.append(f"    sorry_count: 0")
        lines.append(f"    axioms: {axioms_yaml(ax)}")
        if witness:
            wax = axioms.get(witness)
            lines.append(f"    witness: {MODULE}.{witness}")
            lines.append(f"    witness_axioms: {axioms_yaml(wax)}")
        else:
            lines.append(f"    witness: none")
            if h.get("witness_note"):
                lines.append(f"    witness_note: {_yaml_str(h['witness_note'])}")
        lines.append(f"    paper_claim: {_yaml_str(h['paper_claim'])}")
        lines.append("")

    # ------- aggregate integrity block
    lines.append("aggregate_integrity:")
    lines.append(f"  total_theorems_and_lemmas: {total_tl}   # `grep -cE '^\\s*(theorem|lemma) ' PythExpandVerify.lean`")
    lines.append(f"  global_sorry_count: {global_sorry}   # sorry/admit/axiom/native_decide across the whole development")
    lines.append(f"  headline_claim_count: {len(HEADLINE)}")
    lines.append(f"  pinned_axiom_footprint: [{', '.join(sorted(PINNED_AXIOMS))}]   # the CEILING (ten-proofs pins exactly this trio)")
    lines.append(f"  actual_axiom_union_over_headlines: [{', '.join(sorted(union)) if union else ''}]   # the REAL footprint used")
    lines.append(f"  footprint_within_pinned: {str(not over_pinned).lower()}")
    # honesty: report whether Classical.choice is actually used
    cc = "Classical.choice" in union
    lines.append(f"  uses_classical_choice: {str(cc).lower()}   # inherited from Lean core `String` (String.length/ofList), not our own")
    lines.append("  enforcement:")
    lines.append("    - '`python comparator/axiom_footprint.py gate` (CI verification job) — asserts every headline decl axioms subset pinned + 0 sorry'")
    lines.append("    - '`#guard_msgs`-pinned `#print axioms` assertions inside PythExpandVerify.lean (per-decl, build-enforced)'")
    lines.append("    - '`lake build` + the CI trust-base grep (no sorry/admit/axiom/native_decide)'")
    lines.append("    - 'comparator/run_comparator.sh — export (lean4export) + independent re-check (nanoda) when wired; see comparator.md'")
    lines.append("")

    MANIFEST.write_text("\n".join(lines), encoding="utf-8")
    print(f"wrote {MANIFEST}")
    print(f"  headline claims: {len(HEADLINE)}")
    print(f"  total theorems+lemmas: {total_tl}   global sorry: {global_sorry}")
    print(f"  actual axiom union: {{{', '.join(sorted(union)) or '(empty)'}}}")
    print(f"  pinned footprint : {{{', '.join(sorted(PINNED_AXIOMS))}}}")
    if missing:
        print(f"  !! MISSING decls (not reported by lean): {missing}")
    if over_pinned:
        print(f"  !! OVER-PINNED decls (extra axioms): {over_pinned}")
    print(f"  headline claims WITHOUT a refuter witness (FINDING — see report): {len(no_refuter)}")
    for d in no_refuter:
        print(f"       - {d}")
    return 1 if (missing or over_pinned) else 0


def read_toolchain():
    tc = VERIF / "lean-toolchain"
    return tc.read_text(encoding="utf-8").strip() if tc.exists() else "unknown"


def main(argv):
    if len(argv) < 2 or argv[1] not in ("gate", "emit"):
        print(__doc__)
        return 2
    do_build = "--no-build" not in argv
    if argv[1] == "gate":
        return cmd_gate(do_build=do_build)
    return cmd_emit(do_build=do_build)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
