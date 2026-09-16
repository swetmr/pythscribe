"""M1 runtime-verified gates on the demo APP, driven with Playwright against the real server
(the same driver the reproducibility notebook uses):
  A1 client panel, lossless PNG: path=browser-wasm, server python_calls=0, .wasm fetched,
     WASM output checksum == the reference computed from the same file (exact, in-browser);
  A2 bytes-uploaded reduction is REAL: client vs naive on the same JPEG, BOTH measured by the
     server-side meter, ratio > 5x; the naive meter reading covers the whole file (the meter
     is not blind); negative control: the fallback app's client panel (which uploads the
     ORIGINAL) makes the same assertion go RED -- "client secretly uploaded the full image"
     is caught by the measurement, not by trust;
  A3 fallback: an app copy WITHOUT the artifacts still works (python-fallback, python_calls=1,
     exact result, no .wasm request);
  A4 resolution: a POISONED .wasm (off-by-one kernel, manifest re-signed) changes the
     in-browser output (so the output comes from the .wasm), while a poisoned JS twin in
     the glue changes nothing (the twin is not on the image path).
"""
from __future__ import annotations

import json
import shutil
import sys
from pathlib import Path

import numpy as np
import pytest

from conftest import REPO, gate, gate_import, gate_node
from pythscribe.artifacts import MANIFEST_NAME, manifest_self_hash, sha256_file, verify
from pythscribe.build import build_module

IMG_DIR = REPO / "examples" / "gradio-image-preprocess"
sys.path.insert(0, str(IMG_DIR))
import reference as R  # noqa: E402
from drive import AppUnderTest, browser_session, drive_client, drive_naive, open_app  # noqa: E402

JPEG = IMG_DIR / "test_images" / "photo_1600x1200.jpg"
SMALL_JPEG = IMG_DIR / "test_images" / "photo_640x480.jpg"
MAX_DIM = 512


@pytest.fixture(autouse=True, scope="module")
def _need_stack():
    gate_node()
    gate_import("playwright.sync_api")
    gate_import("gradio_wasmfunction")
    gate((IMG_DIR / "__pythscribe__" / "downscale_box" / MANIFEST_NAME).is_file(), "M1 artifacts not built")


@pytest.fixture(scope="module")
def png(tmp_path_factory) -> Path:
    """A lossless input: every decoder yields the same pixels, so the in-browser checksum
    can be compared EXACTLY with the reference (JPEG decoders may differ by an LSB)."""
    p = tmp_path_factory.mktemp("png") / "photo_640x480.png"
    from PIL import Image

    Image.fromarray(R.load_rgb(SMALL_JPEG), "RGB").save(p, format="PNG")
    return p


def reference_checksum(image: Path) -> tuple[int, int, int, int]:
    rgb = R.load_rgb(image)
    h, w = rgb.shape[:2]
    scale = R.choose_scale(w, h, MAX_DIM)
    ref = R.box_reference(rgb, scale)
    return R.checksum_rgb(ref), scale, ref.shape[1], ref.shape[0]


def assert_reduction(client: dict, naive: dict, min_ratio: float = 5.0) -> float:
    """The metric's assertion: both readings are the SERVER's; the client's own number is
    never used. Raises when the reduction is not there."""
    assert client["upload_measured"] and naive["upload_measured"], (client, naive)
    ratio = naive["upload_bytes"] / max(1, client["upload_bytes"])
    assert ratio >= min_ratio, f"bytes-uploaded reduction only {ratio:.2f}x (client {client['upload_bytes']} vs naive {naive['upload_bytes']})"
    return ratio


def copy_app(dst: Path, *, with_artifacts: bool = True) -> Path:
    shutil.copytree(IMG_DIR, dst, ignore=shutil.ignore_patterns("__pycache__", "_app.log", "metrics*", "*.ipynb", "test_images"))
    if not with_artifacts:
        shutil.rmtree(dst / "__pythscribe__")
    return dst


def resign(adir: Path) -> None:
    """Re-sign an artifact after replacing a file (the self-hash is an integrity guard, not
    an authenticity guard -- artifacts.py); the poisoned artifact must still RESOLVE, or the
    control would silently test the fallback instead."""
    m = json.loads((adir / MANIFEST_NAME).read_text(encoding="utf-8"))
    for rel in m["files"]:
        m["files"][rel] = sha256_file(adir / rel)
    m["manifest_sha256"] = manifest_self_hash(m)
    (adir / MANIFEST_NAME).write_text(json.dumps(m, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    verify(adir, function=m["function"], expected_source_sha256=m["source_sha256"])


def test_a1_a2_client_runs_in_wasm_and_uploads_far_fewer_bytes(png):
    crc, scale, ow, oh = reference_checksum(png)
    with AppUnderTest() as app, browser_session() as page:
        open_app(page, app)
        c_png = drive_client(page, app, png)
        c_jpg = drive_client(page, app, JPEG)
        n_jpg = drive_naive(page, app, JPEG)
        responses = list(page.responses)
        errors = list(page.console_errors)
        m = app.metrics()
    # A1: the browser path, with SERVER-side markers (top level) vs the browser's own story (`client`)
    assert c_png["path"] == "browser-wasm", c_png
    assert c_png["python_calls"] == 0 and c_png["python_calls_global_delta"] == 0  # the kernel's Python body never ran here
    assert c_png["consistent"] is True
    cb = c_png["client"]
    assert cb["browser_error"] is None and cb["path_claimed"] == "browser-wasm"
    assert cb["wasm_fetched"] >= 1 and "downscale_box" in cb["wasm_exports"] and cb["wasm_how"] == "streaming"
    assert any(u.endswith("downscale_box.wasm") and s == 200 for u, s in responses), responses[-10:]
    assert (c_png["out_w"], c_png["out_h"]) == (ow, oh)  # dims of the RECEIVED file (server-decoded)
    assert cb["scale"] == scale and (cb["in_w"], cb["in_h"]) == (640, 480)  # client-reported, checked for consistency above
    assert cb["checksum"] == crc  # exact: the browser's WASM output == NumPy reference on identical pixels
    assert not errors, errors
    # A2: the reduction is measured, not asserted -- same meter, same tab, same file
    assert n_jpg["path"] == "server-pillow" and n_jpg["server_resize_ms"] > 0
    assert n_jpg["upload_measured"] and n_jpg["files_in_upload_request"] == 1
    assert n_jpg["upload_bytes"] >= JPEG.stat().st_size  # the meter saw the WHOLE original
    assert n_jpg["upload_bytes"] - JPEG.stat().st_size < 4096  # ...plus only multipart framing
    assert n_jpg["wire_bytes"] == n_jpg["upload_bytes"] + n_jpg["join_bytes"]
    assert c_jpg["path"] == "browser-wasm" and c_jpg["python_calls"] == 0
    assert c_jpg["upload_bytes"] >= c_jpg["file_bytes"]  # the small JPEG, as received
    ratio = assert_reduction(c_jpg, n_jpg)
    assert ratio > 10, ratio
    assert n_jpg["wire_bytes"] / c_jpg["wire_bytes"] > 10  # the event JSON does not change the picture
    assert c_jpg["server_resize_ms"] == 0.0 and c_jpg["server_cpu_ms"] < n_jpg["server_cpu_ms"] + 50
    # the app's own metrics endpoint carries the same records (the notebook reads these)
    assert [r["mode"] for r in m["results"]] == ["client", "client", "naive"]
    assert m["python_calls"]["downscale_box"] == 0


def test_a2_negative_control_full_upload_is_caught_and_a3_fallback_works(tmp_path):
    """Run a COPY of the app without the artifact directory: the client panel uploads the
    original and the server runs the same kernel in Python. That record is exactly what a
    'client that secretly uploads the full image' looks like on the meter -- the reduction
    assertion must go RED on it (A2 negative control). And the app still works (A3)."""
    app_dir = copy_app(tmp_path / "app_no_artifact", with_artifacts=False)
    crc, scale, ow, oh = reference_checksum(SMALL_JPEG)
    with AppUnderTest(app_dir) as app, browser_session() as page:
        open_app(page, app)
        c = drive_client(page, app, SMALL_JPEG)
        n = drive_naive(page, app, SMALL_JPEG)
        responses = list(page.responses)
        errors = list(page.console_errors)
        m = app.metrics()
    assert m["artifact_status"] == "absent"
    assert c["path"] == "python-fallback" and c["python_calls"] == 1 and c["python_calls_global_delta"] == 1, c  # the Python body ran ONCE, here
    assert c["client"]["browser_error"] and "no usable artifact" in c["client"]["browser_error"]
    assert c["client"]["path_claimed"] == "upload-original"
    assert (c["scale"], c["out_w"], c["out_h"]) == (scale, ow, oh)  # server-computed on this path
    assert c["checksum"] == crc  # the Python kernel is exact too (this JPEG decoded by Pillow both times)
    assert c["server_resize_ms"] > 0
    assert not any(u.endswith(".wasm") for u, _ in responses)
    assert not errors, errors
    # the negative control: the same assertion that passes for the real client path is RED here
    assert c["upload_bytes"] >= SMALL_JPEG.stat().st_size
    with pytest.raises(AssertionError, match="reduction only"):
        assert_reduction(c, n)
    # ...and the notebook's summarize refuses to tabulate this as the '@wasm in the tab' arm (review r1/B4)
    import metrics_lib as M

    for r in (c, n):
        r["image"] = SMALL_JPEG.name
    with pytest.raises(ValueError, match="did not all run in the tab"):
        M.summarize([c, n])
    row = M.summarize([c, n], allow_fallback=True)["rows"][0]
    assert row["client_path"] == "python-fallback" and row["reduction_x"] < 1.05


def test_a4_poisoned_wasm_changes_output_poisoned_twin_does_not(tmp_path, png):
    """Resolution markers for 'it ran in WASM in the tab': swap the .wasm for an off-by-one
    kernel (re-signed so the artifact still resolves) -> the browser's checksum changes;
    poison the JS twin in the glue -> nothing changes (the image path never loads the glue)."""
    crc, scale, ow, oh = reference_checksum(png)
    # build the mutant kernel
    src = (IMG_DIR / "kernels.py").read_text(encoding="utf-8")
    mutated = src.replace("area = scale * scale\n", "area = scale * scale - 1\n")
    assert mutated != src
    mut_dir = tmp_path / "mutant"
    mut_dir.mkdir()
    (mut_dir / "kernels.py").write_text(mutated, encoding="utf-8")
    [mut_wasm] = [a.wasm for a in build_module(mut_dir / "kernels.py", quiet=True) if a.function == "downscale_box"]

    # (i) poisoned .wasm
    app_a = copy_app(tmp_path / "app_poisoned_wasm")
    shutil.copyfile(mut_wasm, app_a / "__pythscribe__" / "downscale_box" / "downscale_box.wasm")
    resign(app_a / "__pythscribe__" / "downscale_box")
    with AppUnderTest(app_a) as app, browser_session() as page:
        open_app(page, app)
        c = drive_client(page, app, png)
        status = app.metrics()["artifact_status"]
    assert status == "resolved"  # the control tested the BROWSER path, not the fallback
    assert c["path"] == "browser-wasm" and c["python_calls"] == 0
    assert (c["client"]["scale"], c["out_w"], c["out_h"]) == (scale, ow, oh)
    assert c["client"]["checksum"] != crc  # the output comes from the .wasm: change it and the output changes

    # (ii) poisoned JS twin in the glue
    app_b = copy_app(tmp_path / "app_poisoned_twin")
    glue = app_b / "__pythscribe__" / "downscale_box" / "downscale_box.glue.js"
    text = glue.read_text(encoding="utf-8")
    poisoned = text.replace("function downscale_box(px, w, h, scale, out) {", "function downscale_box(px, w, h, scale, out) {\n    return -1;", 1)
    assert poisoned != text
    glue.write_text(poisoned, encoding="utf-8", newline="\n")
    resign(app_b / "__pythscribe__" / "downscale_box")
    with AppUnderTest(app_b) as app, browser_session() as page:
        open_app(page, app)
        c = drive_client(page, app, png)
        responses = list(page.responses)
        status = app.metrics()["artifact_status"]
    assert status == "resolved"
    assert c["path"] == "browser-wasm" and c["python_calls"] == 0
    assert c["client"]["checksum"] == crc
    assert not any(u.endswith(".glue.js") for u, _ in responses)  # the image path never even loads the twin


def test_reference_helpers_are_a_different_implementation():
    """Guard against a tautological oracle: the reference must not import the kernel."""
    import ast

    tree = ast.parse((IMG_DIR / "reference.py").read_text(encoding="utf-8"))
    imported = {n.module.split(".")[0] if isinstance(n, ast.ImportFrom) and n.module else a.name.split(".")[0]
                for n in ast.walk(tree) if isinstance(n, (ast.Import, ast.ImportFrom)) for a in (n.names if isinstance(n, ast.Import) else [None]) if a is not None or isinstance(n, ast.ImportFrom)}
    assert "kernels" not in imported and "pythscribe" not in imported, imported
    rgb = np.arange(4 * 6 * 3, dtype=np.uint8).reshape(4, 6, 3)
    ref = R.box_reference(rgb, 2)
    assert ref.shape == (2, 3, 3) and int(ref[0, 0, 0]) == (0 + 3 + 18 + 21) // 4
