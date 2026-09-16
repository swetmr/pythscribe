"""Launch a Streamlit app (`examples/streamlit-wasm/app.py` or a copy) as a subprocess for the
E2E gates. Parallels _app_harness.py (the Gradio harness)."""
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
DEMO_DIR = REPO / "examples" / "streamlit-wasm"


def free_port() -> int:
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class StreamlitApp:
    def __init__(self, app_dir: Path = DEMO_DIR, env: dict[str, str] | None = None, timeout: float = 120.0):
        self.app_dir = Path(app_dir)
        self.port = free_port()
        self.url = f"http://127.0.0.1:{self.port}"
        self.health = self.url + "/_stcore/health"
        # `streamlit run` sets cwd to the app dir; put the repo root on PYTHONPATH so the
        # subprocess finds `pythscribe` whether or not it is pip-installed (matches how the
        # test process imports it).
        pypath = os.pathsep.join(p for p in (str(REPO), os.environ.get("PYTHONPATH", "")) if p)
        self.env = {
            **os.environ,
            "PYTHONPATH": pypath,
            "PYTHONUTF8": "1",
            "STREAMLIT_BROWSER_GATHER_USAGE_STATS": "false",
            "STREAMLIT_SERVER_HEADLESS": "true",
            **(env or {}),
        }
        self.timeout = timeout
        self.proc: subprocess.Popen | None = None
        self.log = self.app_dir / "_app.log"

    def __enter__(self) -> "StreamlitApp":
        self._logf = open(self.log, "w", encoding="utf-8")
        self.proc = subprocess.Popen(
            [
                sys.executable, "-m", "streamlit", "run", str(self.app_dir / "app.py"),
                "--server.headless=true", f"--server.port={self.port}",
                "--server.address=127.0.0.1", "--browser.gatherUsageStats=false",
                "--global.developmentMode=false",
            ],
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
                with urllib.request.urlopen(self.health, timeout=2) as r:
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
