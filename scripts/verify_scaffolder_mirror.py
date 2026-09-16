#!/usr/bin/env python3
"""M7 -- the scaffolder-mirror gate: RAW BYTES, ONE payload, NO normalization (spec 13-09-26, plan §M7.3).

    python scripts/verify_scaffolder_mirror.py [--payload dist/scaffolder-payload] [--vendored pythscribe/_web/create-pyths-app]

Sibling of `scripts/verify_runtime_mirror.py`: compares the wheel's vendored `create-pyths-app`
scaffolder `pythscribe/_web/create-pyths-app/` against the prepared payload `dist/scaffolder-payload/`
(from `scripts/prepare_release_payloads.py`). This is what makes `pyths new` produce a scaffold
BYTE-IDENTICAL to `npm create pyths-app@V` by construction (validation §M7-3): both come from the ONE
prepared payload. Three findings, each RED naming the file:
  * MISSING  -- in the payload, not vendored;
  * EXTRA    -- vendored, not in the payload;
  * CHANGED  -- both present, bytes differ (a one-byte edit to the vendored scaffolder is RED -- §M7-3).
There is NO LF / whitespace normalization: every side comes from the one prepared payload, so ANY
byte difference is real drift (a CRLF-only difference is RED). A membership sanity floor (the packed
tarball's own required members) keeps a payload that is itself wrong from making the gate vacuously
green. `packages/create-pyths-app/` <-> `pythscribe/_web/create-pyths-app/` is a declared byte-mirror
(orientation only; THIS script is the byte gate).
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from prepare_release_payloads import REPO, VENDORED_SCAFFOLDER, PayloadError  # noqa: E402

DEFAULT_PAYLOAD = REPO / "dist" / "scaffolder-payload"
# The packed tarball ALWAYS carries these (prepare_scaffolder_payload asserts it); requiring them on
# BOTH sides here means a truncated payload cannot silently pass the byte compare against a matching
# (equally truncated) vendored copy.
REQUIRED_MEMBERS = ("package.json", "index.js", "LICENSE")
# S10: the two skills copy-skills.cjs GENERATES into skills/. The skills/ subtree of BOTH sides must be
# EXACTLY these -- so a stale `skills/old.md` a prior prepack left behind (present on BOTH sides, thus
# invisible to the raw-byte compare) is caught, and a dropped generated skill is caught.
GENERATED_SKILLS = ("skills/compressing-ps-to-psc.md", "skills/pythscribe-language.md")


def tree_bytes(root: Path) -> dict[str, bytes]:
    if not root.is_dir():
        raise PayloadError(f"{root} is not a directory (run scripts/prepare_release_payloads.py first?)")
    return {p.relative_to(root).as_posix(): p.read_bytes() for p in sorted(root.rglob("*")) if p.is_file()}


def compare(payload: dict[str, bytes], vendored: dict[str, bytes]) -> list[str]:
    """RED lines (empty == GREEN). Raw bytes; no normalization of any kind."""
    problems: list[str] = []
    for rel in sorted(set(payload) - set(vendored)):
        problems.append(f"MISSING from the vendored copy: {rel}")
    for rel in sorted(set(vendored) - set(payload)):
        problems.append(f"EXTRA in the vendored copy (not in the payload): {rel}")
    for rel in sorted(set(payload) & set(vendored)):
        a, b = payload[rel], vendored[rel]
        if a != b:
            hint = ""
            if a.replace(b"\r\n", b"\n") == b.replace(b"\r\n", b"\n"):
                hint = " (line-ending-only difference -- CRLF is drift, not noise)"
            problems.append(f"CHANGED bytes: {rel}{hint} (payload {len(a)} B, vendored {len(b)} B)")
    return problems


def verify(payload_dir: Path, vendored_dir: Path) -> list[str]:
    payload = tree_bytes(payload_dir)
    vendored = tree_bytes(vendored_dir)
    problems: list[str] = []
    for what, tree in (("payload", payload), ("vendored copy", vendored)):
        missing = [m for m in REQUIRED_MEMBERS if m not in tree]
        if missing:
            problems.append(f"{what} is missing required scaffolder members {missing}")
        # S10: the skills/ subtree must be EXACTLY the generated skills (catches a stale skills/old.md
        # present on BOTH sides -- the raw-byte compare cannot see it -- and a dropped generated skill).
        skills = sorted(m for m in tree if m.startswith("skills/"))
        if skills != sorted(GENERATED_SKILLS):
            problems.append(f"{what} skills/ subtree is {skills}, not exactly {sorted(GENERATED_SKILLS)} "
                            "(a stale or missing generated skill; run `node scripts/copy-skills.cjs` / prepare_release_payloads --vendor)")
        # S10: LF enforcement -- a CRLF file baked on BOTH sides passes the byte compare but is drift
        # (the same discipline as the runtime preparer's check_lf_and_identity).
        for rel in sorted(tree):
            if b"\r" in tree[rel]:
                problems.append(f"{what}: {rel} is not LF (contains CR) -- the scaffolder payload must be LF (copy-skills normalizes CRLF->LF)")
    problems += compare(payload, vendored)
    return problems


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--payload", default=str(DEFAULT_PAYLOAD))
    ap.add_argument("--vendored", default=str(VENDORED_SCAFFOLDER))
    ns = ap.parse_args(argv)
    try:
        problems = verify(Path(ns.payload), Path(ns.vendored))
    except PayloadError as e:
        print(f"verify_scaffolder_mirror: RED\n{e}", file=sys.stderr)
        return 1
    if problems:
        print("verify_scaffolder_mirror: RED (raw-byte drift between the prepared payload and the vendored scaffolder)", file=sys.stderr)
        for line in problems:
            print(f"  {line}", file=sys.stderr)
        print("  fix: python scripts/prepare_release_payloads.py --vendor (never edit the vendored copy by hand)", file=sys.stderr)
        return 1
    n = sum(1 for p in Path(ns.vendored).rglob("*") if p.is_file())
    print(f"verify_scaffolder_mirror: GREEN -- {n} files byte-identical between {ns.payload} and {ns.vendored}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
