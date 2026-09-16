"""Round-5 release-hardening controls (codex PASS-5: the 3 residuals -> ZERO / TAG-SAFE).

The three residuals codex pass-5 left open, each closed at the ROOT and each shipping (the anti-vacuity paired-control convention)
its PAIRED NEGATIVE CONTROL -- a witness that provably goes RED when the property is violated -- AND a
HAPPY-PATH control proving the honest release path still lints/verifies GREEN (the recurring batch
failure mode was fixes that broke the honest path):

  1. HIGH   step-metadata failure-masking bypass (lint_release_workflows.py): the exact-command pin
            validated only `run:`; a gate STEP could carry continue-on-error / if / background / a
            custom shell and stay GREEN while defeating the gate. ROOT FIX: an ALLOWLIST of permitted
            gate-step keys closes the CLASS (no 4th metadata trick), + standard-shell-only.
  2. MEDIUM B6 bound by display name, not required job KEY (require_ci_success.py): delete jobs.lint +
            add fake-lint named Lint passed. ROOT FIX: required job KEYS are the authority; the family
            is derived from those exact keys; missing/renamed key, duplicate display name, or a
            drifted family is RED, inside verify() too.
  3. MEDIUM omitted --checkout accepted (verify_npm_identity.py): OMISSION exited 0 -> a vacuous
            content_bound:false R-NI. ROOT FIX: --checkout is argparse-REQUIRED (fail-closed).

Offline / injected fakes only; the live registry / Actions API / gh portions run in CI.
"""
from __future__ import annotations

import copy
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

RELEASE = REPO / ".github" / "workflows" / "release.yml"
PUBLISH = REPO / ".github" / "workflows" / "publish-pypi.yml"
CI = REPO / ".github" / "workflows" / "ci.yml"

PROMO_JOBS = ("testpypi", "pypi", "testpypi-validate", "testpypi-evidence")


def _wf():
    return yaml.safe_load(RELEASE.read_text(encoding="utf-8")), yaml.safe_load(PUBLISH.read_text(encoding="utf-8"))


def _set_on_gate(wf: dict, jobkey: str, needle: str, **meta) -> dict:
    """Deep-copy `wf`; set step-level `meta` on every step of `jobkey` whose run contains `needle`."""
    mut = copy.deepcopy(wf)
    for s in mut["jobs"][jobkey]["steps"]:
        if needle in str(s.get("run", "")):
            s.update(meta)
    return mut


# ============================================================================ Fix 1: step-metadata allowlist


def test_fix1_primitive_allowlist_and_shell():
    """The primitive: only allowlisted keys are permitted, and a `shell:` must be the standard shell."""
    ok = {"name": "gate", "run": "python x", "env": {"T": "1"}, "shell": "bash"}
    assert wl._gate_step_metadata_problems(ok, "P1", "testpypi") == []
    for bad_key in ("continue-on-error", "if", "background", "timeout-minutes"):
        p = wl._gate_step_metadata_problems({"run": "python x", bad_key: True}, "P1", "testpypi")
        assert any("disallowed step key" in x and bad_key in x for x in p), (bad_key, p)
    p = wl._gate_step_metadata_problems({"run": "python x", "shell": "bash -c '... || true'"}, "P1", "testpypi")
    assert any("standard shell" in x for x in p), p


def test_fix1_happy_path_real_gate_steps_pass_and_lint_green():
    """HAPPY PATH (no false-RED): every real promotion gate step + the npm-identity gate step carries only
    allowlisted keys (testpypi-validate legitimately uses `shell: bash`), and the full lint stays GREEN."""
    rel, pub = _wf()
    assert wl.lint(rel, pub) == []
    for jk in PROMO_JOBS:
        for s in pub["jobs"][jk]["steps"]:
            if "require_evidence.py" in str(s.get("run", "")):
                assert wl._gate_step_metadata_problems(s, "P1", jk) == [], (jk, s)
    for s in rel["jobs"]["npm-identity"]["steps"]:
        if "verify_npm_identity.py" in str(s.get("run", "")):
            assert wl._gate_step_metadata_problems(s, "S9", "npm-identity") == [], s


def test_fix1_control_continue_on_error_on_every_promotion_job_and_npm_identity_is_red():
    """NEGATIVE CONTROL: `continue-on-error: true` (the job proceeds AFTER the gate fails) on each of the 4
    promotion gates AND the npm-identity gate -> lint RED. This is the exact codex pass-5 reproduction."""
    rel, pub = _wf()
    for jk in PROMO_JOBS:
        mut = _set_on_gate(pub, jk, "require_evidence.py", **{"continue-on-error": True})
        probs = wl.lint(rel, mut)
        assert any("disallowed step key" in x and "continue-on-error" in x and jk in x for x in probs), (jk, probs)
    mut = _set_on_gate(rel, "npm-identity", "verify_npm_identity.py", **{"continue-on-error": True})
    probs = wl.lint(mut, pub)
    assert any("disallowed step key" in x and "continue-on-error" in x and "npm-identity" in x for x in probs), probs


def test_fix1_control_background_is_red():
    """NEGATIVE CONTROL: `background: true` (detach the gate) on a promotion gate and on npm-identity -> RED."""
    rel, pub = _wf()
    assert any("background" in x for x in wl.lint(rel, _set_on_gate(pub, "pypi", "require_evidence.py", background=True)))
    assert any("background" in x for x in wl.lint(_set_on_gate(rel, "npm-identity", "verify_npm_identity.py", background=True), pub))


def test_fix1_control_custom_shell_is_red():
    """NEGATIVE CONTROL: a custom `shell: bash -c '... || true'` (masks a failed gate) -> RED, on both a
    promotion gate and npm-identity."""
    rel, pub = _wf()
    assert any("standard shell" in x for x in wl.lint(rel, _set_on_gate(pub, "testpypi", "require_evidence.py", shell="bash -c 'source {0} || true'")))
    assert any("standard shell" in x for x in wl.lint(_set_on_gate(rel, "npm-identity", "verify_npm_identity.py", shell="bash -c ': || true'"), pub))


def test_fix1_control_if_on_the_gate_step_is_red():
    """NEGATIVE CONTROL: an `if:` on the gate step (a skipped step does not fail the job) -> RED, on both a
    promotion gate and npm-identity. (`if` masking is the class member the exact-`run:` pin does not see.)"""
    rel, pub = _wf()
    assert any("disallowed step key" in x and "'if'" in x for x in wl.lint(rel, _set_on_gate(pub, "testpypi-validate", "require_evidence.py", **{"if": "always()"})))
    assert any("disallowed step key" in x and "'if'" in x for x in wl.lint(_set_on_gate(rel, "npm-identity", "verify_npm_identity.py", **{"if": "always()"}), pub))


def test_fix1_class_closure_any_unknown_key_is_red():
    """CLASS CLOSURE: the fix is an ALLOWLIST, so ANY key beyond it -- even one no codex pass named
    (`timeout-minutes`, or a wholly invented `masked`) -- is RED by construction. There is no 4th trick."""
    rel, pub = _wf()
    for meta in ({"timeout-minutes": 0}, {"masked-by-future-trick": True}):
        probs = wl.lint(rel, _set_on_gate(pub, "pypi", "require_evidence.py", **meta))
        assert any("disallowed step key" in x for x in probs), (meta, probs)


# ============================================================================ Fix 2: B6 required job KEY authority


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


def test_fix2_happy_path_key_binding_green_on_real_ci():
    """HAPPY PATH: every REQUIRED_CI_JOB_KEYS entry is a real ci.yml KEY, the derived family == REQUIRED_CI_JOBS,
    no duplicate display names -> binding GREEN, and verify() (which runs the binding) GREEN on an all-green run."""
    assert rcs.REQUIRED_CI_JOB_KEYS  # the KEY authority exists
    assert rcs.binding_problems() == []
    sha = "a" * 40
    assert rcs.verify(sha, fetch_runs=lambda wf, s: [_push_run(sha)], fetch_jobs=_all_jobs_green) == []


def test_fix2_control_key_substitution_reds_binding_and_verify(tmp_path):
    """NEGATIVE CONTROL (the exact codex pass-5 reproduction): delete `jobs.lint` and add a no-op job under
    key `fake-lint` with `name: Lint`. The display name "Lint" still exists, but the required KEY `lint` is
    gone -> binding RED (missing KEY) AND, because verify() runs the binding, verify() RED. A required job
    can no longer be replaced by an unrelated job reusing its display name."""
    def substitute(ci):
        del ci["jobs"]["lint"]
        ci["jobs"]["fake-lint"] = {"name": "Lint", "runs-on": "ubuntu-latest", "steps": [{"run": "true"}]}
    p = _ci_variant(tmp_path, substitute)
    bp = rcs.binding_problems(p)
    assert any("KEY 'lint'" in x for x in bp), bp
    sha = "a" * 40
    vp = rcs.verify(sha, fetch_runs=lambda wf, s: [_push_run(sha)], fetch_jobs=_all_jobs_green, ci_path=p)
    assert vp != [], vp


def test_fix2_control_renamed_required_key_is_red(tmp_path):
    """NEGATIVE CONTROL: renaming a required KEY (`lint` -> `linting`) -> binding RED even though the display
    name is unchanged (the KEY is the authority, not the name)."""
    def rename(ci):
        ci["jobs"]["linting"] = ci["jobs"].pop("lint")
    p = _ci_variant(tmp_path, rename)
    assert any("KEY 'lint'" in x for x in rcs.binding_problems(p)), rcs.binding_problems(p)


def test_fix2_control_duplicate_display_name_across_keys_is_red(tmp_path):
    """NEGATIVE CONTROL: keep `jobs.lint` but ADD `fake-lint` also named `Lint` -> two keys produce the same
    display name -> ambiguous run->required binding -> RED (a no-op job could shadow the required one)."""
    def dup(ci):
        ci["jobs"]["fake-lint"] = {"name": "Lint", "runs-on": "ubuntu-latest", "steps": [{"run": "true"}]}
    p = _ci_variant(tmp_path, dup)
    bp = rcs.binding_problems(p)
    assert any("MULTIPLE job keys" in x and "Lint" in x for x in bp), bp


def test_fix2_regression_added_matrix_leg_and_removed_job_still_red(tmp_path):
    """REGRESSION (round-4 controls survive the root fix): an added matrix leg -> RED naming the leg +
    REQUIRED_CI_JOBS; a removed required job -> RED naming its display name."""
    p = _ci_variant(tmp_path, lambda ci: ci["jobs"]["test"]["strategy"]["matrix"]["os"].append("freebsd-latest"))
    assert any("freebsd-latest" in x and "REQUIRED_CI_JOBS" in x for x in rcs.binding_problems(p)), rcs.binding_problems(p)
    p2 = _ci_variant(tmp_path, lambda ci: ci["jobs"].pop("lint"))
    assert any("'Lint'" in x for x in rcs.binding_problems(p2)), rcs.binding_problems(p2)


# ============================================================================ Fix 3: --checkout is required (fail-closed)


def test_fix3_control_omitted_checkout_exits_nonzero(tmp_path):
    """NEGATIVE CONTROL (the residual codex pass-5 hole): OMITTING `--checkout` ENTIRELY -- not just empty --
    now exits non-zero (argparse-required), so verify() can no longer run with checkout=None and emit a
    vacuous content_bound:false R-NI."""
    script = str(SCRIPTS / "verify_npm_identity.py")
    r = subprocess.run([sys.executable, script, "--manifest", str(tmp_path / "m.json"),
                        "--dist", str(tmp_path), "--out", str(tmp_path / "o.json")],
                       capture_output=True, text=True)
    assert r.returncode != 0, (r.returncode, r.stdout, r.stderr)
    assert "checkout" in (r.stderr + r.stdout).lower(), (r.stdout, r.stderr)


def test_fix3_regression_empty_and_nonexistent_checkout_are_exit_2_and_valid_dir_passes_guard(tmp_path):
    """REGRESSION + HAPPY PATH: `--checkout ""` and a non-existent dir stay hard exit-2; a real dir PASSES
    the guard (then fails LATER on the bogus manifest, proving no false-RED on the honest invocation)."""
    script = str(SCRIPTS / "verify_npm_identity.py")
    common = ["--manifest", str(tmp_path / "nope.json"), "--dist", str(tmp_path), "--out", str(tmp_path / "o.json")]
    r = subprocess.run([sys.executable, script, *common, "--checkout", ""], capture_output=True, text=True)
    assert r.returncode == 2 and "non-empty" in r.stderr, (r.returncode, r.stderr)
    r = subprocess.run([sys.executable, script, *common, "--checkout", str(tmp_path / "does-not-exist")], capture_output=True, text=True)
    assert r.returncode == 2 and "not a directory" in r.stderr, (r.returncode, r.stderr)
    r = subprocess.run([sys.executable, script, *common, "--checkout", str(tmp_path)], capture_output=True, text=True)
    assert r.returncode != 2 and "non-empty" not in r.stderr and "not a directory" not in r.stderr, (r.returncode, r.stderr)


# ============================================================================ previously-CLOSED set stays closed


def test_previously_closed_set_stays_green():
    """The pass-1..4 CLOSED set is untouched by this round: the full workflow lint is GREEN and the exact
    ci.yml key binding + an all-green push run verify GREEN (the honest release path is intact)."""
    rel, pub = _wf()
    assert wl.lint(rel, pub) == []
    assert rcs.binding_problems() == []
    sha = "b" * 40
    assert rcs.verify(sha, fetch_runs=lambda wf, s: [_push_run(sha)], fetch_jobs=_all_jobs_green) == []
