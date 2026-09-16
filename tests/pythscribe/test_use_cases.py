"""M1.5 §B -- the three lead use cases, as gates (examples/wasm-use-cases):

  U1  non-vectorisable decoding: edit_distance / viterbi on the server == CPython == the
      independent reference; the speedup record is measured, > 1x, and honest about its baseline
  U2  sandboxed (LLM-generated) code: `measure_sandbox` reports contained=True, and each of
      its RED halves individually (the I/O module really writes under a WASI linker; the loop
      really runs unmetered until killed; open() never builds)
  U3  isomorphic (browser + server), REAL TAB (gradio + playwright gated): the tab's bits ==
      the in-process bits == CPython; server-side counters 0; the .wasm was fetched by the tab;
      PAIRED RED CONTROL: a re-signed MUTANT artifact (kernel without abs) served to the same
      app makes `identical` False -- the cross-path differential catches a seeded mutation
  U4  redaction / fallback / single-artifact / where-it-loses records hold (their `ok` flags
      are derived, and each has a sub-assertion that would go RED on the obvious bypass)
"""
from __future__ import annotations

import json
import shutil
import sys
from pathlib import Path

import pytest

from conftest import REPO, gate, gate_import, gate_node, import_module_from
from pythscribe import binding_of
from pythscribe.artifacts import MANIFEST_NAME, manifest_self_hash, verify
from pythscribe.build import build_module

UC = REPO / "examples" / "wasm-use-cases"
sys.path.insert(0, str(UC))
import features_lib as F  # noqa: E402

gate_import("wasmtime")


@pytest.fixture(scope="module")
def K():
    mod = import_module_from(UC / "kernels.py", "kernels_uc_for_use_case_tests")
    b = binding_of(mod.edit_distance)
    gate(b.artifact is not None, f"use-case artifacts not built ({b.artifact_status}); run `python -m pythscribe.build examples/wasm-use-cases/kernels.py`")
    assert b.mode == "server", (b.mode, b.mode_reason)
    return mod


# ----------------------------------------------------------------------------- U1
def test_u1_decoders_agree_and_speedup_is_measured(K):
    rec = F.measure_speedup(K, n=150, seed=7, repeats=2)
    assert rec["all_equal"] and rec["wasm_value"] == rec["reference"]  # correctness: a hard gate
    assert rec["wasm_s"] > 0 and rec["cpython_s"] > 0 and rec["ratio"] == rec["cpython_s"] / rec["wasm_s"]  # derived, not typed
    if rec["ratio"] <= 1.0:  # performance is reported, not gated (codex m1.5 r2/#6): a throttled runner is not a wrong answer
        import warnings

        warnings.warn(f"no speedup measured on this machine: ratio {rec['ratio']:.2f}x (wasm {rec['wasm_s']*1e3:.1f} ms, cpython {rec['cpython_s']*1e3:.1f} ms)")
    assert "CPython" in rec["baseline"] and "NOT NumPy" in rec["baseline"]
    assert rec["host_imports"] == []  # pure WASM
    inp = F._viterbi_inputs(5, 60, 3)
    bw, pw = F.viterbi_call(K.viterbi, inp)
    bp, pp = F.viterbi_call(binding_of(K.viterbi).run_python, inp)
    assert pw == pp and F.digest(bw, pw) == F.digest(bp, pp)
    assert len(set(pw)) > 1  # a real path, not a constant


# ----------------------------------------------------------------------------- U2
def test_u2_sandbox_contains_and_every_red_half_is_real(K, tmp_path):
    rec = F.measure_sandbox(K, fuel=10_000_000, workdir=tmp_path / "sb")
    assert rec["contained"], rec
    assert rec["snippet"]["equal"] and rec["snippet"]["fuel_used"] > 1000 and rec["snippet"]["host_imports"] == []
    assert rec["fuel_trap"]["trapped"] and rec["fuel_trap"]["elapsed_s"] < 5 and rec["fuel_trap"]["finite_call_fuel_used"] > 0
    assert rec["fuel_trap"]["red_control"]["still_running_at_kill"] and "interrupted" in rec["fuel_trap"]["red_control"]["outcome"]
    assert rec["io_attempt"]["refused"] and "wasi_snapshot_preview1" in rec["io_attempt"]["error"]
    assert rec["io_attempt"]["red_control"]["wrote_file_under_wasi_linker"] and (tmp_path / "sb" / "escaped.txt").read_text() == "pwned\n"
    assert rec["open_in_kernel"]["refused_at_build"] and not rec["open_in_kernel"]["artifact_exists"]
    # the `contained` flag is a conjunction: knock out any one leg and it must drop (derived, not typed)
    import copy

    for path in (("snippet", "equal"), ("fuel_trap", "trapped"), ("fuel_trap", "red_control", "still_running_at_kill"),
                 ("io_attempt", "refused"), ("io_attempt", "red_control", "wrote_file_under_wasi_linker"), ("open_in_kernel", "refused_at_build")):
        m = copy.deepcopy(rec)
        d = m
        for k in path[:-1]:
            d = d[k]
        d[path[-1]] = False
        # the REAL predicate (features_lib.sandbox_contained), not a copy of it (opus m1.5 r1/S7)
        assert F.sandbox_contained(m) is False, path
    m = copy.deepcopy(rec)
    m["open_in_kernel"]["artifact_exists"] = True  # the seventh leg (codex m1.5 r2/#4): a refusal that still left an artifact
    assert F.sandbox_contained(m) is False
    assert F.sandbox_contained(rec) is True


# ----------------------------------------------------------------------------- U3 (real tab)
A = [0.0, 1.5, 2.25, 3.0, 2.5, 1.0, -0.5, 0.1]
B = [0.0, 0.2, 1.0, 2.0, 3.1, 3.0, 2.0, 1.0, 0.0, -0.1]


@pytest.fixture(scope="module")
def browser_stack():
    gate_node()
    gate_import("gradio")
    gate_import("gradio_wasmfunction")
    gate_import("playwright.sync_api")


def test_u3_isomorphic_real_tab_bits_identical(K, browser_stack):
    import iso_drive as drive

    rec = drive.measure_isomorphic(A, B, app_dir=UC)
    assert rec["console_errors"] == [], rec
    assert rec["path"] == "browser-wasm" and rec["wasm_fetched"], rec
    assert rec["mode"] == "server"
    assert rec["browser_bits"] == rec["server_bits"] == rec["cpython_bits"] and rec["identical"], rec
    assert rec["python_calls"] == 0 and rec["server_calls"] == 0, rec  # the tab did the work
    # and the bits are THIS input's DTW distance, not a constant
    from pythscribe.build.runner import float_bits

    assert rec["cpython_bits"] == float_bits(F.dtw_ref(A, B))


def test_u3b_red_control_mutant_wasm_in_the_tab_is_caught(K, browser_stack, tmp_path):
    """Serve a RE-SIGNED mutant artifact (dtw without the abs) to a copy of the app: the tab
    and the in-process path both run the mutant, CPython does not -> identical=False. The
    cross-path differential sees a seeded mutation; a demo that only compared browser==server
    would have passed (both mutant) -- the CPython arm is load-bearing."""
    import iso_drive as drive

    app_dir = tmp_path / "uc-mutant"
    app_dir.mkdir()
    for f in ("app.py", "kernels.py", "iso_drive.py"):
        shutil.copyfile(UC / f, app_dir / f)
    shutil.copytree(UC / "__pythscribe__", app_dir / "__pythscribe__")
    # the mutant: drop the abs -> negative differences reduce the distance
    src = (UC / "kernels.py").read_text(encoding="utf-8")
    mutated = src.replace("            if d < 0.0:\n                d = -d\n", "            if d < 0.0:\n                d = d\n")
    assert mutated != src
    mdir = tmp_path / "mutant-src"
    mdir.mkdir()
    (mdir / "kernels.py").write_text(mutated, encoding="utf-8")
    [art] = [a for a in build_module(mdir / "kernels.py", quiet=True) if a.function == "dtw_distance"]
    target = app_dir / "__pythscribe__" / "dtw_distance"
    shutil.rmtree(target)
    shutil.copytree(art.dir, target)
    m = json.loads((target / MANIFEST_NAME).read_text(encoding="utf-8"))
    m["source_sha256"] = binding_of(K.dtw_distance).source_sha256  # re-sign as if built from the UNmutated source
    m["manifest_sha256"] = manifest_self_hash(m)
    (target / MANIFEST_NAME).write_text(json.dumps(m, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    verify(target, function="dtw_distance", expected_source_sha256=binding_of(K.dtw_distance).source_sha256)
    rec = drive.measure_isomorphic(A, B, app_dir=app_dir)
    assert rec["path"] == "browser-wasm" and rec["wasm_fetched"], rec
    assert rec["browser_bits"] == rec["server_bits"], rec  # both ran the mutant .wasm
    assert rec["browser_bits"] != rec["cpython_bits"] and rec["identical"] is False, rec  # RED as required


# ----------------------------------------------------------------------------- U4
def test_u4_redaction_identical_across_channels_and_matches_regex(K):
    gate_node()
    rec = F.measure_redaction_via_browser_shim(K, seed=1, n=2000, min_run=4)
    assert rec["all_identical"] and rec["matches_regex_reference"] and rec["masked_server"] > 0
    assert rec["masked_browser_shim"] == rec["masked_server"] == rec["masked_cpython"]
    assert "*" in rec["sample_after"] and rec["sample_before"] != rec["sample_after"]
    # short runs are NOT masked (the min_run clause is real): with min_run=1 more is masked
    rec1 = F.measure_redaction_via_browser_shim(K, seed=1, n=2000, min_run=1)
    assert rec1["masked_server"] > rec["masked_server"] and rec1["matches_regex_reference"]


def test_u4_fallback_single_artifact_and_where_it_loses(K, tmp_path):
    fb = F.measure_fallback(tmp_path / "fb")
    assert fb["ok"] and fb["mode"] == "fallback" and fb["python_calls"] == 1 and fb["server_calls"] == 0
    sa = F.measure_single_artifact(K, tmp_path / "single")
    assert sa["ok"] and sa["files_needed"] == 1 and not sa["compiler_reachable"] and sa["wasm_bytes"] < 4096
    assert sorted(p.name for p in (tmp_path / "single").iterdir()) == ["edit_distance.wasm"]
    wl = F.measure_where_it_loses(K, n=100_000, per_element_n=3_000, seed=0, repeats=2)
    assert wl["vectorised"]["numpy_wins"] and wl["vectorised"]["values_close"]
    assert wl["per_element"]["batching_wins"] and wl["per_element"]["equal"] and wl["per_element"]["per_element_over_batched_x"] > 5


def test_u4_m1_preprocessing_evidence_is_reread_not_restated():
    ev = F.m1_preprocessing_evidence()
    gate(ev is not None, "M1 metrics_summary.json not committed")
    committed = json.loads((REPO / "examples" / "gradio-image-preprocess" / "metrics_summary.json").read_text(encoding="utf-8"))
    assert ev["aggregate_reduction_x"] == committed["aggregate"]["reduction_x"] and ev["images"] == [r["image"] for r in committed["rows"]]
