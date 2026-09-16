"""Relhard round 6 -- the codex pass-6 edge in the B6 API-side leg selector.

codex pass-6 finding: duplicate rows for the same display name AND the same attempt were resolved by
LIST ORDER -- so a leg whose success row happened to sort first was GREEN, while the same
disagreement with the failure row first was RED. A release gate must never depend on the order the
Actions API happens to return duplicate job rows.

Fix (require_ci_success.required_job_problems): WORST-CONCLUSION-WINS over every row at the selected
(latest) attempt -- any non-success row is RED, order-independent. These tests pin BOTH orderings RED
(the paired negative control) and keep the legitimate-rerun path GREEN.
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
import require_ci_success as rcs  # noqa: E402


def _green_rows() -> list[dict]:
    return [{"name": r, "run_attempt": 1, "conclusion": "success"} for r in rcs.REQUIRED_CI_JOBS]


def _dup_rows(leg: str, first: str, second: str, attempt: int = 1) -> list[dict]:
    """All required legs green, but `leg` gets TWO rows at the same attempt with the given conclusions."""
    rows = [{"name": r, "run_attempt": 1, "conclusion": "success"} for r in rcs.REQUIRED_CI_JOBS if r != leg]
    rows.append({"name": leg, "run_attempt": attempt, "conclusion": first})
    rows.append({"name": leg, "run_attempt": attempt, "conclusion": second})
    return rows


LEG = sorted(rcs.REQUIRED_CI_JOBS)[0]  # any required leg; the edge is name-agnostic


def test_pass6_happy_duplicate_rows_both_success_is_green():
    """Two identical success rows for one leg at the same attempt -> still GREEN (duplicates that AGREE)."""
    assert rcs.required_job_problems(_dup_rows(LEG, "success", "success")) == []


def test_pass6_control_failure_first_is_red():
    """NEGATIVE CONTROL: duplicate rows, FAILURE first -> RED (this was RED before the fix too)."""
    p = rcs.required_job_problems(_dup_rows(LEG, "failure", "success"))
    assert any(LEG in x and "not success" in x for x in p), p


def test_pass6_control_success_first_is_red():
    """NEGATIVE CONTROL (the exact codex pass-6 masking): duplicate rows, SUCCESS first -> MUST be RED.
    Pre-fix this returned GREEN because the first-seen success row won the tie by list order. This is
    the row that flips RED only with the worst-conclusion-wins fix -- the mutation-verified control."""
    p = rcs.required_job_problems(_dup_rows(LEG, "success", "failure"))
    assert any(LEG in x and "not success" in x for x in p), p


def test_pass6_control_order_independence_symmetry():
    """The selector is ORDER-INDEPENDENT: success-first and failure-first give the SAME verdict (both RED)."""
    sf = rcs.required_job_problems(_dup_rows(LEG, "success", "failure"))
    ff = rcs.required_job_problems(_dup_rows(LEG, "failure", "success"))
    assert bool(sf) and bool(ff), (sf, ff)  # both non-empty == both RED


def test_pass6_control_other_nonsuccess_conclusions_masked_are_red():
    """Any non-success conclusion (cancelled/timed_out/None/skipped) masked by a success duplicate is RED."""
    for bad in ("cancelled", "timed_out", "skipped", None):
        p = rcs.required_job_problems(_dup_rows(LEG, "success", bad))
        assert any(LEG in x and "not success" in x for x in p), (bad, p)


def test_pass6_regression_legitimate_rerun_latest_attempt_wins_is_green():
    """REGRESSION: a real rerun (attempt-1 failure, attempt-2 success -- DIFFERENT attempts) is GREEN.
    The fix must not break the legitimate 'rerun fixed it' path: the latest attempt is selected, and at
    that attempt the only row is success."""
    rows = [{"name": r, "run_attempt": 1, "conclusion": "success"} for r in rcs.REQUIRED_CI_JOBS if r != LEG]
    rows.append({"name": LEG, "run_attempt": 1, "conclusion": "failure"})  # first attempt failed
    rows.append({"name": LEG, "run_attempt": 2, "conclusion": "success"})  # rerun passed
    assert rcs.required_job_problems(rows) == []


def test_pass6_regression_stale_success_earlier_attempt_cannot_rescue_a_failed_rerun():
    """REGRESSION (the dual): a later attempt that FAILED is RED even though an earlier attempt succeeded --
    the latest attempt is authoritative, so a stale earlier success cannot mask a failed rerun."""
    rows = [{"name": r, "run_attempt": 1, "conclusion": "success"} for r in rcs.REQUIRED_CI_JOBS if r != LEG]
    rows.append({"name": LEG, "run_attempt": 1, "conclusion": "success"})  # earlier attempt passed
    rows.append({"name": LEG, "run_attempt": 2, "conclusion": "failure"})  # rerun FAILED
    p = rcs.required_job_problems(rows)
    assert any(LEG in x and "attempt 2" in x for x in p), p


def test_pass6_verify_end_to_end_success_first_duplicate_is_red():
    """END-TO-END through verify(): a push-to-main run whose jobs include a success-first masked failure
    duplicate is RED at the release gate (not just in the unit selector)."""
    sha = "a" * 40
    run = {"id": 42, "head_sha": sha, "status": "completed", "conclusion": "success",
           "event": "push", "head_branch": "main", "run_attempt": 1}
    problems = rcs.verify(
        sha,
        fetch_runs=lambda wf, s: [run],
        fetch_jobs=lambda _rid: _dup_rows(LEG, "success", "failure"),
    )
    assert any(LEG in x and "not success" in x for x in problems), problems
