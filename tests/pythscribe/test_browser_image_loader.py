"""v0.2.6 fix B -- `pythscribe.gradio.browser`: the `js=` image loader. No browser, no server:
the spec is derived from the kernels' statically-read signatures and the artifact URL, and the
contract is REFUSED (never guessed) for a kernel outside it."""
from __future__ import annotations

import json
import re

import pytest

from conftest import REPO, gate, gate_import, import_module_from

gate_import("gradio")  # `wasm_url` registers a Gradio static path

USE_CASES = REPO / "examples" / "wasm-use-cases"

from pythscribe.ffi import SHIM  # noqa: E402
from pythscribe.gradio.browser import CLIENT_JS, browser_image_loader_js, image_kernel_spec  # noqa: E402


@pytest.fixture(scope="module")
def kernels():
    for fn in ("threshold_lum", "sobel"):
        assert (USE_CASES / "__pythscribe__" / fn / "manifest.json").is_file(), f"build examples/wasm-use-cases/kernels.py ({fn})"
    mod = import_module_from(USE_CASES / "kernels.py", "kernels_uc_for_browser_loader")  # by path: never shadow the M1 `kernels`
    return mod.threshold_lum, mod.sobel


def test_spec_roles_and_extras(kernels):
    threshold_lum, sobel = kernels
    s = image_kernel_spec(threshold_lum)
    assert s["fn"] == "threshold_lum"
    assert s["param_types"] == ["Array[uint8, 2]", "Array[uint8, 2]", "int", "int", "int"] and s["return_type"] == "int"
    assert s["roles"] == {"img": 0, "out": 1, "h": 2, "w": 3} and s["extras"] == [4] and s["extra_names"] == ["thr"]
    assert s["wasm"].startswith("/gradio_api/file=") and s["wasm"].endswith("/threshold_lum.wasm")
    s2 = image_kernel_spec(sobel)
    assert s2["roles"] == {"img": 0, "out": 1, "h": 2, "w": 3} and s2["extras"] == []


def test_spec_optional_dims_roles_downscale_nn():
    mod = import_module_from(REPO / "examples" / "gradio-image-preprocess" / "kernels.py", "m1_kernels_for_browser_loader")
    s = image_kernel_spec(mod.downscale_nn)
    assert s["roles"] == {"img": 0, "oh": 2, "ow": 3, "out": 4} and s["extras"] == [1] and s["extra_names"] == ["scale"]  # (img, scale, oh, ow, out)


def _wasm(src: str, name: str):
    """A kernel whose SOURCE is read statically by the decorator (needs a real file)."""
    import tempfile
    import textwrap
    from pathlib import Path

    f = Path(tempfile.mkdtemp()) / f"{name}_mod.py"
    f.write_text("from __future__ import annotations\nfrom pythscribe import wasm\n" + textwrap.dedent(src), encoding="utf-8")
    return getattr(import_module_from(f), name)


@pytest.mark.parametrize("src,name,msg", [
    ("@wasm\ndef k(img: Array[uint8, 2], out: Array[uint8, 2], h: int, w: int) -> float:\n    return 1.0\n", "k", "returns `int`"),
    ("@wasm\ndef k(img: Array[uint8, 2], out: Array[uint8, 2], h: float, w: int) -> int:\n    return 1\n", "k", "parameter `h` must be `int`"),
    ("@wasm\ndef k(img: Array[uint8, 2], out: Array[uint8, 2], mask: Array[uint8, 2], h: int, w: int) -> int:\n    return 1\n", "k", "has no role"),
    ("@wasm\ndef k(img: Array[uint8, 2], h: int, w: int) -> int:\n    return 1\n", "k", "missing required parameter(s) ['out']"),
    ("@wasm\ndef k(img: Array[uint8, 2], out: Array[uint8, 2], oh: int) -> int:\n    return 1\n", "k", "`oh` and `ow` must be given together"),
    ("@wasm\ndef k(img: list[int], out: list[int], h: int, w: int) -> int:\n    return 1\n", "k", "parameter `img` must be `Array[uint8, 2]`"),
    ("@wasm\ndef k(img: Array[uint8, 2], out: Array[uint8, 2], h: int, w: int, lut: list[int]) -> int:\n    return 1\n", "k", "is not a scalar"),
    # the mutation set (opus r1/#10): `img` written -> refused (it is never read back); `out` never written -> refused
    ("@wasm\ndef k(img: Array[uint8, 2], out: Array[uint8, 2], h: int, w: int) -> int:\n    img[0][0] = 1\n    out[0][0] = 1\n    return 1\n", "k", "the kernel mutates `img`"),
    ("@wasm\ndef k(img: Array[uint8, 2], out: Array[uint8, 2], h: int, w: int) -> int:\n    return h * w\n", "k", "never writes `out`"),
    # a reserved role name used as a tunable of another type is refused, never silently captured (opus r1/#11)
    ("@wasm\ndef k(img: Array[uint8, 2], out: Array[uint8, 2], w: float) -> int:\n    out[0][0] = 1\n    return 1\n", "k", "`w` is a RESERVED role name"),
])
def test_contract_refusals(src, name, msg):
    fn = _wasm(src, name)
    with pytest.raises(TypeError, match=re.escape(msg)):
        image_kernel_spec(fn)


def test_loader_js_inlines_shim_client_and_spec(kernels):
    threshold_lum, sobel = kernels
    js = browser_image_loader_js({"threshold": threshold_lum, "sobel": sobel}, global_name="imgClient")
    assert js.startswith("async () => {") and js.rstrip().endswith("}")
    m = re.search(r'ARRAY_LAYOUT_VERSION\s*=\s*"([^"]+)"', SHIM.read_text(encoding="utf-8"))
    assert m and json.dumps(m.group(1))[1:-1] in js, "the shim's source (with its array layout version) must be inlined"
    assert "function makeImageClient(ffi, spec)" in js and CLIENT_JS.is_file()
    assert "window.imgClient = makeImageClient(ffi, " in js and "window.imgClient.ready = true;" in js
    call = js.rindex("makeImageClient(ffi, ") + len("makeImageClient(ffi, ")  # the CALL (the definition comes first)
    spec = json.loads(js[call:js.index(");\n    window.imgClient.ready", call)])
    assert set(spec) == {"threshold", "sobel"} and spec["threshold"]["fn"] == "threshold_lum"
    # the stub is published SYNCHRONOUSLY before the first await (an early handler gets 'loading', not a TypeError),
    # a load failure lands in loadError, and the blob URL is revoked (opus r1/#7, #9)
    stub = js.index("window.imgClient = { ready: false, loadError: null")
    assert stub < js.index("await import(__shimUrl)") and "loadError: msg" in js and "URL.revokeObjectURL(__shimUrl)" in js
    # the escaping invariants: the EMITTED hook never contains `</script>` (covers the shim + spec, which
    # cross as JSON string literals -- json.dumps does NOT escape `/` -- and our own inlined client file),
    # and no raw U+2028/2029 (JSON ASCII-escaping)
    assert "</script>" not in js.lower()
    assert "\u2028" not in js and "\u2029" not in js


def test_loader_refuses_bad_inputs(kernels):
    threshold_lum, _ = kernels
    with pytest.raises(ValueError):
        browser_image_loader_js({})
    with pytest.raises(ValueError):
        browser_image_loader_js({"threshold": threshold_lum}, global_name="not a name")
    with pytest.raises(ValueError):
        browser_image_loader_js({"bad key!": threshold_lum})


def test_loader_refuses_a_kernel_without_artifact(monkeypatch):
    """No silent fallback: a kernel with no usable artifact is refused at app-build time."""
    from pythscribe import binding_of
    from pythscribe.decorators import NO_JIT_ENV

    monkeypatch.setenv(NO_JIT_ENV, "1")  # the documented opt-out: compile-on-first-call stays off
    fn = _wasm("@wasm\ndef nk(img: Array[uint8, 2], out: Array[uint8, 2], h: int, w: int) -> int:\n    out[0][0] = 1\n    return h * w\n", "nk")
    b = binding_of(fn)
    assert b.artifact is None and b.artifact_status == "absent", (b.artifact, b.artifact_status)
    with pytest.raises(RuntimeError, match="no usable artifact"):
        image_kernel_spec(fn)
    assert b.artifact is None, "the refusal must not have compiled anything"


# ---- kernel semantics vs the NumPy references on RANDOM images (no browser: the SAME .wasm on the
# server path). Closes the "uint8 intermediate wraps where NumPy's int64 does not" arm on inputs the
# shipped photo might mask (saturated channels, |gx|+|gy| > 255, sums > 255). GATED, never skipped
# silently: under PYTHSCRIBE_REQUIRE_ORACLE a missing wasmtime goes RED (opus r1/#2).
@pytest.fixture(scope="module")
def probe_refs():
    return import_module_from(USE_CASES / "browser_image_probe.py", "browser_image_probe_for_loader")


@pytest.fixture(scope="module")
def downscale_nn():
    return import_module_from(REPO / "examples" / "gradio-image-preprocess" / "kernels.py", "m1_kernels_for_random_diff").downscale_nn


def _server_mode(fn):
    from pythscribe import binding_of

    b = binding_of(fn)
    b.ensure_compiled()
    return b.mode == "server"


@pytest.mark.parametrize("seed", range(6))
def test_kernels_match_numpy_references_on_random_images(kernels, probe_refs, downscale_nn, seed):
    np = gate_import("numpy")
    threshold_lum, sobel = kernels
    gate(_server_mode(threshold_lum) and _server_mode(sobel) and _server_mode(downscale_nn),
         "wasmtime server path unavailable (the kernels would run as plain Python here)")
    rng = np.random.default_rng(seed)
    h, w = int(rng.integers(3, 40)), int(rng.integers(3, 40))
    rgb = rng.integers(0, 256, size=(h, w, 3), dtype=np.uint8)
    if seed % 2:  # force saturated / extreme neighbourhoods
        rgb[: h // 2] = 255
        rgb[h // 2 :, : w // 2] = 0
    im = np.ascontiguousarray(rgb).reshape(h, w * 3)
    for thr in (0, 128, 383, 764, 765):
        out = np.zeros_like(im)
        assert threshold_lum(im, out, h, w, thr) == h * w
        np.testing.assert_array_equal(out, probe_refs.ref_threshold(rgb, thr), err_msg=f"threshold {thr}")
    out = np.zeros_like(im)
    assert sobel(im, out, h, w) == (h - 2) * (w - 2)
    np.testing.assert_array_equal(out, probe_refs.ref_sobel(rgb))
    for s in (1, 2, 3):
        oh, ow = h // s, w // s
        out = np.zeros((oh, ow * 3), dtype=np.uint8)
        assert downscale_nn(im, s, oh, ow, out) == oh * ow
        np.testing.assert_array_equal(out, probe_refs.ref_downscale_nn(rgb, s), err_msg=f"downscale {s}")
    from pythscribe import binding_of

    for fn in (threshold_lum, sobel, downscale_nn):
        assert binding_of(fn).server_calls > 0 and binding_of(fn).python_calls == 0, f"{fn.__name__}: the WASM server path must have run (not the Python body)"
