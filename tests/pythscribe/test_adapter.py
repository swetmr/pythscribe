"""The Gradio adapter's trust boundary (review R1/B2, SF2, SF5, SF6): the server keeps the
dispatched args + call counter per nonce and never trusts the client's copy; the float is
reconstructed from `bits`; non-crossable args are refused; the file URL is percent-encoded.
No server or browser needed."""
from __future__ import annotations

import json
import math
import shutil
from urllib.parse import unquote

import pytest

from conftest import DEMO_DIR, gate_import, import_module_from
from pythscribe import binding_of
from pythscribe.build.runner import float_bits


@pytest.fixture(autouse=True, scope="module")
def _need_component():
    gate_import("gradio")
    gate_import("gradio_wasmfunction")


@pytest.fixture
def adapter():
    import pythscribe.gradio as G

    return G


@pytest.fixture
def noartifact_kernel(tmp_path):
    d = tmp_path / "noart"
    d.mkdir()
    shutil.copyfile(DEMO_DIR / "kernels.py", d / "kernels.py")
    return import_module_from(d / "kernels.py").rms_gain


@pytest.fixture
def artifact_kernel(tmp_path, demo_artifact):
    d = tmp_path / "art"
    d.mkdir()
    shutil.copyfile(DEMO_DIR / "kernels.py", d / "kernels.py")
    shutil.copytree(demo_artifact.dir, d / "__pythscribe__" / "rms_gain")
    return import_module_from(d / "kernels.py").rms_gain


def hop(payload):
    """The JSON round trip the Gradio return path takes (Python -> browser -> Python)."""
    return json.loads(json.dumps(payload))


def test_fallback_dispatch_carries_python_result_with_bits(adapter, noartifact_kernel):
    p = adapter.dispatch(noartifact_kernel, [3.0, 4.0], -0.0)
    assert p["bundle"] is None
    r = p["result"]
    assert r["path"] == "python-fallback" and r["python_calls"] == 1
    assert r["bits"] == float_bits(-0.0 / ((25.0 / 2) ** 0.5))
    hp = hop(p)  # JSON-safe: no Infinity tokens, the float is recoverable from bits
    got = adapter.result_of(noartifact_kernel, hp)
    assert got["path"] == "python-fallback"
    assert adapter.float_from_bits(got["bits"]) == 0.0 and math.copysign(1.0, adapter.float_from_bits(got["bits"])) < 0


def test_fallback_dispatch_non_finite_result_is_json_safe(adapter, noartifact_kernel):
    p = adapter.dispatch(noartifact_kernel, [1e-160], 1e308)
    s = json.dumps(p)
    assert "Infinity" not in s and "NaN" not in s
    assert p["result"]["value"] is None and adapter.float_from_bits(p["result"]["bits"]) == math.inf


def test_dispatch_refuses_non_crossable_args(adapter, noartifact_kernel):
    with pytest.raises(ValueError, match="non-finite"):
        adapter.dispatch(noartifact_kernel, [1.0], math.inf)
    with pytest.raises(ValueError, match="2\\*\\*53"):
        adapter.dispatch(noartifact_kernel, [1.0], 2**53 + 1)
    with pytest.raises(TypeError):
        adapter.dispatch(noartifact_kernel, [object()], 1.0)


def test_browser_dispatch_and_result_reconstructed_from_bits(adapter, artifact_kernel):
    p = adapter.dispatch(artifact_kernel, [3.0, 4.0], -0.0)
    assert p["result"] is None and p["bundle"].startswith("/gradio_api/file=")
    assert p["bundle"].endswith("rms_gain.js") and " " not in p["bundle"]
    b = binding_of(artifact_kernel)
    assert b.python_calls == 0
    # what the component would write: the JSON `value` is 0 (corrupted by JSON.stringify(-0)),
    # the bits are authoritative
    client = hop({**p, "result": {"value": 0, "bits": float_bits(-0.0), "path": "browser-wasm", "python_calls": 0,
                                  "wasm_fetched": ["x.wasm"]}})
    r = adapter.result_of(artifact_kernel, client)
    assert r["path"] == "browser-wasm" and r["python_calls"] == 0
    assert math.copysign(1.0, r["value"]) < 0 and r["value"] == 0.0


def test_python_calls_marker_is_server_side_truth(adapter, artifact_kernel):
    """If the SERVER ran the kernel between dispatch and result, the marker says so no
    matter what the client claims; a run that happened BEFORE this dispatch is not counted.
    M1.5: a direct call of an artifact-bound kernel runs the in-process WASM when wasmtime
    is installed (mode 'server') and the Python body otherwise (mode 'browser'); the RED
    marker is `python_calls` in the one world and `server_calls` in the other -- asserted
    by the binding's resolved mode, never by "whichever is nonzero"."""
    from pythscribe import binding_of

    mode = binding_of(artifact_kernel).mode
    assert mode in ("server", "browser"), mode
    marker = "server_calls" if mode == "server" else "python_calls"
    other = "python_calls" if mode == "server" else "server_calls"
    artifact_kernel([1.0], 1.0)  # an unrelated earlier in-process call
    p = adapter.dispatch(artifact_kernel, [3.0, 4.0], 10.0)
    client = hop({**p, "result": {"value": 1.0, "bits": float_bits(1.0), "path": "browser-wasm", "python_calls": 0}})
    r = adapter.result_of(artifact_kernel, client)
    assert r["python_calls"] == 0 and r["server_calls"] == 0
    artifact_kernel([3.0, 4.0], 10.0)  # the server DID run it after dispatch
    r = adapter.result_of(artifact_kernel, client)
    assert r[marker] == 1 and r[other] == 0, (mode, r)  # RED marker, despite client's 0


def test_python_calls_marker_under_explicit_browser_mode(monkeypatch, tmp_path, demo_artifact):
    """The other world, forced: PYTHSCRIBE_MODE=browser makes a direct call run the Python
    body even with wasmtime installed, so `python_calls` is the RED marker."""
    import pythscribe.gradio as G
    from pythscribe import binding_of

    monkeypatch.setenv("PYTHSCRIBE_MODE", "browser")
    d = tmp_path / "art_browser"
    d.mkdir()
    shutil.copyfile(DEMO_DIR / "kernels.py", d / "kernels.py")
    shutil.copytree(demo_artifact.dir, d / "__pythscribe__" / "rms_gain")
    fn = import_module_from(d / "kernels.py").rms_gain
    assert binding_of(fn).mode == "browser"
    p = G.dispatch(fn, [3.0, 4.0], 10.0)
    assert p["bundle"] is not None  # the adapter still sends it to the tab
    client = hop({**p, "result": {"value": 1.0, "bits": float_bits(1.0), "path": "browser-wasm", "python_calls": 0}})
    fn([3.0, 4.0], 10.0)
    r = G.result_of(fn, client)
    assert r["python_calls"] == 1 and r["server_calls"] == 0


def test_browser_error_reruns_python_on_dispatched_args_not_client_args(adapter, artifact_kernel):
    p = adapter.dispatch(artifact_kernel, [3.0, 4.0], 10.0)
    tampered = hop({**p, "args": ["a", "b", "c"], "error": "import failed"})
    r = adapter.result_of(artifact_kernel, tampered)
    assert r["path"] == "python-fallback" and r["browser_error"] == "import failed"
    assert adapter.float_from_bits(r["bits"]) == 10.0 / ((25.0 / 2) ** 0.5)
    assert r["python_calls"] == 1


def test_malformed_bits_and_unknown_nonce_never_yield_a_wrong_value(adapter, artifact_kernel):
    p = adapter.dispatch(artifact_kernel, [3.0, 4.0], 10.0)
    for bits in ("zz", "  00000000000000", "0000 0000 000000", "zz00000000000000", None, 12, "0" * 15):
        bad = hop({**p, "result": {"value": 42.0, "bits": bits, "path": "browser-wasm", "python_calls": 0}})
        r = adapter.result_of(artifact_kernel, bad)  # must never raise (R2/S1: struct.error escaped)
        assert r["path"] == "python-fallback" and "malformed" in r["browser_error"], bits
        assert r["value"] != 42.0
    unknown = hop({**p, "nonce": 10**9, "result": {"value": 42.0, "bits": float_bits(42.0), "path": "browser-wasm", "python_calls": 0}})
    r2 = adapter.result_of(artifact_kernel, unknown)
    assert r2["path"] == "unknown" and r2["value"] is None


def test_bundle_url_is_percent_encoded(adapter, tmp_path, demo_artifact):
    weird = tmp_path / "c#sharp dir"
    weird.mkdir()
    shutil.copyfile(DEMO_DIR / "kernels.py", weird / "kernels.py")
    shutil.copytree(demo_artifact.dir, weird / "__pythscribe__" / "rms_gain")
    fn = import_module_from(weird / "kernels.py").rms_gain
    url = adapter.bundle_url(fn)
    assert "#" not in url and " " not in url
    assert unquote(url).endswith("c#sharp dir/__pythscribe__/rms_gain/rms_gain.js")


def test_nonce_is_bound_to_the_function_that_dispatched_it(adapter, tmp_path):
    """Review R2/S4: a nonce dispatched for kernel f must not be answered with kernel g's
    result nor re-run g on f's arguments."""
    d = tmp_path / "two"
    d.mkdir()
    (d / "kernels.py").write_text(
        "from pythscribe import wasm\n\n@wasm\ndef f(x: float) -> float:\n    return x * 2.0\n\n"
        "@wasm\ndef g(x: float) -> float:\n    return x + 1000.0\n",
        encoding="utf-8",
    )
    mod = import_module_from(d / "kernels.py")
    p = adapter.dispatch(mod.f, 3.0)
    assert p["result"]["path"] == "python-fallback"
    r = adapter.result_of(mod.g, hop({**p, "result": {"value": 7.0, "bits": float_bits(7.0), "path": "browser-wasm", "python_calls": 0}}))
    assert r["path"] == "unknown" and r["value"] is None
    r2 = adapter.result_of(mod.g, hop({**p, "result": None, "error": "boom"}))
    assert r2["path"] == "unknown"
    assert binding_of(mod.g).python_calls == 0  # g never ran


def test_deadline_falls_back_when_browser_never_answers(adapter, artifact_kernel):
    """Review R2/S8: a dispatched browser-path call with no result within the deadline is
    answered by Python on the dispatched args; a completed one is left alone."""
    p = adapter.dispatch(artifact_kernel, [3.0, 4.0], 10.0)
    r = adapter.deadline_result(artifact_kernel, p, wait_s=0.3)
    assert r["path"] == "python-fallback" and r["browser_error"].startswith("timeout")
    assert adapter.float_from_bits(r["bits"]) == 10.0 / ((25.0 / 2) ** 0.5) and r["python_calls"] == 1
    assert adapter.deadline_result(artifact_kernel, p, wait_s=0.1) is None  # already completed
    p2 = adapter.dispatch(artifact_kernel, [3.0, 4.0], 10.0)
    client = hop({**p2, "result": {"value": 2.8, "bits": float_bits(2.8), "path": "browser-wasm", "python_calls": 0}})
    adapter.result_of(artifact_kernel, client)  # the browser answered
    assert adapter.deadline_result(artifact_kernel, p2, wait_s=0.1) is None


def test_non_float_result_is_refused_at_the_adapter_boundary(adapter, tmp_path):
    d = tmp_path / "nf"
    d.mkdir()
    (d / "kernels.py").write_text("from pythscribe import wasm\n\n@wasm\ndef h(x: float) -> float:\n    return [x]\n", encoding="utf-8")
    mod = import_module_from(d / "kernels.py")
    p = adapter.dispatch(mod.h, 1.0)
    assert p["result"]["error"].startswith("TypeError") and p["result"]["value"] is None
    json.dumps(p)  # JSON-safe
