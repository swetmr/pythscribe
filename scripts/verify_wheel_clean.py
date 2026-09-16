#!/usr/bin/env python3
"""Fail unless EVERY built wheel carries ONLY the pythscribe package + the vendored
gradio_wasmfunction (no monorepo leakage of crates/, npm/, examples/, tests/, ...), with the
bundled compiler + the licence file present in EACH (spec 13-09-26, M1.3 / validation §H2).

    python scripts/verify_wheel_clean.py [--dist dist] [--allow-binaryless]

Iterates ALL wheels (the pre-M1 script opened `whls[0]` only -- with 5 platform wheels a corrupt
3rd one was invisible). The allowlist is over TOP-LEVEL names: {pythscribe, gradio_wasmfunction,
*.dist-info}; `_bin/`, `_runtime/`, `_web/` are subdirectories INSIDE pythscribe/ and are RED as
top-level entries. `--allow-binaryless` is for the source-install policy check only (a
`py3-none-any` wheel from an sdist build); release wheels must carry exactly one
`pythscribe/_bin/pyths[.exe]`.
"""
from __future__ import annotations

import argparse
import sys
import zipfile
from pathlib import Path

ALLOWED_TOP_LEVEL = {"pythscribe", "gradio_wasmfunction"}
PACKAGE_SUBDIRS = ("_bin", "_runtime", "_web")
BINARY_NAMES = ("pythscribe/_bin/pyths", "pythscribe/_bin/pyths.exe")
REQUIRED_FILES = ("pythscribe/LICENSE", "pythscribe/__init__.py", "pythscribe/_launcher.py", "pythscribe/build/__init__.py")


def verify_wheel(path: Path, *, require_binary: bool = True) -> list[str]:
    """RED lines for ONE wheel (empty == GREEN)."""
    problems: list[str] = []
    try:
        with zipfile.ZipFile(path) as z:
            names = z.namelist()
    except (zipfile.BadZipFile, OSError) as e:
        return [f"{path.name}: not a readable wheel ({e})"]
    tops = sorted({n.split("/")[0] for n in names})
    bad = [t for t in tops if t not in ALLOWED_TOP_LEVEL and not t.endswith(".dist-info")]
    if bad:
        problems.append(f"{path.name}: unexpected top-level entries {bad} (whole tree: {tops})")
    for sub in PACKAGE_SUBDIRS:
        if sub in tops:
            problems.append(f"{path.name}: `{sub}/` is a TOP-LEVEL entry; it must live inside pythscribe/")
    bins = [n for n in names if n in BINARY_NAMES]
    if require_binary and len(bins) != 1:
        problems.append(f"{path.name}: expected exactly one bundled compiler at pythscribe/_bin/pyths[.exe], found {bins}")
    if not require_binary and bins:
        problems.append(f"{path.name}: a binary-less wheel must not carry a compiler, found {bins}")
    for req in REQUIRED_FILES:
        if req not in names:
            problems.append(f"{path.name}: missing {req}")
    if not any(n.endswith(".dist-info/licenses/pythscribe/LICENSE") or n.endswith(".dist-info/LICENSE") for n in names):
        problems.append(f"{path.name}: no licence file under .dist-info/")
    if not any(n.endswith(".dist-info/entry_points.txt") for n in names):
        problems.append(f"{path.name}: no entry_points.txt (the `pyths` console script is missing)")
    return problems


def verify_all(dist: Path, *, require_binary: bool = True) -> tuple[list[Path], list[str]]:
    whls = sorted(dist.glob("*.whl"))
    if not whls:
        return [], [f"no wheel in {dist}/"]
    problems: list[str] = []
    for w in whls:
        problems.extend(verify_wheel(w, require_binary=require_binary))
    return whls, problems


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--dist", default="dist")
    ap.add_argument("--allow-binaryless", action="store_true", help="source-install policy check: the wheels must carry NO compiler")
    ns = ap.parse_args(argv)
    whls, problems = verify_all(Path(ns.dist), require_binary=not ns.allow_binaryless)
    if problems:
        print(f"verify_wheel_clean: RED ({len(problems)} problem(s)) over {len(whls)} wheel(s):", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    print(f"verify_wheel_clean: OK -> {len(whls)} wheel(s) {[w.name for w in whls]}, top-level within {sorted(ALLOWED_TOP_LEVEL)} + *.dist-info")
    return 0


if __name__ == "__main__":
    sys.exit(main())
