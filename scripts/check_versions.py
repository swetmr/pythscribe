#!/usr/bin/env python3
"""Version-drift guard (run in CI + before a release tag). Two invariants:

1. The COMPILER-tracked group all equals the Cargo workspace version (the canonical committed
   source): every package.json + its @pythscribe/cli-* pins + the lockfile + the pip package's
   `pythscribe/_pin.py COMPILER_VERSION`. A stale `_pin` means the shipped pip runtime pins a
   compiler version the released `pyths` no longer reports, so compile-on-first-call silently
   falls back -- the B1 blocker. Catches a manual bump that missed a file BEFORE a release run
   wastes CI on the resulting npm EPUBLISHCONFLICT (the 0.2.1 platform-template drift).

2. The pip package's own distribution version agrees with itself: `pyproject.toml` version ==
   `pythscribe/__init__.__version__`. These MAY lead the compiler version during development
   (e.g. a `0.2.5a0` pre-release while the compiler is still 0.2.4); `set_version.py` unifies
   everything at release, and the publish workflow gates `tag == pyproject version`.

    python scripts/check_versions.py
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PKG_JSON = [
    "runtime/package.json",
    "npm/pythscribe/package.json",
    "npm/@pythscribe/cli-win32-x64/package.json",
    "npm/@pythscribe/cli-linux-x64/package.json",
    "npm/@pythscribe/cli-linux-arm64/package.json",
    "npm/@pythscribe/cli-darwin-x64/package.json",
    "npm/@pythscribe/cli-darwin-arm64/package.json",
    "packages/create-pyths-app/package.json",
    "packages/next-plugin-pyths/package.json",
    "packages/vite-plugin-pyths/package.json",
]


def _read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


def _match1(rel: str, pattern: str) -> str:
    m = re.search(pattern, _read(rel), flags=re.M)
    return m.group(1) if m else "(not found)"


def main() -> int:
    cargo = re.search(r'^version = "([^"]+)"', _read("Cargo.toml"), flags=re.M)
    if not cargo:
        print("check_versions: could not find the workspace version in Cargo.toml", file=sys.stderr)
        return 1
    expected = cargo.group(1)

    # (1) the Cargo-tracked group
    seen: list[tuple[str, str]] = [("Cargo.toml", expected)]
    for rel in PKG_JSON:
        j = json.loads(_read(rel))
        seen.append((rel, j.get("version", "(missing)")))
        for k, v in (j.get("optionalDependencies") or {}).items():
            if k.startswith("@pythscribe/cli-"):
                seen.append((f"{rel} -> {k}", v))
        # the caret-pinned intra-distribution deps set_version.py keeps in lockstep (^expected)
        for section in ("dependencies", "devDependencies"):
            for k, v in (j.get(section) or {}).items():
                if k in ("pyths-runtime", "vite-plugin-pyths", "next-plugin-pyths"):
                    seen.append((f"{rel} -> {k}", v.lstrip("^")))
    lf = re.search(r'"version": "([^"]+)"', _read("npm/pythscribe/package-lock.json"))
    if lf:
        seen.append(("npm/pythscribe/package-lock.json", lf.group(1)))
    seen.append(("pythscribe/_pin.py COMPILER_VERSION", _match1("pythscribe/_pin.py", r'COMPILER_VERSION = "([^"]+)"')))

    drift = [(loc, v) for loc, v in seen if v != expected]
    if drift:
        print(f"Version DRIFT -- these disagree with Cargo.toml ({expected}):", file=sys.stderr)
        for loc, v in drift:
            print(f"  {loc} = {v}", file=sys.stderr)
        print(f"\nFix with:  python scripts/set_version.py {expected}", file=sys.stderr)
        return 1

    # (2) the pip package's own distribution version agrees with itself (may lead Cargo in dev)
    pyproj = _match1("pyproject.toml", r'^version = "([^"]+)"')
    dunder = _match1("pythscribe/__init__.py", r'__version__ = "([^"]+)"')
    if pyproj != dunder:
        print(f"pip-version drift: pyproject.toml ({pyproj}) != pythscribe/__init__.__version__ ({dunder})",
              file=sys.stderr)
        print(f"\nFix with:  python scripts/set_version.py {pyproj}", file=sys.stderr)
        return 1

    print(f"OK: {len(seen)} compiler-tracked locations == {expected}; "
          f"pip package version {pyproj} (pyproject == __version__).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
