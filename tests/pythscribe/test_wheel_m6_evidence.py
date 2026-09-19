"""M6 gates (spec 13-09-26-lib-selfcontained-pip-wheel; plan M6.1-M6.5; validation §E, §K, §L) -- the LOCALLY
verifiable subset of the release evidence model, every gate with its PAIRED NEGATIVE CONTROL (the anti-vacuity paired-control convention),
mutation-verified (RED-on-revert) on FIXTURE build outputs. The live publish / registry / TestPyPI steps are CI-only.

  M6.1 assemble_manifest: fixture wheels + B_t archives + payloads + sdist -> a manifest that round-trips through
       require_evidence (shape, self-hash, release-run binding on a stubbed jobs API). Controls: wheel binary != B_t;
       vendored runtime byte-diff; a missing target; a foreign wheel; --tag != v<V>; a final V with a mismatched
       compiler pin; a hand-edited (tampered) manifest; the circularity guard (a TRACKED manifest -> refused).
  C4   stamp idempotence: `node npm/publish.mjs --check` GREEN on the committed package.json files; Python's
       stamped_form == Node's JSON.stringify form on every file (differential); an escaped em dash / CRLF -> RED.
  E1   verify_npm_identity on fixture tarballs: GREEN when every package matches; E1b a platform package whose
       bin/pyths != manifest/wheel (the alreadyPublished skip) -> RED naming the package; E1a runtime byte-diff ->
       RED; a missing package -> RED; a wrapper with a stale pin -> RED; tarball != manifest npm[...] -> RED; and
       the CONSUMER refuses an R-NI whose per-package verdict lies (sha mismatch recorded).
  E2/E3 R-TP / R-TV shapes + verify_dist_manifest + the promotion gate at BOTH entrypoints (testpypi: R-BA+R-NI;
       pypi: all four): missing / failed / partial-target / wrong-run / different-manifest-hash / all-features
       absent -> refused; one wheel byte mutated after R-TP -> RED; sdist substituted -> RED.
  K    readme_spots: static GREEN on the real README; `pythscribe[nope]` -> RED; a `[web]` mention -> RED; MCP
       without "(v0.3)" -> RED; a With-Node capability in the Without-Node row -> RED; platform list vs an
       extended expected set / a dropped source note -> RED; RUN mode against the locally built wheel (dev
       compiler present) GREEN and a bogus extra in a run block -> RED.
  L    pretag_gate: the mirror-pin rule (stale tag -> RED), licence-files presence, the versions/lint/readme gates.
  WF   lint M6 rules: RED on a fail-closed manifest placeholder, a missing npm-identity, an ungated release job,
       a testpypi gate that needs R-TP, a pypi gate missing R-TV, a rebuild in publish-pypi.yml, a 4-target
       validate matrix, a missing --rtp binding, a wheel leg without the README spots.
"""
from __future__ import annotations

import copy
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path

import pytest
import yaml

from conftest import REPO, gate
from pythscribe.artifacts import manifest_self_hash
from pythscribe.build._native import EXPECTED_WHEEL_SET, TRIPLE_TO_TAG, host_wheel_tag
from test_wheel_m4_acceptance import SHA_OTHER, SHA_S, StubAPI, env_run, job, prereq_jobs, run_rec

SCRIPTS = REPO / "scripts"
sys.path.insert(0, str(SCRIPTS))
import assemble_manifest as am  # noqa: E402
import assemble_rtp as rtp  # noqa: E402
import assemble_rtv as rtv  # noqa: E402
import lint_release_workflows as wl  # noqa: E402
import pretag_gate as pg  # noqa: E402
import readme_spots as rs  # noqa: E402
import require_evidence as re_  # noqa: E402
import set_version as sv  # noqa: E402
import testpypi_validate as tpv  # noqa: E402
import verify_dist_manifest as vdm  # noqa: E402
import verify_npm_identity as vni  # noqa: E402

TRIPLES = sorted(TRIPLE_TO_TAG)
VERSION = "0.2.5"
RUN_ID = 1001
NODE = shutil.which("node")
SKIP_PACKAGING = os.environ.get("PYTHSCRIBE_SKIP_PACKAGING") == "1"


def _sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def _binname(triple: str) -> str:
    return "pyths.exe" if "windows" in triple else "pyths"


# ----------------------------------------------------------------------------- fixture build outputs


RUNTIME_FILES = {"package.json": b'{"name":"pyths-runtime","version":"' + VERSION.encode() + b'"}\n', "src/core.js": b"export const core = 1;\n",
                 "src/core.js.map": b'{"version":3}\n', "LICENSE": b"MIT License\n"}
SCAFFOLDER_FILES = {"package.json": b'{"name":"create-pyths-app","version":"' + VERSION.encode() + b'"}\n', "index.js": b"console.log('scaffold');\n", "LICENSE": b"MIT License\n"}


def _tgz(path: Path, files: dict[str, bytes]) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(path, "w:gz") as tf:
        for rel, data in sorted(files.items()):
            ti = tarfile.TarInfo("package/" + rel)
            ti.size = len(data)
            tf.addfile(ti, io.BytesIO(data))
    return path


def _files_json(files: dict[str, bytes]) -> dict[str, str]:
    return {k: _sha(v) for k, v in sorted(files.items())}


def make_wheel(path: Path, *, triple: str, binary: bytes, version: str = VERSION, pin_version: str = VERSION,
               runtime: dict[str, bytes] = RUNTIME_FILES, scaffolder: dict[str, bytes] = SCAFFOLDER_FILES,
               extras=("gradio", "streamlit", "all", "test", "web-bundled")) -> Path:  # 0.2.9: no `server` extra
    path.parent.mkdir(parents=True, exist_ok=True)
    meta = f"Metadata-Version: 2.4\nName: pythscribe\nVersion: {version}\n" + "".join(f"Provides-Extra: {e}\n" for e in extras)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("pythscribe/__init__.py", "")
        z.writestr("pythscribe/_pin.py", f'COMPILER_VERSION = "{pin_version}"\nCOMPILER_COMMIT = "{"c" * 40}"\n')
        z.writestr(f"pythscribe/_bin/{_binname(triple)}", binary)
        for rel, data in runtime.items():
            z.writestr(f"pythscribe/_runtime/pyths-runtime/{rel}", data)
        for rel, data in scaffolder.items():  # B5: the wheel vendors the scaffolder tree too
            z.writestr(f"pythscribe/_web/create-pyths-app/{rel}", data)
        z.writestr(f"pythscribe-{version}.dist-info/METADATA", meta)
    return path


def make_build_outputs(root: Path, *, version: str = VERSION, pin_version: str = VERSION, binaries: dict[str, bytes] | None = None,
                       wheel_binaries: dict[str, bytes] | None = None, runtime: dict[str, bytes] = RUNTIME_FILES,
                       wheel_runtime: dict[str, bytes] | None = None, wheel_scaffolder: dict[str, bytes] | None = None) -> tuple[Path, Path]:
    """dist/ (5 wheels + sdist + prepared payloads) and artifacts/ (pyths-<triple>/<archive>) as the manifest job sees them."""
    binaries = binaries or {t: f"B_t::{t}".encode() for t in TRIPLES}
    wheel_binaries = wheel_binaries or binaries
    dist, arts = root / "dist", root / "artifacts"
    dist.mkdir(parents=True, exist_ok=True)
    for t in TRIPLES:
        make_wheel(dist / f"pythscribe-{version}-py3-none-{TRIPLE_TO_TAG[t]}.whl", triple=t, binary=wheel_binaries[t], version=version,
                   pin_version=pin_version, runtime=wheel_runtime or runtime, scaffolder=wheel_scaffolder or SCAFFOLDER_FILES)
        d = arts / f"pyths-{t}"
        d.mkdir(parents=True)
        if "windows" in t:
            with zipfile.ZipFile(d / f"pyths-{t}.zip", "w") as z:
                z.writestr("pyths.exe", binaries[t])
        else:
            with tarfile.open(d / f"pyths-{t}.tar.gz", "w:gz") as tf:
                ti = tarfile.TarInfo("pyths")
                ti.size = len(binaries[t])
                tf.addfile(ti, io.BytesIO(binaries[t]))
    with tarfile.open(dist / f"pythscribe-{version}.tar.gz", "w:gz") as tf:
        ti = tarfile.TarInfo(f"pythscribe-{version}/PKG-INFO")
        data = f"Name: pythscribe\nVersion: {version}\n".encode()
        ti.size = len(data)
        tf.addfile(ti, io.BytesIO(data))
    _tgz(dist / "runtime-payload.tgz", runtime)
    (dist / "runtime-payload.files.json").write_text(json.dumps(_files_json(runtime), indent=2) + "\n", encoding="utf-8")
    _tgz(dist / "scaffolder-payload.tgz", SCAFFOLDER_FILES)
    (dist / "scaffolder-payload.files.json").write_text(json.dumps(_files_json(SCAFFOLDER_FILES), indent=2) + "\n", encoding="utf-8")
    return dist, arts


def assemble(root: Path, **kw) -> tuple[dict, Path, Path]:
    dist, arts = make_build_outputs(root, **{k: v for k, v in kw.items() if k in ("version", "pin_version", "binaries", "wheel_binaries", "runtime", "wheel_runtime", "wheel_scaffolder")})
    m = am.assemble(dist=dist, binaries=arts, source_sha=SHA_S, run_id=RUN_ID, run_attempt=1, tag=kw.get("tag", f"v{kw.get('version', VERSION)}"))
    return m, dist, arts


def _write_json(p: Path, obj) -> Path:
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(obj, indent=2), encoding="utf-8")
    return p


# ============================================================================ M6.1: the manifest producer


def test_m61_manifest_assembles_binds_and_round_trips_through_the_consumer(tmp_path):
    m, dist, arts = assemble(tmp_path)
    assert m["schema"] == re_.MANIFEST_SCHEMA and m["version"] == VERSION and m["tag"] == f"v{VERSION}"
    assert m["manifest_sha256"] == manifest_self_hash(m) and re_.check_manifest_shape(m) == []
    for t in TRIPLES:
        info = m["targets"][t]
        assert info["native_sha256"] == _sha(f"B_t::{t}".encode()) and info["wheel_tag"] == TRIPLE_TO_TAG[t]
        assert info["wheel_sha256"] == _sha((dist / info["wheel_filename"]).read_bytes())
    assert m["runtime_payload"]["files"] == _files_json(RUNTIME_FILES) and m["scaffolder_payload"]["files"] == _files_json(SCAFFOLDER_FILES)
    assert m["runtime_payload"]["tarball_sha256"] == _sha((dist / "runtime-payload.tgz").read_bytes())
    assert m["npm"]["pyths-runtime"] == m["runtime_payload"]["tarball_sha256"] and m["npm"]["pythscribe"] is None
    assert set(m["npm"]) == re_.NPM_PACKAGES and set(m["expected_wheel_set"]) == EXPECTED_WHEEL_SET
    assert m["compiler"] == {"version": VERSION, "commit": "c" * 40} and m["prerequisite_jobs"] == sorted(re_.REQUIRED_PREREQ_JOBS)
    # the SAME-RUN consumer accepts it (stubbed jobs API; the run's own conclusion is never read)
    api = StubAPI(jobs={RUN_ID: prereq_jobs()}, forbid_run=True)
    assert re_.verify_manifest_binding(m, "release-run", env=env_run(RUN_ID), api=api) == []
    # written by the CLI and re-read: byte-stable, self-hash intact
    out = tmp_path / "release_manifest.json"
    r = subprocess.run([sys.executable, str(SCRIPTS / "assemble_manifest.py"), "--dist", str(dist), "--binaries", str(arts), "--out", str(out), "--tag", f"v{VERSION}",
                        "--source-sha", SHA_S, "--run-id", str(RUN_ID), "--run-attempt", "1", "--checkout", str(tmp_path)], capture_output=True, text=True)
    assert r.returncode == 0 and "GREEN" in r.stdout, (r.stdout, r.stderr)
    m2 = json.loads(out.read_text(encoding="utf-8"))
    assert m2["manifest_sha256"] == manifest_self_hash(m2) and m2["targets"] == m["targets"]
    # a hand-edit after the fact is a self-hash mismatch at every consumer
    m2["version"] = "9.9.9"
    assert any("self-hash mismatch" in x for x in re_.check_manifest_shape(m2))


def test_m61_controls_each_broken_binding_is_red(tmp_path):
    t0 = TRIPLES[0]
    # wheel binary != B_t (a post-finalize rewrite / a stale wheel)
    with pytest.raises(am.ManifestError, match="binary inside .* != finalized B_t"):
        assemble(tmp_path / "a", wheel_binaries={**{t: f"B_t::{t}".encode() for t in TRIPLES}, t0: b"rewritten"})
    # vendored runtime inside a wheel != the prepared payload (one byte)
    with pytest.raises(am.ManifestError, match="vendored runtime inside .* != runtime-payload.files.json"):
        assemble(tmp_path / "b", wheel_runtime={**RUNTIME_FILES, "src/core.js": b"export const core = 2;\n"})
    # B5: vendored SCAFFOLDER inside a wheel != the prepared scaffolder payload (one byte) -> RED
    with pytest.raises(am.ManifestError, match="vendored scaffolder inside .* != scaffolder-payload.files.json"):
        assemble(tmp_path / "b_sc", wheel_scaffolder={**SCAFFOLDER_FILES, "index.js": b"console.log('scaffold B');\n"})
    # a missing target wheel
    dist, arts = make_build_outputs(tmp_path / "c")
    (dist / f"pythscribe-{VERSION}-py3-none-{TRIPLE_TO_TAG[t0]}.whl").unlink()
    with pytest.raises(am.ManifestError, match="missing from .* a missing target is RED"):
        am.assemble(dist=dist, binaries=arts, source_sha=SHA_S, run_id=RUN_ID, run_attempt=1, tag=f"v{VERSION}")
    # a foreign wheel in dist (a stale py3-none-any from a source build)
    dist, arts = make_build_outputs(tmp_path / "d")
    make_wheel(dist / f"pythscribe-{VERSION}-py3-none-any.whl", triple=t0, binary=b"x")
    with pytest.raises(am.ManifestError, match="foreign wheels"):
        am.assemble(dist=dist, binaries=arts, source_sha=SHA_S, run_id=RUN_ID, run_attempt=1, tag=f"v{VERSION}")
    # --tag disagrees with the distribution version
    with pytest.raises(am.ManifestError, match="--tag .* != v<distribution version>"):
        assemble(tmp_path / "e", tag="v9.9.9")
    # a FINAL release whose wheels ship another compiler pin (set_version.py not run)
    with pytest.raises(am.ManifestError, match="COMPILER_VERSION pin is '0.2.4'"):
        assemble(tmp_path / "f", pin_version="0.2.4")
    # a pre-release may lead the pin (check_versions.py allows it in dev) -- recorded, not refused
    m, _, _ = assemble(tmp_path / "g", version="0.2.5a0", pin_version="0.2.4", tag="v0.2.5a0")
    assert m["compiler"]["version"] == "0.2.4" and m["version"] == "0.2.5a0"
    # a missing B_t archive
    dist, arts = make_build_outputs(tmp_path / "h")
    shutil.rmtree(arts / f"pyths-{t0}")
    with pytest.raises(am.ManifestError, match="no finalized binary"):
        am.assemble(dist=dist, binaries=arts, source_sha=SHA_S, run_id=RUN_ID, run_attempt=1, tag=f"v{VERSION}")
    # identity inputs
    with pytest.raises(am.ManifestError, match="not a 40-hex commit SHA"):
        am.assemble(dist=dist, binaries=arts, source_sha="", run_id=RUN_ID, run_attempt=1, tag=None)


def test_m61_circularity_guard_refuses_a_tracked_manifest(tmp_path):
    repo = tmp_path / "repo"
    repo.mkdir()
    subprocess.run(["git", "init", "-q", str(repo)], check=True)
    subprocess.run(["git", "-C", str(repo), "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"], check=True)
    tracked = repo / "release_manifest.json"
    tracked.write_text("{}", encoding="utf-8")
    assert am.is_tracked(repo, tracked) is False  # present but untracked: fine
    subprocess.run(["git", "-C", str(repo), "add", "release_manifest.json"], check=True)
    subprocess.run(["git", "-C", str(repo), "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "manifest"], check=True)
    assert am.is_tracked(repo, tracked) is True
    dist, arts = make_build_outputs(tmp_path / "o")
    r = subprocess.run([sys.executable, str(SCRIPTS / "assemble_manifest.py"), "--dist", str(dist), "--binaries", str(arts), "--out", str(tracked), "--tag", f"v{VERSION}",
                        "--source-sha", SHA_S, "--run-id", str(RUN_ID), "--run-attempt", "1", "--checkout", str(repo)], capture_output=True, text=True)
    assert r.returncode == 1 and "TRACKED" in r.stderr and "circularity" in r.stderr, r.stderr
    assert tracked.read_text(encoding="utf-8") == "{}"  # nothing overwritten
    # the real checkout: release_manifest.json is NOT a tracked file (and is gitignored)
    assert am.is_tracked(REPO, REPO / "release_manifest.json") is False
    r = subprocess.run(["git", "-C", str(REPO), "check-ignore", "-q", "release_manifest.json"], capture_output=True)
    assert r.returncode == 0, "release_manifest.json must be gitignored"


# ============================================================================ C4: stamp idempotence


def test_c4_stamped_form_python_equals_node_json_stringify_on_every_package_json():
    """Differential: set_version.stamped_form == JSON.stringify(j, null, 2) + "\\n" (Node) on every enumerated file,
    and the committed files are ALREADY in that form (the tag-ref --check passes)."""
    gate(NODE is not None, "node required for the JSON.stringify oracle")
    for rel in sv.PKG_JSON:
        raw = (REPO / rel).read_bytes()
        py = sv.stamped_form(raw.decode("utf-8"))
        r = subprocess.run([NODE, "-e", "const fs=require('fs');process.stdout.write(JSON.stringify(JSON.parse(fs.readFileSync(process.argv[1],'utf8')),null,2)+'\\n')", str(REPO / rel)],
                           capture_output=True, check=True)
        assert r.stdout.decode("utf-8") == py, rel
        assert raw == py.encode("utf-8"), f"{rel} is not committed in stamped form (run scripts/set_version.py --restamp)"
    r = subprocess.run([NODE, str(REPO / "npm" / "publish.mjs"), "--check"], capture_output=True, text=True, env={**os.environ, "GITHUB_REF_NAME": "v" + json.loads((REPO / "npm" / "pythscribe" / "package.json").read_text())["version"]})
    assert r.returncode == 0 and "GREEN" in r.stdout, (r.stdout, r.stderr)


@pytest.mark.parametrize("mutation", ["escaped-em-dash", "crlf", "trailing-space", "runtime-escaped-em-dash", "runtime-stale-version"])
def test_c4_control_a_byte_stamping_would_change_is_red(mutation):
    """The §C4 control: a package.json (the wrapper, OR the payload package `runtime/package.json`, which publish.mjs
    never rewrites but MUST check) that still carries `\\u2014`, a stale version, CRLF or any other byte -> RED."""
    gate(NODE is not None, "node required for publish.mjs --check")
    target = REPO / ("runtime" if mutation.startswith("runtime-") else "npm/pythscribe") / "package.json"
    original = target.read_bytes()
    text = original.decode("utf-8")
    if mutation.endswith("escaped-em-dash"):
        assert "—" in text
        mutated = text.replace("—", "\\u2014", 1).encode("utf-8")
    elif mutation == "runtime-stale-version":
        mutated = re.sub(r'"version": "[^"]+"', '"version": "0.0.1"', text, count=1).encode("utf-8")
    elif mutation == "crlf":
        mutated = text.replace("\n", "\r\n").encode("utf-8")
    else:
        mutated = text.replace('"name": "pythscribe",', '"name": "pythscribe", ', 1).encode("utf-8")
    assert mutated != original
    env = {**os.environ, "GITHUB_REF_NAME": "v" + json.loads((REPO / "npm" / "pythscribe" / "package.json").read_text(encoding="utf-8"))["version"]}
    try:
        target.write_bytes(mutated)
        r = subprocess.run([NODE, str(REPO / "npm" / "publish.mjs"), "--check"], capture_output=True, text=True, env=env)
        assert r.returncode == 1 and "stamping would change it" in r.stderr and "set_version.py" in r.stderr, (r.stdout, r.stderr)
        assert target.read_bytes() == mutated  # --check never rewrites
    finally:
        target.write_bytes(original)
    r = subprocess.run([NODE, str(REPO / "npm" / "publish.mjs"), "--check"], capture_output=True, text=True, env=env)
    assert r.returncode == 0


def test_b1_publish_mjs_refuses_non_tag_and_version_mismatch(tmp_path):
    """B1: publish.mjs --yes is reachable ONLY from a tag ref, requires --manifest, and refuses a
    version != the manifest. Every case exits at the guard BEFORE any real publish (safe to run)."""
    gate(NODE is not None, "node required for publish.mjs")
    pub_mjs = str(REPO / "npm" / "publish.mjs")
    wrapper_v = json.loads((REPO / "npm" / "pythscribe" / "package.json").read_text(encoding="utf-8"))["version"]
    mf = _write_json(tmp_path / "release_manifest.json", {"version": wrapper_v})
    base = {k: v for k, v in os.environ.items() if k not in ("GITHUB_REF", "GITHUB_REF_NAME")}
    # --yes with NO tag ref -> refused
    r = subprocess.run([NODE, pub_mjs, "--yes", "--manifest", str(mf)], capture_output=True, text=True, env=base)
    assert r.returncode == 1 and "requires a tag ref" in r.stderr, (r.stdout, r.stderr)
    tag_env = {**base, "GITHUB_REF": f"refs/tags/v{wrapper_v}", "GITHUB_REF_NAME": f"v{wrapper_v}", "CI": "true"}
    # --yes on a tag ref but WITHOUT --manifest -> refused
    r = subprocess.run([NODE, pub_mjs, "--yes"], capture_output=True, text=True, env=tag_env)
    assert r.returncode == 1 and "requires --manifest" in r.stderr, (r.stdout, r.stderr)
    # --yes on a tag ref but the manifest version disagrees -> refused
    mf2 = _write_json(tmp_path / "m2.json", {"version": "9.9.9"})
    r = subprocess.run([NODE, pub_mjs, "--yes", "--manifest", str(mf2)], capture_output=True, text=True, env=tag_env)
    assert r.returncode == 1 and "!= release_manifest.json version" in r.stderr, (r.stdout, r.stderr)


def test_c4_restamp_rewrites_only_non_stamped_files(tmp_path, monkeypatch):
    root = tmp_path / "repo"
    for rel in sv.PKG_JSON:
        (root / rel).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(REPO / rel, root / rel)
    monkeypatch.setattr(sv, "ROOT", root)
    (root / sv.PKG_JSON[0]).write_bytes((root / sv.PKG_JSON[0]).read_bytes().replace(b"\n", b"\r\n"))
    assert sv.restamp() == 0
    assert b"\r" not in (root / sv.PKG_JSON[0]).read_bytes()
    for rel in sv.PKG_JSON:
        assert (root / rel).read_bytes() == (REPO / rel).read_bytes()


# ============================================================================ E1: npm identity (R-NI)


def npm_tarballs(root: Path, m: dict, *, binaries: dict[str, bytes] | None = None, runtime: dict[str, bytes] = RUNTIME_FILES,
                 wrapper_pin: str | None = None, skip: tuple[str, ...] = ()) -> Path:
    """The published packages at V as `npm pack` would name them."""
    d = root / "tarballs"
    binaries = binaries or {t: f"B_t::{t}".encode() for t in TRIPLES}
    v = m["version"]
    for pkg, triple in re_.NPM_PLATFORM_TO_TRIPLE.items():
        if pkg in skip:
            continue
        _tgz(d / vni.pack_name(pkg, v), {"package.json": json.dumps({"name": pkg, "version": v}).encode(), f"bin/{_binname(triple)}": binaries[triple], "LICENSE.md": b"FSL\n"})
    if "pyths-runtime" not in skip:
        _tgz(d / vni.pack_name("pyths-runtime", v), runtime)
    if "create-pyths-app" not in skip:
        _tgz(d / vni.pack_name("create-pyths-app", v), SCAFFOLDER_FILES)
    for plugin in sorted(re_.NPM_PLUGIN_PACKAGES):  # S9: the pure-JS plugin packages publish.mjs also ships
        if plugin not in skip:
            _tgz(d / vni.pack_name(plugin, v), {"package.json": json.dumps({"name": plugin, "version": v}).encode(), "index.js": b"module.exports = {};\n"})
    if "pythscribe" not in skip:
        pj = {"name": "pythscribe", "version": v, "dependencies": {"pyths-runtime": f"^{v}"}, "optionalDependencies": {p: (wrapper_pin or v) for p in re_.NPM_PLATFORM_TO_TRIPLE}}
        _tgz(d / vni.pack_name("pythscribe", v), {"package.json": json.dumps(pj).encode(), "bin/pyths.js": b"#!/usr/bin/env node\n"})
    return d


def _expected_content(d: Path, m: dict) -> dict[str, dict[str, str]]:
    """S9 (codex pass-3): the checkout content of each plugin/wrapper package = the members of the
    tarball packed for it (offline stand-in for `npm pack` of the checkout dir of S). Tolerant: only
    packages whose tarball exists in `d` (so a skip/absent-package RED case fails on the fetch, not
    here). Injecting this makes `content_bound: True` so the mandatory consumer binding is satisfied
    on the happy path -- exactly what release.yml's `--checkout .` produces in CI."""
    out: dict[str, dict[str, str]] = {}
    for pkg in (*re_.NPM_PLUGIN_PACKAGES, re_.NPM_WRAPPER):
        tgz = d / vni.pack_name(pkg, m["version"])
        if tgz.is_file():
            out[pkg] = {rel: vni.sha256_bytes(b) for rel, b in vni.tarball_members(tgz).items()}
    return out


def _rni(root: Path, m: dict, dist: Path, **kw) -> dict:
    d = npm_tarballs(root, m, **kw)
    return vni.verify(m, dist, vni.dir_fetcher(d), root / "work", env_run(RUN_ID), expected_content=_expected_content(d, m))


def _cli_checkout(d: Path, checkout: Path, m: dict) -> Path:
    """Fix 3 (codex pass-5): verify_npm_identity.py's `--checkout` is now argparse-REQUIRED (an omitted
    flag was the residual vacuous-R-NI hole). So the CLI smokes below must pass a real checkout. Build
    minimal plugin/wrapper package dirs under `checkout` and `npm pack` each INTO `d` (the tarball-dir the
    CLI fetches from), so the CLI's mandatory content binding is satisfied offline BY CONSTRUCTION --
    registry tarball == `npm pack` of the checkout dir, exactly as release.yml's `--checkout .` yields in
    CI. Requires npm (skip cleanly if absent -- CI's pip-gate job has it)."""
    npm = shutil.which("npm") or shutil.which("npm.cmd")
    if not npm:
        pytest.skip("npm not on PATH -- required for the verify_npm_identity --checkout content binding")
    v = m["version"]
    for pkg, rel in re_.NPM_PLUGIN_WRAPPER_DIRS.items():
        pdir = checkout / rel
        pdir.mkdir(parents=True, exist_ok=True)
        if pkg == re_.NPM_WRAPPER:
            pj = {"name": pkg, "version": v, "dependencies": {"pyths-runtime": f"^{v}"},
                  "optionalDependencies": {p: v for p in re_.NPM_PLATFORM_TO_TRIPLE}}
        else:
            pj = {"name": pkg, "version": v}
        (pdir / "package.json").write_text(json.dumps(pj), encoding="utf-8")
        (pdir / "index.js").write_text("module.exports = {};\n", encoding="utf-8")
        r = subprocess.run([npm, "pack", "--pack-destination", str(d)], cwd=str(pdir),
                           capture_output=True, text=True, shell=(os.name == "nt"))
        assert r.returncode == 0, (pkg, r.stderr or r.stdout)
    return checkout


def test_e1_identity_green_when_every_published_package_matches(tmp_path):
    m, dist, _ = assemble(tmp_path)
    # the prepared payload tarball IS the published artifact: make the fixture tarball the same bytes
    rec = _rni(tmp_path, m, dist)
    # the fixture's runtime tarball is a fresh gzip (different bytes than dist/runtime-payload.tgz) -> RED on the tarball
    # hash binding; copy the prepared tarball in (what publish.mjs publishes) -> GREEN
    assert rec["targets"]["pyths-runtime"]["verdict"] == "fail" and any("npm[pyths-runtime]" in x for x in rec["targets"]["pyths-runtime"]["problems"])
    d = tmp_path / "tarballs"
    shutil.copy(dist / "runtime-payload.tgz", d / vni.pack_name("pyths-runtime", VERSION))
    shutil.copy(dist / "scaffolder-payload.tgz", d / vni.pack_name("create-pyths-app", VERSION))
    rec = vni.verify(m, dist, vni.dir_fetcher(d), tmp_path / "work", env_run(RUN_ID), expected_content=_expected_content(d, m))
    assert rec["verdict"] == "pass" and set(rec["targets"]) == re_.NPM_PACKAGES, {k: v["problems"] for k, v in rec["targets"].items() if v["problems"]}
    for pkg, triple in re_.NPM_PLATFORM_TO_TRIPLE.items():
        b = rec["targets"][pkg]
        assert b["kind"] == "platform" and b["platform"]["binary_sha256"] == m["targets"][triple]["native_sha256"] == b["platform"]["wheel_binary_sha256"]
    rt = rec["targets"]["pyths-runtime"]
    assert rt["payload"]["files_sha256"] == re_.files_map_sha256(m["runtime_payload"]["files"]) and all(rt["payload"]["wheels_vendored_match"].values())
    assert rec["producer"]["run_id"] == RUN_ID and rec["record"] == "R-NI"
    assert re_.check_record("R-NI", rec, m, source_sha=SHA_S) == []
    # the CLI: writes the record, exit 0 (--checkout is REQUIRED now; content-bound offline via npm pack)
    out = tmp_path / "ev" / "R-NI.json"
    checkout = _cli_checkout(d, tmp_path / "checkout", m)
    r = subprocess.run([sys.executable, str(SCRIPTS / "verify_npm_identity.py"), "--manifest", str(_write_json(tmp_path / "m.json", m)), "--dist", str(dist), "--out", str(out), "--tarball-dir", str(d), "--checkout", str(checkout)],
                       capture_output=True, text=True, env={**os.environ, **env_run(RUN_ID)})
    assert r.returncode == 0 and "R-NI: pass" in r.stdout and json.loads(out.read_text())["verdict"] == "pass", (r.stdout, r.stderr)


def test_e1b_platform_package_holding_another_binary_at_v_is_red(tmp_path):
    """The exact codex scenario: npm holds binary A at V (publish.mjs SKIPPED it as alreadyPublished), the wheel +
    manifest carry B, the runtimes match -> RED naming that package; the consumer refuses the record too."""
    m, dist, _ = assemble(tmp_path)
    plat = "@pythscribe/cli-darwin-arm64"
    old = {t: f"B_t::{t}".encode() for t in TRIPLES}
    old[re_.NPM_PLATFORM_TO_TRIPLE[plat]] = b"binary A (stale, already on npm)"
    d = npm_tarballs(tmp_path, m, binaries=old)
    shutil.copy(dist / "runtime-payload.tgz", d / vni.pack_name("pyths-runtime", VERSION))
    shutil.copy(dist / "scaffolder-payload.tgz", d / vni.pack_name("create-pyths-app", VERSION))
    rec = vni.verify(m, dist, vni.dir_fetcher(d), tmp_path / "work", env_run(RUN_ID))
    assert rec["verdict"] == "fail" and rec["targets"][plat]["verdict"] == "fail"
    assert any("same-version-different-binary" in x for x in rec["targets"][plat]["problems"]), rec["targets"][plat]["problems"]
    assert all(b["verdict"] == "pass" for p, b in rec["targets"].items() if p != plat)
    assert any("R-NI: verdict 'fail'" in x for x in re_.check_record("R-NI", rec, m, source_sha=SHA_S))
    # a LYING producer: verdict flipped to pass but the recorded sha is the real one -> the consumer's own binding refuses
    lie = copy.deepcopy(rec)
    lie["verdict"] = "pass"
    lie["targets"][plat]["verdict"] = "pass"
    assert any(f"R-NI[{plat}]: published bin/pyths sha256" in x and "alreadyPublished" in x for x in re_.check_record("R-NI", lie, m, source_sha=SHA_S))
    # the CLI exit code (--checkout REQUIRED now; plugins/wrapper content-bound so the ONLY failure is the binary)
    out = tmp_path / "ev" / "R-NI.json"
    checkout = _cli_checkout(d, tmp_path / "checkout", m)
    r = subprocess.run([sys.executable, str(SCRIPTS / "verify_npm_identity.py"), "--manifest", str(_write_json(tmp_path / "m.json", m)), "--dist", str(dist), "--out", str(out), "--tarball-dir", str(d), "--checkout", str(checkout)],
                       capture_output=True, text=True, env={**os.environ, **env_run(RUN_ID)})
    assert r.returncode == 1 and "R-NI: fail" in r.stdout


def test_e1a_runtime_payload_byte_diff_missing_package_and_stale_wrapper_pin_are_red(tmp_path):
    m, dist, _ = assemble(tmp_path)
    # E1a: runtime B in the wheel/manifest while npm serves A (one byte)
    d = npm_tarballs(tmp_path / "a", m, runtime={**RUNTIME_FILES, "src/core.js": b"export const core = 1; // A\n"})
    shutil.copy(dist / "scaffolder-payload.tgz", d / vni.pack_name("create-pyths-app", VERSION))
    rec = vni.verify(m, dist, vni.dir_fetcher(d), tmp_path / "a" / "work", env_run(RUN_ID))
    pr = rec["targets"]["pyths-runtime"]["problems"]
    assert rec["verdict"] == "fail" and any("CHANGED bytes ['src/core.js']" in x for x in pr), pr
    lie = copy.deepcopy(rec)
    lie["verdict"] = lie["targets"]["pyths-runtime"]["verdict"] = "pass"
    assert any("R-NI[pyths-runtime]: published payload files hash" in x for x in re_.check_record("R-NI", lie, m, source_sha=SHA_S))
    # a package absent from the registry (never published) -> RED for that package
    d = npm_tarballs(tmp_path / "b", m, skip=("@pythscribe/cli-win32-x64",))
    rec = vni.verify(m, dist, vni.dir_fetcher(d), tmp_path / "b" / "work", env_run(RUN_ID))
    assert rec["verdict"] == "fail" and any("not published" in x for x in rec["targets"]["@pythscribe/cli-win32-x64"]["problems"])
    assert rec["targets"]["@pythscribe/cli-win32-x64"]["published_tarball_sha256"] is None
    # a wrapper whose optionalDependencies pin a stale version -> RED
    d = npm_tarballs(tmp_path / "c", m, wrapper_pin="0.2.4")
    rec = vni.verify(m, dist, vni.dir_fetcher(d), tmp_path / "c" / "work", env_run(RUN_ID))
    assert any("expected the exact pin" in x for x in rec["targets"]["pythscribe"]["problems"])
    # a wheel whose vendored runtime drifted from the manifest is caught from the runtime package's side too
    dist2, _ = make_build_outputs(tmp_path / "d", wheel_runtime={**RUNTIME_FILES, "src/core.js.map": b'{"version":4}\n'})
    d = npm_tarballs(tmp_path / "d", m)
    shutil.copy(dist / "runtime-payload.tgz", d / vni.pack_name("pyths-runtime", VERSION))
    rec = vni.verify(m, dist2, vni.dir_fetcher(d), tmp_path / "d" / "work", env_run(RUN_ID))
    assert any("vendored pyths-runtime inside" in x for x in rec["targets"]["pyths-runtime"]["problems"])
    # B5: a wheel whose vendored SCAFFOLDER drifted from the manifest is caught from create-pyths-app's
    # side (npm ships scaffolder A while the wheel vendors B -- the previously-open chain)
    dist3, _ = make_build_outputs(tmp_path / "e", wheel_scaffolder={**SCAFFOLDER_FILES, "index.js": b"console.log('scaffold B');\n"})
    d = npm_tarballs(tmp_path / "e", m)
    shutil.copy(dist / "runtime-payload.tgz", d / vni.pack_name("pyths-runtime", VERSION))
    shutil.copy(dist / "scaffolder-payload.tgz", d / vni.pack_name("create-pyths-app", VERSION))
    rec = vni.verify(m, dist3, vni.dir_fetcher(d), tmp_path / "e" / "work", env_run(RUN_ID))
    ca = rec["targets"]["create-pyths-app"]
    assert ca["verdict"] == "fail" and any("vendored create-pyths-app inside" in x for x in ca["problems"]), ca["problems"]
    assert not all(ca["payload"]["wheels_vendored_match"].values())


def test_e1_npm_packages_equals_publish_mjs_set():
    """S9: the publish set (npm/publish.mjs) and the R-NI evidence set (NPM_PACKAGES) derive from ONE
    STRUCTURAL authority -- npm/packages.json -- loaded by both, not a regex-linked duplicate. A package
    added to the authority but missing from either consumer (the vite/next-plugin gap that let stale
    plugins pass promotion) is caught here."""
    auth = json.loads((REPO / "npm" / "packages.json").read_text(encoding="utf-8"))
    published = set(auth["platform"]) | set(auth["payload"]) | set(auth["plugins"]) | {auth["wrapper"]}
    assert published == set(re_.NPM_PACKAGES), f"packages.json set != NPM_PACKAGES; symmetric diff {published ^ set(re_.NPM_PACKAGES)}"
    assert re_.NPM_PLUGIN_PACKAGES == frozenset(auth["plugins"]) and re_.NPM_WRAPPER == auth["wrapper"]
    assert {"vite-plugin-pyths", "next-plugin-pyths"} == set(auth["plugins"])
    # publish.mjs loads the SAME file (structural, not a regex): it reads npm/packages.json by name
    assert 'readFileSync(join(__dirname, "packages.json")' in (REPO / "npm" / "publish.mjs").read_text(encoding="utf-8")


def test_s9_rni_covers_the_plugin_packages(tmp_path):
    """S9: R-NI now downloads and validates vite-plugin-pyths / next-plugin-pyths. An absent one (a
    stale plugin publish.mjs skipped) -> R-NI RED and the consumer refuses the record."""
    m, dist, _ = assemble(tmp_path)
    d = npm_tarballs(tmp_path, m)
    shutil.copy(dist / "runtime-payload.tgz", d / vni.pack_name("pyths-runtime", VERSION))
    shutil.copy(dist / "scaffolder-payload.tgz", d / vni.pack_name("create-pyths-app", VERSION))
    rec = vni.verify(m, dist, vni.dir_fetcher(d), tmp_path / "work", env_run(RUN_ID), expected_content=_expected_content(d, m))
    assert rec["verdict"] == "pass" and set(rec["targets"]) == re_.NPM_PACKAGES, {k: v["problems"] for k, v in rec["targets"].items() if v["problems"]}
    for pl in re_.NPM_PLUGIN_PACKAGES:
        assert rec["targets"][pl]["kind"] == "plugin" and rec["targets"][pl]["verdict"] == "pass"
    assert re_.check_record("R-NI", rec, m, source_sha=SHA_S) == []
    # a plugin ABSENT from the registry (the codex scenario: publish.mjs skipped a stale one) -> RED
    d2 = npm_tarballs(tmp_path / "abs", m, skip=("vite-plugin-pyths",))
    shutil.copy(dist / "runtime-payload.tgz", d2 / vni.pack_name("pyths-runtime", VERSION))
    shutil.copy(dist / "scaffolder-payload.tgz", d2 / vni.pack_name("create-pyths-app", VERSION))
    rec2 = vni.verify(m, dist, vni.dir_fetcher(d2), tmp_path / "abs" / "work", env_run(RUN_ID))
    assert rec2["verdict"] == "fail" and rec2["targets"]["vite-plugin-pyths"]["verdict"] == "fail"
    assert any(re_.check_record("R-NI", rec2, m, source_sha=SHA_S))  # the consumer refuses it too
    # a plugin at the WRONG version -> RED
    d3 = npm_tarballs(tmp_path / "ver", m, skip=("next-plugin-pyths",))
    shutil.copy(dist / "runtime-payload.tgz", d3 / vni.pack_name("pyths-runtime", VERSION))
    shutil.copy(dist / "scaffolder-payload.tgz", d3 / vni.pack_name("create-pyths-app", VERSION))
    _tgz(d3 / vni.pack_name("next-plugin-pyths", VERSION), {"package.json": json.dumps({"name": "next-plugin-pyths", "version": "0.2.4"}).encode(), "index.js": b"x\n"})
    rec3 = vni.verify(m, dist, vni.dir_fetcher(d3), tmp_path / "ver" / "work", env_run(RUN_ID))
    assert rec3["targets"]["next-plugin-pyths"]["verdict"] == "fail" and any("expected next-plugin-pyths@" in x for x in rec3["targets"]["next-plugin-pyths"]["problems"])


def test_e1_platform_map_equals_build_platform_packages_mjs_targets():
    """NPM_PLATFORM_TO_TRIPLE (the consumer's authority) == the TARGETS table of npm/build-platform-packages.mjs."""
    src = (REPO / "npm" / "build-platform-packages.mjs").read_text(encoding="utf-8")
    got = {m.group(1): m.group(2) for m in re.finditer(r'pkg:\s*"([^"]+)".*?rust:\s*"([^"]+)"', src)}
    assert got == re_.NPM_PLATFORM_TO_TRIPLE
    assert set(re_.NPM_PLATFORM_TO_TRIPLE.values()) == set(TRIPLE_TO_TAG)


# ============================================================================ E2/E3: R-TP, R-TV, verify_dist_manifest, both entrypoints


def _legs_ok(root: Path, m: dict, *, drop: dict[str, tuple[str, ...]] | None = None, fail_smoke: str | None = None) -> Path:
    legs = root / "legs"
    for t in TRIPLES:
        d = legs / t
        d.mkdir(parents=True, exist_ok=True)
        info = m["targets"][t]
        files = {
            "download.json": {"verdict": "pass", "wheel_filename": info["wheel_filename"], "installed_wheel_sha256": info["wheel_sha256"], "index_url": "https://test.pypi.org/simple/"},
            "a0.json": {"verdict": "pass", "problems": [], "topology": "native-scrubbed-path"},
            "leg.json": {"steps": {s: "pass" for s in ("A1", "A2", "A3", "A4")}, "problems": [], "installed_binary_sha256": info["native_sha256"], "a3_bits": "4008000000000000"},
            "smoke.json": {"verdict": "fail" if fail_smoke == t else "pass", "tests": tpv.SMOKE_TEST, "passed": 5, "failed": 0, "problems": [] if fail_smoke != t else ["pytest exit 1"]},
        }
        for fn, obj in files.items():
            if fn not in (drop or {}).get(t, ()):
                _write_json(d / fn, obj)
    return legs


def promotion_env(sha: str = SHA_S) -> dict[str, str]:
    return {"GITHUB_SHA": sha, "GITHUB_RUN_ID": "5005", "GITHUB_RUN_ATTEMPT": "1", "GITHUB_REPOSITORY": "swetmr/pythscribe"}


def test_e2_rtp_and_verify_dist_manifest_bind_the_promoted_bytes(tmp_path):
    m, dist, _ = assemble(tmp_path)
    pub = tmp_path / "pub"  # what the publish job downloads: the 5 wheels + sdist ONLY
    pub.mkdir()
    for name in re_.distribution_set(m):
        shutil.copy(dist / name, pub / name)
    assert vdm.verify(m, pub) == []
    # B3: the registry-served set is injected (offline). It equals the manifest -> pass.
    served = re_.distribution_set(m)
    rec = rtp.assemble(m, pub, "https://test.pypi.org/legacy/", promotion_env(), registry_files=served)
    assert rec["verdict"] == "pass" and rec["uploaded_files"] == served and rec["registry_files"] == served and rec["producer"]["workflow"] == "publish-pypi.yml"
    assert re_.check_record("R-TP", rec, m, source_sha=SHA_S) == []
    assert vdm.verify(m, pub, rec) == []
    # B3 control: local dist matches the manifest, but the registry SERVES a different sdist (a
    # pre-existing artifact skip-existing left in place). The local-only hash would pass; the registry
    # proof does not -> R-TP fail, and the consumer refuses the record.
    served_bad = {**served, m["sdist"]["filename"]: _sha(b"a foreign sdist pre-existing on TestPyPI")}
    rec_bad = rtp.assemble(m, pub, "https://test.pypi.org/legacy/", promotion_env(), registry_files=served_bad)
    assert rec_bad["verdict"] == "fail" and any("registry-served sha256" in x for x in rec_bad["problems"])
    assert any("registry_files != the manifest" in x for x in re_.check_record("R-TP", rec_bad, m, source_sha=SHA_S))
    # an unreachable/empty registry is a FAILURE, never a vacuous pass (the codex reproduction)
    rec_none = rtp.assemble(m, pub, "https://test.pypi.org/legacy/", promotion_env(), registry_files={})
    assert rec_none["verdict"] == "fail" and any("NOT served by the registry" in x for x in rec_none["problems"])
    # E2: one wheel byte mutated after R-TP -> the pypi job's binding RED
    w = pub / m["targets"][TRIPLES[0]]["wheel_filename"]
    orig = w.read_bytes()
    w.write_bytes(orig + b"\x00")
    p = vdm.verify(m, pub, rec)
    assert any(w.name in x and "not the manifest-bound build output" in x for x in p), p
    w.write_bytes(orig)
    # the sdist substituted -> RED; a foreign file -> RED; a missing file -> RED
    sd = pub / m["sdist"]["filename"]
    sd.write_bytes(b"substituted")
    assert any(sd.name in x for x in vdm.verify(m, pub, rec))
    shutil.copy(dist / sd.name, sd)
    (pub / "extra.whl").write_bytes(b"x")
    assert any("FOREIGN" in x for x in vdm.verify(m, pub))
    (pub / "extra.whl").unlink()
    w.unlink()
    assert any("MISSING" in x for x in vdm.verify(m, pub))
    # an R-TP that recorded other bytes (the dist changed between upload and promotion)
    other = dict(rec, uploaded_files={**rec["uploaded_files"], sd.name: _sha(b"other")})
    shutil.copy(dist / w.name, w)
    assert any("uploaded_files != the manifest" in x for x in vdm.verify(m, pub, other))
    r = subprocess.run([sys.executable, str(SCRIPTS / "verify_dist_manifest.py"), "--manifest", str(_write_json(tmp_path / "m.json", m)), "--dist", str(pub), "--rtp", str(_write_json(tmp_path / "rtp.json", rec))], capture_output=True, text=True)
    assert r.returncode == 0 and "GREEN" in r.stdout
    r = subprocess.run([sys.executable, str(SCRIPTS / "verify_dist_manifest.py"), "--manifest", str(tmp_path / "m.json"), "--dist", str(pub), "--rtp", str(_write_json(tmp_path / "rtp2.json", other))], capture_output=True, text=True)
    assert r.returncode == 1 and "RED" in r.stderr
    # the producer with a foreign dist: verdict fail (and exit 1)
    (pub / "extra.whl").write_bytes(b"x")
    r = subprocess.run([sys.executable, str(SCRIPTS / "assemble_rtp.py"), "--manifest", str(tmp_path / "m.json"), "--dist", str(pub), "--repository-url", "u", "--out", str(tmp_path / "ev" / "R-TP.json")], capture_output=True, text=True, env={**os.environ, **promotion_env()})
    assert r.returncode == 1 and "R-TP: fail" in r.stdout


def test_e3_rtv_assembly_pass_and_every_skipped_check_is_a_fail(tmp_path):
    m, _, _ = assemble(tmp_path)
    rec = rtv.assemble(m, _legs_ok(tmp_path / "ok", m), promotion_env())
    assert rec["verdict"] == "pass" and set(rec["targets"]) == set(TRIPLES)
    assert all(b[re_.RTV_ALL_FEATURES_FIELD] == "pass" and b["steps"] == {s: "pass" for s in re_.RTV_REQUIRED_STEPS} for b in rec["targets"].values())
    assert re_.check_record("R-TV", rec, m, source_sha=SHA_S) == []
    # the all-features smoke not run on one leg: the field is ABSENT -> producer fail AND consumer refuses
    rec2 = rtv.assemble(m, _legs_ok(tmp_path / "nosmoke", m, drop={TRIPLES[1]: ("smoke.json",)}), promotion_env())
    assert rec2["verdict"] == "fail" and re_.RTV_ALL_FEATURES_FIELD not in rec2["targets"][TRIPLES[1]]
    assert any(f"R-TV[{TRIPLES[1]}]: `all_features` absent" in x for x in re_.check_record("R-TV", rec2, m, source_sha=SHA_S))
    # the smoke failed on one leg
    rec3 = rtv.assemble(m, _legs_ok(tmp_path / "smokefail", m, fail_smoke=TRIPLES[2]), promotion_env())
    assert rec3["verdict"] == "fail" and rec3["targets"][TRIPLES[2]][re_.RTV_ALL_FEATURES_FIELD] == "fail"
    # a downloaded wheel that is not the manifest's bytes -> that leg fails (and the consumer sees the sha mismatch)
    legs = _legs_ok(tmp_path / "sha", m)
    dl = json.loads((legs / TRIPLES[0] / "download.json").read_text())
    dl["installed_wheel_sha256"] = _sha(b"served by TestPyPI from an earlier upload")
    _write_json(legs / TRIPLES[0] / "download.json", dl)
    rec4 = rtv.assemble(m, legs, promotion_env())
    assert rec4["verdict"] == "fail" and any("!= manifest wheel_sha256" in x for x in rec4["targets"][TRIPLES[0]]["problems"])
    lie = copy.deepcopy(rec4)
    lie["verdict"] = "pass"
    assert any("installed_wheel_sha256 != manifest" in x for x in re_.check_record("R-TV", lie, m, source_sha=SHA_S))
    # 4 legs -> fail; a lying 4-target record -> the consumer's target-set check
    legs4 = _legs_ok(tmp_path / "four", m)
    shutil.rmtree(legs4 / TRIPLES[4])
    rec5 = rtv.assemble(m, legs4, promotion_env())
    assert rec5["verdict"] == "fail" and "leg directory" in rec5["targets"][TRIPLES[4]]["problems"][0]
    del rec5["targets"][TRIPLES[4]]
    rec5["verdict"] = "pass"
    assert any("skipped leg" in x for x in re_.check_record("R-TV", rec5, m, source_sha=SHA_S))
    # a missing a0.json -> A0 fail
    rec6 = rtv.assemble(m, _legs_ok(tmp_path / "noa0", m, drop={TRIPLES[3]: ("a0.json",)}), promotion_env())
    assert rec6["targets"][TRIPLES[3]]["steps"]["A0"] == "fail" and rec6["verdict"] == "fail"
    r = subprocess.run([sys.executable, str(SCRIPTS / "assemble_rtv.py"), "--manifest", str(_write_json(tmp_path / "m.json", m)), "--legs", str(legs4), "--out", str(tmp_path / "ev" / "R-TV.json")],
                       capture_output=True, text=True, env={**os.environ, **promotion_env()})
    assert r.returncode == 1 and "R-TV: fail" in r.stdout


def _all_records(root: Path, m: dict, dist: Path, **over) -> Path:
    """R-BA (via the M4 assembler on green legs), R-NI, R-TP, R-TV -- all bound to `m`, producers at S."""
    from test_wheel_m4_acceptance import _leg_dir, rba
    ev = root / "evidence"
    ev.mkdir(parents=True, exist_ok=True)
    legs = root / "rba-legs"
    for t in TRIPLES:
        _leg_dir(legs, t, m)
    d = npm_tarballs(root, m)
    shutil.copy(dist / "runtime-payload.tgz", d / vni.pack_name("pyths-runtime", VERSION))
    shutil.copy(dist / "scaffolder-payload.tgz", d / vni.pack_name("create-pyths-app", VERSION))
    pub = root / "pub"
    pub.mkdir(exist_ok=True)
    for name in re_.distribution_set(m):
        shutil.copy(dist / name, pub / name)
    recs = {
        "R-BA": rba.assemble(m, legs, env_run(RUN_ID)),
        "R-NI": vni.verify(m, dist, vni.dir_fetcher(d), root / "work", env_run(RUN_ID), expected_content=_expected_content(d, m)),
        "R-TP": rtp.assemble(m, pub, "https://test.pypi.org/legacy/", promotion_env(), registry_files=re_.distribution_set(m)),
        "R-TV": rtv.assemble(m, _legs_ok(root, m), promotion_env()),
    }
    for k, v in over.items():
        recs[k] = v
    for name, rec in recs.items():
        if rec is not None:
            _write_json(ev / f"{name}.json", rec)
    return ev


# B4: the producer run's jobs must carry the FIXED assembler job of each record with conclusion
# success (verify_records finds THAT job, not just the run overall). Build-run 5005 is the promotion run.
def _build_run_jobs(attempt: int = 1) -> list[dict]:
    return prereq_jobs(attempt) + [job("node-free-evidence", attempt), job("npm-identity", attempt)]


def _promo_run_jobs(attempt: int = 1) -> list[dict]:
    return [job("testpypi", attempt), job("testpypi-validate", attempt), job("testpypi-evidence", attempt)]


def _api() -> StubAPI:
    return StubAPI(runs={RUN_ID: run_rec(RUN_ID), 5005: dict(run_rec(5005), path=".github/workflows/publish-pypi.yml")},
                   jobs={RUN_ID: _build_run_jobs(), 5005: _promo_run_jobs()})


TESTPYPI_NEED = ["R-BA", "R-NI"]
PYPI_NEED = ["R-BA", "R-NI", "R-TP", "R-TV"]


def test_e3_both_entrypoints_green_then_every_control_refuses(tmp_path):
    m, dist, _ = assemble(tmp_path)
    ev = _all_records(tmp_path / "ok", m, dist)
    for rec in ("R-BA", "R-NI", "R-TP", "R-TV"):
        assert json.loads((ev / f"{rec}.json").read_text())["verdict"] == "pass", rec
    api = _api()
    assert re_.verify_records(m, TESTPYPI_NEED, ev, env=promotion_env(), api=api) == []
    assert re_.verify_records(m, PYPI_NEED, ev, env=promotion_env(), api=api) == []

    def both(evd: Path, needle: str, *, testpypi_red: bool = True) -> None:
        p_pypi = re_.verify_records(m, PYPI_NEED, evd, env=promotion_env(), api=api)
        assert any(needle in x for x in p_pypi), (needle, p_pypi)
        p_tp = re_.verify_records(m, TESTPYPI_NEED, evd, env=promotion_env(), api=api)
        if testpypi_red:
            assert any(needle in x for x in p_tp), (needle, p_tp)
        else:
            assert p_tp == []

    # missing record: R-NI missing -> BOTH refuse; R-TV missing -> pypi refuses, testpypi does not need it (the codex scenario)
    both(_all_records(tmp_path / "s1", m, dist, **{"R-NI": None}), "R-NI: record file")
    both(_all_records(tmp_path / "s2", m, dist, **{"R-TV": None}), "R-TV: record file", testpypi_red=False)
    # verdict fail
    rni = json.loads((ev / "R-NI.json").read_text())
    both(_all_records(tmp_path / "s3", m, dist, **{"R-NI": dict(rni, verdict="fail")}), "R-NI: verdict 'fail'")
    # partial target set (a skipped leg) in R-BA (both) and in R-TV (pypi)
    rba_rec = json.loads((ev / "R-BA.json").read_text())
    part = copy.deepcopy(rba_rec)
    del part["targets"][TRIPLES[0]]
    both(_all_records(tmp_path / "s4", m, dist, **{"R-BA": part}), "R-BA: targets")
    rtv_rec = json.loads((ev / "R-TV.json").read_text())
    part = copy.deepcopy(rtv_rec)
    del part["targets"][TRIPLES[0]]
    both(_all_records(tmp_path / "s5", m, dist, **{"R-TV": part}), "R-TV: targets", testpypi_red=False)
    # wrong run: record source_sha != S / producer head_sha != S / a different manifest hash
    wrong = copy.deepcopy(rni)
    wrong["source_sha"] = wrong["producer"]["head_sha"] = SHA_OTHER
    both(_all_records(tmp_path / "s6", m, dist, **{"R-NI": wrong}), "R-NI: source_sha")
    m_other, dist_other, _ = assemble(tmp_path / "other", binaries={t: f"OTHER::{t}".encode() for t in TRIPLES})
    ev_other = _all_records(tmp_path / "s7", m_other, dist_other)
    for rec in ("R-BA", "R-NI", "R-TP", "R-TV"):
        p = re_.verify_records(m, [rec], ev_other, env=promotion_env(), api=api)
        assert any(f"{rec}: bound to manifest" in x for x in p), (rec, p)
    # R-TP producer claiming release.yml -> refused; R-NI producer at a different run -> refused
    rtp_rec = json.loads((ev / "R-TP.json").read_text())
    bad_wf = copy.deepcopy(rtp_rec)
    bad_wf["producer"]["workflow"] = "release.yml"
    assert any("R-TP: producer workflow" in x for x in re_.verify_records(m, ["R-TP"], _all_records(tmp_path / "s8", m, dist, **{"R-TP": bad_wf}), env=promotion_env(), api=api))
    other_run = copy.deepcopy(rni)
    other_run["producer"]["run_id"] = 7007
    both(_all_records(tmp_path / "s9", m, dist, **{"R-NI": other_run}), "R-NI: producer run 7007")
    # R-TV without the all-features field on one target -> pypi refuses
    no_af = copy.deepcopy(rtv_rec)
    del no_af["targets"][TRIPLES[2]][re_.RTV_ALL_FEATURES_FIELD]
    both(_all_records(tmp_path / "s10", m, dist, **{"R-TV": no_af}), "`all_features` absent", testpypi_red=False)
    # the producer run of R-TV not green via the API -> refused
    api_red = StubAPI(runs={RUN_ID: run_rec(RUN_ID), 5005: dict(run_rec(5005), path=".github/workflows/publish-pypi.yml", conclusion="failure")},
                      jobs={RUN_ID: _build_run_jobs(), 5005: _promo_run_jobs()})
    assert any("R-TV: producer run 5005 is not a completed+successful run" in x for x in re_.verify_records(m, PYPI_NEED, ev, env=promotion_env(), api=api_red))


def test_b4_producer_job_binding_refuses_forged_and_wrong_job_records(tmp_path):
    """B4: a record must name its FIXED assembler job, and verify_records requires THAT job's own
    conclusion to be success in the producer run -- not merely the run overall."""
    m, dist, _ = assemble(tmp_path)
    ev = _all_records(tmp_path / "ok", m, dist)
    api = _api()
    assert re_.verify_records(m, PYPI_NEED, ev, env=promotion_env(), api=api) == []  # clean baseline
    # (1) check_record refuses a record naming the WRONG producer job (the pre-B4 self-asserted claims)
    rba = json.loads((ev / "R-BA.json").read_text())
    assert rba["producer"]["job"] == "node-free-evidence"  # assemble_rba now names its assembler, not the matrix
    wrong = copy.deepcopy(rba); wrong["producer"]["job"] = "node-free-acceptance"
    assert any("producer job 'node-free-acceptance'" in x for x in re_.check_record("R-BA", wrong, m, source_sha=SHA_S))
    rtv = json.loads((ev / "R-TV.json").read_text())
    assert rtv["producer"]["job"] == "testpypi-evidence"
    wrong = copy.deepcopy(rtv); wrong["producer"]["job"] = "testpypi-validate"
    assert any("producer job 'testpypi-validate'" in x for x in re_.check_record("R-TV", wrong, m, source_sha=SHA_S))
    # (2) the codex scenario: a forged R-TV naming a completed+success publish-pypi.yml run whose
    #     testpypi-evidence job NEVER RAN (a target=pypi dispatch is green overall) -> refused
    forged = copy.deepcopy(rtv); forged["producer"]["run_id"] = 6006
    api_forge = StubAPI(
        runs={RUN_ID: run_rec(RUN_ID), 6006: dict(run_rec(6006), path=".github/workflows/publish-pypi.yml")},
        jobs={RUN_ID: _build_run_jobs(), 6006: [job("pypi")]},  # green target=pypi run, no testpypi-evidence job
    )
    evf = _all_records(tmp_path / "forge", m, dist, **{"R-TV": forged})
    p = re_.verify_records(m, ["R-TV"], evf, env=promotion_env(), api=api_forge)
    assert any("producer job 'testpypi-evidence'" in x and "did not run" in x for x in p), p
    # (3) the assembler job present but concluded FAILURE -> refused (the whole-run-green mask)
    api_fail = StubAPI(
        runs={RUN_ID: run_rec(RUN_ID), 5005: dict(run_rec(5005), path=".github/workflows/publish-pypi.yml")},
        jobs={RUN_ID: _build_run_jobs(), 5005: [job("testpypi"), job("testpypi-validate"), job("testpypi-evidence", conclusion="failure")]},
    )
    p = re_.verify_records(m, ["R-TV"], ev, env=promotion_env(), api=api_fail)
    assert any("did not conclude success" in x for x in p), p


def test_e3_cli_entrypoints_with_the_exact_workflow_need_sets(tmp_path, monkeypatch):
    """The two `require_evidence.py --role promotion --need ...` invocations, exactly as publish-pypi.yml writes them."""
    m, dist, _ = assemble(tmp_path)
    ev = _all_records(tmp_path / "ok", m, dist)
    mf = _write_json(tmp_path / "release_manifest.json", m)
    monkeypatch.setattr(re_, "api_from_env", lambda env: _api())
    for k, v in promotion_env().items():
        monkeypatch.setenv(k, v)
    pub = yaml.safe_load((REPO / ".github" / "workflows" / "publish-pypi.yml").read_text(encoding="utf-8"))
    for job in ("testpypi", "pypi"):
        step = next(s for s in pub["jobs"][job]["steps"] if "require_evidence.py" in str(s.get("run", "")))
        args = step["run"].split("require_evidence.py", 1)[1].split()
        args = [{"release_manifest.json": str(mf), "evidence": str(ev), '"${GITHUB_REF_NAME}"': f"v{VERSION}"}.get(a, a) for a in args]
        assert re_.main(args) == 0, (job, args)
    (ev / "R-TV.json").unlink()
    assert re_.main(["--role", "promotion", "--manifest", str(mf), "--need", *TESTPYPI_NEED, "--evidence-dir", str(ev), "--tag", f"v{VERSION}"]) == 0
    assert re_.main(["--role", "promotion", "--manifest", str(mf), "--need", *PYPI_NEED, "--evidence-dir", str(ev), "--tag", f"v{VERSION}"]) == 1


def test_e3_testpypi_validate_leg_pure_parts(tmp_path):
    m, dist, _ = assemble(tmp_path)
    t = TRIPLES[0]
    dl = tmp_path / "dl"
    dl.mkdir()
    shutil.copy(dist / m["targets"][t]["wheel_filename"], dl)
    rec = tpv.check_download(dl, m, t, "https://test.pypi.org/simple/")
    assert rec["verdict"] == "pass" and rec["installed_wheel_sha256"] == m["targets"][t]["wheel_sha256"]
    (dl / m["targets"][t]["wheel_filename"]).write_bytes(b"served: an earlier upload at the same version")
    rec = tpv.check_download(dl, m, t, "u")
    assert rec["verdict"] == "fail" and any("refusing to install" in x for x in rec["problems"])
    shutil.copy(dist / m["targets"][TRIPLES[1]]["wheel_filename"], dl)  # two wheels -> not exactly the manifest-named one
    assert tpv.check_download(dl, m, t, "u")["verdict"] == "fail"
    site = tmp_path / "venv" / "lib" / "site-packages"
    site.mkdir(parents=True)
    ok = tpv.smoke_summary(0, "5 passed in 3.2s", str(site / "pythscribe" / "__init__.py"), site)
    assert ok["verdict"] == "pass" and ok["passed"] == 5
    assert tpv.smoke_summary(1, "4 passed, 1 failed", str(site / "pythscribe" / "__init__.py"), site)["verdict"] == "fail"
    assert tpv.smoke_summary(0, "no tests ran", str(site / "pythscribe" / "__init__.py"), site)["verdict"] == "fail"
    shadow = tpv.smoke_summary(0, "5 passed", str(REPO / "pythscribe" / "__init__.py"), site)
    assert shadow["verdict"] == "fail" and any("not the venv site-packages" in x for x in shadow["problems"])


# ============================================================================ K: README spots


README = (REPO / "README.md").read_text(encoding="utf-8")
DECLARED = rs.declared_extras_pyproject(REPO / "pyproject.toml")


def test_k_static_green_on_the_real_readme_and_spots_extracted():
    assert rs.static_checks(README, DECLARED) == []
    blocks, problems = rs.extract_spots(README)
    assert problems == [] and len(blocks) >= 4
    kinds = {b.kind for b in blocks}
    assert kinds == {"spot", "node"}
    assert all(rs.SPOT_SECTIONS.search(b.section) for b in blocks)
    assert any("pip install pythscribe" in b.code for b in blocks if b.kind == "spot")
    assert any("from pythscribe import wasm" in b.code for b in blocks if b.kind == "spot")
    assert any("pyths new" in b.code for b in blocks if b.kind == "node")
    r = subprocess.run([sys.executable, str(SCRIPTS / "readme_spots.py"), "--static"], capture_output=True, text=True)
    assert r.returncode == 0 and "GREEN" in r.stdout, (r.stdout, r.stderr)
    # the pip-primary inversion (plan M6.4): Installation leads with pip, the environment table follows, the deep reference stays
    inst = README.split("## Installation", 1)[1].split("## Quick Start", 1)[0]
    assert inst.index("pip install pythscribe") < inst.index("npm install pythscribe")
    assert "| **Without Node** |" in inst and "| **With Node**" in inst and "pyths doctor" in inst
    assert "pythscribe/README.md" in inst and "MIT AND LicenseRef-FSL-1.1-ALv2" in inst


def test_k_controls_each_false_claim_is_red():
    # a non-existent extra in a command. 0.2.9: the `[server]` install line no longer exists (wasmtime is
    # CORE), so mutate a line that DOES exist -- the `[web-bundled]` one -- and assert the mutation LANDED
    # (a no-op replace would make this control vacuous; codex 0.2.9 review blocker 3).
    assert 'pip install "pythscribe[web-bundled]"' in README
    bad = README.replace('pip install "pythscribe[web-bundled]"', 'pip install "pythscribe[nope]"', 1)
    assert bad != README
    assert any("pythscribe[nope]" in x for x in rs.static_checks(bad, DECLARED))
    # a resurrected [server] extra (0.2.9 removed it: wasmtime is a core dependency) -> RED
    assert any("pythscribe[server]" in x for x in rs.check_extras(README + "\n`pip install pythscribe[server]`\n", DECLARED))
    # an extra the wheel does not declare (the wheel's Provides-Extra is the authority in the acceptance run)
    assert any("web-bundled" in x for x in rs.static_checks(README, DECLARED - {"web-bundled"}))
    # a resurrected [web] extra
    assert any("pythscribe[web]" in x for x in rs.check_extras(README + "\n`pip install pythscribe[web]`\n", DECLARED))
    # MCP without the (v0.3) label. MCP is NOT in this release's README (removed by design), but
    # check_env_table still GUARDS against an MCP mention in the Without-Node row lacking the
    # "(v0.3)" label if it is ever re-added -- so feed a SYNTHETIC violating row rather than mutate
    # the (MCP-free) README, whose "MCP server *(v0.3)*" string no longer exists.
    _wn = rs._table_row(README, "Without Node")
    _mcp_bad = README.replace(_wn, _wn.replace("Offline, no toolchain.", "Offline, no toolchain, MCP server.", 1), 1)
    assert any("(v0.3)" in x for x in rs.check_env_table(_mcp_bad))
    # a With-Node capability claimed in the Without-Node row
    row = rs._table_row(README, "Without Node")
    assert any("With-Node capability" in x for x in rs.check_env_table(README.replace(row, row.replace("Offline, no toolchain.", "Offline, no toolchain, dev server + HMR."), 1)))
    # the platform list vs the matrix: a target added to the matrix without a README update
    assert any("PLATFORM_WORDS" in x or "omits" in x for x in rs.check_platforms(README, EXPECTED_WHEEL_SET | {"musllinux_1_2_x86_64"}))
    monkey = dict(rs.PLATFORM_WORDS, win_arm64=("Windows", "arm64"))
    orig = rs.PLATFORM_WORDS
    rs.PLATFORM_WORDS = monkey
    try:
        assert any("omits Windows arm64" in x for x in rs.check_platforms(README, EXPECTED_WHEEL_SET | {"win_arm64"}))
    finally:
        rs.PLATFORM_WORDS = orig
    # the README claiming a platform the matrix does not build
    assert any("does not build" in x for x in rs.check_platforms(README.replace("Windows x64.", "Windows x64/arm64.", 1)))
    # the source-install note deleted
    assert any("source-install note" in x for x in rs.check_platforms(README.replace("builds from source **without** the compiler", "installs", 1)))
    # the glibc floor dropped
    assert any("glibc 2.28" in x for x in rs.check_platforms(README.replace(" (glibc 2.28+)", "", 1)))
    # a spot marker outside Installation / Quick Start
    stray = README.replace("## CLI Reference\n", "## CLI Reference\n<!-- spot -->\n```bash\npyths --version\n```\n", 1)
    assert any("spots live under Installation" in x for x in rs.extract_spots(stray)[1])


@pytest.fixture(scope="module")
def host_wheel(tmp_path_factory) -> dict:
    """A REAL wheel from this checkout with the dev compiler staged (the M1 fixture pattern) + a venv with it installed."""
    if SKIP_PACKAGING:
        pytest.skip("packaging gate skipped by env")
    binname = "pyths.exe" if os.name == "nt" else "pyths"
    dev = REPO / "target" / "release" / binname
    gate(dev.is_file(), f"{dev} missing: `cargo build --release --bin pyths` first")
    tag = host_wheel_tag()
    gate(tag is not None, "not a release wheel host")
    staged = REPO / "pythscribe" / "_bin" / binname
    staged.parent.mkdir(exist_ok=True)
    if not staged.is_file() or staged.read_bytes() != dev.read_bytes():
        shutil.copy2(dev, staged)
    out = tmp_path_factory.mktemp("dist")
    env = {k: v for k, v in os.environ.items() if k != "PYTHONPATH"}
    env["PYTHSCRIBE_WHEEL_PLATFORM"] = tag
    r = subprocess.run([sys.executable, "-m", "build", "--wheel", "--outdir", str(out), str(REPO)], capture_output=True, text=True, timeout=900, cwd=str(REPO), env=env)
    gate(r.returncode == 0, f"python -m build failed: {r.stderr[-2000:]}")
    [whl] = out.glob("*.whl")
    venv = tmp_path_factory.mktemp("venv") / "venv"
    subprocess.run([sys.executable, "-m", "venv", str(venv)], check=True)
    py = venv / ("Scripts" if os.name == "nt" else "bin") / ("python.exe" if os.name == "nt" else "python")
    r = subprocess.run([str(py), "-m", "pip", "install", "--quiet", "--no-deps", str(whl)], capture_output=True, text=True, timeout=600)
    assert r.returncode == 0, r.stderr[-2000:]
    return {"wheel": whl, "python": py, "tag": tag}


def test_k_run_mode_executes_the_readme_spots_against_the_shipped_wheel(host_wheel):
    """The acceptance-job SPOT, locally: pip lines bound to the built wheel (direct_url.json), `python kernels.py`
    compiles with the BUNDLED compiler + runs under wasmtime, `pyths build` / `pyths doctor` run the installed launcher."""
    pytest.importorskip("wasmtime")
    py, whl = host_wheel["python"], host_wheel["wheel"]
    assert rs.installed_wheel_path(py) == whl.resolve() or rs.installed_wheel_path(py) == whl
    assert rs.declared_extras_wheel(whl) == DECLARED
    r = subprocess.run([sys.executable, str(SCRIPTS / "readme_spots.py"), "--run", "--python", str(py)], capture_output=True, text=True, timeout=1800)
    assert r.returncode == 0 and "GREEN" in r.stdout and "(run)" in r.stdout, (r.stdout[-3000:], r.stderr[-3000:])
    assert "DEFER" in r.stdout and "web-parity" in r.stdout  # node blocks are deferred, never counted
    assert "ok   README:" in r.stdout and "kernels.py" in r.stdout
    # control: a bogus extra in a RUN block -> RED. NOTE pip itself only WARNS on an undeclared extra and exits 0
    # (observed: `pip install <wheel>[nope]` succeeded), so the run side checks Provides-Extra itself -- otherwise
    # this control would be vacuous (the static check catches the same claim over the whole README first).
    block = rs.Block(line=1, lang="bash", code='pip install "pythscribe[nope]"\n', section="Installation")
    rep = rs.run_spots([block], python=py, wheel=whl, node=False, pip_args=["--no-index"])
    assert rep.problems and "does not declare" in rep.problems[0] and "['nope']" in rep.problems[0], rep
    ok = rs.run_spots([rs.Block(line=1, lang="bash", code="pip install pythscribe\n", section="Installation")], python=py, wheel=whl, node=False, pip_args=["--no-index", "--no-deps"])
    assert ok.problems == [] and ok.passed
    # a wrong expected output -> RED
    block = rs.Block(line=2, lang="python", code='# t.py\nprint("hello")\n# → world\n', section="Installation")
    rep = rs.run_spots([block], python=py, wheel=whl, node=False, pip_args=[])
    assert rep.problems and "expected stdout ['world']" in rep.problems[0]
    # a spot that cannot bind to the shipped wheel is RED, never silently skipped
    rep = rs.run_spots([rs.Block(line=3, lang="bash", code="pip install pythscribe\n", section="Installation")], python=py, wheel=None, node=False, pip_args=[])
    assert rep.problems and "cannot be bound to the shipped wheel" in rep.problems[0]
    # the README's `uv pip install` line: the SAME binding + extras refusal (decided BEFORE uv is resolved, so
    # these two controls are host-independent) ...
    block = rs.Block(line=1, lang="bash", code='uv pip install "pythscribe[nope]"\n', section="Installation")
    rep = rs.run_spots([block], python=py, wheel=whl, node=False, pip_args=["--no-index"])
    assert rep.problems and "does not declare" in rep.problems[0] and "['nope']" in rep.problems[0], rep
    rep = rs.run_spots([rs.Block(line=3, lang="bash", code="uv pip install pythscribe\n", section="Installation")], python=py, wheel=None, node=False, pip_args=[])
    assert rep.problems and "cannot be bound to the shipped wheel" in rep.problems[0]
    # ... and the ARGV it hands uv is asserted (DETERMINISTIC: uv's presence and the spawn are mocked, so this does
    # not depend on whether THIS host has uv). Anti-vacuity: the target venv already has pythscribe installed, so
    # a mutant that passes the bare `pythscribe` token to uv (ignoring the bound wheel) would still exit 0 with
    # `--no-index` -- only the argv assertion makes that mutant RED: the BOUND WHEEL PATH must be the requirement,
    # `--python <target>` must pin the interpreter, and no bare `pythscribe` token may reach uv.
    import subprocess as _sp

    fake_uv = str(whl.parent / ("uv.exe" if os.name == "nt" else "uv"))  # never spawned (subprocess.run is mocked)
    real_which = rs.shutil.which
    spawned: list[list[str]] = []

    def _which_with_uv(name, *a, **kw):
        return fake_uv if name == "uv" else real_which(name, *a, **kw)

    def _which_without_uv(name, *a, **kw):
        return None if name == "uv" else real_which(name, *a, **kw)

    def _capture(argv, *a, **kw):
        spawned.append([str(x) for x in argv])
        return _sp.CompletedProcess(argv, 0, stdout="", stderr="")

    def _uv_spots(code: str, *, present: bool):
        spawned.clear()
        with pytest.MonkeyPatch.context() as mp:
            mp.setattr(rs.shutil, "which", _which_with_uv if present else _which_without_uv)
            mp.setattr(rs.subprocess, "run", _capture)
            return rs.run_spots([rs.Block(line=1, lang="bash", code=code, section="Installation")], python=py, wheel=whl, node=False, pip_args=["--no-index", "--no-deps"])

    # uv PRESENT: exactly one spawn, of the real uv, with the bound wheel (+ extras) and the pinned target interpreter
    for code, want_req in (("uv pip install pythscribe\n", str(whl)), ('uv pip install "pythscribe[gradio]"\n', f"{whl}[gradio]")):  # 0.2.9: no `server` extra
        ok = _uv_spots(code, present=True)
        assert ok.problems == [] and len(ok.passed) == 1 and ok.deferred == [], ok
        assert len(spawned) == 1, spawned
        argv = spawned[0]
        assert argv[:3] == [fake_uv, "pip", "install"], argv
        assert argv[argv.index("--python") + 1] == str(py), argv
        assert want_req in argv, (want_req, argv)  # the SHIPPED wheel is the requirement uv consumes
        assert "pythscribe" not in argv and not any(a.startswith("pythscribe[") for a in argv), argv  # never the bare name
        assert argv[-2:] == ["--no-index", "--no-deps"], argv  # pip_args reach the uv arm too
    # uv ABSENT: one DEFER, NOTHING spawned (never silently pip-substituted, never counted as passed)
    ok = _uv_spots("uv pip install pythscribe\n", present=False)
    assert ok.problems == [] and ok.passed == [] and len(ok.deferred) == 1 and "uv is not installed" in ok.deferred[0], ok
    assert spawned == [], spawned


# ============================================================================ L: pre-tag gate


def _committed_space_tag() -> str:
    """The @v<X.Y.Z> tag the committed Space requirements is pinned to. DERIVED (never a literal) so a
    version bump can never staleness-break these gate self-tests -- the recurring class this closes. The
    release-time invariant (pin == the tag being cut) is enforced by pretag_gate's mirror-pin gate with the
    REAL tag; here we only exercise the mirror_pin_problems LOGIC (match -> [], mismatch -> RED)."""
    m = re.search(r"@(v\d+\.\d+\.\d+)", pg.SPACE_REQUIREMENTS.read_text(encoding="utf-8"))
    assert m, "committed Space requirements has no @v<X.Y.Z> pin"
    return m.group(1)


_WRONG_TAG = "v0.9.9"  # a deliberately-different tag for the mismatch path (never a real release)


def test_l3_mirror_pin_rule_and_the_committed_space_requirements():
    text = pg.SPACE_REQUIREMENTS.read_text(encoding="utf-8")
    cut = _committed_space_tag()
    assert cut != _WRONG_TAG
    # match: checking against the tag the file IS pinned to -> no problems
    assert pg.mirror_pin_problems(text, cut) == []
    # mismatch: any other tag -> RED naming the wrong cut tag
    assert any(f"not the tag being cut `@{_WRONG_TAG}`" in x for x in pg.mirror_pin_problems(text, _WRONG_TAG))
    # a mutated pin (wrong found value) checked against the real cut tag -> RED naming both
    assert any(f"`@{cut}`" in x and _WRONG_TAG in x for x in pg.mirror_pin_problems(text.replace(f"@{cut}", f"@{_WRONG_TAG}"), cut))
    # a missing gradio_wasmfunction pin -> RED
    assert any("no `gradio_wasmfunction @" in x for x in pg.mirror_pin_problems(text.split("gradio_wasmfunction")[0], cut))


def test_l2_license_files_present_and_the_gate_scripts_run():
    assert pg.gate_license_files() == []
    assert pg.gate_readme() == [] and pg.gate_workflows() == []
    if NODE:
        # the versions gate: check_versions + guard_tag_version + publish.mjs --check for the tag this tree is at
        cargo_v = json.loads((REPO / "npm" / "pythscribe" / "package.json").read_text())["version"]
        p = pg.gate_versions(f"v{cargo_v}")
        # guard_tag_version binds tag == pyproject == pin; on a dev tree pyproject may lead -> that line, and ONLY that
        # line, may be RED here (the release tree is unified by set_version.py); the stamp check must be GREEN
        assert not any("stamp idempotence RED" in x for x in p), p
        assert all("guard_tag_version" in x for x in p), p
    # B6: a `--only` subset that passes is PARTIAL/DIAGNOSTIC, NOT an unqualified release-authorizing GREEN.
    # cut tag DERIVED from the committed pin (never a literal) so a version bump cannot staleness-break this.
    cut = _committed_space_tag()
    r = subprocess.run([sys.executable, str(SCRIPTS / "pretag_gate.py"), cut, "--only", "readme", "workflows", "mirror-pin"], capture_output=True, text=True)
    assert r.returncode == 0 and "PARTIAL/DIAGNOSTIC" in r.stdout and "NOT a" in r.stdout and "CHECKLIST (manual" in r.stdout, (r.stdout, r.stderr)
    assert "pretag_gate: GREEN (release-authorizing)" not in r.stdout  # a subset must never claim the authorizing GREEN
    r = subprocess.run([sys.executable, str(SCRIPTS / "pretag_gate.py"), _WRONG_TAG, "--only", "mirror-pin"], capture_output=True, text=True)
    assert r.returncode == 1 and "[mirror-pin] RED" in r.stdout and "not the tag being cut" in r.stderr
    r = subprocess.run([sys.executable, str(SCRIPTS / "pretag_gate.py"), "0.2.5"], capture_output=True, text=True)
    assert r.returncode == 2


def test_b6_partial_run_is_not_release_authorizing_green_and_ci_gate_verifies_exact_sha():
    """B6: the codex reproduction -- `--only workflows` printed an unqualified GREEN while the version
    guard was RED. A subset now prints PARTIAL/DIAGNOSTIC (exit 0) and never the authorizing GREEN;
    require_ci_success verifies a completed+success CI run at the EXACT sha (a red/absent one is RED)."""
    # the exact reproduction: --only workflows (which passes) must NOT print the authorizing GREEN
    r = subprocess.run([sys.executable, str(SCRIPTS / "pretag_gate.py"), "v0.2.5", "--only", "workflows"], capture_output=True, text=True)
    assert r.returncode == 0 and "PARTIAL/DIAGNOSTIC" in r.stdout
    assert "pretag_gate: GREEN (release-authorizing)" not in r.stdout, r.stdout
    # the exact-SHA CI-success primitive (injected fetchers; no network)
    import require_ci_success as rcs
    sha = "a" * 40

    def push_run(**over):
        return {"id": 42, "head_sha": sha, "status": "completed", "conclusion": "success", "event": "push", "head_branch": "main", "run_attempt": 1, **over}

    def all_jobs_green(run_id):  # every REQUIRED_CI_JOBS complete leg present + success (matrix legs enumerated)
        return [{"name": r, "run_attempt": 1, "conclusion": "success"} for r in rcs.REQUIRED_CI_JOBS]

    green = lambda wf, s: [push_run()]
    assert rcs.verify(sha, fetch_runs=green, fetch_jobs=all_jobs_green) == []
    # B6 (codex pass-3): fetch_jobs=None must FAIL CLOSED, never a runs-only GREEN
    assert any("fail closed" in x for x in rcs.verify(sha, fetch_runs=green, fetch_jobs=None))
    # B6 (codex pass-3): one matrix leg failed (Windows) while the siblings pass -> RED (the prefix+max bypass)
    def windows_leg_fails(run_id):
        return [dict(j, conclusion=("failure" if j["name"] == "Test (windows-latest, 1.94.0)" else j["conclusion"])) for j in all_jobs_green(run_id)]
    pw = rcs.verify(sha, fetch_runs=green, fetch_jobs=windows_leg_fails)
    assert any("Test (windows-latest, 1.94.0)`" in x and "not success" in x for x in pw), pw
    # B6: a missing matrix leg (only one Test leg ran) -> RED
    def only_ubuntu(run_id):
        return [j for j in all_jobs_green(run_id) if not j["name"].startswith("Test (") or j["name"] == "Test (ubuntu-latest, 1.94.0)"]
    assert any("Test (macos-latest, 1.94.0)` did not run" in x for x in rcs.verify(sha, fetch_runs=green, fetch_jobs=only_ubuntu))
    # B6: REQUIRED_CI_JOBS is BOUND to ci.yml -- every required leg is a real display name (rename -> RED)
    assert rcs.binding_problems() == []
    import copy as _copy, tempfile as _tmp, yaml as _yaml
    _ci = _yaml.safe_load((REPO / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8"))
    _mut = _copy.deepcopy(_ci)
    _mut["jobs"]["lint"]["name"] = "Linting (renamed)"
    _p = Path(_tmp.mkdtemp()) / "ci.yml"
    _p.write_text(_yaml.safe_dump(_mut), encoding="utf-8")
    assert any("Lint" in x for x in rcs.binding_problems(_p))
    # B6: a MANUAL workflow_dispatch run at the SHA (push-only packaging skips, overall green) -> refused
    dispatch = lambda wf, s: [push_run(event="workflow_dispatch")]
    assert any("no completed+successful PUSH-to-main" in x for x in rcs.verify(sha, fetch_runs=dispatch, fetch_jobs=all_jobs_green))
    # B6: a non-main push run -> refused
    branch = lambda wf, s: [push_run(head_branch="release/x")]
    assert any("no completed+successful PUSH-to-main" in x for x in rcs.verify(sha, fetch_runs=branch, fetch_jobs=all_jobs_green))
    # B6: a push-to-main run MISSING a required job (the packaging skip) -> refused
    def missing_packaging(run_id):
        return [j for j in all_jobs_green(run_id) if j["name"] != "Packaging (fresh install)"]  # noqa: F811
    p = rcs.verify(sha, fetch_runs=green, fetch_jobs=missing_packaging)
    assert any("Packaging (fresh install)` did not run" in x for x in p), p
    # a required job that FAILED -> refused
    def failed_lint(run_id):
        return [dict(j, conclusion=("failure" if j["name"] == "Lint" else j["conclusion"])) for j in all_jobs_green(run_id)]
    assert any("`Lint`" in x and "not success" in x for x in rcs.verify(sha, fetch_runs=green, fetch_jobs=failed_lint))
    red = lambda wf, s: [push_run(conclusion="failure")]
    assert any("no completed+successful" in x for x in rcs.verify(sha, fetch_runs=red, fetch_jobs=all_jobs_green))
    absent = lambda wf, s: []
    assert any("no completed+successful" in x for x in rcs.verify(sha, fetch_runs=absent, fetch_jobs=all_jobs_green))
    # a green run at a DIFFERENT sha does not count
    other = lambda wf, s: [push_run(head_sha="b" * 40)]
    assert any("no completed+successful" in x for x in rcs.verify(sha, fetch_runs=other, fetch_jobs=all_jobs_green))
    # an unreachable API is a FAILURE, never a pass
    def boom(wf, s):
        raise RuntimeError("network down")
    assert any("could not query" in x for x in rcs.verify(sha, fetch_runs=boom, fetch_jobs=all_jobs_green))


def test_l2_control_a_missing_licence_file_is_red(tmp_path):
    py = (REPO / "pyproject.toml").read_text(encoding="utf-8").replace('"pythscribe/LICENSES-MAP.md",', '"pythscribe/NO-SUCH-MAP.md",', 1)
    (tmp_path / "pyproject.toml").write_text(py, encoding="utf-8")
    (tmp_path / "pythscribe").mkdir()
    for rel in ("pythscribe/LICENSE", "LICENSE.md"):
        (tmp_path / rel).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(REPO / rel, tmp_path / rel)
    assert any("NO-SUCH-MAP.md" in x for x in pg.gate_license_files(tmp_path / "pyproject.toml"))


def test_l5_all_features_smoke_runs_against_the_src_build():
    """L5 pre-tag half: the simple all-features test against the src build (the dev compiler + wasmtime)."""
    binname = "pyths.exe" if os.name == "nt" else "pyths"
    gate((REPO / "target" / "release" / binname).is_file(), "dev compiler required (cargo build --release)")
    pytest.importorskip("wasmtime")
    assert pg.gate_smoke() == []


# ============================================================================ WF: the M6 lint rules, mutation-verified


def _wf():
    rel = yaml.safe_load((REPO / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8"))
    pub = yaml.safe_load((REPO / ".github" / "workflows" / "publish-pypi.yml").read_text(encoding="utf-8"))
    return rel, pub


def test_wf_m6_green_on_the_real_workflows_and_the_job_graph():
    rel, pub = _wf()
    assert wl.lint(rel, pub) == []
    jobs = rel["jobs"]
    assert jobs["manifest"]["needs"] == ["prepare", "wheel-set"] and jobs["release"]["needs"] == "manifest"
    assert set(jobs["npm-identity"]["needs"]) == {"manifest", "npm-publish"}
    assert "node-free-acceptance" not in wl._transitive_needs(jobs, "manifest")
    assert {r["target"] for r in pub["jobs"]["testpypi-validate"]["strategy"]["matrix"]["include"]} == set(TRIPLE_TO_TAG)
    assert pub["jobs"]["testpypi-evidence"]["needs"] == ["testpypi", "testpypi-validate"]
    assert not any("python -m build" in str(s.get("run", "")) for j in pub["jobs"].values() for s in j["steps"])


def test_wf_m6_each_rule_fires():
    rel, pub = _wf()
    # M1: the fail-closed placeholder (the M4 state) is RED now
    mut = copy.deepcopy(rel)
    mut["jobs"]["manifest"]["steps"] = [{"uses": "actions/checkout@v4"}, {"name": "placeholder", "run": "echo not landed\nexit 1"}]
    p = wl.lint(mut, pub)
    assert any("does not run scripts/assemble_manifest.py" in x for x in p) and any("exit 1" in x for x in p)
    mut = copy.deepcopy(rel)
    mut["jobs"]["prepare"]["steps"] = [s for s in mut["jobs"]["prepare"]["steps"] if "publish.mjs --check" not in str(s.get("run", ""))]
    assert any("stamp idempotence" in x for x in wl.lint(mut, pub))
    # M2: npm-identity missing / evidence after the identity script / publish before the evidence step
    mut = copy.deepcopy(rel)
    del mut["jobs"]["npm-identity"]
    assert any("no `npm-identity` job" in x for x in wl.lint(mut, pub))
    mut = copy.deepcopy(rel)
    st = mut["jobs"]["npm-identity"]["steps"]
    ev = next(i for i, s in enumerate(st) if "require_evidence.py" in str(s.get("run", "")))
    st.append(st.pop(ev))
    assert any("BEFORE verify_npm_identity.py" in x for x in wl.lint(mut, pub))
    mut = copy.deepcopy(rel)
    mut["jobs"]["npm-identity"]["needs"] = "manifest"
    assert any("must include `npm-publish`" in x for x in wl.lint(mut, pub))
    mut = copy.deepcopy(rel)
    st = mut["jobs"]["npm-publish"]["steps"]
    ev = next(i for i, s in enumerate(st) if "require_evidence.py" in str(s.get("run", "")))
    st.append(st.pop(ev))
    assert any("BEFORE `node npm/publish.mjs --yes`" in x for x in wl.lint(mut, pub))
    # M3: the Release of binaries ungated
    mut = copy.deepcopy(rel)
    mut["jobs"]["release"]["needs"] = "build"
    assert any("M3" in x for x in wl.lint(mut, pub))
    # M4: a wheel leg without the README spots
    mut = copy.deepcopy(rel)
    for s in mut["jobs"]["wheel"]["steps"]:
        if isinstance(s.get("env"), dict) and "CIBW_TEST_COMMAND" in s["env"]:
            s["env"]["CIBW_TEST_COMMAND"] = "python {project}/scripts/wheel_m1_spots.py {project}"
    assert any("readme_spots.py --run" in x for x in wl.lint(mut, pub))
    # M4b (0.2.9): a wheel leg without the bare-install SERVER gate (the same mutant dropped it too) -> RED
    assert any("wheel_server_spot.py" in x for x in wl.lint(mut, pub))
    mut = copy.deepcopy(rel)
    for s in mut["jobs"]["wheel"]["steps"]:
        if isinstance(s.get("env"), dict) and "CIBW_TEST_COMMAND" in s["env"]:
            s["env"]["CIBW_TEST_COMMAND"] = s["env"]["CIBW_TEST_COMMAND"].replace(" && python {project}/scripts/wheel_server_spot.py {project}", "")
            assert "wheel_server_spot.py" not in s["env"]["CIBW_TEST_COMMAND"]  # the mutation landed (never vacuous)
    assert any("wheel_server_spot.py" in x for x in wl.lint(mut, pub)) and not any("readme_spots.py --run" in x for x in wl.lint(mut, pub))
    # P1: the testpypi gate needing R-TP (a cycle: R-TP is produced by that job); the pypi gate missing R-TV
    mutp = copy.deepcopy(pub)
    for s in mutp["jobs"]["testpypi"]["steps"]:
        if "require_evidence.py" in str(s.get("run", "")):
            s["run"] = s["run"].replace("--need R-NI", "--need R-NI R-TP")
    assert any("`testpypi` gate must be exactly" in x for x in wl.lint(rel, mutp))
    mutp = copy.deepcopy(pub)
    for s in mutp["jobs"]["pypi"]["steps"]:
        if "require_evidence.py" in str(s.get("run", "")):
            s["run"] = s["run"].replace("--need R-NI R-TP R-TV", "--need R-NI R-TP")
    assert any("`pypi` gate must be exactly" in x for x in wl.lint(rel, mutp))
    # P1: a rebuild sneaks back in
    mutp = copy.deepcopy(pub)
    mutp["jobs"]["pypi"]["steps"].insert(-1, {"name": "rebuild", "run": "python -m build"})
    assert any("REBUILDS" in x for x in wl.lint(rel, mutp))
    # P1: the --rtp binding dropped from the pypi job; verify_dist_manifest after the publish step
    mutp = copy.deepcopy(pub)
    for s in mutp["jobs"]["pypi"]["steps"]:
        if "verify_dist_manifest.py" in str(s.get("run", "")):
            s["run"] = s["run"].replace(" --rtp evidence/R-TP.json", "")
    assert any("--rtp" in x for x in wl.lint(rel, mutp))
    mutp = copy.deepcopy(pub)
    st = mutp["jobs"]["testpypi"]["steps"]
    vd = next(i for i, s in enumerate(st) if "verify_dist_manifest.py" in str(s.get("run", "")))
    st.append(st.pop(vd))
    assert any("BEFORE the publish action" in x for x in wl.lint(rel, mutp))
    # P1: a 4-target validate matrix; no R-TV assembler; a download before the evidence gate
    mutp = copy.deepcopy(pub)
    mutp["jobs"]["testpypi-validate"]["strategy"]["matrix"]["include"].pop()
    assert any("matrix targets" in x for x in wl.lint(rel, mutp))
    mutp = copy.deepcopy(pub)
    mutp["jobs"]["testpypi-evidence"]["steps"] = [s for s in mutp["jobs"]["testpypi-evidence"]["steps"] if "assemble_rtv.py" not in str(s.get("run", ""))]
    assert any("assembles R-TV" in x for x in wl.lint(rel, mutp))
    mutp = copy.deepcopy(pub)
    st = mutp["jobs"]["pypi"]["steps"]
    dl = next(i for i, s in enumerate(st) if "gh run download" in str(s.get("run", "")))
    st.insert(1, st.pop(dl))
    assert any("precedes the evidence gate" in x for x in wl.lint(rel, mutp))
    # E3: a skip_evidence dispatch input / an `if:` on the gate -> RED (the M4 rule, re-asserted on the M6 file)
    mutp = copy.deepcopy(pub)
    (mutp.get("on") or mutp.get(True))["workflow_dispatch"]["inputs"]["skip_evidence"] = {"type": "boolean", "default": False}
    assert any("exposes `skip_evidence`" in x for x in wl.lint(rel, mutp))


def test_b1_b2_b6_npm_publish_publication_gates_are_linted():
    """B1/B2/B6: the linter requires npm-publish to (B2) need node-free-evidence, (B1) run only on a
    tag ref, run guard_tag_version + require_evidence + require_ci_success + consume R-BA BEFORE
    publish.mjs --yes, and (B1) bind --manifest. The real workflow is GREEN; each mutation is RED."""
    rel, pub = _wf()
    assert wl.lint(rel, pub) == []
    npm = rel["jobs"]["npm-publish"]
    # v0.2.5: acceptance advisory -> npm needs only `manifest` (NOT node-free-evidence). Re-add in v0.2.6.
    assert set(npm["needs"]) == {"manifest"} and "refs/tags/" in str(npm.get("if", ""))
    # B1: drop the tag-ref if: -> RED
    mut = copy.deepcopy(rel); mut["jobs"]["npm-publish"].pop("if", None)
    assert any("must carry `if: startsWith(github.ref" in x for x in wl.lint(mut, pub))
    # B1: publish step without --manifest -> RED
    mut = copy.deepcopy(rel)
    for s in mut["jobs"]["npm-publish"]["steps"]:
        if "publish.mjs --yes" in str(s.get("run", "")):
            s["run"] = s["run"].replace(" --manifest release_manifest.json", "")
    assert any("must pass `--manifest" in x for x in wl.lint(mut, pub))
    # B6: drop the require_ci_success step -> RED
    mut = copy.deepcopy(rel)
    mut["jobs"]["npm-publish"]["steps"] = [s for s in mut["jobs"]["npm-publish"]["steps"] if "require_ci_success.py" not in str(s.get("run", ""))]
    assert any("require_ci_success.py (B6)" in x for x in wl.lint(mut, pub))
    # v0.2.5: the R-BA consume/validate step is REMOVED from npm-publish (acceptance advisory); the lint no
    # longer requires it. Re-add this negative control in v0.2.6 when the R-BA gate returns.
    # B1: guard_tag_version dropped -> RED
    mut = copy.deepcopy(rel)
    mut["jobs"]["npm-publish"]["steps"] = [s for s in mut["jobs"]["npm-publish"]["steps"] if "guard_tag_version.py" not in str(s.get("run", ""))]
    assert any("guard_tag_version.py (B1)" in x for x in wl.lint(mut, pub))


def test_s1_intermediate_testpypi_gate_is_r_ba_r_ni_not_self_referential_rtp(tmp_path):
    """opus M6 B1 -- the deadlock the other E3 tests MASKED by stubbing run 5005 as completed.
    `testpypi-validate`/`testpypi-evidence` run in the SAME run (5005) that produced R-TP/R-TV, so that run
    is still `in_progress` while they gate. The SHIPPED intermediate gate `--need R-BA R-NI` PASSES; a MUTANT
    that re-adds R-TP (or R-TV) is REFUSED, because that record's producer.run_id == 5005 is not yet completed
    -> the whole `target=testpypi` dispatch would deadlock (R-TV never emitted -> pypi unreachable)."""
    m, dist, _ = assemble(tmp_path)
    ev = _all_records(tmp_path / "ok", m, dist)
    api_inprog = StubAPI(
        runs={RUN_ID: run_rec(RUN_ID),
              5005: dict(run_rec(5005), path=".github/workflows/publish-pypi.yml",
                         status="in_progress", conclusion=None)},
        jobs={RUN_ID: _build_run_jobs()},  # B4: R-BA/R-NI producer jobs are in the (completed) build run
    )
    # SHIPPED intermediate gate: passes even while 5005 (the R-TP/R-TV producer) is in progress
    assert re_.verify_records(m, ["R-BA", "R-NI"], ev, env=promotion_env(), api=api_inprog) == []
    # MUTANT (the original over-binding): R-TP self-references the in-progress run -> refused (deadlock)
    p_rtp = re_.verify_records(m, ["R-BA", "R-NI", "R-TP"], ev, env=promotion_env(), api=api_inprog)
    assert any("R-TP: producer run 5005 is not a completed+successful run" in x for x in p_rtp), p_rtp
    p_rtv = re_.verify_records(m, ["R-BA", "R-NI", "R-TV"], ev, env=promotion_env(), api=api_inprog)
    assert any("R-TV: producer run 5005 is not a completed+successful run" in x for x in p_rtv), p_rtv


def test_s11_lint_catches_r_tv_added_to_the_testpypi_need_set():
    """S11: the workflow linter parses `--need` as a SET and compares it exactly per job. Adding R-TV
    to `testpypi` (a future deadlock: R-TV's producer testpypi-validate `needs: testpypi`) is now
    caught -- a superset that the old substring test (`--need R-BA R-NI` in run) missed.
    Paired control (mutation-verified): on the SHIPPED linter the mutated workflow is RED; reverting
    parse_need to a substring test makes it GREEN again (the vacuity the fix closes)."""
    rel, pub = _wf()
    assert wl.lint(rel, pub) == []  # the real workflows are clean
    # R-TV bolted onto the testpypi gate -- the exact codex S11 scenario
    mutp = copy.deepcopy(pub)
    for s in mutp["jobs"]["testpypi"]["steps"]:
        if "require_evidence.py" in str(s.get("run", "")):
            s["run"] = s["run"].replace("--need R-NI", "--need R-NI R-TV")
    problems = wl.lint(rel, mutp)
    assert any("`testpypi` gate must be exactly" in x for x in problems), problems
    # and R-TV added to an intermediate job is caught by S1's set comparison too
    mutp = copy.deepcopy(pub)
    for s in mutp["jobs"]["testpypi-validate"]["steps"]:
        if "require_evidence.py" in str(s.get("run", "")):
            s["run"] = s["run"].replace("--need R-NI", "--need R-NI R-TV")
    assert any("`testpypi-validate` gate must be exactly" in x for x in wl.lint(rel, mutp)), wl.lint(rel, mutp)
    # the parse_need primitive itself: superset != the required set
    assert wl.parse_need("require_evidence.py --need R-BA R-NI R-TV --evidence-dir e") == {"R-BA", "R-NI", "R-TV"}
    assert wl.parse_need("require_evidence.py --role promotion") is None
