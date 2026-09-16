"""v0.2.5 M1 demo: image preprocessing BEFORE upload, in the browser, by a `@wasm` kernel --
with the naive control (upload the full image, the server resizes) IN THE SAME APP, and the
request-body bytes that reached the server MEASURED by the server itself (an ASGI meter
counting the bytes it received; HTTP headers / TCP framing are not counted).

    pip install -e .[gradio]  &&  pip install -e pythscribe/gradio/wasm_function
    python -m pythscribe.build examples/gradio-image-preprocess/kernels.py   # explicit build (committed)
    python examples/gradio-image-preprocess/app.py

Positioning (plan §2): bandwidth + server CPU per request. Nothing here touches a model or a GPU.
Without a built artifact the client panel still works: the original is uploaded and the SAME
kernel runs in Python (path=python-fallback).

Trust boundary of every record (review r1/B5): top-level fields are SERVER-derived (what the
server received, measured, decoded, ran, or timed); everything the browser reported about
itself lives under `client` and is labelled as such. The one client field the server
promotes is `path`, and only after the server has established it: 'browser-wasm' requires
that the server did NOT run the kernel for this dispatch, that the received file decodes,
and that its dimensions are consistent with the dispatched max_dim.
"""
from __future__ import annotations

import collections
import hashlib
import io
import json
import os
import sys
import threading
import time
from pathlib import Path

import gradio as gr
import numpy as np
from fastapi import FastAPI
from PIL import Image

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import reference as R  # noqa: E402
from kernels import box_scale, downscale_box  # noqa: E402

from pythscribe import binding_of  # noqa: E402
from pythscribe.gradio import WasmFunction, dispatch_image, image_result_of, wasm_url  # noqa: E402

MAX_DIM = int(os.environ.get("PYTHSCRIBE_MAX_DIM", "512"))
QUALITY = float(os.environ.get("PYTHSCRIBE_JPEG_QUALITY", "0.85"))


def _measure_cpu_tick_ms() -> float | None:
    """The EMPIRICAL granularity of process_time() (15.625 ms on Windows, ~1 ms or finer
    elsewhere; `get_clock_info` reports a nominal 100 ns that the scheduler never delivers):
    spin until the counter moves twice and take the step (bounded to 0.25 s of one core, ONCE,
    lazily on the first record -- codex r2/r3). None if it could not be measured within the
    bound -- never a made-up 0. A "0.0" CPU reading in a record means "below one tick", not
    zero (opus r1/S4)."""
    deadline = time.perf_counter() + 0.25
    t0 = time.process_time()
    while time.process_time() == t0:
        if time.perf_counter() > deadline:
            return None
    t1 = time.process_time()
    while time.process_time() == t1:
        if time.perf_counter() > deadline:
            return None
    return (time.process_time() - t1) * 1e3


_cpu_tick: list = []
_cpu_tick_lock = threading.Lock()


def cpu_tick_ms() -> float | None:
    with _cpu_tick_lock:  # calibrate ONCE even under simultaneous first requests (codex r4)
        if not _cpu_tick:
            _cpu_tick.append(_measure_cpu_tick_ms())
        return _cpu_tick[0]


DOWN = binding_of(downscale_box)
SCALE = binding_of(box_scale)
WASM_URL = wasm_url(downscale_box)  # registers the artifact dir as a static path (None -> fallback)


# ----------------------------------------------------------------------------- byte meter
class BytesMeter:
    """Pure-ASGI wrapper: counts the request-body bytes of EVERY http request as received
    from the socket, and for `/gradio_api/upload` maps each saved file (by sha256 of its
    content, hashed BEFORE the response is released, i.e. before Gradio's background move)
    to the bytes of the request that carried it. Attribution is a per-hash FIFO that a
    handler CONSUMES (review r1/B3: identical content uploaded twice yields two records,
    handed out in upload order, never last-writer-wins), and a request that carried more
    than one file is recorded as UNMEASURED rather than charged to every file (r1/S5).
    This is the server-side truth the app reports as 'bytes uploaded'; the client's own
    size claim is shown beside it, never instead of it."""

    # an upload whose handler never came (the tab closed between /upload and the event) must
    # not be handed to a later identical upload: records expire (codex r2). Scope statement:
    # attribution is exact for the sequential single-session measurement the notebook runs;
    # concurrent identical uploads from several sessions are attributed in upload order.
    RECORD_TTL_S = 120.0

    def __init__(self, app):
        self.app = app
        self.by_sha: dict[str, collections.deque] = {}
        self.requests: collections.deque = collections.deque(maxlen=4096)
        self.lock = threading.Lock()

    async def __call__(self, scope, receive, send):
        if scope["type"] != "http":
            return await self.app(scope, receive, send)
        path = scope.get("path", "")
        counted = 0
        is_upload = path.rstrip("/").endswith("/upload")
        resp_chunks: list[bytes] = []

        async def rx():
            nonlocal counted
            m = await receive()
            if m["type"] == "http.request":
                counted += len(m.get("body", b""))
            return m

        async def tx(m):
            if is_upload and m["type"] == "http.response.body":
                resp_chunks.append(m.get("body", b""))
                if not m.get("more_body", False):
                    self._record_upload(counted, b"".join(resp_chunks))
            await send(m)

        try:
            await self.app(scope, rx, tx)
        finally:
            with self.lock:
                self.requests.append({"path": path, "method": scope.get("method"), "body_bytes": counted, "t": time.time()})

    def _record_upload(self, body_bytes: int, response: bytes) -> None:
        try:
            saved = json.loads(response.decode("utf-8"))
        except Exception:
            return
        if not isinstance(saved, list):
            return
        paths = [p for p in saved if isinstance(p, str)]
        for p in paths:
            try:
                sha = sha256_file(p)
            except OSError:
                continue
            rec = {"upload_bytes": body_bytes if len(paths) == 1 else None, "files_in_request": len(paths), "upload_path": p, "t": time.monotonic()}
            with self.lock:
                self.by_sha.setdefault(sha, collections.deque(maxlen=64)).append(rec)

    def consume(self, file_path: str | os.PathLike) -> dict | None:
        """The OLDEST unexpired, unconsumed upload record whose content equals `file_path`'s,
        or None."""
        try:
            sha = sha256_file(file_path)
        except OSError:
            return None
        now = time.monotonic()
        with self.lock:
            q = self.by_sha.get(sha)
            while q and now - q[0]["t"] > self.RECORD_TTL_S:
                q.popleft()  # abandoned upload: never attribute it to a later identical one
            if not q:
                return None
            return q.popleft()


def sha256_file(path: str | os.PathLike) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


METER: BytesMeter | None = None
RESULTS: collections.deque = collections.deque(maxlen=512)
_seq = [0]
_seq_lock = threading.Lock()


def _next_seq() -> int:
    with _seq_lock:
        _seq[0] += 1
        return _seq[0]


def _join_bytes(request: gr.Request | None) -> int | None:
    """Body bytes of the event request that carried the component values (the small JSON
    beside the upload). Reported separately AND folded into `wire_bytes` (r1/S4)."""
    try:
        cl = request.headers.get("content-length") if request is not None else None  # type: ignore[union-attr]
        return int(cl) if cl else None
    except (ValueError, AttributeError):
        return None


def _measured(path: str, join_bytes: int | None) -> dict:
    """Server-side byte accounting for one received file: `upload_bytes` = the request BODY
    bytes of the /upload that carried it (multipart framing included; HTTP headers/TCP framing
    excluded -- this is body accounting, not packet accounting); `wire_bytes` = upload body +
    event body, and it is UNMEASURED (None) rather than understated when either is unknown
    (codex r2: a chunked event without Content-Length must not read as 0)."""
    m = METER.consume(path) if METER is not None else None
    ub = m["upload_bytes"] if m else None
    multi = bool(m) and m["files_in_request"] != 1
    return {
        # `upload_seen`: the meter saw this content arrive through /upload -- the gate before the
        # server touches a client-named file. `upload_measured`: its bytes are attributable (one
        # file per request); a multi-file request is SEEN (decoded normally) but unmeasured (codex r3).
        "upload_seen": m is not None,
        "upload_bytes": ub,
        "upload_measured": ub is not None,
        "upload_unmeasured_reason": (None if ub is not None else
                                     ("the /upload request carried several files; bytes are not attributable to one file" if multi
                                      else "no /upload request with this content was seen in this process")),
        "join_bytes": join_bytes,
        "wire_bytes": (ub + join_bytes) if (ub is not None and join_bytes is not None) else None,
        "wire_measured": ub is not None and join_bytes is not None,
        "files_in_upload_request": m["files_in_request"] if m else None,
    }


def _record(rec: dict) -> dict:
    rec["seq"] = _next_seq()
    rec["t"] = time.time()
    RESULTS.append(rec)
    return rec


# ----------------------------------------------------------------------------- python fallback
def python_fallback_resize(path: str) -> tuple[np.ndarray, dict]:
    """The load-bearing fallback: the SAME kernels, run by CPython on the server (this is
    what runs when no artifact is built or the browser path failed). Timed like the control."""
    t0 = time.perf_counter()
    c0 = time.process_time()
    rgb = R.load_rgb(path)
    t1 = time.perf_counter()
    h, w = rgb.shape[:2]
    scale = SCALE.run_python(w, h, MAX_DIM)  # the PYTHON body, explicitly, and counted (M1.5: a bare call could take the server path)
    ow, oh = w // scale, h // scale
    px = R.pack(rgb).ravel().tolist()
    out = [0] * (ow * oh)
    n = DOWN.run_python(px, w, h, scale, out)  # the PYTHON body, explicitly, and counted
    small = R.unpack(np.asarray(out, dtype=np.int64), ow, oh)
    t2 = time.perf_counter()
    jpg = R.encode_jpeg(small, int(round(QUALITY * 100)))
    t3 = time.perf_counter()
    c1 = time.process_time()
    assert n == ow * oh
    return small, {
        "in_w": w, "in_h": h, "scale": scale, "out_w": ow, "out_h": oh,
        "server_decode_ms": (t1 - t0) * 1e3, "server_resize_ms": (t2 - t1) * 1e3,
        "server_encode_ms": (t3 - t2) * 1e3, "server_cpu_ms": (c1 - c0) * 1e3,
        "encoded_bytes": len(jpg), "checksum": R.checksum_rgb(small),
    }


# ----------------------------------------------------------------------------- handlers
_CLIENT_FIELDS = ("orig_name", "orig_bytes", "orig_type", "in_w", "in_h", "scale", "out_w", "out_h", "checksum",
                  "upload_bytes_client", "wasm_exports", "wasm_how", "heap_bytes", "layout_version", "user_agent", "browser_error")
_CLIENT_MS = ("decode_ms", "scale_ms", "pack_ms", "instantiate_ms", "wasm_call_ms", "unpack_ms", "encode_ms", "upload_ms", "total_ms")


def _client_block(r: dict) -> dict:
    """Everything the browser said about itself -- kept, labelled, never promoted."""
    blk = {k: r.get(k) for k in _CLIENT_FIELDS}
    blk["ms"] = {k: r.get(k) for k in _CLIENT_MS}
    blk["wasm_fetched"] = len(r.get("wasm_fetched") or [])
    blk["path_claimed"] = r.get("path")
    return blk


def fresh_payload() -> dict:
    return dispatch_image(downscale_box, box_scale, max_dim=MAX_DIM, quality=QUALITY)


def on_client(payload: dict | None, request: gr.Request):
    """`change` of the component: the browser uploaded either the WASM-resized JPEG
    (claimed path browser-wasm) or the original (upload-original -> Python fallback here).
    The server decides the recorded `path` from what IT did and received."""
    r = image_result_of(downscale_box, payload)
    if r is None:
        return gr.skip(), gr.skip(), gr.skip()
    if r.get("file") is None:
        rec = _record({"mode": "client", "path": "error", "error": r.get("error"), "python_calls": None, "client": _client_block(r)})
        return gr.skip(), json.dumps(rec, indent=2), fresh_payload()
    path = r["file"]["path"]  # already confined to Gradio's upload folder by image_result_of (opus r1/B1)
    join = _join_bytes(request)
    measured = _measured(path, join)
    if not measured["upload_seen"]:
        # the ONE authority before the server touches a client-named file: the byte meter must
        # have seen this exact content arrive through /upload in this process. A path that was
        # never uploaded is refused, never decoded (opus r1/B1).
        rec = _record({"mode": "client", "path": "error", "python_calls": 0, "nonce": r.get("nonce"), **measured,
                       "error": f"refused before decoding: {measured['upload_unmeasured_reason']}", "client": _client_block(r)})
        return gr.skip(), json.dumps(rec, indent=2), fresh_payload()
    rec: dict = {
        "mode": "client",
        "file_bytes": os.path.getsize(path),
        **measured,
        "nonce": r.get("nonce"),
        "max_dim": r.get("max_dim"),
        "cpu_timer_resolution_ms": cpu_tick_ms(),
        "client": _client_block(r),
    }
    ran_python_here = False
    if r["path"] == "browser-wasm":
        t0 = time.perf_counter()
        c0 = time.process_time()
        rgb = R.load_rgb(path)  # the SMALL image: all the server ever decodes on this path
        t1 = time.perf_counter()
        c1 = time.process_time()
        oh, ow = rgb.shape[:2]
        cb = rec["client"]
        # consistency of the client's story with what the server holds (never trusted, only checked)
        consistent = (
            max(ow, oh) <= int(r.get("max_dim") or MAX_DIM)
            and isinstance(cb.get("in_w"), int) and isinstance(cb.get("in_h"), int) and isinstance(cb.get("scale"), int)
            and cb["scale"] >= 1 and cb["in_w"] // cb["scale"] == ow and cb["in_h"] // cb["scale"] == oh
            and cb["scale"] == R.choose_scale(cb["in_w"], cb["in_h"], int(r.get("max_dim") or MAX_DIM))
        )
        rec.update({
            "path": "browser-wasm",
            "out_w": ow, "out_h": oh,  # of the RECEIVED file
            "consistent": consistent,
            "server_decode_ms": (t1 - t0) * 1e3, "server_resize_ms": 0.0, "server_encode_ms": 0.0,
            "server_cpu_ms": (c1 - c0) * 1e3,
            "encoded_bytes": os.path.getsize(path),
        })
        shown = rgb
    else:
        small, info = python_fallback_resize(path)
        ran_python_here = True
        rec.update({"path": "python-fallback", **info, "consistent": True})
        shown = small
    # the path marker, server-derived BY CONSTRUCTION for this dispatch (r1/B6): did THIS
    # handler run the kernel's Python body? The process-global counter delta (M0's marker)
    # is kept as a secondary, single-session signal.
    rec["python_calls"] = 1 if ran_python_here else 0
    rec["python_calls_global_delta"] = DOWN.calls() - r["calls_before"]
    return shown, json.dumps(_record(rec), indent=2), fresh_payload()


def on_naive(path: str | None, request: gr.Request):
    """The control: the stock gr.File uploaded the ORIGINAL; the server decodes it, box-reduces
    with Pillow (the realistic server-side resize) and encodes the small JPEG."""
    if not path:
        return gr.skip(), gr.skip(), gr.skip()
    calls_before = DOWN.calls()
    join = _join_bytes(request)
    measured = _measured(path, join)
    if not measured["upload_seen"]:  # same authority as the client panel: never decode an un-uploaded file
        rec = _record({"mode": "naive", "path": "error", "python_calls": 0, **measured,
                       "error": f"refused before decoding: {measured['upload_unmeasured_reason']}"})
        return gr.skip(), json.dumps(rec, indent=2), None
    s = R.server_resize(path, MAX_DIM, int(round(QUALITY * 100)))
    jpg = s.pop("jpeg")
    rec = _record({
        "mode": "naive",
        "path": "server-pillow",
        "orig_name": os.path.basename(path),
        "file_bytes": os.path.getsize(path),
        **measured,
        "cpu_timer_resolution_ms": cpu_tick_ms(),
        "python_calls": 0,
        "python_calls_global_delta": DOWN.calls() - calls_before,
        **s,
    })
    # the third output resets the file component so the next upload gets a fresh picker
    return np.asarray(Image.open(io.BytesIO(jpg)).convert("RGB")), json.dumps(rec, indent=2), None


# ----------------------------------------------------------------------------- UI
def build_demo() -> gr.Blocks:
    with gr.Blocks(title="pythscribe @wasm -- image preprocessing before upload") as demo:
        gr.Markdown(
            "## Resize in the browser with a `@wasm`-compiled Python kernel, then upload ~10 KB instead of ~2.5 MB\n"
            f"artifact: **{DOWN.artifact_status}** "
            + (f"(`{DOWN.artifact.wasm.name}`, {DOWN.artifact.wasm.stat().st_size} B; served at `{WASM_URL}`)" if DOWN.artifact else "(Python fallback: the original is uploaded and the same kernel runs here)")
            + f" | max side **{MAX_DIM} px**, JPEG q={QUALITY:g}. Bytes = request-body bytes counted by the server; "
            "the naive panel is the control (stock upload, Pillow resize on the server). No model, no GPU: bandwidth + server CPU per request."
        )
        with gr.Row():
            with gr.Column():
                gr.Markdown("### A. client-side `@wasm` preprocess, then upload")
                comp = WasmFunction(label="downscale_box (@wasm, in this tab)", elem_id="client-wasm")
                client_img = gr.Image(label="what the server received", elem_id="client-image", type="numpy")
                client_out = gr.Textbox(label="measured by the server (request-body bytes; browser claims under `client`)", elem_id="client-result", lines=14, interactive=False)
            with gr.Column():
                gr.Markdown("### B. naive control: upload the full image, the server resizes")
                naive_file = gr.File(label="original image (uploaded untouched)", elem_id="naive-file", file_types=["image"], type="filepath")
                naive_img = gr.Image(label="what the server produced", elem_id="naive-image", type="numpy")
                naive_out = gr.Textbox(label="measured (server side)", elem_id="naive-result", lines=14, interactive=False)

        demo.load(fresh_payload, None, [comp])
        comp.change(on_client, [comp], [client_img, client_out, comp])
        naive_file.upload(on_naive, [naive_file], [naive_img, naive_out, naive_file])
    return demo


def build_app() -> BytesMeter:
    global METER
    api = FastAPI()

    @api.get("/pythscribe-metrics")
    def metrics():
        return {
            "max_dim": MAX_DIM, "quality": QUALITY, "artifact_status": DOWN.artifact_status,
            "cpu_timer_resolution_ms": cpu_tick_ms(),
            "results": list(RESULTS), "python_calls": {"downscale_box": DOWN.calls(), "box_scale": SCALE.calls()},
            "requests": len(METER.requests) if METER else 0,
        }

    @api.post("/pythscribe-metrics/reset")
    def reset():
        RESULTS.clear()
        return {"ok": True}

    demo = build_demo()
    app = gr.mount_gradio_app(api, demo, path="/")
    METER = BytesMeter(app)
    return METER


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(
        build_app(),
        # a Space must bind every interface (opus r1/S11); locally stay on loopback
        host=os.environ.get("GRADIO_SERVER_NAME", "0.0.0.0" if os.environ.get("SPACE_ID") else "127.0.0.1"),
        port=int(os.environ.get("GRADIO_SERVER_PORT", "7860")),
        log_level="warning",
    )
