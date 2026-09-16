"""Drive the demo app with a real browser (Playwright) -- the ONE measurement path both the
e2e gates and the reproducibility notebook use, so 'bytes uploaded' is always the number the
SERVER measured on its side of the socket for a real upload from a real tab.

    from drive import AppUnderTest, drive_client, drive_naive
    with AppUnderTest() as app, browser_session() as page:
        rec = drive_client(page, app, "test_images/photo_640x480.jpg")
"""
from __future__ import annotations

import json
import os
import socket
import subprocess
import sys
import time
import urllib.request
from contextlib import closing, contextmanager
from pathlib import Path

HERE = Path(__file__).resolve().parent
APP = HERE / "app.py"


def free_port() -> int:
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class AppUnderTest:
    """The demo as a subprocess (a copy of it when `app_dir` is given: e.g. one WITHOUT the
    artifact directory, for the fallback gate)."""

    def __init__(self, app_dir: Path | None = None, env: dict[str, str] | None = None, timeout: float = 120.0):
        self.app_dir = Path(app_dir) if app_dir else HERE
        self.port = free_port()
        self.url = f"http://127.0.0.1:{self.port}"
        self.env = {**os.environ, "GRADIO_SERVER_PORT": str(self.port), "PYTHONUTF8": "1", "GRADIO_ANALYTICS_ENABLED": "False", **(env or {})}
        self.timeout = timeout
        self.proc: subprocess.Popen | None = None
        self.log = self.app_dir / "_app.log"

    def __enter__(self) -> "AppUnderTest":
        self._logf = open(self.log, "w", encoding="utf-8")
        self.proc = subprocess.Popen([sys.executable, str(self.app_dir / "app.py")], cwd=str(self.app_dir), env=self.env, stdout=self._logf, stderr=subprocess.STDOUT)
        deadline = time.time() + self.timeout
        while time.time() < deadline:
            if self.proc.poll() is not None:
                self._logf.close()
                raise RuntimeError(f"app exited early ({self.proc.returncode}):\n{self.log.read_text(encoding='utf-8')[-3000:]}")
            try:
                with urllib.request.urlopen(self.url + "/pythscribe-metrics", timeout=2) as r:
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

    def metrics(self) -> dict:
        with urllib.request.urlopen(self.url + "/pythscribe-metrics", timeout=10) as r:
            return json.loads(r.read().decode("utf-8"))

    def reset(self) -> None:
        req = urllib.request.Request(self.url + "/pythscribe-metrics/reset", method="POST")
        with urllib.request.urlopen(req, timeout=10) as r:
            r.read()


@contextmanager
def browser_session(headless: bool = True):
    from playwright.sync_api import sync_playwright

    with sync_playwright() as p:
        b = p.chromium.launch(headless=headless)
        ctx = b.new_context()
        page = ctx.new_page()
        page.console_errors = []  # type: ignore[attr-defined]
        page.responses = []  # type: ignore[attr-defined]
        page.on("console", lambda m: page.console_errors.append(m.text) if m.type == "error" else None)  # type: ignore[attr-defined]
        page.on("pageerror", lambda e: page.console_errors.append(f"pageerror: {e}"))  # type: ignore[attr-defined]
        page.on("response", lambda r: page.responses.append((r.url, r.status)))  # type: ignore[attr-defined]
        try:
            yield page
        finally:
            ctx.close()
            b.close()


def _wait_json(page, selector: str, after_seq: int, timeout_ms: int) -> dict:
    page.wait_for_function(
        """([sel, seq]) => { const t = document.querySelector(sel + ' textarea'); if (!t) return false;
             try { const j = JSON.parse(t.value); return j.seq > seq; } catch (e) { return false; } }""",
        arg=[selector, after_seq],
        timeout=timeout_ms,
    )
    return json.loads(page.locator(selector + " textarea").input_value())


def open_app(page, app: AppUnderTest, timeout_ms: int = 90_000) -> None:
    page.goto(app.url, wait_until="load")
    page.wait_for_selector("[data-testid=wasm-file]", timeout=timeout_ms)
    page.wait_for_selector("#naive-file input[type=file]", state="attached", timeout=timeout_ms)


def _last_seq(page, selector: str) -> int:
    try:
        return int(json.loads(page.locator(selector + " textarea").input_value()).get("seq", 0))
    except Exception:
        return 0


def drive_client(page, app: AppUnderTest, image: str | Path, timeout_ms: int = 180_000) -> dict:
    """Pick `image` in the client panel; return the SERVER's record for it."""
    seq = _last_seq(page, "#client-result")
    page.locator("[data-testid=wasm-file]").set_input_files(str(image))
    return _wait_json(page, "#client-result", seq, timeout_ms)


def drive_naive(page, app: AppUnderTest, image: str | Path, timeout_ms: int = 180_000) -> dict:
    """Upload `image` untouched through the stock gr.File; return the SERVER's record."""
    seq = _last_seq(page, "#naive-result")
    # gr.File swaps its picker for a preview while it holds a file; the app resets it after
    # each measured upload, so wait for the picker to be back before feeding the next file
    page.wait_for_selector("#naive-file input[type=file]", state="attached", timeout=timeout_ms)
    page.locator("#naive-file input[type=file]").set_input_files(str(image))
    return _wait_json(page, "#naive-result", seq, timeout_ms)


def collect_records(images: list[str | Path], app_dir: Path | None = None, env: dict | None = None, repeats: int = 1) -> list[dict]:
    """Drive client + naive for every image, `repeats` times, in ONE app + ONE tab; returns
    the server records (each tagged with `image` and `rep`). Raises on any browser console error."""
    records: list[dict] = []
    with AppUnderTest(app_dir, env=env) as app, browser_session() as page:
        open_app(page, app)
        for rep in range(repeats):
            for img in images:
                img = Path(img)
                c = drive_client(page, app, img)
                n = drive_naive(page, app, img)
                for r in (c, n):
                    r["image"] = img.name
                    r["rep"] = rep
                records.extend([c, n])
        errors = list(page.console_errors)  # type: ignore[attr-defined]
    if errors:
        raise RuntimeError(f"browser console errors during collection: {errors}")
    return records


if __name__ == "__main__":
    if len(sys.argv) >= 4 and sys.argv[1] == "--collect":
        # subprocess mode for metrics_lib.collect (Playwright's sync API cannot run on a thread
        # with a live asyncio loop, and Windows selector loops cannot spawn its driver)
        spec = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
        recs = collect_records(spec["images"], Path(spec["app_dir"]) if spec.get("app_dir") else None, spec.get("env"), int(spec.get("repeats", 1)))
        Path(sys.argv[3]).write_text(json.dumps(recs), encoding="utf-8")
        sys.exit(0)
    # quick manual smoke: python drive.py test_images/photo_640x480.jpg
    img = sys.argv[1] if len(sys.argv) > 1 else str(HERE / "test_images" / "photo_640x480.jpg")
    with AppUnderTest() as app, browser_session() as page:
        open_app(page, app)
        c = drive_client(page, app, img)
        n = drive_naive(page, app, img)
        print(json.dumps({"client": c, "naive": n, "console_errors": page.console_errors}, indent=2))  # type: ignore[attr-defined]
