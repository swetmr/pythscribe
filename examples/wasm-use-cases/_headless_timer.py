"""Headless-Chromium timer for the typed-array `downscale_nn` kernel, used by gradio_demo.ipynb.

Run as a SUBPROCESS (never imported): a fresh Python process on Windows gets the default
ProactorEventLoop, which supports the subprocess Playwright needs to spawn the browser -- unlike
the SelectorEventLoop that Jupyter/ipykernel installs process-wide (which raises NotImplementedError
on `create_subprocess_exec`). Reads one JSON params file (argv[1]) and prints a single
`RESULT_JSON:{...}` line with the median in-browser time, the kernel return, and the output pixels.

    python _headless_timer.py params.json    # params: repo_root, b64 (row-major RGB bytes), W,H,scale,oh,ow,k
"""
import base64
import functools
import http.server
import json
import socketserver
import sys
import threading

from playwright.sync_api import sync_playwright

_JS = r"""
async ({b64, W, H, scale, oh, ow, k}) => {
  const bin = atob(b64);
  const flat = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) flat[i] = bin.charCodeAt(i);
  const rows = [];
  for (let y = 0; y < H; y++) rows.push(flat.subarray(y * W * 3, (y + 1) * W * 3));
  await window.pythscribeNN(rows, scale, oh, ow);                 // warm
  const ts = []; let last;
  for (let i = 0; i < k; i++) { const t0 = performance.now(); last = await window.pythscribeNN(rows, scale, oh, ow); ts.push(performance.now() - t0); }
  ts.sort((a, b) => a - b);
  const out = [];
  for (const r of last.rows) for (const v of r) out.push(v);
  return { median_ms: ts[Math.floor(ts.length / 2)], ret: last.ret, out, layout: last.layout };
}
"""


def main() -> None:
    P = json.load(open(sys.argv[1], encoding="utf-8"))
    socketserver.TCPServer.allow_reuse_address = True
    httpd = socketserver.TCPServer(
        ("127.0.0.1", 0),
        functools.partial(http.server.SimpleHTTPRequestHandler, directory=P["repo_root"]),
    )
    port = httpd.server_address[1]
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    try:
        with sync_playwright() as p:
            browser = p.chromium.launch()
            page = browser.new_page()
            errs = []
            page.on("pageerror", lambda e: errs.append(str(e)))
            page.goto(f"http://127.0.0.1:{port}/examples/gradio-image-preprocess/typed_array_nn.html")
            res = page.evaluate(_JS, {k: P[k] for k in ("b64", "W", "H", "scale", "oh", "ow", "k")})
            browser.close()
            if errs:
                res["page_errors"] = errs[:3]
    finally:
        httpd.shutdown()
    print("RESULT_JSON:" + json.dumps(res))


if __name__ == "__main__":
    main()
