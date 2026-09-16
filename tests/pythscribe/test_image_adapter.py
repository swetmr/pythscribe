"""The image adapter's trust boundary (pythscribe.gradio.image), unit-tested in-process:
  T1 a client-named file path OUTSIDE Gradio's upload folder is refused before anything
     touches it (opus r1/B1 -- Gradio's own check is inert under mount_gradio_app);
  T2 the app refuses to decode a file inside the folder whose content the byte meter never
     saw arrive through /upload (the second half of the same authority);
  T3 a dispatch answers exactly once: a replayed result is refused (opus r1/S7);
  T4 the FFI signature grammar refuses keyword-only / *args / **kwargs (and accepts positional
     params with defaults, every argument being supplied) instead of silently truncating the
     parameter list (opus r1/B3, codex r2);
  T5 non-finite float returns cross the Node runner exactly (never flattened to null).
"""
from __future__ import annotations

import json
import os
import sys
import tempfile
from pathlib import Path

import pytest

from conftest import REPO, gate_import, gate_node, import_module_from
from pythscribe.build import build_module
from pythscribe.ffi import FfiError, run_kernel, signature_of

IMG_DIR = REPO / "examples" / "gradio-image-preprocess"


@pytest.fixture(scope="module")
def image_mod():
    gate_import("gradio")
    from pythscribe.gradio import image

    return image


@pytest.fixture(scope="module")
def kernels():
    return import_module_from(IMG_DIR / "kernels.py", "kernels_image_adapter_tests")


def _payload(image_mod, kernels):
    return image_mod.dispatch_image(kernels.downscale_box, kernels.box_scale, max_dim=512, quality=0.85)


def test_t1_file_outside_upload_folder_is_refused(image_mod, kernels, tmp_path):
    outside = tmp_path / "secret.png"
    outside.write_bytes(b"\x89PNG not really")
    assert not image_mod.file_path_allowed(str(outside))
    p = _payload(image_mod, kernels)
    p["result"] = {"path": "browser-wasm", "file": {"path": str(outside), "meta": {"_type": "gradio.FileData"}}, "in_w": 1, "in_h": 1, "scale": 1}
    r = image_mod.image_result_of(kernels.downscale_box, p)
    assert r["path"] == "error" and r["file"] is None
    assert "outside Gradio's upload folder" in r["error"]
    # a nonexistent path and a directory are refused the same way
    assert not image_mod.file_path_allowed(str(tmp_path / "nope.png"))
    assert not image_mod.file_path_allowed(str(tmp_path))
    # and a file INSIDE the folder passes this first gate
    from gradio import utils as gu

    inside_dir = Path(gu.get_upload_folder()) / "pythscribe-test"
    inside_dir.mkdir(parents=True, exist_ok=True)
    inside = inside_dir / "ok.bin"
    inside.write_bytes(b"x")
    try:
        assert image_mod.file_path_allowed(str(inside))
    finally:
        inside.unlink(missing_ok=True)


def test_t2_app_refuses_a_file_the_meter_never_saw(image_mod, kernels, monkeypatch):
    """A path inside the upload folder that was never uploaded through /upload (so the byte
    meter has no record of its content) must not be decoded: path='error', no image."""
    gate_import("gradio_wasmfunction")
    sys.path.insert(0, str(IMG_DIR))
    app = import_module_from(IMG_DIR / "app.py", "app_image_adapter_tests")
    app.build_app()  # installs the meter (empty)
    from gradio import utils as gu

    d = Path(gu.get_upload_folder()) / "pythscribe-test"
    d.mkdir(parents=True, exist_ok=True)
    f = d / "never_uploaded.jpg"
    from PIL import Image

    Image.new("RGB", (8, 8), (10, 20, 30)).save(f, format="JPEG")
    try:
        p = app.fresh_payload()
        p["result"] = {"path": "browser-wasm", "file": {"path": str(f), "meta": {"_type": "gradio.FileData"}},
                       "in_w": 8, "in_h": 8, "scale": 1, "out_w": 8, "out_h": 8, "checksum": 0}
        img, text, fresh = app.on_client(p, None)
        rec = json.loads(text)
        assert rec["path"] == "error" and rec["error"].startswith("refused before decoding:"), rec
        assert "no /upload request with this content" in rec["error"]
        assert rec["upload_measured"] is False
        assert "out_w" not in rec  # nothing was decoded
        assert isinstance(fresh, dict) and fresh["nonce"] != p["nonce"]
    finally:
        f.unlink(missing_ok=True)


def test_t7_lying_client_is_marked_inconsistent(image_mod, kernels, tmp_path):
    """opus r2/NS-6: a client that uploads a small JPEG but claims a 4000x3000 input at scale 8
    gets `consistent: False` from the SERVER (which decoded 8x8), and `summarize` then refuses
    the record; an honest claim gets True."""
    gate_import("gradio_wasmfunction")
    sys.path.insert(0, str(IMG_DIR))
    app = import_module_from(IMG_DIR / "app.py", "app_lying_client_tests")
    app.build_app()
    from gradio import utils as gu
    from PIL import Image

    d = Path(gu.get_upload_folder()) / "pythscribe-test"
    d.mkdir(parents=True, exist_ok=True)
    f = d / "small_uploaded.jpg"
    Image.new("RGB", (8, 8), (10, 20, 30)).save(f, format="JPEG")
    try:
        claims = {"lying": {"in_w": 4000, "in_h": 3000, "scale": 8, "out_w": 500, "out_h": 375},
                  "honest": {"in_w": 8, "in_h": 8, "scale": 1, "out_w": 8, "out_h": 8}}
        got = {}
        for name, claim in claims.items():
            _fake_upload(app.METER, b"x" * 1234, [str(f)])  # the meter SAW the content arrive
            p = app.fresh_payload()
            p["result"] = {"path": "browser-wasm", "file": {"path": str(f), "meta": {"_type": "gradio.FileData"}}, "checksum": 0, **claim}
            _, text, _ = app.on_client(p, None)
            got[name] = json.loads(text)
        assert got["lying"]["path"] == "browser-wasm" and got["lying"]["consistent"] is False
        assert (got["lying"]["out_w"], got["lying"]["out_h"]) == (8, 8) and got["lying"]["client"]["in_w"] == 4000
        assert got["honest"]["consistent"] is True and got["honest"]["upload_bytes"] == 1234
        # a MULTI-FILE upload the meter saw: the handler DECODES it (path stays browser-wasm, the
        # server's out dims are real) but its bytes are unmeasured (codex r3/r4)
        g = d / "other.jpg"
        Image.new("RGB", (4, 4), (1, 2, 3)).save(g, format="JPEG")
        try:
            _fake_upload(app.METER, b"x" * 3500, [str(f), str(g)])
            p = app.fresh_payload()
            p["result"] = {"path": "browser-wasm", "file": {"path": str(f), "meta": {"_type": "gradio.FileData"}}, "checksum": 0, **claims["honest"]}
            _, text, _ = app.on_client(p, None)
            multi = json.loads(text)
        finally:
            g.unlink(missing_ok=True)
        assert multi["path"] == "browser-wasm" and (multi["out_w"], multi["out_h"]) == (8, 8)  # decoded
        assert multi["upload_seen"] is True and multi["upload_measured"] is False and multi["upload_bytes"] is None
        assert "several files" in multi["upload_unmeasured_reason"]
        import metrics_lib as M

        naive = {"image": "x", "mode": "naive", "path": "server-pillow", "upload_bytes": 5000, "upload_measured": True, "wire_bytes": 5500,
                 "wire_measured": True, "join_bytes": 500, "in_w": 8, "in_h": 8, "scale": 1, "checksum": 0, "server_cpu_ms": 1, "server_resize_ms": 1, "server_decode_ms": 1}
        lying = dict(got["lying"], image="x")
        with pytest.raises(ValueError, match="inconsistent"):
            M.summarize([lying, naive])
    finally:
        f.unlink(missing_ok=True)


def test_t3_replayed_result_is_refused(image_mod, kernels):
    p = _payload(image_mod, kernels)
    p["error"] = "simulated browser failure"  # an error with no file -> completes the nonce
    r1 = image_mod.image_result_of(kernels.downscale_box, p)
    assert r1["path"] == "error"
    r2 = image_mod.image_result_of(kernels.downscale_box, p)
    assert r2["path"] == "replay" and "already answered" in r2["error"]
    # a RESULTLESS echo of the answered nonce (the component re-rendering the same payload) is
    # simply idle -- never a spurious error record (opus r2/NS-8, paired per r3)
    idle = {**p, "error": None, "result": None}
    assert image_mod.image_result_of(kernels.downscale_box, idle) is None


def test_t4_signature_grammar_refuses_non_positional_params(import_source):
    from pythscribe import binding_of

    kw = import_source("from pythscribe import wasm\n\n@wasm\ndef kwf(a: float, *, b: float = 2.0) -> float:\n    return a + b\n")
    assert [n for n, _ in binding_of(kw.kwf).params] == ["a", "b"]  # every param is SEEN
    with pytest.raises(FfiError, match="keyword-only"):
        signature_of(kw.kwf)
    va = import_source("from pythscribe import wasm\n\n@wasm\ndef vaf(a: float, *rest: float) -> float:\n    return a\n")
    assert [n for n, _ in binding_of(va.vaf).params] == ["a", "*rest"]
    with pytest.raises(FfiError):
        signature_of(va.vaf)
    kwa = import_source("from pythscribe import wasm\n\n@wasm\ndef kwa(a: float, **opts: float) -> float:\n    return a\n")
    assert [n for n, _ in binding_of(kwa.kwa).params] == ["a", "**opts"]
    with pytest.raises(FfiError):
        signature_of(kwa.kwa)
    # a positional param WITH a default is callable by position with every argument supplied:
    # accepted (codex r2), and the shim's exact-arity check still demands the full list
    dflt = import_source("from pythscribe import wasm\n\n@wasm\ndef df(a: float, b: float = 1.0) -> float:\n    return a + b\n")
    assert signature_of(dflt.df) == (["float", "float"], "float")
    ok = import_source("from pythscribe import wasm\n\n@wasm\ndef good(a: float, b: int) -> float:\n    return a\n")
    assert signature_of(ok.good) == (["float", "int"], "float")


def _fake_upload(meter, body: bytes, saved_paths: list[str]):
    """Drive the ASGI meter with a synthetic /gradio_api/upload request whose downstream app
    'saves' `saved_paths` and answers with their JSON list."""
    import asyncio

    async def downstream(scope, receive, send):
        while True:
            m = await receive()
            if not m.get("more_body"):
                break
        await send({"type": "http.response.start", "status": 200, "headers": []})
        await send({"type": "http.response.body", "body": json.dumps(saved_paths).encode(), "more_body": False})

    meter.app = downstream
    msgs = [{"type": "http.request", "body": body[: len(body) // 2], "more_body": True}, {"type": "http.request", "body": body[len(body) // 2:], "more_body": False}]
    sent = []

    async def receive():
        return msgs.pop(0)

    async def send(m):
        sent.append(m)

    asyncio.run(meter({"type": "http", "path": "/gradio_api/upload", "method": "POST"}, receive, send))
    return sent


def test_t6_byte_meter_attributes_in_upload_order_expires_and_refuses_multi_file(tmp_path):
    """The meter's contract (codex r2 RED controls): identical content uploaded twice with
    different body sizes is handed out in upload order (never last-writer-wins); an abandoned
    record expires; a multi-file request is UNMEASURED for every file (never charged twice)."""
    gate_import("gradio")
    sys.path.insert(0, str(IMG_DIR))
    app = import_module_from(IMG_DIR / "app.py", "app_meter_tests")
    meter = app.BytesMeter(None)
    f = tmp_path / "same.bin"
    f.write_bytes(b"identical content")
    _fake_upload(meter, b"x" * 1300, [str(f)])
    _fake_upload(meter, b"x" * 1450, [str(f)])
    first, second = meter.consume(f), meter.consume(f)
    assert (first["upload_bytes"], second["upload_bytes"]) == (1300, 1450)  # FIFO, both distinct
    assert meter.consume(f) is None  # consumed: a third handler gets nothing, not a stale reading
    # an abandoned upload expires instead of being handed to a later identical upload
    _fake_upload(meter, b"x" * 999, [str(f)])
    meter.by_sha[app.sha256_file(f)][0]["t"] -= app.BytesMeter.RECORD_TTL_S + 1  # (monotonic timestamps)
    _fake_upload(meter, b"x" * 2000, [str(f)])
    assert meter.consume(f)["upload_bytes"] == 2000
    # a request that carried two files is not attributable to either
    g = tmp_path / "other.bin"
    g.write_bytes(b"other")
    _fake_upload(meter, b"x" * 3500, [str(f), str(g)])
    a, b = meter.consume(f), meter.consume(g)
    assert a["upload_bytes"] is None and b["upload_bytes"] is None and a["files_in_request"] == 2
    app.METER = meter
    _fake_upload(meter, b"x" * 3500, [str(f), str(g)])
    m = app._measured(str(f), 500)
    # SEEN by the meter (the handler may decode it) but its bytes are not attributable (codex r3)
    assert m["upload_seen"] is True and m["upload_measured"] is False and "several files" in m["upload_unmeasured_reason"] and m["wire_bytes"] is None
    m = app._measured(str(f), 500)  # nothing left for this content: not seen -> the handler refuses to decode
    assert m["upload_seen"] is False and "no /upload request" in m["upload_unmeasured_reason"]
    # an unknown event body length makes the WIRE figure unmeasured, never understated
    _fake_upload(meter, b"x" * 700, [str(f)])
    m = app._measured(str(f), None)
    assert m["upload_bytes"] == 700 and m["upload_measured"] is True and m["wire_bytes"] is None and m["wire_measured"] is False
    import metrics_lib as M

    rec_c = {"image": "i", "mode": "client", "path": "browser-wasm", "python_calls": 0, "consistent": True, "upload_bytes": 100, "upload_measured": True,
             "wire_bytes": None, "wire_measured": False, "join_bytes": None, "out_w": 1, "out_h": 1, "server_cpu_ms": 0, "server_resize_ms": 0, "server_decode_ms": 0,
             "client": {"checksum": 1, "ms": {"wasm_call_ms": 1, "total_ms": 1}}}
    rec_n = {"image": "i", "mode": "naive", "path": "server-pillow", "upload_bytes": 1000, "upload_measured": True, "wire_bytes": 1500, "wire_measured": True,
             "join_bytes": 500, "in_w": 2, "in_h": 2, "scale": 2, "checksum": 1, "server_cpu_ms": 1, "server_resize_ms": 1, "server_decode_ms": 1}
    s = M.summarize([rec_c, rec_n])
    assert s["rows"][0]["reduction_x"] == 10.0 and s["rows"][0]["wire_measured"] is False
    assert s["rows"][0]["wire_reduction_x"] != s["rows"][0]["wire_reduction_x"]  # NaN: not a number pretending to be exact
    assert s["aggregate"]["wire_measured"] is False


def test_t5_non_finite_float_returns_cross_exactly(tmp_path):
    gate_node()
    src = (
        "from pythscribe import wasm\n\n@wasm\ndef inff(a: float) -> float:\n    return a * 1e308 * 10.0\n\n"
        "@wasm\ndef nanf(a: float) -> float:\n    return a * 1e308 * 10.0 - a * 1e308 * 10.0\n"
    )
    (tmp_path / "k.py").write_text(src, encoding="utf-8")
    arts = {a.function: a for a in build_module(tmp_path / "k.py", quiet=True)}
    mod = import_module_from(tmp_path / "k.py", "nonfinite_kernels_for_tests")
    [r] = run_kernel(arts["inff"].wasm, "inff", ["float"], [{"args": [1.0]}], return_type="float")
    assert r["ok"] and r["value"] == float("inf") == mod.inff(1.0)
    [r] = run_kernel(arts["nanf"].wasm, "nanf", ["float"], [{"args": [1.0]}], return_type="float")
    assert r["ok"] and r["value"] != r["value"] and mod.nanf(1.0) != mod.nanf(1.0)  # NaN, on both arms
    assert r["value"] is not None
