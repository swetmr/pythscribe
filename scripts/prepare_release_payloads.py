#!/usr/bin/env python3
"""M0.1 -- the ONE prepared release payload (spec 13-09-26-lib-selfcontained-pip-wheel, plan §M0.1).

    python scripts/prepare_release_payloads.py [--out dist] [--vendor]

Runs `npm pack` over `runtime/` (-> `dist/runtime-payload/` + `dist/runtime-payload.tgz`) and over
`packages/create-pyths-app/` (-> `dist/scaffolder-payload/` + `dist/scaffolder-payload.tgz`), on the
checkout of the immutable source SHA, BEFORE the compiler build, the wheel build or the npm publish.
Everything downstream consumes THIS output -- `npm/publish.mjs` publishes the prepared `.tgz`, the
wheel vendors `dist/runtime-payload/` (`--vendor` copies it to `pythscribe/_runtime/pyths-runtime/`),
and `scripts/verify_runtime_mirror.py` compares the vendored copy against it raw-byte.

Gates (each RED names the offending file; exit 1):
  * tarball membership == E ∪ {LICENSE} EXACTLY, where E = the paths `runtime_package_files()`
    (`crates/pyths_runtime/src/lib.rs`, the 70-file authority incl. all 34 `.js.map`) embeds.
    An extra file (a `src/x.js` not registered in `runtime_package_files()`, a `.test.mjs` leaking
    into the tarball) or a missing one (a `files` entry dropped from package.json) is RED -- the
    "dropped entrypoint" guard (validation §C2).
  * every payload file is LF (§C5) and byte-identical to the checkout (NO normalization anywhere:
    the checkout, the payload, the wheel-vendored copy and the published tarball are the same bytes).
Emits `dist/runtime-payload.files.json` (`{path: sha256}`) for the release manifest (M6).

This is a RELEASE-MACHINE step (it needs `npm`); a user's machine never runs it.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
LIB_RS = REPO / "crates" / "pyths_runtime" / "src" / "lib.rs"
RUNTIME_DIR = REPO / "runtime"
SCAFFOLDER_DIR = REPO / "packages" / "create-pyths-app"
VENDORED_RUNTIME = REPO / "pythscribe" / "_runtime" / "pyths-runtime"
# M7 (spec 13-09-26): the prepared `create-pyths-app` scaffolder payload vendored into the wheel at
# pythscribe/_web/create-pyths-app/ (mirror-gated by scripts/verify_scaffolder_mirror.py; resolved by
# pythscribe._web.find_scaffolder). Same discipline as the runtime: the ONE prepared payload is the
# source of truth for the vendored copy AND the npm-published `create-pyths-app@V`.
VENDORED_SCAFFOLDER = REPO / "pythscribe" / "_web" / "create-pyths-app"
NPM_ONLY = frozenset({"LICENSE"})  # N: tarball files outside E -- exactly {LICENSE} (requirements §5.4)

# `("<path>", include_str!("../../../runtime/<path>"))` pairs -- the tuple's first element is the
# package-root-relative path `materialize_runtime_package` writes; the include path must agree.
_EMBED = re.compile(
    r'\(\s*"(?P<path>[^"]+)"\s*,\s*include_str!\(\s*"\.\./\.\./\.\./runtime/(?P<inc>[^"]+)"\s*\)\s*,?\s*\)',
    re.MULTILINE,
)


class PayloadError(RuntimeError):
    pass


def embedded_membership(lib_rs: Path = LIB_RS) -> list[str]:
    """E = the paths `runtime_package_files()` embeds, read from the Rust authority itself (no
    cargo build needed). Sanity: paths == include paths, unique, every `.js` has its `.js.map`."""
    text = lib_rs.read_text(encoding="utf-8")
    start = text.find("pub fn runtime_package_files()")
    if start < 0:
        raise PayloadError(f"{lib_rs}: runtime_package_files() not found")
    body = text[start:]
    end = body.find("\n}\n")
    body = body[: end if end > 0 else len(body)]
    paths: list[str] = []
    for m in _EMBED.finditer(body):
        if m.group("path") != m.group("inc"):
            raise PayloadError(f"{lib_rs}: embedded path {m.group('path')!r} != include path {m.group('inc')!r}")
        paths.append(m.group("path"))
    if len(set(paths)) != len(paths):
        dup = sorted({p for p in paths if paths.count(p) > 1})
        raise PayloadError(f"{lib_rs}: duplicate embedded paths {dup}")
    if len(paths) < 60:
        raise PayloadError(f"{lib_rs}: parse looks broken (only {len(paths)} embedded paths found)")
    for p in paths:
        if p.endswith(".js") and f"{p}.map" not in paths:
            raise PayloadError(f"{lib_rs}: {p} has no sibling {p}.map in runtime_package_files()")
    return sorted(paths)


def expected_runtime_membership(lib_rs: Path = LIB_RS) -> set[str]:
    return set(embedded_membership(lib_rs)) | set(NPM_ONLY)


def sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def _npm() -> str:
    npm = shutil.which("npm") or shutil.which("npm.cmd")
    if not npm:
        raise PayloadError("npm not found on PATH (this is a release-machine step)")
    return npm


def npm_pack(pkg_dir: Path, dest: Path) -> Path:
    """`npm pack` of `pkg_dir` into `dest`; returns the tarball path."""
    # Start from a clean dest so a re-run is idempotent: `npm pack` names the tarball by
    # package+version, so a stale same-version .tgz from a prior (aborted) run would be
    # OVERWRITTEN in place and the `before/new` diff would then see 0 new tarballs (a false
    # "produced 0 tarballs" RED on every re-run). Wipe it first.
    shutil.rmtree(dest, ignore_errors=True)
    dest.mkdir(parents=True, exist_ok=True)
    before = set(dest.glob("*.tgz"))
    proc = subprocess.run(
        [_npm(), "pack", "--pack-destination", str(dest)],
        cwd=str(pkg_dir), capture_output=True, text=True, check=False, shell=(os.name == "nt"),
    )
    if proc.returncode != 0:
        raise PayloadError(f"npm pack failed in {pkg_dir}:\n{proc.stderr.strip() or proc.stdout.strip()}")
    new = sorted(set(dest.glob("*.tgz")) - before, key=lambda p: p.stat().st_mtime_ns)
    if len(new) != 1:
        raise PayloadError(f"npm pack in {pkg_dir} produced {len(new)} tarballs in {dest} (expected 1)")
    return new[0]


def tarball_members(tgz: Path) -> dict[str, bytes]:
    """`{package-root-relative posix path: bytes}` of every regular file in the tarball. Members
    are under the `package/` prefix npm uses; anything else (or a path escaping it) is refused."""
    out: dict[str, bytes] = {}
    with tarfile.open(tgz, "r:gz") as tf:
        for m in tf.getmembers():
            if m.isdir():
                continue
            if not m.isfile():
                raise PayloadError(f"{tgz.name}: non-regular member {m.name!r} ({m.type!r})")
            name = m.name
            if not name.startswith("package/"):
                raise PayloadError(f"{tgz.name}: member {name!r} is not under package/")
            rel = name[len("package/"):]
            parts = rel.split("/")
            if not rel or any(p in ("", ".", "..") for p in parts):
                raise PayloadError(f"{tgz.name}: refusing member path {name!r}")
            f = tf.extractfile(m)
            assert f is not None
            out[rel] = f.read()
    return out


def extract_to(members: dict[str, bytes], dest: Path) -> None:
    if dest.exists():
        shutil.rmtree(dest)
    for rel, data in sorted(members.items()):
        p = dest / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)


def check_membership(members: set[str], expected: set[str], what: str) -> None:
    extra = sorted(members - expected)
    missing = sorted(expected - members)
    problems = []
    if extra:
        problems.append(f"EXTRA in {what} (not in runtime_package_files() + LICENSE): {extra}")
    if missing:
        problems.append(f"MISSING from {what} (dropped from package.json `files`?): {missing}")
    if problems:
        raise PayloadError("\n".join(problems))


def check_lf_and_identity(members: dict[str, bytes], checkout: Path, what: str) -> None:
    problems = []
    for rel in sorted(members):
        data = members[rel]
        if b"\r" in data:
            problems.append(f"{what}: {rel} is not LF (contains CR) -- the checkout must be LF (.gitattributes `runtime/** -text`)")
        src = checkout / rel
        if not src.is_file():
            problems.append(f"{what}: {rel} is in the tarball but not in the checkout {checkout}")
        elif src.read_bytes() != data:
            problems.append(f"{what}: {rel} bytes differ between the tarball and the checkout (npm rewrote it?)")
    if problems:
        raise PayloadError("\n".join(problems))


def files_json(members: dict[str, bytes]) -> dict[str, str]:
    return {rel: sha256_bytes(members[rel]) for rel in sorted(members)}


def prepare_runtime_payload(out: Path, runtime_dir: Path = RUNTIME_DIR, lib_rs: Path = LIB_RS) -> dict[str, str]:
    """Pack `runtime_dir` -> `<out>/runtime-payload{,.tgz}` + `<out>/runtime-payload.files.json`,
    asserting membership == E ∪ {LICENSE}, LF, byte-identity with `runtime_dir`. Returns files.json."""
    expected = expected_runtime_membership(lib_rs)
    scratch = out / ".pack"
    tgz = npm_pack(runtime_dir, scratch)
    members = tarball_members(tgz)
    check_membership(set(members), expected, "runtime tarball")
    check_lf_and_identity(members, runtime_dir, "runtime tarball")
    final_tgz = out / "runtime-payload.tgz"
    if final_tgz.exists():
        final_tgz.unlink()
    shutil.move(str(tgz), str(final_tgz))
    shutil.rmtree(scratch, ignore_errors=True)
    extract_to(members, out / "runtime-payload")
    fj = files_json(members)
    (out / "runtime-payload.files.json").write_text(json.dumps(fj, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    n_e = len(expected - NPM_ONLY)
    print(f"prepare_release_payloads: runtime payload membership == E({n_e}) + {sorted(NPM_ONLY)} "
          f"({len(members)} files), LF, byte-identical to {runtime_dir.name}/ -> {out / 'runtime-payload'}")
    return fj


def prepare_scaffolder_payload(out: Path, scaffolder_dir: Path = SCAFFOLDER_DIR) -> dict[str, str]:
    """Pack `packages/create-pyths-app/` -> `<out>/scaffolder-payload{,.tgz}` + files.json. The
    scaffolder IS the template (a single index.js + the copied skills); membership sanity only."""
    scratch = out / ".pack"
    tgz = npm_pack(scaffolder_dir, scratch)
    members = tarball_members(tgz)
    for need in ("package.json", "index.js", "LICENSE"):
        if need not in members:
            raise PayloadError(f"scaffolder tarball: missing {need}")
    # S10: the generated skills/ subtree must be EXACTLY the two skills copy-skills.cjs emits (a stale
    # skills/old.md left by a prior prepack must not ship), and every member must be LF (the same
    # discipline as the runtime preparer -- copy-skills normalizes CRLF->LF, so a CR here is drift).
    skills = sorted(r for r in members if r.startswith("skills/"))
    expected_skills = ["skills/compressing-ps-to-psc.md", "skills/pythscribe-language.md"]
    if skills != expected_skills:
        raise PayloadError(f"scaffolder tarball: skills/ subtree is {skills}, not exactly {expected_skills} (stale/missing generated skill)")
    crlf = sorted(r for r in members if b"\r" in members[r])
    if crlf:
        raise PayloadError(f"scaffolder tarball: not LF (CR in {crlf}) -- copy-skills must normalize CRLF->LF")
    bad = sorted(r for r in members if r.startswith("node_modules/") or ".test." in r)
    if bad:
        raise PayloadError(f"scaffolder tarball: unexpected members {bad}")
    final_tgz = out / "scaffolder-payload.tgz"
    if final_tgz.exists():
        final_tgz.unlink()
    shutil.move(str(tgz), str(final_tgz))
    shutil.rmtree(scratch, ignore_errors=True)
    extract_to(members, out / "scaffolder-payload")
    fj = files_json(members)
    (out / "scaffolder-payload.files.json").write_text(json.dumps(fj, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    print(f"prepare_release_payloads: scaffolder payload ({len(members)} files) -> {out / 'scaffolder-payload'}")
    return fj


def vendor_runtime_payload(payload_dir: Path, vendored: Path = VENDORED_RUNTIME) -> None:
    """Copy `dist/runtime-payload/` -> `pythscribe/_runtime/pyths-runtime/` (M0.2), raw bytes,
    package-root-relative paths preserved so the `exports` map resolves. The copy is COMMITTED and
    gated by `scripts/verify_runtime_mirror.py`; this is the only sanctioned way to refresh it."""
    if vendored.exists():
        shutil.rmtree(vendored)
    shutil.copytree(payload_dir, vendored)
    n = sum(1 for p in vendored.rglob("*") if p.is_file())
    print(f"prepare_release_payloads: vendored {n} files -> {vendored}")


def vendor_scaffolder_payload(payload_dir: Path, vendored: Path = VENDORED_SCAFFOLDER) -> None:
    """Copy `dist/scaffolder-payload/` -> `pythscribe/_web/create-pyths-app/` (M7), raw bytes,
    package-root-relative paths preserved so `node index.js` resolves + the skills bundle ships.
    The copy is COMMITTED and gated by `scripts/verify_scaffolder_mirror.py`; this is the only
    sanctioned way to refresh it (never edit the vendored copy by hand)."""
    if vendored.exists():
        shutil.rmtree(vendored)
    vendored.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(payload_dir, vendored)
    n = sum(1 for p in vendored.rglob("*") if p.is_file())
    print(f"prepare_release_payloads: vendored {n} scaffolder files -> {vendored}")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--out", default=str(REPO / "dist"), help="output directory (default: dist/)")
    ap.add_argument("--vendor", action="store_true", help="also refresh pythscribe/_runtime/pyths-runtime/ AND pythscribe/_web/create-pyths-app/ from the prepared payloads")
    ns = ap.parse_args(argv)
    out = Path(ns.out).resolve()
    try:
        prepare_runtime_payload(out)
        prepare_scaffolder_payload(out)
        if ns.vendor:
            vendor_runtime_payload(out / "runtime-payload")
            vendor_scaffolder_payload(out / "scaffolder-payload")
    except PayloadError as e:
        print(f"prepare_release_payloads: RED\n{e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
