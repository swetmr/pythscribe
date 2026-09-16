"""Run a built artifact under Node -- the compiled arm of the differential oracle."""
from __future__ import annotations

import json
import shutil
import struct
import subprocess
from pathlib import Path
from typing import Any, Sequence

from ..artifacts import ArtifactInfo

_RUNNER = Path(__file__).with_name("_node_runner.mjs")


class RunnerError(RuntimeError):
    pass


def float_bits(v: float) -> str:
    """Little-endian IEEE-754 hex of a Python float -- must equal the Node runner's `bits`."""
    return struct.pack("<d", float(v)).hex()


def run_artifact(artifact: ArtifactInfo | Path, function: str, calls: Sequence[Sequence[Any]]) -> list[dict]:
    entry = artifact.entry if isinstance(artifact, ArtifactInfo) else Path(artifact)
    node = shutil.which("node")
    if not node:
        # T3 (spec 13-09-26): the Node oracle arm is opt-in verification, never the critical path --
        # a structured, typed message (no spawn traceback); `pyths build` / `@wasm` need no Node.
        raise RunnerError(
            "node is required to run the compiled artifact under the Node oracle (run_artifact), and no `node` "
            "is on PATH. `pyths build`, `@wasm` (wasmtime, in-process) and the adapters do NOT need Node; "
            "install Node (https://nodejs.org) or put it on PATH to use this verify arm."
        )
    proc = subprocess.run(
        [node, str(_RUNNER), str(entry), function],
        input=json.dumps([list(c) for c in calls]),
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise RunnerError(f"node runner failed (exit {proc.returncode}): {proc.stderr.strip()[:2000]}")
    try:
        return json.loads(proc.stdout)
    except ValueError as e:
        raise RunnerError(f"node runner emitted non-JSON: {proc.stdout[:500]!r}") from e
