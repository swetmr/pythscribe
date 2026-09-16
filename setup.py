"""The platform-wheel tag hook (spec 13-09-26, M1.3) -- a `cmdclass` extension of setuptools.build_meta.

All metadata lives in pyproject.toml; this file exists ONLY to hook two setuptools commands:

  * `bdist_wheel` -- the pip package is pure Python PLUS one native data file, the finalized
    compiler binary staged at `pythscribe/_bin/pyths[.exe]` by the cibuildwheel leg. A pure
    `py3-none-any` tag would be a LIE for such a wheel (pip would install the Linux binary on
    macOS), so when the binary is staged the wheel is tagged `py3-none-<platform>` (`plat_name`
    handed to bdist_wheel exactly as `--plat-name` would be; `Root-Is-Purelib` stays true). The
    tag is `PYTHSCRIBE_WHEEL_PLATFORM` (set per leg by `CIBW_ENVIRONMENT`) or, when unset, the
    HOST's tag. A cheap format+architecture consistency check between the staged binary and the
    tag runs here; the REAL floor (glibc / minos / dylibs) is enforced by the repair hook,
    `scripts/wheel_passthrough.py`, before cibuildwheel accepts the wheel.
    Binary ABSENT -> the SOURCE-INSTALL policy (requirements §6): the wheel builds and installs
    BINARY-LESS, tagged `py3-none-any`, with a VISIBLE notice on stderr (`pip install -v` /
    `python -m build` show it); `find_pyths()` / `pyths doctor` name the gap at use time. This is
    the path `pip install <sdist>` / `pythscribe @ git+...` takes on any platform -- prebuilt
    `__pythscribe__/` artifacts and the Python fallback keep working (the committed-artifacts
    Space case), so refusing outright was rejected by the spec.
  * `sdist` -- never carries the binary (MANIFEST.in prunes `pythscribe/_bin`); the hook VERIFIES
    the produced tarball and refuses one that does (the paired control for the prune line:
    removing it is RED here, not silently a 7 MB "source" tarball).

Why a cmdclass and not a `backend-path` in-tree backend (measured, 2026-09-15): an in-tree backend
puts the repo root on `sys.path` BEFORE `setuptools` is first imported, so setuptools resolved its
own `__version__` from the repo's `pythscribe.egg-info` ("0.2.5a0"), its vendored bdist_wheel
took the `setuptools_major_version < 57` legacy branch and shipped the ROOT `LICENSE.md` (FSL) as
the wheel's licence instead of `pythscribe/LICENSE` (MIT) -- a silent licence corruption. The
cmdclass runs after setuptools is imported and touches no path.
"""
from __future__ import annotations

import os
import sys
import tarfile
from pathlib import Path

from setuptools import setup
from setuptools.command.bdist_wheel import bdist_wheel as _bdist_wheel
from setuptools.command.sdist import sdist as _sdist

ROOT = Path(__file__).resolve().parent
BIN_DIR = ROOT / "pythscribe" / "_bin"
PLATFORM_ENV = "PYTHSCRIBE_WHEEL_PLATFORM"
NOTICE = (
    "pythscribe: NOTICE -- no prebuilt compiler is staged at pythscribe/_bin/ (source install, or no "
    "wheel for this platform). Building a BINARY-LESS wheel: `pyths build` and compile-on-first-call "
    "are unavailable in this install; prebuilt `__pythscribe__/` artifacts and the Python fallback "
    "work. Prebuilt wheels: Linux x86_64/aarch64 (glibc 2.28+), macOS x86_64/arm64, Windows x64."
)


def staged_binary() -> Path | None:
    for name in ("pyths.exe", "pyths"):
        p = BIN_DIR / name
        if p.is_file():
            return p
    return None


def platform_tag_for(binary: Path) -> str:
    """The wheel tag for the staged binary, from the ONE authority (pythscribe/build/_native.py).
    The repo root is put on sys.path only here, transiently, AFTER setuptools is fully imported."""
    sys.path.insert(0, str(ROOT))
    try:
        from pythscribe.build._native import (
            EXPECTED_WHEEL_SET, NativeFormatError, check_format_and_arch, host_wheel_tag, inspect_binary,
        )
    finally:
        sys.path.remove(str(ROOT))
    tag = os.environ.get(PLATFORM_ENV) or host_wheel_tag()
    if not tag:
        raise SystemExit(f"setup.py: a compiler binary is staged at {binary} but this host is not a release target "
                         f"and {PLATFORM_ENV} is unset; set it to one of {sorted(EXPECTED_WHEEL_SET)}")
    if tag not in EXPECTED_WHEEL_SET:
        raise SystemExit(f"setup.py: {PLATFORM_ENV}={tag!r} is not in the expected wheel set {sorted(EXPECTED_WHEEL_SET)}")
    try:
        info = inspect_binary(binary.read_bytes())
    except NativeFormatError as e:
        raise SystemExit(f"setup.py: {binary} is not a native executable ({e})") from None
    problems = check_format_and_arch(info, tag)
    if problems:
        raise SystemExit("setup.py: the staged binary does not match the wheel tag:\n  " + "\n  ".join(problems))
    return tag


class bdist_wheel(_bdist_wheel):
    def finalize_options(self) -> None:
        binary = staged_binary()
        self._pythscribe_tag = None
        if binary is None:
            print(NOTICE, file=sys.stderr, flush=True)
        elif self.plat_name is None:
            self._pythscribe_tag = platform_tag_for(binary)
            self.plat_name = self._pythscribe_tag  # bdist_wheel then sets plat_name_supplied = True
        super().finalize_options()

    def run(self) -> None:
        super().run()
        wheels = [f for (cmd, _py, f) in self.distribution.dist_files if cmd == "bdist_wheel"]
        want = f"-py3-none-{self._pythscribe_tag or 'any'}.whl"
        bad = [w for w in wheels if not w.endswith(want)]
        if bad:
            raise SystemExit(f"setup.py: the wheel was not tagged {want}: {bad}")


def verify_sdist_tarball(path: str) -> None:
    """The sdist policy, checked on the PRODUCED tarball: no compiler binary inside, the tag hook
    (this file) present. Raises SystemExit with the reason."""
    with tarfile.open(path) as tf:
        members = tf.getnames()
    leaked = [m for m in members if "/pythscribe/_bin/" in m or m.endswith("/pythscribe/_bin")]
    if leaked:
        raise SystemExit(f"setup.py: the sdist must never carry the compiler binary (MANIFEST.in `prune pythscribe/_bin` missing?): {leaked}")
    if not any(m.endswith("/setup.py") for m in members):
        raise SystemExit("setup.py: the sdist lacks setup.py (the tag hook); a source install could not build")


class sdist(_sdist):
    def run(self) -> None:
        super().run()
        for path in self.archive_files:
            if path.endswith(".tar.gz"):
                verify_sdist_tarball(path)


if __name__ == "__main__":
    setup(cmdclass={"bdist_wheel": bdist_wheel, "sdist": sdist})
