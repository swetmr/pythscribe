#!/usr/bin/env python3
"""Fail unless EVERY built wheel carries the complete mixed-licence notice set (spec 13-09-26, M5;
validation §H1): the PEP 639 expression `MIT AND LicenseRef-FSL-1.1-ALv2` in METADATA and, under
`<dist-info>/licenses/`, the MIT text (pythscribe/LICENSE), the FSL text (LICENSE.md), the vendored
runtime's own MIT notice (pythscribe/_runtime/pyths-runtime/LICENSE), the per-path map
(pythscribe/LICENSES-MAP.md) and the third-party inventory of the bundled compiler
(third_party/inventory.json + notices/*.txt + THIRD_PARTY_NOTICES.md) covering every release target.

    python scripts/verify_wheel_license.py [--dist dist] [--cargo-lock auto|none|PATH] [--check-closure]

What makes it non-vacuous (each is a paired control in tests/pythscribe/test_wheel_m5.py):
  * every notice is required by NAME and by CONTENT fingerprint -- an MIT text where the FSL text
    should be, or an empty file, is RED, not "a file exists";
  * METADATA's `License-File` list and the licenses/ tree must agree both ways (declared-but-absent
    and present-but-undeclared are RED), and the legacy `License:` field / `License ::` classifier
    must be absent (PEP 639 forbids mixing; a backend downgrade that emits them is RED);
  * the third-party inventory is bound to Cargo.lock: `inventory.json` records the LF-normalized
    sha256 of the Cargo.lock it was generated from and `--cargo-lock` (default: the checkout's, when
    this script runs from one) must match -- a dependency bump without regeneration is RED;
  * the inventory is ONE shared artifact (byte-identical in every wheel), so for EVERY release target
    (pythscribe/build/_native.py::TRIPLE_TO_TAG) -- not only the wheel's own -- it must list crates, and
    every crate maps to >= 1 notice file whose bytes hash to the recorded sha256 -- a dropped target, or
    a removed/edited notice, is RED in a host-tagged wheel exactly as in a binary-less `any` wheel;
  * the platform tag must be a release target (tag -> triple) or `any`;
  * `--check-closure` re-derives the crate closure for the wheel's OWN target(s) with `cargo metadata`
    (the INDEPENDENT source) and requires set-equality with the inventory -- a crate dropped from BOTH
    inventory.json and its notice file (self-consistent tampering) is RED; cargo missing is RED, never a skip.
Iterates ALL wheels in --dist (5 platform wheels + possibly a none-any); the first RED wheel does
not hide the rest.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import zipfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
from pythscribe.build._native import TRIPLE_TO_TAG  # noqa: E402

EXPECTED_EXPRESSION = "MIT AND LicenseRef-FSL-1.1-ALv2"
TAG_TO_TRIPLE: dict[str, str] = {tag: triple for triple, tag in TRIPLE_TO_TAG.items()}
MIT_TEXT = "pythscribe/LICENSE"
FSL_TEXT = "LICENSE.md"
RUNTIME_TEXT = "pythscribe/_runtime/pyths-runtime/LICENSE"
MAP_DOC = "pythscribe/LICENSES-MAP.md"
INVENTORY = "third_party/inventory.json"
NOTICES_MD = "third_party/THIRD_PARTY_NOTICES.md"
NOTICES_DIR = "third_party/notices/"
REQUIRED_FILES = (MIT_TEXT, FSL_TEXT, RUNTIME_TEXT, MAP_DOC, INVENTORY, NOTICES_MD)
PACKAGE_RUNTIME_LICENSE = "pythscribe/_runtime/pyths-runtime/LICENSE"  # the payload copy INSIDE the package
BINARY_NAMES = ("pythscribe/_bin/pyths", "pythscribe/_bin/pyths.exe")
_WHEEL = re.compile(r"^(?P<name>[^-]+)-(?P<ver>[^-]+)-(?P<py>[^-]+)-(?P<abi>[^-]+)-(?P<plat>[^-]+)\.whl$")


def sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def cargo_lock_sha256(lock: Path) -> str:
    return sha256_bytes(lock.read_bytes().replace(b"\r\n", b"\n"))


# ----------------------------------------------------------------------------- content fingerprints


def fp_mit(text: str) -> list[str]:
    out = []
    if not text.startswith("MIT License"):
        out.append("does not start with `MIT License`")
    if "Copyright (c)" not in text:
        out.append("no `Copyright (c)` line")
    if "Permission is hereby granted, free of charge" not in text:
        out.append("not the MIT grant text")
    return out


def fp_fsl(text: str) -> list[str]:
    out = []
    if "Functional Source License, Version 1.1, ALv2" not in text:
        out.append("not the FSL-1.1-ALv2 title")
    if "FSL-1.1-ALv2" not in text or "Copyright" not in text:
        out.append("missing the FSL-1.1-ALv2 abbreviation / copyright notice")
    if "Permission is hereby granted, free of charge" in text:
        out.append("this is an MIT text, not the FSL")
    return out


def fp_map(text: str) -> list[str]:
    out = []
    for needle in (EXPECTED_EXPRESSION, "_bin/pyths", "third_party/", MIT_TEXT, FSL_TEXT, "pyths-runtime"):
        if needle not in text:
            out.append(f"per-path map lacks `{needle}`")
    return out


FINGERPRINTS = {MIT_TEXT: fp_mit, FSL_TEXT: fp_fsl, RUNTIME_TEXT: fp_mit, MAP_DOC: fp_map}


# ----------------------------------------------------------------------------- METADATA


def parse_metadata(text: str) -> dict[str, list[str]]:
    """Header fields (multi-valued) up to the first blank line; the body is not needed."""
    fields: dict[str, list[str]] = {}
    for line in text.replace("\r\n", "\n").split("\n"):
        if line == "":
            break
        if line[:1].isspace():
            continue  # continuation of a folded field
        k, _, v = line.partition(":")
        fields.setdefault(k.strip(), []).append(v.strip())
    return fields


def wheel_tag(path: Path) -> str | None:
    m = _WHEEL.match(path.name)
    return m.group("plat") if m else None


def targets_for_tag(tag: str) -> list[str] | None:
    if tag == "any":
        return list(TRIPLE_TO_TAG)  # a binary-less wheel still ships the inventory for EVERY target
    triple = TAG_TO_TRIPLE.get(tag)
    return [triple] if triple else None


# ----------------------------------------------------------------------------- the per-wheel gate


def verify_wheel(path: Path, *, cargo_lock_sha: str | None = None, check_closure: bool = False) -> list[str]:
    """RED lines for ONE wheel (empty == GREEN)."""
    P: list[str] = []
    w = path.name
    try:
        with zipfile.ZipFile(path) as z:
            names = z.namelist()
            blobs = {n: z.read(n) for n in names if "/licenses/" in n or n.endswith("/METADATA") or n == PACKAGE_RUNTIME_LICENSE}
    except (zipfile.BadZipFile, OSError) as e:
        return [f"{w}: not a readable wheel ({e})"]
    dist_infos = sorted({n.split("/")[0] for n in names if n.split("/")[0].endswith(".dist-info")})
    if len(dist_infos) != 1:
        return [f"{w}: expected exactly one .dist-info, found {dist_infos}"]
    di = dist_infos[0]
    meta_name = f"{di}/METADATA"
    if meta_name not in blobs:
        return [f"{w}: no {meta_name}"]
    meta = parse_metadata(blobs[meta_name].decode("utf-8", errors="replace"))

    # 1. the expression (PEP 639) -- exactly one, exact; no legacy field / classifier
    expr = meta.get("License-Expression", [])
    if expr != [EXPECTED_EXPRESSION]:
        P.append(f"{w}: METADATA License-Expression is {expr!r}, expected [{EXPECTED_EXPRESSION!r}]")
    if meta.get("License"):
        P.append(f"{w}: METADATA carries the legacy `License:` field {meta['License']!r} (PEP 639: expression only)")
    bad_cls = [c for c in meta.get("Classifier", []) if c.startswith("License ::")]
    if bad_cls:
        P.append(f"{w}: METADATA carries `License ::` classifiers {bad_cls} (PEP 639: expression only)")

    # 2. License-File <-> licenses/ tree, both ways
    declared = set(meta.get("License-File", []))
    prefix = f"{di}/licenses/"
    present = {n[len(prefix):] for n in names if n.startswith(prefix) and not n.endswith("/")}
    for f in sorted(declared - present):
        P.append(f"{w}: METADATA declares License-File {f} but licenses/{f} is absent")
    for f in sorted(present - declared):
        P.append(f"{w}: licenses/{f} is present but not declared as License-File")
    for req in REQUIRED_FILES:
        if req not in present:
            P.append(f"{w}: required notice licenses/{req} is missing")

    def text_of(rel: str) -> str | None:
        b = blobs.get(prefix + rel)
        return None if b is None else b.decode("utf-8", errors="replace")

    # 3. content fingerprints of the fixed notices
    for rel, fp in FINGERPRINTS.items():
        t = text_of(rel)
        if t is None:
            continue
        if not t.strip():
            P.append(f"{w}: licenses/{rel} is empty")
            continue
        for reason in fp(t):
            P.append(f"{w}: licenses/{rel}: {reason}")

    # 4. the vendored runtime's notice: the licenses/ copy == the payload copy inside the package
    if PACKAGE_RUNTIME_LICENSE not in blobs:
        P.append(f"{w}: the vendored runtime payload has no {PACKAGE_RUNTIME_LICENSE}")
    elif RUNTIME_TEXT in present and blobs[PACKAGE_RUNTIME_LICENSE] != blobs[prefix + RUNTIME_TEXT]:
        P.append(f"{w}: licenses/{RUNTIME_TEXT} differs from the payload copy {PACKAGE_RUNTIME_LICENSE}")

    # 5. the third-party inventory for this wheel's target(s)
    tag = wheel_tag(path)
    targets = targets_for_tag(tag) if tag else None
    if targets is None:
        P.append(f"{w}: platform tag {tag!r} is not a release target ({sorted(TAG_TO_TRIPLE)}) nor `any`")
    inv_text = text_of(INVENTORY)
    inv = None
    if inv_text is not None:
        try:
            inv = json.loads(inv_text)
        except ValueError as e:
            P.append(f"{w}: licenses/{INVENTORY} is not JSON ({e})")
    if inv is not None:
        if inv.get("schema") != 1:
            P.append(f"{w}: inventory schema {inv.get('schema')!r} != 1")
        if inv.get("wheel_tags") != TRIPLE_TO_TAG:
            P.append(f"{w}: inventory wheel_tags {inv.get('wheel_tags')!r} != the release target map {TRIPLE_TO_TAG!r}")
        if cargo_lock_sha is not None and inv.get("cargo_lock_sha256") != cargo_lock_sha:
            P.append(f"{w}: inventory was generated from Cargo.lock {str(inv.get('cargo_lock_sha256'))[:12]} but the checkout's is "
                     f"{cargo_lock_sha[:12]} -- STALE third-party inventory; run scripts/gen_third_party_notices.py")
        notices = inv.get("notices") or {}
        if not notices:
            P.append(f"{w}: inventory lists no notice texts")
        for fname, n in notices.items():
            b = blobs.get(prefix + NOTICES_DIR + fname)
            if b is None:
                P.append(f"{w}: notice licenses/{NOTICES_DIR}{fname} ({n.get('license_id')}) is missing")
            elif sha256_bytes(b) != n.get("sha256"):
                hint = " -- CRLF line endings: the checkout was not byte-exact (.gitattributes `third_party/** -text`)" if b"\r\n" in b else ""
                P.append(f"{w}: notice licenses/{NOTICES_DIR}{fname} bytes do not match the inventory sha256 (edited notice){hint}")
            elif not b.strip():
                P.append(f"{w}: notice licenses/{NOTICES_DIR}{fname} is empty")
        extra = sorted(f[len(NOTICES_DIR):] for f in present if f.startswith(NOTICES_DIR))
        for f in extra:
            if f not in notices:
                P.append(f"{w}: licenses/{NOTICES_DIR}{f} is not in the inventory")
        md = text_of(NOTICES_MD) or ""
        # The inventory is ONE shared artifact (byte-identical in every wheel; `wheel_tags` above must equal the
        # full release map), so its self-consistency is checked for EVERY release target regardless of which
        # wheel carries it -- a target dropped from the inventory is RED in a host-tagged wheel too, not only in
        # a binary-less `any` wheel. Only the tag validity (above) and the cargo-metadata closure (below) are
        # per this wheel's own target(s).
        for triple in TRIPLE_TO_TAG:
            rows = (inv.get("targets") or {}).get(triple)
            if not rows:
                P.append(f"{w}: inventory has no crates for target {triple} (the inventory must cover every release "
                         f"target {sorted(TRIPLE_TO_TAG)}; this wheel's tag is {tag!r})")
                continue
            for r in rows:
                key = f"{r.get('name')}@{r.get('version')}"
                if not r.get("license"):
                    P.append(f"{w}: {triple}: {key} has no licence expression")
                if not r.get("notices"):
                    P.append(f"{w}: {triple}: {key} has NO notice text")
                for fname in r.get("notices") or []:
                    n = notices.get(fname)
                    if n is None:
                        P.append(f"{w}: {triple}: {key} references notice {fname} which the inventory does not list")
                    elif key not in n.get("crates", []):
                        P.append(f"{w}: {triple}: notice {fname} does not list {key} as a user")
                if md and f"| {r.get('name')} | {r.get('version')} |" not in md:
                    P.append(f"{w}: {triple}: {key} is absent from licenses/{NOTICES_MD}")
        if check_closure and targets:
            P.extend(_closure_diff(w, inv, targets))
    return P


def _closure_diff(w: str, inv: dict, targets: list[str]) -> list[str]:
    """The INDEPENDENT source: `cargo metadata` per target must equal the inventory's crate set."""
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    try:
        from gen_third_party_notices import closure_for_target
    finally:
        sys.path.pop(0)
    if subprocess.run(["cargo", "--version"], capture_output=True).returncode != 0:
        return [f"{w}: --check-closure requires cargo on PATH (the closure check is NOT skippable)"]
    out = []
    for triple in targets:
        try:
            live = {f"{c['name']}@{c['version']}" for c in closure_for_target(triple)}
        except SystemExit as e:
            return [f"{w}: --check-closure: {e}"]
        have = {f"{r.get('name')}@{r.get('version')}" for r in (inv.get("targets") or {}).get(triple, [])}
        if live - have:
            out.append(f"{w}: {triple}: crates linked into the binary but ABSENT from the inventory: {sorted(live - have)}")
        if have - live:
            out.append(f"{w}: {triple}: inventory lists crates not in the dependency closure: {sorted(have - live)}")
    return out


def verify_all(dist: Path, *, cargo_lock_sha: str | None = None, check_closure: bool = False) -> tuple[list[Path], list[str]]:
    whls = sorted(dist.glob("*.whl"))
    if not whls:
        return [], [f"no wheel in {dist}/"]
    problems: list[str] = []
    for whl in whls:
        problems.extend(verify_wheel(whl, cargo_lock_sha=cargo_lock_sha, check_closure=check_closure))
    return whls, problems


def resolve_cargo_lock(arg: str) -> tuple[str | None, str]:
    if arg == "none":
        return None, "Cargo.lock binding SKIPPED (--cargo-lock none)"
    if arg == "auto":
        lock = REPO / "Cargo.lock"
        if not lock.is_file():
            return None, "Cargo.lock binding SKIPPED (no Cargo.lock next to this checkout; pass --cargo-lock PATH)"
    else:
        lock = Path(arg)
        if not lock.is_file():
            raise SystemExit(f"verify_wheel_license: --cargo-lock {lock} is not a file")
    sha = cargo_lock_sha256(lock)
    return sha, f"bound to {lock} ({sha[:12]})"


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--dist", default="dist")
    ap.add_argument("--cargo-lock", default="auto", help="auto (the checkout's Cargo.lock, default) | none | PATH -- the inventory must have been generated from it")
    ap.add_argument("--check-closure", action="store_true", help="also re-derive each target's crate closure with `cargo metadata` and require set-equality (cargo required)")
    ns = ap.parse_args(argv)
    sha, how = resolve_cargo_lock(ns.cargo_lock)
    whls, problems = verify_all(Path(ns.dist), cargo_lock_sha=sha, check_closure=ns.check_closure)
    if problems:
        print(f"verify_wheel_license: RED ({len(problems)} problem(s)) over {len(whls)} wheel(s); {how}:", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    print(f"verify_wheel_license: OK -> {len(whls)} wheel(s) {[w.name for w in whls]}: License-Expression `{EXPECTED_EXPRESSION}`, "
          f"notices {list(REQUIRED_FILES)} + third_party/notices/* present and fingerprinted; inventory {how}"
          f"{'; closure re-derived with cargo metadata' if ns.check_closure else ''}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
