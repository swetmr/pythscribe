#!/usr/bin/env python3
"""Single-command version bump -- the ONE place that enumerates EVERY version location, so a
release bump can't silently miss a file. (Motivated by the 0.2.1 release, where the 5
npm/@pythscribe/cli-* platform TEMPLATES were left at 0.2.0 and re-publishing 0.2.0 was
rejected by npm.)

The git TAG stays the release source of truth (npm/publish.mjs derives npm versions from it at
publish time); this script keeps the COMMITTED files consistent so `pyths --version` is right
and scripts/check_versions.py / CI passes. Targeted string replacement -- does NOT reformat.

    python scripts/set_version.py 0.2.5
    python scripts/set_version.py --restamp     # no bump: rewrite every package.json in STAMPED form at its current version

STAMPED FORM (spec 13-09-26 M6.1, requirements §5.4 "stamp idempotence, not stamping"): every package.json
this script touches is written EXACTLY as `npm/publish.mjs`'s `stampVersion` would emit it --
`JSON.stringify(j, null, 2) + "\\n"` (2-space indent, literal non-ASCII such as the em dash, trailing
newline; here `json.dumps(j, indent=2, ensure_ascii=False) + "\\n"`, byte-identical for these files). On a
tag ref publish.mjs runs in `--check` mode and REFUSES if stamping would change any byte, so the committed
bytes are what npm ships (checkout == prepared payload == npm tarball, §C4).
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# The full enumeration of package.json version locations -- shared with check_versions.py.
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

SEMVER = r"\d+\.\d+\.\d+(?:[-+][\w.]+)?"


def _read(rel: str) -> str:
    # newline="" -- no universal-newline translation: the stamped-form comparison is on the RAW bytes
    # (a CRLF checkout must be rewritten to LF, exactly what publish.mjs --check compares)
    with open(ROOT / rel, encoding="utf-8", newline="") as fh:
        return fh.read()


def _write(rel: str, s: str) -> None:
    (ROOT / rel).write_text(s, encoding="utf-8", newline="\n")


def set_pkg_version(rel: str, version: str) -> None:
    s = _read(rel)
    # the package's own `version` (first occurrence)
    s = re.sub(r'("version":\s*)"' + SEMVER + '"', r'\g<1>"' + version + '"', s, count=1)
    # any @pythscribe/cli-* dependency pins (exact-pinned)
    s = re.sub(r'("@pythscribe/cli-[\w-]+":\s*)"' + SEMVER + '"', r'\g<1>"' + version + '"', s)
    # intra-distribution JS-dep pins the wrapper carries (caret-pinned) -- keep in lockstep
    s = re.sub(
        r'("(?:pyths-runtime|vite-plugin-pyths|next-plugin-pyths)":\s*)"\^?' + SEMVER + '"',
        r'\g<1>"^' + version + '"',
        s,
    )
    _write(rel, stamped_form(s))


def stamped_form(text: str) -> str:
    """The exact bytes `stampVersion` (npm/publish.mjs) emits for this JSON: `JSON.stringify(j, null, 2) + "\\n"`."""
    return json.dumps(json.loads(text), indent=2, ensure_ascii=False) + "\n"


def restamp() -> int:
    """No bump: rewrite every enumerated package.json in stamped form at its CURRENT version."""
    changed = 0
    for rel in PKG_JSON:
        s = _read(rel)
        out = stamped_form(s)
        if out != s:
            _write(rel, out)
            changed += 1
    print(f"Restamped {changed} of {len(PKG_JSON)} package.json files into stamped form (no version change).")
    return 0


def set_cargo_version(version: str) -> None:
    s = re.sub(r'^version = "' + SEMVER + '"', f'version = "{version}"', _read("Cargo.toml"), count=1, flags=re.M)
    _write("Cargo.toml", s)


def set_lock_version(version: str) -> None:
    lines = _read("npm/pythscribe/package-lock.json").split("\n")
    n = 0
    for i in range(len(lines)):
        if n >= 2:
            break
        if re.match(r'^\s*"version": "' + SEMVER + r'",?\s*$', lines[i]):
            lines[i] = re.sub(r'"' + SEMVER + '"', f'"{version}"', lines[i], count=1)
            n += 1
    _write("npm/pythscribe/package-lock.json", "\n".join(lines))


def _replace_once(rel: str, pattern: str, replacement: str) -> None:
    s = _read(rel)
    out, count = re.subn(pattern, replacement, s, count=1, flags=re.M)
    if count == 0:
        raise SystemExit(f"set_version: no version match in {rel} (pattern {pattern!r})")
    _write(rel, out)


def set_py_package_version(version: str) -> None:
    """The pip package `pythscribe`: its distribution version (pyproject + __version__) AND its
    COMPILER pin (`_pin.COMPILER_VERSION`, which build_kernel matches against `pyths --version` --
    a stale pin silently degrades compile-on-first-call to the Python fallback). `[^"]+` (not
    SEMVER) so a PEP-440 pre-release like `0.2.5a0` is replaced whole. COMPILER_COMMIT stays a
    manual, per-release edit (this script cannot know the tagged commit)."""
    _replace_once("pyproject.toml", r'^version = "[^"]+"', f'version = "{version}"')
    _replace_once("pythscribe/__init__.py", r'__version__ = "[^"]+"', f'__version__ = "{version}"')
    _replace_once("pythscribe/_pin.py", r'COMPILER_VERSION = "[^"]+"', f'COMPILER_VERSION = "{version}"')


def main(argv: list[str]) -> int:
    if len(argv) == 2 and argv[1] == "--restamp":
        return restamp()
    if len(argv) != 2 or not re.fullmatch(SEMVER, argv[1]):
        print("Usage: python scripts/set_version.py <x.y.z> | --restamp", file=sys.stderr)
        return 1
    version = argv[1]
    for rel in PKG_JSON:
        set_pkg_version(rel, version)
    set_cargo_version(version)
    set_lock_version(version)
    set_py_package_version(version)
    print(f"Set version -> {version} across {len(PKG_JSON)} package.json + Cargo.toml + lockfile "
          "+ pyproject.toml + pythscribe __version__/_pin.")
    print("Reminder: bump pythscribe/_pin.py COMPILER_COMMIT to the tagged commit by hand.")
    print(f'Next: git commit -am "release {version}" && git tag v{version} && git push --follow-tags')
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
