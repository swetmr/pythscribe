#!/usr/bin/env python3
"""M0.3 -- the runtime-mirror gate: RAW BYTES, ONE payload, NO normalization (spec 13-09-26, plan §M0.3).

    python scripts/verify_runtime_mirror.py [--payload dist/runtime-payload] [--vendored pythscribe/_runtime/pyths-runtime]

Compares the wheel's vendored copy `pythscribe/_runtime/pyths-runtime/` against the prepared payload
`dist/runtime-payload/` (from `scripts/prepare_release_payloads.py`; in M6.2 also against the
published tarball). Three findings, each RED naming the file:
  * MISSING  -- in the payload, not vendored;
  * EXTRA    -- vendored, not in the payload;
  * CHANGED  -- both present, bytes differ.
There is NO LF / sourcemap / whitespace normalization: every side comes from the one prepared
payload, so ANY byte difference is real drift -- a CRLF-only difference is RED (validation §D1;
the rev-2 "normalization noise stays GREEN" control is deleted by design). Both sides are also
required to be EXACTLY E ∪ {LICENSE} (E = `runtime_package_files()`), so a payload that is itself
wrong cannot make the gate vacuously green.

Untouched, by design: the `crates/pyths_runtime/js/{runtime,operators}.js` re-export shims keep
their own linkage/parity invariant (they are NOT byte-compared here -- §D2); the compiler's
`include_str!` embedded-runtime invariant + `runtime_maps.rs` stay their own gates
(`scripts/verify_embedded_runtime.py` is §D4). `runtime/` <-> `pythscribe/_runtime/` is a
declared byte-mirror (orientation only; THIS script is the byte gate).
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from prepare_release_payloads import (  # noqa: E402
    LIB_RS,
    REPO,
    VENDORED_RUNTIME,
    PayloadError,
    expected_runtime_membership,
)

DEFAULT_PAYLOAD = REPO / "dist" / "runtime-payload"


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


def verify(payload_dir: Path, vendored_dir: Path, lib_rs: Path = LIB_RS) -> list[str]:
    expected = expected_runtime_membership(lib_rs)
    payload = tree_bytes(payload_dir)
    vendored = tree_bytes(vendored_dir)
    problems: list[str] = []
    for what, tree in (("payload", payload), ("vendored copy", vendored)):
        extra = sorted(set(tree) - expected)
        missing = sorted(expected - set(tree))
        if extra:
            problems.append(f"{what} has files outside runtime_package_files() + LICENSE: {extra}")
        if missing:
            problems.append(f"{what} lacks runtime_package_files() + LICENSE members: {missing}")
    problems += compare(payload, vendored)
    return problems


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--payload", default=str(DEFAULT_PAYLOAD))
    ap.add_argument("--vendored", default=str(VENDORED_RUNTIME))
    ns = ap.parse_args(argv)
    try:
        problems = verify(Path(ns.payload), Path(ns.vendored))
    except PayloadError as e:
        print(f"verify_runtime_mirror: RED\n{e}", file=sys.stderr)
        return 1
    if problems:
        print("verify_runtime_mirror: RED (raw-byte drift between the prepared payload and the vendored copy)", file=sys.stderr)
        for line in problems:
            print(f"  {line}", file=sys.stderr)
        print("  fix: python scripts/prepare_release_payloads.py --vendor (never edit the vendored copy by hand)", file=sys.stderr)
        return 1
    n = sum(1 for p in Path(ns.vendored).rglob("*") if p.is_file())
    print(f"verify_runtime_mirror: GREEN -- {n} files byte-identical between {ns.payload} and {ns.vendored}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
