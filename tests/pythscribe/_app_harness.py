"""Launch `examples/gradio-wasm/app.py` (or a copy of it) as a subprocess for the E2E gates."""
from __future__ import annotations

import os
import socket
import subprocess
import sys
import time
import urllib.request
from contextlib import closing
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
DEMO_DIR = REPO / "examples" / "gradio-wasm"


def free_port() -> int:
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class GradioApp:
    def __init__(self, app_dir: Path = DEMO_DIR, env: dict[str, str] | None = None, timeout: float = 90.0):
        self.app_dir = Path(app_dir)
        self.port = free_port()
        self.url = f"http://127.0.0.1:{self.port}"
        self.env = {**os.environ, "GRADIO_SERVER_PORT": str(self.port), "PYTHONUTF8": "1", "GRADIO_ANALYTICS_ENABLED": "False", **(env or {})}
        self.timeout = timeout
        self.proc: subprocess.Popen | None = None
        self.log = self.app_dir / "_app.log"

    def __enter__(self) -> "GradioApp":
        self._logf = open(self.log, "w", encoding="utf-8")
        self.proc = subprocess.Popen(
            [sys.executable, str(self.app_dir / "app.py")],
            cwd=str(self.app_dir),
            env=self.env,
            stdout=self._logf,
            stderr=subprocess.STDOUT,
        )
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

    def get(self, path: str) -> tuple[int, dict[str, str], bytes]:
        req = urllib.request.Request(self.url + path)
        try:
            with urllib.request.urlopen(req, timeout=10) as r:
                return r.status, {k.lower(): v for k, v in r.headers.items()}, r.read()
        except urllib.error.HTTPError as e:
            return e.code, {k.lower(): v for k, v in e.headers.items()}, e.read()
