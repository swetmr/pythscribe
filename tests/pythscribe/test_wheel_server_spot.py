"""0.2.9 bare-install SERVER gate -- `scripts/wheel_server_spot.py`, the last leg of every wheel leg's
CIBW_TEST_COMMAND (release.yml). wasmtime became a CORE dependency in 0.2.9 (it was wrongly a `[server]`
extra, so a bare `pip install pythscribe` silently ran `@wasm` as plain Python with no speedup); the gate
proves, on a bare install, that a LITERAL `@wasm` kernel runs on the in-process server path.

Every gate ships its PAIRED NEGATIVE CONTROL (the anti-vacuity convention):
  S1  GREEN here: wasmtime importable + the compiler resolvable -> exit 0, `"server_spot": "GREEN"`, mode server.
  S2  RED when wasmtime is NOT importable (an ImportError stub shadows it on PYTHONPATH = the 0.2.8 bare
      install): exit non-zero, the wasmtime assertion names the CORE-dependency fix, no GREEN line.
  S3  RED when the kernel does not end on the server path (PYTHSCRIBE_MODE=fallback pins plain Python):
      exit non-zero, the mode assertion fires -- a script that only checked `wasmtime_available()` would
      pass this mutant, so the mode/counter assertions are load-bearing.
  WF  release.yml wires the gate on every wheel leg, and the workflow lint (M4b) EXACT-PINS the whole
      cibuildwheel test context -- the codex 0.2.9 re-review reproduced three bypasses of a presence check
      (echo-swap / per-OS CIBW_TEST_COMMAND_<OS> override / CIBW_TEST_SKIP) that kept lint GREEN while the
      gate never ran. Each of those mutants, plus `;`/`||` chaining, a masking `pip install wasmtime &&`
      prefix, CIBW_TEST_EXTRAS/REQUIRES/BEFORE_TEST, an environment injection, an extra cibuildwheel flag,
      failure-masking step metadata, and a pyproject `[tool.cibuildwheel]` table, is RED below; the honest
      workflow is GREEN.
The script itself is run as a SUBPROCESS (sys.executable, pythscribe from this checkout via PYTHONPATH),
exactly as cibuildwheel runs it -- only the install origin differs (site-packages there; wheel_m1_spots.py
asserts THAT on the same leg).
"""
from __future__ import annotations

import copy
import json
import os
import subprocess
import sys
from pathlib import Path

import pytest
import yaml

from conftest import REPO, gate
from pythscribe._pin import COMPILER_VERSION
from pythscribe.build import BuildError, find_pyths, pyths_version
from pythscribe.runtime import wasmtime_available

sys.path.insert(0, str(REPO / "scripts"))
import lint_release_workflows as wl  # noqa: E402

SCRIPT = REPO / "scripts" / "wheel_server_spot.py"


def _env(*extra_paths: Path, **overrides: str) -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if k not in ("PYTHSCRIBE_NO_JIT", "PYTHSCRIBE_MODE", "PYTHSCRIBE_CACHE", "PYTHSCRIBE_DISABLE_ARTIFACTS")}
    parts = [str(p) for p in extra_paths] + [str(REPO)] + ([env["PYTHONPATH"]] if env.get("PYTHONPATH") else [])
    env["PYTHONPATH"] = os.pathsep.join(parts)
    env["PYTHONUTF8"] = "1"
    env.update(overrides)
    return env


def _run(env: dict[str, str], cwd: Path) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, str(SCRIPT), str(REPO)], capture_output=True, text=True, check=False, timeout=600, env=env, cwd=str(cwd))


def _require_toolchain() -> None:
    gate(wasmtime_available(), "wasmtime-py required (it is a CORE dep; the S1 arm proves the server path binds)")
    try:
        pyths = find_pyths()
    except BuildError as e:
        gate(False, f"the pinned compiler is required for compile-on-first-call: {e}")
        raise  # unreachable
    # compile-on-first-call refuses a compiler whose version != the pin (a stale staged `_bin` binary in a dev
    # checkout); that is an environment prerequisite, not the gate's verdict -- say so (CI: a FAILURE under
    # PYTHSCRIBE_REQUIRE_ORACLE; the wheel legs always bundle the pinned binary).
    got = pyths_version(pyths)
    gate(got == COMPILER_VERSION, f"`pyths` at {pyths} reports {got!r} but the pin is {COMPILER_VERSION!r} (restage target/release into pythscribe/_bin)")


def test_s1_green_on_a_wasmtime_install_kernel_runs_server_side(tmp_path):
    _require_toolchain()
    r = _run(_env(), tmp_path)
    assert r.returncode == 0, (r.stdout[-2000:], r.stderr[-3000:])
    out = json.loads(r.stdout.strip().splitlines()[-1])
    assert out["server_spot"] == "GREEN" and out["mode"] == "server" and out["value"] == 14.0 and out["wasmtime"] is True, out


def test_s2_red_when_wasmtime_is_not_importable(tmp_path):
    """The 0.2.8 footgun, simulated: an ImportError stub shadows `wasmtime` -> the gate must go RED at the
    FIRST assertion (the core-dependency one), never reach a GREEN line."""
    _require_toolchain()
    shadow = tmp_path / "shadow" / "wasmtime"
    shadow.mkdir(parents=True)
    (shadow / "__init__.py").write_text('raise ImportError("shadowed: simulating a bare install WITHOUT wasmtime")\n', encoding="utf-8")
    r = _run(_env(tmp_path / "shadow"), tmp_path)
    assert r.returncode != 0, (r.stdout, r.stderr[-1500:])
    assert "wasmtime is NOT importable on this bare install" in r.stderr and "CORE dependency" in r.stderr, r.stderr[-2000:]
    assert '"server_spot": "GREEN"' not in r.stdout, r.stdout


def test_s3_red_when_the_kernel_stays_on_python(tmp_path):
    """wasmtime present but the kernel never binds the server path (PYTHSCRIBE_MODE=fallback, an explicit
    request the JIT never overrides): RED on the mode assertion. Discriminates a vacuous gate that only
    probed `wasmtime_available()`."""
    _require_toolchain()
    r = _run(_env(PYTHSCRIBE_MODE="fallback"), tmp_path)
    assert r.returncode != 0, (r.stdout, r.stderr[-1500:])
    assert "did NOT run `@wasm` on the server path" in r.stderr and "'fallback'" in r.stderr, r.stderr[-2000:]
    assert '"server_spot": "GREEN"' not in r.stdout, r.stdout


# ============================================================ WF: the workflow lint (M4b) and its bypass mutants


def _wf() -> tuple[dict, dict]:
    rel = yaml.safe_load((REPO / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8"))
    pub = yaml.safe_load((REPO / ".github" / "workflows" / "publish-pypi.yml").read_text(encoding="utf-8"))
    return rel, pub


def _cibw_step(rel: dict) -> dict:
    [s] = [s for s in rel["jobs"]["wheel"]["steps"] if "cibuildwheel" in str(s.get("run", ""))]
    return s


def _m4b(rel: dict, pub: dict, pyproject_text: str | None = None) -> list[str]:
    return [x for x in wl.lint(rel, pub, pyproject_text) if x.startswith("M4b")]


def test_wf_release_yml_runs_the_gate_on_every_wheel_leg():
    text = (REPO / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8")
    line = next(ln for ln in text.splitlines() if "CIBW_TEST_COMMAND:" in ln)
    assert "scripts/wheel_m1_spots.py {project}" in line and "readme_spots.py" in line and line.rstrip().endswith("scripts/wheel_server_spot.py {project}"), line
    rel, pub = _wf()
    assert wl.lint(rel, pub) == []  # the honest workflow + pyproject are GREEN under the exact pin
    assert wl._ws(_cibw_step(rel)["env"]["CIBW_TEST_COMMAND"]) == wl.PINNED_CIBW_TEST_COMMAND


def _mutate_cmd(rel: dict, fn) -> dict:
    mut = copy.deepcopy(rel)
    s = _cibw_step(mut)
    before = s["env"]["CIBW_TEST_COMMAND"]
    s["env"]["CIBW_TEST_COMMAND"] = fn(before)
    assert s["env"]["CIBW_TEST_COMMAND"] != before, "the mutation must LAND (never a vacuous control)"
    return mut


@pytest.mark.parametrize("name, fn", [
    ("echo-swap: filename present, never executed", lambda c: c.replace("&& python {project}/scripts/wheel_server_spot.py", "&& echo {project}/scripts/wheel_server_spot.py")),
    ("`;` chain: a failing gate no longer fails the leg", lambda c: c.replace("--run && python {project}/scripts/wheel_server_spot.py", "--run ; python {project}/scripts/wheel_server_spot.py")),
    ("`|| true` suffix swallows the gate's exit code", lambda c: c + " || true"),
    ("`||` chain: the gate runs only if the README spots FAIL", lambda c: c.replace("--run && python {project}/scripts/wheel_server_spot.py", "--run || python {project}/scripts/wheel_server_spot.py")),
    ("masking prefix: `pip install wasmtime &&` hides a wasmtime-as-extra regression", lambda c: "pip install wasmtime && " + c),
    ("segment dropped", lambda c: c.replace(" && python {project}/scripts/wheel_server_spot.py {project}", "")),
    ("comment appended (a shell ignores it; a substring lint might not)", lambda c: c + " # && python {project}/scripts/wheel_server_spot.py {project}"),
    ("`:` no-op in place of python", lambda c: c.replace("&& python {project}/scripts/wheel_server_spot.py {project}", "&& : {project}/scripts/wheel_server_spot.py {project}")),
])
def test_wf_m4b_test_command_mutants_are_red(name, fn):
    rel, pub = _wf()
    mut = _mutate_cmd(rel, fn)
    probs = _m4b(mut, pub)
    assert any("EXACT pinned test command" in x for x in probs), (name, probs)


@pytest.mark.parametrize("key, value", [
    ("CIBW_TEST_COMMAND_WINDOWS", "python {project}/scripts/wheel_m1_spots.py {project}"),  # per-OS override drops the gate
    ("CIBW_TEST_COMMAND_LINUX", "true"),
    ("CIBW_TEST_SKIP", "*-win_amd64"),                                                        # skips the platform's test
    ("CIBW_TEST_SKIP", "*"),
    ("CIBW_TEST_EXTRAS", "server"),                                                           # would re-add wasmtime via an extra
    ("CIBW_TEST_REQUIRES", "wasmtime"),                                                       # masks wasmtime-as-extra
    ("CIBW_BEFORE_TEST", "pip install wasmtime"),
    ("CIBW_BEFORE_TEST_WINDOWS", "pip install wasmtime"),
    ("CIBW_TEST_SOURCES", "scripts"),                                                         # re-points {project}
    ("CIBW_TEST_ENVIRONMENT", "PYTHSCRIBE_MODE=fallback"),
    ("CIBW_ENVIRONMENT_WINDOWS", "PYTHSCRIBE_PYTHS=C:/elsewhere/pyths.exe"),                  # per-OS env injection
])
def test_wf_m4b_extra_test_context_keys_are_red_at_every_scope(key, value):
    rel, pub = _wf()
    # step scope
    mut = copy.deepcopy(rel)
    _cibw_step(mut)["env"][key] = value
    assert any(f"`{key}` at step" in x for x in _m4b(mut, pub)), (key, _m4b(mut, pub))
    # job scope
    mut = copy.deepcopy(rel)
    mut["jobs"]["wheel"].setdefault("env", {})[key] = value
    assert any(f"`{key}` at job `wheel` scope" in x for x in _m4b(mut, pub)), (key, _m4b(mut, pub))
    # workflow scope
    mut = copy.deepcopy(rel)
    mut.setdefault("env", {})[key] = value
    assert any(f"`{key}` at workflow scope" in x for x in _m4b(mut, pub)), (key, _m4b(mut, pub))


def test_wf_m4b_environment_pin_and_placement():
    rel, pub = _wf()
    # an injected PYTHSCRIBE_* variable in the pinned CIBW_ENVIRONMENT masks the gate's premise -> RED
    mut = copy.deepcopy(rel)
    _cibw_step(mut)["env"]["CIBW_ENVIRONMENT"] += " PYTHSCRIBE_MODE=fallback"
    assert any("`CIBW_ENVIRONMENT` is not the exact pinned value" in x for x in _m4b(mut, pub))
    # the pinned CIBW_TEST_COMMAND moved to job scope (with the step's removed) -> RED twice: missing on the step, refused at job scope
    mut = copy.deepcopy(rel)
    cmd = _cibw_step(mut)["env"].pop("CIBW_TEST_COMMAND")
    mut["jobs"]["wheel"].setdefault("env", {})["CIBW_TEST_COMMAND"] = cmd
    probs = _m4b(mut, pub)
    assert any("EXACT pinned test command" in x for x in probs) and any("`CIBW_TEST_COMMAND` at job `wheel` scope" in x for x in probs), probs
    # a second cibuildwheel step (one honest, one that would re-run with a different test context) -> RED
    mut = copy.deepcopy(rel)
    mut["jobs"]["wheel"]["steps"].append({"name": "again", "run": "pipx run cibuildwheel==4.2.1 --output-dir wheelhouse", "env": {"CIBW_TEST_COMMAND": "true"}})
    assert any("EXACTLY ONE cibuildwheel step" in x for x in _m4b(mut, pub))


@pytest.mark.parametrize("name, mutate", [
    ("cibuildwheel version downgraded (==3.0.0)", lambda s: s.update(run=s["run"].replace("cibuildwheel==4.2.1", "cibuildwheel==3.0.0"))),
    ("cibuildwheel version unpinned (no ==)", lambda s: s.update(run=s["run"].replace("cibuildwheel==4.2.1", "cibuildwheel"))),
    ("--config-file opens a second test-context channel", lambda s: s.update(run=s["run"].rstrip() + " --config-file cibw.toml")),
    ("--only narrows the legs", lambda s: s.update(run=s["run"].rstrip() + " --only cp312-manylinux_x86_64")),
    ("a wrapper script instead of the pinned invocation", lambda s: s.update(run="bash scripts/run_cibw.sh")),
    ("continue-on-error lets the job proceed after the gate FAILS", lambda s: s.update({"continue-on-error": True})),
    ("an `if:` skips the step", lambda s: s.update({"if": "false"})),
    ("a custom shell can `|| true` the command", lambda s: s.update(shell="bash -c '{0} || true'")),
])
def test_wf_m4b_cibuildwheel_invocation_and_step_metadata_mutants_are_red(name, mutate):
    rel, pub = _wf()
    mut = copy.deepcopy(rel)
    mutate(_cibw_step(mut))
    probs = _m4b(mut, pub)
    assert probs, name
    # a wrapper hides the cibuildwheel invocation entirely -> refused as "no cibuildwheel step" (also RED)
    assert any(("pinned invocation" in x) or ("EXACTLY ONE cibuildwheel step" in x) or ("disallowed step key" in x) or ("standard shell" in x) for x in probs), (name, probs)


def _writer_step(rel: dict) -> dict:
    [s] = [s for s in rel["jobs"]["wheel"]["steps"] if "GITHUB_ENV" in str(s.get("run", ""))]
    return s


def test_wf_m4b_honest_wheel_job_has_exactly_the_pinned_github_env_writer():
    rel, pub = _wf()
    steps = rel["jobs"]["wheel"]["steps"]
    writers = [s for s in steps if wl.GITHUB_ENV_WRITE_RE.search(str(s.get("run", "")))]
    assert len(writers) == 1 and wl._ws(writers[0]["run"]) == wl.PINNED_WHEEL_GITHUB_ENV_WRITER
    assert wl._ws(_cibw_step(rel)["run"]) == wl.PINNED_CIBW_RUN == "pipx run cibuildwheel==4.2.1 --output-dir wheelhouse"
    assert _m4b(rel, pub) == []


@pytest.mark.parametrize("name, step", [
    ("earlier step writes CIBW_TEST_SKIP=* to $GITHUB_ENV (codex round-3 repro)", {"name": "sneak", "run": 'echo "CIBW_TEST_SKIP=*" >> "$GITHUB_ENV"'}),
    ("earlier step writes CIBW_TEST_COMMAND=true to $GITHUB_ENV", {"name": "sneak", "run": 'echo "CIBW_TEST_COMMAND=true" >> "$GITHUB_ENV"'}),
    ("${GITHUB_ENV} spelling", {"name": "sneak", "run": 'echo "CIBW_TEST_SKIP=*" >> "${GITHUB_ENV}"'}),
    ("python heredoc writer", {"name": "sneak", "shell": "bash", "run": "python - <<'EOF' >> \"$GITHUB_ENV\"\nprint('CIBW_BEFORE_TEST=pip install wasmtime')\nEOF\n"}),
    ("$GITHUB_PATH shim (a fake python ahead on PATH)", {"name": "sneak", "run": 'mkdir -p shim && printf \'#!/bin/sh\\nexit 0\\n\' > shim/python && chmod +x shim/python && echo "$PWD/shim" >> "$GITHUB_PATH"'}),
    ("non-CIBW runtime injection (PIP_NO_DEPS masks the core-dep install)", {"name": "sneak", "run": 'echo "PIP_NO_DEPS=1" >> "$GITHUB_ENV"'}),
])
@pytest.mark.parametrize("where", ["before-cibuildwheel", "first", "after-cibuildwheel"])
def test_wf_m4b_github_env_writers_in_the_wheel_job_are_red(name, step, where):
    rel, pub = _wf()
    mut = copy.deepcopy(rel)
    steps = mut["jobs"]["wheel"]["steps"]
    ci = next(i for i, s in enumerate(steps) if "cibuildwheel" in str(s.get("run", "")))
    idx = {"before-cibuildwheel": ci, "first": 0, "after-cibuildwheel": ci + 1}[where]
    steps.insert(idx, dict(step))
    probs = _m4b(mut, pub)
    assert any("writes $GITHUB_ENV/$GITHUB_PATH and is not the pinned" in x for x in probs), (name, where, probs)


@pytest.mark.parametrize("env", [{"PIP_NO_DEPS": "1"}, {"PIP_CONSTRAINT": "c.txt"}, {"PYTHSCRIBE_MODE": "fallback"}, {"HARMLESS": "x"}])
def test_wf_m4b_any_job_scope_env_on_wheel_is_red(env):
    rel, pub = _wf()
    assert not rel["jobs"]["wheel"].get("env")
    mut = copy.deepcopy(rel)
    mut["jobs"]["wheel"]["env"] = env
    assert any("sets a job-scope `env`" in x for x in _m4b(mut, pub)), _m4b(mut, pub)


def test_wf_m4b_edited_or_missing_pinned_writer_is_red():
    rel, pub = _wf()
    # the pinned writer step EDITED (a CIBW line appended to its heredoc-free tail) -> RED both ways
    mut = copy.deepcopy(rel)
    w = _writer_step(mut)
    w["run"] = w["run"].rstrip("\n") + '\necho "CIBW_TEST_SKIP=*" >> "$GITHUB_ENV"\n'
    probs = _m4b(mut, pub)
    assert any("is not the pinned B_t staging step" in x for x in probs) and any("missing or edited" in x for x in probs), probs
    # the writer's $GITHUB_ENV line silently changed (a different key) -> RED
    mut = copy.deepcopy(rel)
    w = _writer_step(mut)
    w["run"] = w["run"].replace("PYTHSCRIBE_RELEASE_BINARY_SHA256=", "PYTHSCRIBE_RELEASE_BINARY_SHA256=x")
    assert any("is not the pinned B_t staging step" in x for x in _m4b(mut, pub))
    # the pinned writer removed entirely -> RED (the pin is the allowlist, and the sha256 the repair hook needs is gone)
    mut = copy.deepcopy(rel)
    mut["jobs"]["wheel"]["steps"] = [s for s in mut["jobs"]["wheel"]["steps"] if "GITHUB_ENV" not in str(s.get("run", ""))]
    assert any("missing or edited" in x for x in _m4b(mut, pub))


# ---------------------------------------------------------------- the GOLDEN PIN of the whole `wheel` job (codex r4)


def test_wf_golden_pin_matches_the_honest_wheel_job_exactly():
    rel, pub = _wf()
    w = rel["jobs"]["wheel"]
    assert [wl._canon_step(s) for s in w["steps"]] == wl.PINNED_WHEEL_STEPS
    assert {k: w[k] for k in ("name", "needs", "runs-on", "strategy")} == wl.PINNED_WHEEL_JOB
    assert set(w) == wl.PINNED_WHEEL_JOB_KEYS
    assert len(wl.PINNED_WHEEL_STEPS) == 8 and wl.PINNED_WHEEL_STEPS[4]["if"] == "matrix.archs == 'aarch64'"
    assert not [x for x in wl.lint(rel, pub) if x.startswith("GP")]


def _gp(rel, pub) -> list[str]:
    return [x for x in wl.lint(rel, pub) if x.startswith("GP")]


@pytest.mark.parametrize("name, step", [
    ("(i) indirect-variable $GITHUB_ENV writer (no literal CIBW_ key in the text)", {"name": "sneak", "run": 'V=CIBW_TEST_SKIP; echo "$V=*" >> "$GITHUB_ENV"'}),
    ("(ii) actions/github-script core.exportVariable", {"name": "sneak", "uses": "actions/github-script@v7", "with": {"script": "core.exportVariable('CIBW_TEST_SKIP', '*')"}}),
    ("(iii) an arbitrary extra run: step", {"name": "extra", "run": "echo hello"}),
    ("(iii') an arbitrary extra action", {"uses": "actions/cache@v4", "with": {"path": "~/.cache", "key": "x"}}),
    ("(i') a plain $GITHUB_ENV writer", {"name": "sneak", "run": 'echo "CIBW_TEST_SKIP=*" >> "$GITHUB_ENV"'}),
])
@pytest.mark.parametrize("where", ["first", "before-cibuildwheel", "after-cibuildwheel", "last"])
def test_wf_golden_pin_any_extra_step_is_red(name, step, where):
    rel, pub = _wf()
    mut = copy.deepcopy(rel)
    steps = mut["jobs"]["wheel"]["steps"]
    ci = next(i for i, s in enumerate(steps) if "cibuildwheel" in str(s.get("run", "")))
    idx = {"first": 0, "before-cibuildwheel": ci, "after-cibuildwheel": ci + 1, "last": len(steps)}[where]
    steps.insert(idx, dict(step))
    probs = _gp(mut, pub)
    assert any("GOLDEN PIN" in x for x in probs), (name, where, probs)
    assert any(f"step {idx}" in x or "has 9 steps" in x for x in probs), (name, where, probs)


def test_wf_golden_pin_reordered_removed_and_edited_steps_are_red():
    rel, pub = _wf()
    # (iv) a removed pinned step (the QEMU setup)
    mut = copy.deepcopy(rel)
    mut["jobs"]["wheel"]["steps"] = [s for s in mut["jobs"]["wheel"]["steps"] if "qemu" not in str(s.get("uses", ""))]
    assert any("has 7 steps" in x for x in _gp(mut, pub))
    # (iv') two pinned steps swapped (stage B_t after cibuildwheel)
    mut = copy.deepcopy(rel)
    st = mut["jobs"]["wheel"]["steps"]
    st[3], st[5] = st[5], st[3]
    probs = _gp(mut, pub)
    assert any("step 3" in x for x in probs) and any("step 5" in x for x in probs), probs
    # (v) an edited SHA on a pinned action
    mut = copy.deepcopy(rel)
    mut["jobs"]["wheel"]["steps"][0]["uses"] = "actions/checkout@0000000000000000000000000000000000000000"
    assert any("step 0" in x and "uses" in x for x in _gp(mut, pub))
    # (v') a floating tag instead of the pinned one
    mut = copy.deepcopy(rel)
    mut["jobs"]["wheel"]["steps"][7]["uses"] = "actions/upload-artifact@v4"
    assert any("step 7" in x for x in _gp(mut, pub))
    # (v'') edited run text / with: / if: / env: on pinned steps
    mut = copy.deepcopy(rel)
    mut["jobs"]["wheel"]["steps"][6]["run"] = mut["jobs"]["wheel"]["steps"][6]["run"].replace("verify_wheel_clean.py", "true")
    assert any("step 6" in x and "run" in x for x in _gp(mut, pub))
    mut = copy.deepcopy(rel)
    mut["jobs"]["wheel"]["steps"][1]["with"]["python-version"] = "3.13"
    assert any("step 1" in x and "with" in x for x in _gp(mut, pub))
    mut = copy.deepcopy(rel)
    mut["jobs"]["wheel"]["steps"][4]["if"] = "always()"
    assert any("step 4" in x and "if" in x for x in _gp(mut, pub))
    mut = copy.deepcopy(rel)
    del mut["jobs"]["wheel"]["steps"][4]["if"]
    assert any("step 4" in x for x in _gp(mut, pub))
    mut = copy.deepcopy(rel)
    _cibw_step(mut)["env"]["CIBW_BUILD"] = "cp313-*"
    assert any("step 5" in x and "env" in x for x in _gp(mut, pub))
    mut = copy.deepcopy(rel)
    _cibw_step(mut)["continue-on-error"] = True
    assert any("step 5" in x and "continue-on-error" in x for x in _gp(mut, pub))
    # job-level: an extra key (container / defaults / env), a changed needs, a narrowed matrix
    for key, val in (("container", "python:3.12"), ("defaults", {"run": {"shell": "bash -c '{0} || true'"}}), ("env", {"CIBW_TEST_SKIP": "*"}), ("timeout-minutes", 5)):
        mut = copy.deepcopy(rel)
        mut["jobs"]["wheel"][key] = val
        assert any("job keys" in x and key in x for x in _gp(mut, pub)), key
    mut = copy.deepcopy(rel)
    mut["jobs"]["wheel"]["needs"] = ["build", "prepare"]
    assert any("`needs`" in x for x in _gp(mut, pub))
    mut = copy.deepcopy(rel)
    mut["jobs"]["wheel"]["strategy"]["matrix"]["include"].pop()
    assert any("`strategy`" in x for x in _gp(mut, pub))


CODEX_R5_ROOT_DEFAULTS = {"run": {"shell": "bash --noprofile --norc -c \"export CIBW_TEST_SKIP=*; exec bash -e '{0}'\""}}


def test_wf_golden_pin_workflow_level_root_defaults_and_env_are_red():
    """codex r5 1(b): a ROOT `defaults.run.shell` is inherited by the shell-less cibuildwheel step and exports
    CIBW_TEST_SKIP=* around it -> every wheel test skipped while every job/step pin stayed GREEN. The workflow
    level is now pinned like the job: exact top-level key set + exact `env`."""
    rel, pub = _wf()
    assert set(rel) == wl.PINNED_WORKFLOW_KEYS and rel["env"] == wl.PINNED_WORKFLOW_ENV
    assert not [x for x in wl.lint(rel, pub) if x.startswith("GP-WF")]
    # codex's exact root block -> RED
    mut = copy.deepcopy(rel)
    mut["defaults"] = CODEX_R5_ROOT_DEFAULTS
    probs = [x for x in wl.lint(mut, pub) if x.startswith("GP-WF")]
    assert any("top-level keys" in x and "'defaults'" in x for x in probs), probs
    # a BENIGN root defaults.run.shell is refused just the same (allowlist-shape: the honest workflow has none)
    mut = copy.deepcopy(rel)
    mut["defaults"] = {"run": {"shell": "bash"}}
    assert any("'defaults'" in x for x in wl.lint(mut, pub) if x.startswith("GP-WF"))
    # any other root key (concurrency), an extra/changed workflow env key
    mut = copy.deepcopy(rel)
    mut["concurrency"] = {"group": "release"}
    assert any("'concurrency'" in x for x in wl.lint(mut, pub) if x.startswith("GP-WF"))
    for env in ({"PLAYWRIGHT_VERSION": "1.55.0", "PIP_NO_DEPS": "1"}, {"PLAYWRIGHT_VERSION": "1.55.0", "BASH_ENV": "x.sh"}, {"PLAYWRIGHT_VERSION": "1.56.0"}, {}):
        mut = copy.deepcopy(rel)
        mut["env"] = env
        assert any("workflow `env` is not the GOLDEN PIN" in x for x in wl.lint(mut, pub)), env
    mut = copy.deepcopy(rel)
    del mut["env"]
    assert any("top-level keys" in x for x in wl.lint(mut, pub) if x.startswith("GP-WF"))
    # a JOB-level defaults on `wheel` is RED via the job key pin (confirming the remaining inheritance level)
    mut = copy.deepcopy(rel)
    mut["jobs"]["wheel"]["defaults"] = CODEX_R5_ROOT_DEFAULTS
    assert any("job keys" in x and "defaults" in x for x in _gp(mut, pub))
    # and an explicit hijacked shell ON the cibuildwheel step is RED via the step pin
    mut = copy.deepcopy(rel)
    _cibw_step(mut)["shell"] = CODEX_R5_ROOT_DEFAULTS["run"]["shell"]
    assert any("step 5" in x and "shell" in x for x in _gp(mut, pub))


def test_wf_m4b_pyproject_cibuildwheel_table_is_red():
    rel, pub = _wf()
    real = (REPO / "pyproject.toml").read_text(encoding="utf-8")
    assert _m4b(rel, pub, real) == []
    for table in ("[tool.cibuildwheel]\ntest-skip = \"*\"\n", "[tool.cibuildwheel]\ntest-command = \"true\"\n",
                  "[tool.cibuildwheel.windows]\nbefore-test = \"pip install wasmtime\"\n"):
        probs = _m4b(rel, pub, real + "\n" + table)
        assert any("[tool.cibuildwheel]" in x for x in probs), (table, probs)
    assert any("not valid TOML" in x for x in _m4b(rel, pub, real + "\n[broken\n"))
