#!/usr/bin/env python3
"""M1.3 -- the cibuildwheel REPAIR hook that never repairs (spec 13-09-26; validation §I4/§I5/§I6).

    CIBW_REPAIR_WHEEL_COMMAND_* = "... && python scripts/wheel_passthrough.py {wheel} {dest_dir}
                                   --binary <B_t> --tag <expected tag> [--require-auditwheel|--require-otool]"

The bundled compiler is the FINALIZED release binary B_t (requirements §5.3). `auditwheel repair`
/ `delocate-wheel` would rewrite it (relink, re-sign, graft) and break the npm<->pip byte identity,
so the hook is a PASSTHROUGH: the wheel is copied byte-for-byte into {dest_dir} -- but only after
every check below passes. Any violation exits non-zero and emits NOTHING, so cibuildwheel fails the
leg ("no wheel in dest_dir"). Checks, in order:
  1. the wheel's platform tag is in EXPECTED_WHEEL_SET and equals --tag (the leg's floor);
  2. exactly one `pythscribe/_bin/pyths[.exe]` inside, and sha256(it) == sha256(B_t) -- a repair or
     re-sign that touched the binary is RED with a CONTENT vs SIGNATURE-ONLY diagnostic (macOS,
     via `codesign --remove-signature` on both, when codesign is available);
  3. on POSIX tags the zip entry carries the executable bit (pip restores it at install);
  4. the REAL compatibility floor of the binary <= the tag (`pythscribe/build/_native.py`, the ONE
     parser): manylinux_2_28 -> max GLIBC_x.y <= 2.28, ELF e_machine == tag arch, DT_NEEDED within
     the allowlist; macosx_11_0 -> LC_BUILD_VERSION minos <= 11.0, cputype == tag arch, thin (not
     fat), dylibs under /usr/lib or /System; win_amd64 -> PE Machine == AMD64 + console subsystem
     (no OS-version floor is claimed, rev-7). Corroborated by the platform tool when present or
     required: `auditwheel show` (policy parsed; --require-auditwheel makes its absence RED) and
     `otool -l` (minos parsed; --require-otool likewise);
  5. copy, re-hash the copy, and assert {dest_dir} holds exactly this one wheel.
Importable: `verify(...)` returns the RED lines for the tests' synthesized fixtures.
"""
from __future__ import annotations

import argparse
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
from pythscribe.build._native import (  # noqa: E402
    EXPECTED_WHEEL_SET, NativeFormatError, check_floor, inspect_binary, tag_requirements,
)

BINARY_NAMES = ("pythscribe/_bin/pyths", "pythscribe/_bin/pyths.exe")
_WHEEL = re.compile(r"^(?P<name>[^-]+)-(?P<ver>[^-]+)-(?P<py>[^-]+)-(?P<abi>[^-]+)-(?P<plat>[^-]+)\.whl$")
_AUDITWHEEL_POLICY = re.compile(r'consistent with the following platform tag:\s*"?(manylinux_(\d+)_(\d+)_([a-z0-9_]+))"?')
_OTOOL_MINOS = re.compile(r"^\s*(?:minos|version)\s+(\d+)\.(\d+)(?:\.(\d+))?\s*$", re.M)


def sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def wheel_tag(path: Path) -> str:
    m = _WHEEL.match(path.name)
    if not m:
        raise ValueError(f"{path.name} is not a wheel filename")
    return m.group("plat")


def _signature_diagnostic(a: bytes, b: bytes) -> str:
    """macOS: is the diff signature-only (finalized too late: signing) or content? Needs codesign."""
    if shutil.which("codesign") is None:
        return "content-or-signature diff (codesign unavailable here to separate the two)"
    with tempfile.TemporaryDirectory() as td:
        pa, pb = Path(td) / "a", Path(td) / "b"
        pa.write_bytes(a)
        pb.write_bytes(b)
        for p in (pa, pb):
            subprocess.run(["codesign", "--remove-signature", str(p)], capture_output=True, check=False)
        if pa.read_bytes() == pb.read_bytes():
            return "SIGNATURE-ONLY diff: the binary was (re-)signed after finalization -- finalize (strip + codesign) BEFORE fan-out"
        return "CONTENT diff: the binary was rewritten after finalization (a repair step ran on it)"


def _tool_corroboration(tag: str, wheel: Path, binary: bytes, *, require_auditwheel: bool, require_otool: bool) -> list[str]:
    fmt, arch, floor = tag_requirements(tag)
    problems: list[str] = []
    if fmt == "elf":
        aw = shutil.which("auditwheel")
        if aw is None:
            if require_auditwheel:
                problems.append("--require-auditwheel: `auditwheel` is not on PATH (the Linux leg must corroborate the floor)")
            return problems
        p = subprocess.run([aw, "show", str(wheel)], capture_output=True, text=True, check=False)
        m = _AUDITWHEEL_POLICY.search(p.stdout + p.stderr)
        if p.returncode != 0 or not m:
            problems.append(f"`auditwheel show` did not report a platform tag (exit {p.returncode}): {(p.stderr or p.stdout).strip()[:400]}")
            return problems
        got, major, minor, aw_arch = m.group(1), int(m.group(2)), int(m.group(3)), m.group(4)
        assert floor is not None
        if (major, minor) > tuple(floor):
            problems.append(f"auditwheel: the binary is only consistent with {got} (glibc {major}.{minor}) but the tag promises {tag}")
        want_arch = {"x86_64": "x86_64", "aarch64": "aarch64"}[arch]
        if aw_arch != want_arch:
            problems.append(f"auditwheel: platform arch {aw_arch} != tag arch {want_arch}")
    elif fmt == "macho":
        ot = shutil.which("otool")
        if ot is None:
            if require_otool:
                problems.append("--require-otool: `otool` is not on PATH (the macOS leg must corroborate the floor)")
            return problems
        with tempfile.TemporaryDirectory() as td:
            b = Path(td) / "pyths"
            b.write_bytes(binary)
            p = subprocess.run([ot, "-l", str(b)], capture_output=True, text=True, check=False)
        vers = [(int(a), int(bb), int(c or 0)) for a, bb, c in _OTOOL_MINOS.findall(p.stdout)]
        if p.returncode != 0 or not vers:
            problems.append(f"`otool -l` reported no minos/version (exit {p.returncode})")
        else:
            assert floor is not None
            hi = max(vers)
            if hi > tuple(floor):
                problems.append(f"otool: minos {'.'.join(map(str, hi))} exceeds the {tag} floor")
    return problems


def verify(wheel: Path, expected_binary: Path | None, expected_tag: str | None, *, expected_sha256: str | None = None,
           require_auditwheel: bool = False, require_otool: bool = False) -> list[str]:
    """RED lines (empty == GREEN). Pure checks -- does not copy. The identity check needs B_t's BYTES
    (`expected_binary`, full signature-vs-content diagnostic) or at least its DIGEST
    (`expected_sha256`, the Linux container leg passes it through the environment); neither -> RED
    (the check is never allowed to be vacuous)."""
    problems: list[str] = []
    try:
        tag = wheel_tag(wheel)
    except ValueError as e:
        return [str(e)]
    if tag not in EXPECTED_WHEEL_SET:
        return [f"{wheel.name}: platform tag {tag!r} is not in the expected wheel set {sorted(EXPECTED_WHEEL_SET)}"]
    if expected_tag is not None and tag != expected_tag:
        return [f"{wheel.name}: tag {tag!r} != this leg's expected tag {expected_tag!r}"]
    try:
        with zipfile.ZipFile(wheel) as z:
            infos = [i for i in z.infolist() if i.filename in BINARY_NAMES]
            if len(infos) != 1:
                return [f"{wheel.name}: expected exactly one bundled compiler at pythscribe/_bin/pyths[.exe], found {[i.filename for i in infos]}"]
            zi = infos[0]
            data = z.read(zi)
    except (zipfile.BadZipFile, OSError) as e:
        return [f"{wheel.name}: not a readable wheel ({e})"]
    fmt, _arch, _floor = tag_requirements(tag)
    want_name = "pythscribe/_bin/pyths.exe" if fmt == "pe" else "pythscribe/_bin/pyths"
    if zi.filename != want_name:
        problems.append(f"{wheel.name}: the binary is named {zi.filename}, expected {want_name} for tag {tag}")
    # 2. identity with B_t
    bt: bytes | None = None
    if expected_binary is not None:
        try:
            bt = expected_binary.read_bytes()
        except OSError as e:
            problems.append(f"--binary {expected_binary}: unreadable ({e})")
    if bt is not None:
        if sha256_bytes(bt) != sha256_bytes(data):
            diag = _signature_diagnostic(bt, data) if fmt == "macho" else "CONTENT diff: the binary inside the wheel is not B_t (a repair step / re-sign rewrote it, or the wrong binary was staged)"
            problems.append(f"{wheel.name}: sha256(binary in wheel)={sha256_bytes(data)[:16]}... != sha256(B_t)={sha256_bytes(bt)[:16]}... ({len(data)} B vs {len(bt)} B) -- {diag}")
    elif expected_sha256:
        if sha256_bytes(data) != expected_sha256.lower():
            problems.append(f"{wheel.name}: sha256(binary in wheel)={sha256_bytes(data)[:16]}... != sha256(B_t)={expected_sha256[:16]}... -- CONTENT diff: the binary inside the wheel is not B_t (a repair step / re-sign rewrote it, or the wrong binary was staged)")
    elif expected_binary is None:
        problems.append("no --binary B_t (nor --binary-sha256) given: the identity check would be vacuous (refusing)")
    # 3. exec bit
    if fmt != "pe" and not ((zi.external_attr >> 16) & 0o111):
        problems.append(f"{wheel.name}: {zi.filename} has no executable bit in the zip entry (chmod +x before packaging; pip restores the mode at install)")
    # 4. the real floor
    try:
        info = inspect_binary(data)
    except NativeFormatError as e:
        problems.append(f"{wheel.name}: bundled file is not a native executable ({e})")
        return problems
    problems.extend(f"{wheel.name}: {p}" for p in check_floor(info, tag))
    problems.extend(f"{wheel.name}: {p}" for p in _tool_corroboration(tag, wheel, data, require_auditwheel=require_auditwheel, require_otool=require_otool))
    return problems


def passthrough(wheel: Path, dest_dir: Path) -> Path:
    """Byte-for-byte copy; the copy is re-hashed and dest_dir must then hold exactly this wheel."""
    dest_dir.mkdir(parents=True, exist_ok=True)
    out = dest_dir / wheel.name
    shutil.copyfile(wheel, out)
    if sha256_bytes(out.read_bytes()) != sha256_bytes(wheel.read_bytes()):
        out.unlink(missing_ok=True)
        raise SystemExit(f"wheel_passthrough: the copy's sha256 differs from the source (I/O corruption?)")
    others = [p.name for p in dest_dir.glob("*.whl") if p.name != wheel.name]
    if others:
        out.unlink(missing_ok=True)
        raise SystemExit(f"wheel_passthrough: {dest_dir} already holds other wheel(s) {others}; a leg emits exactly one")
    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("wheel")
    ap.add_argument("dest_dir")
    ap.add_argument("--binary", required=False, default=os.environ.get("PYTHSCRIBE_RELEASE_BINARY"), help="B_t, the finalized release binary (or env PYTHSCRIBE_RELEASE_BINARY); its bytes give the full signature-vs-content diagnostic")
    ap.add_argument("--binary-sha256", default=os.environ.get("PYTHSCRIBE_RELEASE_BINARY_SHA256"), help="sha256 of B_t (or env PYTHSCRIBE_RELEASE_BINARY_SHA256) -- the identity check when B_t's bytes are not reachable (Linux container leg)")
    ap.add_argument("--tag", default=os.environ.get("PYTHSCRIBE_WHEEL_PLATFORM"), help="the leg's expected platform tag (or env PYTHSCRIBE_WHEEL_PLATFORM)")
    ap.add_argument("--require-auditwheel", action="store_true")
    ap.add_argument("--require-otool", action="store_true")
    ns = ap.parse_args(argv)
    wheel = Path(ns.wheel)
    binary = Path(ns.binary) if ns.binary and Path(ns.binary).is_file() else None
    problems = verify(wheel, binary, ns.tag, expected_sha256=ns.binary_sha256, require_auditwheel=ns.require_auditwheel, require_otool=ns.require_otool)
    if problems:
        print(f"wheel_passthrough: RED ({len(problems)} problem(s)); NOT emitting {wheel.name}:", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    out = passthrough(wheel, Path(ns.dest_dir))
    with zipfile.ZipFile(out) as z:
        data = next(z.read(n) for n in z.namelist() if n in BINARY_NAMES)
    print(f"wheel_passthrough: GREEN -- {out.name} ({wheel_tag(out)}) emitted byte-for-byte to {ns.dest_dir}; "
          f"binary sha256 {sha256_bytes(data)} == B_t; floor: {inspect_binary(data).describe}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
