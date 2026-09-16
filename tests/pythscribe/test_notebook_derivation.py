"""The reproducibility notebook's numbers are COMPUTED, not constants:
  N1 `summarize` is a function of the records (different inputs -> different table; swapping
     the roles inverts the ratio; a missing measurement is refused);
  N2 the committed evidence (`metrics_summary.json`) is exactly what `summarize` derives from
     the committed `metrics_records.json` -- the table cannot have been typed in;
  N3 (browser + node gated) the notebook executes end-to-end from a clean copy and its table
     names every committed image with a measured, >5x reduction.
"""
from __future__ import annotations

import copy
import json
import os
import shutil
import sys

import pytest

from conftest import REPO, gate, gate_import, gate_node

IMG_DIR = REPO / "examples" / "gradio-image-preprocess"
sys.path.insert(0, str(IMG_DIR))
import metrics_lib as M  # noqa: E402


def _rec(image: str, mode: str, upload_bytes: int, **extra) -> dict:
    """A synthetic SERVER record in the app's shape (top level = server-derived; `client` =
    the browser's own story)."""
    base = {
        "image": image, "mode": mode, "upload_bytes": upload_bytes, "upload_measured": True, "file_bytes": upload_bytes - 200,
        "join_bytes": 500, "wire_bytes": upload_bytes + 500, "wire_measured": True, "out_w": 400, "out_h": 300,
        "server_resize_ms": 5.0 if mode == "naive" else 0.0, "server_decode_ms": 20.0 if mode == "naive" else 2.0,
        "server_cpu_ms": 30.0 if mode == "naive" else 0.0,
        "path": "server-pillow" if mode == "naive" else "browser-wasm",
        "python_calls": 0,
    }
    if mode == "naive":
        base.update({"in_w": 1600, "in_h": 1200, "scale": 4, "checksum": 1})
    else:
        base.update({"consistent": True, "client": {"in_w": 1600, "in_h": 1200, "scale": 4, "checksum": 1, "wasm_fetched": 2,
                                                     "ms": {"wasm_call_ms": 50.0, "total_ms": 200.0}}})
    base.update(extra)
    return base


def test_n1_summary_is_a_function_of_records():
    a = [_rec("a.jpg", "client", 10_000), _rec("a.jpg", "naive", 400_000)]
    b = [_rec("a.jpg", "client", 20_000), _rec("a.jpg", "naive", 400_000)]
    sa, sb = M.summarize(a), M.summarize(b)
    assert sa["aggregate"]["reduction_x"] == pytest.approx(40.0)
    assert sb["aggregate"]["reduction_x"] == pytest.approx(20.0)
    assert sa["rows"][0]["wire_reduction_x"] == pytest.approx(400_500 / 10_500)
    assert sa["rows"][0]["checksum_match"] is True
    assert M.format_table(sa) != M.format_table(sb)
    # roles swapped -> the ratio inverts (no "always ~40x" constant anywhere)
    swapped = [_rec("a.jpg", "client", 400_000), _rec("a.jpg", "naive", 10_000)]
    assert M.summarize(swapped)["aggregate"]["reduction_x"] == pytest.approx(1 / 40.0)
    # an unmeasured upload is refused, never silently counted
    bad = copy.deepcopy(a)
    bad[0]["upload_measured"] = False
    with pytest.raises(ValueError, match="not measured"):
        M.summarize(bad)
    with pytest.raises(ValueError, match="need both"):
        M.summarize(a[:1])
    # a client record that did NOT run in the tab is refused unless tabulated AS a fallback (r1/B4)
    fb = [_rec("a.jpg", "client", 400_200, path="python-fallback", python_calls=1, in_w=1600, in_h=1200, scale=4, checksum=1), _rec("a.jpg", "naive", 400_000)]
    with pytest.raises(ValueError, match="did not all run in the tab"):
        M.summarize(fb)
    row = M.summarize(fb, allow_fallback=True)["rows"][0]
    assert row["client_path"] == "python-fallback" and row["reduction_x"] == pytest.approx(400_000 / 400_200)
    # a client whose story contradicts what the server received is refused
    inc = copy.deepcopy(a)
    inc[0]["consistent"] = False
    with pytest.raises(ValueError, match="inconsistent"):
        M.summarize(inc)
    # the browser's checksum is compared against the SERVER's reference, never trusted
    mism = copy.deepcopy(a)
    mism[0]["client"]["checksum"] = 2
    assert M.summarize(mism)["rows"][0]["checksum_match"] is False


def test_n2_committed_evidence_is_derived_from_committed_records():
    rec_p, sum_p = IMG_DIR / "metrics_records.json", IMG_DIR / "metrics_summary.json"
    gate(rec_p.is_file() and sum_p.is_file(), "no committed notebook evidence (run metrics.ipynb)")
    records = json.loads(rec_p.read_text(encoding="utf-8"))
    committed = json.loads(sum_p.read_text(encoding="utf-8"))
    cp = {r["image"]: {"cpython_kernel_ms": r["cpython_kernel_ms"]} for r in committed["rows"] if "cpython_kernel_ms" in r}
    derived = M.summarize(records, cp or None)
    assert [r["image"] for r in derived["rows"]] == [r["image"] for r in committed["rows"]]
    for d, c in zip(derived["rows"], committed["rows"]):
        for k in ("naive_upload_bytes", "client_upload_bytes", "reduction_x", "reduction_pct", "wire_reduction_x", "naive_server_resize_ms", "naive_server_cpu_ms", "client_wasm_call_ms", "checksum_match", "client_path"):
            assert d[k] == pytest.approx(c[k]) if isinstance(c[k], float) else d[k] == c[k], (d["image"], k)
    assert derived["aggregate"]["reduction_x"] == pytest.approx(committed["aggregate"]["reduction_x"])
    names = {p.name for p in M.committed_images()}
    assert {r["image"] for r in committed["rows"]} == names
    assert committed["aggregate"]["client_paths"] == "browser-wasm"
    for r in records:
        if r["mode"] == "client":
            assert r["path"] == "browser-wasm" and r["python_calls"] == 0 and r["consistent"] is True


def test_n3_notebook_executes_end_to_end_from_a_clean_copy(tmp_path):
    gate_node()
    gate_import("playwright.sync_api")
    gate_import("gradio_wasmfunction")
    nbformat = gate_import("nbformat")
    nbclient = gate_import("nbclient")
    gate_import("ipykernel")
    gate((IMG_DIR / "__pythscribe__" / "downscale_box" / "manifest.json").is_file(), "M1 artifacts not built")
    work = tmp_path / "nb"
    shutil.copytree(IMG_DIR, work, ignore=shutil.ignore_patterns("__pycache__", "_app.log", "metrics_records.json", "metrics_summary.json", "bytes_reduction.png"))
    nb = nbformat.read(work / "metrics.ipynb", as_version=4)
    os.environ["NB_REPEATS"] = "1"
    os.environ["PYTHONUTF8"] = "1"
    client = nbclient.NotebookClient(nb, timeout=1500, kernel_name="python3", resources={"metadata": {"path": str(work)}})
    client.execute()
    text = "\n".join(
        "".join(o.get("text", "") or "".join(o.get("data", {}).get("text/markdown", "")) or "".join(o.get("data", {}).get("text/plain", "")))
        for c in nb.cells if c.cell_type == "code" for o in c.get("outputs", [])
    )
    for p in M.committed_images():
        assert p.name in text
    assert "RED as required" in text and "computed, not constants" in text
    summary = json.loads((work / "metrics_summary.json").read_text(encoding="utf-8"))
    assert all(r["reduction_x"] > 5 and r["client_path"] == "browser-wasm" for r in summary["rows"])
    assert (work / "bytes_reduction.png").stat().st_size > 1000
