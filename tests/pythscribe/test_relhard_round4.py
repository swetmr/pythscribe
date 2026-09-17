"""Round-4 release-hardening controls (codex PASS-4: actions:read, B4 partial-rerun + fail-closed,
B6 exact matrix binding, S9+S11 exact-command-pin root fix).

The final push to ZERO / TAG-SAFE. Every fix ships (the anti-vacuity paired-control convention) its PAIRED NEGATIVE CONTROL -- a
witness that provably goes RED when the property is violated -- AND a HAPPY-PATH control proving the
honest release path still works (the recurring pass-4 failure mode was fixes that broke the happy path).
Offline / injected fakes only; the live registry / Actions API / gh portions run in CI.
"""
from __future__ import annotations

import copy
import json
import subprocess
import sys
from pathlib import Path

import yaml

from conftest import REPO

SCRIPTS = REPO / "scripts"
sys.path.insert(0, str(SCRIPTS))
sys.path.insert(0, str(REPO / "tests" / "pythscribe"))
import lint_release_workflows as wl  # noqa: E402
import require_ci_success as rcs  # noqa: E402
import require_evidence as re_  # noqa: E402
from test_relhard_round2 import ArtAPI  # noqa: E402
from test_wheel_m4_acceptance import SHA_S, StubAPI, run_rec  # noqa: E402
from test_wheel_m6_evidence import (  # noqa: E402
    RUN_ID,
    _all_records,
    _build_run_jobs,
    _promo_run_jobs,
    assemble as assemble_fixture,
    promotion_env,
)

RELEASE = REPO / ".github" / "workflows" / "release.yml"
PUBLISH = REPO / ".github" / "workflows" / "publish-pypi.yml"
CI = REPO / ".github" / "workflows" / "ci.yml"


def _wf():
    return yaml.safe_load(RELEASE.read_text(encoding="utf-8")), yaml.safe_load(PUBLISH.read_text(encoding="utf-8"))


# ============================================================================ Fix 1: actions:read for evidence/CI jobs


def test_perm_primitive_effective_permissions_and_actions_read():
    """The primitive: effective permissions = the job's own block if declared, else workflow-level; and
    only a dict with actions read/write, or the `*-all` string shorthands, grant actions."""
    wf = {"permissions": {"contents": "write", "actions": "read"}, "jobs": {}}
    assert wl._effective_permissions({}, wf) == {"contents": "write", "actions": "read"}  # inherits workflow-level
    assert wl._effective_permissions({"permissions": {"contents": "read"}}, wf) == {"contents": "read"}  # own block wins
    assert wl._grants_actions_read({"actions": "read"}) and wl._grants_actions_read({"actions": "write"})
    assert not wl._grants_actions_read({"contents": "write"}) and not wl._grants_actions_read(None)
    assert wl._grants_actions_read("read-all") and wl._grants_actions_read("write-all") and not wl._grants_actions_read("read")


def test_perm_real_workflows_grant_actions_read_to_every_evidence_job():
    """HAPPY PATH: the shipped workflows grant effective actions:read to every require_evidence.py /
    require_ci_success.py job (no false-RED); the full lint is GREEN."""
    rel, pub = _wf()
    assert wl.actions_read_problems(rel, "release.yml") == []
    assert wl.actions_read_problems(pub, "publish-pypi.yml") == []
    assert wl.lint(rel, pub) == []


def test_perm_control_dropping_actions_read_from_a_job_block_is_red():
    """NEGATIVE CONTROL (job-level): remove actions:read from npm-identity's OWN permissions block -> RED
    (a job block fully replaces the workflow default, so the inherited grant does not save it)."""
    rel, pub = _wf()
    mut = copy.deepcopy(rel)
    mut["jobs"]["npm-identity"]["permissions"].pop("actions", None)
    probs = wl.lint(mut, pub)
    assert any("PERM" in x and "npm-identity" in x and "actions: read" in x for x in probs), probs


def test_perm_control_dropping_workflow_level_actions_read_reds_the_inheriting_jobs():
    """NEGATIVE CONTROL (workflow-level inheritance): remove actions:read from release.yml's workflow
    permissions -> the jobs WITHOUT their own block (manifest, node-free-acceptance, node-free-evidence)
    lose it -> RED. present -> GREEN."""
    rel, pub = _wf()
    mut = copy.deepcopy(rel)
    mut["permissions"].pop("actions", None)
    probs = wl.actions_read_problems(mut, "release.yml")
    assert any("`manifest`" in x for x in probs), probs
    assert any("node-free-evidence" in x for x in probs), probs


def test_perm_control_dropping_actions_read_from_a_publish_job_is_red():
    """NEGATIVE CONTROL: publish-pypi.yml testpypi-validate loses actions:read -> RED (it queries the API)."""
    rel, pub = _wf()
    mut = copy.deepcopy(pub)
    mut["jobs"]["testpypi-validate"]["permissions"].pop("actions", None)
    assert any("PERM" in x and "testpypi-validate" in x for x in wl.lint(rel, mut))


# ============================================================================ Fix 2: B4 partial-rerun consumer + fail-closed verifier


class NoArtifactAPI(StubAPI):
    """A production-SHAPED API is required to expose the immutable-artifact surface. This stub lacks it AND
    declares it is NOT a sanctioned unit opt-out (SKIP_ARTIFACT_BINDING=False), so the verifier must fail
    closed on it -- the codex pass-4 B4 secondary gap (an injected method-less API silently skipped binding)."""

    SKIP_ARTIFACT_BINDING = False


def _art_api(artifacts):
    return ArtAPI(runs={RUN_ID: run_rec(RUN_ID), 5005: dict(run_rec(5005), path=".github/workflows/publish-pypi.yml")},
                  jobs={RUN_ID: _build_run_jobs(), 5005: _promo_run_jobs()}, artifacts=artifacts)


def test_b4_verifier_fails_closed_when_artifact_methods_are_absent(tmp_path):
    """NEGATIVE CONTROL (fail closed): a verifier API lacking list_artifacts/read_artifact_member and NOT
    declaring the unit opt-out must be REFUSED -- it can no longer skip the immutable-artifact binding."""
    m, dist, _ = assemble_fixture(tmp_path)
    ev = _all_records(tmp_path / "ok", m, dist)
    api = NoArtifactAPI(runs={RUN_ID: run_rec(RUN_ID)}, jobs={RUN_ID: _build_run_jobs()})
    p = re_.verify_records(m, ["R-BA"], ev, env=promotion_env(), api=api)
    assert any("lacks the immutable-artifact methods" in x and "fail closed" in x for x in p), p


def test_b4_happy_path_stub_optout_and_real_artifact_api_are_accepted(tmp_path):
    """HAPPY PATH (no false-RED): a base StubAPI that EXPLICITLY opts out (SKIP_ARTIFACT_BINDING=True) is
    accepted for structural-logic unit tests, and a real artifact-capable API whose bytes match is accepted."""
    m, dist, _ = assemble_fixture(tmp_path)
    ev = _all_records(tmp_path / "ok", m, dist)
    stub = StubAPI(runs={RUN_ID: run_rec(RUN_ID)}, jobs={RUN_ID: _build_run_jobs()})
    assert stub.SKIP_ARTIFACT_BINDING is True
    assert re_.verify_records(m, ["R-BA"], ev, env=promotion_env(), api=stub) == []  # sanctioned opt-out
    good = (ev / "R-BA.json").read_bytes()
    api = _art_api({RUN_ID: {"evidence-R-BA-1": good}, 5005: {}})
    assert re_.verify_records(m, ["R-BA"], ev, env=promotion_env(), api=api) == []  # bytes match -> GREEN


def test_b4_partial_rerun_consumes_the_producer_attempt_1_artifact(tmp_path):
    """HAPPY-PATH RERUN: on a partial rerun of only the consumer, the producer was NOT rerun, so its R-BA
    stays at producer attempt 1 (`evidence-R-BA-1`). verify_records keys the artifact binding on the RECORD's
    own producer.run_attempt (1), so it ACCEPTS attempt-1's artifact even when the consumer is on attempt 2;
    a clobbered on-disk copy still goes RED."""
    m, dist, _ = assemble_fixture(tmp_path)
    ev = _all_records(tmp_path / "ok", m, dist)
    good = (ev / "R-BA.json").read_bytes()
    # only the attempt-1 producer artifact exists (producer not rerun); consumer is on attempt 2
    consumer_attempt2 = dict(promotion_env(), GITHUB_RUN_ATTEMPT="2")
    api = _art_api({RUN_ID: {"evidence-R-BA-1": good}, 5005: {}})
    assert re_.verify_records(m, ["R-BA"], ev, env=consumer_attempt2, api=api) == []  # rerun ACCEPTED
    # clobber the on-disk (Release-asset) copy -> byte mismatch vs the immutable attempt-1 artifact -> RED
    (ev / "R-BA.json").write_text((ev / "R-BA.json").read_text() + "\n", encoding="utf-8")
    p = re_.verify_records(m, ["R-BA"], ev, env=consumer_attempt2, api=api)
    assert any("differ from the immutable Actions artifact 'evidence-R-BA-1'" in x for x in p), p


def test_b4_npm_publish_does_not_consume_r_ba_v025_advisory():
    """v0.2.5: node-free acceptance is ADVISORY (harness broken by runner drift), so npm-publish no longer
    consumes/validates R-BA at all -- it must NOT download an evidence-R-BA artifact. (The B4 partial-rerun
    R-BA consume logic is re-added in v0.2.6 when acceptance is fixed; the require_evidence unit tests still
    cover the mechanism.)"""
    rel = yaml.safe_load(RELEASE.read_text(encoding="utf-8"))
    steps = rel["jobs"]["npm-publish"]["steps"]
    consume = [s for s in steps if "evidence-R-BA" in str(s.get("run", "")) or (s.get("with") or {}).get("name", "").startswith("evidence-R-BA")]
    assert consume == [], consume  # no R-BA consume/validate step in npm-publish this release


# ============================================================================ Fix 3: B6 exact matrix-family binding + enforced in verify()


def _push_run(sha, **over):
    return {"id": 42, "head_sha": sha, "status": "completed", "conclusion": "success",
            "event": "push", "head_branch": "main", "run_attempt": 1, **over}


def _all_jobs_green(_run_id):
    return [{"name": r, "run_attempt": 1, "conclusion": "success"} for r in rcs.REQUIRED_CI_JOBS]


def _ci_variant(tmp_path, mutate) -> Path:
    ci = yaml.safe_load(CI.read_text(encoding="utf-8"))
    mutate(ci)
    p = tmp_path / "ci_variant.yml"
    p.write_text(yaml.safe_dump(ci), encoding="utf-8")
    return p


def test_b6_binding_and_verify_are_green_on_the_real_ci():
    """HAPPY PATH: the shipped ci.yml binds EXACTLY to REQUIRED_CI_JOBS, and verify() (which now runs the
    binding internally) is GREEN on a well-formed all-green push run."""
    sha = "a" * 40
    assert rcs.binding_problems() == []
    assert rcs.verify(sha, fetch_runs=lambda wf, s: [_push_run(sha)], fetch_jobs=_all_jobs_green) == []


def test_b6_control_added_matrix_leg_reds_binding_and_verify(tmp_path):
    """NEGATIVE CONTROL (the pass-4 reproduction): adding `freebsd-latest` to the `test` matrix makes
    `Test (freebsd-latest, 1.94.0)` a real leg NOT in REQUIRED_CI_JOBS while its siblings ARE -> the exact
    matrix-family binding is RED, and because verify() runs the binding, verify() is RED too (a job list
    that omits the new leg no longer slips through)."""
    def add_leg(ci):
        ci["jobs"]["test"]["strategy"]["matrix"]["os"].append("freebsd-latest")
    p = _ci_variant(tmp_path, add_leg)
    bp = rcs.binding_problems(p)
    assert any("freebsd-latest" in x and "REQUIRED_CI_JOBS" in x for x in bp), bp
    sha = "a" * 40
    vp = rcs.verify(sha, fetch_runs=lambda wf, s: [_push_run(sha)], fetch_jobs=_all_jobs_green, ci_path=p)
    assert any("freebsd-latest" in x for x in vp), vp


def test_b6_control_removed_required_job_reds_binding(tmp_path):
    """NEGATIVE CONTROL: dropping a required job (`lint`) from ci.yml makes `Lint` no longer a real display
    name -> the subset half of the binding is RED (the required set cannot silently shrink)."""
    def drop_lint(ci):
        del ci["jobs"]["lint"]
    p = _ci_variant(tmp_path, drop_lint)
    assert any("'Lint'" in x for x in rcs.binding_problems(p)), rcs.binding_problems(p)
    sha = "a" * 40
    assert rcs.verify(sha, fetch_runs=lambda wf, s: [_push_run(sha)], fetch_jobs=_all_jobs_green, ci_path=p) != []


# ============================================================================ Fix 4+5: S9 + S11 exact-command-pin root fix


def test_s9s11_pin_primitive_normalizes_only_the_interpreter():
    """The primitive: _normalize_gate_run touches ONLY the leading python/python3 token; the exact pin is
    matched byte-for-byte otherwise (this is why any shell composition cannot equal it)."""
    pin = wl._pinned_promotion_command("testpypi")
    honest = 'python3 scripts/require_evidence.py --role promotion --manifest release_manifest.json --need R-NI --evidence-dir evidence --tag "${GITHUB_REF_NAME}"'
    assert wl._normalize_gate_run(honest) == pin
    assert wl._exact_pin_problems(honest, pin, "S11", "testpypi") == []  # no false-RED on the honest command


def test_s11_control_if_then_fi_wrapper_is_refused():
    """NEGATIVE CONTROL (codex pass-4): wrapping a promotion gate in `if …; then :; fi` masks a failed
    evidence check while staying green-shaped -> the exact pin refuses it by construction."""
    rel, pub = _wf()
    assert wl.lint(rel, pub) == []
    for jobkey in ("testpypi", "pypi", "testpypi-validate", "testpypi-evidence"):
        mut = copy.deepcopy(pub)
        for s in mut["jobs"][jobkey]["steps"]:
            if "require_evidence.py" in str(s.get("run", "")):
                s["run"] = f"if {s['run']}; then\n  :\nfi\n"
        probs = wl.lint(rel, mut)
        assert any("EXACT pinned command" in x and jobkey in x for x in probs), (jobkey, probs)


def test_s11_control_trailing_true_on_a_new_line_is_refused():
    """NEGATIVE CONTROL (the trick the OPERATOR BLACKLIST misses): a newline + `true` masks a failed gate
    with NO shell operator, no comment, one invocation -- the old checks are all green, but the exact pin
    (single-line) refuses the multiline command. This is the proof the root fix adds real coverage."""
    rel, pub = _wf()
    mut = copy.deepcopy(pub)
    for s in mut["jobs"]["testpypi"]["steps"]:
        if "require_evidence.py" in str(s.get("run", "")):
            s["run"] = s["run"] + "\ntrue"
    probs = wl.lint(rel, mut)
    assert any("EXACT pinned command" in x for x in probs), probs
    # prove the OLD operator/comment/count checks would NOT have caught it (the blacklist gap)
    only_old = wl._gate_command_problems(mut["jobs"]["testpypi"]["steps"][
        next(i for i, s in enumerate(mut["jobs"]["testpypi"]["steps"]) if "require_evidence.py" in str(s.get("run", "")))
    ]["run"], "require_evidence.py", "S11", "testpypi")
    assert only_old == [], only_old  # blacklist is blind here; only the pin catches it


def test_s9_control_empty_checkout_is_refused_by_lint():
    """NEGATIVE CONTROL: `--checkout ""` (empty) passes the old operator scan AND the old --checkout-present
    regex, yet disables the content binding. The exact pin (`--checkout .`) refuses it."""
    rel, pub = _wf()
    mut = copy.deepcopy(rel)
    for s in mut["jobs"]["npm-identity"]["steps"]:
        if "verify_npm_identity.py" in str(s.get("run", "")):
            s["run"] = s["run"].replace(" --checkout .", ' --checkout ""')
    probs = wl.lint(mut, pub)
    assert any("EXACT pinned command" in x and "npm-identity" in x for x in probs), probs


def test_s9_control_multiline_identity_gate_is_refused():
    """NEGATIVE CONTROL: a multiline npm-identity gate -> exact pin RED."""
    rel, pub = _wf()
    mut = copy.deepcopy(rel)
    for s in mut["jobs"]["npm-identity"]["steps"]:
        if "verify_npm_identity.py" in str(s.get("run", "")):
            s["run"] = s["run"] + "\necho done"
    assert any("EXACT pinned command" in x for x in wl.lint(mut, pub))


def test_s9_empty_and_missing_checkout_are_hard_errors_in_verify_npm_identity(tmp_path):
    """NEGATIVE CONTROL (verify_npm_identity itself): `--checkout ""` and a non-existent --checkout are
    hard errors (exit 2), never a silent content_bound:false. HAPPY PATH: a real dir passes the guard."""
    script = str(SCRIPTS / "verify_npm_identity.py")
    common = ["--manifest", str(tmp_path / "nope.json"), "--dist", str(tmp_path), "--out", str(tmp_path / "o.json")]
    r = subprocess.run([sys.executable, script, *common, "--checkout", ""], capture_output=True, text=True)
    assert r.returncode == 2 and "non-empty" in r.stderr, (r.returncode, r.stderr)
    r = subprocess.run([sys.executable, script, *common, "--checkout", str(tmp_path / "does-not-exist")], capture_output=True, text=True)
    assert r.returncode == 2 and "not a directory" in r.stderr, (r.returncode, r.stderr)
    # HAPPY PATH: a valid --checkout dir passes the guard (it then fails LATER on the bogus manifest, not the guard)
    r = subprocess.run([sys.executable, script, *common, "--checkout", str(tmp_path)], capture_output=True, text=True)
    assert r.returncode != 2 and "non-empty" not in r.stderr and "not a directory" not in r.stderr, (r.returncode, r.stderr)


def test_round3_shell_bypasses_still_refused():
    """Regression: the round-3 shell tricks (`|| require_evidence`, repeated `--need`, `# --checkout .`)
    stay RED under the exact-pin root fix."""
    rel, pub = _wf()
    # || second gate
    mut = copy.deepcopy(pub)
    for s in mut["jobs"]["pypi"]["steps"]:
        if "require_evidence.py" in str(s.get("run", "")):
            s["run"] = s["run"] + " || python3 scripts/require_evidence.py --role promotion --manifest release_manifest.json --evidence-dir evidence"
    assert any("EXACT pinned command" in x for x in wl.lint(rel, mut)) or any("shell control" in x for x in wl.lint(rel, mut))
    # repeated --need
    mut = copy.deepcopy(pub)
    for s in mut["jobs"]["pypi"]["steps"]:
        if "require_evidence.py" in str(s.get("run", "")):
            s["run"] = s["run"].replace("--need R-NI R-TP R-TV", "--need R-NI R-TP R-TV --need")
    assert wl.lint(rel, mut) != []
    # commented --checkout
    mut = copy.deepcopy(rel)
    for s in mut["jobs"]["npm-identity"]["steps"]:
        if "verify_npm_identity.py" in str(s.get("run", "")):
            s["run"] = s["run"].replace(" --checkout .", " # --checkout .")
    assert wl.lint(mut, pub) != []
