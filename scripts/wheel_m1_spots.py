#!/usr/bin/env python3
"""M1 EXIT SPOTs, run against an INSTALLED pythscribe wheel (spec 13-09-26, plan M1 exit; validation §B0/§B1).

    python scripts/wheel_m1_spots.py <project root>     # run with the venv's python

cibuildwheel runs this as CIBW_TEST_COMMAND on every leg (fresh venv, the just-built wheel
installed, cwd outside the project); tests/pythscribe/test_wheel_m1.py runs the SAME script
against the locally built + installed wheel, so the local exit evidence and the CI evidence
are one shipped-path witness. Asserted, on the REAL installed bytes:
  * `pythscribe` and the `pyths` console script resolve under this interpreter's site-packages /
    scripts dir (not the checkout);
  * `find_pyths()` returns the bundled `_bin` path (absolute; inside site-packages);
  * `pyths --version` == the compiler pin;
  * `pyths build hello.py` (hello.py staged ALONE in a temp cwd) exits 0 and the artifact
    self-verifies (`verify()`), compiler version == pin;
  * `pyths compile examples/hello/main.ps --stdout` FORWARDS (prints JS containing the program);
  * `pyths compile --help` forwards and shows the compile FLAG `--emit-cert`; `pyths --quiet
    compile ... --stdout` (global flag) forwards; `pyths cache status` produces native output;
  * `pyths --help` shows the native help AND the launcher section; `pyths init --help` is native.
Exit non-zero with the failing assertion on any miss.
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import sysconfig
import tempfile
from pathlib import Path


def _run(args: list[str], **kw) -> subprocess.CompletedProcess:
    return subprocess.run(args, capture_output=True, text=True, check=False, timeout=180, **kw)


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    project = Path(argv[0]).resolve()
    import pythscribe
    from pythscribe._pin import COMPILER_VERSION
    from pythscribe._static import find_wasm_defs, kernel_source, sha256_text
    from pythscribe.artifacts import verify
    from pythscribe.build import bundled_pyths, find_pyths

    site = Path(pythscribe.__file__).resolve().parent
    assert "site-packages" in str(site).replace("\\", "/"), f"pythscribe imported from {site}, not an installed wheel"
    scripts = Path(sysconfig.get_path("scripts"))
    pyths = shutil.which("pyths", path=str(scripts)) or shutil.which("pyths")
    assert pyths and Path(pyths).resolve().parent == scripts.resolve(), f"`pyths` console script not in {scripts}: {pyths}"
    # find_pyths -> the bundled binary
    got = find_pyths()
    bundled = bundled_pyths()
    assert bundled is not None and got == bundled and got.is_absolute() and got.parent == site / "_bin", (got, bundled)
    assert str(got).replace("\\", "/") != str(Path(pyths).resolve()).replace("\\", "/"), "find_pyths returned the console script"
    # --version == pin
    p = _run([pyths, "--version"])
    assert p.returncode == 0 and p.stdout.strip() == f"pyths {COMPILER_VERSION}", (p.returncode, p.stdout, p.stderr)
    # build hello.py in a temp cwd with ONLY hello.py staged
    with tempfile.TemporaryDirectory() as td:
        cwd = Path(td)
        shutil.copyfile(project / "examples" / "hello.py", cwd / "hello.py")
        p = _run([pyths, "build", "hello.py"], cwd=str(cwd))
        assert p.returncode == 0, (p.returncode, p.stdout, p.stderr)
        adir = cwd / "__pythscribe__" / "hello"
        assert (adir / "manifest.json").is_file() and (adir / "hello.wasm").is_file() and (adir / "hello.glue.js").is_file() and (adir / "hello.js").is_file(), sorted(os.listdir(adir))
        src = (cwd / "hello.py").read_text(encoding="utf-8")
        [node] = find_wasm_defs(src, "hello.py")
        info = verify(adir, function="hello", expected_source_sha256=sha256_text(kernel_source(src, node)))
        assert info.manifest["compiler"]["version"] == COMPILER_VERSION, info.manifest["compiler"]
        # forwarding: compile --stdout on the pinned .ps example
        p = _run([pyths, "compile", str(project / "examples" / "hello" / "main.ps"), "--stdout"], cwd=str(cwd))
        assert p.returncode == 0 and len(p.stdout) > 20, (p.returncode, p.stdout[:200], p.stderr[:400])
        p2 = _run([pyths, "--quiet", "compile", str(project / "examples" / "hello" / "main.ps"), "--stdout"], cwd=str(cwd))
        assert p2.returncode == 0 and p2.stdout == p.stdout, (p2.returncode, p2.stderr[:400])
    p = _run([pyths, "compile", "--help"])
    assert p.returncode == 0 and "--emit-cert" in p.stdout and "--stdout" in p.stdout, p.stdout[:400]
    p = _run([pyths, "cache", "status"])
    assert p.returncode == 0 and p.stdout.strip(), (p.returncode, p.stdout, p.stderr)
    p = _run([pyths, "--help"])
    assert p.returncode == 0 and "compile" in p.stdout and "pythscribe launcher commands" in p.stdout and "build <module.py>" in p.stdout, p.stdout
    p = _run([pyths, "init", "--help"])
    assert p.returncode == 0 and "Initialize a new PythScribe project" in p.stdout, p.stdout
    p = _run([pyths, "build", "--help"])
    assert p.returncode == 0 and p.stdout.startswith("usage: pyths build"), p.stdout[:200]
    print(json.dumps({"spots": "GREEN", "pyths": pyths, "bundled": str(got), "version": COMPILER_VERSION, "site": str(site)}))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
