"""Round-2 release-hardening controls (codex re-review: B4, B6, S9+wrapper, B7, S11, operational).

Every fix ships its PAIRED NEGATIVE CONTROL here (the anti-vacuity paired-control convention): a witness that provably goes RED
when the property is violated, so the class cannot silently return. Offline / injected fakes only;
the live registry / Actions API / npm-pack portions are exercised in CI.
"""
from __future__ import annotations

import copy
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest
import yaml

from conftest import REPO
from pythscribe.runtime import abi

SCRIPTS = REPO / "scripts"
sys.path.insert(0, str(SCRIPTS))
sys.path.insert(0, str(REPO / "tests" / "pythscribe"))
import lint_release_workflows as wl  # noqa: E402
import require_ci_success as rcs  # noqa: E402
import require_evidence as re_  # noqa: E402
import verify_npm_identity as vni  # noqa: E402
from _abi_helpers import module_with_unreachable_start  # noqa: E402
from test_wheel_m4_acceptance import SHA_S, StubAPI, env_run, job as mkjob, run_rec  # noqa: E402
from test_wheel_m6_evidence import (  # noqa: E402
    PYPI_NEED,
    RUN_ID,
    VERSION,
    _all_records,
    _build_run_jobs,
    _promo_run_jobs,
    _tgz,
    assemble as assemble_fixture,
    npm_tarballs,
    promotion_env,
)


class ArtAPI(StubAPI):
    """StubAPI + the B4 (d) immutable-artifact surface. `artifacts` = {run_id: {artifact_name: member_bytes}}
    (the member is always `<record>.json`)."""

    def __init__(self, *a, artifacts=None, **k):
        super().__init__(*a, **k)
        self._by_id: dict[int, bytes] = {}
        self._by_run: dict[int, list[dict]] = {}
        nid = 9000
        for rid, d in (artifacts or {}).items():
            self._by_run[rid] = []
            for nm, b in d.items():
                self._by_id[nid] = b
                self._by_run[rid].append({"id": nid, "name": nm, "expired": False})
                nid += 1

    def list_artifacts(self, run_id: int) -> list[dict]:
        return list(self._by_run.get(run_id, []))

    def read_artifact_member(self, artifact_id: int, member: str) -> bytes:
        return self._by_id[artifact_id]


def _promo_api():
    return StubAPI(runs={RUN_ID: run_rec(RUN_ID), 5005: dict(run_rec(5005), path=".github/workflows/publish-pypi.yml")},
                   jobs={RUN_ID: _build_run_jobs(), 5005: _promo_run_jobs()})


# ============================================================================ B7: WASM ABI byte-walk exact consumption


def _inflate_section(module: bytes, sid: int, extra: bytes) -> bytes:
    """Append `extra` INSIDE the declared payload of the (first) section with id `sid`."""
    data = bytearray(module)
    i = 8
    while i < len(data):
        start = i
        cur = data[i]
        i += 1
        size, i = abi._read_leb128_u32(bytes(data), i)
        end = i + size
        if cur == sid:
            payload = bytes(data[i:end]) + extra
            newsec = bytes([sid]) + abi._encode_leb128_u32(len(payload)) + payload
            return bytes(data[:start]) + newsec + bytes(data[end:])
        i = end
    raise AssertionError(f"no section id {sid}")


def test_b7_ok_module_passes_both_halves():
    ok = module_with_unreachable_start(abi_global="ok")
    assert abi.module_problem(ok) is None
    assert abi.check_abi_global_export(ok) == abi.SUPPORTED_ABI_MAJOR


@pytest.mark.parametrize("sid,label", [(7, "export"), (6, "global")])
def test_b7_trailing_bytes_inside_a_section_are_refused(sid, label):
    """codex B7: the byte-walk never required i == section_end, so trailing garbage inside the export
    (or global) section payload was ACCEPTED by check_module (wasmtime rejected it only later). Now
    check_module refuses it up front."""
    ok = module_with_unreachable_start(abi_global="ok")
    assert abi.module_problem(ok) is None  # the positive twin
    bad = _inflate_section(ok, sid, b"\xff")
    prob = abi.module_problem(bad)
    assert prob is not None and f"{label} section has 1 trailing byte" in prob, prob
    with pytest.raises(ValueError, match="trailing byte"):
        abi.check_module(bad, name="trailing")


def test_b7_duplicate_export_section_is_refused():
    ok = module_with_unreachable_start(abi_global="ok")
    # append a second export section (id 7) -- a last-write-wins parse could smuggle a second global
    data = bytearray(ok)
    i, dup = 8, None
    while i < len(data):
        start = i
        sid = data[i]
        i += 1
        size, i = abi._read_leb128_u32(bytes(data), i)
        end = i + size
        if sid == 7:
            dup = bytes(data[start:end])
            break
        i = end
    assert dup is not None
    twice = bytes(ok) + dup
    with pytest.raises(ValueError, match="duplicate export section"):
        abi.check_abi_global_export(twice)


def test_b7_unsupported_wasm_version_is_refused():
    ok = module_with_unreachable_start(abi_global="ok")
    bad = bytes(ok[:4]) + b"\x02\x00\x00\x00" + bytes(ok[8:])
    with pytest.raises(ValueError, match="unsupported WebAssembly version"):
        abi.check_abi_global_export(bad)


# ============================================================================ S11: promotion evidence-gate discipline


def _wf():
    rel = yaml.safe_load((REPO / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8"))
    pub = yaml.safe_load((REPO / ".github" / "workflows" / "publish-pypi.yml").read_text(encoding="utf-8"))
    return rel, pub


def test_s11_repeated_need_flag_is_refused():
    """codex S11: `--need R-BA R-NI R-TP R-TV --need` (trailing empty) was lint-green while argparse
    used the LAST empty value -> no records required. The linter now rejects a repeated `--need`."""
    rel, pub = _wf()
    assert wl.lint(rel, pub) == []  # the real workflows are clean
    mut = copy.deepcopy(pub)
    for s in mut["jobs"]["pypi"]["steps"]:
        if "require_evidence.py" in str(s.get("run", "")):
            s["run"] = s["run"].replace("--need R-NI R-TP R-TV", "--need R-NI R-TP R-TV --need")
    probs = wl.lint(rel, mut)
    assert any("gate repeats `--need`" in x for x in probs), probs


def test_s11_a_second_unchecked_evidence_step_is_refused():
    """codex S11: a second `require_evidence.py --need ...` step in a promotion job was unchecked."""
    rel, pub = _wf()
    mut = copy.deepcopy(pub)
    steps = mut["jobs"]["testpypi"]["steps"]
    idx = next(i for i, s in enumerate(steps) if "require_evidence.py" in str(s.get("run", "")))
    steps.insert(idx + 1, {"name": "second evidence (unchecked)", "run": "python3 scripts/require_evidence.py --role promotion --manifest release_manifest.json --need --evidence-dir evidence"})
    probs = wl.lint(rel, mut)
    assert any("EXACTLY ONE evidence invocation" in x for x in probs), probs


# ============================================================================ operational: local CI gate is DEFERRED, not red-by-default


def test_operational_local_ci_gate_is_deferred_never_red(monkeypatch):
    """codex re-review (fresh operational): pretag_gate treated a missing GITHUB_REPOSITORY as a
    FAILED mandatory CI gate (red-by-default locally, encouraging bypass). It is now DEFERRED --
    neither a false-GREEN nor a false-RED -- while release.yml still enforces it before any publish."""
    import pretag_gate as pg
    # This asserts the NO-REPO deferral, so the ambient GITHUB_REPOSITORY must be unset -- otherwise
    # inside GitHub Actions the direct gate_ci(post_tag=True) call runs the REAL exact-SHA check at the
    # in-progress HEAD and returns RED (never a successful run at its own commit yet). The CLI subprocess
    # below already scrubs it via `env`; do the same for the in-process calls.
    monkeypatch.delenv("GITHUB_REPOSITORY", raising=False)
    assert pg.gate_ci(post_tag=False) == pg.DEFERRED
    env = {k: v for k, v in os.environ.items() if k != "GITHUB_REPOSITORY"}
    assert pg.gate_ci(post_tag=True) == pg.DEFERRED  # no repo -> still deferred, not RED
    # via the CLI: `--only ci` prints DEFERRED and exits 0 (PARTIAL/DIAGNOSTIC), never RED
    r = subprocess.run([sys.executable, str(SCRIPTS / "pretag_gate.py"), "v0.2.5", "--only", "ci"],
                       capture_output=True, text=True, env=env)
    assert r.returncode == 0 and "[ci] DEFERRED" in r.stdout, (r.stdout, r.stderr)
    assert "[ci] RED" not in r.stdout and "pretag_gate: RED" not in (r.stdout + r.stderr), (r.stdout, r.stderr)


# ============================================================================ B4: producer provenance (attempt, exactly-one, display-name, artifact bytes)


def test_b4_malformed_run_attempt_is_refused(tmp_path):
    """codex B4: `"run_attempt": "garbage"` became None and DISABLED the attempt filter, so any
    successful execution of the named job authenticated the record. Now a non-positive-int attempt is
    refused outright."""
    m, dist, _ = assemble_fixture(tmp_path)
    ev = _all_records(tmp_path / "ok", m, dist)
    assert re_.verify_records(m, ["R-TV"], ev, env=promotion_env(), api=_promo_api()) == []  # positive twin
    rtv = json.loads((ev / "R-TV.json").read_text())
    rtv["producer"]["run_attempt"] = "garbage"
    evf = _all_records(tmp_path / "g", m, dist, **{"R-TV": rtv})
    p = re_.verify_records(m, ["R-TV"], evf, env=promotion_env(), api=_promo_api())
    assert any("run_attempt 'garbage'" in x and "not a positive integer" in x for x in p), p
    rtv["producer"]["run_attempt"] = 0
    evf2 = _all_records(tmp_path / "z", m, dist, **{"R-TV": rtv})
    assert any("not a positive integer" in x for x in re_.verify_records(m, ["R-TV"], evf2, env=promotion_env(), api=_promo_api()))


def test_b4_job_name_collision_is_refused(tmp_path):
    """codex B4: any successful matching job was accepted, so a collided (failed + success) same-name
    pair authenticated the record. Now EXACTLY ONE execution at the record's attempt is required."""
    m, dist, _ = assemble_fixture(tmp_path)
    ev = _all_records(tmp_path / "ok", m, dist)
    collide = StubAPI(
        runs={RUN_ID: run_rec(RUN_ID), 5005: dict(run_rec(5005), path=".github/workflows/publish-pypi.yml")},
        jobs={RUN_ID: _build_run_jobs(),
              5005: [mkjob("testpypi"), mkjob("testpypi-validate"),
                     mkjob("testpypi-evidence", conclusion="failure"),
                     mkjob("testpypi-evidence", conclusion="success")]},  # two same-name at attempt 1
    )
    p = re_.verify_records(m, ["R-TV"], ev, env=promotion_env(), api=collide)
    assert any("EXACTLY ONE execution" in x and "collision" in x for x in p), p


def test_b4_lint_catches_producer_job_name_map_drift():
    """codex B4 (the deadlock cause): the Actions /jobs API returns each job's DISPLAY name, not its
    key. A descriptive `name:` on a producer job would make the API name diverge from
    RECORD_PRODUCER_JOB and REJECT an honest record. The linter now catches the drift."""
    rel, pub = _wf()
    assert wl.lint(rel, pub) == []
    mut = copy.deepcopy(pub)
    mut["jobs"]["testpypi"]["name"] = "Publish the manifest-bound dist to TestPyPI (release validation stage)"
    probs = wl.lint(rel, mut)
    assert any("B4: producer job 'testpypi'" in x and "declares `name:" in x for x in probs), probs
    # and the map must actually agree with the shipped workflow names (no drift on the real files)
    assert re_.RECORD_PRODUCER_JOB == {"R-BA": "node-free-evidence", "R-NI": "npm-identity", "R-TP": "testpypi", "R-TV": "testpypi-evidence"}


def test_b4_record_bytes_bound_to_the_immutable_actions_artifact(tmp_path):
    """codex B4 (d): the record bytes came from a mutable --clobber'd Release asset. verify_records now
    binds them to the producing run's IMMUTABLE Actions artifact `evidence-<name>`. GREEN when the
    bytes match; RED when the on-disk (Release-asset) copy is clobbered, or the artifact is absent."""
    m, dist, _ = assemble_fixture(tmp_path)
    ev = _all_records(tmp_path / "ok", m, dist)
    # B4 (codex pass-3): the immutable artifact name is PER-ATTEMPT (evidence-<name>-<run_attempt>);
    # the fixtures produce records at run_attempt 1.
    arts = {
        RUN_ID: {"evidence-R-BA-1": (ev / "R-BA.json").read_bytes(), "evidence-R-NI-1": (ev / "R-NI.json").read_bytes()},
        5005: {"evidence-R-TP-1": (ev / "R-TP.json").read_bytes(), "evidence-R-TV-1": (ev / "R-TV.json").read_bytes()},
    }
    api = ArtAPI(runs={RUN_ID: run_rec(RUN_ID), 5005: dict(run_rec(5005), path=".github/workflows/publish-pypi.yml")},
                 jobs={RUN_ID: _build_run_jobs(), 5005: _promo_run_jobs()}, artifacts=arts)
    assert re_.verify_records(m, PYPI_NEED, ev, env=promotion_env(), api=api) == []  # bytes match -> GREEN
    # clobber the on-disk Release-asset copy (still a valid record: json.loads is unchanged) -> byte mismatch -> RED
    (ev / "R-BA.json").write_text((ev / "R-BA.json").read_text() + "\n", encoding="utf-8")
    p = re_.verify_records(m, ["R-BA"], ev, env=promotion_env(), api=api)
    assert any("differ from the immutable Actions artifact 'evidence-R-BA-1'" in x for x in p), p
    # a MISSING immutable artifact (only a Release asset exists) -> RED
    arts_missing = {RUN_ID: {"evidence-R-NI-1": (ev / "R-NI.json").read_bytes()},
                    5005: {"evidence-R-TP-1": (ev / "R-TP.json").read_bytes(), "evidence-R-TV-1": (ev / "R-TV.json").read_bytes()}}
    api2 = ArtAPI(runs={RUN_ID: run_rec(RUN_ID), 5005: dict(run_rec(5005), path=".github/workflows/publish-pypi.yml")},
                  jobs={RUN_ID: _build_run_jobs(), 5005: _promo_run_jobs()}, artifacts=arts_missing)
    ev2 = _all_records(tmp_path / "ok2", m, dist)
    p2 = re_.verify_records(m, ["R-BA"], ev2, env=promotion_env(), api=api2)
    assert any("expected EXACTLY ONE immutable Actions artifact 'evidence-R-BA-1'" in x for x in p2), p2


# ============================================================================ S9 + NEW-wrapper: plugin/wrapper content bound to the checkout


def _content_of(tgz: Path) -> dict:
    return {rel: vni.sha256_bytes(b) for rel, b in vni.tarball_members(tgz).items()}


def test_s9_plugin_and_wrapper_content_bound_to_the_checkout(tmp_path):
    """codex S9 + NEW-wrapper (PARTIAL->closed): check_plugin/check_wrapper validated only
    package.json name/version, so stale/malicious bytes at the SAME name@version passed R-NI (the
    immutable-registry hole). The content is now bound to the checkout of S (membership + per-file
    sha256). Here the checkout content is INJECTED (offline; CI passes --checkout)."""
    m, dist, _ = assemble_fixture(tmp_path)
    d = npm_tarballs(tmp_path / "legit", m)
    shutil.copy(dist / "runtime-payload.tgz", d / vni.pack_name("pyths-runtime", VERSION))
    shutil.copy(dist / "scaffolder-payload.tgz", d / vni.pack_name("create-pyths-app", VERSION))
    expected = {pkg: _content_of(d / vni.pack_name(pkg, VERSION)) for pkg in (*re_.NPM_PLUGIN_PACKAGES, re_.NPM_WRAPPER)}
    # positive twin: registry == checkout -> GREEN, content_bound True, consumer accepts
    rec = vni.verify(m, dist, vni.dir_fetcher(d), tmp_path / "w", env_run(RUN_ID), expected_content=expected)
    assert rec["verdict"] == "pass", {k: v["problems"] for k, v in rec["targets"].items() if v["problems"]}
    for pl in re_.NPM_PLUGIN_PACKAGES:
        assert rec["targets"][pl]["plugin"]["content_bound"] is True
    assert rec["targets"][re_.NPM_WRAPPER]["wrapper"]["content_bound"] is True
    assert re_.check_record("R-NI", rec, m, source_sha=SHA_S) == []

    # RED: the registry serves STALE bytes for a plugin at the same version (publish.mjs skipped it)
    d2 = npm_tarballs(tmp_path / "stale", m)
    shutil.copy(dist / "runtime-payload.tgz", d2 / vni.pack_name("pyths-runtime", VERSION))
    shutil.copy(dist / "scaffolder-payload.tgz", d2 / vni.pack_name("create-pyths-app", VERSION))
    _tgz(d2 / vni.pack_name("vite-plugin-pyths", VERSION),
         {"package.json": json.dumps({"name": "vite-plugin-pyths", "version": VERSION}).encode(), "index.js": b"module.exports = require('evil');\n"})
    rec2 = vni.verify(m, dist, vni.dir_fetcher(d2), tmp_path / "w2", env_run(RUN_ID), expected_content=expected)
    assert rec2["targets"]["vite-plugin-pyths"]["verdict"] == "fail"
    assert any("content != `npm pack` of the checkout" in x for x in rec2["targets"]["vite-plugin-pyths"]["problems"]), rec2["targets"]["vite-plugin-pyths"]["problems"]
    # a lying producer (verdict flipped to pass) is caught by the CONSUMER's content-hash re-check
    lie = copy.deepcopy(rec2)
    lie["verdict"] = "pass"
    lie["targets"]["vite-plugin-pyths"]["verdict"] = "pass"
    assert any("content hash" in x for x in re_.check_record("R-NI", lie, m, source_sha=SHA_S))

    # RED: the WRAPPER serves stale launcher bytes at the same version (the NEW-wrapper hole)
    d3 = npm_tarballs(tmp_path / "wrap", m)
    shutil.copy(dist / "runtime-payload.tgz", d3 / vni.pack_name("pyths-runtime", VERSION))
    shutil.copy(dist / "scaffolder-payload.tgz", d3 / vni.pack_name("create-pyths-app", VERSION))
    wpj = {"name": "pythscribe", "version": VERSION, "dependencies": {"pyths-runtime": f"^{VERSION}"},
           "optionalDependencies": {p: VERSION for p in re_.NPM_PLATFORM_TO_TRIPLE}}
    _tgz(d3 / vni.pack_name("pythscribe", VERSION), {"package.json": json.dumps(wpj).encode(), "bin/pyths.js": b"#!/usr/bin/env node\n// STALE launcher glue\n"})
    rec3 = vni.verify(m, dist, vni.dir_fetcher(d3), tmp_path / "w3", env_run(RUN_ID), expected_content=expected)
    assert rec3["targets"]["pythscribe"]["verdict"] == "fail"
    assert any("content != `npm pack` of the checkout" in x for x in rec3["targets"]["pythscribe"]["problems"]), rec3["targets"]["pythscribe"]["problems"]


def test_s9_lint_requires_checkout_binding_in_npm_identity():
    """S9: the linter requires verify_npm_identity.py to run with `--checkout` (the content binding
    cannot be silently dropped). The real workflow is GREEN; removing --checkout is RED."""
    rel, pub = _wf()
    assert wl.lint(rel, pub) == []
    mut = copy.deepcopy(rel)
    for s in mut["jobs"]["npm-identity"]["steps"]:
        if "verify_npm_identity.py" in str(s.get("run", "")):
            s["run"] = s["run"].replace(" --checkout .", "")
    assert any("must run verify_npm_identity.py with a real `--checkout" in x for x in wl.lint(mut, pub))


def test_s9_one_structural_authority_for_the_npm_set():
    """S9: publish set + evidence set derive from ONE file (npm/packages.json), not a regex duplicate."""
    auth = json.loads((REPO / "npm" / "packages.json").read_text(encoding="utf-8"))
    published = set(auth["platform"]) | set(auth["payload"]) | set(auth["plugins"]) | {auth["wrapper"]}
    assert published == set(re_.NPM_PACKAGES)
    assert re_.NPM_PLUGIN_PACKAGES == frozenset(auth["plugins"]) and re_.NPM_WRAPPER == auth["wrapper"]
    assert 'readFileSync(join(__dirname, "packages.json")' in (REPO / "npm" / "publish.mjs").read_text(encoding="utf-8")
