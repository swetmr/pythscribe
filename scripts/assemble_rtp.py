#!/usr/bin/env python3
"""Assemble the R-TP testpypi-publication evidence record (spec 13-09-26, requirements §5.2; plan M6.3).

    python scripts/assemble_rtp.py --manifest release_manifest.json --dist dist --repository-url https://test.pypi.org/legacy/ --out evidence/R-TP.json

Runs in publish-pypi.yml `testpypi` right AFTER the upload step succeeded. `uploaded_files` = the sha256 of every
file in `<dist>` -- the exact bytes handed to the uploader -- and the verdict is `pass` iff that set == the
manifest's distribution set (5 wheels + sdist, by filename AND hash; `verify_dist_manifest.py` asserted the same
before the upload, so a pass here means TestPyPI received the manifest-bound bytes). The record binds S, the
manifest hash and this run (producer publish-pypi.yml / testpypi); the `pypi` job later requires `uploaded_files`
== the dist it promotes (E2). Exit 0 pass / 1 fail (record written either way).
"""
from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Callable

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts"))
from require_evidence import RECORD_SCHEMA, distribution_set  # noqa: E402

PROJECT = "pythscribe"
# A callable `(project, version) -> {filename: registry-served sha256}` -- injectable for tests / offline.
RegistryFetcher = Callable[[str, str], dict[str, str]]


def sha256_file(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def json_api_base(repository_url: str) -> str:
    """The Warehouse JSON-API ORIGIN for an upload (legacy) URL. TestPyPI serves JSON on the SAME host it
    uploads to (`test.pypi.org`), but PRODUCTION PyPI uploads to `upload.pypi.org` and serves JSON on
    `pypi.org` -- so map the known upload host to its serve host, else the post-publish verify queries a
    host that has no JSON API and spuriously reports the version missing (codex 2026-09-18)."""
    u = urllib.parse.urlsplit(repository_url)
    if u.scheme and u.netloc:
        netloc = "pypi.org" if u.netloc == "upload.pypi.org" else u.netloc
        return f"{u.scheme}://{netloc}"
    return repository_url.rstrip("/")


def registry_files_via_json(repository_url: str, version: str, *, project: str = PROJECT, timeout: int = 60,
                            attempts: int = 18, backoff_s: float = 10.0, expected: set[str] | None = None) -> dict[str, str]:
    """B3: what the registry ACTUALLY serves for project@version -- `{filename: sha256}` from the
    Warehouse JSON API (`digests.sha256` is the registry's own hash of the bytes it will serve). This
    is the proof that skip-existing did not leave a DIFFERENT pre-existing file (e.g. a foreign sdist)
    in place: a mismatch against the manifest is RED even though the local upload "succeeded".

    The JSON API propagates AFTER the (irreversible) upload, and it can list SOME files before the rest,
    so retry a 404/transient-5xx OR an INCOMPLETE set until the full `expected` set (the manifest's
    filenames) is served -- returning on the first non-empty response would falsely RED a correct
    publish that is still mid-propagation (codex 2026-09-18). Without `expected`, fall back to "any
    non-empty". A persistently absent/incomplete version still fails after the retries, on its own
    merits (the caller records the manifest mismatch)."""
    base = json_api_base(repository_url)
    url = f"{base}/pypi/{urllib.parse.quote(project)}/{urllib.parse.quote(version)}/json"
    req = urllib.request.Request(url, headers={"Accept": "application/json"})
    served: dict[str, str] = {}
    for i in range(attempts):
        served = {}
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:  # noqa: S310 -- fixed https registry host
                data = json.loads(r.read().decode("utf-8"))
            for u in data.get("urls", []):
                if u.get("yanked"):
                    continue  # a YANKED file is skipped by normal installs -> treat as NOT served (codex astra), so a yanked release fails the exact-set proof instead of passing it
                fn = u.get("filename")
                sha = (u.get("digests") or {}).get("sha256")
                if fn and sha:
                    served[fn] = sha
            complete = (set(expected) <= set(served)) if expected else bool(served)
            if complete:
                return served  # the full expected set has propagated to this host
        except urllib.error.HTTPError as e:
            if e.code not in (404, 408, 429, 500, 502, 503, 504) or i == attempts - 1:
                raise
        except (urllib.error.URLError, OSError, ValueError, http.client.HTTPException, TimeoutError):
            # transient network/parse blips right after the (irreversible) upload -- RemoteDisconnected /
            # ConnectionReset (OSError), IncompleteRead / BadStatusLine (HTTPException), a truncated body ->
            # JSONDecodeError (ValueError). Retry within the bounded loop rather than RED a correct publish
            # on noise (fable pass B1). The last attempt still raises -> caller records the failure.
            if i == attempts - 1:
                raise
        if i < attempts - 1:
            have = len(served)
            need = len(expected) if expected else "?"
            print(f"registry JSON incomplete for {project}=={version} at {base} ({have}/{need} files); retry {i + 1}/{attempts - 1}", file=sys.stderr)
            time.sleep(backoff_s)
    return served  # incomplete after all retries -> the caller records the manifest mismatch (RED)


def assemble(manifest: dict, dist: Path, repository_url: str, env: dict[str, str],
             *, registry_files: dict[str, str] | None = None, fetch_registry: RegistryFetcher | None = None) -> dict:
    """R-TP: the LOCAL dist bytes handed to the uploader AND the bytes the registry SERVES must both
    equal the manifest distribution set (5 wheels + sdist). `registry_files` (an explicit served map)
    or `fetch_registry` (a callable) inject the registry for tests/offline; otherwise the Warehouse
    JSON API is queried. Verdict `pass` iff local uploaded == manifest AND registry-served == manifest."""
    uploaded = {q.name: sha256_file(q) for q in sorted(dist.iterdir()) if q.is_file()} if dist.is_dir() else {}
    want = distribution_set(manifest)
    problems: list[str] = []
    for name in sorted(set(want) - set(uploaded)):
        problems.append(f"{name}: in the manifest, not uploaded")
    for name in sorted(set(uploaded) - set(want)):
        problems.append(f"{name}: uploaded, not in the manifest's distribution set")
    for name in sorted(set(want) & set(uploaded)):
        if uploaded[name] != want[name]:
            problems.append(f"{name}: uploaded sha256 {uploaded[name]} != manifest {want[name]}")

    # B3: PROVE the registry serves exactly the manifest bytes (sdist included). skip-existing can leave
    # a DIFFERENT pre-existing artifact in place; the local hash would still pass, so this is the gate.
    registry_error: str | None = None
    if registry_files is None:
        try:
            if fetch_registry is not None:
                registry_files = fetch_registry(repository_url, manifest.get("version", ""))
            else:  # the real fetcher retries until the COMPLETE expected set has propagated (MEDIUM, codex)
                registry_files = registry_files_via_json(repository_url, manifest.get("version", ""), expected=set(want))
        except Exception as e:  # noqa: BLE001 -- a registry we cannot query is a FAILURE, never a pass
            registry_files = {}
            registry_error = f"could not query the registry {repository_url}: {e}"
    served = dict(registry_files)
    if registry_error:
        problems.append(registry_error)
    for name in sorted(set(want) - set(served)):
        problems.append(f"{name}: in the manifest, NOT served by the registry (skip-existing left a different/absent file?)")
    for name in sorted(set(served) - set(want)):
        problems.append(f"{name}: served by the registry but not in the manifest's distribution set")
    for name in sorted(set(want) & set(served)):
        if served[name] != want[name]:
            problems.append(f"{name}: registry-served sha256 {served[name]} != manifest {want[name]} (the registry serves DIFFERENT bytes than the manifest -- a pre-existing artifact)")

    # LOW (codex): the SAME assembler serves TestPyPI (R-TP, job testpypi) and the production PyPI
    # post-publish PROOF (job pypi). Label the record by the registry so the audit artifact does not
    # claim a TestPyPI job produced a production proof (it stays a verification artifact -- exit code is
    # the gate -- but the identity is now honest).
    is_pypi_prod = json_api_base(repository_url) == "https://pypi.org"
    return {
        "record": "R-PP" if is_pypi_prod else "R-TP",
        "schema": RECORD_SCHEMA,
        "source_sha": env.get("GITHUB_SHA", ""),
        "manifest_sha256": manifest.get("manifest_sha256"),
        "producer": {
            "workflow": "publish-pypi.yml", "job": "pypi" if is_pypi_prod else "testpypi",
            "run_id": int(env.get("GITHUB_RUN_ID", "0") or 0), "run_attempt": int(env.get("GITHUB_RUN_ATTEMPT", "0") or 0),
            "head_sha": env.get("GITHUB_SHA", ""),
        },
        "repository_url": repository_url,
        "registry_json_base": json_api_base(repository_url),
        "version": manifest.get("version"),
        "uploaded_files": uploaded,
        "registry_files": dict(sorted(served.items())),
        "targets": {},
        "problems": problems,
        "verdict": "pass" if not problems and uploaded and served else "fail",
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="assemble_rtp.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--manifest", required=True)
    ap.add_argument("--dist", required=True)
    ap.add_argument("--repository-url", required=True)
    ap.add_argument("--out", required=True)
    ns = ap.parse_args(argv)
    manifest = json.loads(Path(ns.manifest).read_text(encoding="utf-8"))
    rec = assemble(manifest, Path(ns.dist), ns.repository_url, dict(os.environ))
    out = Path(ns.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(rec, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"{rec['record']}: {rec['verdict']} -> {out} (uploaded {len(rec['uploaded_files'])}, registry-served {len(rec['registry_files'])})"
          + (f" problems={rec['problems']}" if rec["problems"] else ""))
    return 0 if rec["verdict"] == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
