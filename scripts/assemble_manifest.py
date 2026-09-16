#!/usr/bin/env python3
"""M6.1 -- the POST-BUILD release manifest producer (spec 13-09-26, requirements §5.1; plan M6.1).

    python scripts/assemble_manifest.py --dist dist --binaries artifacts --out release_manifest.json \
        [--tag v<V>] [--source-sha S --run-id N --run-attempt K]   # defaults: GITHUB_SHA / GITHUB_RUN_ID / GITHUB_RUN_ATTEMPT

Runs in the `manifest` job of release.yml AFTER `prepare`, the 5 `build` legs, the 5 `wheel` legs and
`wheel-set` -- NEVER after acceptance (acceptance binds every install to THIS file; the reverse edge was
the rev-3 deadlock). Inputs are the FINALIZED artifacts only:
  * `<dist>/pythscribe-<V>-py3-none-<tag>.whl` x5 + `<dist>/pythscribe-<V>.tar.gz` (the `pythscribe-dist` artifact);
  * `<binaries>/pyths-<triple>/pyths-<triple>.{tar.gz,zip}` (or the extracted `pyths[.exe]`) -- B_t per target;
  * `<dist>/runtime-payload.{tgz,files.json}` + `<dist>/scaffolder-payload.{tgz,files.json}` (M0's prepared payloads).
It BINDS them before it records anything (each RED names the target):
  * the binary INSIDE wheel t hashes to B_t (`native_sha256[t]` -- the finalize-before-fan-out identity, §5.3);
  * the vendored runtime INSIDE every wheel == `runtime-payload.files.json` exactly (set + sha; the D-identity);
  * every wheel carries the SAME `pythscribe/_pin.py` compiler pin, which at a final `vX.Y.Z` must equal V
    (guard_tag_version.py enforces the same at the tag; here it is defense in depth, per-wheel);
  * the distribution version V is the sdist's, every wheel agrees, and `--tag` (when given) == `v<V>`.
Circularity guard: the output path must not be a TRACKED file of the checkout (a manifest committed inside the
commit it identifies is circular) -- it refuses to overwrite/emit one. The record carries `manifest_sha256` ==
`pythscribe.artifacts.manifest_self_hash` (the ONE canonical self-hash) and is re-validated through
`require_evidence.check_manifest_shape` before it is written. `prerequisite_jobs` is INFORMATIONAL (consumers
use the fixed constant). `optimizer` records the ambient optimizer identity of the manifest runner with
`applied: false` -- the release builds no artifact; per-artifact optimizer state lives in artifact manifests (M3).

Exit 0 GREEN (manifest written); 1 RED (nothing written).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
sys.path.insert(0, str(REPO / "scripts"))
from pythscribe.artifacts import manifest_self_hash  # noqa: E402
from pythscribe.build._native import EXPECTED_WHEEL_SET, TRIPLE_TO_TAG  # noqa: E402
from require_evidence import (  # noqa: E402
    MANIFEST_SCHEMA,
    NPM_PAYLOAD_PACKAGES,
    NPM_PLATFORM_TO_TRIPLE,
    NPM_PLUGIN_PACKAGES,
    NPM_WRAPPER,
    REQUIRED_PREREQ_JOBS,
    check_manifest_shape,
)

VENDORED_PREFIX = "pythscribe/_runtime/pyths-runtime/"
SCAFFOLDER_PREFIX = "pythscribe/_web/create-pyths-app/"  # B5: the wheel's vendored scaffolder tree
PIN_MEMBER = "pythscribe/_pin.py"
_SDIST = re.compile(r"^pythscribe-(?P<ver>.+)\.tar\.gz$")
_FINAL = re.compile(r"^\d+\.\d+\.\d+$")


class ManifestError(RuntimeError):
    pass


def sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def binary_name(triple: str) -> str:
    return "pyths.exe" if "windows" in triple else "pyths"


# ----------------------------------------------------------------------------- inputs


def read_binary(binaries: Path, triple: str) -> bytes:
    """B_t: the finalized release binary, from `<binaries>/pyths-<triple>/` -- either the extracted
    `pyths[.exe]` or the packaged archive the build leg uploaded (exactly one member)."""
    d = binaries / f"pyths-{triple}"
    name = binary_name(triple)
    if (d / name).is_file():
        return (d / name).read_bytes()
    archives = sorted(d.glob("pyths-*.tar.gz")) + sorted(d.glob("pyths-*.zip")) if d.is_dir() else []
    if len(archives) != 1:
        raise ManifestError(f"{triple}: no finalized binary under {d} (expected {name} or exactly one pyths-{triple}.tar.gz/.zip; found {[a.name for a in archives]})")
    a = archives[0]
    if a.suffix == ".zip":
        with zipfile.ZipFile(a) as z:
            names = [n for n in z.namelist() if not n.endswith("/")]
            if names != [name]:
                raise ManifestError(f"{triple}: archive {a.name} members {names} != [{name}]")
            return z.read(name)
    with tarfile.open(a, "r:gz") as t:
        members = [m for m in t.getmembers() if m.isfile()]
        if [m.name for m in members] != [name]:
            raise ManifestError(f"{triple}: archive {a.name} members {[m.name for m in members]} != [{name}]")
        f = t.extractfile(members[0])
        assert f is not None
        return f.read()


def wheel_members(wheel: Path) -> dict[str, bytes]:
    with zipfile.ZipFile(wheel) as z:
        return {n: z.read(n) for n in z.namelist() if not n.endswith("/")}


def wheel_pin(members: dict[str, bytes]) -> tuple[str, str]:
    """(COMPILER_VERSION, COMPILER_COMMIT) shipped INSIDE the wheel."""
    src = members.get(PIN_MEMBER)
    if src is None:
        raise ManifestError(f"wheel lacks {PIN_MEMBER}")
    text = src.decode("utf-8")
    mv = re.search(r'COMPILER_VERSION = "([^"]+)"', text)
    mc = re.search(r'COMPILER_COMMIT = "([^"]+)"', text)
    if not mv or not mc:
        raise ManifestError(f"{PIN_MEMBER} inside the wheel has no COMPILER_VERSION / COMPILER_COMMIT")
    return mv.group(1), mc.group(1)


def wheel_vendored_runtime(members: dict[str, bytes]) -> dict[str, str]:
    return {n[len(VENDORED_PREFIX):]: sha256_bytes(b) for n, b in members.items() if n.startswith(VENDORED_PREFIX)}


def wheel_vendored_scaffolder(members: dict[str, bytes]) -> dict[str, str]:
    return {n[len(SCAFFOLDER_PREFIX):]: sha256_bytes(b) for n, b in members.items() if n.startswith(SCAFFOLDER_PREFIX)}


def load_payload(dist: Path, stem: str) -> dict:
    tgz, fj = dist / f"{stem}.tgz", dist / f"{stem}.files.json"
    for p in (tgz, fj):
        if not p.is_file():
            raise ManifestError(f"prepared payload input missing: {p} (scripts/prepare_release_payloads.py output)")
    files = json.loads(fj.read_text(encoding="utf-8"))
    if not isinstance(files, dict) or not files or not all(isinstance(v, str) and len(v) == 64 for v in files.values()):
        raise ManifestError(f"{fj}: not a {{path: sha256}} map")
    return {"tarball_sha256": sha256_file(tgz), "files": dict(sorted(files.items()))}


def is_tracked(checkout: Path, path: Path) -> bool:
    """True when `path` is a git-TRACKED file of `checkout` (the circularity guard's question)."""
    try:
        rel = path.resolve().relative_to(checkout.resolve())
    except ValueError:
        return False
    r = subprocess.run(["git", "-C", str(checkout), "ls-files", "--error-unmatch", "--", rel.as_posix()],
                       capture_output=True, text=True, check=False)
    return r.returncode == 0


def dist_version(dist: Path) -> str:
    sdists = [p for p in sorted(dist.glob("pythscribe-*.tar.gz")) if _SDIST.match(p.name)]
    if len(sdists) != 1:
        raise ManifestError(f"expected exactly one sdist pythscribe-<V>.tar.gz in {dist}, found {[p.name for p in sdists]}")
    return _SDIST.match(sdists[0].name).group("ver")  # type: ignore[union-attr]


# ----------------------------------------------------------------------------- assembly


def assemble(*, dist: Path, binaries: Path, source_sha: str, run_id: int, run_attempt: int, tag: str | None,
             optimizer: dict | None = None) -> dict:
    """The manifest dict (self-hash included), or ManifestError naming the first broken binding."""
    if not re.fullmatch(r"[0-9a-f]{40}", source_sha or ""):
        raise ManifestError(f"source_sha {source_sha!r} is not a 40-hex commit SHA (GITHUB_SHA of the tag commit S)")
    if run_id < 1 or run_attempt < 1:
        raise ManifestError("build_run_id / build_run_attempt must be positive (GITHUB_RUN_ID / GITHUB_RUN_ATTEMPT)")
    version = dist_version(dist)
    nominal_tag = f"v{version}"
    if tag is not None and tag != nominal_tag:
        raise ManifestError(f"--tag {tag!r} != v<distribution version> {nominal_tag!r} (the dist was built from another version)")
    runtime_payload = load_payload(dist, "runtime-payload")
    scaffolder_payload = load_payload(dist, "scaffolder-payload")
    sdist = dist / f"pythscribe-{version}.tar.gz"

    targets: dict[str, dict] = {}
    pins: dict[str, tuple[str, str]] = {}
    for triple, wtag in sorted(TRIPLE_TO_TAG.items()):
        name = f"pythscribe-{version}-py3-none-{wtag}.whl"
        wheel = dist / name
        if not wheel.is_file():
            raise ManifestError(f"{triple}: wheel {name} missing from {dist} (the complete set is required; a missing target is RED)")
        native = read_binary(binaries, triple)
        native_sha = sha256_bytes(native)
        members = wheel_members(wheel)
        member = f"pythscribe/_bin/{binary_name(triple)}"
        if member not in members:
            raise ManifestError(f"{triple}: {name} carries no {member}")
        inside = sha256_bytes(members[member])
        if inside != native_sha:
            raise ManifestError(f"{triple}: binary inside {name} ({inside}) != finalized B_t ({native_sha}) -- the wheel does not ship the finalized binary (post-finalize rewrite?)")
        vend = wheel_vendored_runtime(members)
        if vend != runtime_payload["files"]:
            extra = sorted(set(vend) - set(runtime_payload["files"]))
            missing = sorted(set(runtime_payload["files"]) - set(vend))
            changed = sorted(k for k in set(vend) & set(runtime_payload["files"]) if vend[k] != runtime_payload["files"][k])
            raise ManifestError(f"{triple}: vendored runtime inside {name} != runtime-payload.files.json (extra {extra}, missing {missing}, changed {changed})")
        # B5: the wheel's vendored SCAFFOLDER tree must also equal the ONE prepared scaffolder payload
        # -- the same D-identity as the runtime. Without this the npm↔wheel scaffolder chain was open
        # (npm could ship scaffolder A while the wheel vendored B; every gate was green).
        vend_sc = wheel_vendored_scaffolder(members)
        if vend_sc != scaffolder_payload["files"]:
            extra = sorted(set(vend_sc) - set(scaffolder_payload["files"]))
            missing = sorted(set(scaffolder_payload["files"]) - set(vend_sc))
            changed = sorted(k for k in set(vend_sc) & set(scaffolder_payload["files"]) if vend_sc[k] != scaffolder_payload["files"][k])
            raise ManifestError(f"{triple}: vendored scaffolder inside {name} != scaffolder-payload.files.json (extra {extra}, missing {missing}, changed {changed})")
        pins[triple] = wheel_pin(members)
        targets[triple] = {
            "native_sha256": native_sha,
            "wheel_filename": name,
            "wheel_sha256": sha256_file(wheel),
            "wheel_tag": wtag,
        }
    if len({p for p in pins.values()}) != 1:
        raise ManifestError(f"the 5 wheels ship different compiler pins: {pins}")
    pin_version, pin_commit = next(iter(pins.values()))
    if _FINAL.match(version) and pin_version != version:
        raise ManifestError(f"final release {version}: the wheels' COMPILER_VERSION pin is {pin_version!r} (run scripts/set_version.py {version})")
    foreign = sorted(p.name for p in dist.iterdir() if p.suffix == ".whl" and p.name not in {t["wheel_filename"] for t in targets.values()})
    if foreign:
        raise ManifestError(f"foreign wheels in {dist}: {foreign} (only the 5 release wheels may be present)")
    manifest = {
        "schema": MANIFEST_SCHEMA,
        "version": version,
        "tag": nominal_tag,
        "source_sha": source_sha,
        "build_run_id": int(run_id),
        "build_run_attempt": int(run_attempt),
        "compiler": {"version": pin_version, "commit": pin_commit},
        "optimizer": optimizer if optimizer is not None else {"id": "none", "applied": False},
        "targets": targets,
        "sdist": {"filename": sdist.name, "sha256": sha256_file(sdist)},
        "runtime_payload": runtime_payload,
        "scaffolder_payload": scaffolder_payload,
        "npm": {
            **{pkg: None for pkg in sorted(NPM_PLATFORM_TO_TRIPLE)},
            "pyths-runtime": runtime_payload["tarball_sha256"],
            "create-pyths-app": scaffolder_payload["tarball_sha256"],
            **{pkg: None for pkg in sorted(NPM_PLUGIN_PACKAGES)},  # S9: not prepared payloads; identity by name@version
            NPM_WRAPPER: None,
        },
        "expected_wheel_set": sorted(EXPECTED_WHEEL_SET),
        "prerequisite_jobs": sorted(REQUIRED_PREREQ_JOBS),  # informational (consumers use the fixed constant)
    }
    assert set(NPM_PAYLOAD_PACKAGES) == {"pyths-runtime", "create-pyths-app"}
    manifest["manifest_sha256"] = manifest_self_hash(manifest)
    problems = check_manifest_shape(manifest)
    if problems:
        raise ManifestError("assembled manifest fails the consumer's shape check:\n  " + "\n  ".join(problems))
    return manifest


def ambient_optimizer() -> dict:
    try:
        from pythscribe.build.optimizer import resolve_wasm_opt
        return {"id": resolve_wasm_opt().id, "applied": False}
    except Exception as e:  # noqa: BLE001 -- informational field; never blocks the manifest
        return {"id": "none", "applied": False, "error": str(e)}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="assemble_manifest.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--dist", default="dist")
    ap.add_argument("--binaries", default="artifacts")
    ap.add_argument("--out", default="release_manifest.json")
    ap.add_argument("--tag", default=None, help="the pushed tag (must equal v<distribution version>)")
    ap.add_argument("--source-sha", default=os.environ.get("GITHUB_SHA", ""))
    ap.add_argument("--run-id", type=int, default=int(os.environ.get("GITHUB_RUN_ID", "0") or 0))
    ap.add_argument("--run-attempt", type=int, default=int(os.environ.get("GITHUB_RUN_ATTEMPT", "0") or 0))
    ap.add_argument("--checkout", default=str(REPO), help="the checkout of S (circularity guard: the output must not be tracked there)")
    ns = ap.parse_args(argv)
    out = Path(ns.out)
    try:
        if is_tracked(Path(ns.checkout), out):
            raise ManifestError(f"{out} is a TRACKED file of {ns.checkout} -- the release manifest is a post-build evidence artifact and must never be committed inside the commit it identifies (circularity); refusing to emit")
        manifest = assemble(dist=Path(ns.dist), binaries=Path(ns.binaries), source_sha=ns.source_sha, run_id=ns.run_id,
                            run_attempt=ns.run_attempt, tag=ns.tag, optimizer=ambient_optimizer())
    except ManifestError as e:
        print(f"assemble_manifest: RED -- {e}", file=sys.stderr)
        return 1
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"assemble_manifest: GREEN -- {out} version={manifest['version']} tag={manifest['tag']} run={manifest['build_run_id']}/{manifest['build_run_attempt']} "
          f"source={manifest['source_sha'][:12]} manifest_sha256={manifest['manifest_sha256'][:12]}")
    for t, info in manifest["targets"].items():
        print(f"  {t}: {info['wheel_filename']} wheel={info['wheel_sha256'][:12]} native={info['native_sha256'][:12]}")
    print(f"  sdist {manifest['sdist']['filename']} {manifest['sdist']['sha256'][:12]}; runtime payload {len(manifest['runtime_payload']['files'])} files; "
          f"scaffolder payload {len(manifest['scaffolder_payload']['files'])} files; compiler {manifest['compiler']['version']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
