"""The headless (Playwright/Chromium) driver for `browser_image_filters_app.py` -- ONE source of
truth used by BOTH `tests/pythscribe/test_browser_image_filters_e2e.py` (the asserting gate) and
`browser_image_filters.ipynb` (the reporting cell, which runs it as a subprocess).

It launches the app as a subprocess, drives the IN-TAB tab (slider moves, filter switches) and
the SERVER tab (the paired negative control), and returns, per path:
  * `compute_requests`  -- requests to /gradio_api/* OTHER than static `file=` fetches during the
                           interaction (the client-side claim: 0 on the in-tab path)
  * `counters_delta`    -- the SERVER-side kernel counters (python_calls + server_calls) since the
                           interaction began, read from the app's /pythscribe-probe route (0 in-tab)
  * `median_ms`         -- slider move -> output <img> src change, median over the moves
  * the in-tab OUT rows per (kernel, setting) read from `window.pythscribeImage.last` AND the
    displayed output's PNG data URL, for the bit-for-bit comparison against the NumPy reference
    (`references()` below), which the test and the notebook both run.

Run as a script (a SUBPROCESS: Windows' Proactor loop lets Playwright spawn Chromium; Jupyter's
Selector loop cannot) it prints one `RESULT_JSON:{...}` line without the pixel payloads.
"""
from __future__ import annotations

import base64
import io
import json
import os
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path
from typing import Any

import numpy as np

HERE = Path(__file__).resolve().parent
APP = HERE / "browser_image_filters_app.py"

THRESHOLDS = (90, 160, 60, 200, 120, 30, 180, 150, 70, 110)  # slider values (per-channel mean)
SCALES = (2, 3)


# ---- references (independent NumPy twins of the kernels; the threshold takes thr*3) --------------
def ref_threshold(rgb: np.ndarray, thr_sum: int) -> np.ndarray:
    h, w = rgb.shape[:2]
    s = rgb.astype(np.int64).sum(axis=2)
    v = np.where(s > thr_sum, 255, 0).astype(np.uint8)
    return np.repeat(v, 3, axis=1).reshape(h, w * 3)


def ref_sobel(rgb: np.ndarray) -> np.ndarray:
    h, w = rgb.shape[:2]
    r = rgb[:, :, 0].astype(np.int64)
    gx = np.zeros_like(r)
    gy = np.zeros_like(r)
    gx[1:-1, 1:-1] = (r[:-2, 2:] + 2 * r[1:-1, 2:] + r[2:, 2:]) - (r[:-2, :-2] + 2 * r[1:-1, :-2] + r[2:, :-2])
    gy[1:-1, 1:-1] = (r[2:, :-2] + 2 * r[2:, 1:-1] + r[2:, 2:]) - (r[:-2, :-2] + 2 * r[:-2, 1:-1] + r[:-2, 2:])
    e = np.minimum(np.abs(gx) + np.abs(gy), 255)
    e[0, :] = 0
    e[-1, :] = 0
    e[:, 0] = 0
    e[:, -1] = 0
    return np.repeat(e.astype(np.uint8), 3, axis=1).reshape(h, w * 3)


def ref_downscale_nn(rgb: np.ndarray, scale: int) -> np.ndarray:
    h, w = rgb.shape[:2]
    oh, ow = h // scale, w // scale
    return np.ascontiguousarray(rgb[: oh * scale : scale, : ow * scale : scale, :]).reshape(oh, ow * 3)


def references(rgb: np.ndarray) -> dict[str, np.ndarray]:
    refs = {f"threshold@{t}": ref_threshold(rgb, t * 3) for t in THRESHOLDS}
    refs["sobel"] = ref_sobel(rgb)
    for s in SCALES:
        refs[f"downscale@{s}"] = ref_downscale_nn(rgb, s)
    return refs


def png_data_url_to_rows(data_url: str) -> np.ndarray:
    from PIL import Image

    b64 = data_url.split(",", 1)[1]
    im = np.asarray(Image.open(io.BytesIO(base64.b64decode(b64))).convert("RGB"), dtype=np.uint8)
    return im.reshape(im.shape[0], im.shape[1] * 3)


# ---- the app subprocess ---------------------------------------------------------------------------
def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class App:
    def __init__(self, timeout: float = 120.0):
        self.port = free_port()
        self.url = f"http://127.0.0.1:{self.port}"
        self.timeout = timeout
        self.log = Path(tempfile.gettempdir()) / f"pythscribe_browser_image_app_{self.port}.log"

    def __enter__(self) -> "App":
        env = {**os.environ, "GRADIO_SERVER_PORT": str(self.port), "GRADIO_ANALYTICS_ENABLED": "False", "PYTHONUTF8": "1"}
        self._logf = open(self.log, "w", encoding="utf-8")
        self.proc = subprocess.Popen([sys.executable, str(APP)], cwd=str(HERE), env=env, stdout=self._logf, stderr=subprocess.STDOUT)
        deadline = time.time() + self.timeout
        while time.time() < deadline:
            if self.proc.poll() is not None:
                self._logf.close()
                raise RuntimeError(f"app exited early ({self.proc.returncode}):\n{self.log.read_text(encoding='utf-8')[-3000:]}")
            try:
                self.probe()
                return self
            except Exception:
                time.sleep(0.5)
        self.__exit__(None, None, None)
        raise TimeoutError(f"app did not come up on {self.url}:\n{self.log.read_text(encoding='utf-8')[-3000:]}")

    def __exit__(self, *exc) -> None:
        if self.proc.poll() is None:
            self.proc.kill()
            try:
                self.proc.wait(timeout=15)
            except subprocess.TimeoutExpired:
                pass
        self._logf.close()

    def probe(self) -> dict[str, Any]:
        with urllib.request.urlopen(self.url + "/pythscribe-probe", timeout=5) as r:
            return json.loads(r.read())


def _counter_total(counts: dict[str, Any]) -> int:
    return sum(int(v["python_calls"]) + int(v["server_calls"]) for v in counts.values())


def _counter_deltas(before: dict[str, Any], after: dict[str, Any]) -> dict[str, int]:
    """PER-KERNEL (python_calls + server_calls) delta -- asserted per kernel, so a marker that
    cannot move for one kernel is not hidden inside a cross-kernel sum."""
    return {k: (int(after[k]["python_calls"]) + int(after[k]["server_calls"]))
            - (int(before[k]["python_calls"]) + int(before[k]["server_calls"])) for k in after}


# JS helpers shared by both tabs: set a Gradio slider, click a radio/tab, wait for an <img> src change.
_JS_LIB = """
const sleep = ms => new Promise(r => setTimeout(r, ms));
const imgSrc = sel => { const i = document.querySelector(sel + ' img'); return i ? i.src : ''; };
const setSlider = (sel, v) => { const el = document.querySelector(sel + ' input[type=range]');
  const set = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set; set.call(el, v);
  el.dispatchEvent(new Event('input', {bubbles: true})); el.dispatchEvent(new Event('change', {bubbles: true})); };
const clickRadio = (sel, text) => { const l = [...document.querySelectorAll(sel + ' label')].find(x => x.textContent.trim() === text); l.click(); };
const clickTab = text => { const b = [...document.querySelectorAll('button[role=tab]')].find(x => x.textContent.trim().startsWith(text)); b.click(); };
const waitSrcChange = async (sel, before, limit) => { let w = 0; while (w < limit && imgSrc(sel) === before) { await sleep(1); w += 1; } return imgSrc(sel) !== before; };
const timedMove = async (slider, outSel, v, limit) => { const b = imgSrc(outSel); const t0 = performance.now(); setSlider(slider, v);
  const ok = await waitSrcChange(outSel, b, limit); return { ms: performance.now() - t0, ok }; };
const status = sel => (document.querySelector(sel + ' textarea') || {}).value || '';
"""

_JS_CLIENT = _JS_LIB + """
const thresholds = A[0], scales = A[1];
const rows = () => window.pythscribeImage.last.rows.map(r => Array.from(r));
const outs = {}, shown = {}, rets = {}, kms = [];
const take = (key) => { outs[key] = rows(); shown[key] = imgSrc('#c-out'); rets[key] = window.pythscribeImage.last.ret; };
// warm-up (first decode + .wasm instantiate are one-time costs, excluded from the median)
await timedMove('#c-thr', '#c-out', 100, 15000); await timedMove('#c-thr', '#c-out', 140, 15000);
const ts = [];
for (const v of thresholds) { const m = await timedMove('#c-thr', '#c-out', v, 15000); if (!m.ok) throw new Error('no output for thr ' + v + ': ' + status('#c-status'));
  ts.push(m.ms); kms.push(window.pythscribeImage.last.ms); take('threshold@' + v); }
let b = imgSrc('#c-out'); clickRadio('#c-kind', 'sobel'); if (!(await waitSrcChange('#c-out', b, 20000))) throw new Error('no sobel output: ' + status('#c-status'));
take('sobel'); const sobelMs = window.pythscribeImage.last.ms;
b = imgSrc('#c-out'); clickRadio('#c-kind', 'downscale'); if (!(await waitSrcChange('#c-out', b, 20000))) throw new Error('no downscale output: ' + status('#c-status'));
for (const s of scales) { const m = await timedMove('#c-scale', '#c-out', s, 15000); if (!m.ok && window.pythscribeImage.last.extras[0] !== s) throw new Error('no downscale output for scale ' + s + ': ' + status('#c-status'));
  take('downscale@' + s); }
ts.sort((a, b) => a - b); kms.sort((a, b) => a - b);
return { median_ms: ts[ts.length >> 1], kernel_median_ms: kms[kms.length >> 1], sobel_kernel_ms: sobelMs, calls: window.pythscribeImage.calls,
         status: status('#c-status'), last_error: window.pythscribeImage.lastError, load_error: window.pythscribeImage.loadError,
         ready: window.pythscribeImage.ready, layout: window.pythscribeImage.layout, outs, shown, rets };
"""

_JS_SERVER = _JS_LIB + """
const thresholds = A[0];
clickTab('Server');
await sleep(300);
await timedMove('#s-thr', '#s-out', 100, 30000); await timedMove('#s-thr', '#s-out', 140, 30000);
const ts = [];
for (const v of thresholds) { const m = await timedMove('#s-thr', '#s-out', v, 30000); if (!m.ok) throw new Error('no server output for thr ' + v + ': ' + status('#s-status')); ts.push(m.ms); }
ts.sort((a, b) => a - b);
const thrStatus = status('#s-status');
// the control must exercise EVERY kernel (the counter marker is asserted per kernel)
let b = imgSrc('#s-out'); clickRadio('#s-kind', 'sobel'); if (!(await waitSrcChange('#s-out', b, 30000))) throw new Error('no server sobel output: ' + status('#s-status'));
b = imgSrc('#s-out'); clickRadio('#s-kind', 'downscale'); if (!(await waitSrcChange('#s-out', b, 30000))) throw new Error('no server downscale output: ' + status('#s-status'));
return { median_ms: ts[ts.length >> 1], status: thrStatus, last_status: status('#s-status') };
"""


def _is_compute(url: str) -> bool:
    """A server request that is NOT a static file fetch: /gradio_api/queue/join, /queue/data,
    /upload, /run/predict, /heartbeat ... (and any WebSocket URL, which never carries file=)."""
    return ("/gradio_api/" in url and "/gradio_api/file=" not in url) or url.startswith(("ws://", "wss://"))


def expected_ret(key: str, h: int, w: int) -> int:
    """The scalar the kernel's export returns for (kernel, setting) `key` on an h x w image --
    pins the return channel (ffi.call's returnType conversion) alongside the pixel channel."""
    if key.startswith("threshold@"):
        return h * w
    if key == "sobel":
        return (h - 2) * (w - 2)
    s = int(key.split("@", 1)[1])
    return (h // s) * (w // s)


def drive(app: App, *, thresholds=THRESHOLDS, scales=SCALES, server_control: bool = True) -> dict[str, Any]:
    from playwright.sync_api import sync_playwright

    reqs: list[str] = []
    errors: list[str] = []
    with sync_playwright() as p:
        br = p.chromium.launch()
        try:
            pg = br.new_page()
            pg.on("request", lambda r: reqs.append(r.url))
            pg.on("websocket", lambda ws: reqs.append(ws.url))  # transport-independent marker (a WS handshake is not a "request")
            pg.on("pageerror", lambda e: errors.append(str(e)))
            pg.goto(app.url, wait_until="networkidle", timeout=60000)
            try:
                pg.wait_for_function("window.pythscribeImage && window.pythscribeImage.ready === true", timeout=30000)
            except Exception as e:  # surface the client's own diagnostic, not a bare timeout
                load_error = pg.evaluate("window.pythscribeImage ? window.pythscribeImage.loadError : 'no client object'")
                raise RuntimeError(f"in-tab client never became ready; loadError={load_error!r}; page errors={errors}") from e
            # ---- the in-tab path ----
            c_before = app.probe()
            n0 = len(reqs)
            client = pg.evaluate(f"async (a) => {{ const A = a;{_JS_CLIENT} }}", [list(thresholds), list(scales)])
            pg.wait_for_timeout(300)
            c_reqs = reqs[n0:]
            c_after = app.probe()
            client["compute_requests"] = [u for u in c_reqs if _is_compute(u)]
            client["file_requests"] = [u for u in c_reqs if "/gradio_api/file=" in u]
            client["wasm_requests"] = [u for u in c_reqs if u.endswith(".wasm")]
            client["counters_delta"] = _counter_total(c_after) - _counter_total(c_before)
            client["counters_delta_per_kernel"] = _counter_deltas(c_before, c_after)
            client["counters_after"] = c_after
            result: dict[str, Any] = {"client": client, "page_errors": list(errors)}
            # ---- the server path (paired negative control) ----
            if server_control:
                s_before = app.probe()
                n1 = len(reqs)
                server = pg.evaluate(f"async (a) => {{ const A = a;{_JS_SERVER} }}", [list(thresholds)])
                pg.wait_for_timeout(300)
                s_reqs = reqs[n1:]
                s_after = app.probe()
                server["compute_requests"] = [u for u in s_reqs if _is_compute(u)]
                server["counters_delta"] = _counter_total(s_after) - _counter_total(s_before)
                server["counters_delta_per_kernel"] = _counter_deltas(s_before, s_after)
                result["server"] = server
        finally:
            br.close()
    return result


def fidelity(result: dict[str, Any], rgb: np.ndarray) -> dict[str, dict[str, Any]]:
    """Per (kernel, setting): whether the in-tab OUT rows and the DISPLAYED PNG both equal the
    NumPy reference bit-for-bit (+ the mismatch counts)."""
    refs = references(rgb)
    out: dict[str, dict[str, Any]] = {}
    h, w = rgb.shape[:2]
    for key, ref in refs.items():
        got = np.array(result["client"]["outs"][key], dtype=np.uint8)
        shown = png_data_url_to_rows(result["client"]["shown"][key])
        out[key] = {
            "shape": list(got.shape),
            "rows_equal": bool(got.shape == ref.shape and np.array_equal(got, ref)),
            "rows_mismatches": int((got != ref).sum()) if got.shape == ref.shape else -1,
            "shown_equal": bool(shown.shape == ref.shape and np.array_equal(shown, ref)),
            "ret": result["client"]["rets"].get(key),
            "ret_equal": result["client"]["rets"].get(key) == expected_ret(key, h, w),
        }
    return out


def load_rgb() -> np.ndarray:
    """The app's input image (the same decode as browser_image_filters_app.load_rgb; duplicated
    so this driver imports nothing from the app -- and, in the tests, no `kernels` module)."""
    from PIL import Image

    p = HERE.parent / "gradio-image-preprocess" / "test_images" / "photo_640x480.jpg"
    return np.asarray(Image.open(p).convert("RGB"), dtype=np.uint8)


def main() -> None:
    rgb = load_rgb()
    with App() as app:
        res = drive(app)
    fid = fidelity(res, rgb)
    c, s = res["client"], res.get("server", {})
    summary = {
        "image": list(rgb.shape),
        "client": {k: v for k, v in c.items() if k not in ("outs", "shown", "counters_after")},
        "server": s,
        "fidelity": fid,
        "all_bit_for_bit": all(v["rows_equal"] and v["shown_equal"] and v["ret_equal"] for v in fid.values()),
        "page_errors": res["page_errors"],
    }
    print("RESULT_JSON:" + json.dumps(summary))


if __name__ == "__main__":
    main()
