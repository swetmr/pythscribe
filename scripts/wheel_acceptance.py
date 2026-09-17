#!/usr/bin/env python3
"""M4 app-side acceptance runner (spec 13-09-26, plan M4; validation §A) -- runs INSIDE the node-free
app environment (the Linux app container, or the `app` OS user on a native macOS/Windows leg) with the
candidate wheel's own venv python. It imports ONLY the installed `pythscribe` (never the checkout) and
the standard library; the workflow copies this file into the temp cwd that holds `hello.py`.

    python wheel_acceptance.py a0     [--allow-under DIR ...] [--probe CMD] [--root DIR ...] --out a0.json
    python wheel_acceptance.py bind   --wheel W --manifest M --target TRIPLE --wheelhouse DIR --out bind.json
    python wheel_acceptance.py run    --manifest M --target TRIPLE --checkout DIR --hello hello.py --out leg.json
    python wheel_acceptance.py post-a5 --sampler-log FILE [--allow-under DIR ...] [--probe CMD] [--root DIR ...] --out post.json
    python wheel_acceptance.py sampler --out FILE          (diagnostic /proc sampler; NEVER the verdict)

Steps (each recorded as pass/fail in the leg JSON; any fail exits non-zero):
  A0   node ABSENCE, explicit: no `node`/`npm`/`npx` on PATH AND every `node`/`node.exe`/`nodejs*` FILE
       found on the box is (Linux) nonexistent / (native legs) under an allowed root AND refused by the
       `app` probe (validation §A A0, rev-8). Names, not contents: a Playwright driver's bundled node
       is a file named `node` -> found -> RED (the Playwright-in-app control).
  A0b  candidate binding: sha256(W) == manifest.targets[t].wheel_sha256 BEFORE install; the wheelhouse
       holds NO other `pythscribe-*` distribution and its one pythscribe wheel IS W (bytes).
  A1   the installed package resolves under the venv (pythscribe.__file__ under site-packages, `pyths`
       under the venv scripts dir, neither inside the checkout) and the ACTUAL installed bytes of
       pythscribe/_bin/pyths[.exe] hash == the RECORD digest == manifest native_sha256[t] (rev-5 SF-3).
  A2   `pyths build hello.py` (only hello.py staged) -> artifact {js, glue.js, wasm, manifest};
       verify() passes; compiler version == pin; find_pyths() == the bundled _bin.
  A3   the artifact runs under wasmtime in-process: mode server, float_bits(hello(1.0)) == float_bits(3.0).
  A4   first-call witness (validation §G): beside-source artifact removed, PYTHSCRIBE_CACHE cold, fresh
       import; server_calls +1, python_calls +0, mode server, exactly one keyed cache dir, no node spawn.
"""
from __future__ import annotations

import argparse
import base64
import csv
import hashlib
import importlib.util
import io
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import sysconfig
import tempfile
import time
import zipfile
from pathlib import Path
from typing import Callable, Iterable

NODE_NAMES = ("node", "node.exe")
NODE_PREFIX = "nodejs"
PATH_NAMES = ("node", "npm", "npx")
_WHEEL = re.compile(r"^pythscribe-(?P<ver>[^-]+)-py3-none-(?P<plat>[^-]+)\.whl$")


def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def _under(p: Path, root: Path) -> bool:
    try:
        p.resolve().relative_to(root.resolve())
        return True
    except ValueError:
        return False


# ----------------------------------------------------------------------------- A0: node absence


def is_node_name(name: str) -> bool:
    n = name.lower()
    return n in NODE_NAMES or n.startswith(NODE_PREFIX)


def find_node_files(roots: Iterable[Path], *, skip: Iterable[Path] = ()) -> list[Path]:
    """Every regular FILE named node / node.exe / nodejs* under `roots` (symlinks to files included;
    the walk never follows directory symlinks, so a bind-mounted checkout cannot mask a hit).
    `skip` prunes pseudo filesystems (/proc, /sys) -- never a place a binary lives."""
    skip_r = [Path(s) for s in skip]
    found: list[Path] = []
    for root in roots:
        root = Path(root)
        if root.is_file():
            if is_node_name(root.name):
                found.append(root)
            continue
        for dirpath, dirnames, filenames in os.walk(root, followlinks=False, onerror=lambda e: None):
            d = Path(dirpath)
            if any(_under(d, s) for s in skip_r):
                dirnames[:] = []
                continue
            for fn in filenames:
                if is_node_name(fn):
                    p = d / fn
                    if p.is_file() and not _is_noninterpreter(p):
                        found.append(p)
    return sorted(set(found))


# CodeQL source/doc extensions -- NodeJS.qll / NodeJSLib.model.yml / NodeJS.qhelp lowercase to `nodejs...`
# and drift onto the CI runner image (the M4 macOS A0 false positive).
_NODE_CODEQL_SUFFIXES = (".qll", ".ql", ".qhelp", ".model.yml")


def _is_noninterpreter(p: Path) -> bool:
    """A node-NAMED file that is provably NOT an interpreter, so A0 must not flag it. Recognised by a
    CodeQL source suffix (NodeJS.qll / NodeJSLib.model.yml) OR a Homebrew formula-alias symlink (nodejs ->
    Formula/node.rb) -- AND, decisively, confirmed NON-EXECUTABLE. Executability is the discriminator: any
    +x file stays in scope (a real interpreter, or an adversarially-renamed node binary named `nodejs.qll`
    / symlinked `node -> runtime.rb`, is still found + PROBED -- never a false negative; codex 2026-09-17).
    The exact names node/node.exe/nodejs are handled by is_node_name and are never routed here."""
    try:
        if os.access(p, os.X_OK):     # executable -> a candidate interpreter, NEVER excluded
            return False
        n = p.name.lower()
        if n.endswith(_NODE_CODEQL_SUFFIXES):                          # non-+x CodeQL text artifact
            return True
        if p.is_symlink() and p.resolve().suffix.lower() == ".rb":     # non-+x Homebrew formula alias
            return True
        return False
    except OSError:
        return False


def which_on_path(names: Iterable[str] = PATH_NAMES, path_env: str | None = None) -> dict[str, str]:
    env_path = os.environ.get("PATH", "") if path_env is None else path_env
    hits: dict[str, str] = {}
    for n in names:
        w = shutil.which(n, path=env_path)
        if w:
            hits[n] = w
    return hits


def a0_verdict(found: Iterable[Path], allowed_roots: Iterable[Path] = (), probe: Callable[[Path], tuple[bool, str]] | None = None,
               path_hits: dict[str, str] | None = None) -> list[str]:
    """RED lines. Linux legs: allowed_roots == () -> ANY found file is RED. Native legs: each found file
    must be under an allowed root (~ctl / the runner's externals/) AND the `app` probe must report it
    DENIED (open EACCES + exec refused); a file outside the roots, or one the probe can reach, is RED."""
    p: list[str] = []
    for n, w in sorted((path_hits or {}).items()):
        p.append(f"A0: `{n}` is on PATH at {w} (node leaked)")
    roots = [Path(r) for r in allowed_roots]
    for f in found:
        f = Path(f)
        if not roots:
            p.append(f"A0: node file present on the filesystem: {f}")
            continue
        if not any(_under(f, r) for r in roots):
            p.append(f"A0: node file OUTSIDE the allowed roots {[str(r) for r in roots]}: {f}")
            continue
        if probe is None:
            p.append(f"A0: native leg without an `app` probe cannot certify {f} (fail-closed)")
            continue
        denied, detail = probe(f)
        if not denied:
            p.append(f"A0: node file REACHABLE by `app`: {f} ({detail})")
    return p


def _probe_via_command(cmd: str) -> Callable[[Path], tuple[bool, str]]:
    """Run `<cmd> <file>` (the platform-correct boundary probe, scripts/native_node_boundary.py probe-one);
    exit 0 == denied, anything else == reachable (fail-closed: a broken probe is a reachable verdict)."""
    def probe(f: Path) -> tuple[bool, str]:
        r = subprocess.run(cmd.split() + [str(f)], capture_output=True, text=True, timeout=120)
        return (r.returncode == 0, (r.stdout + r.stderr).strip()[-400:])
    return probe


# ----------------------------------------------------------------------------- A0b: candidate binding


def check_candidate(wheel: Path, manifest: dict, target: str) -> list[str]:
    """sha256(W) == manifest.targets[t].wheel_sha256 and the filename is the manifest-named one."""
    p: list[str] = []
    info = (manifest.get("targets") or {}).get(target)
    if not isinstance(info, dict):
        return [f"A0b: manifest has no targets[{target}]"]
    if not wheel.is_file():
        return [f"A0b: candidate wheel {wheel} does not exist"]
    if wheel.name != info.get("wheel_filename"):
        p.append(f"A0b: wheel filename {wheel.name} != manifest-named {info.get('wheel_filename')}")
    got = sha256_file(wheel)
    if got != info.get("wheel_sha256"):
        p.append(f"A0b: sha256({wheel.name}) = {got} != manifest wheel_sha256 {info.get('wheel_sha256')} -- refusing to install")
    m = _WHEEL.match(wheel.name)
    if not m or m.group("plat") != info.get("wheel_tag"):
        p.append(f"A0b: {wheel.name} is not py3-none-{info.get('wheel_tag')}")
    return p


def check_wheelhouse(wheelhouse: Path, wheel: Path) -> list[str]:
    """The offline wheelhouse holds exactly ONE pythscribe distribution and it is W (bytes)."""
    p: list[str] = []
    ours = [q for q in wheelhouse.iterdir() if q.name.lower().startswith("pythscribe-") or q.name.lower().startswith("pythscribe_")]
    if not ours:
        return [f"A0b: wheelhouse {wheelhouse} holds no pythscribe distribution"]
    want = sha256_file(wheel)
    for q in sorted(ours):
        if q.name != wheel.name:
            p.append(f"A0b: FOREIGN pythscribe distribution in the wheelhouse: {q.name} (only {wheel.name} may be present)")
        elif sha256_file(q) != want:
            p.append(f"A0b: wheelhouse copy of {q.name} differs from the candidate (bytes)")
    return p


# ----------------------------------------------------------------------------- A1: installed identity


def record_digest(site: Path, relpath: str) -> str | None:
    """The `sha256=<urlsafe-b64, unpadded>` digest pip wrote for `relpath` into RECORD, as hex."""
    for di in site.glob("pythscribe-*.dist-info"):
        rec = di / "RECORD"
        if not rec.is_file():
            continue
        for row in csv.reader(io.StringIO(rec.read_text(encoding="utf-8"))):
            if len(row) >= 2 and row[0].replace("\\", "/") == relpath.replace("\\", "/"):
                algo, _, digest = row[1].partition("=")
                if algo != "sha256" or not digest:
                    return None
                return base64.urlsafe_b64decode(digest + "=" * (-len(digest) % 4)).hex()
    return None


def check_installed(site: Path, scripts_dir: Path, manifest: dict, target: str, *, checkout: Path | None,
                    pythscribe_file: Path, pyths_path: Path | None) -> list[str]:
    p: list[str] = []
    if not _under(pythscribe_file, site):
        p.append(f"A1: pythscribe imported from {pythscribe_file}, not the venv site-packages {site}")
    if checkout is not None and _under(pythscribe_file, checkout):
        p.append(f"A1: pythscribe.__file__ {pythscribe_file} is INSIDE the checkout (checkout-shadowing)")
    if pyths_path is None or not _under(pyths_path, scripts_dir):
        p.append(f"A1: `pyths` resolves to {pyths_path}, not under the venv scripts dir {scripts_dir}")
    binname = "pyths.exe" if os.name == "nt" else "pyths"
    binary = site / "pythscribe" / "_bin" / binname
    if not binary.is_file():
        return p + [f"A1: installed compiler {binary} is missing"]
    want = (manifest.get("targets") or {}).get(target, {}).get("native_sha256")
    got_bytes = sha256_file(binary)
    got_record = record_digest(site, f"pythscribe/_bin/{binname}")
    if got_record is None:
        p.append("A1: RECORD has no sha256 row for pythscribe/_bin/" + binname)
    elif got_record != got_bytes:
        p.append(f"A1: installed BYTES {got_bytes} != RECORD digest {got_record} (swapped binary after install)")
    if got_bytes != want:
        p.append(f"A1: installed BYTES {got_bytes} != manifest native_sha256[{target}] {want}")
    return p


# ----------------------------------------------------------------------------- A2-A4 (shipped path)


def _import_file(path: Path, name: str):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    try:
        spec.loader.exec_module(mod)
    finally:
        sys.modules.pop(name, None)
    return mod


def _keyed_dirs(cache: Path, version: str) -> set[Path]:
    root = cache / version
    if not root.is_dir():
        return set()
    return {sha for optid in root.iterdir() if optid.is_dir() for sha in optid.iterdir() if sha.is_dir()}


def _refuse_network() -> None:
    def refuse(self, *a, **k):
        raise AssertionError("network access attempted inside the node-free acceptance")
    socket.socket.connect = refuse  # type: ignore[method-assign]
    socket.socket.connect_ex = refuse  # type: ignore[method-assign]


def run_a2_a4(hello: Path, manifest: dict, target: str, *, checkout: Path | None) -> dict:
    """A1 (origins + bytes) then A2/A3/A4 through the SHIPPED path. Returns the per-target block."""
    import pythscribe
    from pythscribe import binding_of
    from pythscribe._pin import COMPILER_VERSION
    from pythscribe._static import find_wasm_defs, kernel_source, sha256_text
    from pythscribe.artifacts import verify
    from pythscribe.build import bundled_pyths, find_pyths
    from pythscribe.build.runner import float_bits

    steps: dict[str, str] = {}
    problems: list[str] = []
    site = Path(sysconfig.get_path("purelib")).resolve()
    scripts_dir = Path(sysconfig.get_path("scripts")).resolve()
    pyths = shutil.which("pyths", path=str(scripts_dir)) or shutil.which("pyths")
    p1 = check_installed(site, scripts_dir, manifest, target, checkout=checkout,
                         pythscribe_file=Path(pythscribe.__file__).resolve(), pyths_path=Path(pyths).resolve() if pyths else None)
    problems += p1
    steps["A1"] = "fail" if p1 else "pass"
    binname = "pyths.exe" if os.name == "nt" else "pyths"
    binary = site / "pythscribe" / "_bin" / binname
    out = {
        "steps": steps, "problems": problems, "compiler_version": COMPILER_VERSION,
        "installed_binary_sha256": sha256_file(binary) if binary.is_file() else None,
        "record_sha256": record_digest(site, f"pythscribe/_bin/{binname}"),
        "manifest_native_sha256": (manifest.get("targets") or {}).get(target, {}).get("native_sha256"),
        "site_packages": str(site), "pyths": pyths,
    }
    _refuse_network()
    spawned: list[str] = []
    real_run, real_popen = subprocess.run, subprocess.Popen

    def rec_run(args, *a, **k):
        spawned.append(Path(str(args[0])).name if isinstance(args, (list, tuple)) else str(args))
        return real_run(args, *a, **k)

    class rec_popen(real_popen):  # type: ignore[misc,valid-type]
        def __init__(self, args, *a, **k):
            spawned.append(Path(str(args[0])).name if isinstance(args, (list, tuple)) else str(args))
            super().__init__(args, *a, **k)

    subprocess.run, subprocess.Popen = rec_run, rec_popen  # type: ignore[assignment]
    cwd = hello.parent
    # ---- A2: explicit build, the ONLY staged file is hello.py
    try:
        others = [q.name for q in cwd.iterdir() if q.name not in ("hello.py", "wheel_acceptance.py", "__pycache__")]
        if others:
            raise AssertionError(f"temp cwd holds more than hello.py: {others}")
        got = find_pyths()
        if bundled_pyths() != got or not _under(got, site):
            raise AssertionError(f"find_pyths() = {got}, bundled = {bundled_pyths()}")
        r = real_run([pyths, "build", "hello.py"], cwd=str(cwd), capture_output=True, text=True, timeout=600)
        if r.returncode != 0:
            raise AssertionError(f"pyths build failed ({r.returncode}): {r.stderr[-1500:]}")
        adir = cwd / "__pythscribe__" / "hello"
        for fn in ("hello.js", "hello.glue.js", "hello.wasm", "manifest.json"):
            if not (adir / fn).is_file():
                raise AssertionError(f"artifact lacks {fn}: {sorted(os.listdir(adir)) if adir.is_dir() else 'no dir'}")
        src = hello.read_text(encoding="utf-8")
        [node] = find_wasm_defs(src, "hello.py")
        info = verify(adir, function="hello", expected_source_sha256=sha256_text(kernel_source(src, node)))
        if info.manifest["compiler"]["version"] != COMPILER_VERSION:
            raise AssertionError(f"artifact compiler version {info.manifest['compiler']['version']} != pin {COMPILER_VERSION}")
        steps["A2"] = "pass"
        out["artifact_wasm_sha256"] = sha256_file(adir / "hello.wasm")
    except Exception as e:  # noqa: BLE001 -- every failure is a recorded verdict
        steps["A2"] = "fail"
        problems.append(f"A2: {e}")
    # ---- A3: run the built artifact under wasmtime (in-process), bit-for-bit
    try:
        os.environ.pop("PYTHSCRIBE_NO_JIT", None)
        os.environ.pop("PYTHSCRIBE_MODE", None)
        mod = _import_file(hello, "acceptance_a3")
        b = binding_of(mod.hello)
        if b.artifact_status != "resolved":
            raise AssertionError(f"beside-source artifact not resolved: {b.artifact_status}")
        py0, sv0 = b.counts()
        r = mod.hello(1.0)
        py1, sv1 = b.counts()
        if float_bits(r) != float_bits(3.0):
            raise AssertionError(f"hello(1.0) bits {float_bits(r)} != {float_bits(3.0)}")
        if b.mode != "server" or (py1, sv1) != (py0, sv0 + 1):
            raise AssertionError(f"mode={b.mode} counts {(py0, sv0)} -> {(py1, sv1)}: not the wasmtime server path")
        steps["A3"] = "pass"
        out["a3_bits"] = float_bits(r)
    except Exception as e:  # noqa: BLE001
        steps["A3"] = "fail"
        problems.append(f"A3: {e}")
    # ---- A4: first-call witness on post-call counters (validation §G), cold cache, artifact removed
    try:
        shutil.rmtree(cwd / "__pythscribe__", ignore_errors=True)
        cache = Path(tempfile.mkdtemp(prefix="acc-jit-"))
        os.environ["PYTHSCRIBE_CACHE"] = str(cache)
        os.environ.pop("PYTHSCRIBE_NO_JIT", None)
        os.environ.pop("PYTHSCRIBE_MODE", None)
        if _keyed_dirs(cache, COMPILER_VERSION):
            raise AssertionError("JIT cache not cold")
        mod = _import_file(hello, "acceptance_a4")
        b = binding_of(mod.hello)
        if b.mode != "fallback" or b.artifact_status != "absent":
            raise AssertionError(f"something compiled at import: mode={b.mode} status={b.artifact_status}")
        py0, sv0 = b.counts()
        r = mod.hello(1.0)
        py1, sv1 = b.counts()
        if not (sv1 == sv0 + 1 and py1 == py0):
            raise AssertionError(f"counters {(py0, sv0)} -> {(py1, sv1)}: the Python body ran / server did not (jit_reason={getattr(b, '_jit_reason', None)!r})")
        if b.mode != "server" or b.artifact_status != "resolved":
            raise AssertionError(f"mode={b.mode} status={b.artifact_status}")
        keyed = _keyed_dirs(cache, COMPILER_VERSION)
        if len(keyed) != 1:
            raise AssertionError(f"expected exactly one newly built keyed dir, found {keyed}")
        if float_bits(r) != float_bits(3.0):
            raise AssertionError(f"first-call bits {float_bits(r)}")
        if (cwd / "__pythscribe__").exists():
            raise AssertionError("the JIT wrote beside the source")
        node_spawns = [n for n in spawned if n.lower() in ("node", "node.exe", "npm", "npm.cmd", "npx", "npx.cmd")]
        if node_spawns:
            raise AssertionError(f"node reached: {node_spawns}")
        if not any(n.startswith("pyths") for n in spawned):
            raise AssertionError(f"the compiler did not run: {spawned}")
        steps["A4"] = "pass"
        out["a4_counts"] = {"python": [py0, py1], "server": [sv0, sv1]}
    except Exception as e:  # noqa: BLE001
        steps["A4"] = "fail"
        problems.append(f"A4: {e}")
    finally:
        subprocess.run, subprocess.Popen = real_run, real_popen  # type: ignore[assignment]
    out["spawned"] = spawned
    return out


# ----------------------------------------------------------------------------- diagnostic sampler


def sampler(out: Path, interval: float = 0.1) -> None:
    """Append every process whose comm is node to `out` (Linux /proc; diagnostic ONLY -- a polling
    sampler cannot certify absence; R-BA's node_free comes from A0/A4b/A5b)."""
    seen = 0
    while True:
        for pid in os.listdir("/proc"):
            if not pid.isdigit():
                continue
            try:
                comm = Path(f"/proc/{pid}/comm").read_text().strip()
            except OSError:
                continue
            if is_node_name(comm):
                seen += 1
                with open(out, "a", encoding="utf-8") as f:
                    f.write(f"{time.time():.3f} pid={pid} comm={comm}\n")
        time.sleep(interval)


# ----------------------------------------------------------------------------- CLI


def _write(out: Path | None, payload: dict) -> None:
    text = json.dumps(payload, indent=2, sort_keys=True)
    if out:
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(text + "\n", encoding="utf-8")
    print(text)


def _a0_like(step: str, ns) -> int:
    roots = [Path(r) for r in (ns.root or (["/"] if os.name != "nt" else [f"{d}:\\" for d in "CDEF" if Path(f"{d}:\\").exists()]))]
    found = find_node_files(roots, skip=[Path("/proc"), Path("/sys"), Path("/dev")])
    probe = _probe_via_command(ns.probe) if ns.probe else None
    problems = a0_verdict(found, [Path(a) for a in (ns.allow_under or [])], probe, which_on_path())
    payload = {"step": step, "found": [str(f) for f in found], "path_hits": which_on_path(), "problems": problems,
               "verdict": "fail" if problems else "pass"}
    if step == "A5b" and ns.sampler_log:
        log = Path(ns.sampler_log)
        payload["sampler_hits"] = log.read_text(encoding="utf-8").splitlines() if log.is_file() else []
    _write(Path(ns.out) if ns.out else None, payload)
    return 1 if problems else 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="wheel_acceptance.py", description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("a0", "post-a5"):
        s = sub.add_parser(name)
        s.add_argument("--allow-under", nargs="*", default=[], help="native legs: roots a node file may live under (~ctl, externals/)")
        s.add_argument("--probe", default=None, help="native legs: `<cmd>` run as `<cmd> <file>`; exit 0 == denied for `app`")
        s.add_argument("--root", nargs="*", default=None, help="filesystem roots to walk (default: / or every drive)")
        s.add_argument("--sampler-log", default=None)
        s.add_argument("--out", default=None)
    s = sub.add_parser("bind")
    s.add_argument("--wheel", required=True)
    s.add_argument("--manifest", required=True)
    s.add_argument("--target", required=True)
    s.add_argument("--wheelhouse", default=None)
    s.add_argument("--out", default=None)
    s = sub.add_parser("run")
    s.add_argument("--manifest", required=True)
    s.add_argument("--target", required=True)
    s.add_argument("--checkout", default=None)
    s.add_argument("--hello", default="hello.py")
    s.add_argument("--out", default=None)
    s = sub.add_parser("sampler")
    s.add_argument("--out", required=True)
    ns = ap.parse_args(argv)

    if ns.cmd == "a0":
        return _a0_like("A0", ns)
    if ns.cmd == "post-a5":
        return _a0_like("A5b", ns)
    if ns.cmd == "sampler":
        sampler(Path(ns.out))
        return 0
    manifest = json.loads(Path(ns.manifest).read_text(encoding="utf-8"))
    if ns.cmd == "bind":
        w = Path(ns.wheel).resolve()
        problems = check_candidate(w, manifest, ns.target)
        if not problems and ns.wheelhouse:
            problems += check_wheelhouse(Path(ns.wheelhouse), w)
        _write(Path(ns.out) if ns.out else None, {"step": "A0b", "wheel": str(w), "wheel_sha256": sha256_file(w) if w.is_file() else None,
                                                  "problems": problems, "verdict": "fail" if problems else "pass"})
        return 1 if problems else 0
    hello = Path(ns.hello).resolve()
    checkout = Path(ns.checkout).resolve() if ns.checkout else None
    if checkout is not None and _under(hello.parent, checkout):
        _write(Path(ns.out) if ns.out else None, {"problems": [f"A1: the acceptance cwd {hello.parent} is inside the checkout {checkout}"], "steps": {"A1": "fail"}})
        return 1
    res = run_a2_a4(hello, manifest, ns.target, checkout=checkout)
    res["target"] = ns.target
    res["wheel_sha256"] = manifest["targets"][ns.target]["wheel_sha256"]
    _write(Path(ns.out) if ns.out else None, res)
    return 1 if res["problems"] else 0


if __name__ == "__main__":
    sys.exit(main())
