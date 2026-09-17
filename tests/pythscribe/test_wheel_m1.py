"""M1 gates (spec 13-09-26-lib-selfcontained-pip-wheel; validation §B + §I + the M1 exit) -- every gate
ships its PAIRED NEGATIVE CONTROL (the anti-vacuity paired-control convention).

  B0  launcher forwarding: --version == pin; compile --stdout / --emit-cert (a compile FLAG) / the
      global --quiet forward; --help = native help + the launcher section; `init` is native; the
      M7 commands (new/install/dev/node/npm) fail cleanly without Node (the §3.2 report, exit 1, no
      traceback). Control: `cache status` produces native output (a launcher mutant
      that swallows an unknown subcommand instead of forwarding -> RED).
  B1  bundled resolution: launcher on PATH, no external native pyths -> find_pyths() returns the
      bundled _bin path (absolute, native-validated), never the console script.
      Control: _bin removed, launcher on PATH -> reports the missing compiler, never the launcher,
      never recurses (5 s timeout); ANOTHER venv's launcher on PATH is rejected by the probe
      protocol (it answers `pythscribe-launcher`).
  B2  pin mismatch -> build_kernel raises the pin error.
  B3  graceful T6: node absent -> `pyths run x.ps` prints the message, exit non-zero, no traceback.
      Control: node present -> `pyths run` executes normally.
  B4  source-install policy: `pip install <sdist>` succeeds BINARY-LESS with the visible notice;
      find_pyths() names the gap; `pyths doctor` says not bundled; a committed artifact still
      resolves. Controls: the notice is asserted on the install output (a silent backend -> RED);
      the sdist carries no binary and setup.py refuses one that does.
  I1  the local wheel is `py3-none-<host tag>`; I4/I5 wheel_passthrough: byte-for-byte copy, the
      binary sha == B_t, exactly one wheel in dest_dir. Controls: a rewritten binary ("repair") ->
      hash RED with the content diagnostic; a foreign/none-any tag -> RED; a different wheel copied
      -> the set/hash gates RED; no --binary -> RED (never vacuous).
  I6  floors on SYNTHESIZED binaries (host-independent): GLIBC_2.31 in manylinux_2_28 -> RED;
      aarch64 in an x86_64 tag -> RED; a non-allowlisted DT_NEEDED -> RED; minos 12.0 in
      macosx_11_0 -> RED; arm64 in x86_64 -> RED; a Homebrew dylib -> RED; a fat binary -> RED;
      a PE GUI/ARM64 image in win_amd64 -> RED; conforming fixtures -> GREEN; a mutant that skips
      the floor read passes all of them (asserted as the discriminating property).
  E4  verify_wheel_set: 5 wheels + sdist -> GREEN; one missing -> RED; an extra none-any -> RED.
  H2  verify_wheel_clean iterates ALL wheels: the 3rd corrupt -> RED; a top-level `_bin/` -> RED.
  WF  release.yml: every CIBW_REPAIR_WHEEL_COMMAND_* ends in wheel_passthrough.py {wheel} {dest_dir}
      and never runs `auditwheel repair` / `delocate-wheel` (the emit-nothing / real-repair controls
      as a workflow-lint fixture).
Needs a `pyths` built from THIS checkout at target/release (ci.yml builds it; the venv tests build
a wheel with that binary staged at pythscribe/_bin/ -- gitignored). Under PYTHSCRIBE_SKIP_PACKAGING=1
the wheel/venv gates are skipped (same switch as test_packaging.py).
"""
from __future__ import annotations

import hashlib
import io
import os
import re
import shutil
import struct
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path

import pytest

from conftest import REPO, gate
from pythscribe._pin import COMPILER_VERSION
from pythscribe.build import (
    LAUNCHER_PROBE_ENV,
    LAUNCHER_PROBE_REPLY,
    BuildError,
    SOURCE_INSTALL_NOTICE,
    build_kernel,
    find_pyths,
)
import pythscribe.build as build_mod
from pythscribe.build._native import (
    EXPECTED_WHEEL_SET,
    NativeFormatError,
    check_floor,
    host_wheel_tag,
    inspect_binary,
    sniff_format,
)

SCRIPTS = REPO / "scripts"
sys.path.insert(0, str(SCRIPTS))
import verify_wheel_clean as clean  # noqa: E402
import verify_wheel_set as wset  # noqa: E402
import wheel_passthrough as pt  # noqa: E402

VERSION = wset.pyproject_version()
BIN_NAME = "pyths.exe" if os.name == "nt" else "pyths"
DEV_BINARY = REPO / "target" / "release" / BIN_NAME
STAGED = REPO / "pythscribe" / "_bin" / BIN_NAME
SKIP_PACKAGING = os.environ.get("PYTHSCRIBE_SKIP_PACKAGING") == "1"
SYSTEM_PATH = "C:\\Windows\\System32" if os.name == "nt" else ""


def _sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def _run(args, timeout=120, **kw) -> subprocess.CompletedProcess:
    return subprocess.run([str(a) for a in args], capture_output=True, text=True, check=False, timeout=timeout, **kw)


def _venv_paths(venv: Path) -> tuple[Path, Path, Path]:
    """(python, scripts dir, site-packages)"""
    if os.name == "nt":
        py, scripts = venv / "Scripts" / "python.exe", venv / "Scripts"
    else:
        py, scripts = venv / "bin" / "python", venv / "bin"
    r = _run([py, "-c", "import sysconfig; print(sysconfig.get_path('purelib'))"])
    return py, scripts, Path(r.stdout.strip())


def _make_venv(root: Path, name: str) -> tuple[Path, Path, Path]:
    venv = root / name
    r = _run([sys.executable, "-m", "venv", str(venv)])
    assert r.returncode == 0, r.stderr
    return _venv_paths(venv)


# ----------------------------------------------------------------------------- fixtures: the real wheel + venvs


@pytest.fixture(scope="module")
def dev_binary() -> Path:
    gate(DEV_BINARY.is_file(), f"{DEV_BINARY} missing: `cargo build --release --bin pyths` first")
    r = _run([DEV_BINARY, "--version"])
    assert r.stdout.strip() == f"pyths {COMPILER_VERSION}", r.stdout
    return DEV_BINARY


@pytest.fixture(scope="module")
def staged(dev_binary: Path) -> Path:
    """The dev binary staged at pythscribe/_bin/ (what the cibuildwheel leg does from B_t)."""
    STAGED.parent.mkdir(exist_ok=True)
    if not STAGED.is_file() or STAGED.read_bytes() != dev_binary.read_bytes():
        shutil.copy2(dev_binary, STAGED)
    return STAGED


@pytest.fixture(scope="module")
def dist(tmp_path_factory, staged: Path) -> dict[str, Path]:
    """wheel (with the staged binary, host-tagged) + sdist built from this checkout."""
    if SKIP_PACKAGING:
        pytest.skip("packaging gate skipped by env")
    tag = host_wheel_tag()
    gate(tag is not None, "this host is not a release wheel target")
    out = tmp_path_factory.mktemp("dist")
    env = {**os.environ, "PYTHSCRIBE_WHEEL_PLATFORM": tag}
    for what in ("--wheel", "--sdist"):
        p = _run([sys.executable, "-m", "build", what, "--outdir", str(out), str(REPO)], timeout=600, cwd=str(REPO), env=env)
        gate(p.returncode == 0, f"python -m build {what} failed: {p.stderr[-2000:]}")
    [whl] = out.glob("*.whl")
    [sdist] = out.glob("*.tar.gz")
    assert whl.name == f"pythscribe-{VERSION}-py3-none-{tag}.whl", whl.name  # I1
    return {"wheel": whl, "sdist": sdist, "tag": tag, "dir": out}


@pytest.fixture(scope="module")
def wheel_venv(tmp_path_factory, dist) -> dict[str, Path]:
    """Venv A: the wheel installed (no deps, no index)."""
    py, scripts, site = _make_venv(tmp_path_factory.mktemp("venvA"), "venv")
    p = _run([py, "-m", "pip", "install", "--no-index", "--no-deps", str(dist["wheel"])], timeout=300)
    assert p.returncode == 0, p.stderr[-2000:]
    pyths = shutil.which("pyths", path=str(scripts))
    assert pyths, list(scripts.iterdir())
    return {"python": py, "scripts": scripts, "site": site, "pyths": Path(pyths), "bin": site / "pythscribe" / "_bin" / BIN_NAME}


@pytest.fixture(scope="module")
def sdist_venv(tmp_path_factory, dist) -> dict:
    """Venv C: the SDIST installed from a cwd OUTSIDE the checkout (binary-less source install)."""
    root = tmp_path_factory.mktemp("venvC")
    py, scripts, site = _make_venv(root, "venv")
    p = _run([py, "-m", "pip", "install", "-v", "--no-deps", str(dist["sdist"])], timeout=600, cwd=str(root))
    assert p.returncode == 0, p.stderr[-3000:]
    return {"python": py, "scripts": scripts, "site": site, "pyths": Path(shutil.which("pyths", path=str(scripts)) or ""), "install_output": p.stdout + p.stderr, "root": root}


def _env_with_path(*dirs: Path | str, base: dict | None = None) -> dict:
    env = dict(base or os.environ)
    env["PATH"] = os.pathsep.join(str(d) for d in dirs if str(d))
    env.pop("PYTHSCRIBE_PYTHS", None)
    return env


# ----------------------------------------------------------------------------- the M1 exit (shipped-path SPOT)


def test_exit_spots_in_the_installed_venv(wheel_venv, tmp_path):
    """The SAME script cibuildwheel runs as CIBW_TEST_COMMAND on every leg (scripts/wheel_m1_spots.py):
    origins under site-packages, find_pyths() == the bundled _bin, --version == pin, `pyths build
    hello.py` -> a self-verifying artifact, compile --stdout / --quiet / --help / init forward."""
    p = _run([wheel_venv["python"], SCRIPTS / "wheel_m1_spots.py", REPO], timeout=600, cwd=str(tmp_path),
             env=_env_with_path(wheel_venv["scripts"], SYSTEM_PATH))
    assert p.returncode == 0, (p.stdout[-2000:], p.stderr[-3000:])
    assert '"spots": "GREEN"' in p.stdout.strip().splitlines()[-1], p.stdout
    assert str(wheel_venv["bin"]).lower() in p.stdout.lower().replace("\\\\", "\\"), (p.stdout, wheel_venv["bin"])


# ----------------------------------------------------------------------------- B0 forwarding


def test_b0_version_and_flag_forwarding(wheel_venv, tmp_path):
    env = _env_with_path(wheel_venv["scripts"], SYSTEM_PATH)
    pyths = wheel_venv["pyths"]
    assert _run([pyths, "--version"], env=env).stdout.strip() == f"pyths {COMPILER_VERSION}"
    # --emit-cert is a compile FLAG: it must reach the native binary (the cert file is written)
    src = tmp_path / "x.ps"
    src.write_text("def f(x):\n    return x + 1\n\nprint(f(1))\n", encoding="utf-8")
    p = _run([pyths, "compile", src, "-o", tmp_path / "x.js", "--emit-cert", "--no-dts"], env=env, cwd=str(tmp_path))
    assert p.returncode == 0, (p.stdout, p.stderr)
    assert (tmp_path / "x.js").is_file() and (tmp_path / "x.js.cert.json").is_file(), sorted(os.listdir(tmp_path))
    q = _run([pyths, "--quiet", "compile", src, "--stdout"], env=env, cwd=str(tmp_path))
    assert q.returncode == 0 and "f" in q.stdout and q.stderr.strip() == "", (q.stdout[:200], q.stderr)
    v = _run([pyths, "--verbose", "compile", src, "--stdout"], env=env, cwd=str(tmp_path))
    assert v.returncode == 0 and v.stdout == q.stdout


def test_b0_help_is_native_plus_launcher_section_and_init_is_native(wheel_venv):
    env = _env_with_path(wheel_venv["scripts"], SYSTEM_PATH)
    pyths = wheel_venv["pyths"]
    h = _run([pyths, "--help"], env=env)
    assert h.returncode == 0
    assert "compile" in h.stdout and "cache" in h.stdout  # native help
    assert h.stdout.index("Usage") < h.stdout.index("pythscribe launcher commands") and "build <module.py>" in h.stdout and "doctor" in h.stdout
    n = _run([pyths], env=env)  # no args: native usage (exit 2) + the launcher section on stderr
    assert n.returncode != 0 and "pythscribe launcher commands" in n.stderr
    i = _run([pyths, "init", "--help"], env=env)
    assert i.returncode == 0 and "Initialize a new PythScribe project" in i.stdout  # NOT shadowed by `new`
    b = _run([pyths, "build", "--help"], env=env)
    assert b.returncode == 0 and b.stdout.startswith("usage: pyths build")
    # M7 SHIPPED: every frontend command routes through `_web.find_node()` FIRST; with no Node resolvable (this
    # venv's PATH is scripts + System32, no PYTHS_NODE, no vendored `[web-bundled]` -- the wheel was installed
    # --no-deps) the launcher prints the state-tailored §3.2 report at its boundary -- exit 1, never a traceback.
    env.pop("PYTHS_NODE", None)
    for m7 in ("new", "install", "dev", "node", "npm"):
        r = _run([pyths, m7, "anything"], env=env)
        assert r.returncode == 1 and "need Node.js" in r.stderr and "no Node.js was found on PATH" in r.stderr, (m7, r.returncode, r.stderr)
        assert "pyths doctor" in r.stderr and "Traceback" not in r.stderr, (m7, r.stderr)
        assert "implemented in M7" not in r.stderr, (m7, r.stderr)  # the pre-M7 placeholder is gone


def test_b0_control_unknown_subcommand_is_forwarded_not_swallowed(wheel_venv):
    """`cache status` is NOT a launcher command; native output proves forwarding (a mutant that
    swallows unknown subcommands prints nothing native -> RED)."""
    env = _env_with_path(wheel_venv["scripts"], SYSTEM_PATH)
    r = _run([wheel_venv["pyths"], "cache", "status"], env=env)
    assert r.returncode == 0 and r.stdout.strip(), (r.returncode, r.stdout, r.stderr)
    bogus = _run([wheel_venv["pyths"], "frobnicate"], env=env)
    assert bogus.returncode != 0 and "frobnicate" in bogus.stderr  # clap's own error, i.e. it REACHED the binary


def test_launcher_probe_protocol_answers_without_forwarding():
    """find_pyths()'s probe: a launcher run with PYTHSCRIBE_LAUNCHER_PROBE=1 prints the reply and exits
    3 -- it never resolves a compiler, so the probe can never recurse."""
    r = _run([sys.executable, "-m", "pythscribe._launcher", "--version"], env={**os.environ, LAUNCHER_PROBE_ENV: "1"}, timeout=20, cwd=str(REPO))
    assert r.returncode == 3 and r.stdout.strip() == LAUNCHER_PROBE_REPLY, (r.returncode, r.stdout, r.stderr)


# ----------------------------------------------------------------------------- B1 bundled resolution + the recursion control


def test_b1_find_pyths_returns_the_bundled_binary_in_the_venv(wheel_venv, tmp_path):
    code = "from pythscribe.build import find_pyths, bundled_pyths; p = find_pyths(); print(p); assert p == bundled_pyths() and p.is_absolute()"
    r = _run([wheel_venv["python"], "-c", code], env=_env_with_path(wheel_venv["scripts"], SYSTEM_PATH), cwd=str(tmp_path), timeout=30)
    assert r.returncode == 0, r.stderr
    got = Path(r.stdout.strip())
    assert got == wheel_venv["bin"].resolve() or os.path.samefile(got, wheel_venv["bin"]), (got, wheel_venv["bin"])
    assert not os.path.samefile(got, wheel_venv["pyths"])


def test_b1_control_bin_removed_launcher_on_path_never_returns_the_launcher(tmp_path_factory, dist, wheel_venv, tmp_path):
    """Venv B: wheel installed, then _bin DELETED; PATH = B's own scripts dir (its launcher) + A's
    scripts dir (ANOTHER venv's launcher, a PE stub on Windows that passes the magic check and
    must be rejected by the PROBE protocol). find_pyths() must report the missing compiler naming
    the rejected candidates, never return a launcher, never recurse (5 s budget each)."""
    py, scripts, site = _make_venv(tmp_path_factory.mktemp("venvB"), "venv")
    p = _run([py, "-m", "pip", "install", "--no-index", "--no-deps", str(dist["wheel"])], timeout=300)
    assert p.returncode == 0, p.stderr[-2000:]
    (site / "pythscribe" / "_bin" / BIN_NAME).unlink()  # a damaged install (the dir stays; a REMOVED dir reads as a source install)
    env = _env_with_path(scripts, wheel_venv["scripts"], SYSTEM_PATH)
    code = "from pythscribe.build import find_pyths, BuildError\ntry:\n    print('RETURNED', find_pyths())\nexcept BuildError as e:\n    print('ERR', e)\n"
    r = _run([py, "-c", code], env=env, cwd=str(tmp_path), timeout=15)
    assert r.returncode == 0, r.stderr
    assert r.stdout.startswith("ERR"), r.stdout
    assert "bundled binary is missing" in r.stdout and "rejected" in r.stdout, r.stdout
    assert "console script (launcher) itself" in r.stdout, r.stdout  # B's own launcher: rejected by identity, never run
    # A's launcher is rejected, never run: on Windows it is a PE stub that passes the magic check and
    # is rejected by the PROBE protocol ("answered the probe"); on POSIX it is a text shebang console
    # script, rejected as "a text shebang wrapper" BEFORE the probe. Either proves it is never returned.
    assert ("answered the probe" in r.stdout) or ("shebang wrapper" in r.stdout), r.stdout
    assert not any(line.startswith("RETURNED") for line in r.stdout.splitlines())
    # the other venv's launcher is on PATH: the probe protocol must have rejected it (Windows: a PE stub)
    other = str(wheel_venv["scripts"]).lower()
    assert other in r.stdout.lower(), (other, r.stdout)
    # and the launcher itself, with its compiler gone, degrades gracefully -- no recursion, no traceback
    pyths_b = shutil.which("pyths", path=str(scripts))
    v = _run([pyths_b, "--version"], env=env, timeout=15)
    assert v.returncode != 0 and "pyths compiler not found" in v.stderr and "Traceback" not in v.stderr, (v.returncode, v.stdout, v.stderr)
    d = _run([pyths_b, "doctor"], env=env, timeout=15)
    assert d.returncode == 0 and d.stdout.startswith("Compiler: not bundled"), d.stdout


def test_b1_unit_launcher_exclusion_and_native_validation(tmp_path, monkeypatch, dev_binary):
    """Unit twins of the venv control on this interpreter: a shebang wrapper / a `.py` / a text file
    named pyths on PATH are rejected without being executed; a copy of the real binary is accepted
    (and returned absolute); the bundled dir wins over PATH."""
    monkeypatch.delenv("PYTHSCRIBE_PYTHS", raising=False)
    monkeypatch.setattr(build_mod, "_REPO_ROOT", tmp_path / "no-such-repo")
    monkeypatch.setattr(build_mod, "_BUNDLED_DIR", tmp_path / "no-bin")
    fake = tmp_path / "fakebin"
    fake.mkdir()
    (fake / BIN_NAME).write_bytes(b"#!/usr/bin/python\nprint('pyths 9.9.9')\n")  # a console-script-shaped wrapper
    monkeypatch.setenv("PATH", os.pathsep.join([str(fake), SYSTEM_PATH]))
    with pytest.raises(BuildError) as ei:
        find_pyths()
    assert "text shebang wrapper" in str(ei.value) or "script wrapper" in str(ei.value), ei.value
    (fake / BIN_NAME).write_bytes(b"not a binary at all\n")
    with pytest.raises(BuildError, match="not a native executable"):
        find_pyths()
    real = tmp_path / "realbin"
    real.mkdir()
    shutil.copy2(dev_binary, real / BIN_NAME)
    monkeypatch.setenv("PATH", os.pathsep.join([str(fake), str(real), SYSTEM_PATH]))
    got = find_pyths()  # the wrapper is skipped, the native binary further down PATH is accepted
    assert got.is_absolute() and got.parent == real.resolve()
    # bundled wins over PATH
    bundled = tmp_path / "bin"
    bundled.mkdir()
    shutil.copy2(dev_binary, bundled / BIN_NAME)
    monkeypatch.setattr(build_mod, "_BUNDLED_DIR", bundled)
    assert find_pyths().parent == bundled.resolve()
    # a bundled file that is not native -> a clear error (never executed)
    (bundled / BIN_NAME).write_bytes(b"#!/bin/sh\n")
    with pytest.raises(BuildError, match="not a native executable"):
        find_pyths()
    # PYTHSCRIBE_PYTHS pointing at the console script -> refused (the recursion class, env arm)
    monkeypatch.setattr(build_mod, "_RUNNING_LAUNCHER_ARGV0", fake / BIN_NAME)
    monkeypatch.setenv("PYTHSCRIBE_PYTHS", str(fake / BIN_NAME))
    with pytest.raises(BuildError, match="launcher"):
        find_pyths()


def test_b1_install_time_arch_self_check_is_a_clear_error(tmp_path, monkeypatch):
    """A wheel for another platform force-installed here: the bundled binary's format/arch != host
    -> a clear BuildError naming both (never a SIGILL / Exec format error)."""
    monkeypatch.delenv("PYTHSCRIBE_PYTHS", raising=False)
    bundled = tmp_path / "bin"
    bundled.mkdir()
    foreign = _macho64(0x0100000C, (11, 0, 0), ["/usr/lib/libSystem.B.dylib"]) if os.name == "nt" else _pe(0x8664, 3)
    (bundled / BIN_NAME).write_bytes(foreign)
    monkeypatch.setattr(build_mod, "_BUNDLED_DIR", bundled)
    with pytest.raises(BuildError, match="wheel for another platform"):
        find_pyths()


# ----------------------------------------------------------------------------- B2 pin


def test_b2_pin_mismatch_raises(tmp_path, monkeypatch, dev_binary):
    monkeypatch.setattr(build_mod, "pyths_version", lambda p: "9.9.9")
    src = tmp_path / "hello.py"
    src.write_text((REPO / "examples" / "hello.py").read_text(encoding="utf-8"), encoding="utf-8")
    with pytest.raises(BuildError, match="pinned to"):
        build_kernel(src, "hello", "def hello(x: float) -> float:\n    return x * 2.0 + 1.0\n", pyths=dev_binary, force=True, quiet=True)


# ----------------------------------------------------------------------------- B3 graceful T6


def test_b3_run_without_node_is_graceful(wheel_venv, tmp_path):
    env = _env_with_path(wheel_venv["scripts"], SYSTEM_PATH)  # no node reachable
    assert shutil.which("node", path=env["PATH"]) is None
    src = tmp_path / "m.ps"
    src.write_text('print("hi")\n', encoding="utf-8")
    for cmd in ("run", "test"):
        r = _run([wheel_venv["pyths"], cmd, src], env=env, cwd=str(tmp_path))
        assert r.returncode == 2 and f"pyths {cmd}" in r.stderr and "Node" in r.stderr and "`pyths build`" in r.stderr, (cmd, r.stderr)
        assert "Traceback" not in r.stderr and "Traceback" not in r.stdout


def test_b3_control_run_with_node_executes(wheel_venv, tmp_path):
    node = shutil.which("node")
    gate(bool(node), "node is required for the B3 positive control")
    src = tmp_path / "m.ps"
    src.write_text('print("hi from node")\n', encoding="utf-8")
    env = _env_with_path(wheel_venv["scripts"], Path(node).parent, SYSTEM_PATH)
    r = _run([wheel_venv["pyths"], "run", src], env=env, cwd=str(tmp_path))
    assert r.returncode == 0 and "hi from node" in r.stdout, (r.returncode, r.stdout, r.stderr)


# ----------------------------------------------------------------------------- B4 source-install policy


def test_b4_sdist_has_no_binary_and_setup_refuses_one(dist, tmp_path):
    members = tarfile.open(dist["sdist"]).getnames()
    assert not any("_bin" in m for m in members), [m for m in members if "_bin" in m]
    assert any(m.endswith("/setup.py") for m in members) and any(m.endswith("/MANIFEST.in") for m in members)
    assert dist["sdist"].stat().st_size < 5_000_000
    # the hook's own control: an sdist tarball carrying _bin is refused (setup.py is importable as a
    # module -- `setup()` runs only under __main__)
    import importlib.util

    spec = importlib.util.spec_from_file_location("pythscribe_setup_hook", REPO / "setup.py")
    hook = importlib.util.module_from_spec(spec)
    assert spec.loader
    spec.loader.exec_module(hook)
    hook.verify_sdist_tarball(str(dist["sdist"]))  # the real one passes

    def _tar(path: Path, names: list[str]) -> Path:
        with tarfile.open(path, "w:gz") as tf:
            for name in names:
                ti = tarfile.TarInfo(name)
                ti.size = 1
                tf.addfile(ti, io.BytesIO(b"x"))
        return path

    with pytest.raises(SystemExit, match="must never carry the compiler binary"):
        hook.verify_sdist_tarball(str(_tar(tmp_path / "leak.tar.gz", ["p-0/setup.py", "p-0/pythscribe/_bin/pyths"])))
    with pytest.raises(SystemExit, match="lacks setup.py"):
        hook.verify_sdist_tarball(str(_tar(tmp_path / "nohook.tar.gz", ["p-0/pyproject.toml"])))


def test_b4_source_install_succeeds_binaryless_with_the_visible_notice(sdist_venv, tmp_path):
    out = sdist_venv["install_output"]
    assert "pythscribe: NOTICE -- no prebuilt compiler" in out, out[-3000:]  # control: a silent backend -> RED
    assert "Successfully installed pythscribe" in out
    site = sdist_venv["site"]
    assert (site / "pythscribe" / "__init__.py").is_file() and not (site / "pythscribe" / "_bin").exists()
    env = _env_with_path(sdist_venv["scripts"], SYSTEM_PATH)
    code = "from pythscribe.build import find_pyths, BuildError\ntry:\n    find_pyths(); print('FOUND')\nexcept BuildError as e:\n    print('ERR', e)\n"
    r = _run([sdist_venv["python"], "-c", code], env=env, cwd=str(tmp_path), timeout=15)
    assert r.returncode == 0 and r.stdout.startswith("ERR") and "no prebuilt compiler for this platform / source install" in r.stdout, (r.stdout, r.stderr)
    assert "`__pythscribe__/` artifacts and the Python fallback still work" in r.stdout
    assert sdist_venv["pyths"].is_file()
    v = _run([sdist_venv["pyths"], "--version"], env=env, cwd=str(tmp_path), timeout=15)
    assert v.returncode != 0 and "no prebuilt compiler" in v.stderr and "Traceback" not in v.stderr, (v.returncode, v.stderr)
    d = _run([sdist_venv["pyths"], "doctor"], env=env, cwd=str(tmp_path), timeout=15)
    # M7.2b doctor line: `<label>: <state> -- <detail>; fix: <fix>` (one authority: `_probe_compiler`)
    assert d.returncode == 0 and d.stdout.startswith("Compiler: not bundled -- source install / no wheel for this platform; fix:") and "Traceback" not in d.stderr, (d.stdout, d.stderr)
    # a committed artifact still resolves without a compiler (resolve() needs none)
    uc = REPO / "examples" / "wasm-use-cases"
    gate((uc / "__pythscribe__" / "edit_distance" / "manifest.json").is_file(), "use-case artifacts not built")
    probe = tmp_path / "probe"
    probe.mkdir()
    shutil.copyfile(uc / "kernels.py", probe / "kernels.py")
    shutil.copytree(uc / "__pythscribe__", probe / "__pythscribe__")
    code = "import sys; sys.path.insert(0, sys.argv[1]); import kernels; from pythscribe import binding_of; b = binding_of(kernels.edit_distance); print(b.artifact_status, b.mode)"
    r = _run([sdist_venv["python"], "-c", code, str(probe)], env={**env, "PYTHSCRIBE_NO_JIT": "1"}, cwd=str(tmp_path), timeout=30)
    assert r.returncode == 0 and r.stdout.split()[0] == "resolved", (r.stdout, r.stderr)


# ----------------------------------------------------------------------------- synthesized native fixtures (host-independent)


def _elf64(machine: int, needed: list[str], versions: list[tuple[str, list[str]]]) -> bytes:
    """A minimal ELF64 LE executable image: one PT_LOAD (vaddr == offset), one PT_DYNAMIC with
    DT_STRTAB / DT_NEEDED / DT_VERNEED / DT_VERNEEDNUM. `versions` = [(libname, [ver, ...])]."""
    strtab = bytearray(b"\x00")
    offs: dict[str, int] = {}

    def s(name: str) -> int:
        if name not in offs:
            offs[name] = len(strtab)
            strtab.extend(name.encode() + b"\x00")
        return offs[name]

    for n in needed:
        s(n)
    for lib, vers in versions:
        s(lib)
        for v in vers:
            s(v)
    ehdr_size, phdr_size = 64, 56
    strtab_off = ehdr_size + 2 * phdr_size
    verneed_off = (strtab_off + len(strtab) + 3) & ~3
    verneed = bytearray()
    for i, (lib, vers) in enumerate(versions):
        last = i == len(versions) - 1
        entry_size = 16 + 16 * len(vers)
        verneed += struct.pack("<HHIII", 1, len(vers), s(lib), 16, 0 if last else entry_size)
        for j, v in enumerate(vers):
            verneed += struct.pack("<IHHII", 0, 0, 2 + j, s(v), 0 if j == len(vers) - 1 else 16)
    dyn_off = (verneed_off + len(verneed) + 7) & ~7
    dyn = bytearray()
    dyn += struct.pack("<qQ", 5, strtab_off)
    dyn += struct.pack("<qQ", 10, len(strtab))
    for n in needed:
        dyn += struct.pack("<qQ", 1, s(n))
    if versions:
        dyn += struct.pack("<qQ", 0x6FFFFFFE, verneed_off)
        dyn += struct.pack("<qQ", 0x6FFFFFFF, len(versions))
    dyn += struct.pack("<qQ", 0, 0)
    total = dyn_off + len(dyn)
    ehdr = b"\x7fELF" + bytes([2, 1, 1, 0]) + b"\x00" * 8
    ehdr += struct.pack("<HHIQQQIHHHHHH", 2, machine, 1, 0x1000, ehdr_size, 0, 0, ehdr_size, phdr_size, 2, 64, 0, 0)
    ph_load = struct.pack("<IIQQQQQQ", 1, 5, 0, 0, 0, total, total, 0x1000)
    ph_dyn = struct.pack("<IIQQQQQQ", 2, 6, dyn_off, dyn_off, dyn_off, len(dyn), len(dyn), 8)
    img = bytearray(ehdr + ph_load + ph_dyn)
    assert len(img) == strtab_off
    img += strtab
    img += b"\x00" * (verneed_off - len(img))
    img += verneed
    img += b"\x00" * (dyn_off - len(img))
    img += dyn
    return bytes(img)


def _macho64(cputype: int, minos: tuple[int, int, int] | None, dylibs: list[str], *, fat: bool = False) -> bytes:
    if fat:
        return b"\xca\xfe\xba\xbe" + b"\x00" * 60
    cmds = bytearray()
    ncmds = 0
    if minos is not None:
        v = (minos[0] << 16) | (minos[1] << 8) | minos[2]
        cmds += struct.pack("<IIIIII", 0x32, 24, 1, v, v, 0)  # LC_BUILD_VERSION, PLATFORM_MACOS
        ncmds += 1
    for d in dylibs:
        name = d.encode() + b"\x00"
        pad = (-(24 + len(name))) % 8
        cmds += struct.pack("<IIIIII", 0x0C, 24 + len(name) + pad, 24, 0, 0, 0) + name + b"\x00" * pad
        ncmds += 1
    hdr = struct.pack("<IiiIIIII", 0xFEEDFACF, cputype, 0, 2, ncmds, len(cmds), 0, 0)
    return hdr + bytes(cmds)


def _pe(machine: int, subsystem: int, characteristics: int = 0x0022) -> bytes:
    img = bytearray(b"MZ" + b"\x00" * 0x3A + struct.pack("<I", 0x40))
    coff = struct.pack("<HHIIIHH", machine, 0, 0, 0, 0, 240, characteristics)
    opt = bytearray(struct.pack("<H", 0x20B)) + b"\x00" * 238
    struct.pack_into("<H", opt, 68, subsystem)
    img += b"PE\x00\x00" + coff + opt
    return bytes(img)


LINUX_OK = dict(machine=0x3E, needed=["libc.so.6", "libm.so.6", "libgcc_s.so.1"], versions=[("libc.so.6", ["GLIBC_2.2.5", "GLIBC_2.28", "GLIBC_2.17"]), ("libgcc_s.so.1", ["GCC_3.0"])])


def test_i6_elf_parser_and_floors():
    ok = inspect_binary(_elf64(**LINUX_OK))
    assert ok.fmt == "elf" and ok.arch == "x86_64" and ok.elf_glibc_max == (2, 28) and set(ok.elf_needed) == {"libc.so.6", "libm.so.6", "libgcc_s.so.1"}
    assert "GCC_3.0" in ok.elf_version_needs
    assert check_floor(ok, "manylinux_2_28_x86_64") == []
    # GLIBC_2.31 in a 2_28 tag -> RED naming both sides (the falsely-permissive-tag scenario)
    hi = inspect_binary(_elf64(0x3E, ["libc.so.6"], [("libc.so.6", ["GLIBC_2.28", "GLIBC_2.31"])]))
    [p] = check_floor(hi, "manylinux_2_28_x86_64")
    assert "GLIBC_2.31" in p and "2.28" in p and "falsely-permissive" in p, p
    # aarch64 binary in the x86_64 tag -> RED; and it is GREEN under its own tag
    arm = inspect_binary(_elf64(0xB7, LINUX_OK["needed"], LINUX_OK["versions"]))
    assert any("architecture mismatch" in p for p in check_floor(arm, "manylinux_2_28_x86_64"))
    assert check_floor(arm, "manylinux_2_28_aarch64") == []
    # a non-allowlisted DT_NEEDED -> RED
    dep = inspect_binary(_elf64(0x3E, ["libc.so.6", "libssl.so.3"], LINUX_OK["versions"]))
    assert any("libssl.so.3" in p and "allowlist" in p for p in check_floor(dep, "manylinux_2_28_x86_64"))
    # no glibc requirement at all -> the floor cannot be established -> RED (never vacuously green)
    none = inspect_binary(_elf64(0x3E, ["libc.so.6"], []))
    assert any("cannot be established" in p for p in check_floor(none, "manylinux_2_28_x86_64"))
    # wrong format for the tag -> RED
    assert any("format mismatch" in p for p in check_floor(ok, "win_amd64"))


def test_i6_macho_parser_and_floors():
    ok = inspect_binary(_macho64(0x0100000C, (11, 0, 0), ["/usr/lib/libSystem.B.dylib", "/System/Library/Frameworks/CoreFoundation.framework/Versions/A/CoreFoundation"]))
    assert ok.fmt == "macho" and ok.arch == "aarch64" and ok.macho_minos == (11, 0, 0) and len(ok.macho_dylibs) == 2
    assert check_floor(ok, "macosx_11_0_arm64") == []
    twelve = inspect_binary(_macho64(0x0100000C, (12, 0, 0), ["/usr/lib/libSystem.B.dylib"]))
    [p] = check_floor(twelve, "macosx_11_0_arm64")
    assert "minos is 12.0.0" in p and "11.0" in p and "falsely-permissive" in p, p
    assert any("architecture mismatch" in p for p in check_floor(ok, "macosx_11_0_x86_64"))  # arm64 in the x86_64 tag
    x = inspect_binary(_macho64(0x01000007, (10, 15, 0), ["/usr/lib/libSystem.B.dylib"]))
    assert check_floor(x, "macosx_11_0_x86_64") == []  # an OLDER floor than the tag is fine
    brew = inspect_binary(_macho64(0x01000007, (11, 0, 0), ["/usr/lib/libSystem.B.dylib", "/opt/homebrew/lib/libfoo.dylib"]))
    assert any("/opt/homebrew/lib/libfoo.dylib" in p for p in check_floor(brew, "macosx_11_0_x86_64"))
    fat = inspect_binary(_macho64(0, None, [], fat=True))
    assert fat.macho_fat and any("fat" in p for p in check_floor(fat, "macosx_11_0_arm64"))
    nomin = inspect_binary(_macho64(0x0100000C, None, ["/usr/lib/libSystem.B.dylib"]))
    assert any("cannot be established" in p for p in check_floor(nomin, "macosx_11_0_arm64"))


def test_i6_pe_parser_and_floors():
    ok = inspect_binary(_pe(0x8664, 3))
    assert ok.fmt == "pe" and ok.arch == "x86_64" and ok.pe_subsystem == 3 and check_floor(ok, "win_amd64") == []
    assert any("subsystem" in p for p in check_floor(inspect_binary(_pe(0x8664, 2)), "win_amd64"))  # GUI image
    assert any("architecture mismatch" in p for p in check_floor(inspect_binary(_pe(0xAA64, 3)), "win_amd64"))  # ARM64
    assert any("DLL" in p for p in check_floor(inspect_binary(_pe(0x8664, 3, 0x2022)), "win_amd64"))
    for bad in (b"#!/usr/bin/env python\n", b"", b"garbage"):
        with pytest.raises(NativeFormatError):
            inspect_binary(bad)
        assert sniff_format(bad[:4]) is None


def test_i6_control_a_mutant_that_skips_the_floor_read_passes_every_fixture():
    """The discriminating property: format+arch alone (what a tag string + listing check sees)
    accepts all three falsely-permissive fixtures; only the floor read rejects them."""
    from pythscribe.build._native import check_format_and_arch

    fixtures = [
        (inspect_binary(_elf64(0x3E, ["libc.so.6"], [("libc.so.6", ["GLIBC_2.31"])])), "manylinux_2_28_x86_64"),
        (inspect_binary(_macho64(0x01000007, (12, 0, 0), ["/usr/lib/libSystem.B.dylib"])), "macosx_11_0_x86_64"),
        (inspect_binary(_macho64(0x01000007, (11, 0, 0), ["/opt/homebrew/lib/x.dylib"])), "macosx_11_0_x86_64"),
    ]
    for info, tag in fixtures:
        assert check_format_and_arch(info, tag) == []  # the mutant's verdict: GREEN
        assert check_floor(info, tag)  # the gate's verdict: RED


# ----------------------------------------------------------------------------- I4/I5 wheel_passthrough on synthesized + real wheels


def _fixture_wheel(dirpath: Path, tag: str, binary: bytes, *, name: str | None = None, mode: int = 0o755) -> Path:
    w = dirpath / f"pythscribe-{VERSION}-py3-none-{tag}.whl"
    entry = name or ("pythscribe/_bin/pyths.exe" if tag == "win_amd64" else "pythscribe/_bin/pyths")
    with zipfile.ZipFile(w, "w") as z:
        zi = zipfile.ZipInfo(entry)
        zi.external_attr = (0o100000 | mode) << 16
        z.writestr(zi, binary)
        z.writestr(f"pythscribe-{VERSION}.dist-info/WHEEL", f"Tag: py3-none-{tag}\n")
    return w


def test_i5_passthrough_green_on_conforming_fixtures_and_red_on_each_control(tmp_path):
    for tag, binary in (("manylinux_2_28_x86_64", _elf64(**LINUX_OK)), ("macosx_11_0_arm64", _macho64(0x0100000C, (11, 0, 0), ["/usr/lib/libSystem.B.dylib"])), ("win_amd64", _pe(0x8664, 3))):
        d = tmp_path / tag
        d.mkdir()
        bt = d / "B_t"
        bt.write_bytes(binary)
        w = _fixture_wheel(d, tag, binary)
        assert pt.verify(w, bt, tag) == [], (tag, pt.verify(w, bt, tag))
        # the CLI: emits byte-for-byte into dest_dir, exactly one wheel
        r = _run([sys.executable, SCRIPTS / "wheel_passthrough.py", w, d / "out", "--binary", bt, "--tag", tag])
        assert r.returncode == 0 and "GREEN" in r.stdout, (r.stdout, r.stderr)
        [out] = (d / "out").glob("*.whl")
        assert out.read_bytes() == w.read_bytes()
        # controls: a "repair" that rewrote the binary inside the wheel -> hash RED with the diagnostic
        other = d / "other"
        other.write_bytes(binary + b"\x00")
        (d / "rep").mkdir()
        w2 = _fixture_wheel(d / "rep", tag, binary + b"\x00")
        probs = pt.verify(w2, bt, tag)
        assert any("!= sha256(B_t)" in p and ("CONTENT" in p or "content-or-signature" in p) for p in probs), probs
        assert any("expected tag" in p for p in pt.verify(w, bt, "win_amd64" if tag != "win_amd64" else "manylinux_2_28_x86_64"))
        assert any("vacuous" in p for p in pt.verify(w, None, tag))  # no B_t -> refuses (never vacuous)
        # a stray (differently named) wheel already in dest_dir -> the leg emits exactly one -> RED, nothing left behind
        stray = d / "out" / f"pythscribe-0.0.0-py3-none-{tag}.whl"
        stray.write_bytes(b"PK")
        r = _run([sys.executable, SCRIPTS / "wheel_passthrough.py", w, d / "out", "--binary", bt, "--tag", tag])
        assert r.returncode != 0 and "exactly one" in (r.stdout + r.stderr), (r.stdout, r.stderr)
        assert sorted(p.name for p in (d / "out").glob("*.whl")) == [stray.name]
        stray.unlink()
    # tag outside the expected set / none-any -> RED
    w = _fixture_wheel(tmp_path, "any", _pe(0x8664, 3), name="pythscribe/_bin/pyths.exe")
    assert any("not in the expected wheel set" in p for p in pt.verify(w, None, None))
    # exec bit missing on a POSIX tag -> RED
    w = _fixture_wheel(tmp_path, "manylinux_2_28_aarch64", _elf64(0xB7, LINUX_OK["needed"], LINUX_OK["versions"]), mode=0o644)
    bt = tmp_path / "bt_arm"
    bt.write_bytes(_elf64(0xB7, LINUX_OK["needed"], LINUX_OK["versions"]))
    assert any("executable bit" in p for p in pt.verify(w, bt, "manylinux_2_28_aarch64"))
    # wrong binary NAME for the tag (pyths.exe in a Linux wheel) -> RED
    (tmp_path / "n").mkdir()
    w = _fixture_wheel(tmp_path / "n", "manylinux_2_28_x86_64", _elf64(**LINUX_OK), name="pythscribe/_bin/pyths.exe")
    assert any("expected pythscribe/_bin/pyths for tag" in p for p in pt.verify(w, None, None))
    # sha-only identity (the Linux container leg passes B_t's digest through the environment)
    d = tmp_path / "sha"
    d.mkdir()
    w = _fixture_wheel(d, "win_amd64", _pe(0x8664, 3))
    assert pt.verify(w, None, "win_amd64", expected_sha256=_sha(_pe(0x8664, 3))) == []
    [p] = pt.verify(w, None, "win_amd64", expected_sha256="00" * 32)
    assert "!= sha256(B_t)" in p and "CONTENT" in p, p


def test_i5_falsely_permissive_tags_are_red_through_the_passthrough(tmp_path):
    cases = [
        ("manylinux_2_28_x86_64", _elf64(0x3E, ["libc.so.6"], [("libc.so.6", ["GLIBC_2.31"])]), "GLIBC_2.31"),
        ("macosx_11_0_x86_64", _macho64(0x01000007, (12, 0, 0), ["/usr/lib/libSystem.B.dylib"]), "minos is 12.0.0"),
        ("macosx_11_0_x86_64", _macho64(0x0100000C, (11, 0, 0), ["/usr/lib/libSystem.B.dylib"]), "architecture mismatch"),
    ]
    for i, (tag, binary, needle) in enumerate(cases):
        d = tmp_path / str(i)
        d.mkdir()
        bt = d / "B_t"
        bt.write_bytes(binary)
        w = _fixture_wheel(d, tag, binary)
        probs = pt.verify(w, bt, tag)
        assert any(needle in p for p in probs), (tag, probs)
        r = _run([sys.executable, SCRIPTS / "wheel_passthrough.py", w, d / "out", "--binary", bt, "--tag", tag])
        assert r.returncode == 1 and "NOT emitting" in r.stderr and not (d / "out").exists(), (r.stdout, r.stderr)  # emits nothing


def test_i4_passthrough_on_the_real_local_wheel(dist, dev_binary, tmp_path):
    """The built wheel through the real hook with B_t = the dev binary. On a host whose glibc is
    newer than the 2_28 floor (ubuntu-latest builds) the ONLY finding must be the glibc floor --
    the gate is then doing exactly its job; elsewhere GREEN."""
    probs = pt.verify(dist["wheel"], dev_binary, dist["tag"])
    info = inspect_binary(dev_binary.read_bytes())
    if info.fmt == "elf" and info.elf_glibc_max and info.elf_glibc_max > (2, 28):
        assert len(probs) == 1 and "glibc floor too high" in probs[0], probs
        return
    assert probs == [], probs
    r = _run([sys.executable, SCRIPTS / "wheel_passthrough.py", dist["wheel"], tmp_path / "out", "--binary", dev_binary, "--tag", dist["tag"]])
    assert r.returncode == 0 and "GREEN" in r.stdout and _sha(dev_binary.read_bytes()) in r.stdout, (r.stdout, r.stderr)
    [out] = (tmp_path / "out").glob("*.whl")
    assert out.read_bytes() == dist["wheel"].read_bytes()
    # the "real repair" control on the real wheel: rewrite the binary inside -> hash RED with the content diagnostic
    tampered = tmp_path / "t" / dist["wheel"].name
    tampered.parent.mkdir()
    with zipfile.ZipFile(dist["wheel"]) as zin, zipfile.ZipFile(tampered, "w") as zout:
        for item in zin.infolist():
            data = zin.read(item.filename)
            if item.filename in pt.BINARY_NAMES:
                data += b"\x00"
            zout.writestr(item, data)
    probs = pt.verify(tampered, dev_binary, dist["tag"])
    assert any("!= sha256(B_t)" in p for p in probs), probs


# ----------------------------------------------------------------------------- E4 verify_wheel_set / H2 verify_wheel_clean


def _touch_set(d: Path, tags, *, sdist=True, version=VERSION):
    d.mkdir(parents=True, exist_ok=True)
    for t in tags:
        (d / f"pythscribe-{version}-py3-none-{t}.whl").write_bytes(b"PK")
    if sdist:
        (d / f"pythscribe-{version}.tar.gz").write_bytes(b"")


def test_e4_verify_wheel_set_green_with_five_red_with_four_or_extra(tmp_path):
    full = tmp_path / "full"
    _touch_set(full, EXPECTED_WHEEL_SET)
    assert wset.verify(full, version=VERSION) == []
    r = _run([sys.executable, SCRIPTS / "verify_wheel_set.py", "--dist", full])
    assert r.returncode == 0 and "GREEN" in r.stdout and "5 wheels" in r.stdout, (r.stdout, r.stderr)
    four = tmp_path / "four"
    _touch_set(four, EXPECTED_WHEEL_SET - {"macosx_11_0_arm64"})
    probs = wset.verify(four, version=VERSION)
    assert probs == ["MISSING wheel for target macosx_11_0_arm64"], probs
    r = _run([sys.executable, SCRIPTS / "verify_wheel_set.py", "--dist", four])
    assert r.returncode == 1 and "MISSING wheel for target macosx_11_0_arm64" in r.stderr
    extra = tmp_path / "extra"
    _touch_set(extra, EXPECTED_WHEEL_SET | {"any"})
    [p] = wset.verify(extra, version=VERSION)
    assert p.startswith("EXTRA wheel") and "binary-less" in p, p
    nosd = tmp_path / "nosd"
    _touch_set(nosd, EXPECTED_WHEEL_SET, sdist=False)
    assert any("exactly one sdist" in p for p in wset.verify(nosd, version=VERSION))
    assert wset.verify(nosd, version=VERSION, need_sdist=False) == []
    ver = tmp_path / "ver"
    _touch_set(ver, EXPECTED_WHEEL_SET, version="0.0.1")
    assert any("expected " + VERSION in p for p in wset.verify(ver, version=VERSION))
    assert sorted(wset.EXPECTED_WHEEL_SET) == ["macosx_11_0_arm64", "macosx_11_0_x86_64", "manylinux_2_28_aarch64", "manylinux_2_28_x86_64", "win_amd64"]


def _bad_wheel(path: Path, entries: dict[str, bytes]) -> Path:
    with zipfile.ZipFile(path, "w") as z:
        for n, b in entries.items():
            z.writestr(n, b)
    return path


def test_h2_verify_wheel_clean_iterates_all_wheels(dist, tmp_path):
    assert clean.verify_wheel(dist["wheel"]) == []
    good = dist["wheel"].read_bytes()
    d = tmp_path / "dist"
    d.mkdir()
    for t in ("manylinux_2_28_x86_64", "manylinux_2_28_aarch64"):
        (d / f"pythscribe-{VERSION}-py3-none-{t}.whl").write_bytes(good)
    r = _run([sys.executable, SCRIPTS / "verify_wheel_clean.py", "--dist", d])
    assert r.returncode == 0 and "2 wheel(s)" in r.stdout, (r.stdout, r.stderr)
    # the 3rd wheel corrupt (monorepo leakage) -> RED naming it (a whls[0]-only script stays green here)
    base = {n: zipfile.ZipFile(dist["wheel"]).read(n) for n in zipfile.ZipFile(dist["wheel"]).namelist()}
    _bad_wheel(d / f"pythscribe-{VERSION}-py3-none-win_amd64.whl", {**base, "crates/x.rs": b""})
    r = _run([sys.executable, SCRIPTS / "verify_wheel_clean.py", "--dist", d])
    assert r.returncode == 1 and "win_amd64.whl: unexpected top-level entries ['crates']" in r.stderr, r.stderr
    # a top-level _bin/ (outside pythscribe/) -> RED; a wheel without the compiler -> RED; binary-less mode inverts that
    probs = clean.verify_wheel(_bad_wheel(tmp_path / "top.whl", {**base, "_bin/pyths": b""}))
    assert any("`_bin/` is a TOP-LEVEL entry" in p for p in probs), probs
    nobin = _bad_wheel(tmp_path / "nobin.whl", {n: b for n, b in base.items() if n not in clean.BINARY_NAMES})
    assert any("exactly one bundled compiler" in p for p in clean.verify_wheel(nobin))
    assert clean.verify_wheel(nobin, require_binary=False) == []
    assert any("must not carry a compiler" in p for p in clean.verify_wheel(dist["wheel"], require_binary=False))


# ----------------------------------------------------------------------------- WF workflow-lint fixture (I5 emit-nothing / real-repair controls)


def test_wf_release_yml_repair_hooks_are_passthrough_and_never_repair():
    text = (REPO / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8")
    hooks = re.findall(r"CIBW_REPAIR_WHEEL_COMMAND_(LINUX|MACOS|WINDOWS):\s*>-?\s*\n?\s*(.+)", text)
    assert {h[0] for h in hooks} == {"LINUX", "MACOS", "WINDOWS"}, hooks
    for plat, cmd in hooks:
        assert "scripts/wheel_passthrough.py {wheel} {dest_dir}" in cmd, (plat, cmd)  # emits the wheel (a check-only hook -> cibuildwheel fails the leg)
        assert "--binary" in cmd and "--tag" in cmd, (plat, cmd)
        assert "auditwheel repair" not in cmd and "delocate-wheel" not in cmd, (plat, cmd)  # never a real repair
    assert "--require-auditwheel" in dict(hooks)["LINUX"] and "--require-otool" in dict(hooks)["MACOS"]
    for tag in EXPECTED_WHEEL_SET:
        assert tag in text, tag
    assert "scripts/verify_wheel_set.py" in text and "scripts/verify_wheel_clean.py" in text
    assert "CIBW_TEST_COMMAND" in text and "wheel_m1_spots.py" in text
    assert "MACOSX_DEPLOYMENT_TARGET" in text and "11.0" in text and ".2.28" in text  # the by-construction floors
