#!/usr/bin/env python3
"""M4 A5 browser witness -- the CONTROLLER side (spec 13-09-26, plan M4; validation §A A5).

Runs where Playwright + Chromium live: the Linux CONTROLLER container (internal network only) or the
`ctl` OS user on a native leg. It reaches the app ONLY over TCP (`--url`), never a shared filesystem.
It has no pythscribe dependency: the app's own textbox reports the path markers (`describe()`).

    python wheel_acceptance_controller.py --url http://app:7860 --out a5.json [--timeout-ms 120000]

Asserts (each a RED line on failure): the page loads and the WasmFunction component mounts without
console errors; the component's assets load (`rms_gain.wasm` 200 AND `rms_gain.js` + `rms_gain.glue.js`
200); clicking Run yields `path=browser-wasm python_calls=0 wasm_fetched=1` -- the compiled function
RENDERED in the tab (never the Python fallback, which would be a green-looking page with the wrong path).
"""
from __future__ import annotations

import argparse
import json
import re
import sys
import time
import urllib.request
from pathlib import Path

XS, TARGET = "1, 2, 3, 4", 0.5


def parse(text: str) -> dict:
    return dict(re.findall(r"(\w+)=('[^']*'|\S+)", text))


def wait_http(url: str, timeout_s: float) -> None:
    t0 = time.time()
    last = ""
    while time.time() - t0 < timeout_s:
        try:
            with urllib.request.urlopen(url, timeout=5) as r:  # noqa: S310 -- internal network only
                if r.status == 200:
                    return
        except Exception as e:  # noqa: BLE001
            last = str(e)
        time.sleep(1)
    raise SystemExit(f"app at {url} never answered: {last}")


def drive(url: str, timeout_ms: int) -> dict:
    from playwright.sync_api import sync_playwright

    problems: list[str] = []
    responses: list[tuple[str, int]] = []
    console_errors: list[str] = []
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        ctx = browser.new_context()
        page = ctx.new_page()
        page.on("response", lambda r: responses.append((r.url, r.status)))
        page.on("console", lambda m: console_errors.append(m.text) if m.type == "error" else None)
        page.on("pageerror", lambda e: console_errors.append(f"pageerror: {e}"))
        page.goto(url, wait_until="load")
        page.wait_for_selector("#run", timeout=timeout_ms)
        page.wait_for_selector("[data-testid=wasm-status]", timeout=timeout_ms)
        mount_errors = list(console_errors)
        page.locator("#xs textarea").fill(XS)
        page.locator("#target input").fill(str(TARGET))
        page.locator("#run").click()
        page.wait_for_function(
            "() => { const t = document.querySelector('#result textarea'); return !!t && /path=/.test(t.value); }",
            timeout=timeout_ms,
        )
        text = page.locator("#result textarea").input_value()
        status = page.locator("[data-testid=wasm-status]").inner_text()
        ctx.close()
        browser.close()
    r = parse(text)
    if mount_errors:
        problems.append(f"A5: console errors at mount: {mount_errors}")
    if r.get("path") != "browser-wasm":
        problems.append(f"A5: path={r.get('path')!r}, the compiled function did not render in the tab: {text}")
    if r.get("python_calls") != "0":
        problems.append(f"A5: python_calls={r.get('python_calls')!r} (the Python body ran)")
    if r.get("wasm_fetched") != "1":
        problems.append(f"A5: wasm_fetched={r.get('wasm_fetched')!r}")
    wasm_hits = [(u, s) for u, s in responses if u.endswith("rms_gain.wasm")]
    if not wasm_hits or not all(s == 200 for _, s in wasm_hits):
        problems.append(f"A5: component .wasm asset did not load: {wasm_hits}")
    js_hits = [(u, s) for u, s in responses if u.endswith("rms_gain.js") or u.endswith("rms_gain.glue.js")]
    if len(js_hits) < 2 or not all(s == 200 for _, s in js_hits):
        problems.append(f"A5: component js assets did not load: {js_hits}")
    if not status.startswith("done (browser-wasm)"):
        problems.append(f"A5: status {status!r}")
    if console_errors:
        problems.append(f"A5: console errors: {console_errors}")
    return {"step": "A5", "url": url, "result_text": text, "status": status, "parsed": r, "responses": len(responses),
            "asset_hits": {"wasm": wasm_hits, "js": js_hits}, "problems": problems, "verdict": "fail" if problems else "pass"}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="wheel_acceptance_controller.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--url", required=True)
    ap.add_argument("--out", default=None)
    ap.add_argument("--timeout-ms", type=int, default=120_000)
    ap.add_argument("--wait-s", type=float, default=180)
    ns = ap.parse_args(argv)
    wait_http(ns.url, ns.wait_s)
    res = drive(ns.url, ns.timeout_ms)
    text = json.dumps(res, indent=2, sort_keys=True)
    if ns.out:
        Path(ns.out).parent.mkdir(parents=True, exist_ok=True)
        Path(ns.out).write_text(text + "\n", encoding="utf-8")
    print(text)
    return 1 if res["problems"] else 0


if __name__ == "__main__":
    sys.exit(main())
