#!/usr/bin/env python3
"""M6.3 -- the manifest-bound distribution set (spec 13-09-26, plan M6.3; validation §E2/§E4).

    python scripts/verify_dist_manifest.py --manifest release_manifest.json --dist dist [--rtp evidence/R-TP.json]

GREEN iff `<dist>` holds EXACTLY the manifest's distribution set (the 5 wheels + the sdist, by filename) and
every file hashes to the manifest's sha256. Run by BOTH publish-pypi.yml jobs right after downloading the
build run's `pythscribe-dist` artifact and BEFORE anything else touches it (no rebuild ever happens there).
With `--rtp`, the `pypi` job additionally requires R-TP's `uploaded_files` == the manifest's set, so the bytes
promoted to PyPI are the bytes TestPyPI validated (E2: mutate one wheel byte after R-TP -> RED; substitute the
sdist -> RED; a foreign file in dist/ -> RED).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts"))
from require_evidence import distribution_set  # noqa: E402


def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def verify(manifest: dict, dist: Path, rtp: dict | None = None) -> list[str]:
    """RED lines (empty == GREEN)."""
    p: list[str] = []
    want = distribution_set(manifest)
    if not dist.is_dir():
        return [f"{dist} is not a directory"]
    have = {q.name: q for q in dist.iterdir() if q.is_file()}
    for name in sorted(set(want) - set(have)):
        p.append(f"MISSING from {dist}: {name}")
    for name in sorted(set(have) - set(want)):
        p.append(f"FOREIGN file in {dist}: {name} (only the manifest's distribution set may be promoted)")
    for name in sorted(set(want) & set(have)):
        got = sha256_file(have[name])
        if got != want[name]:
            p.append(f"{name}: sha256 {got} != manifest {want[name]} (the bytes are not the manifest-bound build output)")
    if rtp is not None:
        if rtp.get("uploaded_files") != want:
            p.append("R-TP uploaded_files != the manifest's distribution set (the bytes validated on TestPyPI are not these bytes)")
        if rtp.get("manifest_sha256") != manifest.get("manifest_sha256"):
            p.append("R-TP is bound to another manifest")
    return p


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="verify_dist_manifest.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--manifest", default="release_manifest.json")
    ap.add_argument("--dist", default="dist")
    ap.add_argument("--rtp", default=None, help="evidence/R-TP.json: also require uploaded_files == the manifest's set (pypi job)")
    ns = ap.parse_args(argv)
    manifest = json.loads(Path(ns.manifest).read_text(encoding="utf-8"))
    rtp = json.loads(Path(ns.rtp).read_text(encoding="utf-8")) if ns.rtp else None
    problems = verify(manifest, Path(ns.dist), rtp)
    if problems:
        print(f"verify_dist_manifest: RED ({len(problems)}):", file=sys.stderr)
        for x in problems:
            print(f"  {x}", file=sys.stderr)
        return 1
    print(f"verify_dist_manifest: GREEN -- {len(distribution_set(manifest))} files in {ns.dist} == manifest {manifest.get('manifest_sha256', '')[:12]}" + (" == R-TP" if rtp else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main())
