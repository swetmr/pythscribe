"""#495 — the Gradio adapter's `dispatch` shares the ONE-authority crossed-return-type gate with
the Streamlit adapter (`pythscribe.gradio._require_scalar_float_return`, the same helper M3 gave
Streamlit; test_streamlit_adapter.py::test_u10 is the Streamlit twin).

A `-> None`/`-> int`/`-> bool` @wasm kernel must be REFUSED at `dispatch` — BEFORE the artifact /
browser-vs-fallback branch — so the browser path can no longer silently yield `nan` (`-> None`) or a
float (`-> int`/`-> bool`) while the Python-fallback path raises TypeError. That asymmetric
silent-wrong-value class (browser vs fallback) IS the bug; closing it at the ONE shared boundary
makes the M0 "must return float" contract path-INDEPENDENT.

Framework-free: `import pythscribe.gradio` lazy-imports Gradio, so these run without Gradio installed
(exactly like the Streamlit twin runs without Streamlit)."""
from __future__ import annotations

import pytest

import pythscribe.gradio as G
from pythscribe import binding_of

_NONFLOAT = {
    "int": "from pythscribe import wasm\n@wasm\ndef k(x: int) -> int:\n    return x\n",
    "None": "from pythscribe import wasm\n@wasm\ndef k(x: int) -> None:\n    return None\n",
    "bool": "from pythscribe import wasm\n@wasm\ndef k(x: int) -> bool:\n    return x > 0\n",
}
_FLOAT = "from pythscribe import wasm\n@wasm\ndef k(x: float) -> float:\n    return x * 2.0\n"


@pytest.mark.parametrize("ann", list(_NONFLOAT))
def test_495_gradio_dispatch_refuses_non_float_return(import_source, ann):
    """POSITIVE + PAIRED CONTROL. A non-float @wasm kernel is refused at `dispatch`.

    The kernel has NO artifact, so the FALLBACK arm would otherwise run (`_python_result`); the
    guard fires BEFORE that branch, so the SAME refusal covers the browser arm too — no more
    browser-`nan`/float vs fallback-TypeError asymmetry.

    Mutant control: delete the `_require_scalar_float_return(b)` call in `dispatch` and this
    returns a payload (no raise) -> the test goes RED. That is the exact silent-wrong-value bug."""
    mod = import_source(_NONFLOAT[ann], stem=f"g_nonfloat_{ann}")
    assert binding_of(mod.k).artifact is None  # the fallback arm would otherwise produce a result
    with pytest.raises(TypeError, match="must return float"):
        G.dispatch(mod.k, 3)


def test_495_gradio_dispatch_admits_float_return(import_source):
    """OVER-REFUSAL CONTROL. A legitimate `-> float` kernel is NOT refused — `dispatch` returns a
    payload — so the gate discriminates by return type; it does not refuse everything (which would
    make the positive test vacuously green)."""
    mod = import_source(_FLOAT, stem="g_float_ok")
    p = G.dispatch(mod.k, 3.0)
    assert isinstance(p, dict) and p["fn"] == "k"
    # no artifact -> the fallback produced the result server-side; it is the float body, exactly.
    assert p["result"] is not None and p["result"]["path"] == "python-fallback"
    assert p["result"]["value"] == 6.0


def test_495_shared_gate_is_one_authority():
    """The Streamlit adapter reuses the SAME helper object (ONE authority, not a divergent copy)."""
    import pythscribe.streamlit as S

    assert S._require_scalar_float_return is G._require_scalar_float_return
