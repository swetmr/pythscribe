"""M4 gates (spec 13-09-26-lib-selfcontained-pip-wheel; plan M4; validation §A + §E0) -- the LOCALLY verifiable
subset of the candidate-bound node-free acceptance, every gate with its PAIRED NEGATIVE CONTROL (the anti-vacuity paired-control convention),
the spec's mutants applied as REAL monkeypatches of the shipped rule (RED-on-revert evidence).

  E0  require_evidence.py -- role-literal manifest binding on FIXTURE manifests + a stubbed Actions jobs API:
      in-progress parent run (release-run PASSES on prerequisite jobs, promotion REFUSES); run-A manifest handed
      to run B -> REFUSED (a mutant selecting the mode from the manifest accepts); empty/absent prerequisite list
      -> the fixed set is still required, a missing build leg -> REFUSED (a mutant reading the manifest's list
      accepts); failed build leg -> REFUSED; the five re-run fixtures (i)-(v) incl. (i)/(iii) THROUGH promotion;
      tampered self-hash / wrong SHA / failed run -> REFUSED; no --role -> exit 2 (a default-role mutant exits 0).
      Mutants (each -> RED): whole-run conclusion in release-run; promotion accepting in_progress; strict `==`
      attempt; earliest-execution; ignoring the manifest-job binding; binding promotion to the run's attempt.
  A0  node detection: a planted `node` / `node.exe` / Playwright-driver `node` file is found by the filesystem
      walk -> RED; PATH hits -> RED; native legs: outside ~ctl -> RED, probe-reachable -> RED, no probe -> RED.
  A0b candidate binding: wrong-wheel sha -> refuse; wrong filename/tag -> refuse; a foreign pythscribe-* in the
      wheelhouse -> refuse; a wheelhouse copy with different bytes -> refuse.
  A1  installed identity: bytes == RECORD == manifest -> GREEN; stale-native (bytes swapped, RECORD untouched) ->
      RED (a RECORD-only mutant stays green: asserted); checkout-shadowing -> RED.
  A6  assemble_rba: all legs pass -> verdict pass; a deleted a0.json (skipped check) -> fail AND the promotion
      consumer refuses the record (no A0 field); 4/5 legs -> fail; the codex scenario (R-TV absent at pypi) -> refuse.
  WF  scripts/lint_release_workflows.py GREEN on the real files; RED on: an injected manifest<-acceptance cycle,
      a `uses:` step inside the native scrubbed window, a dropped restore step, a missing `--role`, a `role` input.
The container / native-runner acceptance RUN (A1-A5 on the 5 targets + R-BA emission) is CI-only (release.yml).
"""
from __future__ import annotations

import base64
import copy
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest
import yaml

from conftest import REPO
from pythscribe.artifacts import manifest_self_hash
from pythscribe.build._native import TRIPLE_TO_TAG

SCRIPTS = REPO / "scripts"
sys.path.insert(0, str(SCRIPTS))
import assemble_rba as rba  # noqa: E402
import lint_release_workflows as wl  # noqa: E402
import require_evidence as re_  # noqa: E402
import wheel_acceptance as wa  # noqa: E402

TRIPLES = sorted(TRIPLE_TO_TAG)
SHA_S = "a" * 40
SHA_OTHER = "b" * 40
VERSION = "0.2.5"


def _sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


# ----------------------------------------------------------------------------- fixtures: manifests, jobs API stub


def make_manifest(run_id: int = 1001, attempt: int = 1, sha: str = SHA_S, **over) -> dict:
    m = {
        "schema": re_.MANIFEST_SCHEMA, "version": VERSION, "tag": f"v{VERSION}", "source_sha": sha,
        "build_run_id": run_id, "build_run_attempt": attempt,
        "targets": {t: {"native_sha256": _sha(f"bin-{t}".encode()), "wheel_filename": f"pythscribe-{VERSION}-py3-none-{tag}.whl",
                        "wheel_sha256": _sha(f"whl-{t}".encode()), "wheel_tag": tag} for t, tag in TRIPLE_TO_TAG.items()},
        "sdist": {"filename": f"pythscribe-{VERSION}.tar.gz", "sha256": _sha(b"sdist")},
        "runtime_payload": {"tarball_sha256": _sha(b"rt.tgz"), "files": {"package.json": _sha(b"pj"), "src/core.js": _sha(b"core"), "LICENSE": _sha(b"mit")}},
        "scaffolder_payload": {"tarball_sha256": _sha(b"sc.tgz"), "files": {"package.json": _sha(b"spj"), "index.js": _sha(b"idx"), "LICENSE": _sha(b"mit")}},
        "expected_wheel_set": sorted(TRIPLE_TO_TAG.values()),
    }
    m.update(over)
    m["manifest_sha256"] = manifest_self_hash(m)
    return m


def job(name: str, attempt: int = 1, conclusion: str = "success") -> dict:
    return {"name": name, "run_attempt": attempt, "conclusion": conclusion, "status": "completed" if conclusion else "in_progress"}


def prereq_jobs(attempt: int = 1, *, drop: str | None = None, conclusion: dict[str, str] | None = None) -> list[dict]:
    out = []
    for n in sorted(re_.REQUIRED_PREREQ_JOBS):
        if n == drop:
            continue
        out.append(job(n, attempt, (conclusion or {}).get(n, "success")))
    return out


class StubAPI:
    """The Actions API, stubbed: runs by id; jobs?filter=all by id. `forbid_run=True` proves a role never reads the run.

    B4 secondary hardening (codex pass-4): this base stub deliberately OMITS the immutable-artifact surface
    (list_artifacts / read_artifact_member) to unit-test the STRUCTURAL record logic. It EXPLICITLY declares
    `SKIP_ARTIFACT_BINDING = True` so require_evidence.artifact_binding_problems knows this is a sanctioned
    unit opt-out (the byte-binding is exercised by ArtAPI + in CI by the real API); an API lacking the methods
    WITHOUT this flag fails closed. `ArtAPI` (which supplies the real methods) never consults the flag."""

    SKIP_ARTIFACT_BINDING = True

    def __init__(self, runs: dict[int, dict] | None = None, jobs: dict[int, list[dict]] | None = None, forbid_run: bool = False):
        self.runs, self.jobs, self.forbid_run = runs or {}, jobs or {}, forbid_run
        self.run_calls = 0

    def get_run(self, run_id: int) -> dict:
        self.run_calls += 1
        if self.forbid_run:
            raise AssertionError("get_run consulted -- this role must never read the whole run's conclusion")
        return self.runs.get(run_id, {"id": run_id, "status": "unknown"})

    def list_jobs(self, run_id: int) -> list[dict]:
        return list(self.jobs.get(run_id, []))


def env_run(run_id: int = 1001, attempt: int = 1, sha: str = SHA_S) -> dict[str, str]:
    return {"GITHUB_RUN_ID": str(run_id), "GITHUB_RUN_ATTEMPT": str(attempt), "GITHUB_SHA": sha, "GITHUB_REPOSITORY": "swetmr/pythscribe"}


def run_rec(run_id: int, sha: str = SHA_S, status: str = "completed", conclusion: str | None = "success", attempt: int = 1) -> dict:
    return {"id": run_id, "head_sha": sha, "status": status, "conclusion": conclusion, "run_attempt": attempt, "path": ".github/workflows/release.yml"}


# ============================================================================ E0: in-progress parent run (the primary control)


def test_e0_in_progress_parent_run_release_run_passes_promotion_refuses():
    m = make_manifest()
    api_rr = StubAPI(jobs={1001: prereq_jobs()}, forbid_run=True)  # the run is in_progress; get_run is FORBIDDEN
    assert re_.verify_manifest_binding(m, "release-run", env=env_run(), api=api_rr) == []
    api_pr = StubAPI(runs={1001: run_rec(1001, status="in_progress", conclusion=None)}, jobs={1001: prereq_jobs()})
    probs = re_.verify_manifest_binding(m, "promotion", env=env_run(), api=api_pr)
    assert any("!= completed" in x for x in probs), probs


def test_e0_mutant_release_run_that_checks_whole_run_conclusion_is_red(monkeypatch):
    """The mutant: same-run consumer consults the run's conclusion -> deadlocks/refuses on the in-progress fixture."""
    m = make_manifest()
    real = re_.verify_manifest_binding

    def mutant(manifest, role, *, env, api):
        p = real(manifest, role, env=env, api=api)
        return p + re_.run_complete_ok(api.get_run(int(manifest["build_run_id"])))  # the forbidden read

    monkeypatch.setattr(re_, "verify_manifest_binding", mutant)
    api = StubAPI(runs={1001: run_rec(1001, status="in_progress", conclusion=None)}, jobs={1001: prereq_jobs()})
    assert re_.verify_manifest_binding(m, "release-run", env=env_run(), api=api) != []  # RED on the fixture


def test_e0_mutant_promotion_accepting_in_progress_is_red(monkeypatch):
    m = make_manifest()
    api = StubAPI(runs={1001: run_rec(1001, status="in_progress", conclusion=None)}, jobs={1001: prereq_jobs()})
    monkeypatch.setattr(re_, "run_complete_ok", lambda run: [])  # the mutant: completion never required
    assert re_.verify_manifest_binding(m, "promotion", env=env_run(), api=api) == []  # accepts -> the rule is load-bearing
    monkeypatch.undo()
    assert re_.verify_manifest_binding(m, "promotion", env=env_run(), api=api) != []


# ============================================================================ E0: smuggled manifest / wrong-mode-from-manifest


def test_e0_run_a_manifest_in_run_b_is_refused_never_reclassified():
    m_a = make_manifest(run_id=1001)  # run A: completed + successful + self-consistent, SAME sha
    api = StubAPI(runs={1001: run_rec(1001)}, jobs={1001: prereq_jobs()})
    probs = re_.verify_manifest_binding(m_a, "release-run", env=env_run(run_id=2002), api=api)
    assert any("build_run_id 1001 != GITHUB_RUN_ID 2002" in x for x in probs), probs
    assert api.run_calls == 0  # identity failed first; the foreign run is never consulted
    # the discriminating twin: the SAME manifest under the promotion role literal is accepted -- so it is the ROLE
    # literal (never a manifest field) that decides; a mutant deriving the mode from build_run_id would accept here
    assert re_.verify_manifest_binding(m_a, "promotion", env=env_run(run_id=2002), api=api) == []


def test_e0_mutant_mode_from_manifest_accepts_the_smuggled_manifest(monkeypatch):
    m_a = make_manifest(run_id=1001)
    api = StubAPI(runs={1001: run_rec(1001)}, jobs={1001: prereq_jobs()})
    real = re_.verify_manifest_binding

    def mutant(manifest, role, *, env, api):  # "if the manifest names another run, treat as promotion"
        role = "promotion" if str(manifest["build_run_id"]) != env.get("GITHUB_RUN_ID") else role
        return real(manifest, role, env=env, api=api)

    monkeypatch.setattr(re_, "verify_manifest_binding", mutant)
    assert re_.verify_manifest_binding(m_a, "release-run", env=env_run(run_id=2002), api=api) == []  # the mutant ACCEPTS = RED


# ============================================================================ E0: fixed prerequisite set


@pytest.mark.parametrize("prereq_field", [None, []])
def test_e0_empty_or_absent_prerequisite_list_fixed_set_still_required(prereq_field):
    over = {} if prereq_field is None else {"prerequisite_jobs": prereq_field}
    m = make_manifest(**over)
    full = StubAPI(jobs={1001: prereq_jobs()})
    assert re_.verify_manifest_binding(m, "release-run", env=env_run(), api=full) == []
    missing = StubAPI(jobs={1001: prereq_jobs(drop="Build aarch64-apple-darwin")})
    probs = re_.verify_manifest_binding(m, "release-run", env=env_run(), api=missing)
    assert any("`Build aarch64-apple-darwin` has no execution" in x for x in probs), probs


def test_e0_mutant_requiring_only_the_manifests_list_accepts_the_empty_list(monkeypatch):
    m = make_manifest(prerequisite_jobs=[])
    missing = StubAPI(jobs={1001: prereq_jobs(drop="Build aarch64-apple-darwin")})
    monkeypatch.setattr(re_, "required_prereq_jobs", lambda manifest: frozenset(manifest.get("prerequisite_jobs", [])))
    assert re_.verify_manifest_binding(m, "release-run", env=env_run(), api=missing) == []  # accepts -> RED


def test_e0_failed_build_leg_refused():
    m = make_manifest()
    api = StubAPI(jobs={1001: prereq_jobs(conclusion={"Build x86_64-pc-windows-msvc": "failure"})})
    probs = re_.verify_manifest_binding(m, "release-run", env=env_run(), api=api)
    assert any("`Build x86_64-pc-windows-msvc`: latest execution (attempt 1) concluded 'failure'" in x for x in probs), probs


def test_e0_required_prereq_set_is_the_fixed_seven():
    assert re_.REQUIRED_PREREQ_JOBS == frozenset({"prepare", "manifest"} | {f"Build {t}" for t in TRIPLE_TO_TAG})


# ============================================================================ E0: re-run fixtures (rev-7 SF-B), mutation-verified


def _rerun_jobs(*, manifest_attempt: int = 1, build_fail_attempt2: bool = False) -> list[dict]:
    """Attempt 1: everything green; attempt 2: one acceptance leg retried (never a prerequisite) unless told otherwise."""
    jobs = prereq_jobs(1)
    jobs.append(job("node-free-acceptance x86_64-apple-darwin", 1, "failure"))
    jobs.append(job("node-free-acceptance x86_64-apple-darwin", 2, "success"))
    if manifest_attempt == 2:
        jobs.append(job("manifest", 2, "success"))
    if build_fail_attempt2:
        jobs.append(job("Build x86_64-unknown-linux-gnu", 2, "failure"))
    return jobs


def test_e0_rerun_i_attempt2_consumer_attempt1_manifest_passes():
    m = make_manifest(attempt=1)
    api = StubAPI(jobs={1001: _rerun_jobs()})
    assert re_.verify_manifest_binding(m, "release-run", env=env_run(attempt=2), api=api) == []


def test_e0_rerun_ii_different_run_id_refused():
    m = make_manifest(run_id=1001, attempt=1)
    api = StubAPI(jobs={1001: _rerun_jobs(), 3003: _rerun_jobs()})
    assert re_.verify_manifest_binding(m, "release-run", env=env_run(run_id=3003, attempt=2), api=api) != []


def test_e0_rerun_iii_manifest_job_reexecuted_at_attempt2_stale_manifest_refused():
    m = make_manifest(attempt=1)
    api = StubAPI(jobs={1001: _rerun_jobs(manifest_attempt=2)})
    probs = re_.verify_manifest_binding(m, "release-run", env=env_run(attempt=2), api=api)
    assert any("latest successful attempt 2" in x and "stale" in x for x in probs), probs


def test_e0_rerun_iv_build_leg_latest_execution_failure_refused_even_though_attempt1_succeeded():
    m = make_manifest(attempt=1)
    api = StubAPI(jobs={1001: _rerun_jobs(build_fail_attempt2=True)})
    probs = re_.verify_manifest_binding(m, "release-run", env=env_run(attempt=2), api=api)
    assert any("`Build x86_64-unknown-linux-gnu`: latest execution (attempt 2) concluded 'failure'" in x for x in probs), probs


def test_e0_rerun_v_manifest_attempt_ahead_of_consumer_refused():
    m = make_manifest(attempt=3)
    api = StubAPI(jobs={1001: _rerun_jobs()})
    probs = re_.verify_manifest_binding(m, "release-run", env=env_run(attempt=2), api=api)
    assert any("build_run_attempt 3 is not admissible at GITHUB_RUN_ATTEMPT 2" in x for x in probs), probs


def test_e0_rerun_i_and_iii_through_promotion():
    # (i) through promotion: run completed+success at latest attempt 2 (an acceptance leg retried); manifest job's
    # latest successful execution at attempt 1; the promoter holds the attempt-1 manifest -> ACCEPTED
    m = make_manifest(attempt=1)
    api = StubAPI(runs={1001: run_rec(1001, attempt=2)}, jobs={1001: _rerun_jobs()})
    assert re_.verify_manifest_binding(m, "promotion", env=env_run(), api=api) == []
    # (iii) through promotion: the manifest job re-executed at attempt 2 -> the attempt-1 manifest is stale -> REFUSED
    api3 = StubAPI(runs={1001: run_rec(1001, attempt=2)}, jobs={1001: _rerun_jobs(manifest_attempt=2)})
    probs = re_.verify_manifest_binding(m, "promotion", env=env_run(), api=api3)
    assert any("latest successful attempt 2" in x for x in probs), probs


def test_e0_mutant_strict_attempt_equality_false_refuses_rerun_i(monkeypatch):
    m = make_manifest(attempt=1)
    api = StubAPI(jobs={1001: _rerun_jobs()})
    monkeypatch.setattr(re_, "attempt_rule", lambda ma, ca: ma == ca)  # the rev-6 wording
    assert re_.verify_manifest_binding(m, "release-run", env=env_run(attempt=2), api=api) != []  # false-REFUSE = RED


def test_e0_mutant_earliest_execution_accepts_rerun_iv(monkeypatch):
    m = make_manifest(attempt=1)
    api = StubAPI(jobs={1001: _rerun_jobs(build_fail_attempt2=True)})

    def earliest(jobs, *, max_attempt=None):
        out: dict[str, dict] = {}
        for j in jobs:
            if max_attempt is not None and int(j["run_attempt"]) > max_attempt:
                continue
            if j["name"] not in out or int(j["run_attempt"]) < int(out[j["name"]]["run_attempt"]):
                out[j["name"]] = j
        return out

    monkeypatch.setattr(re_, "latest_executions", earliest)
    assert re_.verify_manifest_binding(m, "release-run", env=env_run(attempt=2), api=api) == []  # accepts -> RED


def test_e0_mutant_ignoring_the_manifest_job_binding_accepts_rerun_iii(monkeypatch):
    m = make_manifest(attempt=1)
    api = StubAPI(jobs={1001: _rerun_jobs(manifest_attempt=2)})
    monkeypatch.setattr(re_, "latest_successful_attempt", lambda jobs, name, max_attempt=None: None)
    assert re_.verify_manifest_binding(m, "release-run", env=env_run(attempt=2), api=api) == []  # accepts -> RED


def test_e0_mutant_promotion_bound_to_the_runs_latest_attempt_false_refuses_i(monkeypatch):
    m = make_manifest(attempt=1)
    api = StubAPI(runs={1001: run_rec(1001, attempt=2)}, jobs={1001: _rerun_jobs()})
    monkeypatch.setattr(re_, "latest_successful_attempt", lambda jobs, name, max_attempt=None: 2)  # "the run's attempt"
    assert re_.verify_manifest_binding(m, "promotion", env=env_run(), api=api) != []  # false-REFUSE = RED


# ============================================================================ E0: identity + tamper + no-role


def test_e0_tampered_manifest_wrong_sha_and_failed_run_refused():
    m = make_manifest()
    api = StubAPI(runs={1001: run_rec(1001)}, jobs={1001: prereq_jobs()})
    bad = dict(m, version="9.9.9")  # self-hash no longer covers the content
    assert any("self-hash mismatch" in x for x in re_.verify_manifest_binding(bad, "release-run", env=env_run(), api=api))
    assert any("source_sha" in x for x in re_.verify_manifest_binding(m, "release-run", env=env_run(sha=SHA_OTHER), api=api))
    api_other = StubAPI(runs={1001: run_rec(1001, sha=SHA_OTHER)}, jobs={1001: prereq_jobs()})
    assert any("head_sha" in x for x in re_.verify_manifest_binding(make_manifest(sha=SHA_OTHER), "promotion", env=env_run(sha=SHA_OTHER, run_id=1001), api=StubAPI(runs={1001: run_rec(1001, sha=SHA_S)}, jobs={1001: prereq_jobs()})))
    api_failed = StubAPI(runs={1001: run_rec(1001, conclusion="failure")}, jobs={1001: prereq_jobs()})
    assert any("conclusion 'failure'" in x for x in re_.verify_manifest_binding(m, "promotion", env=env_run(), api=api_failed))
    assert api_other.run_calls == 0


def test_e0_no_role_invocation_exits_2_and_names_role(tmp_path):
    (tmp_path / "m.json").write_text(json.dumps(make_manifest()), encoding="utf-8")
    r = subprocess.run([sys.executable, str(SCRIPTS / "require_evidence.py"), "--manifest", str(tmp_path / "m.json")], capture_output=True, text=True)
    assert r.returncode == 2 and "--role" in r.stderr, (r.returncode, r.stderr)
    r = subprocess.run([sys.executable, str(SCRIPTS / "require_evidence.py"), "--role", "auto", "--manifest", str(tmp_path / "m.json")], capture_output=True, text=True)
    assert r.returncode == 2 and "invalid choice" in r.stderr


def test_e0_mutant_default_role_makes_the_no_role_invocation_succeed():
    ap = re_.build_parser()
    with pytest.raises(SystemExit) as ei:
        ap.parse_args([])
    assert ei.value.code == 2
    role = next(a for a in ap._actions if a.dest == "role")
    role.required, role.default = False, "promotion"  # the mutant
    assert ap.parse_args([]).role == "promotion"  # exits 0 -> RED


def test_e0_release_run_cli_green_and_red_with_a_stubbed_api(tmp_path, monkeypatch):
    m = make_manifest()
    (tmp_path / "m.json").write_text(json.dumps(m), encoding="utf-8")
    monkeypatch.setattr(re_, "api_from_env", lambda env: StubAPI(jobs={1001: prereq_jobs()}, forbid_run=True))
    for k, v in env_run().items():
        monkeypatch.setenv(k, v)
    assert re_.main(["--role", "release-run", "--manifest", str(tmp_path / "m.json")]) == 0
    monkeypatch.setenv("GITHUB_RUN_ID", "2002")
    assert re_.main(["--role", "release-run", "--manifest", str(tmp_path / "m.json")]) == 1


# ============================================================================ A0: node detection (the filesystem scan)


def test_a0_planted_node_file_is_found_and_refused(tmp_path):
    root = tmp_path / "fs"
    (root / "usr" / "lib").mkdir(parents=True)
    (root / "usr" / "lib" / "libc.so").write_bytes(b"x")
    assert wa.find_node_files([root]) == [] and wa.a0_verdict([], path_hits={}) == []
    # injected-Node: a file named node anywhere -> found -> RED
    (root / "usr" / "local" / "bin").mkdir(parents=True)
    (root / "usr" / "local" / "bin" / "node").write_bytes(b"\x7fELF")
    found = wa.find_node_files([root])
    assert [f.name for f in found] == ["node"]
    assert any("node file present" in x for x in wa.a0_verdict(found))
    # Playwright-in-app: the driver's bundled `node` is a FILE named node -> the same walk finds it
    drv = root / "venv" / "lib" / "python3.12" / "site-packages" / "playwright" / "driver"
    drv.mkdir(parents=True)
    (drv / "node").write_bytes(b"\x7fELF")
    (drv / "package").mkdir()
    assert {f.parent.name for f in wa.find_node_files([root])} == {"bin", "driver"}
    # node.exe and nodejs* are names too; a directory named node is not a file
    (root / "nodejs22").mkdir()
    (root / "node.exe").write_bytes(b"MZ")
    (root / "nodejs22" / "nodejs.cmd").write_bytes(b"@echo")
    names = {f.name for f in wa.find_node_files([root])}
    assert names == {"node", "node.exe", "nodejs.cmd"}


def test_a0_path_hits_are_red(tmp_path):
    b = tmp_path / "bin"
    b.mkdir()
    exe = b / ("node.exe" if os.name == "nt" else "node")
    exe.write_bytes(b"x")
    exe.chmod(0o755)
    hits = wa.which_on_path(path_env=str(b))
    assert "node" in hits
    assert any("on PATH" in x for x in wa.a0_verdict([], path_hits=hits))
    assert wa.which_on_path(path_env=str(tmp_path / "empty")) == {}


def test_a0_native_leg_verdict_requires_allowed_root_and_a_denying_probe(tmp_path):
    ctl = tmp_path / "ctl"
    (ctl / "venv" / "driver").mkdir(parents=True)
    node = ctl / "venv" / "driver" / "node"
    node.write_bytes(b"x")
    ext = tmp_path / "externals" / "node20" / "bin"
    ext.mkdir(parents=True)
    (ext / "node").write_bytes(b"x")
    found = wa.find_node_files([tmp_path])
    denied = lambda p: (True, "EACCES")  # noqa: E731
    reachable = lambda p: (False, "open ok")  # noqa: E731
    assert wa.a0_verdict(found, [ctl, tmp_path / "externals"], denied) == []
    assert any("REACHABLE by `app`" in x for x in wa.a0_verdict(found, [ctl, tmp_path / "externals"], reachable))  # permission drift
    assert any("without an `app` probe" in x for x in wa.a0_verdict(found, [ctl, tmp_path / "externals"], None))  # fail-closed
    (tmp_path / "usr-local-bin").mkdir()
    (tmp_path / "usr-local-bin" / "node").write_bytes(b"x")  # scrub skipped
    assert any("OUTSIDE the allowed roots" in x for x in wa.a0_verdict(wa.find_node_files([tmp_path]), [ctl, tmp_path / "externals"], denied))
    # through the CLI, with a node-FREE PATH (on this dev box the real Node is on PATH: the first run below without
    # the scrubbed PATH is itself the PATH-hit control -- asserted, so the PATH check is proven live)
    cli = [sys.executable, str(SCRIPTS / "wheel_acceptance.py"), "a0", "--root", str(tmp_path), "--allow-under", str(ctl), str(tmp_path / "externals")]
    if wa.which_on_path():
        r = subprocess.run(cli + ["--probe", f"{sys.executable} -c pass"], capture_output=True, text=True)
        assert r.returncode == 1 and "is on PATH" in r.stdout
    scrubbed = {**os.environ, "PATH": "C:\\Windows\\System32" if os.name == "nt" else "/usr/bin:/bin"}
    # a BROKEN probe (non-zero, not a denial) counts as reachable: fail-closed, never a silent pass
    r = subprocess.run(cli + ["--probe", f"{sys.executable} -c import_no_such_probe_module"], capture_output=True, text=True, env=scrubbed)
    assert r.returncode == 1 and "REACHABLE" in r.stdout, r.stdout
    # a probe that answers 0 (denied) for every file, but usr-local-bin/node is still planted -> OUTSIDE -> RED
    r = subprocess.run(cli + ["--probe", f"{sys.executable} -c pass"], capture_output=True, text=True, env=scrubbed)
    assert r.returncode == 1 and "OUTSIDE" in r.stdout, r.stdout
    (tmp_path / "usr-local-bin" / "node").unlink()
    r = subprocess.run(cli + ["--probe", f"{sys.executable} -c pass"], capture_output=True, text=True, env=scrubbed)
    assert r.returncode == 0 and '"verdict": "pass"' in r.stdout, r.stdout  # the positive twin


# ============================================================================ A0b: candidate binding


def _wheel(path: Path, payload: bytes) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)
    return path


def test_a0b_wrong_wheel_sha_and_foreign_wheelhouse_dist_refused(tmp_path):
    t = "x86_64-unknown-linux-gnu"
    good = b"PK-good-wheel"
    m = make_manifest()
    m["targets"][t]["wheel_sha256"] = _sha(good)
    w = _wheel(tmp_path / "dist" / m["targets"][t]["wheel_filename"], good)
    assert wa.check_candidate(w, m, t) == []
    # wrong-wheel: a wheel whose bytes do not hash to the manifest -> refused BEFORE install
    bad = _wheel(tmp_path / "dist2" / m["targets"][t]["wheel_filename"], b"PK-other")
    probs = wa.check_candidate(bad, m, t)
    assert any("refusing to install" in x for x in probs), probs
    # wrong name / tag
    ren = _wheel(tmp_path / "dist3" / f"pythscribe-{VERSION}-py3-none-manylinux_2_28_aarch64.whl", good)
    assert any("!= manifest-named" in x for x in wa.check_candidate(ren, m, t))
    # wheelhouse: exactly one pythscribe dist and it IS W
    wh = tmp_path / "wheelhouse"
    wh.mkdir()
    shutil.copy(w, wh / w.name)
    (wh / "wasmtime-48.0.0-py3-none-any.whl").write_bytes(b"dep")
    assert wa.check_wheelhouse(wh, w) == []
    (wh / "pythscribe-0.2.4-py3-none-manylinux_2_28_x86_64.whl").write_bytes(b"old")  # the wrong-wheel control
    assert any("FOREIGN pythscribe distribution" in x and "0.2.4" in x for x in wa.check_wheelhouse(wh, w))
    (wh / "pythscribe-0.2.4-py3-none-manylinux_2_28_x86_64.whl").unlink()
    (wh / w.name).write_bytes(b"PK-other")
    assert any("differs from the candidate" in x for x in wa.check_wheelhouse(wh, w))
    r = subprocess.run([sys.executable, str(SCRIPTS / "wheel_acceptance.py"), "bind", "--wheel", str(bad), "--manifest", str(_write_json(tmp_path / "m.json", m)), "--target", t], capture_output=True, text=True)
    assert r.returncode == 1 and '"verdict": "fail"' in r.stdout


def _write_json(p: Path, obj) -> Path:
    p.write_text(json.dumps(obj), encoding="utf-8")
    return p


# ============================================================================ A1: installed identity (bytes == RECORD == manifest)


def _site(tmp_path: Path, binary: bytes) -> tuple[Path, Path, str]:
    binname = "pyths.exe" if os.name == "nt" else "pyths"
    site = tmp_path / "venv" / "lib" / "site-packages"
    (site / "pythscribe" / "_bin").mkdir(parents=True)
    (site / "pythscribe" / "__init__.py").write_text("", encoding="utf-8")
    (site / "pythscribe" / "_bin" / binname).write_bytes(binary)
    di = site / f"pythscribe-{VERSION}.dist-info"
    di.mkdir()
    digest = base64.urlsafe_b64encode(hashlib.sha256(binary).digest()).rstrip(b"=").decode()
    (di / "RECORD").write_text(f"pythscribe/__init__.py,sha256=abc,0\npythscribe/_bin/{binname},sha256={digest},{len(binary)}\n", encoding="utf-8")
    scripts = tmp_path / "venv" / "bin"
    scripts.mkdir()
    (scripts / "pyths").write_bytes(b"#!py")
    return site, scripts, binname


def test_a1_installed_bytes_equal_record_equal_manifest_and_stale_native_is_red(tmp_path):
    t = TRIPLES[0]
    binary = b"native-bytes-B_t"
    site, scripts, binname = _site(tmp_path, binary)
    m = make_manifest()
    m["targets"][t]["native_sha256"] = _sha(binary)
    common = dict(checkout=tmp_path / "checkout", pythscribe_file=site / "pythscribe" / "__init__.py", pyths_path=scripts / "pyths")
    assert wa.record_digest(site, f"pythscribe/_bin/{binname}") == _sha(binary)
    assert wa.check_installed(site, scripts, m, t, **common) == []
    # stale-native: swap the installed bytes, leave RECORD untouched
    (site / "pythscribe" / "_bin" / binname).write_bytes(b"swapped")
    probs = wa.check_installed(site, scripts, m, t, **common)
    assert any("installed BYTES" in x and "!= RECORD" in x for x in probs) and any("!= manifest native_sha256" in x for x in probs), probs
    # the RECORD-only mutant would stay green here: RECORD still equals the manifest (asserted = why bytes are hashed)
    assert wa.record_digest(site, f"pythscribe/_bin/{binname}") == m["targets"][t]["native_sha256"]
    (site / "pythscribe" / "_bin" / binname).write_bytes(binary)
    # checkout-shadowing: pythscribe imported from inside the checkout -> RED
    ck = tmp_path / "checkout" / "pythscribe"
    ck.mkdir(parents=True)
    (ck / "__init__.py").write_text("", encoding="utf-8")
    probs = wa.check_installed(site, scripts, m, t, checkout=tmp_path / "checkout", pythscribe_file=ck / "__init__.py", pyths_path=scripts / "pyths")
    assert any("checkout-shadowing" in x for x in probs) and any("not the venv site-packages" in x for x in probs), probs
    # pyths resolving outside the venv -> RED
    probs = wa.check_installed(site, scripts, m, t, checkout=None, pythscribe_file=site / "pythscribe" / "__init__.py", pyths_path=tmp_path / "usr" / "bin" / "pyths")
    assert any("`pyths` resolves to" in x for x in probs)


# ============================================================================ A6: R-BA assembly + the promotion consumer


def _leg_dir(legs: Path, t: str, m: dict, *, drop: tuple[str, ...] = (), fail: tuple[str, ...] = ()) -> Path:
    d = legs / t
    d.mkdir(parents=True, exist_ok=True)
    nat = m["targets"][t]["native_sha256"]
    def ok(step): return {"step": step, "problems": [] if step not in fail else [f"{step}: injected"], "verdict": "pass" if step not in fail else "fail"}
    files = {"a0.json": ok("A0"), "bind.json": dict(ok("A0b"), wheel_sha256=m["targets"][t]["wheel_sha256"]), "a4b.json": ok("A4b"), "a5.json": ok("A5"), "a5b.json": dict(ok("A5b"), sampler_hits=[]),
             "leg.json": {"steps": {s: ("fail" if s in fail else "pass") for s in ("A1", "A2", "A3", "A4")}, "problems": [], "installed_binary_sha256": nat, "record_sha256": nat,
                          "compiler_version": VERSION, "a3_bits": "000000000000084", "a4_counts": {"python": [0, 0], "server": [0, 1]}}}
    for fn, obj in files.items():
        if fn not in drop:
            _write_json(d / fn, obj)
    return d


def _records(tmp_path: Path, m: dict, *, rba: dict | None = None, skip: tuple[str, ...] = ()) -> Path:
    ev = tmp_path / "evidence"
    ev.mkdir(parents=True, exist_ok=True)
    base = {"schema": re_.RECORD_SCHEMA, "source_sha": SHA_S, "manifest_sha256": m["manifest_sha256"], "verdict": "pass",
            "producer": {"workflow": "release.yml", "job": "x", "run_id": 1001, "run_attempt": 1, "head_sha": SHA_S}}
    def rni_block(p: str) -> dict:  # the M6.2 per-package binding fields the consumer re-checks
        b = {"verdict": "pass", "published_tarball_sha256": _sha(p.encode())}
        if p in re_.NPM_PLATFORM_TO_TRIPLE:
            b["platform"] = {"triple": re_.NPM_PLATFORM_TO_TRIPLE[p], "binary_sha256": m["targets"][re_.NPM_PLATFORM_TO_TRIPLE[p]]["native_sha256"]}
        elif p in re_.NPM_PAYLOAD_PACKAGES:
            b["payload"] = {"files_sha256": re_.files_map_sha256(m[re_.NPM_PAYLOAD_PACKAGES[p]]["files"])}
        elif p in re_.NPM_PLUGIN_PACKAGES or p == re_.NPM_WRAPPER:
            # S9 (codex pass-3): the consumer requires content_bound True + both hashes equal for
            # plugin/wrapper packages (bound to the checkout of S by verify_npm_identity --checkout).
            h = _sha(f"content-{p}".encode())
            key = "wrapper" if p == re_.NPM_WRAPPER else "plugin"
            b[key] = {"content_bound": True, "registry_files_sha256": h, "checkout_files_sha256": h}
        return b

    # B4: each record names its FIXED assembler job (verify_records requires that job's own success).
    recs = {
        "R-BA": rba if rba is not None else rba_pass(tmp_path, m),
        "R-NI": dict(base, record="R-NI", producer=dict(base["producer"], job="npm-identity"), targets={p: rni_block(p) for p in re_.NPM_PACKAGES}),
        "R-TP": dict(base, record="R-TP", producer=dict(base["producer"], workflow="publish-pypi.yml", job="testpypi", run_id=5005), targets={},
                     uploaded_files=re_.distribution_set(m), registry_files=re_.distribution_set(m)),  # B3: registry-served proof
        "R-TV": dict(base, record="R-TV", producer=dict(base["producer"], workflow="publish-pypi.yml", job="testpypi-evidence", run_id=5005),
                     targets={t: {"steps": {s: "pass" for s in re_.RTV_REQUIRED_STEPS}, "installed_wheel_sha256": m["targets"][t]["wheel_sha256"], re_.RTV_ALL_FEATURES_FIELD: "pass"} for t in m["targets"]}),
    }
    for name, rec in recs.items():
        if name not in skip:
            _write_json(ev / f"{name}.json", rec)
    return ev


def rba_pass(tmp_path: Path, m: dict) -> dict:
    legs = tmp_path / "legs-ok"
    for t in TRIPLES:
        _leg_dir(legs, t, m)
    return rba.assemble(m, legs, env_run())


def test_a6_assemble_rba_pass_and_skipped_check_or_missing_leg_fail(tmp_path):
    m = make_manifest()
    rec = rba_pass(tmp_path, m)
    assert rec["verdict"] == "pass" and set(rec["targets"]) == set(TRIPLES) and all(b["node_free"] for b in rec["targets"].values())
    assert rec["prerequisite_jobs_verified"] == sorted(re_.REQUIRED_PREREQ_JOBS) and rec["producer"]["run_id"] == 1001
    assert re_.check_record("R-BA", rec, m, source_sha=SHA_S) == []
    # skipped-check: delete a0.json on one leg -> that leg's A0 is `fail`, node_free False, verdict fail
    legs = tmp_path / "legs-skip"
    for t in TRIPLES:
        _leg_dir(legs, t, m, drop=("a0.json",) if t == TRIPLES[2] else ())
    rec = rba.assemble(m, legs, env_run())
    assert rec["verdict"] == "fail" and rec["targets"][TRIPLES[2]]["node_free"] is False and rec["targets"][TRIPLES[2]]["steps"]["A0"] == "fail"
    assert any("R-BA[" in x and "step `A0`" in x for x in re_.check_record("R-BA", rec, m, source_sha=SHA_S))
    # a record that simply LACKS the A0 field (a producer that never ran it) is refused by the consumer
    no_a0 = copy.deepcopy(rba_pass(tmp_path, m))
    for b in no_a0["targets"].values():
        del b["steps"]["A0"]
    assert any("step `A0` absent" in x for x in re_.check_record("R-BA", no_a0, m, source_sha=SHA_S))
    # 4/5 legs -> fail + the consumer refuses a 4-target record
    legs4 = tmp_path / "legs-4"
    for t in TRIPLES[:4]:
        _leg_dir(legs4, t, m)
    rec4 = rba.assemble(m, legs4, env_run())
    assert rec4["verdict"] == "fail" and "leg directory" in rec4["targets"][TRIPLES[4]]["problems"][0]
    del rec4["targets"][TRIPLES[4]]
    rec4["verdict"] = "pass"  # a producer that lies about the verdict still fails the target-set check
    assert any("a skipped leg" in x for x in re_.check_record("R-BA", rec4, m, source_sha=SHA_S))
    # a failed A5 on one leg -> fail; node_free unaffected by A5 itself (A0/A4b/A5b decide it)
    legs5 = tmp_path / "legs-a5"
    for t in TRIPLES:
        _leg_dir(legs5, t, m, fail=("A5",) if t == TRIPLES[0] else ())
    rec5 = rba.assemble(m, legs5, env_run())
    assert rec5["verdict"] == "fail" and rec5["targets"][TRIPLES[0]]["node_free"] is True and rec5["targets"][TRIPLES[0]]["steps"]["A5"] == "fail"
    # installed sha drift on one leg -> A1 fail
    legs6 = tmp_path / "legs-sha"
    for t in TRIPLES:
        _leg_dir(legs6, t, m)
    leg = json.loads((legs6 / TRIPLES[1] / "leg.json").read_text())
    leg["installed_binary_sha256"] = _sha(b"swapped")
    _write_json(legs6 / TRIPLES[1] / "leg.json", leg)
    rec6 = rba.assemble(m, legs6, env_run())
    assert rec6["targets"][TRIPLES[1]]["steps"]["A1"] == "fail" and rec6["verdict"] == "fail"
    r = subprocess.run([sys.executable, str(SCRIPTS / "assemble_rba.py"), "--manifest", str(_write_json(tmp_path / "m.json", m)), "--legs", str(legs6), "--out", str(tmp_path / "ev" / "R-BA.json")],
                       capture_output=True, text=True, env={**os.environ, **env_run()})
    assert r.returncode == 1 and "R-BA: fail" in r.stdout


# B4: the producer run's jobs must carry each record's FIXED assembler job (conclusion success).
def _build_jobs(attempt: int = 1) -> list[dict]:
    return prereq_jobs(attempt) + [job("node-free-evidence", attempt), job("npm-identity", attempt)]


def _promo_jobs(attempt: int = 1) -> list[dict]:
    return [job("testpypi", attempt), job("testpypi-validate", attempt), job("testpypi-evidence", attempt)]


def test_e3_promotion_records_green_and_each_control_refuses(tmp_path):
    m = make_manifest()
    api = StubAPI(runs={1001: run_rec(1001), 5005: dict(run_rec(5005), path=".github/workflows/publish-pypi.yml")}, jobs={1001: _build_jobs(), 5005: _promo_jobs()})
    ev = _records(tmp_path, m)
    assert re_.verify_records(m, ["R-BA", "R-NI"], ev, env=env_run(), api=api) == []
    assert re_.verify_records(m, ["R-BA", "R-NI", "R-TP", "R-TV"], ev, env=env_run(), api=api) == []
    # the codex scenario: R-BA + R-NI pass, R-TV absent, pypi dispatched -> refuse
    ev2 = _records(tmp_path / "s2", m, skip=("R-TV",))
    assert any("R-TV: record file" in x and "missing" in x for x in re_.verify_records(m, ["R-BA", "R-NI", "R-TP", "R-TV"], ev2, env=env_run(), api=api))
    assert re_.verify_records(m, ["R-BA", "R-NI"], ev2, env=env_run(), api=api) == []  # testpypi does not need it
    # verdict fail
    bad = rba_pass(tmp_path, m)
    bad["verdict"] = "fail"
    ev3 = _records(tmp_path / "s3", m, rba=bad)
    assert any("R-BA: verdict 'fail'" in x for x in re_.verify_records(m, ["R-BA"], ev3, env=env_run(), api=api))
    # wrong-run: record bound to another manifest hash / another source sha / a producer at another head_sha
    m7 = make_manifest(run_id=7007)
    legs7 = tmp_path / "legs-7007"
    for t in TRIPLES:
        _leg_dir(legs7, t, m7)
    other = rba.assemble(m7, legs7, env_run(run_id=7007))  # a record from ANOTHER run's manifest + producer
    ev4 = _records(tmp_path / "s4", m, rba=other)
    p4 = re_.verify_records(m, ["R-BA"], ev4, env=env_run(), api=api)
    assert any("bound to manifest" in x for x in p4) and any("producer run 7007 != the manifest's build run" in x for x in p4), p4
    rs = rba_pass(tmp_path, m)
    rs["source_sha"] = SHA_OTHER
    rs["producer"]["head_sha"] = SHA_OTHER
    p5 = re_.verify_records(m, ["R-BA"], _records(tmp_path / "s5", m, rba=rs), env=env_run(), api=api)
    assert any("source_sha" in x for x in p5) and any("producer head_sha" in x for x in p5), p5
    # producer run not green via the API
    api_red = StubAPI(runs={1001: run_rec(1001, conclusion="failure"), 5005: run_rec(5005)}, jobs={1001: _build_jobs(), 5005: _promo_jobs()})
    assert any("R-BA: producer run 1001 is not a completed+successful run" in x for x in re_.verify_records(m, ["R-BA"], ev, env=env_run(), api=api_red))
    # R-TP with one uploaded hash mutated -> refuse; R-TV with 4 targets -> refuse
    tp = json.loads((ev / "R-TP.json").read_text())
    tp["uploaded_files"][m["sdist"]["filename"]] = _sha(b"substituted sdist")
    (tmp_path / "s6").mkdir()
    _write_json(tmp_path / "s6" / "R-TP.json", tp)
    assert any("R-TP: uploaded_files != the manifest" in x for x in re_.verify_records(m, ["R-TP"], tmp_path / "s6", env=env_run(), api=api))
    tv = json.loads((ev / "R-TV.json").read_text())
    del tv["targets"][TRIPLES[0]]
    (tmp_path / "s7").mkdir()
    _write_json(tmp_path / "s7" / "R-TV.json", tv)
    assert any("R-TV: targets" in x and "skipped leg" in x for x in re_.verify_records(m, ["R-TV"], tmp_path / "s7", env=env_run(), api=api))


def test_e3_promotion_cli_green_then_red_on_a_missing_record(tmp_path, monkeypatch):
    m = make_manifest()
    ev = _records(tmp_path, m)
    _write_json(tmp_path / "m.json", m)
    monkeypatch.setattr(re_, "api_from_env", lambda env: StubAPI(runs={1001: run_rec(1001), 5005: dict(run_rec(5005), path=".github/workflows/publish-pypi.yml")}, jobs={1001: _build_jobs(), 5005: _promo_jobs()}))
    for k, v in env_run().items():
        monkeypatch.setenv(k, v)
    args = ["--role", "promotion", "--manifest", str(tmp_path / "m.json"), "--evidence-dir", str(ev), "--tag", f"v{VERSION}"]
    assert re_.main(args + ["--need", "R-BA", "R-NI", "R-TP", "R-TV"]) == 0
    (ev / "R-NI.json").unlink()
    assert re_.main(args + ["--need", "R-BA", "R-NI"]) == 1
    assert re_.main(["--role", "promotion", "--manifest", str(tmp_path / "m.json"), "--tag", "v9.9.9"]) == 1  # tag binding
    assert re_.main(["--role", "release-run", "--manifest", str(tmp_path / "m.json"), "--need", "R-BA"]) == 1  # --need is promotion-only


# ============================================================================ WF: workflow-lint (GREEN on the real files; RED per mutation)


def _wf():
    rel = yaml.safe_load((REPO / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8"))
    pub = yaml.safe_load((REPO / ".github" / "workflows" / "publish-pypi.yml").read_text(encoding="utf-8"))
    return rel, pub


def test_wf_lint_green_on_the_real_workflows():
    rel, pub = _wf()
    assert wl.lint(rel, pub) == []
    r = subprocess.run([sys.executable, str(SCRIPTS / "lint_release_workflows.py")], capture_output=True, text=True)
    assert r.returncode == 0 and "GREEN" in r.stdout, (r.stdout, r.stderr)
    jobs = rel["jobs"]
    assert jobs["node-free-acceptance"]["needs"] == "manifest"
    assert "node-free-acceptance" not in wl._transitive_needs(jobs, "manifest")
    assert {r["target"] for r in jobs["node-free-acceptance"]["strategy"]["matrix"]["include"]} == set(TRIPLE_TO_TAG)
    # B2: npm-publish follows node-free acceptance evidence (R-BA), and B1: only on a tag ref
    assert set(jobs["npm-publish"]["needs"]) == {"manifest", "node-free-evidence"} and jobs["build"]["needs"] == "prepare"
    assert "refs/tags/" in str(jobs["npm-publish"].get("if", ""))
    assert "node-free-acceptance" not in wl._transitive_needs(jobs, "manifest")  # still no cycle
    # promotion steps are the first gate of BOTH publish jobs and carry no `if:`
    for key in ("testpypi", "pypi"):
        steps = pub["jobs"][key]["steps"]
        idx = next(i for i, s in enumerate(steps) if "require_evidence.py" in str(s.get("run", "")))
        assert "--role promotion" in steps[idx]["run"] and "if" not in steps[idx]
        assert all("python -m build" not in str(s.get("run", "")) for s in steps[:idx])
    assert "--need R-BA R-NI R-TP R-TV" in pub["jobs"]["pypi"]["steps"][next(i for i, s in enumerate(pub["jobs"]["pypi"]["steps"]) if "require_evidence.py" in str(s.get("run", "")))]["run"]


def test_wf_lint_red_on_an_injected_cycle():
    rel, pub = _wf()
    mut = copy.deepcopy(rel)
    mut["jobs"]["manifest"]["needs"] = ["prepare", "wheel-set", "node-free-acceptance"]
    probs = wl.lint(mut, pub)
    assert any("deadlock cycle" in x for x in probs) and any("cycle" in x for x in probs), probs
    mut2 = copy.deepcopy(rel)
    mut2["jobs"]["node-free-acceptance"]["needs"] = "wheel-set"  # A0b would run without a manifest
    assert any("must include `manifest`" in x for x in wl.lint(mut2, pub))


def test_wf_lint_red_on_a_uses_step_inside_the_native_window_and_on_a_dropped_restore():
    rel, pub = _wf()
    mut = copy.deepcopy(rel)
    steps = mut["jobs"]["node-free-acceptance"]["steps"]
    lo = next(i for i, s in enumerate(steps) if str(s.get("id", "")).startswith("scrub"))
    steps.insert(lo + 2, {"name": "bad", "uses": "actions/upload-artifact@v4", "if": "runner.os == 'macOS'"})
    assert any("INSIDE the scrubbed window" in x for x in wl.lint(mut, pub))
    mut2 = copy.deepcopy(rel)
    mut2["jobs"]["node-free-acceptance"]["steps"] = [s for s in mut2["jobs"]["node-free-acceptance"]["steps"] if not str(s.get("id", "")).startswith("restore")]
    assert any("restore" in x for x in wl.lint(mut2, pub))
    mut3 = copy.deepcopy(rel)
    st = mut3["jobs"]["node-free-acceptance"]["steps"]
    hi = max(i for i, s in enumerate(st) if str(s.get("id", "")).startswith("restore"))
    mut3["jobs"]["node-free-acceptance"]["steps"] = st[: hi + 1]  # no upload after restore: the loud fail-safe is gone
    assert any("no `uses:` step follows the restore" in x for x in wl.lint(mut3, pub))


def test_wf_lint_red_on_a_missing_role_literal_and_a_role_input():
    rel, pub = _wf()
    mut = copy.deepcopy(pub)
    for s in mut["jobs"]["pypi"]["steps"]:
        if "require_evidence.py" in str(s.get("run", "")):
            s["run"] = s["run"].replace("--role promotion", "")
    assert any("without the literal `--role promotion`" in x for x in wl.lint(rel, mut))
    mut2 = copy.deepcopy(pub)
    on = mut2.get("on") or mut2.get(True)
    on["workflow_dispatch"]["inputs"]["role"] = {"type": "string", "default": "promotion"}
    assert any("exposes `role`" in x for x in wl.lint(rel, mut2))
    mut3 = copy.deepcopy(pub)
    for s in mut3["jobs"]["testpypi"]["steps"]:
        if "require_evidence.py" in str(s.get("run", "")):
            s["if"] = "github.event.inputs.skip_evidence != 'true'"
    assert any("carries an `if:`" in x for x in wl.lint(rel, mut3))
    mut4 = copy.deepcopy(rel)
    for s in mut4["jobs"]["node-free-acceptance"]["steps"]:
        if "require_evidence.py" in str(s.get("run", "")):
            s["run"] = s["run"].replace("--role release-run", "--role promotion")
    assert any("without the literal `--role release-run`" in x for x in wl.lint(mut4, pub))
    mut5 = copy.deepcopy(rel)
    mut5["jobs"]["build"]["name"] = "Compile ${{ matrix.target }}"  # a rename leaves REQUIRED_PREREQ_JOBS stale -> RED
    assert any("G2" in x for x in wl.lint(mut5, pub))


# ============================================================================ optional: the real wheel through the app-side runner (dev binary present)


def test_run_a2_a4_through_the_shipped_path_when_a_wheel_is_buildable(tmp_path):
    """Runs scripts/wheel_acceptance.py `run` against a wheel built from THIS checkout (only when the dev compiler
    is at target/release and wasmtime is importable); otherwise skipped -- the CI legs run it for real."""
    dev = REPO / "target" / "release" / ("pyths.exe" if os.name == "nt" else "pyths")
    if not dev.is_file() or os.environ.get("PYTHSCRIBE_SKIP_PACKAGING") == "1":
        pytest.skip("no dev compiler at target/release (the CI acceptance legs run the shipped path)")
    pytest.importorskip("wasmtime")
    from pythscribe.build._native import host_wheel_tag
    tag = host_wheel_tag()
    if tag is None:
        pytest.skip("not a release wheel host")
    staged = REPO / "pythscribe" / "_bin" / dev.name
    staged.parent.mkdir(exist_ok=True)
    if not staged.is_file() or staged.read_bytes() != dev.read_bytes():
        shutil.copy2(dev, staged)
    out = tmp_path / "dist"
    env = {k: v for k, v in os.environ.items() if k != "PYTHONPATH"}
    env["PYTHSCRIBE_WHEEL_PLATFORM"] = tag
    r = subprocess.run([sys.executable, "-m", "build", "--wheel", "--outdir", str(out), str(REPO)], capture_output=True, text=True, timeout=900, cwd=str(REPO), env=env)
    assert r.returncode == 0, r.stderr[-2000:]
    [whl] = out.glob("*.whl")
    triple = next(t for t, tg in TRIPLE_TO_TAG.items() if tg == tag)
    m = make_manifest()
    m["targets"][triple].update(wheel_filename=whl.name, wheel_sha256=_sha(whl.read_bytes()), native_sha256=_sha(dev.read_bytes()))
    m["manifest_sha256"] = manifest_self_hash(m)
    _write_json(tmp_path / "m.json", m)
    assert wa.check_candidate(whl, m, triple) == []
    venv = tmp_path / "venv"
    subprocess.run([sys.executable, "-m", "venv", str(venv)], check=True)
    py = venv / ("Scripts" if os.name == "nt" else "bin") / ("python.exe" if os.name == "nt" else "python")
    r = subprocess.run([str(py), "-m", "pip", "install", "--quiet", f"{whl}[server]"], capture_output=True, text=True, timeout=900)
    assert r.returncode == 0, r.stderr[-2000:]
    cwd = tmp_path / "cwd"
    cwd.mkdir()
    shutil.copy(REPO / "examples" / "hello.py", cwd / "hello.py")
    shutil.copy(SCRIPTS / "wheel_acceptance.py", cwd / "wheel_acceptance.py")
    penv = {k: v for k, v in os.environ.items() if k not in ("PYTHONPATH", "PYTHSCRIBE_PYTHS", "PYTHSCRIBE_NO_JIT")}
    penv["PATH"] = os.pathsep.join([str(py.parent), "C:\\Windows\\System32" if os.name == "nt" else "/usr/bin:/bin"])
    r = subprocess.run([str(py), str(cwd / "wheel_acceptance.py"), "run", "--manifest", str(tmp_path / "m.json"), "--target", triple, "--checkout", str(REPO),
                        "--hello", str(cwd / "hello.py"), "--out", str(tmp_path / "leg.json")], capture_output=True, text=True, timeout=900, cwd=str(cwd), env=penv)
    leg = json.loads((tmp_path / "leg.json").read_text())
    assert r.returncode == 0 and leg["steps"] == {"A1": "pass", "A2": "pass", "A3": "pass", "A4": "pass"}, (r.stdout[-1500:], r.stderr[-1500:], leg.get("problems"))
    assert leg["installed_binary_sha256"] == leg["record_sha256"] == m["targets"][triple]["native_sha256"]
