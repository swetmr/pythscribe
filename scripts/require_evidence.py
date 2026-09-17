#!/usr/bin/env python3
"""ONE authority for release-evidence verification (spec 13-09-26, requirements §5.1 + §5.2; plan M4/M6.1).

    python scripts/require_evidence.py --role release-run --manifest release_manifest.json
    python scripts/require_evidence.py --role promotion   --manifest release_manifest.json \
        --need R-BA R-NI [R-TP R-TV] --evidence-dir evidence [--tag v<V>]

`--role` is argparse-REQUIRED with NO default (rev-7 SF-C): the role is the TRUSTED workflow-step
literal, never a manifest field and never a dispatch input (rev-6 SF-1). A step that omits it exits 2.

  release-run  (same-run consumers inside release.yml: node-free-acceptance, npm-publish, npm-identity)
      * manifest self-hash valid (`manifest_sha256` == the canonical hash of every other field);
      * run IDENTITY enforced: manifest.build_run_id == $GITHUB_RUN_ID and source_sha == $GITHUB_SHA
        (a self-consistent manifest from ANOTHER run at the same SHA is REFUSED, never re-classified);
      * attempt rule, re-run tolerant (rev-7 SF-B): manifest.build_run_attempt <= $GITHUB_RUN_ATTEMPT;
      * the FIXED constant REQUIRED_PREREQ_JOBS (`prepare`, the 5 `Build <triple>` legs, `manifest`)
        must ALL be present with conclusion == success on each job's LATEST execution with
        run_attempt <= $GITHUB_RUN_ATTEMPT (GET /runs/{id}/jobs?filter=all) -- NEVER
        manifest.prerequisite_jobs (informational; empty/absent changes nothing);
      * manifest.build_run_attempt == the `manifest` job's latest successful attempt (a stale
        manifest from a superseded attempt is refused);
      * it NEVER reads the run's own status/conclusion (in_progress by definition while it runs).
  promotion    (publish-pypi.yml testpypi / testpypi-validate / pypi, after release.yml has ended)
      * self-hash valid; source_sha == $GITHUB_SHA (the dispatched tag ref's commit S);
      * build_run_id resolves (Actions API) to a release.yml run with head_sha == S,
        status == completed, conclusion == success;
      * manifest.build_run_attempt == the `manifest` job's latest successful execution (rev-8), NOT
        the run's latest attempt (a retried acceptance leg bumps the run attempt without
        re-generating the manifest);
      * every record in --need (R-BA, R-NI, R-TP, R-TV) exists under --evidence-dir, is bound to S +
        this manifest's hash, has verdict == pass, covers EXACTLY its required target set, its per-target
        content is complete (R-BA: every step of RBA_REQUIRED_STEPS == pass, node_free true, installed
        bytes == manifest native_sha256; R-TV: A0..A4; R-TP: uploaded files == manifest hashes), and
        its producer run is verified via the API (head_sha == S, completed + success).

Evidence-record schema (requirements §5.2; producers: M4 R-BA, M6 R-NI/R-TP/R-TV):
  {record, schema, source_sha, manifest_sha256, producer: {workflow, job, run_id, run_attempt,
   head_sha}, prerequisite_jobs_verified: [...], targets: {<key>: {...}}, verdict: "pass"|"fail"}
R-BA per-target: {wheel_tag, topology, steps: {A0, A0b, A1, A2, A3, A4, A4b, A5, A5b: "pass"|"fail"},
   node_free: bool, installed_binary_sha256, record_sha256, manifest_native_sha256, wheel_sha256}.
R-NI per-package (scripts/verify_npm_identity.py, M6.2): {kind: platform|payload|wrapper, verdict,
   published_tarball_sha256, platform: {triple, binary_sha256 (== manifest native_sha256[triple] ==
   the wheel's binary)}, payload: {files_sha256 (== canonical hash of the manifest's payload files)}}.
R-TP (scripts/assemble_rtp.py, M6.3): {uploaded_files: {filename: sha256} == the manifest's distribution
   set (5 wheels + sdist), repository_url}.
R-TV per-target (scripts/assemble_rtv.py, M6.3): {steps: {A0..A4}, installed_wheel_sha256 (== manifest
   wheel_sha256), all_features: "pass" (the L5 all-features smoke against the PUBLIC artifact)}.

Exit: 0 GREEN; 1 RED (every problem printed to stderr); 2 usage.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any, Iterable, Protocol

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
from pythscribe.artifacts import manifest_self_hash  # noqa: E402  -- the ONE canonical self-hash
from pythscribe.build._native import EXPECTED_WHEEL_SET, TRIPLE_TO_TAG  # noqa: E402

ROLES = ("release-run", "promotion")
RECORDS = ("R-BA", "R-NI", "R-TP", "R-TV")
MANIFEST_SCHEMA = "pythscribe-release-manifest/1"
RECORD_SCHEMA = "pythscribe-release-evidence/1"

# The FIXED, COMPLETE prerequisite set for same-run consumers (plan M6.1; requirements §5.1). These are
# the jobs' DISPLAY names exactly as the Actions jobs API reports them (`name:` of release.yml, with the
# build matrix expanded); tests/pythscribe/test_wheel_m4_acceptance.py binds each name to a real
# release.yml job so a rename cannot leave this constant stale. NOT manifest.prerequisite_jobs.
REQUIRED_PREREQ_JOBS: frozenset[str] = frozenset({
    "prepare",
    "Build x86_64-unknown-linux-gnu",
    "Build aarch64-unknown-linux-gnu",
    "Build x86_64-apple-darwin",
    "Build aarch64-apple-darwin",
    "Build x86_64-pc-windows-msvc",
    "manifest",
})
MANIFEST_JOB = "manifest"
RELEASE_WORKFLOW_PATH = ".github/workflows/release.yml"

# Per-record required target sets and per-target content.
RBA_REQUIRED_STEPS: tuple[str, ...] = ("A0", "A0b", "A1", "A2", "A3", "A4", "A4b", "A5", "A5b")
RTV_REQUIRED_STEPS: tuple[str, ...] = ("A0", "A1", "A2", "A3", "A4")
RTV_ALL_FEATURES_FIELD = "all_features"  # L5: the all-features smoke against the PUBLIC (TestPyPI) artifact
WHEEL_TARGETS: frozenset[str] = frozenset(TRIPLE_TO_TAG)
# npm platform package -> Rust triple (the SAME mapping npm/build-platform-packages.mjs TARGETS encodes;
# tests bind the two so they cannot drift). `bin/pyths[.exe]` inside `@pythscribe/cli-<plat>@V` must hash to
# manifest.targets[triple].native_sha256 == the binary inside wheel `triple` (requirements §5.3, E1b).
NPM_PLATFORM_TO_TRIPLE: dict[str, str] = {
    "@pythscribe/cli-win32-x64": "x86_64-pc-windows-msvc",
    "@pythscribe/cli-linux-x64": "x86_64-unknown-linux-gnu",
    "@pythscribe/cli-linux-arm64": "aarch64-unknown-linux-gnu",
    "@pythscribe/cli-darwin-x64": "x86_64-apple-darwin",
    "@pythscribe/cli-darwin-arm64": "aarch64-apple-darwin",
}
NPM_PAYLOAD_PACKAGES: dict[str, str] = {"pyths-runtime": "runtime_payload", "create-pyths-app": "scaffolder_payload"}
# S9: the npm publish set AND the R-NI evidence set derive from ONE structural authority -- npm/packages.json,
# loaded by BOTH npm/publish.mjs (Node) and here (Python) -- not a regex-linked duplicate. Before S9 the
# pure-JS plugin packages were absent from NPM_PACKAGES, so R-NI never downloaded them (a stale/absent
# vite-plugin-pyths@V or next-plugin-pyths@V passed promotion). The platform/payload names in the authority
# are asserted to agree with the (triple / manifest-key) maps above, so a drift fails LOUD at import.
NPM_PACKAGES_AUTHORITY: dict = json.loads((REPO / "npm" / "packages.json").read_text(encoding="utf-8"))
NPM_PLUGIN_PACKAGES: frozenset[str] = frozenset(NPM_PACKAGES_AUTHORITY["plugins"])
NPM_WRAPPER: str = NPM_PACKAGES_AUTHORITY["wrapper"]
# S9 (codex pass-3): the plugin/wrapper package DIRECTORIES also derive from the ONE authority
# npm/packages.json (this DELETES the duplicate literal that lived in verify_npm_identity.py, where a
# newly-added plugin could be left unbound). Plugin dirs come straight from the `plugins` map; the
# wrapper lives at `npm/<wrapper>` -- the same directory publish.mjs's pkgDir() resolves. Repo-root-relative.
NPM_PLUGIN_WRAPPER_DIRS: dict[str, str] = {**NPM_PACKAGES_AUTHORITY["plugins"], NPM_WRAPPER: f"npm/{NPM_WRAPPER}"}
assert set(NPM_PACKAGES_AUTHORITY["platform"]) == set(NPM_PLATFORM_TO_TRIPLE), "npm/packages.json platform set != NPM_PLATFORM_TO_TRIPLE"
assert set(NPM_PACKAGES_AUTHORITY["payload"]) == set(NPM_PAYLOAD_PACKAGES), "npm/packages.json payload set != NPM_PAYLOAD_PACKAGES"
NPM_PACKAGES: frozenset[str] = frozenset({*NPM_PLATFORM_TO_TRIPLE, *NPM_PAYLOAD_PACKAGES, *NPM_PLUGIN_PACKAGES, NPM_WRAPPER})
REQUIRED_TARGETS: dict[str, frozenset[str]] = {
    "R-BA": WHEEL_TARGETS,
    "R-NI": NPM_PACKAGES,
    "R-TP": frozenset(),  # R-TP binds `uploaded_files` to the manifest's distribution set instead
    "R-TV": WHEEL_TARGETS,
}
# Which workflow produces which record (a record claiming the wrong producer is refused).
RECORD_PRODUCER_WORKFLOW: dict[str, str] = {"R-BA": "release.yml", "R-NI": "release.yml", "R-TP": "publish-pypi.yml", "R-TV": "publish-pypi.yml"}
# B4: the FIXED job that ASSEMBLES each record (the job whose step runs the assembler script) -- NOT
# the matrix job the evidence is collected FROM. `check_record` requires the record to name this exact
# job, and `verify_records` finds THIS job in the producer run and requires ITS OWN conclusion to be
# success -- so a run that is completed+success overall while THIS job was skipped/absent (e.g. a
# target=pypi run where the testpypi-* jobs never ran) cannot authenticate a forged record.
RECORD_PRODUCER_JOB: dict[str, str] = {
    "R-BA": "node-free-evidence",   # assemble_rba.py runs here (NOT the node-free-acceptance matrix)
    "R-NI": "npm-identity",         # verify_npm_identity.py runs here
    "R-TP": "testpypi",             # assemble_rtp.py runs here
    "R-TV": "testpypi-evidence",    # assemble_rtv.py runs here (NOT the testpypi-validate matrix)
}


def files_map_sha256(files: dict[str, str]) -> str:
    """Canonical hash of a `{path: sha256}` payload map (sorted keys, compact JSON) -- the ONE way a
    record binds a payload's contents to the manifest's `runtime_payload.files` / `scaffolder_payload.files`."""
    return hashlib.sha256(json.dumps(dict(sorted(files.items())), separators=(",", ":"), sort_keys=True).encode("utf-8")).hexdigest()


def distribution_set(manifest: dict) -> dict[str, str]:
    """`{filename: sha256}` of everything the manifest binds for PyPI: the 5 wheels + the sdist."""
    want = {manifest["targets"][t]["wheel_filename"]: manifest["targets"][t]["wheel_sha256"] for t in manifest["targets"]}
    sd = manifest.get("sdist") or {}
    if sd.get("filename"):
        want[sd["filename"]] = sd.get("sha256")
    return want


# ----------------------------------------------------------------------------- Actions API (injectable)


class ActionsAPI(Protocol):
    def get_run(self, run_id: int) -> dict: ...
    def list_jobs(self, run_id: int) -> list[dict]: ...
    # B4 (d): the immutable-artifact binding. Optional on injected stubs (verify_records enforces it
    # only when BOTH are present); the production GitHubActionsAPI always implements them.
    def list_artifacts(self, run_id: int) -> list[dict]: ...
    def read_artifact_member(self, artifact_id: int, member: str) -> bytes: ...


class GitHubActionsAPI:
    """The real client: GET /repos/{repo}/actions/runs/{id} and /jobs?filter=all (paginated)."""

    def __init__(self, repo: str, token: str | None, api_url: str = "https://api.github.com"):
        self.repo, self.token, self.api_url = repo, token, api_url.rstrip("/")

    def _get(self, path: str, params: dict[str, Any] | None = None) -> Any:
        url = f"{self.api_url}/repos/{self.repo}{path}"
        if params:
            url += "?" + urllib.parse.urlencode(params)
        req = urllib.request.Request(url, headers={
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            **({"Authorization": f"Bearer {self.token}"} if self.token else {}),
        })
        with urllib.request.urlopen(req, timeout=60) as r:  # noqa: S310 -- fixed https API host
            return json.loads(r.read().decode("utf-8"))

    def get_run(self, run_id: int) -> dict:
        return self._get(f"/actions/runs/{run_id}")

    def list_jobs(self, run_id: int) -> list[dict]:
        jobs: list[dict] = []
        page = 1
        while True:
            chunk = self._get(f"/actions/runs/{run_id}/jobs", {"filter": "all", "per_page": 100, "page": page})
            jobs.extend(chunk.get("jobs", []))
            if len(chunk.get("jobs", [])) < 100:
                return jobs
            page += 1

    def list_artifacts(self, run_id: int) -> list[dict]:
        arts: list[dict] = []
        page = 1
        while True:
            chunk = self._get(f"/actions/runs/{run_id}/artifacts", {"per_page": 100, "page": page})
            items = chunk.get("artifacts", [])
            arts.extend(items)
            if len(items) < 100:
                return arts
            page += 1

    def read_artifact_member(self, artifact_id: int, member: str) -> bytes:
        """Download the run's immutable artifact zip and return one member's bytes (B4 (d))."""
        import io
        import zipfile
        url = f"{self.api_url}/repos/{self.repo}/actions/artifacts/{artifact_id}/zip"
        req = urllib.request.Request(url, headers={
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            **({"Authorization": f"Bearer {self.token}"} if self.token else {}),
        })
        with urllib.request.urlopen(req, timeout=120) as r:  # noqa: S310 -- fixed https API host (follows the signed redirect)
            blob = r.read()
        with zipfile.ZipFile(io.BytesIO(blob)) as z:
            return z.read(member)


def api_from_env(env: dict[str, str]) -> GitHubActionsAPI:
    repo = env.get("GITHUB_REPOSITORY")
    if not repo:
        raise SystemExit("require_evidence: GITHUB_REPOSITORY is not set")
    return GitHubActionsAPI(repo, env.get("GITHUB_TOKEN") or env.get("GH_TOKEN"), env.get("GITHUB_API_URL", "https://api.github.com"))


# ----------------------------------------------------------------------------- job-execution helpers


def latest_executions(jobs: Iterable[dict], *, max_attempt: int | None = None) -> dict[str, dict]:
    """Group the run's job executions by display name; keep each job's LATEST execution whose
    run_attempt <= max_attempt (None = no bound). The latest execution wins -- an attempt-1 success
    superseded by an attempt-2 failure is a failure."""
    latest: dict[str, dict] = {}
    for j in jobs:
        attempt = int(j.get("run_attempt", 1))
        if max_attempt is not None and attempt > max_attempt:
            continue
        cur = latest.get(j["name"])
        if cur is None or attempt > int(cur.get("run_attempt", 1)):
            latest[j["name"]] = j
    return latest


def latest_successful_attempt(jobs: Iterable[dict], name: str, *, max_attempt: int | None = None) -> int | None:
    best: int | None = None
    for j in jobs:
        attempt = int(j.get("run_attempt", 1))
        if j.get("name") != name or j.get("conclusion") != "success":
            continue
        if max_attempt is not None and attempt > max_attempt:
            continue
        best = attempt if best is None else max(best, attempt)
    return best


# ----------------------------------------------------------------------------- manifest binding


def _int(v: Any) -> int | None:
    """LENIENT parse -- ONLY for values that arrive as trusted runner-env STRINGS (GITHUB_RUN_ATTEMPT)
    or as GitHub Actions-API job dicts (already JSON ints). NEVER for a forgeable evidence-record /
    manifest field: those go through `_strict_pos_int` (see B4)."""
    try:
        return int(v)
    except (TypeError, ValueError):
        return None


def _strict_pos_int(v: Any) -> int | None:
    """B4 (codex pass-3): a strict POSITIVE JSON integer. `type(v) is int` rejects bool (its type is
    bool, so `True`/`False` are refused even though `isinstance(True, int)`) AND float AND str -- so a
    record/manifest carrying `true`, `1.5`, or `"1"` cannot authenticate against attempt/run 1 (they all
    used to collapse to int 1). Returns the int only when it is a genuine positive int, else None."""
    return v if (type(v) is int and v > 0) else None


# Small, named rules -- each is the exact site a spec mutant (validation §E0) attacks, so the tests can
# monkeypatch ONE of them and prove the fixture goes RED (the rule is live, not decorative).
def attempt_rule(manifest_attempt: int, current_attempt: int) -> bool:
    """release-run: re-run tolerant (rev-7 SF-B) -- a partial re-run bumps the run attempt without
    re-executing successful jobs, so the attempt-1 manifest is the legitimate artifact of attempt 2."""
    return manifest_attempt <= current_attempt


def run_complete_ok(run: dict) -> list[str]:
    """promotion: the whole release run must be finished AND green."""
    p: list[str] = []
    if run.get("status") != "completed":
        p.append(f"run {run.get('id')} status {run.get('status')!r} != completed (promotion needs the whole release run finished)")
    if run.get("conclusion") != "success":
        p.append(f"run {run.get('id')} conclusion {run.get('conclusion')!r} != success")
    return p


def required_prereq_jobs(manifest: dict) -> frozenset[str]:
    """The FIXED constant. `manifest.prerequisite_jobs` is informational and never consulted."""
    return REQUIRED_PREREQ_JOBS


def check_manifest_shape(m: dict) -> list[str]:
    p: list[str] = []
    if not isinstance(m, dict):
        return ["manifest is not a JSON object"]
    for k in ("schema", "version", "tag", "source_sha", "build_run_id", "build_run_attempt", "targets", "sdist",
              "runtime_payload", "scaffolder_payload", "expected_wheel_set", "manifest_sha256"):
        if k not in m:
            p.append(f"manifest: missing field `{k}`")
    if p:
        return p
    for k in ("runtime_payload", "scaffolder_payload"):
        files = (m.get(k) or {}).get("files") if isinstance(m.get(k), dict) else None
        if not isinstance(files, dict) or not files:
            p.append(f"manifest: {k}.files missing/empty (the prepared payload's `{{path: sha256}}` map)")
    if not isinstance(m.get("sdist"), dict) or not m["sdist"].get("filename") or not m["sdist"].get("sha256"):
        p.append("manifest: sdist {filename, sha256} missing")
    if m["schema"] != MANIFEST_SCHEMA:
        p.append(f"manifest: schema {m['schema']!r} != {MANIFEST_SCHEMA!r}")
    if m["manifest_sha256"] != manifest_self_hash(m):
        p.append("manifest: self-hash mismatch (manifest_sha256 != canonical hash of the other fields) -- tampered or hand-edited")
    if _strict_pos_int(m["build_run_id"]) is None or _strict_pos_int(m["build_run_attempt"]) is None:
        p.append("manifest: build_run_id / build_run_attempt must be strict positive integers (a bool/float/string is refused)")
    if not isinstance(m["targets"], dict) or set(m["targets"]) != WHEEL_TARGETS:
        p.append(f"manifest: targets {sorted(m['targets']) if isinstance(m['targets'], dict) else m['targets']!r} != the release target set {sorted(WHEEL_TARGETS)}")
    else:
        for t, info in m["targets"].items():
            for k in ("native_sha256", "wheel_filename", "wheel_sha256", "wheel_tag"):
                if not isinstance(info, dict) or not info.get(k):
                    p.append(f"manifest: targets[{t}] missing `{k}`")
            if isinstance(info, dict) and info.get("wheel_tag") != TRIPLE_TO_TAG[t]:
                p.append(f"manifest: targets[{t}].wheel_tag {info.get('wheel_tag')!r} != {TRIPLE_TO_TAG[t]!r}")
    if isinstance(m.get("expected_wheel_set"), list) and set(m["expected_wheel_set"]) != EXPECTED_WHEEL_SET:
        p.append(f"manifest: expected_wheel_set {sorted(m['expected_wheel_set'])} != {sorted(EXPECTED_WHEEL_SET)}")
    return p


def verify_manifest_binding(manifest: dict, role: str, *, env: dict[str, str], api: ActionsAPI, producer: bool = False) -> list[str]:
    """RED lines (empty == GREEN). `role` is the trusted step literal; it is validated, never inferred.

    `producer` (release-run only): the caller IS the `manifest` job, running this self-check WHILE it is
    still in-progress. The `manifest` job cannot require its OWN conclusion to be `success` yet, so it is
    excluded from the prerequisite-jobs check (the run's build prereqs -- prepare + the 5 Build legs --
    are still verified, as is run identity). Same-run CONSUMERS (acceptance / npm-publish / npm-identity)
    call WITHOUT producer, so they still require `manifest` to have succeeded."""
    if role not in ROLES:
        return [f"unknown role {role!r} (expected one of {ROLES})"]
    p = check_manifest_shape(manifest)
    if p:
        return p
    sha = env.get("GITHUB_SHA", "")
    if not sha:
        return ["GITHUB_SHA is not set (the source SHA S is the workflow's checkout, never an input)"]
    if manifest["source_sha"] != sha:
        p.append(f"manifest.source_sha {manifest['source_sha']} != GITHUB_SHA {sha}")
    run_id = int(manifest["build_run_id"])
    m_attempt = int(manifest["build_run_attempt"])

    if role == "release-run":
        cur_run = env.get("GITHUB_RUN_ID", "")
        cur_attempt = _int(env.get("GITHUB_RUN_ATTEMPT", ""))
        if not cur_run or cur_attempt is None:
            return p + ["GITHUB_RUN_ID / GITHUB_RUN_ATTEMPT are not set (release-run role needs the run identity)"]
        if str(run_id) != cur_run:
            p.append(f"manifest.build_run_id {run_id} != GITHUB_RUN_ID {cur_run} -- a manifest from another run is refused in release-run role (never re-classified as promotion)")
        if not attempt_rule(m_attempt, cur_attempt):
            p.append(f"manifest.build_run_attempt {m_attempt} is not admissible at GITHUB_RUN_ATTEMPT {cur_attempt}")
        if p:
            return p  # identity failed: do not consult the API for a foreign run
        jobs = api.list_jobs(run_id)
        latest = latest_executions(jobs, max_attempt=cur_attempt)
        req = required_prereq_jobs(manifest)
        if producer:
            req = req - {MANIFEST_JOB}  # the manifest job runs this while in-progress; it cannot require itself
        for name in sorted(req):
            j = latest.get(name)
            if j is None:
                p.append(f"required prerequisite job `{name}` has no execution in run {run_id} (attempt <= {cur_attempt})")
            elif j.get("conclusion") != "success":
                p.append(f"required prerequisite job `{name}`: latest execution (attempt {j.get('run_attempt')}) concluded {j.get('conclusion')!r}, not success")
        ok_attempt = latest_successful_attempt(jobs, MANIFEST_JOB, max_attempt=cur_attempt)
        if ok_attempt is not None and ok_attempt != m_attempt:
            p.append(f"manifest.build_run_attempt {m_attempt} != the `{MANIFEST_JOB}` job's latest successful attempt {ok_attempt} (stale manifest from a superseded attempt)")
        return p

    # promotion: whole-run completed + success at head_sha == S, then the manifest-job attempt binding
    run = api.get_run(run_id)
    if run.get("head_sha") != sha:
        p.append(f"run {run_id} head_sha {run.get('head_sha')} != S {sha}")
    p += run_complete_ok(run)
    if run.get("path") and not str(run["path"]).endswith(RELEASE_WORKFLOW_PATH.rsplit("/", 1)[-1]):
        p.append(f"run {run_id} is a {run['path']} run, not {RELEASE_WORKFLOW_PATH}")
    if p:
        return p
    jobs = api.list_jobs(run_id)
    latest = latest_executions(jobs)
    mj = latest.get(MANIFEST_JOB)
    if mj is None or mj.get("conclusion") != "success":
        p.append(f"`{MANIFEST_JOB}` job: no successful latest execution in run {run_id}")
        return p
    ok_attempt = latest_successful_attempt(jobs, MANIFEST_JOB)
    if ok_attempt != m_attempt:
        p.append(f"manifest.build_run_attempt {m_attempt} != the `{MANIFEST_JOB}` job's latest successful attempt {ok_attempt} (a re-generated manifest supersedes this one)")
    return p


# ----------------------------------------------------------------------------- evidence records (promotion)


def _steps_ok(steps: Any, required: tuple[str, ...], where: str) -> list[str]:
    p: list[str] = []
    if not isinstance(steps, dict):
        return [f"{where}: `steps` missing (a record without step verdicts cannot be accepted)"]
    for s in required:
        if s not in steps:
            p.append(f"{where}: step `{s}` absent (a skipped check is a failure, not a pass)")
        elif steps[s] != "pass":
            p.append(f"{where}: step `{s}` == {steps[s]!r}")
    return p


def check_record(name: str, rec: dict, manifest: dict, *, source_sha: str) -> list[str]:
    """Structural + binding checks of one evidence record against the manifest (no API)."""
    p: list[str] = []
    if not isinstance(rec, dict):
        return [f"{name}: not a JSON object"]
    if rec.get("record") != name:
        p.append(f"{name}: record field {rec.get('record')!r} != {name!r}")
    if rec.get("schema") != RECORD_SCHEMA:
        p.append(f"{name}: schema {rec.get('schema')!r} != {RECORD_SCHEMA!r}")
    if rec.get("source_sha") != source_sha:
        p.append(f"{name}: source_sha {rec.get('source_sha')} != S {source_sha}")
    if rec.get("manifest_sha256") != manifest.get("manifest_sha256"):
        p.append(f"{name}: bound to manifest {rec.get('manifest_sha256')}, not this manifest {manifest.get('manifest_sha256')}")
    if rec.get("verdict") != "pass":
        p.append(f"{name}: verdict {rec.get('verdict')!r} != pass")
    prod = rec.get("producer")
    if not isinstance(prod, dict) or not all(k in prod for k in ("workflow", "job", "run_id", "run_attempt", "head_sha")):
        p.append(f"{name}: producer block incomplete")
    elif prod.get("head_sha") != source_sha:
        p.append(f"{name}: producer head_sha {prod.get('head_sha')} != S {source_sha}")
    elif name in ("R-BA", "R-NI") and str(prod.get("run_id")) != str(manifest.get("build_run_id")):
        p.append(f"{name}: producer run {prod.get('run_id')} != the manifest's build run {manifest.get('build_run_id')} (same-run records must come from the run that built the manifest)")
    elif str(prod.get("workflow", "")).rsplit("/", 1)[-1] != RECORD_PRODUCER_WORKFLOW[name]:
        p.append(f"{name}: producer workflow {prod.get('workflow')!r} != {RECORD_PRODUCER_WORKFLOW[name]!r}")
    elif prod.get("job") != RECORD_PRODUCER_JOB[name]:
        p.append(f"{name}: producer job {prod.get('job')!r} != the FIXED assembler job {RECORD_PRODUCER_JOB[name]!r} (a record must name the job that assembled it, not an arbitrary/collected-from job)")
    targets = rec.get("targets")
    if not isinstance(targets, dict):
        return p + [f"{name}: `targets` missing"]
    need = REQUIRED_TARGETS[name]
    if need and set(targets) != need:
        p.append(f"{name}: targets {sorted(targets)} != required {sorted(need)} (a subset -- a skipped leg -- is a failure)")
    if name == "R-BA":
        for t in sorted(need & set(targets)):
            info = targets[t]
            where = f"R-BA[{t}]"
            if not isinstance(info, dict):
                p.append(f"{where}: not an object")
                continue
            p += _steps_ok(info.get("steps"), RBA_REQUIRED_STEPS, where)
            if info.get("node_free") is not True:
                p.append(f"{where}: node_free != true")
            want = manifest["targets"][t]["native_sha256"]
            if info.get("installed_binary_sha256") != want:
                p.append(f"{where}: installed_binary_sha256 {info.get('installed_binary_sha256')} != manifest native_sha256 {want}")
            if info.get("record_sha256") != want:
                p.append(f"{where}: RECORD digest {info.get('record_sha256')} != manifest native_sha256 {want}")
            if info.get("wheel_sha256") != manifest["targets"][t]["wheel_sha256"]:
                p.append(f"{where}: wheel_sha256 {info.get('wheel_sha256')} != manifest wheel_sha256")
    elif name == "R-TV":
        for t in sorted(need & set(targets)):
            info = targets[t]
            where = f"R-TV[{t}]"
            if not isinstance(info, dict):
                p.append(f"{where}: not an object")
                continue
            p += _steps_ok(info.get("steps"), RTV_REQUIRED_STEPS, where)
            if info.get("installed_wheel_sha256") != manifest["targets"][t]["wheel_sha256"]:
                p.append(f"{where}: installed_wheel_sha256 != manifest wheel_sha256")
            if RTV_ALL_FEATURES_FIELD not in info:
                p.append(f"{where}: `{RTV_ALL_FEATURES_FIELD}` absent (the L5 all-features smoke against the public artifact was not run -- a skipped check is a failure)")
            elif info.get(RTV_ALL_FEATURES_FIELD) != "pass":
                p.append(f"{where}: {RTV_ALL_FEATURES_FIELD} == {info.get(RTV_ALL_FEATURES_FIELD)!r}")
    elif name == "R-TP":
        want = distribution_set(manifest)
        got = rec.get("uploaded_files")
        if got != want:
            p.append(f"R-TP: uploaded_files != the manifest's distribution set ({sorted(want)})")
        # B3: the registry must have SERVED exactly the manifest bytes (sdist included) -- proof that
        # skip-existing did not leave a different pre-existing artifact that R-TV (wheels only) missed.
        served = rec.get("registry_files")
        if served != want:
            p.append(f"R-TP: registry_files != the manifest's distribution set -- the registry did not serve exactly the manifest bytes (sdist included); got {sorted(served) if isinstance(served, dict) else served!r}")
    elif name == "R-NI":
        for pkg in sorted(need & set(targets)):
            info = targets[pkg]
            where = f"R-NI[{pkg}]"
            if not isinstance(info, dict) or info.get("verdict") != "pass":
                p.append(f"{where}: per-package verdict != pass")
                continue
            if not info.get("published_tarball_sha256"):
                p.append(f"{where}: no published_tarball_sha256 (the package was not downloaded from the registry)")
            if pkg in NPM_PLATFORM_TO_TRIPLE:
                triple = NPM_PLATFORM_TO_TRIPLE[pkg]
                want_bin = manifest["targets"][triple]["native_sha256"]
                got_bin = (info.get("platform") or {}).get("binary_sha256")
                if got_bin != want_bin:
                    p.append(f"{where}: published bin/pyths sha256 {got_bin} != manifest native_sha256[{triple}] {want_bin} (same-version-different-binary -- the alreadyPublished skip)")
            elif pkg in NPM_PAYLOAD_PACKAGES:
                files = (manifest.get(NPM_PAYLOAD_PACKAGES[pkg]) or {}).get("files") or {}
                want_files = files_map_sha256(files) if files else None
                got_files = (info.get("payload") or {}).get("files_sha256")
                if not files or got_files != want_files:
                    p.append(f"{where}: published payload files hash {got_files} != manifest {NPM_PAYLOAD_PACKAGES[pkg]} {want_files}")
            elif pkg in NPM_PLUGIN_PACKAGES or pkg == NPM_WRAPPER:
                # S9/NEW-wrapper (codex pass-3): the plugin/wrapper content MUST be bound to the checkout
                # of S. Before, `check_record` only compared hashes when `content_bound` was truthy, so an
                # R-NI carrying `content_bound: false` for every package was ACCEPTED (a VACUOUS gate --
                # the immutable-registry hole wide open). Now the binding is MANDATORY at the consumer:
                # `content_bound` must be exactly True AND both the registry and checkout content hashes
                # must be present AND equal. A false/absent content_bound, a missing hash, or a mismatch
                # (a lying producer that recorded a diff but flipped the verdict) is RED.
                cblk = info.get("plugin") or info.get("wrapper") or {}
                reg, chk = cblk.get("registry_files_sha256"), cblk.get("checkout_files_sha256")
                if cblk.get("content_bound") is not True:
                    p.append(f"{where}: content_bound != true -- the plugin/wrapper published content was NOT bound to the checkout of S (a vacuous R-NI); refused. verify_npm_identity.py must run with --checkout")
                elif not reg or not chk:
                    p.append(f"{where}: content binding incomplete (registry_files_sha256={reg!r}, checkout_files_sha256={chk!r}) -- both hashes are required")
                elif reg != chk:
                    p.append(f"{where}: published content hash {reg} != checkout content hash {chk} (stale/malicious bytes at the same name@version)")
    return p


def verify_records(manifest: dict, need: Iterable[str], evidence_dir: Path, *, env: dict[str, str], api: ActionsAPI) -> list[str]:
    p: list[str] = []
    sha = env.get("GITHUB_SHA", "")
    for name in need:
        if name not in RECORDS:
            p.append(f"unknown record {name!r} (expected one of {RECORDS})")
            continue
        f = evidence_dir / f"{name}.json"
        if not f.is_file():
            p.append(f"{name}: record file {f} is missing")
            continue
        try:
            rec = json.loads(f.read_text(encoding="utf-8"))
        except ValueError as e:
            p.append(f"{name}: unreadable JSON ({e})")
            continue
        rp = check_record(name, rec, manifest, source_sha=sha)
        p += rp
        if rp:
            continue
        # B4: the producer run_id is a forgeable JSON field -> STRICT positive int (a bool/float/string
        # must not collapse to a real run id / disable the lookup).
        run_id = _strict_pos_int(rec["producer"]["run_id"])
        if run_id is None:
            p.append(f"{name}: producer.run_id {rec['producer'].get('run_id')!r} is not a positive integer -- refused")
            continue
        run = api.get_run(run_id)
        if run.get("head_sha") != sha or run_complete_ok(run):
            p.append(f"{name}: producer run {run_id} is not a completed+successful run at S (head_sha={run.get('head_sha')}, status={run.get('status')}, conclusion={run.get('conclusion')})")
            continue
        # B4: verify the run is the DECLARED workflow AND the NAMED assembler job of that run concluded
        # success ITSELF, at the record's attempt. A run that is green as a whole is not enough -- a
        # target=pypi dispatch is a completed+success publish-pypi.yml run whose testpypi/testpypi-*
        # jobs never ran, so a forged R-TP/R-TV pointing at it (with an arbitrary job/attempt) must fail.
        want_wf = RECORD_PRODUCER_WORKFLOW[name]
        if run.get("path") and str(run["path"]).rsplit("/", 1)[-1] != want_wf:
            p.append(f"{name}: producer run {run_id} is a {run.get('path')} run, not {want_wf}")
            continue
        want_job = RECORD_PRODUCER_JOB[name]
        # B4: a strict POSITIVE INTEGER attempt. `_int` (lenient) let `true`/`1.5`/`"1"` all become
        # attempt 1 -- authenticating against attempt 1. `_strict_pos_int` refuses bool/float/str and
        # non-positive, so a malformed attempt can neither spoof attempt 1 nor disable the attempt filter.
        rec_attempt = _strict_pos_int(rec["producer"].get("run_attempt"))
        if rec_attempt is None:
            p.append(f"{name}: producer.run_attempt {rec['producer'].get('run_attempt')!r} is not a positive integer -- refused (a bool/float/string/non-positive must not spoof attempt 1 or disable the attempt filter)")
            continue
        # B4: require EXACTLY ONE execution of the named assembler job at that attempt -- zero means the
        # job never ran (a forged producer / the wrong dispatch target); more than one is a job-name
        # collision. Any-success-accepts used to let a collided failed+success pair authenticate the record.
        matches = [j for j in api.list_jobs(run_id)
                   if j.get("name") == want_job and _int(j.get("run_attempt", 1)) == rec_attempt]
        if len(matches) != 1:
            p.append(f"{name}: expected EXACTLY ONE execution of producer job {want_job!r} at attempt {rec_attempt} in run {run_id}, found {len(matches)} "
                     "-- zero means the job did not run (a forged producer / the wrong dispatch target); more than one is a job-name collision; refused")
            continue
        if matches[0].get("conclusion") != "success":
            p.append(f"{name}: producer job {want_job!r} (attempt {rec_attempt}) did not conclude success (conclusion {matches[0].get('conclusion')!r}) -- the assembling job must itself be green")
            continue
        # B4 (d): bind the record BYTES to the run's IMMUTABLE, PER-ATTEMPT Actions artifact, not just
        # the mutable (--clobber'd) Release asset a forger can overwrite.
        p += artifact_binding_problems(name, f, run_id, rec_attempt, api)
    return p


def artifact_binding_problems(name: str, rec_file: Path, run_id: int, run_attempt: int, api: ActionsAPI) -> list[str]:
    """B4 (d): the record file's bytes must equal the `<name>.json` member of the run's IMMUTABLE
    Actions artifact `evidence-<name>-<attempt>` (the upload-artifact output of the producing run,
    which cannot be clobbered like a Release asset). The artifact name is PER-ATTEMPT (the producer
    suffixes it with `github.run_attempt`) so an HONEST rerun-recovery is ACCEPTED: attempt-1 and
    attempt-2 both leave `evidence-<name>-1` and `evidence-<name>-2` in the run (GitHub keeps same-name
    artifacts after a rerun), and this selects THIS record's own attempt's artifact -- not "exactly one
    across the whole run" (which falsely rejected a legitimate rerun, codex pass-3). Enforced whenever
    the API exposes the artifact methods (production always does; an injected stub without them skips
    the byte-download, a CI-only step). A missing/duplicated per-attempt artifact, an unreadable member,
    or a byte mismatch is RED.

    B4 secondary hardening (codex pass-4): the byte-binding now FAILS CLOSED when the API lacks the
    artifact surface -- an injected API without `list_artifacts`/`read_artifact_member` no longer SKIPS
    the immutable-artifact binding (the pass-4 gap). Production (`GitHubActionsAPI`) always implements
    both. The ONLY sanctioned skip is a UNIT stub that EXPLICITLY declares `SKIP_ARTIFACT_BINDING = True`
    (a class attribute a workflow/manifest/record can never set), so the byte-binding is exercised in CI
    by the real API while structural-logic unit tests opt out deliberately. Absent methods + no explicit
    opt-out => RED (production-shaped verification is mandatory)."""
    lister = getattr(api, "list_artifacts", None)
    reader = getattr(api, "read_artifact_member", None)
    if lister is None or reader is None:
        if getattr(api, "SKIP_ARTIFACT_BINDING", False) is True:
            return []  # explicit unit-test opt-out (the byte-binding is exercised in CI by the real API)
        return [f"{name}: verifier API lacks the immutable-artifact methods "
                "(list_artifacts / read_artifact_member) -- cannot bind the record to its per-attempt "
                "Actions artifact; fail closed (production-shaped verification is mandatory)"]
    want, member = f"evidence-{name}-{run_attempt}", f"{name}.json"
    try:
        arts = [a for a in lister(run_id) if a.get("name") == want and not a.get("expired")]
    except Exception as e:  # noqa: BLE001 -- an unqueryable artifact store is a FAILURE, never a pass
        return [f"{name}: could not list the producer run {run_id}'s Actions artifacts ({e}) -- cannot bind the record to an immutable artifact"]
    if len(arts) != 1:
        return [f"{name}: expected EXACTLY ONE immutable Actions artifact {want!r} (this attempt's) in run {run_id}, found {len(arts)} "
                "-- the record must be bound to the producing run's immutable per-attempt artifact, not only a mutable (clobberable) Release asset"]
    try:
        art_bytes = reader(int(arts[0]["id"]), member)
    except Exception as e:  # noqa: BLE001
        return [f"{name}: could not read member {member!r} of the immutable artifact {want!r} ({e})"]
    if art_bytes != rec_file.read_bytes():
        return [f"{name}: the evidence file bytes differ from the immutable Actions artifact {want!r} member {member!r} "
                "-- the Release asset was replaced/clobbered after the run; refused"]
    return []


# ----------------------------------------------------------------------------- CLI


def build_parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(prog="require_evidence.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--role", required=True, choices=ROLES,
                    help="the TRUSTED workflow-step role literal (required; there is no default and no manifest/input source)")
    ap.add_argument("--manifest", default="release_manifest.json")
    ap.add_argument("--need", nargs="*", default=[], choices=RECORDS, help="evidence records required (promotion role)")
    ap.add_argument("--evidence-dir", default="evidence")
    ap.add_argument("--tag", default=None, help="promotion: the dispatched tag ref; manifest.tag must equal it")
    ap.add_argument("--producer", action="store_true",
                    help="release-run: the caller IS the manifest job (self-check while in-progress); excludes the "
                         "`manifest` job from the prerequisite-jobs check. Same-run consumers omit this.")
    return ap


def main(argv: list[str] | None = None) -> int:
    ns = build_parser().parse_args(argv)
    env = dict(os.environ)
    try:
        manifest = json.loads(Path(ns.manifest).read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        print(f"require_evidence: RED -- cannot read manifest {ns.manifest}: {e}", file=sys.stderr)
        return 1
    api = api_from_env(env)
    if ns.producer and ns.role != "release-run":
        print("require_evidence: RED -- --producer is a release-run option (the manifest-job self-check)", file=sys.stderr)
        return 1
    problems = verify_manifest_binding(manifest, ns.role, env=env, api=api, producer=ns.producer)
    if ns.tag is not None and manifest.get("tag") != ns.tag:
        problems.append(f"manifest.tag {manifest.get('tag')!r} != dispatched tag {ns.tag!r}")
    if ns.role == "promotion" and not problems:
        problems += verify_records(manifest, ns.need, Path(ns.evidence_dir), env=env, api=api)
    elif ns.role == "release-run" and ns.need:
        problems.append("--need is a promotion-role option (same-run consumers verify identity + prerequisite jobs only)")
    if problems:
        print(f"require_evidence: RED ({len(problems)} problem(s), role={ns.role}):", file=sys.stderr)
        for line in problems:
            print(f"  {line}", file=sys.stderr)
        return 1
    print(f"require_evidence: GREEN -- role={ns.role} manifest={manifest.get('manifest_sha256', '')[:12]} run={manifest.get('build_run_id')} attempt={manifest.get('build_run_attempt')}"
          + (f" records={list(ns.need)}" if ns.need else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main())
