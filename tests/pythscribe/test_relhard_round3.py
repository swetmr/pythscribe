"""Round-3 release-hardening controls (codex PASS-3: B6, B4, S9+wrapper, B7, S11, operational).

The final push to ZERO findings. Every fix ships its PAIRED NEGATIVE CONTROL here (the anti-vacuity paired-control convention):
a witness that provably goes RED when the property is violated, so the CLASS cannot silently return.
Offline / injected fakes only; the live registry / Actions API / npm-pack portions run in CI.
"""
from __future__ import annotations

import copy
import json
import shutil
import sys
from pathlib import Path

import pytest

from conftest import REPO
from pythscribe.runtime import abi

SCRIPTS = REPO / "scripts"
sys.path.insert(0, str(SCRIPTS))
sys.path.insert(0, str(REPO / "tests" / "pythscribe"))
import lint_release_workflows as wl  # noqa: E402
import require_evidence as re_  # noqa: E402
import verify_npm_identity as vni  # noqa: E402
from _abi_helpers import module_with_unreachable_start  # noqa: E402
from test_relhard_round2 import ArtAPI  # noqa: E402
from test_wheel_m4_acceptance import SHA_S, job as mkjob, run_rec  # noqa: E402
from test_wheel_m6_evidence import (  # noqa: E402
    RUN_ID,
    VERSION,
    _all_records,
    _build_run_jobs,
    _promo_run_jobs,
    assemble as assemble_fixture,
    promotion_env,
)


# ============================================================================ B7: duplicate non-custom section CLASS (type/function/start/code + order)


def _append_section_copy(module: bytes, sid: int) -> bytes:
    """Append a byte-identical copy of the module's (first) section with id `sid` -- a duplicate
    non-custom section that a last-write-wins / incomplete parse would accept."""
    data = bytearray(module)
    i = 8
    while i < len(data):
        start = i
        cur = data[i]
        i += 1
        size, i = abi._read_leb128_u32(bytes(data), i)
        end = i + size
        if cur == sid:
            return bytes(module) + bytes(data[start:end])
        i = end
    raise AssertionError(f"no section id {sid} to duplicate")


@pytest.mark.parametrize("sid,label", [(1, "type"), (3, "function"), (8, "start"), (10, "code")])
def test_b7_duplicate_non_custom_section_is_refused(sid, label):
    """codex PASS-3 B7: duplicate type/function/start/code sections passed check_module (wasmtime
    rejected them only later). check_module now refuses EVERY duplicate non-custom section up front."""
    ok = module_with_unreachable_start(abi_global="ok")
    assert abi.module_problem(ok) is None  # the positive twin
    bad = _append_section_copy(ok, sid)
    prob = abi.module_problem(bad)
    assert prob is not None and f"duplicate {label} section" in prob, prob
    with pytest.raises(ValueError, match=f"duplicate {label} section"):
        abi.check_module(bad, name="dup")


def test_b7_out_of_canonical_order_section_is_refused():
    """A type section (id 1) AFTER the code section (id 10) is out of canonical order -> refused."""
    ok = module_with_unreachable_start(abi_global="ok")
    # a fresh, EMPTY type section (0 entries) appended at the end -- id 1 after id 10 == out of order
    empty_type = bytes([1]) + abi._encode_leb128_u32(1) + b"\x00"
    bad = bytes(ok) + empty_type
    prob = abi.module_problem(bad)
    # id 1 already appeared (rank 0) so it trips the duplicate check first -- still RED, which is the point.
    assert prob is not None and ("duplicate type section" in prob or "out of canonical order" in prob), prob


def test_b7_invalid_section_id_is_refused():
    """A section with an id outside the WASM 0-12 range is malformed -> refused (never parsed)."""
    ok = module_with_unreachable_start(abi_global="ok")
    bad = bytes(ok) + bytes([99]) + abi._encode_leb128_u32(1) + b"\x00"  # id 99, 1-byte payload
    prob = abi.module_problem(bad)
    assert prob is not None and "invalid id 99" in prob, prob


def test_b7_real_ok_module_still_loads_after_the_class_fix():
    """Guard: the honest positive twin still passes both halves (no false-RED on canonical modules)."""
    ok = module_with_unreachable_start(abi_global="ok")
    assert abi.module_problem(ok) is None
    assert abi.check_module(ok)["abi"] == abi.SUPPORTED_ABI_MAJOR


# ============================================================================ B4: strict-int spoofing + happy-path rerun


def _promo_api(jobs5005=None):
    return ArtAPI(runs={RUN_ID: run_rec(RUN_ID), 5005: dict(run_rec(5005), path=".github/workflows/publish-pypi.yml")},
                  jobs={RUN_ID: _build_run_jobs(), 5005: jobs5005 or _promo_run_jobs()},
                  artifacts={RUN_ID: {}, 5005: {}})


@pytest.mark.parametrize("bad_attempt", [True, 1.5, "1", 0, -1])
def test_b4_non_strict_int_run_attempt_is_refused(tmp_path, bad_attempt):
    """codex pass-3 B4(a): `_int` let true / 1.5 / "1" all become attempt 1 (attempt-1 spoofing).
    A record producer.run_attempt that is not a STRICT positive int is now refused (no artifact/job
    even consulted)."""
    m, dist, _ = assemble_fixture(tmp_path)
    rtv = json.loads((_all_records(tmp_path / "ok", m, dist) / "R-TV.json").read_text())
    rtv["producer"]["run_attempt"] = bad_attempt
    evf = _all_records(tmp_path / f"b{str(bad_attempt)}", m, dist, **{"R-TV": rtv})
    p = re_.verify_records(m, ["R-TV"], evf, env=promotion_env(), api=_promo_api())
    assert any("not a positive integer" in x for x in p), (bad_attempt, p)


@pytest.mark.parametrize("field,bad", [("run_id", "5005"), ("run_id", 1.0), ("run_id", True)])
def test_b4_non_strict_int_run_id_is_refused(tmp_path, field, bad):
    """B4 sibling: producer.run_id is also a forgeable JSON field -> strict positive int."""
    m, dist, _ = assemble_fixture(tmp_path)
    rtv = json.loads((_all_records(tmp_path / "ok", m, dist) / "R-TV.json").read_text())
    rtv["producer"][field] = bad
    evf = _all_records(tmp_path / f"r{str(bad)}", m, dist, **{"R-TV": rtv})
    p = re_.verify_records(m, ["R-TV"], evf, env=promotion_env(), api=_promo_api())
    assert any("run_id" in x and "not a positive integer" in x for x in p), (bad, p)


@pytest.mark.parametrize("bad", [True, 1.5, "1"])
def test_b4_manifest_strict_int_fields(bad):
    """B4 sibling: manifest build_run_id/build_run_attempt are strict positive ints (bool/float/str refused)."""
    from test_wheel_m4_acceptance import make_manifest
    m = make_manifest()
    m2 = dict(m, build_run_attempt=bad)  # self-hash now stale, but the strict-int check fires regardless
    assert any("strict positive integers" in x for x in re_.check_manifest_shape(m2)), (bad, re_.check_manifest_shape(m2))


def test_b4_honest_attempt2_rerun_is_accepted(tmp_path):
    """codex pass-3 B4(b) THE happy-path control: after a rerun, GitHub keeps BOTH `evidence-R-TV-1`
    and `evidence-R-TV-2`. The old 'exactly one evidence-R-TV across the run' check FALSELY REJECTED a
    legitimate attempt-2 recovery. The per-attempt binding now ACCEPTS it -- and a forged record still
    fails."""
    m, dist, _ = assemble_fixture(tmp_path)
    rtv = json.loads((_all_records(tmp_path / "ok", m, dist) / "R-TV.json").read_text())
    rtv["producer"]["run_attempt"] = 2
    evf = _all_records(tmp_path / "a2", m, dist, **{"R-TV": rtv})
    good_bytes = (evf / "R-TV.json").read_bytes()
    jobs5005 = [mkjob("testpypi"), mkjob("testpypi-validate"),
                mkjob("testpypi-evidence", attempt=1, conclusion="failure"),
                mkjob("testpypi-evidence", attempt=2, conclusion="success")]
    # BOTH attempts' artifacts present; -1 carries DIFFERENT bytes to prove the record binds to ITS -2
    api = ArtAPI(runs={5005: dict(run_rec(5005), path=".github/workflows/publish-pypi.yml")},
                 jobs={5005: jobs5005},
                 artifacts={5005: {"evidence-R-TV-1": b"attempt-1 stale record bytes", "evidence-R-TV-2": good_bytes}})
    assert re_.verify_records(m, ["R-TV"], evf, env=promotion_env(), api=api) == []  # rerun ACCEPTED
    # a forged record: attempt-2 record whose bytes differ from the -2 artifact -> RED
    (evf / "R-TV.json").write_text((evf / "R-TV.json").read_text() + "\n", encoding="utf-8")
    p = re_.verify_records(m, ["R-TV"], evf, env=promotion_env(), api=api)
    assert any("differ from the immutable Actions artifact 'evidence-R-TV-2'" in x for x in p), p
    # and the attempt-2 record with ONLY the attempt-1 artifact present -> RED (its own attempt's artifact absent)
    api2 = ArtAPI(runs={5005: dict(run_rec(5005), path=".github/workflows/publish-pypi.yml")},
                  jobs={5005: jobs5005}, artifacts={5005: {"evidence-R-TV-1": b"attempt-1 bytes"}})
    evf2 = _all_records(tmp_path / "a2b", m, dist, **{"R-TV": rtv})
    p2 = re_.verify_records(m, ["R-TV"], evf2, env=promotion_env(), api=api2)
    assert any("expected EXACTLY ONE immutable Actions artifact 'evidence-R-TV-2'" in x for x in p2), p2


# ============================================================================ S9: content_bound vacuity + lint comment/authority


def _wf():
    import yaml
    rel = yaml.safe_load((REPO / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8"))
    pub = yaml.safe_load((REPO / ".github" / "workflows" / "publish-pypi.yml").read_text(encoding="utf-8"))
    return rel, pub


def test_s9_check_record_refuses_content_bound_false(tmp_path):
    """codex pass-3 S9(a): an R-NI whose plugin/wrapper blocks say `content_bound: false` for every
    package was ACCEPTED (check_record only compared hashes when content_bound was truthy -- a vacuous
    gate). Now content_bound MUST be exactly True with both hashes present + equal, else RED."""
    from test_wheel_m6_evidence import _expected_content, assemble as _assemble, npm_tarballs
    m, dist, _ = _assemble(tmp_path)
    d = npm_tarballs(tmp_path, m)
    shutil.copy(dist / "runtime-payload.tgz", d / vni.pack_name("pyths-runtime", VERSION))
    shutil.copy(dist / "scaffolder-payload.tgz", d / vni.pack_name("create-pyths-app", VERSION))
    # a producer that DID NOT bind the content (no --checkout / no injection) -> content_bound false
    rec_unbound = vni.verify(m, dist, vni.dir_fetcher(d), tmp_path / "wu", env_run_local())
    for pkg in (*re_.NPM_PLUGIN_PACKAGES, re_.NPM_WRAPPER):
        blk = rec_unbound["targets"][pkg].get("plugin") or rec_unbound["targets"][pkg].get("wrapper")
        assert blk["content_bound"] is False
    probs = re_.check_record("R-NI", rec_unbound, m, source_sha=SHA_S)
    assert any("content_bound != true" in x for x in probs), probs
    # the BOUND producer (injected checkout content) -> content_bound true -> check_record clean
    rec_bound = vni.verify(m, dist, vni.dir_fetcher(d), tmp_path / "wb", env_run_local(), expected_content=_expected_content(d, m))
    assert re_.check_record("R-NI", rec_bound, m, source_sha=SHA_S) == []
    # a lying producer: content_bound true but the two hashes differ -> RED
    lie = copy.deepcopy(rec_bound)
    blk = lie["targets"][sorted(re_.NPM_PLUGIN_PACKAGES)[0]]["plugin"]
    blk["registry_files_sha256"] = "deadbeef"
    assert any("!= checkout content hash" in x for x in re_.check_record("R-NI", lie, m, source_sha=SHA_S))
    # content_bound true but a hash MISSING -> RED (incomplete binding)
    lie2 = copy.deepcopy(rec_bound)
    lie2["targets"][re_.NPM_WRAPPER]["wrapper"].pop("checkout_files_sha256", None)
    assert any("content binding incomplete" in x for x in re_.check_record("R-NI", lie2, m, source_sha=SHA_S))


def env_run_local() -> dict:
    return {"GITHUB_SHA": SHA_S, "GITHUB_RUN_ID": str(RUN_ID), "GITHUB_RUN_ATTEMPT": "1", "GITHUB_REPOSITORY": "swetmr/pythscribe"}


def test_s9_lint_rejects_commented_checkout():
    """codex pass-3 S9(b): a substring lint passed `# --checkout .` (a comment) while the runtime
    skipped binding. The structural parser strips comments first, so a commented flag no longer counts
    AND the comment itself is flagged."""
    rel, pub = _wf()
    assert wl.lint(rel, pub) == []
    mut = copy.deepcopy(rel)
    for s in mut["jobs"]["npm-identity"]["steps"]:
        if "verify_npm_identity.py" in str(s.get("run", "")):
            s["run"] = s["run"].replace(" --checkout .", " # --checkout .")
    probs = wl.lint(mut, pub)
    assert any("real `--checkout" in x for x in probs), probs
    assert any("contains a `#` comment" in x for x in probs), probs


def test_s9_one_authority_for_plugin_wrapper_dirs():
    """codex pass-3 S9(c): the plugin/wrapper DIRECTORIES derive from the ONE authority packages.json
    (no duplicate literal in verify_npm_identity.py)."""
    auth = json.loads((REPO / "npm" / "packages.json").read_text(encoding="utf-8"))
    assert re_.NPM_PLUGIN_WRAPPER_DIRS == {**auth["plugins"], auth["wrapper"]: f"npm/{auth['wrapper']}"}
    assert vni.PLUGIN_WRAPPER_DIRS == re_.NPM_PLUGIN_WRAPPER_DIRS  # verify_npm_identity uses the authority, not a copy
    src = (REPO / "scripts" / "verify_npm_identity.py").read_text(encoding="utf-8")
    assert "packages/vite-plugin-pyths" not in src and "packages/next-plugin-pyths" not in src  # the literal is gone


# ============================================================================ S11: same-step `|| require_evidence` bypass


def test_s11_same_step_or_bypass_is_refused():
    """codex pass-3 S11: `require_evidence --need ... || require_evidence` (second, no --need) is ONE
    step with ONE --need, so the old checks passed while the shell ran an unchecked second gate. The
    structural parser rejects the shell operator and the second invocation."""
    rel, pub = _wf()
    assert wl.lint(rel, pub) == []
    mut = copy.deepcopy(pub)
    for s in mut["jobs"]["pypi"]["steps"]:
        if "require_evidence.py" in str(s.get("run", "")):
            s["run"] = s["run"] + " || python3 scripts/require_evidence.py --role promotion --manifest release_manifest.json --evidence-dir evidence"
    probs = wl.lint(rel, mut)
    assert any("EXACTLY ONE" in x and "invocation" in x for x in probs), probs
    assert any("shell control" in x for x in probs), probs


def test_s11_trailing_comment_hiding_a_flag_is_refused():
    """codex pass-3 S11: a trailing `# ...` comment in the gate step is rejected (it could hide/alter a flag)."""
    rel, pub = _wf()
    mut = copy.deepcopy(pub)
    for s in mut["jobs"]["testpypi"]["steps"]:
        if "require_evidence.py" in str(s.get("run", "")):
            s["run"] = s["run"] + "  # --need can be anything below"
    assert any("contains a `#` comment" in x for x in wl.lint(rel, mut))


# ============================================================================ operational: DEFERRED mandatory gate is NOT release-authorizing


def test_operational_full_run_with_ci_deferred_is_not_authorizing_green():
    """codex pass-3 operational: a FULL pretag run with CI deferred still printed
    `GREEN (release-authorizing)`, contradicting the DEFERRED sentinel. It now reports
    PARTIAL/READY-EXCEPT-DEFERRED (a clearly NON-authorizing status); GREEN (release-authorizing) is
    reserved for a full run in which EVERY mandatory gate actually ran."""
    import pretag_gate as pg
    selected = list(pg.gates("v0.2.5", post_tag=False, skip_wheel_build=True))
    # full run, ci deferred -> NON-authorizing
    msg = pg.final_status("v0.2.5", selected, is_full=True, deferred=["ci"])
    assert "PARTIAL/READY-EXCEPT-DEFERRED" in msg and "GREEN (release-authorizing)" not in msg, msg
    # full run, NOTHING deferred -> the authorizing GREEN
    msg2 = pg.final_status("v0.2.5", selected, is_full=True, deferred=[])
    assert "GREEN (release-authorizing)" in msg2, msg2
    # a subset -> DIAGNOSTIC (never authorizing), unchanged
    msg3 = pg.final_status("v0.2.5", ["workflows"], is_full=False, deferred=[])
    assert "PARTIAL/DIAGNOSTIC" in msg3 and "GREEN (release-authorizing)" not in msg3, msg3
