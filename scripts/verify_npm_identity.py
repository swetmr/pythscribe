#!/usr/bin/env python3
"""M6.2 -- the npm identity gate over EVERY published package -> R-NI (spec 13-09-26, requirements §5.3; validation §E1).

    python scripts/verify_npm_identity.py --manifest release_manifest.json --dist dist --out evidence/R-NI.json \
        [--tarball-dir DIR | --registry https://registry.npmjs.org]

Runs in release.yml `npm-identity` AFTER `npm/publish.mjs` -- which may SKIP already-published packages
(`alreadyPublished`), platform compiler packages included. So the gate never trusts the publish step: it
DOWNLOADS each package at V from the registry (`npm pack <pkg>@<V>`; or reads pre-fetched tarballs from
`--tarball-dir` -- tests, offline), extracts it, and compares:
  * `@pythscribe/cli-<plat>@V`: `bin/pyths[.exe]` sha256 == manifest `native_sha256[triple]` == the binary
    INSIDE wheel `triple` from `--dist` (E1b: npm holding binary A at V while wheel + manifest carry B -> RED);
  * `pyths-runtime@V`: member set + per-file sha256 == manifest `runtime_payload.files` == every wheel's
    vendored `pythscribe/_runtime/pyths-runtime/` (E1a: runtime B in the wheel, A on npm -> RED); the tarball's
    sha256 == manifest `npm["pyths-runtime"]` (the prepared payload IS the published artifact);
  * `create-pyths-app@V`: member set + sha256 == manifest `scaffolder_payload.files`; tarball == manifest `npm[...]`;
  * `pythscribe@V` (wrapper): package.json version == V, every `@pythscribe/cli-*` optionalDependency pinned == V,
    `pyths-runtime` dependency `^V`.
Emits R-NI (requirements §5.2 shape) with a per-package verdict and the SAME identity block the M4 R-BA carries
(the run that produced the manifest; `require_evidence.py --role release-run` has already accepted it). Verdict
`pass` iff every package of the fixed set is present and identical. Exit 0 pass / 1 fail (record written either way).
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
import time
import zipfile
from pathlib import Path
from typing import Callable

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts"))
from require_evidence import (  # noqa: E402
    NPM_PACKAGES,
    NPM_PAYLOAD_PACKAGES,
    NPM_PLATFORM_TO_TRIPLE,
    NPM_PLUGIN_PACKAGES,
    NPM_PLUGIN_WRAPPER_DIRS,
    NPM_WRAPPER,
    RECORD_SCHEMA,
    REQUIRED_PREREQ_JOBS,
    files_map_sha256,
)

VENDORED_PREFIX = "pythscribe/_runtime/pyths-runtime/"
SCAFFOLDER_PREFIX = "pythscribe/_web/create-pyths-app/"  # B5: the wheel's vendored scaffolder tree
Fetcher = Callable[[str, str, Path], Path]

# S9 + NEW-wrapper: the plugin packages and the wrapper are packed from the checkout (not a prepared
# payload), so before this fix R-NI validated only their package.json name/version -- stale/malicious
# bytes at the same name@version passed (the immutable-registry hole). The CONTENT is now bound to the
# checkout of S: R-NI `npm pack`s each of these dirs and compares the registry download member-by-member
# (membership + per-file sha256). The checkout at S is the same commit publish.mjs published from (on a
# tag ref publish.mjs is check-only), so registry == `npm pack <dir>` by construction. Checkout-relative.
# codex pass-3 (c): these directories now come from the ONE authority npm/packages.json
# (NPM_PLUGIN_WRAPPER_DIRS in require_evidence.py) -- no duplicate literal that a new plugin could miss.
PLUGIN_WRAPPER_DIRS: dict[str, str] = dict(NPM_PLUGIN_WRAPPER_DIRS)


class IdentityError(RuntimeError):
    pass


def sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def sha256_file(p: Path) -> str:
    return sha256_bytes(p.read_bytes())


def pack_name(pkg: str, version: str) -> str:
    """The filename `npm pack` writes: `@scope/name` -> `scope-name-<V>.tgz`."""
    return f"{pkg.lstrip('@').replace('/', '-')}-{version}.tgz"


# ----------------------------------------------------------------------------- fetchers


# The just-published version can 404 (`ETARGET`/`notarget`) here for a short window even after
# npm-publish's propagation wait: this verifier is a SEPARATE job on a cold runner and resolves the
# registry independently, so it can query before the packument/version propagates to this path. Retry
# ONLY that propagation-lag signature (a real missing/foreign package still fails fast on its own
# merits). This is the whole reason identity is a retryable job of its own -- so a lag is absorbed, not
# reported as a byte failure. Matches scripts/testpypi_validate.py's Simple-API retry. (2026-09-18)
_NPM_PROPAGATION_LAG = re.compile(r"(?i)ETARGET|notarget|no matching version|E404|404 Not Found")


def registry_fetcher(registry: str | None, *, attempts: int = 12, backoff_s: float = 10.0) -> Fetcher:
    def fetch(pkg: str, version: str, dest: Path) -> Path:
        dest.mkdir(parents=True, exist_ok=True)
        npm = shutil.which("npm") or shutil.which("npm.cmd")
        if not npm:
            raise IdentityError("npm not found on PATH (needed to download the published packages)")
        args = [npm, "pack", f"{pkg}@{version}", "--pack-destination", str(dest)]
        if registry:
            args += ["--registry", registry]
        r = subprocess.run(args, capture_output=True, text=True, check=False, shell=(os.name == "nt"))
        for i in range(1, attempts):
            if r.returncode == 0:
                break
            blob = (r.stderr or "") + (r.stdout or "")
            if not _NPM_PROPAGATION_LAG.search(blob):
                break  # a genuine failure (auth, network, real absence) -- fail fast, do not spin
            print(f"npm pack {pkg}@{version}: not yet on the registry (propagation lag); retry {i}/{attempts - 1}", file=sys.stderr)
            time.sleep(backoff_s)
            r = subprocess.run(args, capture_output=True, text=True, check=False, shell=(os.name == "nt"))
        if r.returncode != 0:
            raise IdentityError(f"npm pack {pkg}@{version} failed: {(r.stderr or r.stdout).strip()[-800:]}")
        out = dest / pack_name(pkg, version)
        if not out.is_file():
            raise IdentityError(f"npm pack {pkg}@{version} produced no {out.name} in {dest}")
        return out
    return fetch


def dir_fetcher(tarball_dir: Path) -> Fetcher:
    def fetch(pkg: str, version: str, dest: Path) -> Path:
        p = tarball_dir / pack_name(pkg, version)
        if not p.is_file():
            raise IdentityError(f"{pkg}@{version}: {p} not found (package not published / not fetched)")
        return p
    return fetch


def local_package_content(pkg_dir: Path, work: Path) -> dict[str, str]:
    """S9: `npm pack` the local checkout dir and return `{member: sha256}` -- the EXPECTED content the
    registry must serve (npm pack and npm publish use identical file selection). Needs npm."""
    if not (pkg_dir / "package.json").is_file():
        raise IdentityError(f"{pkg_dir} has no package.json (cannot bind its published content to the checkout)")
    npm = shutil.which("npm") or shutil.which("npm.cmd")
    if not npm:
        raise IdentityError("npm not found on PATH (needed to pack the checkout for the content binding)")
    # `--pack-destination` MUST be ABSOLUTE: npm runs with cwd=pkg_dir, so a relative dest resolves under
    # the plugin/wrapper package dir (e.g. <pkg_dir>/evidence/.../*.tgz), which does not exist -> ENOENT.
    work = work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    before = set(work.glob("*.tgz"))
    r = subprocess.run([npm, "pack", "--pack-destination", str(work)], cwd=str(pkg_dir),
                       capture_output=True, text=True, check=False, shell=(os.name == "nt"))
    if r.returncode != 0:
        raise IdentityError(f"npm pack {pkg_dir} failed: {(r.stderr or r.stdout).strip()[-800:]}")
    new = sorted(set(work.glob("*.tgz")) - before, key=lambda p: p.stat().st_mtime_ns)
    if len(new) != 1:
        raise IdentityError(f"npm pack {pkg_dir} produced {len(new)} tarballs (expected 1)")
    return {rel: sha256_bytes(b) for rel, b in tarball_members(new[0]).items()}


# ----------------------------------------------------------------------------- readers


def tarball_members(tgz: Path) -> dict[str, bytes]:
    """`{package-root-relative path: bytes}` of every regular file (npm's `package/` prefix stripped)."""
    out: dict[str, bytes] = {}
    with tarfile.open(tgz, "r:gz") as tf:
        for m in tf.getmembers():
            if m.isdir():
                continue
            if not m.isfile():
                raise IdentityError(f"{tgz.name}: non-regular member {m.name!r}")
            if not m.name.startswith("package/"):
                raise IdentityError(f"{tgz.name}: member {m.name!r} is not under package/")
            rel = m.name[len("package/"):]
            if not rel or any(part in ("", ".", "..") for part in rel.split("/")):
                raise IdentityError(f"{tgz.name}: refusing member path {m.name!r}")
            f = tf.extractfile(m)
            assert f is not None
            out[rel] = f.read()
    return out


def wheel_binary_sha256(wheel: Path, triple: str) -> str:
    name = "pythscribe/_bin/" + ("pyths.exe" if "windows" in triple else "pyths")
    with zipfile.ZipFile(wheel) as z:
        if name not in z.namelist():
            raise IdentityError(f"{wheel.name} carries no {name}")
        return sha256_bytes(z.read(name))


def wheel_vendored_runtime(wheel: Path) -> dict[str, str]:
    with zipfile.ZipFile(wheel) as z:
        return {n[len(VENDORED_PREFIX):]: sha256_bytes(z.read(n)) for n in z.namelist() if n.startswith(VENDORED_PREFIX) and not n.endswith("/")}


def wheel_vendored_scaffolder(wheel: Path) -> dict[str, str]:
    with zipfile.ZipFile(wheel) as z:
        return {n[len(SCAFFOLDER_PREFIX):]: sha256_bytes(z.read(n)) for n in z.namelist() if n.startswith(SCAFFOLDER_PREFIX) and not n.endswith("/")}


def _diff(got: dict[str, str], want: dict[str, str]) -> list[str]:
    p = []
    extra, missing = sorted(set(got) - set(want)), sorted(set(want) - set(got))
    changed = sorted(k for k in set(got) & set(want) if got[k] != want[k])
    if extra:
        p.append(f"EXTRA files {extra}")
    if missing:
        p.append(f"MISSING files {missing}")
    if changed:
        p.append(f"CHANGED bytes {changed}")
    return p


def _package_json(members: dict[str, bytes], where: str) -> dict:
    if "package.json" not in members:
        raise IdentityError(f"{where}: tarball has no package.json")
    try:
        return json.loads(members["package.json"].decode("utf-8"))
    except ValueError as e:
        raise IdentityError(f"{where}: package.json unreadable ({e})") from None


# ----------------------------------------------------------------------------- per-package checks


def check_platform(pkg: str, members: dict[str, bytes], manifest: dict, dist: Path) -> tuple[dict, list[str]]:
    triple = NPM_PLATFORM_TO_TRIPLE[pkg]
    binname = "pyths.exe" if "windows" in triple else "pyths"
    p: list[str] = []
    info = manifest["targets"][triple]
    got = sha256_bytes(members[f"bin/{binname}"]) if f"bin/{binname}" in members else None
    wheel = dist / info["wheel_filename"]
    wheel_bin = wheel_binary_sha256(wheel, triple) if wheel.is_file() else None
    if got is None:
        p.append(f"published tarball carries no bin/{binname}")
    if wheel_bin is None:
        p.append(f"wheel {info['wheel_filename']} not in {dist}")
    if got is not None and got != info["native_sha256"]:
        p.append(f"published bin/{binname} sha256 {got} != manifest native_sha256[{triple}] {info['native_sha256']} (same-version-different-binary: the registry holds another binary at this version -- the alreadyPublished skip)")
    if wheel_bin is not None and wheel_bin != info["native_sha256"]:
        p.append(f"binary inside {info['wheel_filename']} {wheel_bin} != manifest native_sha256 {info['native_sha256']}")
    pj = _package_json(members, pkg)
    if pj.get("name") != pkg or pj.get("version") != manifest["version"]:
        p.append(f"package.json is {pj.get('name')}@{pj.get('version')}, expected {pkg}@{manifest['version']}")
    return {"triple": triple, "binary_sha256": got, "manifest_native_sha256": info["native_sha256"], "wheel_binary_sha256": wheel_bin}, p


def check_payload(pkg: str, members: dict[str, bytes], tarball_sha: str, manifest: dict, dist: Path) -> tuple[dict, list[str]]:
    key = NPM_PAYLOAD_PACKAGES[pkg]
    want = manifest[key]["files"]
    got = {rel: sha256_bytes(b) for rel, b in members.items()}
    p = [f"published {pkg} payload != manifest {key}.files: {x}" for x in _diff(got, want)]
    expected_tgz = (manifest.get("npm") or {}).get(pkg)
    if expected_tgz is not None and tarball_sha != expected_tgz:
        p.append(f"published tarball sha256 {tarball_sha} != manifest npm[{pkg}] {expected_tgz} (the prepared payload was not what got published)")
    pj = _package_json(members, pkg)
    if pj.get("name") != pkg or pj.get("version") != manifest["version"]:
        p.append(f"package.json is {pj.get('name')}@{pj.get('version')}, expected {pkg}@{manifest['version']}")
    wheels: dict[str, bool] = {}
    # B5: the payload's wheel-vendored copy, in EVERY wheel -- pyths-runtime at _runtime/, AND
    # create-pyths-app at _web/create-pyths-app/ (before B5 only the runtime was compared, leaving the
    # scaffolder npm↔wheel chain open: npm could ship scaffolder A while the wheel vendored B).
    vendored_of = {"pyths-runtime": wheel_vendored_runtime, "create-pyths-app": wheel_vendored_scaffolder}
    if pkg in vendored_of:
        reader = vendored_of[pkg]
        for triple, info in sorted(manifest["targets"].items()):
            wheel = dist / info["wheel_filename"]
            if not wheel.is_file():
                p.append(f"wheel {info['wheel_filename']} not in {dist}")
                wheels[info["wheel_tag"]] = False
                continue
            vend = reader(wheel)
            d = _diff(vend, want)
            wheels[info["wheel_tag"]] = not d
            if d:
                p.append(f"vendored {pkg} inside {info['wheel_filename']} != manifest {key}.files: {d}")
    block = {"files_sha256": files_map_sha256(got), "manifest_files_sha256": files_map_sha256(want), "file_count": len(got)}
    if wheels:
        block["wheels_vendored_match"] = wheels
    return block, p


def _content_binding(pkg: str, members: dict[str, bytes], expected: dict[str, str] | None) -> tuple[dict, list[str]]:
    """S9/NEW-wrapper: compare the registry tarball's members (membership + per-file sha256) to the
    EXPECTED content = `npm pack` of the checkout dir of S. A byte diff at the same name@version (the
    immutable-registry hole) is RED. When `expected` is None (no checkout/injection available -- e.g. an
    offline unit test) the content binding is SKIPPED; release.yml passes `--checkout` and the linter
    REQUIRES it, so production always binds. The record carries both hashes for the consumer's own check."""
    got = {rel: sha256_bytes(b) for rel, b in members.items()}
    block: dict = {"registry_files_sha256": files_map_sha256(got), "file_count": len(got)}
    if expected is None:
        block["content_bound"] = False
        return block, []
    block["content_bound"] = True
    block["checkout_files_sha256"] = files_map_sha256(expected)
    return block, [f"published {pkg} content != `npm pack` of the checkout: {x}" for x in _diff(got, expected)]


def check_plugin(pkg: str, members: dict[str, bytes], manifest: dict, expected: dict[str, str] | None) -> tuple[dict, list[str]]:
    """S9: a pure-JS plugin package (vite-plugin-pyths / next-plugin-pyths). Validate it was published
    at name@V AND bind its full content to the checkout of S (membership + per-file sha256), closing the
    "same name@version, stale/malicious bytes" hole -- not just the "never downloaded / wrong version" one."""
    p: list[str] = []
    pj = _package_json(members, pkg)
    if pj.get("name") != pkg or pj.get("version") != manifest["version"]:
        p.append(f"package.json is {pj.get('name')}@{pj.get('version')}, expected {pkg}@{manifest['version']}")
    block, cprob = _content_binding(pkg, members, expected)
    block["version"] = pj.get("version")
    return block, p + cprob


def check_wrapper(members: dict[str, bytes], manifest: dict, expected: dict[str, str] | None) -> tuple[dict, list[str]]:
    p: list[str] = []
    v = manifest["version"]
    pj = _package_json(members, NPM_WRAPPER)
    if pj.get("name") != NPM_WRAPPER or pj.get("version") != v:
        p.append(f"package.json is {pj.get('name')}@{pj.get('version')}, expected {NPM_WRAPPER}@{v}")
    opt = pj.get("optionalDependencies") or {}
    for plat in sorted(NPM_PLATFORM_TO_TRIPLE):
        if opt.get(plat) != v:
            p.append(f"optionalDependencies[{plat}] = {opt.get(plat)!r}, expected the exact pin {v!r}")
    deps = pj.get("dependencies") or {}
    if deps.get("pyths-runtime") not in (v, f"^{v}"):
        p.append(f"dependencies[pyths-runtime] = {deps.get('pyths-runtime')!r}, expected ^{v}")
    # NEW: bind the wrapper's FULL content (launcher glue included), not only version + dep pins.
    block, cprob = _content_binding(NPM_WRAPPER, members, expected)
    block.update({"version": pj.get("version"),
                  "optional_dependencies": {k: opt.get(k) for k in sorted(NPM_PLATFORM_TO_TRIPLE)},
                  "pyths_runtime_dep": deps.get("pyths-runtime")})
    return block, p + cprob


# ----------------------------------------------------------------------------- the gate


def verify(manifest: dict, dist: Path, fetch: Fetcher, work: Path, env: dict[str, str],
           *, checkout: Path | None = None, expected_content: dict[str, dict[str, str]] | None = None) -> dict:
    version = manifest["version"]
    # S9: the EXPECTED content of each plugin/wrapper package = `npm pack` of the checkout dir of S
    # (or an injected map for offline tests). None for a package -> its content binding is skipped.
    content_by_pkg: dict[str, dict[str, str]] = dict(expected_content or {})
    if not content_by_pkg and checkout is not None:
        for pkg, rel in PLUGIN_WRAPPER_DIRS.items():
            content_by_pkg[pkg] = local_package_content(checkout / rel, work / "content" / pkg)
    targets: dict[str, dict] = {}
    for pkg in sorted(NPM_PACKAGES):
        kind = ("platform" if pkg in NPM_PLATFORM_TO_TRIPLE else "payload" if pkg in NPM_PAYLOAD_PACKAGES
                else "plugin" if pkg in NPM_PLUGIN_PACKAGES else "wrapper")
        block: dict = {"kind": kind, "published_tarball_sha256": None, "problems": [], "verdict": "fail"}
        try:
            tgz = fetch(pkg, version, work / "tarballs")
            block["published_tarball_sha256"] = sha256_file(tgz)
            members = tarball_members(tgz)
            if pkg in NPM_PLATFORM_TO_TRIPLE:
                block["platform"], problems = check_platform(pkg, members, manifest, dist)
            elif pkg in NPM_PAYLOAD_PACKAGES:
                block["payload"], problems = check_payload(pkg, members, block["published_tarball_sha256"], manifest, dist)
            elif pkg in NPM_PLUGIN_PACKAGES:
                block["plugin"], problems = check_plugin(pkg, members, manifest, content_by_pkg.get(pkg))
            else:
                block["wrapper"], problems = check_wrapper(members, manifest, content_by_pkg.get(pkg))
        except (IdentityError, OSError, tarfile.TarError, zipfile.BadZipFile) as e:
            problems = [str(e)]
        block["problems"] = problems
        block["verdict"] = "pass" if not problems else "fail"
        targets[pkg] = block
    ok = all(b["verdict"] == "pass" for b in targets.values()) and set(targets) == set(NPM_PACKAGES)
    return {
        "record": "R-NI",
        "schema": RECORD_SCHEMA,
        "source_sha": env.get("GITHUB_SHA", ""),
        "manifest_sha256": manifest.get("manifest_sha256"),
        "producer": {
            "workflow": "release.yml", "job": "npm-identity",
            "run_id": int(env.get("GITHUB_RUN_ID", "0") or 0), "run_attempt": int(env.get("GITHUB_RUN_ATTEMPT", "0") or 0),
            "head_sha": env.get("GITHUB_SHA", ""),
        },
        "prerequisite_jobs_verified": sorted(REQUIRED_PREREQ_JOBS),
        "version": version,
        "targets": targets,
        "verdict": "pass" if ok else "fail",
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="verify_npm_identity.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--manifest", default="release_manifest.json")
    ap.add_argument("--dist", default="dist", help="the manifest-bound wheels (pythscribe-dist artifact)")
    ap.add_argument("--out", default="evidence/R-NI.json")
    ap.add_argument("--tarball-dir", default=None, help="pre-fetched `<scope-name>-<V>.tgz` files (tests / offline) instead of the registry")
    ap.add_argument("--registry", default=None, help="npm registry URL (default: npm's configured registry)")
    ap.add_argument("--work", default=None, help="scratch dir for downloads (default: <out dir>/npm-identity-work)")
    ap.add_argument("--checkout", required=True, help="the checkout of S (S9: bind plugin/wrapper published content to `npm pack` of these dirs). REQUIRED and fail-closed -- see below")
    ns = ap.parse_args(argv)
    # S9 ROOT FIX (codex pass-5): `--checkout` is now argparse-REQUIRED. Before, OMITTING it entirely
    # left ns.checkout=None -> verify() ran with checkout=None -> a nominally-passing R-NI with
    # plugin/wrapper `content_bound: false` (a vacuous record). `--checkout ""` already exited 2, but the
    # OMISSION path exited 0 -- the residual codex pass-5 hole. Now the flag MUST be present (required=True),
    # and its value MUST be a real, non-empty, existing directory -- an empty/missing/omitted value is a
    # hard error, never a silent content-binding skip. (Its PRESENCE + exact value in the release path is
    # ALSO pinned by the workflow lint's exact-command pin; this is the runtime fail-closed half.)
    if not str(ns.checkout).strip():
        print("verify_npm_identity: --checkout must be a non-empty directory -- an empty --checkout would "
              "disable the plugin/wrapper content binding (a vacuous R-NI); refused", file=sys.stderr)
        return 2
    if not Path(ns.checkout).is_dir():
        print(f"verify_npm_identity: --checkout {ns.checkout!r} is not a directory", file=sys.stderr)
        return 2
    manifest = json.loads(Path(ns.manifest).read_text(encoding="utf-8"))
    out = Path(ns.out)
    work = Path(ns.work) if ns.work else out.parent / "npm-identity-work"
    fetch = dir_fetcher(Path(ns.tarball_dir)) if ns.tarball_dir else registry_fetcher(ns.registry)
    rec = verify(manifest, Path(ns.dist), fetch, work, dict(os.environ), checkout=Path(ns.checkout))
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(rec, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"R-NI: {rec['verdict']} -> {out}")
    for pkg, b in rec["targets"].items():
        print(f"  {pkg}@{rec['version']}: {b['verdict']} tarball={str(b.get('published_tarball_sha256'))[:12]}" + (f" problems={b['problems']}" if b["problems"] else ""))
    return 0 if rec["verdict"] == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
