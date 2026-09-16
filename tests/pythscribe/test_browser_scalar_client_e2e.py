"""v0.2.6 fix A -- the runtime-verified gate for `pythscribe.gradio.client_side` (SCALAR `@wasm`
kernels driven from a Gradio `js=` hook, no custom component, no hand-written JS): a real Chromium
drives `examples/wasm-use-cases/browser_scalar_client_app.py` and asserts

  1. CLIENT-SIDE: across 10 filter-slider moves, 6 card numbers, 4 texts and 7 loan moves the tab
     issued ZERO /gradio_api compute requests (only static `file=` fetches: each kernel's .wasm
     once), AND the server-side kernel counters (python_calls + server_calls) did not move -- PER
     KERNEL. PAIRED NEGATIVE CONTROL: the app's "Server round-trip" tab (the SAME kernels wired
     through Python fns) driven through the SAME harness FAILS both assertions (requests > 0,
     every kernel's counter > 0) -- a silent fall-back to the server could not pass this gate.
  2. FIDELITY: every float the tab computed (its IEEE-754 bits, read from the client) AND the text
     actually DISPLAYED in the output Textbox equal the CPython kernel run on independent Python
     twins of the `Arg` transforms, for all 27 interactions; the args the transforms produced equal
     the twins' too. PAIRED NEGATIVE CONTROL: the reference at a NEIGHBOURING input (threshold
     shifted, a card digit flipped, a digit dropped from a run, months+1) must NOT equal.
  3. LATENCY: the in-tab median (slider -> the status box shows the new call) is reported and must
     beat the server tab's median for the same kernel.

Gated (skips cleanly) without playwright + chromium, gradio, fastapi. The driver lives in
examples/wasm-use-cases/browser_scalar_probe.py (shared with the notebook's reporting cell).
"""
from __future__ import annotations

from pathlib import Path

import pytest

from conftest import REPO, gate_import, import_module_from

gate_import("gradio")
gate_import("fastapi")

USE_CASES = REPO / "examples" / "wasm-use-cases"
probe = import_module_from(USE_CASES / "browser_scalar_probe.py", "browser_scalar_probe_for_e2e")
KERNEL_FNS = {"filter": "count_above", "luhn": "luhn_ok", "pii": "pii_scan", "loan": "monthly_payment"}


@pytest.fixture(autouse=True, scope="module")
def _need_browser():
    gate_import("playwright.sync_api")


@pytest.fixture(scope="module")
def artifacts():
    """The four committed artifacts VERIFY (manifest + source hash + compiler pin), not merely
    exist -- `test_artifact_committed.py` gates the same thing without a browser."""
    from pythscribe._pin import COMPILER_VERSION
    from pythscribe._static import find_wasm_defs, kernel_source, sha256_text
    from pythscribe.artifacts import verify

    src = USE_CASES / "kernels.py"
    for fn in KERNEL_FNS.values():
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
def mod():
    return probe.kernels()


N_INTERACTIONS = len(probe.THRESHOLDS) + len(probe.CARDS) + len(probe.TEXTS) + len(probe.MONTHS) + len(probe.RATES) - 1
N_WARMUPS = 5


def test_computation_runs_client_side_with_zero_server_involvement(run):
    c = run["client"]
    assert c["load_error"] is None, f"the in-tab client did not load: {c['load_error']}"
    assert c["last_error"] is None, c["status"]
    assert c["calls"] == N_INTERACTIONS + N_WARMUPS, c["calls"]  # exactly one kernel call per event (never a double fire)
    assert c["compute_requests"] == [], f"the in-tab path made compute requests: {c['compute_requests']}"
    assert c["counters_delta_per_kernel"] == {k: 0 for k in KERNEL_FNS}, f"server-side kernel counters moved on the in-tab path: {c['counters_after']}"
    # the only server contact is static: each kernel's .wasm once -- every fetch INSIDE an artifact dir (nothing else served)
    assert len(c["wasm_requests"]) == 4 and sorted(Path(u).name for u in c["wasm_requests"]) == sorted(f"{fn}.wasm" for fn in KERNEL_FNS.values()), c["wasm_requests"]
    from urllib.parse import unquote

    roots = [(USE_CASES / "__pythscribe__" / fn).resolve().as_posix() for fn in KERNEL_FNS.values()]
    for u in c["file_requests"]:
        path = unquote(u.split("/gradio_api/file=", 1)[1]).replace("\\", "/")
        assert any(r in path for r in roots), f"unexpected static fetch outside the artifact dirs: {u}"
    assert len(c["file_requests"]) == 4, f"each .wasm is fetched exactly once and instantiated once: {c['file_requests']}"
    assert "0 round-trips" in c["status"] and "@wasm in-tab" in c["status"]
    assert c["layout"] == "pyths-0.2.4-list-v1"


def test_negative_control_server_tab_fails_the_client_side_assertions(run):
    """The paired control: the same kernels through Python fns MUST trip both markers -- and the
    counter marker PER KERNEL (a marker that cannot move for one kernel is not hidden in a sum)."""
    s = run["server"]
    assert len(s["compute_requests"]) > 0, "the server tab made no compute requests -- the request marker is vacuous"
    assert s["counters_delta"] > 0, "the server tab did not move the kernel counters -- the counter marker is vacuous"
    for k in KERNEL_FNS:
        assert s["counters_delta_per_kernel"][k] > 0, f"the server tab did not move the `{k}` counter -- the marker is vacuous for {k}: {s['counters_delta_per_kernel']}"
    assert "on the SERVER" in s["status"] and "on the SERVER" in s["last_status"]


def test_bit_for_bit_vs_cpython_floats_args_and_displayed_text(run, mod):
    fid = probe.fidelity(run, mod)
    bad = {k: v for k, v in fid.items() if not (v["bits_equal"] and v["shown_equal"] and v["args_equal"])}
    assert not bad, f"in-tab result != CPython reference (bits / displayed text / transformed args): {bad}"
    assert len(fid) == N_INTERACTIONS
    # independent spot checks of the kernels' meaning (not derived from the same constants)
    assert fid["luhn@4242 4242 4242 4242"]["value"] == 0.0 and fid["luhn@4242-4242-4242-4241"]["value"] == 1.0
    assert fid["luhn@"]["shown"] == "enter a card number"
    assert fid["pii@card 4242424242424242 ssn 123456789"]["value"] == 2.0 and fid["pii@no digits here"]["value"] == 0.0
    assert fid["pii@12345678 short 123456789012 long"]["value"] == 1.0  # an 8-digit run does not count; a 12-digit run counts once
    assert fid["loan@r0"]["value"] == probe.PRINCIPAL / probe.MONTHS[-1]  # the r == 0 branch
    assert 0 < fid["filter@0.9"]["value"] < fid["filter@0.05"]["value"] < 2000


def test_fidelity_negative_controls_go_red(run, mod):
    recs = run["client"]["recs"]
    for key, wrong_bits in probe.perturbed_references(mod).items():
        assert recs[key]["bits"] != wrong_bits, f"a neighbouring input must diverge for {key} (control not vacuous)"


def test_in_tab_latency_beats_the_server_round_trip(run):
    c, s = run["client"], run["server"]
    print(f"\nin-tab median {c['median_ms']:.1f} ms (kernel {c['kernel_median_ms']:.3f} ms) vs server median {s['median_ms']:.1f} ms")
    assert c["median_ms"] < s["median_ms"], (c["median_ms"], s["median_ms"])
