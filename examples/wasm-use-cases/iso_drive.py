"""Drive `app.py` in a real (headless) browser with Playwright and read the isomorphic result
back -- the browser half of the "same function, browser + server" measurement. Self-contained
(the notebook and the tests both import it)."""
from __future__ import annotations

import os
import re
import socket
import subprocess
import sys
import time
import urllib.request
from contextlib import closing
from pathlib import Path

HERE = Path(__file__).resolve().parent


def free_port() -> int:
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class App:
    def __init__(self, app_dir: Path = HERE, env: dict[str, str] | None = None, timeout: float = 120.0):
        self.app_dir = Path(app_dir)
        self.port = free_port()
        self.url = f"http://127.0.0.1:{self.port}"
        self.env = {**os.environ, "GRADIO_SERVER_PORT": str(self.port), "PYTHONUTF8": "1", "GRADIO_ANALYTICS_ENABLED": "False", **(env or {})}
        self.timeout = timeout
        self.proc: subprocess.Popen | None = None
        self.log = self.app_dir / "_app.log"

    def __enter__(self) -> "App":
        self._logf = open(self.log, "w", encoding="utf-8")
        self.proc = subprocess.Popen([sys.executable, str(self.app_dir / "app.py")], cwd=str(self.app_dir), env=self.env, stdout=self._logf, stderr=subprocess.STDOUT)
        deadline = time.time() + self.timeout
        while time.time() < deadline:
            if self.proc.poll() is not None:
                self._logf.close()
                raise RuntimeError(f"app exited early ({self.proc.returncode}):\n{self.log.read_text(encoding='utf-8')[-3000:]}")
            try:
                with urllib.request.urlopen(self.url + "/", timeout=2) as r:
                    if r.status == 200:
                        return self
            except Exception:
                time.sleep(0.5)
        self.__exit__(None, None, None)
        raise TimeoutError(f"app did not come up on {self.url}:\n{self.log.read_text(encoding='utf-8')[-3000:]}")

    def __exit__(self, *exc) -> None:
        if self.proc and self.proc.poll() is None:
            self.proc.kill()
            try:
                self.proc.wait(timeout=15)
            except subprocess.TimeoutExpired:
                pass
        try:
            self._logf.close()
        except Exception:
            pass


def parse(text: str) -> dict:
    return dict(re.findall(r"(\w+)=('[^']*'|\S+)", text))


def drive(app: App, browser, a: str, b: str, timeout_ms: int = 90_000) -> dict:
    ctx = browser.new_context()
    page = ctx.new_page()
    responses: list[tuple[str, int]] = []
    errors: list[str] = []
    page.on("response", lambda r: responses.append((r.url, r.status)))
    page.on("console", lambda m: errors.append(m.text) if m.type == "error" else None)
    page.on("pageerror", lambda e: errors.append(f"pageerror: {e}"))
    page.goto(app.url, wait_until="load")
    page.wait_for_selector("#run", timeout=timeout_ms)
    page.wait_for_selector("[data-testid=wasm-status]", timeout=timeout_ms)
    page.locator("#xs textarea").fill(a)
    page.locator("#ys textarea").fill(b)
    page.locator("#run").click()
    page.wait_for_function("() => { const t = document.querySelector('#result textarea'); return !!t && /identical=/.test(t.value); }", timeout=timeout_ms)
    text = page.locator("#result textarea").input_value()
    status = page.locator("[data-testid=wasm-status]").inner_text()
    ctx.close()
    r = parse(text)
    wasm_hits = [(u, s) for u, s in responses if u.endswith("dtw_distance.wasm")]
    return {"text": text, "status": status, "errors": errors, "wasm_hits": wasm_hits, **r}


def _loop_running() -> bool:
    import asyncio

    try:
        asyncio.get_running_loop()
        return True
    except RuntimeError:
        return False


def measure_isomorphic(a: list[float], b: list[float], app_dir: Path = HERE) -> dict:
    """Launch the app, run once in a headless Chromium, return the record the table needs:
    browser bits vs server bits vs CPython bits, path markers, and whether the tab fetched
    the .wasm. Requires gradio + playwright (+ the built artifact). Inside a Jupyter kernel
    (a live asyncio loop, which Playwright's sync API refuses) the measurement runs in a
    subprocess -- the same code, the same record."""
    import importlib.util

    for mod in ("playwright.sync_api", "gradio", "gradio_wasmfunction"):
        if importlib.util.find_spec(mod.split(".")[0]) is None:
            raise ImportError(f"{mod} is not installed (pip install pythscribe[gradio] playwright)")
    if _loop_running():
        import json

        p = subprocess.run(
            [sys.executable, str(Path(__file__).resolve()), "--json", str(Path(app_dir).resolve()), json.dumps(a), json.dumps(b)],
            capture_output=True, text=True, check=False, env={**os.environ, "PYTHONUTF8": "1"},
        )
        if p.returncode != 0:
            raise RuntimeError(f"isomorphic measurement subprocess failed:\n{p.stderr[-3000:]}")
        return json.loads(p.stdout.strip().splitlines()[-1])
    from playwright.sync_api import sync_playwright

    a_text = ", ".join(repr(x) for x in a)
    b_text = ", ".join(repr(x) for x in b)
    with App(app_dir) as app, sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        try:
            r = drive(app, browser, a_text, b_text)
        finally:
            browser.close()
    return {
        "kind": "isomorphic", "a": a, "b": b, "path": r.get("path"), "browser_bits": r.get("bits"),
        "server_bits": r.get("server_bits"), "cpython_bits": r.get("python_bits"),
        "identical": r.get("identical") == "True", "mode": r.get("mode"),
        "python_calls": int(r.get("python_calls", "-1")), "server_calls": int(r.get("server_calls", "-1")),
        "wasm_fetched": bool(r["wasm_hits"]) and all(s == 200 for _, s in r["wasm_hits"]),
        "console_errors": r["errors"], "status": r["status"],
    }


if __name__ == "__main__":  # subprocess mode: drive.py --json <app_dir> <a-json> <b-json>
    import json

    if len(sys.argv) == 5 and sys.argv[1] == "--json":
        rec = measure_isomorphic(json.loads(sys.argv[3]), json.loads(sys.argv[4]), app_dir=Path(sys.argv[2]))
        print(json.dumps(rec))
    else:
        print("usage: drive.py --json <app_dir> <a-json> <b-json>", file=sys.stderr)
        sys.exit(2)
