#!/usr/bin/env python3
"""M1.3 -- the COMPLETE-SET assertion over the built distributions (spec 13-09-26; validation §E4/§I1).

    python scripts/verify_wheel_set.py [--dist dist] [--no-sdist] [--expect tag,tag,...] [--version V]

GREEN iff `{platform tags of <dist>/pythscribe-*.whl} == EXPECTED_WHEEL_SET` (the 5 release targets,
ONE authority: pythscribe/build/_native.py, the same constant the manifest records) AND every wheel
is `py3-none-<tag>` at the same version (== pyproject's unless --version) AND exactly one sdist
`pythscribe-<version>.tar.gz` is present (unless --no-sdist). A MISSING target is RED (iterating the
wheels that happen to exist would never notice one); an EXTRA wheel -- in particular a binary-less
`py3-none-any` one from a source build -- is RED too (it must never be published as a wheel).
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
from pythscribe.build._native import EXPECTED_WHEEL_SET  # noqa: E402

_WHEEL = re.compile(r"^pythscribe-(?P<ver>[^-]+)-(?P<py>[^-]+)-(?P<abi>[^-]+)-(?P<plat>[^-]+)\.whl$")
_SDIST = re.compile(r"^pythscribe-(?P<ver>.+)\.tar\.gz$")


def pyproject_version(pyproject: Path = REPO / "pyproject.toml") -> str:
    for line in pyproject.read_text(encoding="utf-8").splitlines():
        m = re.match(r'^version\s*=\s*"([^"]+)"', line)
        if m:
            return m.group(1)
    raise SystemExit("verify_wheel_set: no `version = \"...\"` line in pyproject.toml")


def verify(dist: Path, expected: frozenset[str] = EXPECTED_WHEEL_SET, *, need_sdist: bool = True, version: str | None = None) -> list[str]:
    """RED lines (empty == GREEN)."""
    problems: list[str] = []
    if not dist.is_dir():
        return [f"{dist} is not a directory"]
    tags: dict[str, str] = {}
    versions: set[str] = set()
    for p in sorted(dist.glob("*.whl")):
        m = _WHEEL.match(p.name)
        if not m:
            problems.append(f"unparseable / foreign wheel name: {p.name}")
            continue
        if m.group("py") != "py3" or m.group("abi") != "none":
            problems.append(f"{p.name}: expected an interpreter-independent py3-none-<platform> tag")
        plat = m.group("plat")
        if plat in tags:
            problems.append(f"duplicate wheel for tag {plat}: {tags[plat]} and {p.name}")
        tags[plat] = p.name
        versions.add(m.group("ver"))
    for t in sorted(expected - set(tags)):
        problems.append(f"MISSING wheel for target {t}")
    for t in sorted(set(tags) - expected):
        problems.append(f"EXTRA wheel outside the expected set: {tags[t]} (tag {t})" + (" -- a binary-less source build must never be published as a wheel" if t == "any" else ""))
    sdists = [p for p in sorted(dist.glob("*.tar.gz")) if _SDIST.match(p.name)]
    if need_sdist:
        if len(sdists) != 1:
            problems.append(f"expected exactly one sdist pythscribe-<version>.tar.gz, found {[p.name for p in sdists]}")
        for p in sdists:
            versions.add(_SDIST.match(p.name).group("ver"))  # type: ignore[union-attr]
    if version is not None:
        for v in sorted(versions - {version}):
            problems.append(f"distribution at version {v}, expected {version}")
    elif len(versions) > 1:
        problems.append(f"mixed versions in {dist}: {sorted(versions)}")
    return problems


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--dist", default=str(REPO / "dist"))
    ap.add_argument("--no-sdist", action="store_true", help="do not require the sdist (wheel-only stages)")
    ap.add_argument("--expect", default=None, help="comma-separated tag set override (tests); default = EXPECTED_WHEEL_SET")
    ap.add_argument("--version", default=None, help="required distribution version (default: pyproject.toml's)")
    ap.add_argument("--print-expected", action="store_true", help="print the expected tag set (one per line) and exit")
    ns = ap.parse_args(argv)
    if ns.print_expected:
        print("\n".join(sorted(EXPECTED_WHEEL_SET)))
        return 0
    expected = frozenset(t.strip() for t in ns.expect.split(",") if t.strip()) if ns.expect else EXPECTED_WHEEL_SET
    version = ns.version or pyproject_version()
    problems = verify(Path(ns.dist), expected, need_sdist=not ns.no_sdist, version=version)
    if problems:
        print(f"verify_wheel_set: RED ({len(problems)} problem(s)) in {ns.dist}:", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    print(f"verify_wheel_set: GREEN -- {len(expected)} wheels {sorted(expected)}{'' if ns.no_sdist else ' + sdist'} at {version} in {ns.dist}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
