#!/usr/bin/env python3
"""PRE-PUBLISH bare-install SERVER gate (0.2.9, shift-left), run against an INSTALLED pythscribe wheel.

    python scripts/wheel_server_spot.py <project root>     # run with the venv's python

cibuildwheel runs this as the LAST leg of CIBW_TEST_COMMAND (after wheel_m1_spots.py + readme_spots.py)
in its fresh test venv, where the just-built wheel is installed with its CORE dependencies only -- no
extras. 0.2.9 made `wasmtime` a core dependency (it was wrongly a `[server]` extra in 0.2.8, so a bare
`pip install pythscribe` silently fell back to plain Python with NO speedup). This gate makes that
regression LOUD before publish. Asserted, on the REAL installed bytes, with a LITERAL `@wasm` kernel
(the static validator rejects aliases -- `@_wasm` is a StaticDecorationError):
  * `pythscribe.runtime.wasmtime_available()` is True on the bare install (wasmtime came in as a CORE dep);
  * calling the kernel returns the CPython value, and `binding_of(spot_sq).mode == "server"` afterwards
    (compile-on-first-call with the BUNDLED compiler + the in-process wasmtime path) -- the counters
    say the WASM ran and the Python body did NOT.
Exit non-zero with the failing assertion on any miss. Paired negative control: tests/pythscribe/
test_wheel_server_spot.py runs this script with `wasmtime` shadowed by an ImportError stub and asserts RED.
"""
from __future__ import annotations

import atexit
import json
import os
import shutil
import sys
import tempfile
from pathlib import Path

# compile-on-first-call is the USER default; a stray PYTHSCRIBE_NO_JIT in the CI env would make this
# gate fail for the wrong reason (mode stays fallback with an "disabled by PYTHSCRIBE_NO_JIT" jit_reason),
# so it is dropped BEFORE the decorator resolves the kernel. A private cache dir makes the witness a
# FRESH compile (never an adopted artifact from an earlier run on the same machine).
os.environ.pop("PYTHSCRIBE_NO_JIT", None)
_CACHE = tempfile.mkdtemp(prefix="pythscribe-server-spot-")
os.environ["PYTHSCRIBE_CACHE"] = _CACHE
atexit.register(shutil.rmtree, _CACHE, True)  # best-effort cleanup (the wasm module is loaded in memory by then)

from pythscribe import binding_of, wasm  # noqa: E402  (after the env pins above, by design)


@wasm
def spot_sq(xs: list[float]) -> float:
    acc = 0.0
    for x in xs:
        acc = acc + x * x
    return acc


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    project = Path(argv[0]).resolve()
    assert project.is_dir(), f"project root not a directory: {project}"
    import pythscribe
    from pythscribe.runtime import wasmtime_available

    site = Path(pythscribe.__file__).resolve().parent
    # (1) the bare install carries the wasm runtime -- wasmtime is a CORE dependency, not an extra
    assert wasmtime_available(), (
        "wasmtime is NOT importable on this bare install: the `@wasm` server path would silently fall back to "
        "plain Python. wasmtime must be a CORE dependency in pyproject.toml [project].dependencies (0.2.9), "
        "never an extra. Run `pyths doctor`."
    )
    b = binding_of(spot_sq)
    py0, sv0 = b.counts()
    # (2) the kernel runs on the in-process SERVER path (compile-on-first-call with the bundled compiler)
    got = spot_sq([1.0, 2.0, 3.0])
    py1, sv1 = b.counts()
    assert got == 14.0, f"spot_sq([1,2,3]) == {got!r}, expected 14.0"
    assert b.mode == "server", (
        f"binding_of(spot_sq).mode == {b.mode!r} (reason: {b.mode_reason!r}; jit: {getattr(b, '_jit_reason', '')!r}) "
        "-- the bare install did NOT run `@wasm` on the server path"
    )
    assert (py1, sv1) == (py0, sv0 + 1), f"counters {(py0, sv0)} -> {(py1, sv1)}: the Python body ran / the WASM did not"
    assert b.artifact_status == "resolved" and b.server_ready, (b.artifact_status, b.server_ready)
    print(json.dumps({"server_spot": "GREEN", "mode": b.mode, "mode_reason": b.mode_reason, "value": got,
                      "wasmtime": True, "site": str(site), "cache": _CACHE}))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
