"""The full-features notebook's numbers are COMPUTED, not constants (spec §validation row 5):
  N1 `summarize` is a function of the records (a changed input changes the table; a missing
     record is refused; a false claim flips its row's pass);
  N2 the committed `full_features_summary.json` is exactly what `summarize` derives from the
     committed `full_features_records.json` -- the table cannot have been typed in -- and
     every measured row passed when it was committed;
  N3 (gated: wasmtime + node + gradio + playwright + nbclient) `full_features.ipynb` executes
     end-to-end from a clean copy with NB_FAST=1 and its final cell prints the derivation
     control; N4 the same for `three_use_cases.ipynb`.
"""
from __future__ import annotations

import copy
import json
import os
import shutil
import sys
from pathlib import Path

import pytest

from conftest import REPO, gate, gate_import, gate_node, soft_perf

UC = REPO / "examples" / "wasm-use-cases"
sys.path.insert(0, str(UC))
import features_lib as F  # noqa: E402


def _records() -> dict:
    p = UC / "full_features_records.json"
    gate(p.is_file(), "no committed notebook evidence (run full_features.ipynb)")
    return json.loads(p.read_text(encoding="utf-8"))


def test_n1_summary_is_a_function_of_records():
    rec = _records()
    s = F.summarize(rec)
    assert s["all_pass"] and s["n_measured"] >= 10
    # a changed input changes the table
    m = copy.deepcopy(rec)
    m["speedup"]["wasm_s"] *= 0.5
    assert F.format_table(F.summarize(m)) != F.format_table(s)
    # a false claim flips its row (the pass flags are derived, not typed)
    m = copy.deepcopy(rec)
    m["speedup"]["all_equal"] = False
    assert F.summarize(m)["rows"][0]["pass"] is False and F.summarize(m)["all_pass"] is False
    m = copy.deepcopy(rec)
    m["sandbox"]["contained"] = False
    assert F.summarize(m)["rows"][1]["pass"] is False
    m = copy.deepcopy(rec)
    m["fanout"]["wasm_scaling"] = 1.0
    assert F.summarize(m)["rows"][2]["pass"] is False
    m = copy.deepcopy(rec)
    m["where_it_loses"]["vectorised"]["numpy_wins"] = False
    assert [r for r in F.summarize(m)["rows"] if "already-vectorised" in r["row"]][0]["pass"] is False
    # a missing record is refused, never silently n/a
    m = copy.deepcopy(rec)
    del m["determinism"]
    with pytest.raises(ValueError, match="record 'determinism' missing"):
        F.summarize(m)
    # a row whose measured arm did not run is marked NOT RUN / NOT MEASURABLE -- never passed (opus m1.5 r1/S5, S8)
    m = copy.deepcopy(rec)
    m["isomorphic"] = None
    s2 = F.summarize(m)
    iso_rows = [r for r in s2["rows"] if r["row"].startswith("Isomorphic")]
    assert iso_rows[0]["pass"] is None and "NOT RUN" in iso_rows[0]["measured"]

    def row(summary, prefix):
        return [r for r in summary["rows"] if r["row"].startswith(prefix)][0]

    m = copy.deepcopy(rec)
    m["determinism"]["equal_node_v8"] = None
    m["determinism"]["node_v8_digest"] = None
    assert row(F.summarize(m), "Bit-for-bit")["pass"] is None and "NOT RUN" in row(F.summarize(m), "Bit-for-bit")["measured"]
    m = copy.deepcopy(rec)
    m["redaction"]["browser_shim_digest"] = None
    m["redaction"]["masked_browser_shim"] = None
    assert row(F.summarize(m), "On-device")["pass"] is None
    m = copy.deepcopy(rec)
    m["fanout"]["cpu_count"] = 2
    assert row(F.summarize(m), "GIL-free")["pass"] is None and "NOT MEASURABLE" in row(F.summarize(m), "GIL-free")["measured"]
    # the independent references and the server arm are part of the pass (opus m1.5 r1/B3, S6)
    m = copy.deepcopy(rec)
    m["redaction"]["matches_regex_reference"] = False
    assert row(F.summarize(m), "On-device")["pass"] is False
    m = copy.deepcopy(rec)
    m["where_it_loses"]["vectorised"]["values_close"] = False
    assert row(F.summarize(m), "WHERE IT LOSES: already")["pass"] is False
    m = copy.deepcopy(rec)
    m["isomorphic"]["mode"] = "browser"
    m["isomorphic"]["server_bits"] = None
    assert row(F.summarize(m), "Isomorphic")["pass"] is False
    m = copy.deepcopy(rec)
    m["isomorphic"]["server_bits"] = "0000000000000000"  # server arm ran but disagreed: identical must not carry the row
    assert row(F.summarize(m), "Isomorphic")["pass"] is False


def test_n2_committed_summary_is_derived_from_committed_records():
    rec = _records()
    p = UC / "full_features_summary.json"
    gate(p.is_file(), "no committed summary")
    committed = json.loads(p.read_text(encoding="utf-8"))
    derived = F.summarize(rec)
    assert derived == committed, "full_features_summary.json is not what summarize() derives from full_features_records.json"
    assert committed["all_pass"] is True
    assert committed["n_measured"] == committed["n_rows"], "every row was measured when the evidence was committed (incl. the real-tab isomorphic row)"
    assert rec["isomorphic"]["identical"] is True and rec["isomorphic"]["path"] == "browser-wasm"
    assert rec["sandbox"]["contained"] is True
    assert rec["where_it_loses"]["vectorised"]["numpy_wins"] is True  # correctness: NumPy wins the vectorised case
    # speedup + fanout scaling are TIMING numbers -> non-blocking (byte-identical wheels; loaded-runner noise)
    soft_perf(rec["speedup"]["ratio"] > 1, f"@wasm speedup ratio {rec['speedup']['ratio']:.2f}x (expected > 1)")
    soft_perf(rec["fanout"]["wasm_scaling"] > rec["fanout"]["cpython_scaling"],
              f"fanout wasm_scaling {rec['fanout']['wasm_scaling']:.2f} !> cpython_scaling {rec['fanout']['cpython_scaling']:.2f}")


def _run_notebook(name: str, tmp_path, needles: list[str], produces: str):
    gate_import("wasmtime")
    gate_node()
    gate_import("playwright.sync_api")
    gate_import("gradio")
    nbformat = gate_import("nbformat")
    nbclient = gate_import("nbclient")
    gate_import("ipykernel")
    gate((UC / "__pythscribe__" / "dtw_distance" / "manifest.json").is_file(), "use-case artifacts not built")
    work = tmp_path / "nb"

    def ignore(d, names):  # the committed EVIDENCE is left behind (the notebook must re-derive it); artifact manifests are kept
        return {n for n in names if n in ("__pycache__", "_app.log") or n.startswith("_") and n.endswith("_work")
                or (Path(d).resolve() == UC.resolve() and n.endswith((".json",)))}

    shutil.copytree(UC, work, ignore=ignore)
    assert not list(work.glob("*.json")) and (work / "__pythscribe__" / "dtw_distance" / "manifest.json").is_file()
    nb = nbformat.read(work / name, as_version=4)
    env = dict(os.environ)
    os.environ["NB_FAST"] = "1"
    os.environ["PYTHONUTF8"] = "1"
    try:
        client = nbclient.NotebookClient(nb, timeout=900, kernel_name="python3", resources={"metadata": {"path": str(work)}})
        client.execute()
    finally:
        os.environ.clear()
        os.environ.update(env)
    text = "\n".join(
        "".join(o.get("text", "") or "".join(o.get("data", {}).get("text/markdown", "")) or "".join(o.get("data", {}).get("text/plain", "")))
        for c in nb.cells if c.cell_type == "code" for o in c.get("outputs", [])
    )
    for n in needles:
        assert n in text, n
    assert (work / produces).is_file()
    return text, work


def test_n3_full_features_notebook_executes_from_a_clean_copy(tmp_path):
    text, work = _run_notebook("full_features.ipynb", tmp_path, ["computed, not constants", "contained = True", "all measured rows pass: True", "missing record refused"], "full_features_summary.json")
    s = json.loads((work / "full_features_summary.json").read_text(encoding="utf-8"))
    assert s["all_pass"] and s["n_measured"] == s["n_rows"]
    assert "NOT RUN" not in text


def test_n4_three_use_cases_notebook_executes_from_a_clean_copy(tmp_path):
    text, work = _run_notebook("three_use_cases.ipynb", tmp_path, ["computed, not constants", "contained: True", "identical: True"], "three_use_cases_summary.json")
    s = json.loads((work / "three_use_cases_summary.json").read_text(encoding="utf-8"))["summary"]
    assert s["sandbox_contained"] and s["isomorphic_identical"] and s["determinism_three_engines"]  # correctness
    soft_perf(s["speedup_ratio"] > 1, f"three-use-cases speedup_ratio {s['speedup_ratio']:.2f}x (expected > 1)")  # timing
