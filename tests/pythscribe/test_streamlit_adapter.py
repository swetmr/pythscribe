"""The Streamlit adapter's trust boundary + dispatch/result_of round-trip (v0.2.5 M3), no
browser and no Streamlit needed for the pure path (the seam is bytes-through-args; the marker
logic and the fallback are server-side). Mirrors test_adapter.py (the Gradio adapter).

The SAME framework-neutral helpers back both adapters (ONE authority): these tests assert the
Streamlit-facing behaviour -- bytes packaging, the crossable grammar, float reconstruction from
bits (never the client JSON number), server-side path markers, unknown-nonce answers, and the
`.wasm`-too-big cap."""
from __future__ import annotations

import base64
import math
import shutil
import struct

import pytest

from conftest import REPO, import_module_from

import pythscribe.streamlit as S
from pythscribe import binding_of
from pythscribe.build.runner import float_bits

STREAMLIT_DIR = REPO / "examples" / "streamlit-wasm"


def f64bits_le(x: float) -> str:
    return struct.pack("<d", x).hex()


@pytest.fixture(scope="module")
def art_kernel():
    """The committed streamlit demo kernel WITH its artifact (browser path)."""
    mod = import_module_from(STREAMLIT_DIR / "kernels.py", "streamlit_kernels_art")
    b = binding_of(mod.rms_gain)
    if b.artifact is None:
        pytest.skip(f"streamlit demo artifact not built (status={b.artifact_status}); run `python -m pythscribe.build examples/streamlit-wasm/kernels.py`")
    return mod.rms_gain


@pytest.fixture
def noart_kernel(tmp_path):
    """The SAME source in a fresh dir with no artifact (Python fallback path)."""
    d = tmp_path / "noart"
    d.mkdir()
    shutil.copyfile(STREAMLIT_DIR / "kernels.py", d / "kernels.py")
    return import_module_from(d / "kernels.py", "streamlit_kernels_noart").rms_gain


# ---------------------------------------------------------------- dispatch (payload shape)

def test_u1_dispatch_with_artifact_carries_wasm_bytes_not_a_url(art_kernel):
    p = S.dispatch(art_kernel, [1.0, 2.0, 3.0, 4.0], 0.5)
    assert p["result"] is None, p  # browser path: result is filled by the iframe
    assert isinstance(p["wasm_b64"], str) and p["wasm_b64"], "browser path must carry the .wasm bytes"
    # the bytes decode to a real WASM module (magic \0asm)
    assert base64.b64decode(p["wasm_b64"])[:4] == b"\x00asm", "wasm_b64 must be the compiled module"
    assert p["param_types"] == ["list[float]", "float"] and p["return_type"] == "float", p
    assert isinstance(p["nonce"], int) and p["fn"] == "rms_gain"
    assert p["browser_refused"] is None


def test_u1b_dispatch_without_artifact_runs_python_fallback_inline(noart_kernel):
    p = S.dispatch(noart_kernel, [1.0, 2.0, 3.0, 4.0], 0.5)
    assert p["wasm_b64"] is None, p
    assert isinstance(p["result"], dict) and p["result"]["path"] == "python-fallback", p
    assert p["result"]["python_calls"] == 1  # the body ran exactly once, server-side


# ---------------------------------------------------------------- result_of (trust boundary)

def test_u2_browser_result_value_is_reconstructed_from_bits(art_kernel):
    truth = binding_of(art_kernel).run_python([1.0, 2.0, 3.0, 4.0], 0.5)  # the CPython body (BEFORE dispatch, so the browser-path delta stays 0)
    p = S.dispatch(art_kernel, [1.0, 2.0, 3.0, 4.0], 0.5)
    # a browser value carrying the CORRECT bits but a WRONG/absent JSON `value`: result_of must
    # trust the BITS, not the client number
    value = {**p, "result": {"path": "browser-wasm", "value": 999.0, "bits": f64bits_le(truth),
                             "python_calls": 0, "wasm_how": "bytes", "wasm_export": "rms_gain"}}
    r = S.result_of(art_kernel, value, expect_nonce=p["nonce"])
    assert r["value"] == truth and r["value"] != 999.0, r  # from bits, not the client copy
    assert r["path"] == "browser-wasm" and r["python_calls"] == 0 and r["server_calls"] == 0


def test_u2b_malformed_bits_falls_back_to_python_never_a_silent_null(art_kernel):
    p = S.dispatch(art_kernel, [2.0, 2.0], 1.0)
    truth = binding_of(art_kernel).run_python([2.0, 2.0], 1.0)
    value = {**p, "result": {"path": "browser-wasm", "value": None, "bits": "not-hex",
                             "python_calls": 0, "wasm_how": "bytes"}}
    r = S.result_of(art_kernel, value, expect_nonce=p["nonce"])
    assert r["path"] == "python-fallback" and r["value"] == truth, r
    assert r["browser_error"] == "malformed browser result (no valid bits)"


def test_u3_non_crossable_args_are_refused(art_kernel):
    for bad in ([float("inf")], 0.5), ([float("nan")], 0.5), ([1.0], 2**53 + 1):
        with pytest.raises((ValueError, TypeError)):
            S.dispatch(art_kernel, *bad)


def test_u4_unknown_nonce_is_answered_never_with_another_snapshot(art_kernel):
    value = {"fn": "rms_gain", "nonce": 99999999, "result": {"path": "browser-wasm", "bits": f64bits_le(1.0)}}
    r = S.result_of(art_kernel, value, expect_nonce=99999999)
    assert r["path"] == "unknown" and "stale client state" in r["error"], r


def test_u5_wasm_too_big_refuses_browser_path_and_runs_fallback(art_kernel, monkeypatch):
    monkeypatch.setattr(S, "MAX_WASM_BYTES", 1)  # any real .wasm exceeds this
    p = S.dispatch(art_kernel, [1.0, 2.0], 0.5)
    assert p["wasm_b64"] is None and p["browser_refused"], p
    assert p["result"]["path"] == "python-fallback"


def test_u6_float_from_bits_roundtrips_special_values():
    for x in (-0.0, math.inf, -math.inf):
        assert S.float_from_bits(f64bits_le(x)) == x or (x != x)
    assert math.copysign(1.0, S.float_from_bits(f64bits_le(-0.0))) == -1.0  # -0.0 preserved
    assert math.isnan(S.float_from_bits(f64bits_le(math.nan)))
    assert S.float_from_bits("not-16-hex") is None  # malformed refused


def test_u7_browser_error_reported_and_fallback_run(art_kernel):
    p = S.dispatch(art_kernel, [3.0], 0.5)
    truth = binding_of(art_kernel).run_python([3.0], 0.5)
    r = S.result_of(art_kernel, {**p, "error": "boom in the tab"}, expect_nonce=p["nonce"])
    assert r["path"] == "python-fallback" and r["value"] == truth and r["browser_error"] == "boom in the tab"


def test_u9_stale_nonce_from_a_previous_dispatch_is_not_answered_as_current(art_kernel):
    """B1 negative control: a keyed Streamlit component keeps its published value across arg
    changes, so `result_of` can be handed a PREVIOUS dispatch's result while a new one is in
    flight. With `expect_nonce` set to the CURRENT dispatch, the stale value must read as 'still
    computing' (None), never as the answer for the new inputs."""
    truth1 = binding_of(art_kernel).run_python([1.0, 2.0, 3.0, 4.0], 0.5)
    p1 = S.dispatch(art_kernel, [1.0, 2.0, 3.0, 4.0], 0.5)
    stale_value = {**p1, "result": {"path": "browser-wasm", "value": truth1, "bits": f64bits_le(truth1),
                                    "python_calls": 0, "wasm_how": "bytes", "wasm_export": "rms_gain"}}
    p2 = S.dispatch(art_kernel, [5.0, 6.0], 0.5)  # a NEW dispatch, different inputs + nonce
    # the component still holds p1's value; result_of for the CURRENT dispatch (p2) must not answer
    assert S.result_of(art_kernel, stale_value, expect_nonce=p2["nonce"]) is None
    # and once the current value arrives it IS answered
    truth2 = binding_of(art_kernel).run_python([5.0, 6.0], 0.5)
    cur = {**p2, "result": {"path": "browser-wasm", "value": truth2, "bits": f64bits_le(truth2),
                            "python_calls": 0, "wasm_how": "bytes", "wasm_export": "rms_gain"}}
    r = S.result_of(art_kernel, cur, expect_nonce=p2["nonce"])
    assert r is not None and r["value"] == truth2 and r["path"] == "browser-wasm"


def test_u10_dispatch_refuses_non_float_return(import_source):
    """B3: dispatch must refuse a kernel whose return type is not float (the one M0 crossed type),
    at the boundary BOTH paths share -- so the contract is not path-dependent."""
    for ann, src in (
        ("int", "from pythscribe import wasm\n@wasm\ndef k(x: int) -> int:\n    return x\n"),
        ("None", "from pythscribe import wasm\n@wasm\ndef k(x: int) -> None:\n    return None\n"),
        ("bool", "from pythscribe import wasm\n@wasm\ndef k(x: int) -> bool:\n    return x > 0\n"),
    ):
        mod = import_source(src, stem=f"nonfloat_{ann}")
        with pytest.raises(TypeError, match="must return float"):
            S.dispatch(mod.k, 3)


def test_u11_deadline_runs_fallback_only_after_the_deadline_and_persists(art_kernel):
    """SF1 + B5: deadline_result returns None while within the deadline, the Python fallback once
    it passes, and thereafter the STORED result on every later call (so the app does not hang at
    'running...' on the next rerun -- B5)."""
    p = S.dispatch(art_kernel, [2.0, 2.0], 1.0)
    assert S.deadline_result(art_kernel, p, wait_s=1000) is None  # far from the deadline
    truth = binding_of(art_kernel).run_python([2.0, 2.0], 1.0)
    r = S.deadline_result(art_kernel, p, wait_s=0.0)  # deadline already passed
    assert r is not None and r["path"] == "python-fallback" and r["value"] == truth
    assert r["browser_error"].startswith("timeout")
    # B5: the finalized result PERSISTS -- a later rerun returns the SAME dict, never None (no hang)
    r2 = S.deadline_result(art_kernel, p, wait_s=0.0)
    assert r2 == r
    # and result_of for that dispatch also returns the stored fallback (a stale/None browser value)
    assert S.result_of(art_kernel, {**p, "result": None}, expect_nonce=p["nonce"]) == r


def test_b4_result_of_requires_expect_nonce(art_kernel):
    """B4: expect_nonce has no default -- omitting it (the r1 silent-wrong-value bug) is a
    TypeError, not a silent 'answer any known nonce'."""
    p = S.dispatch(art_kernel, [1.0], 0.5)
    with pytest.raises(TypeError):
        S.result_of(art_kernel, {**p, "result": None})  # missing required keyword


def test_b5_result_of_is_idempotent_no_python_calls_drift_on_reruns(art_kernel):
    """B5 / SF5: after a browser ERROR, result_of runs the Python body ONCE and caches it; every
    later rerun with the same value returns the cached result -- the body is not re-run, so
    python_calls does not drift 1->2->3 across reruns."""
    before = binding_of(art_kernel).calls()
    p = S.dispatch(art_kernel, [3.0, 4.0], 0.5)
    value = {**p, "error": "boom in the tab"}
    r1 = S.result_of(art_kernel, value, expect_nonce=p["nonce"])
    calls_after_1 = binding_of(art_kernel).calls()
    # three more reruns with the SAME value -> must NOT re-execute the body
    for _ in range(3):
        rn = S.result_of(art_kernel, value, expect_nonce=p["nonce"])
        assert rn == r1  # identical stored result
    assert binding_of(art_kernel).calls() == calls_after_1, "python body re-ran on a rerun (drift)"
    assert calls_after_1 - before == 1, "the fallback body ran exactly once"
    assert r1["path"] == "python-fallback" and r1["browser_error"] == "boom in the tab"


def test_sf10_result_cache_honours_per_function_scope(art_kernel):
    """SF10: the per-nonce result cache must not cross functions -- a value carrying a nonce
    finalized for kernel A, read back for a DIFFERENT function B, must get the loud unknown, never
    A's stored result."""
    p = S.dispatch(art_kernel, [1.0, 2.0], 0.5)
    truth = binding_of(art_kernel).run_python([1.0, 2.0], 0.5)
    v = {**p, "result": {"path": "browser-wasm", "bits": f64bits_le(truth), "python_calls": 0,
                         "wasm_how": "bytes", "wasm_export": "rms_gain"}}
    S.result_of(art_kernel, v, expect_nonce=p["nonce"])  # finalize for rms_gain
    # a stored entry exists for this nonce, but under the name "rms_gain"
    assert S._final_result(p["nonce"], "rms_gain") is not None
    assert S._final_result(p["nonce"], "some_other_kernel") is None  # never crosses scope


def test_sf11_finalize_is_first_wins(art_kernel):
    """SF11: two finalizations of one nonce keep the FIRST result (no diverging stored value)."""
    p = S.dispatch(art_kernel, [1.0], 0.5)
    first = {"path": "browser-wasm", "value": 1.0}
    second = {"path": "python-fallback", "value": 2.0}
    assert S._finalize(p["nonce"], "rms_gain", first) is first
    assert S._finalize(p["nonce"], "rms_gain", second) is first  # first-wins
    assert S._final_result(p["nonce"], "rms_gain") is first


def test_sf9_deadline_terminal_when_dispatch_record_evicted(art_kernel):
    """SF9: past the deadline with no dispatch record (evicted) and nothing cached, deadline_result
    returns a TERMINAL unknown so the demo's poll stops (no endless loop); before the deadline it
    keeps waiting (None)."""
    import time as _t
    # a browser-path payload for an UNKNOWN nonce (never dispatched -> no _recall entry, no cache)
    fake = {"wasm_b64": "x", "nonce": 999_000_001, "dispatched_at": _t.monotonic()}
    assert S.deadline_result(art_kernel, fake, wait_s=1000) is None            # within deadline -> wait
    past = {"wasm_b64": "x", "nonce": 999_000_002, "dispatched_at": _t.monotonic() - 100}
    r = S.deadline_result(art_kernel, past, wait_s=1.0)                        # past deadline, no record
    assert r is not None and r["path"] == "unknown" and "evicted" in r["error"]


def test_sf13_deadline_terminal_when_result_evicted_but_completed(art_kernel, monkeypatch):
    """SF13: a nonce completed (marked) but result-EVICTED (results evict by finalization order)
    must, past the deadline, get a TERMINAL answer so the demo poll stops -- not None forever."""
    monkeypatch.setattr(S, "_RESULT_HISTORY", 1)
    p = S.dispatch(art_kernel, [1.0, 2.0], 0.5)
    S.result_of(art_kernel, {**p, "error": "boom"}, expect_nonce=p["nonce"])  # finalize + complete
    S._finalize(p["nonce"] + 777, "rms_gain", {"path": "x"})  # evict p from the results store
    assert S._final_result(p["nonce"], "rms_gain") is None    # evicted...
    assert S.is_completed(p["nonce"])                          # ...but still completed
    past = {**p, "dispatched_at": p["dispatched_at"] - 100}    # past the deadline
    r = S.deadline_result(art_kernel, past, wait_s=1.0)
    assert r is not None and r["path"] == "unknown", r         # terminal, not None -> poll stops
    # within the deadline it still waits (None), not a premature terminal
    within = {**p, "dispatched_at": p["dispatched_at"]}
    assert S.deadline_result(art_kernel, within, wait_s=10_000) is None


def test_u8_describe_surfaces_the_resolution_markers():
    s = S.describe({"value": 0.3, "bits": "abc", "path": "browser-wasm", "python_calls": 0,
                    "wasm_how": "bytes", "wasm_export": "rms_gain", "server_calls": 0})
    assert "path=browser-wasm" in s and "wasm_how=bytes" in s and "wasm_export=rms_gain" in s and "python_calls=0" in s
