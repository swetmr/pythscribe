"""v0.2.6 fix B -- the runtime-verified gate for IN-TAB image filters (`@wasm` typed-array kernels
driven from a Gradio `js=` hook, no custom component): a real Chromium drives
`examples/wasm-use-cases/browser_image_filters_app.py` and asserts

  1. CLIENT-SIDE: across the slider moves + filter switches the tab issued ZERO /gradio_api
     compute requests (only static `file=` fetches: the input PNG once, each kernel's .wasm once),
     AND the server-side kernel counters (python_calls + server_calls) did not move.
     PAIRED NEGATIVE CONTROL: the app's "Server round-trip" tab (the SAME kernels wired through a
     Python fn) driven through the SAME harness FAILS both assertions (requests > 0, counters > 0)
     -- so a silent fall-back to the server could not pass this gate.
  2. FIDELITY: the OUT rows read back from WASM memory AND the PNG actually DISPLAYED in the
     output component equal the independent NumPy reference bit-for-bit, for threshold at 10
     settings, sobel, and nearest-neighbor downscale at 2 factors.
     PAIRED NEGATIVE CONTROL: the reference at a neighbouring threshold / a transposed read must
     NOT equal the browser output (the equality is load-bearing, not vacuous).
  3. LATENCY: the in-tab median (slider -> displayed image) is reported and must beat the server
     tab's median for the same kernel + image.

Gated (skips cleanly) without playwright + chromium, gradio, numpy, PIL. The driver lives in
examples/wasm-use-cases/browser_image_probe.py (shared with the notebook's reporting cell)."""
from __future__ import annotations

from pathlib import Path

import pytest

from conftest import REPO, gate_import, import_module_from, soft_perf

np = gate_import("numpy")
gate_import("PIL")
gate_import("gradio")
gate_import("fastapi")

USE_CASES = REPO / "examples" / "wasm-use-cases"
# by PATH, never via sys.path: this dir and gradio-image-preprocess both have a `kernels.py`
probe = import_module_from(USE_CASES / "browser_image_probe.py", "browser_image_probe_for_e2e")
load_rgb = probe.load_rgb


@pytest.fixture(autouse=True, scope="module")
def _need_browser():
    gate_import("playwright.sync_api")


@pytest.fixture(scope="module")
def artifacts():
    """The three committed artifacts VERIFY (manifest + source hash + compiler pin), not merely
    exist -- `test_artifact_committed.py` gates the same thing without a browser."""
    from pythscribe._pin import COMPILER_VERSION
    from pythscribe._static import find_wasm_defs, kernel_source, sha256_text
    from pythscribe.artifacts import verify

    for src, fn in ((USE_CASES / "kernels.py", "threshold_lum"), (USE_CASES / "kernels.py", "sobel"),
                    (REPO / "examples" / "gradio-image-preprocess" / "kernels.py", "downscale_nn")):
        [node] = [n for n in find_wasm_defs(src.read_text(encoding="utf-8"), src.name) if n.name == fn]
        info = verify(src.parent / "__pythscribe__" / fn, function=fn, expected_source_sha256=sha256_text(kernel_source(src.read_text(encoding="utf-8"), node)))
        assert info.manifest["compiler"]["version"] == COMPILER_VERSION, fn


@pytest.fixture(scope="module")
def run(artifacts):
    with probe.App() as app:
        res = probe.drive(app)
    assert not res["page_errors"], f"browser page errors: {res['page_errors']}"
    return res


@pytest.fixture(scope="module")
def rgb():
    return load_rgb()


KERNEL_KEYS = ("threshold", "sobel", "downscale")
ARTIFACT_DIRS = (
    USE_CASES / "__pythscribe__" / "threshold_lum",
    USE_CASES / "__pythscribe__" / "sobel",
    REPO / "examples" / "gradio-image-preprocess" / "__pythscribe__" / "downscale_nn",
)


def test_transform_runs_client_side_with_zero_server_involvement(run):
    c = run["client"]
    assert c["load_error"] is None, f"the in-tab client did not load: {c['load_error']}"
    assert c["last_error"] is None, c["status"]
    assert c["calls"] >= len(probe.THRESHOLDS) + 1 + len(probe.SCALES)
    assert c["compute_requests"] == [], f"the in-tab path made compute requests: {c['compute_requests']}"
    assert c["counters_delta"] == 0, f"server-side kernel counters moved on the in-tab path: {c['counters_after']}"
    assert c["counters_delta_per_kernel"] == {k: 0 for k in KERNEL_KEYS}, c["counters_delta_per_kernel"]
    # the only server contact is static: the input image once and each kernel's .wasm once --
    # and every static fetch points INSIDE an artifact dir or Gradio's own cache (nothing else served)
    assert len(c["wasm_requests"]) == 3, c["wasm_requests"]
    from urllib.parse import unquote

    from gradio.utils import get_upload_folder  # Gradio's own answer for the cache root (GRADIO_TEMP_DIR-aware)

    roots = [d.resolve().as_posix() for d in ARTIFACT_DIRS] + [Path(get_upload_folder()).resolve().as_posix()]
    for u in c["file_requests"]:
        path = unquote(u.split("/gradio_api/file=", 1)[1]).replace("\\", "/")  # `_static_file_url` percent-encodes (spaces, #, ?)
        assert any(r in path for r in roots), f"unexpected static fetch outside the artifact dirs / Gradio cache: {u}"
    assert len(c["file_requests"]) <= 3 + 1, f"the input image is fetched once and cached: {c['file_requests']}"
    assert "0 round-trips" in c["status"] and "@wasm in-tab" in c["status"]
    assert c["layout"] == "pyths-0.2.5-array-v2"


def test_negative_control_server_tab_fails_the_client_side_assertions(run):
    """The paired control: the same kernels through a Python fn MUST trip both markers -- and
    the counter marker PER KERNEL (a marker that cannot move for one kernel is not hidden in a
    cross-kernel sum)."""
    s = run["server"]
    assert len(s["compute_requests"]) > 0, "the server tab made no compute requests -- the request marker is vacuous"
    assert s["counters_delta"] > 0, "the server tab did not move the kernel counters -- the counter marker is vacuous"
    for k in KERNEL_KEYS:
        assert s["counters_delta_per_kernel"][k] > 0, f"the server tab did not move the `{k}` counter -- the marker is vacuous for {k}: {s['counters_delta_per_kernel']}"
    assert "on the SERVER" in s["status"] and "on the SERVER" in s["last_status"]


def test_bit_for_bit_vs_numpy_rows_and_displayed_png(run, rgb):
    fid = probe.fidelity(run, rgb)
    bad = {k: v for k, v in fid.items() if not (v["rows_equal"] and v["shown_equal"] and v["ret_equal"])}
    assert not bad, f"in-tab output != NumPy reference (rows / displayed PNG / scalar return): {bad}"
    assert len(fid) == len(probe.THRESHOLDS) + 1 + len(probe.SCALES)
    h, w = rgb.shape[:2]
    assert fid["downscale@2"]["shape"] == [h // 2, (w // 2) * 3]
    assert fid["downscale@2"]["ret"] == (h // 2) * (w // 2) and fid["sobel"]["ret"] == (h - 2) * (w - 2)


def test_fidelity_negative_controls_go_red(run, rgb):
    outs = run["client"]["outs"]
    got = np.array(outs["threshold@150"], dtype=np.uint8)
    assert not np.array_equal(got, probe.ref_threshold(rgb, 153 * 3)), "a neighbouring threshold must diverge (control not vacuous)"
    assert np.array_equal(got, probe.ref_threshold(rgb, 150 * 3))
    h, w = rgb.shape[:2]
    n = min(h, w)
    sob = np.array(outs["sobel"], dtype=np.uint8).reshape(h, w, 3)[:n, :n]
    ref = probe.ref_sobel(rgb).reshape(h, w, 3)[:n, :n]
    assert not np.array_equal(np.transpose(sob, (1, 0, 2)), ref), "a transposed read must diverge from the reference (control not vacuous)"
    assert np.array_equal(sob, ref)
    assert not np.array_equal(np.array(outs["sobel"], dtype=np.uint8), probe.ref_threshold(rgb, 150 * 3)), "sobel must not equal a threshold output"


def test_in_tab_latency_beats_the_server_round_trip(run):
    c, s = run["client"], run["server"]
    print(f"\nin-tab median {c['median_ms']:.1f} ms (kernel {c['kernel_median_ms']:.1f} ms; sobel kernel {c['sobel_kernel_ms']:.1f} ms) "
          f"vs server median {s['median_ms']:.1f} ms")
    # in-tab vs server round-trip is a TIMING claim -> non-blocking (loaded-runner noise; byte-identical wheels)
    soft_perf(c["median_ms"] < s["median_ms"], f"in-tab median {c['median_ms']:.1f} ms !< server median {s['median_ms']:.1f} ms")
