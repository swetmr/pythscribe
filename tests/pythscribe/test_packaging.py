"""M1.5 §E packaging gates (bundle-all-in-one, lazy framework imports, licence files).

  P1  a wheel built from this checkout VENDORS the Gradio component (`gradio_wasmfunction` +
      its built templates), the FFI shim, the server runtime and the MIT licence -- and the
      sdist carries none of the Rust / Lean / npm trees;
  P2  PAIRED CONTROL (the spec's): in a fresh venv with NOTHING but that wheel installed --
      no gradio, no streamlit, no wasmtime -- `import pythscribe`, `pythscribe.gradio`,
      `pythscribe.streamlit`, `pythscribe.runtime` all import; `@wasm` on a kernel with an
      artifact resolves to mode 'browser' (server unavailable, reason names wasmtime) and
      without one to 'fallback'; touching `pythscribe.gradio.WasmFunction` raises the
      "pip install gradio" ImportError; the vendored component is importable once gradio is;
  P3  in THIS environment the vendored module is the one that resolves (no second dist).
Building the wheel needs the network for setuptools in the isolated build env; the venv
needs ~30 MB. Under PYTHSCRIBE_REQUIRE_ORACLE the gate is mandatory.
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path

import pytest

from conftest import REPO, gate

pytestmark = pytest.mark.skipif(os.environ.get("PYTHSCRIBE_SKIP_PACKAGING") == "1", reason="packaging gate skipped by env")


@pytest.fixture(scope="module")
def dist(tmp_path_factory) -> dict[str, Path]:
    out = tmp_path_factory.mktemp("dist")
    for what in ("--wheel", "--sdist"):
        p = subprocess.run([sys.executable, "-m", "build", what, "--outdir", str(out), str(REPO)], capture_output=True, text=True, check=False, cwd=str(REPO))
        gate(p.returncode == 0, f"python -m build {what} failed: {p.stderr[-2000:]}")
    [whl] = out.glob("*.whl")
    [sdist] = out.glob("*.tar.gz")
    return {"wheel": whl, "sdist": sdist}


def test_p1_wheel_vendors_everything_and_sdist_excludes_the_compiler_trees(dist):
    names = zipfile.ZipFile(dist["wheel"]).namelist()
    for needle in (
        "gradio_wasmfunction/__init__.py", "gradio_wasmfunction/wasmfunction.py", "gradio_wasmfunction/templates/component/index.js",
        "gradio_wasmfunction/templates/example/index.js", "pythscribe/ffi/list_buffer.mjs", "pythscribe/ffi/_node_runner.mjs",
        "pythscribe/build/_node_runner.mjs", "pythscribe/runtime/__init__.py", "pythscribe/runtime/_jsmath.py",
        "pythscribe/streamlit/__init__.py", "pythscribe/gradio/image.py", "pythscribe/LICENSE",
        # fix B: the in-tab image client is a runtime-required data file (browser.py reads it)
        "pythscribe/gradio/browser.py", "pythscribe/gradio/browser_image.js",
        "pythscribe/gradio/browser_scalar.js",  # fix A: the in-tab scalar client (client_side)
        # M3: the Streamlit component is vendored (frontend + byte-identical shim copy)
        "pythscribe/streamlit/wasm_component/index.html", "pythscribe/streamlit/wasm_component/list_buffer.mjs",
    ):
        assert any(n.endswith(needle) for n in names), needle
    lic = [n for n in names if n.endswith(".dist-info/licenses/pythscribe/LICENSE") or n.endswith(".dist-info/LICENSE")]
    assert lic, [n for n in names if "dist-info" in n]
    meta = next(n for n in names if n.endswith("METADATA"))
    text = zipfile.ZipFile(dist["wheel"]).read(meta).decode("utf-8")
    assert "License-Expression: MIT AND LicenseRef-FSL-1.1-ALv2" in text  # M5: the mixed wheel (tests/pythscribe/test_wheel_m5.py)
    assert "Requires-Dist: gradio" in text and 'extra == "gradio"' in text  # an EXTRA, never a base dep
    assert not any(line.startswith("Requires-Dist:") and "extra" not in line for line in text.splitlines()), "base deps must be empty"
    assert "Project-URL: Source, https://github.com/swetmr/pythscribe" in text
    assert "wasmtime" in text and 'extra == "server"' in text
    # the sdist is the pip package, not the monorepo
    members = tarfile.open(dist["sdist"]).getnames()
    assert not any("/crates/" in m or "/verification/" in m or "/npm/" in m or "/packages/" in m or "/examples/" in m or "/node_modules/" in m for m in members), [m for m in members if "/crates/" in m][:5]
    assert dist["sdist"].stat().st_size < 5_000_000, dist["sdist"].stat().st_size
    assert any(m.endswith("pythscribe/LICENSE") for m in members) and any(m.endswith("gradio_wasmfunction/templates/component/index.js") for m in members)


_PROBE = r'''
import importlib, importlib.util, json, sys, os
out = {"python": sys.version.split()[0]}
for m in ("gradio", "streamlit", "wasmtime"):
    try:
        importlib.import_module(m); out[m] = "PRESENT (control broken)"
    except ImportError:
        out[m] = "absent"
import pythscribe, pythscribe.gradio, pythscribe.streamlit, pythscribe.runtime, pythscribe.ffi, pythscribe.build
import pythscribe.gradio.browser  # fix B: importable without gradio; its client .js must be INSTALLED (read, not just listed)
from pythscribe import wasm, binding_of
out["version"] = pythscribe.__version__
out["browser_client_js_bytes"] = len(pythscribe.gradio.browser.CLIENT_JS.read_text(encoding="utf-8"))
out["scalar_client_js_bytes"] = len(pythscribe.gradio.browser.SCALAR_CLIENT_JS.read_text(encoding="utf-8"))  # fix A
out["client_side_importable"] = callable(pythscribe.gradio.client_side) and pythscribe.gradio.Arg.digits.produces == "list[int]"
out["wasmtime_available"] = pythscribe.runtime.wasmtime_available()
try:
    pythscribe.gradio.WasmFunction
    out["wasmfunction"] = "NO ERROR (control broken)"
except ImportError as e:
    out["wasmfunction"] = str(e)
try:
    pythscribe.streamlit.WasmComponent()  # M3: declaring the component needs streamlit
    out["st"] = "NO ERROR (control broken)"
except ImportError as e:
    out["st"] = str(e)
try:
    import gradio_wasmfunction  # vendored: present in site-packages, needs gradio to import
    out["vendored"] = "imported without gradio (control broken)"
except ImportError as e:
    out["vendored"] = "present but needs gradio: " + str(e)[:60]
spec = importlib.util.find_spec("gradio_wasmfunction")
out["vendored_path"] = spec.origin if spec else None
d = os.environ["PROBE_DIR"]
sys.path.insert(0, d)
import kernels_art, kernels_noart
ba, bn = binding_of(kernels_art.edit_distance), binding_of(kernels_noart.edit_distance)
out["art_mode"] = ba.mode; out["art_reason"] = ba.mode_reason; out["art_status"] = ba.artifact_status
out["noart_mode"] = bn.mode; out["noart_status"] = bn.artifact_status
out["value"] = kernels_art.edit_distance([1,2,3],[1,3],[0,0,0],[0,0,0])
out["python_calls"] = ba.calls(); out["server_calls"] = ba.server_runs()
print(json.dumps(out))
'''


def test_p2_import_pythscribe_in_a_venv_with_no_frameworks(dist, tmp_path):
    venv = tmp_path / "venv"
    p = subprocess.run([sys.executable, "-m", "venv", str(venv)], capture_output=True, text=True, check=False)
    assert p.returncode == 0, p.stderr
    py = venv / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    p = subprocess.run([str(py), "-m", "pip", "install", "--no-index", "--no-deps", str(dist["wheel"])], capture_output=True, text=True, check=False)
    assert p.returncode == 0, p.stderr[-2000:]
    probe_dir = tmp_path / "probe"
    probe_dir.mkdir()
    uc = REPO / "examples" / "wasm-use-cases"
    src = (uc / "kernels.py").read_text(encoding="utf-8")
    (probe_dir / "kernels_art.py").write_text(src, encoding="utf-8")
    (probe_dir / "kernels_noart.py").write_text(src, encoding="utf-8")
    gate((uc / "__pythscribe__" / "edit_distance" / "manifest.json").is_file(), "use-case artifacts not built")
    shutil.copytree(uc / "__pythscribe__", probe_dir / "__pythscribe__")  # artifacts for kernels_art.py -- but the manifest names kernels.py
    # the artifact dir is keyed by the SOURCE FILE's directory + function, not its file name, so both modules see it;
    # give the no-artifact module its own directory instead
    noart = tmp_path / "probe_noart"
    noart.mkdir()
    (probe_dir / "kernels_noart.py").unlink()
    (noart / "kernels_noart.py").write_text(src, encoding="utf-8")
    probe = _PROBE.replace('sys.path.insert(0, d)\n', f'sys.path.insert(0, d); sys.path.insert(0, {str(noart)!r})\n')
    (tmp_path / "probe.py").write_text(probe, encoding="utf-8")
    p = subprocess.run([str(py), "-I", str(tmp_path / "probe.py")], capture_output=True, text=True, check=False, env={**os.environ, "PROBE_DIR": str(probe_dir), "PYTHONUTF8": "1"}, cwd=str(tmp_path))
    assert p.returncode == 0, p.stderr[-3000:]
    out = json.loads(p.stdout.strip().splitlines()[-1])
    assert out["gradio"] == "absent" and out["streamlit"] == "absent" and out["wasmtime"] == "absent", out  # the control's premise
    assert out["wasmtime_available"] is False
    assert "pip install gradio" in out["wasmfunction"], out
    assert "pip install streamlit" in out["st"], out
    assert out["vendored"].startswith("present but needs gradio"), out
    assert "site-packages" in out["vendored_path"].replace("\\", "/") and "gradio_wasmfunction" in out["vendored_path"], out
    assert out["art_mode"] == "browser" and "wasmtime-py is not installed" in out["art_reason"] and out["art_status"] == "resolved", out
    assert out["browser_client_js_bytes"] > 1000, out  # fix B: the in-tab client .js is installed with the wheel
    assert out["scalar_client_js_bytes"] > 1000 and out["client_side_importable"] is True, out  # fix A: likewise, gradio-free
    assert out["noart_mode"] == "fallback" and out["noart_status"] == "absent", out
    assert out["value"] == 1 and out["python_calls"] == 1 and out["server_calls"] == 0, out


def test_p3_vendored_component_is_the_one_that_resolves_here():
    import importlib.util

    spec = importlib.util.find_spec("gradio_wasmfunction")
    gate(spec is not None and spec.origin is not None, "gradio_wasmfunction not importable (pip install -e .)")
    origin = Path(spec.origin).resolve()
    vendored = (REPO / "pythscribe" / "gradio" / "wasm_function" / "backend" / "gradio_wasmfunction" / "__init__.py").resolve()
    assert origin == vendored or "site-packages" in str(origin), origin
    assert (vendored.parent / "templates" / "component" / "index.js").is_file()
    # and no SECOND distribution provides the same import name (the M0 two-step is gone)
    import importlib.metadata as md

    # a malformed dist in the env can have a None `Name` (editable/namespace installs) -> coalesce, don't crash
    dists = [d.metadata["Name"] for d in md.distributions() if (d.metadata["Name"] or "").lower().replace("-", "_") == "gradio_wasmfunction"]
    assert dists == [], f"separate gradio_wasmfunction distribution(s) still installed: {dists} (pip uninstall gradio_wasmfunction; pip install -e .)"


def test_p3b_pythscribe_adapter_modules_import_framework_free():
    """Fresh interpreter: `import pythscribe.gradio` / `pythscribe.streamlit` must not import
    their framework (lazy) -- an explicit negative control with `-I` and BOTH gradio AND
    streamlit blocked at import (review SF4: the Streamlit adapter reuses helpers from
    pythscribe.gradio, so the base-install control must block both to be meaningful)."""
    code = (
        "import sys\n"
        "class Block:\n"
        "    def find_spec(self, name, path=None, target=None):\n"
        "        if name in ('gradio','streamlit') or name.startswith(('gradio.','streamlit.')): raise ImportError(name+' blocked by the test')\n"
        "sys.meta_path.insert(0, Block())\n"
        "import pythscribe.gradio, pythscribe.streamlit\n"
        "from pythscribe.gradio import dispatch, result_of, describe, dispatch_image\n"
        "from pythscribe.streamlit import dispatch as sd, result_of as sr, describe as sde, deadline_result\n"
        "try:\n    pythscribe.gradio.WasmFunction\n    print('G NOERR')\nexcept ImportError as e:\n    print('G', 'pip install gradio' in str(e))\n"
        "try:\n    pythscribe.streamlit.WasmComponent()\n    print('S NOERR')\nexcept ImportError as e:\n    print('S', 'pip install streamlit' in str(e))\n"
    )
    p = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, check=False, cwd=str(REPO))
    assert p.returncode == 0, p.stderr[-2000:]
    assert p.stdout.strip().splitlines() == ["G True", "S True"], p.stdout


def test_licensing_files_and_map():
    mit = "Permission is hereby granted, free of charge"
    for rel in ("pythscribe/LICENSE", "runtime/LICENSE", "crates/pyths_runtime/js/LICENSE"):
        text = (REPO / rel).read_text(encoding="utf-8")
        assert text.startswith("MIT License") and mit in text, rel
    assert (REPO / "pythscribe" / "LICENSE").read_bytes() == (REPO / "runtime" / "LICENSE").read_bytes()
    fsl = (REPO / "LICENSE.md").read_text(encoding="utf-8")
    assert "Functional Source License" in fsl
    m = (REPO / "LICENSING.md").read_text(encoding="utf-8")
    for needle in ("`crates/`", "FSL-1.1-ALv2", "`runtime/`", "`pythscribe/`", "**MIT**", "pythscribe/LICENSE", "runtime/LICENSE", "crates/pyths_runtime/js/LICENSE", "gradio_wasmfunction"):
        assert needle in m, needle
    py = (REPO / "pyproject.toml").read_text(encoding="utf-8")
    # M5: the mixed wheel -- the full license-files list is asserted in test_wheel_m5.py
    assert 'license = "MIT AND LicenseRef-FSL-1.1-ALv2"' in py and '"pythscribe/LICENSE",' in py and '"LICENSE.md",' in py and 'readme = "pythscribe/README.md"' in py
    assert "pip wheel composition" in m and "LicenseRef-FSL-1.1-ALv2" in m and "nodejs-wheel-binaries" in m
    root = (REPO / "README.md").read_text(encoding="utf-8")
    assert "LICENSING.md" in root and "pythscribe/README.md" in root  # the deep pip reference stays pythscribe/README.md
    # M6.4 (spec 13-09-26): the root README is pip-PRIMARY -- Installation leads with `pip install pythscribe`
    # (the whole toolchain, node-free) and the mixed-licence line; npm is the parallel mirror (test_wheel_m6_evidence.py)
    inst = root.split("## Installation", 1)[1]
    assert inst.lstrip().startswith("**One `pip install` is the whole toolchain**")
    assert "MIT AND LicenseRef-FSL-1.1-ALv2" in inst
