"""The paired negative control for the GATE INFRASTRUCTURE itself (review R1/B3).

A suite whose oracle can be skipped away is vacuous. With PYTHSCRIBE_REQUIRE_ORACLE=1 (as CI
sets it) a missing prerequisite must turn the run RED; without it the same run skips and
says so. Both halves are asserted by running the real suite in a subprocess with the
demo artifact disabled.
"""
from __future__ import annotations

import os
import subprocess
import sys

from conftest import GATE_MISSING, REPO

TARGET = "tests/pythscribe/test_differential.py::test_d1_spot_battery_compiled_equals_python_bit_for_bit"


def _run(extra_env):
    env = {**os.environ, "PYTHSCRIBE_DISABLE_ARTIFACTS": "1", **extra_env}
    env.pop("PYTHSCRIBE_REQUIRE_ORACLE", None)
    env.update(extra_env)
    return subprocess.run(
        [sys.executable, "-m", "pytest", TARGET, "-q", "-ra", "-p", "no:cacheprovider"],
        cwd=str(REPO), env=env, capture_output=True, text=True, check=False,
    )


def test_required_mode_turns_a_skipped_oracle_red():
    proc = _run({"PYTHSCRIBE_REQUIRE_ORACLE": "1"})
    assert proc.returncode != 0, proc.stdout[-2000:]
    assert GATE_MISSING in proc.stdout, proc.stdout[-2000:]
    assert "1 failed" in proc.stdout or "1 error" in proc.stdout, proc.stdout[-2000:]


def test_unrequired_mode_skips_and_says_so():
    proc = _run({})
    assert proc.returncode == 0, proc.stdout[-2000:]
    assert "1 skipped" in proc.stdout and "demo artifact not usable" in proc.stdout, proc.stdout[-2000:]
