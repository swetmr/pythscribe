#!/usr/bin/env python3
"""The `testpypi-validate` leg driver -> `legs/<triple>/` (spec 13-09-26, plan M6.3; validation §E3 R-TV + §L5).

    python scripts/testpypi_validate.py --manifest release_manifest.json --target <triple> \
        --index-url https://test.pypi.org/simple/ --extra-index-url https://pypi.org/simple/ \
        --out legs/<triple> --checkout <repo>

Runs ON the target's runner (publish-pypi.yml matrix). It validates the PUBLIC artifact, never a local build:
  1. `pip download pythscribe==<V> --no-deps --only-binary=:all: --index-url <TestPyPI>` -> exactly the
     manifest-named wheel for this target; sha256 == manifest `wheel_sha256` BEFORE anything is installed
     (download.json; a wheel that is not the manifest-bound bytes is refused -- never installed);
  2. a fresh venv; `pip install "<downloaded wheel>[server]"` (deps from the extra index); assert pythscribe
     imports from the venv's site-packages (never a checkout);
  3. A0 -- PATH scrubbed to the system dirs + the venv scripts dir and NO node file under the venv
     (scripts/wheel_acceptance.py a0 --root <venv>) -> a0.json. Topology `native-scrubbed-path`: the OS-user
     boundary / container isolation is R-BA's job on the SAME bytes (wheel sha == manifest == R-BA's wheel), so
     R-BA's node-free verdict TRANSFERS by identity; this leg proves the public artifact installs + runs;
  4. A1..A4 through the shipped path (scripts/wheel_acceptance.py run, cwd outside any checkout) -> leg.json;
  5. the L5 all-features smoke (tests/pythscribe/test_all_features_smoke.py, PYTHSCRIBE_REQUIRE_ORACLE=1) run by
     the venv's pytest from a cwd outside the checkout -> smoke.json (`all_features` in R-TV).
Every step writes its JSON even on failure (the assembler turns a missing file into a fail; nothing is skipped
silently). Exit 0 iff every step passed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import sysconfig
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SYSTEM_PATH = "C:\\Windows\\System32" if os.name == "nt" else "/usr/bin:/bin"
SMOKE_TEST = "tests/pythscribe/test_all_features_smoke.py"


def sha256_file(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def _write(out: Path, name: str, payload: dict) -> None:
    out.mkdir(parents=True, exist_ok=True)
    (out / name).write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def _run(args: list[str], **kw) -> subprocess.CompletedProcess:
    return subprocess.run([str(a) for a in args], capture_output=True, text=True, check=False, **kw)


# ----------------------------------------------------------------------------- 1. download + bind


def check_download(dl_dir: Path, manifest: dict, target: str, index_url: str) -> dict:
    """download.json: exactly the manifest-named wheel, hashing to the manifest -- or a fail verdict."""
    info = manifest["targets"][target]
    wheels = sorted(p.name for p in dl_dir.glob("*.whl")) if dl_dir.is_dir() else []
    problems: list[str] = []
    if wheels != [info["wheel_filename"]]:
        problems.append(f"downloaded {wheels}, expected exactly [{info['wheel_filename']}]")
    sha = sha256_file(dl_dir / info["wheel_filename"]) if (dl_dir / info["wheel_filename"]).is_file() else None
    if sha != info["wheel_sha256"]:
        problems.append(f"sha256 of the fetched wheel {sha} != manifest wheel_sha256 {info['wheel_sha256']} -- refusing to install")
    return {"step": "download", "index_url": index_url, "wheel_filename": info["wheel_filename"], "installed_wheel_sha256": sha,
            "manifest_wheel_sha256": info["wheel_sha256"], "problems": problems, "verdict": "pass" if not problems else "fail"}


def pip_download(python: Path, version: str, index_url: str, dest: Path,
                 attempts: int = 18, backoff_s: float = 10.0) -> subprocess.CompletedProcess:
    # The workflow's pre-wait polls only the registry JSON API; pip resolves off the SEPARATE Simple/index
    # API, which propagates independently -- so on a first-ever publish the exact `==version` can 404 here for
    # a short window even after the JSON API reports it. Retry the download (never the hash bind) so the
    # propagation lag is absorbed instead of failing the (irreversible-upstream) validate leg. codex 2026-09-17.
    args = [python, "-m", "pip", "download", f"pythscribe=={version}", "--no-deps", "--only-binary=:all:",
            "--index-url", index_url, "--dest", dest]
    r = _run(args, timeout=900)
    for i in range(1, attempts):
        if r.returncode == 0:
            return r
        blob = (r.stderr or "") + (r.stdout or "")
        # only retry the propagation-lag signature; a real resolver/network error still fails fast on its own merits
        if not re.search(r"(?i)no matching distribution|could not find a version|404|not found", blob):
            return r
        print(f"[download] pythscribe=={version} not yet on the index ({index_url}); retry {i}/{attempts - 1}")
        time.sleep(backoff_s)
        r = _run(args, timeout=900)
    return r


# ----------------------------------------------------------------------------- 2. venv


def venv_paths(venv: Path) -> tuple[Path, Path]:
    if os.name == "nt":
        return venv / "Scripts" / "python.exe", venv / "Scripts"
    return venv / "bin" / "python", venv / "bin"


def make_venv(venv: Path) -> tuple[Path, Path]:
    r = _run([sys.executable, "-m", "venv", venv])
    if r.returncode != 0:
        raise RuntimeError(f"venv creation failed: {r.stderr[-1500:]}")
    return venv_paths(venv)


def site_of(python: Path) -> Path:
    r = _run([python, "-c", "import sysconfig; print(sysconfig.get_path('purelib'))"])
    return Path(r.stdout.strip())


def scrubbed_env(scripts: Path, **extra: str) -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if k not in ("PYTHONPATH", "PYTHSCRIBE_PYTHS", "PYTHSCRIBE_NO_JIT", "PYTHSCRIBE_MODE", "PYTHSCRIBE_RUNTIME_DIR")}
    env["PATH"] = os.pathsep.join([str(scripts), SYSTEM_PATH])
    env.update(extra)
    return env


# ----------------------------------------------------------------------------- 5. the all-features smoke


def smoke_summary(returncode: int, stdout: str, pythscribe_file: str, site: Path) -> dict:
    """smoke.json from the pytest run: pass iff exit 0, >0 tests passed, none failed/errored, and pythscribe was
    imported from the venv (never the checkout)."""
    m = re.search(r"(\d+) passed", stdout)
    f = re.search(r"(\d+) (?:failed|error)", stdout)
    passed = int(m.group(1)) if m else 0
    failed = int(f.group(1)) if f else 0
    problems: list[str] = []
    if returncode != 0:
        problems.append(f"pytest exit {returncode}")
    if passed == 0:
        problems.append("no test passed (the smoke did not run)")
    if failed:
        problems.append(f"{failed} failed/errored")
    try:
        Path(pythscribe_file).resolve().relative_to(site.resolve())
    except ValueError:
        problems.append(f"pythscribe imported from {pythscribe_file}, not the venv site-packages {site}")
    return {"step": "all_features", "tests": SMOKE_TEST, "passed": passed, "failed": failed, "pythscribe_file": pythscribe_file,
            "problems": problems, "verdict": "pass" if not problems else "fail"}


# ----------------------------------------------------------------------------- driver


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="testpypi_validate.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--manifest", required=True)
    ap.add_argument("--target", required=True)
    ap.add_argument("--index-url", default="https://test.pypi.org/simple/")
    ap.add_argument("--extra-index-url", default="https://pypi.org/simple/")
    ap.add_argument("--out", required=True)
    ap.add_argument("--checkout", default=str(REPO))
    ap.add_argument("--work", default=None)
    ns = ap.parse_args(argv)
    manifest = json.loads(Path(ns.manifest).read_text(encoding="utf-8"))
    # ABSOLUTE: the a0/run legs run wheel_acceptance.py as a subprocess with a CHANGED cwd (work / a temp
    # cwd), so a relative --out would resolve under THAT cwd and this reader would miss it ("a0/run wrote
    # nothing"). Same relative-path-under-changed-cwd class as verify_npm_identity's work dir (codex 2026-09-18).
    out = Path(ns.out).resolve()
    work = Path(ns.work).resolve() if ns.work else Path(tempfile.mkdtemp(prefix="tpv-"))
    checkout = Path(ns.checkout).resolve()
    version, target = manifest["version"], ns.target
    ok = True

    # 1. download + bind
    dl = work / "dl"
    r = pip_download(Path(sys.executable), version, ns.index_url, dl)
    rec = check_download(dl, manifest, target, ns.index_url)
    if r.returncode != 0:
        rec["problems"].append(f"pip download failed: {(r.stderr or r.stdout)[-1200:]}")
        rec["verdict"] = "fail"
    _write(out, "download.json", rec)
    print(f"[download] {rec['verdict']} {rec.get('wheel_filename')} {rec.get('installed_wheel_sha256')}")
    if rec["verdict"] != "pass":
        for n in ("a0.json", "leg.json", "smoke.json"):
            _write(out, n, {"verdict": "fail", "problems": ["not run: the downloaded wheel is not the manifest-bound artifact"]})
        return 1
    wheel = dl / rec["wheel_filename"]

    # 2. venv + install the PUBLIC wheel by absolute path (deps from the extra index)
    py, scripts = make_venv(work / "venv")
    r = _run([py, "-m", "pip", "install", f"{wheel}[server]", "--index-url", ns.extra_index_url], timeout=900)
    if r.returncode != 0:
        _write(out, "leg.json", {"verdict": "fail", "problems": [f"pip install failed: {r.stderr[-1500:]}"]})
        _write(out, "a0.json", {"verdict": "fail", "problems": ["not run: install failed"]})
        _write(out, "smoke.json", {"verdict": "fail", "problems": ["not run: install failed"]})
        return 1
    site = site_of(py)
    env = scrubbed_env(scripts)

    # 3. A0 -- scrubbed PATH + no node file under the venv
    r = _run([py, checkout / "scripts" / "wheel_acceptance.py", "a0", "--root", work / "venv", "--out", out / "a0.json"], env=env, timeout=600, cwd=str(work))
    a0 = json.loads((out / "a0.json").read_text(encoding="utf-8")) if (out / "a0.json").is_file() else {"verdict": "fail", "problems": ["a0 wrote nothing"]}
    a0["topology"] = "native-scrubbed-path"
    _write(out, "a0.json", a0)
    ok &= a0.get("verdict") == "pass"
    print(f"[A0] {a0.get('verdict')}")

    # 4. A1..A4 through the shipped path, cwd outside the checkout, only hello.py + the runner staged
    cwd = work / "cwd"
    cwd.mkdir(exist_ok=True)
    shutil.copy(checkout / "examples" / "hello.py", cwd / "hello.py")
    shutil.copy(checkout / "scripts" / "wheel_acceptance.py", cwd / "wheel_acceptance.py")
    r = _run([py, cwd / "wheel_acceptance.py", "run", "--manifest", Path(ns.manifest).resolve(), "--target", target, "--checkout", checkout,
              "--hello", cwd / "hello.py", "--out", out / "leg.json"], env=env, timeout=900, cwd=str(cwd))
    leg = json.loads((out / "leg.json").read_text(encoding="utf-8")) if (out / "leg.json").is_file() else {"steps": {}, "problems": ["run wrote nothing"]}
    ok &= r.returncode == 0 and all(leg.get("steps", {}).get(s) == "pass" for s in ("A1", "A2", "A3", "A4"))
    print(f"[A1..A4] {leg.get('steps')} {leg.get('problems') or ''}")

    # 5. the all-features smoke against the installed PUBLIC artifact
    dep = _run([py, "-m", "pip", "install", "pytest>=8", "pytest-timeout>=2", "numpy>=1.26", "gradio>=6,<7", "--index-url", ns.extra_index_url], timeout=900)
    if dep.returncode != 0:
        # a failed smoke-dep install is a STRUCTURED failure, not a crash into a broken pytest env (codex 2026-09-18)
        _write(out, "smoke.json", {"verdict": "fail", "problems": [f"smoke dependency install failed (pytest/numpy/gradio): {(dep.stderr or dep.stdout)[-1500:]}"]})
        ok = False
        print("[all-features] fail: smoke dependency install failed")
    else:
        smoke_cwd = work / "smoke"
        smoke_cwd.mkdir(exist_ok=True)
        pf = _run([py, "-c", "import pythscribe; print(pythscribe.__file__)"], env=env, cwd=str(smoke_cwd))
        pytest_exe = shutil.which("pytest", path=str(scripts)) or str(scripts / "pytest")
        senv = dict(env, PYTHSCRIBE_REQUIRE_ORACLE="1")
        r = _run([pytest_exe, str(checkout / SMOKE_TEST), "-p", "no:cacheprovider", "-q", "--rootdir", str(checkout), "-c", str(checkout / "pyproject.toml")], env=senv, timeout=1800, cwd=str(smoke_cwd))
        smoke = smoke_summary(r.returncode, r.stdout + r.stderr, pf.stdout.strip(), site)
        smoke["stdout_tail"] = (r.stdout + r.stderr)[-3000:]
        _write(out, "smoke.json", smoke)
        ok &= smoke["verdict"] == "pass"
        print(f"[all-features] {smoke['verdict']} passed={smoke['passed']} failed={smoke['failed']}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
