"""Relhard round 7 -- the post-publish PyPI set-proof guard (codex astra pass-3/4).

The `pypi` (production) publish is RESUMABLE (skip-existing:true), so the POST-publish set-proof
(assemble_rtp against upload.pypi.org -> PyPI serves EXACTLY the manifest's distribution set + digests)
is the ONLY catch for a partial/wrong IRREVERSIBLE production upload. astra showed a substring-presence
lint check is VACUOUS: the proof can be neutered while lint stays GREEN via echo-prefix, `|| true`,
step/job continue-on-error, if:false, a `# upload.pypi.org` comment, a defaults.run.shell wrapper, a
proof-step `env: {SHELLOPTS: noexec}`, or a mispointing working-directory.

ROOT FIX (lint_release_workflows PP block): the proof is validated the SAME way as the promotion gates
-- exactly-one invocation + no shell control/comment (`_gate_command_problems`), an execution-neutral
step-key allowlist (`_gate_step_metadata_problems`), an EXACT command pin (`_exact_pin_problems` vs
PYPI_PROOF_PIN) -- PLUS the exec-CONTEXT the shared gates miss: job-level continue-on-error, a
workflow/job defaults.run.shell wrapper, and a TIGHTER proof-step allowlist (name/id/run/shell only --
no env/working-directory). These tests are that gate's PAIRED negative controls: each bypass goes RED,
and the honest workflow stays GREEN (the recurring failure mode was fixes that broke the honest path).

Offline / in-memory mutations only.
"""
from __future__ import annotations

import copy
import sys
from pathlib import Path

import yaml

from conftest import REPO

sys.path.insert(0, str(REPO / "scripts"))
import lint_release_workflows as wl  # noqa: E402

RELEASE = REPO / ".github" / "workflows" / "release.yml"
PUBLISH = REPO / ".github" / "workflows" / "publish-pypi.yml"


def _wf() -> tuple[dict, dict]:
    return (yaml.safe_load(RELEASE.read_text(encoding="utf-8")),
            yaml.safe_load(PUBLISH.read_text(encoding="utf-8")))


def _pp(rel: dict, pub: dict) -> list[str]:
    """The PP-guard problems only (the post-publish-proof family)."""
    return [x for x in wl.lint(rel, pub) if x.startswith("PP:")]


def _proof_step(pub: dict) -> dict:
    return next(s for s in pub["jobs"]["pypi"]["steps"] if "assemble_rtp.py" in str(s.get("run", "")))


def test_happy_path_is_pp_clean_and_lint_green():
    rel, pub = _wf()
    assert _pp(rel, pub) == [], _pp(rel, pub)              # the honest workflow has NO PP problem
    assert wl.lint(rel, pub) == [], wl.lint(rel, pub)      # ...and the whole lint is GREEN


def test_proof_removed_is_red():
    rel, pub = _wf()
    pub["jobs"]["pypi"]["steps"] = [s for s in pub["jobs"]["pypi"]["steps"] if "assemble_rtp.py" not in str(s.get("run", ""))]
    assert any("NO post-publish set-proof" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_skip_existing_false_is_red():
    rel, pub = _wf()
    for s in pub["jobs"]["pypi"]["steps"]:
        if "gh-action-pypi-publish" in str(s.get("uses", "")):
            s.setdefault("with", {})["skip-existing"] = False
    assert any("RESUMABLE" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_echo_prefixed_proof_is_red():
    rel, pub = _wf()
    _proof_step(pub)["run"] = "echo " + _proof_step(pub)["run"]
    assert _pp(rel, pub), "an echo-prefixed (no-op) proof must not lint clean"


def test_or_true_suffixed_proof_is_red():
    rel, pub = _wf()
    s = _proof_step(pub)
    s["run"] = s["run"].strip() + " || true"
    assert _pp(rel, pub), "a `|| true`-suffixed proof (failure swallowed) must not lint clean"


def test_step_continue_on_error_is_red():
    rel, pub = _wf()
    _proof_step(pub)["continue-on-error"] = True
    assert _pp(rel, pub), "a continue-on-error proof step must not lint clean"


def test_if_false_proof_is_red():
    rel, pub = _wf()
    _proof_step(pub)["if"] = "false"
    assert _pp(rel, pub), "an `if: false` (skipped) proof must not lint clean"


def test_mispointed_at_testpypi_with_comment_is_red():
    rel, pub = _wf()
    s = _proof_step(pub)
    s["run"] = s["run"].replace("upload.pypi.org/legacy", "test.pypi.org/legacy") + "  # upload.pypi.org"
    assert _pp(rel, pub), "a proof pointed at TestPyPI (with an upload.pypi.org COMMENT) must not lint clean"


def test_job_level_continue_on_error_is_red():
    rel, pub = _wf()
    pub["jobs"]["pypi"]["continue-on-error"] = True
    assert any("continue-on-error" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_workflow_defaults_shell_wrapper_is_red():
    rel, pub = _wf()
    pub["defaults"] = {"run": {"shell": "bash -c 'source {0} || true'"}}
    assert any("defaults.run.shell" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_job_defaults_shell_wrapper_is_red():
    rel, pub = _wf()
    pub["jobs"]["pypi"]["defaults"] = {"run": {"shell": "bash -c 'source {0} || true'"}}
    assert any("defaults.run.shell" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_proof_step_env_is_red():
    rel, pub = _wf()
    _proof_step(pub)["env"] = {"SHELLOPTS": "noexec"}
    assert any("disallowed key" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_proof_step_working_directory_is_red():
    rel, pub = _wf()
    _proof_step(pub)["working-directory"] = "/tmp"
    assert any("disallowed key" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_proof_before_publish_is_red():
    rel, pub = _wf()
    steps = pub["jobs"]["pypi"]["steps"]
    proof = next(s for s in steps if "assemble_rtp.py" in str(s.get("run", "")))
    pub_i = next(i for i, s in enumerate(steps) if "gh-action-pypi-publish" in str(s.get("uses", "")))
    steps.remove(proof)
    steps.insert(pub_i, proof)  # move the proof to BEFORE the publish
    assert any("IMMEDIATELY after" in x for x in _pp(rel, pub)), _pp(rel, pub)


# ---- codex astra pass-5: exec-context vectors beyond the literal step keys

def test_expression_valued_job_continue_on_error_is_red():
    rel, pub = _wf()
    pub["jobs"]["pypi"]["continue-on-error"] = "${{ true }}"  # a STRING expression, not Python True
    assert any("continue-on-error" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_workflow_env_shellopts_is_red():
    rel, pub = _wf()
    pub["env"] = {"SHELLOPTS": "noexec"}
    assert any("sets `env`" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_job_env_bash_env_is_red():
    rel, pub = _wf()
    pub["jobs"]["pypi"]["env"] = {"BASH_ENV": "/tmp/x"}
    assert any("sets `env`" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_job_env_pythonpath_is_red():
    # fable A1: the guard is now an ALLOWLIST (forbid ANY env), so PYTHONPATH (sitecustomize no-op) is caught
    rel, pub = _wf()
    pub["jobs"]["pypi"]["env"] = {"PYTHONPATH": "/tmp/x"}
    assert any("sets `env`" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_prior_github_path_write_is_red():
    # fable A2: $GITHUB_PATH shim (a fake python3) is a sibling of the guarded $GITHUB_ENV
    rel, pub = _wf()
    steps = pub["jobs"]["pypi"]["steps"]
    pub_i = next(i for i, s in enumerate(steps) if "gh-action-pypi-publish" in str(s.get("uses", "")))
    steps.insert(pub_i + 1, {"name": "shim", "run": 'echo /tmp/s >> "$GITHUB_PATH"'})
    assert any("GITHUB_ENV/$GITHUB_PATH" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_twine_upload_in_run_is_red():
    # fable A4: a run: uploader escapes "exactly one gh-action-pypi-publish"
    rel, pub = _wf()
    pub["jobs"]["pypi"]["steps"].append({"name": "extra", "run": "pipx run twine upload other/*"})
    assert any("twine/uv" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_proof_artifact_upload_not_always_is_red():
    # fable A5: dropping `if: always()` skips the audit upload exactly on the failed-proof (RED) case
    rel, pub = _wf()
    up = next(s for s in pub["jobs"]["pypi"]["steps"]
              if "upload-artifact" in str(s.get("uses", "")) and "PyPI-registry-proof" in str((s.get("with") or {}).get("path", "")))
    up.pop("if", None)
    assert any("if: always()" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_concurrency_removed_is_red():
    # fable D1: the single-writer concurrency guard is a shipped gate -> lint + pair it
    rel, pub = _wf()
    pub.pop("concurrency", None)
    assert any("concurrency" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_concurrency_cancel_in_progress_true_is_red():
    rel, pub = _wf()
    pub["concurrency"]["cancel-in-progress"] = True
    assert any("cancel-in-progress" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_testpypi_rtp_or_true_is_red():
    # fable C1: the testpypi R-TP assembler is now exact-pinned too
    rel, pub = _wf()
    tp = next(s for s in pub["jobs"]["testpypi"]["steps"] if "assemble_rtp.py" in str(s.get("run", "")))
    tp["run"] = tp["run"].strip() + " || true"
    assert any("testpypi R-TP" in x for x in _pp(rel, pub)), _pp(rel, pub)


# ---- fable D2: the (b)-side assemble_rtp fixes had NO paired controls (host-map / yanked / retry)

import json as _json  # noqa: E402

import assemble_rtp as _a  # noqa: E402


class _FakeResp:
    def __init__(self, body: bytes):
        self._b = body

    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False

    def read(self):
        return self._b


def _urls_resp(urls: list[dict]) -> _FakeResp:
    return _FakeResp(_json.dumps({"urls": urls}).encode())


def test_json_api_base_maps_production_pypi_host():
    assert _a.json_api_base("https://upload.pypi.org/legacy/") == "https://pypi.org"       # prod split
    assert _a.json_api_base("https://test.pypi.org/legacy/") == "https://test.pypi.org"    # testpypi same host


def test_registry_yanked_file_is_not_served(monkeypatch):
    monkeypatch.setattr(_a.urllib.request, "urlopen",
                        lambda req, timeout=60: _urls_resp([{"filename": "w.whl", "digests": {"sha256": "aa"}, "yanked": True}]))
    served = _a.registry_files_via_json("https://pypi.org/legacy/", "0.2.7", attempts=1, expected={"w.whl"})
    assert served == {}, served  # a yanked file is NOT served -> the exact-set proof will RED


def test_registry_retries_until_complete_set(monkeypatch):
    calls = {"n": 0}

    def fake(req, timeout=60):
        calls["n"] += 1
        one = {"filename": "a.whl", "digests": {"sha256": "1"}}
        two = {"filename": "b.whl", "digests": {"sha256": "2"}}
        return _urls_resp([one] if calls["n"] == 1 else [one, two])  # partial first, complete second

    monkeypatch.setattr(_a.urllib.request, "urlopen", fake)
    monkeypatch.setattr(_a.time, "sleep", lambda s: None)
    served = _a.registry_files_via_json("https://pypi.org/legacy/", "0.2.7", expected={"a.whl", "b.whl"}, attempts=5)
    assert served == {"a.whl": "1", "b.whl": "2"} and calls["n"] == 2, (served, calls)


def test_registry_transient_blip_is_retried_not_fatal(monkeypatch):
    calls = {"n": 0}

    def fake(req, timeout=60):
        calls["n"] += 1
        if calls["n"] == 1:
            raise ConnectionResetError("RemoteDisconnected")  # OSError transient blip
        return _urls_resp([{"filename": "a.whl", "digests": {"sha256": "1"}}])

    monkeypatch.setattr(_a.urllib.request, "urlopen", fake)
    monkeypatch.setattr(_a.time, "sleep", lambda s: None)
    served = _a.registry_files_via_json("https://pypi.org/legacy/", "0.2.7", expected={"a.whl"}, attempts=5)
    assert served == {"a.whl": "1"} and calls["n"] == 2, (served, calls)


def test_registry_persistent_transient_raises_after_attempts(monkeypatch):
    # fable S-2: the broadened except must still RAISE after N attempts (not swallow) -> caller records RED
    import pytest
    calls = {"n": 0}

    def fake(req, timeout=60):
        calls["n"] += 1
        raise ConnectionResetError("down")

    monkeypatch.setattr(_a.urllib.request, "urlopen", fake)
    monkeypatch.setattr(_a.time, "sleep", lambda s: None)
    with pytest.raises(OSError):
        _a.registry_files_via_json("https://pypi.org/legacy/", "0.2.7", expected={"a.whl"}, attempts=3)
    assert calls["n"] == 3, calls


def test_registry_403_raises_on_first_attempt(monkeypatch):
    # fable S-2: a non-transient HTTP error fails FAST (no retry)
    import pytest
    calls = {"n": 0}

    def fake(req, timeout=60):
        calls["n"] += 1
        raise _a.urllib.error.HTTPError(req.full_url, 403, "forbidden", {}, None)

    monkeypatch.setattr(_a.urllib.request, "urlopen", fake)
    monkeypatch.setattr(_a.time, "sleep", lambda s: None)
    with pytest.raises(_a.urllib.error.HTTPError):
        _a.registry_files_via_json("https://pypi.org/legacy/", "0.2.7", expected={"a.whl"}, attempts=3)
    assert calls["n"] == 1, calls


def test_testpypi_publish_to_prod_url_is_red():
    # fable B-1: the testpypi job (R-NI-gated, runs first) must NOT be editable to publish to production
    rel, pub = _wf()
    for s in pub["jobs"]["testpypi"]["steps"]:
        if "gh-action-pypi-publish" in str(s.get("uses", "")):
            s.setdefault("with", {})["repository-url"] = "https://upload.pypi.org/legacy/"
    assert any("IRREVERSIBLY publish" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_attestations_dropped_is_red():
    # fable S-1: dropping attestations:false -> PEP 740 sidecars break the exact-set proof on a correct publish
    rel, pub = _wf()
    for s in pub["jobs"]["pypi"]["steps"]:
        if "gh-action-pypi-publish" in str(s.get("uses", "")):
            (s.get("with") or {}).pop("attestations", None)
    assert any("attestations:false" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_neutral_step_between_publish_and_proof_is_red():
    # fable S-4: proof must be IMMEDIATELY after publish -- even a neutral step between is RED
    rel, pub = _wf()
    steps = pub["jobs"]["pypi"]["steps"]
    pub_i = next(i for i, s in enumerate(steps) if "gh-action-pypi-publish" in str(s.get("uses", "")))
    steps.insert(pub_i + 1, {"name": "pause", "run": "sleep 30"})
    assert any("IMMEDIATELY after" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_prior_github_env_write_is_red():
    rel, pub = _wf()
    steps = pub["jobs"]["pypi"]["steps"]
    pub_i = next(i for i, s in enumerate(steps) if "gh-action-pypi-publish" in str(s.get("uses", "")))
    steps.insert(pub_i + 1, {"name": "inject", "run": 'echo SHELLOPTS=noexec >> "$GITHUB_ENV"'})
    assert any("GITHUB_ENV" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_job_defaults_working_directory_is_red():
    rel, pub = _wf()
    pub["jobs"]["pypi"]["defaults"] = {"run": {"working-directory": "/tmp"}}
    assert any("working-directory" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_second_publisher_after_proof_is_red():
    rel, pub = _wf()
    steps = pub["jobs"]["pypi"]["steps"]
    steps.append({"name": "second publish", "uses": "pypa/gh-action-pypi-publish@dc37677b2e1c63e2034f94d8a5b11f265b73ba33",
                  "with": {"packages-dir": "dist/", "skip-existing": True}})
    assert any("EXACTLY ONE" in x or "BEFORE a publish" in x for x in _pp(rel, pub)), _pp(rel, pub)


def test_proof_artifact_upload_removed_is_red():
    rel, pub = _wf()
    steps = pub["jobs"]["pypi"]["steps"]
    pub["jobs"]["pypi"]["steps"] = [s for s in steps if not ("upload-artifact" in str(s.get("uses", ""))
                                                             and "PyPI-registry-proof" in str((s.get("with") or {}).get("path", "")))]
    assert any("audit record" in x for x in _pp(rel, pub)), _pp(rel, pub)
