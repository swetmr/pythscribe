#!/usr/bin/env python3
"""Publish-time guard (B1): fail unless the git tag matches the pip package version.

A v* tag triggers publish-pypi.yml, which builds from pyproject.toml -- so a tag whose name
disagrees with the pyproject version would publish the WRONG version (e.g. tag v0.2.5 shipping a
leftover 0.2.5a0 pre-release, which `pip install pythscribe` then won't resolve). This asserts
tag == pyproject `project.version` == pythscribe `__version__`.

    python scripts/guard_tag_version.py "$GITHUB_REF_NAME"   # e.g. v0.2.5
"""
from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main(argv: list[str]) -> int:
    tag = (argv[1] if len(argv) > 1 else "").lstrip("v")
    if not tag:
        print("guard_tag_version: no tag argument (expected e.g. v0.2.5)", file=sys.stderr)
        return 1
    with open(ROOT / "pyproject.toml", "rb") as fh:
        pyproj = tomllib.load(fh)["project"]["version"]
    m = re.search(r'__version__ = "([^"]+)"', (ROOT / "pythscribe" / "__init__.py").read_text(encoding="utf-8"))
    dunder = m.group(1) if m else "(not found)"
    mp = re.search(r'COMPILER_VERSION = "([^"]+)"', (ROOT / "pythscribe" / "_pin.py").read_text(encoding="utf-8"))
    pin = mp.group(1) if mp else "(not found)"

    errs = []
    if tag != pyproj:
        errs.append(f"tag {tag!r} != pyproject version {pyproj!r}")
    if pyproj != dunder:
        errs.append(f"pyproject {pyproj!r} != pythscribe.__version__ {dunder!r}")
    # at a RELEASE tag set_version.py has unified everything -- a stale _pin means the shipped
    # runtime pins a compiler version the released `pyths` no longer reports (compile-on-first-call
    # then silently falls back). Guard it too (closes the hand-edited-pyproject-only-bump path).
    if pin != tag:
        errs.append(f"_pin.COMPILER_VERSION {pin!r} != tag {tag!r} (run scripts/set_version.py)")
    if errs:
        for e in errs:
            print("version guard FAILED:", e, file=sys.stderr)
        print(f"fix: python scripts/set_version.py {tag}", file=sys.stderr)
        return 1
    print(f"version guard OK: tag == pyproject == __version__ == {pyproj}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
