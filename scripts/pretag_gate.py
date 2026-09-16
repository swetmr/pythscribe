#!/usr/bin/env python3
"""M6.5 -- the pre-tag release gate (spec 13-09-26, plan M6.5; validation §L). Runs on the commit to be tagged,
BEFORE `git tag v<V>`, in addition to the standard pre-tag gate (conformance differential, cross-platform smoke).

    python scripts/pretag_gate.py v<V> [--only NAME ...] [--post-tag] [--skip-wheel-build]

Checks (each a named gate; every RED is printed; exit 1 on any):
  layout      L1  the K7 byte-binding + component-bundle tests (tests/pythscribe/test_artifact_committed.py,
                  test_array_shim_layout.py): RED by design if `LAYOUT_VERSION`/`ARRAY_LAYOUT_VERSION` was bumped
                  without `gradio cc build`, or the shim copies drifted.
  licenses    L2  every `license-files` entry of pyproject exists as a FILE, and (unless --skip-wheel-build) a
                  wheel + sdist built from this checkout pass scripts/verify_wheel_license.py + verify_wheel_clean.py
                  (file presence of the MIT / FSL / runtime / map / third-party notices, per wheel).
  versions    C4  scripts/check_versions.py + scripts/guard_tag_version.py v<V> + `node npm/publish.mjs --check`
                  with GITHUB_REF_NAME=v<V> (stamp idempotence: any package.json byte that stamping would change -> RED).
  mirror-pin  L3  examples/gradio-image-preprocess/requirements.txt pins BOTH git sources to `@v<V>` (a stale or
                  non-existent tag -> RED); with --post-tag (after the PUBLIC tag exists): a clean venv installs the
                  requirements (binary-less source install) and the committed artifacts resolve.
  pypi-name   L4  the manual checklist line: `pythscribe` (+ `pyths`) on PyPI/TestPyPI -- Trusted Publisher registered;
                  the NAME is claimed by the FIRST testpypi/pypi publish (the upload fails loudly if it is foreign-owned).
  smoke       L5  the simple demo + all-features test against the src build (tests/pythscribe/test_all_features_smoke.py,
                  PYTHSCRIBE_REQUIRE_ORACLE=1) -- runs AGAIN as R-TV against the TestPyPI wheel.
  readme      K   scripts/readme_spots.py --static (extras, environment table, the platform list == EXPECTED_WHEEL_SET
                  + the source-install note -- §K-platform runs here, on any runner).
  workflows   WF  scripts/lint_release_workflows.py (job graph, roles, promotion gates, M6 producers).
  payloads    C5  scripts/prepare_release_payloads.py into a scratch dir + verify_runtime_mirror.py (membership,
                  LF, byte identity of the vendored runtime); needs npm on this machine.
"""
from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path
from typing import Callable

REPO = Path(__file__).resolve().parents[1]
SCRIPTS = REPO / "scripts"
SPACE_REQUIREMENTS = REPO / "examples" / "gradio-image-preprocess" / "requirements.txt"
PIN_RE = re.compile(r"^(?P<name>pythscribe|gradio_wasmfunction)\s*@\s*git\+https://github\.com/swetmr/pythscribe\.git@(?P<tag>[^#\s]+)")
L4_CHECKLIST = ("CHECKLIST (manual, release PR): `pythscribe` (+ defensively `pyths`) on PyPI / TestPyPI -- the Trusted Publisher "
                "is registered for publish-pypi.yml; the NAME is claimed by the FIRST `target=testpypi` / `target=pypi` publish "
                "(an unclaimed name is claimed then; a foreign-owned one fails the upload loudly -- never silently).")


# A gate may return RED lines (list[str]) OR this sentinel meaning "not runnable here; enforced later"
# -- neither a false-GREEN nor a false-RED (the operational fix for the local CI gate, codex re-review).
DEFERRED = "__DEFERRED__"


def _run(args: list[str], *, env: dict | None = None, cwd: Path | None = None, timeout: int = 1800) -> subprocess.CompletedProcess:
    return subprocess.run([str(a) for a in args], capture_output=True, text=True, check=False, env=env, cwd=str(cwd or REPO), timeout=timeout)


def _tail(r: subprocess.CompletedProcess, n: int = 1200) -> str:
    return ((r.stdout or "") + (r.stderr or "")).strip()[-n:]


# ----------------------------------------------------------------------------- gates (each returns RED lines)


def gate_layout() -> list[str]:
    r = _run([sys.executable, "-m", "pytest", "tests/pythscribe/test_artifact_committed.py", "tests/pythscribe/test_array_shim_layout.py", "-q", "-p", "no:cacheprovider"],
             env={**os.environ, "PYTHSCRIBE_REQUIRE_ORACLE": "1"})
    return [] if r.returncode == 0 else [f"L1 layout/component-bundle tests RED (forgotten `gradio cc build` or a LAYOUT_VERSION bump without the shim?):\n{_tail(r)}"]


def gate_license_files(pyproject: Path = REPO / "pyproject.toml") -> list[str]:
    proj = tomllib.loads(pyproject.read_text(encoding="utf-8"))["project"]
    p: list[str] = []
    for entry in proj.get("license-files") or []:
        if any(ch in entry for ch in "*?["):
            if not list(pyproject.parent.glob(entry)):
                p.append(f"L2 license-files glob {entry!r} matches nothing")
        elif not (pyproject.parent / entry).is_file():
            p.append(f"L2 license-files entry {entry!r} is not a file")
    if not proj.get("license-files"):
        p.append("L2 pyproject declares no license-files")
    return p


def gate_licenses(skip_wheel_build: bool) -> list[str]:
    p = gate_license_files()
    if skip_wheel_build or p:
        return p
    out = Path(tempfile.mkdtemp(prefix="pretag-dist-"))
    env = {k: v for k, v in os.environ.items() if k != "PYTHONPATH"}
    for what in ("--wheel", "--sdist"):
        r = _run([sys.executable, "-m", "build", what, "--outdir", out, REPO], env=env)
        if r.returncode != 0:
            return [f"L2 python -m build {what} failed:\n{_tail(r)}"]
    for script in ("verify_wheel_license.py", "verify_wheel_clean.py"):
        r = _run([sys.executable, SCRIPTS / script, "--dist", out] + (["--cargo-lock", REPO / "Cargo.lock"] if script == "verify_wheel_license.py" else []))
        if r.returncode != 0:
            p.append(f"L2 {script} RED on the locally built distribution:\n{_tail(r)}")
    shutil.rmtree(out, ignore_errors=True)
    return p


def gate_versions(tag: str) -> list[str]:
    p: list[str] = []
    for argv in ([sys.executable, SCRIPTS / "check_versions.py"], [sys.executable, SCRIPTS / "guard_tag_version.py", tag]):
        r = _run(argv)
        if r.returncode != 0:
            p.append(f"C4 {Path(str(argv[1])).name} RED:\n{_tail(r)}")
    node = shutil.which("node")
    if not node:
        return p + ["C4 node not found -- `node npm/publish.mjs --check` (stamp idempotence) cannot run on this machine"]
    r = _run([node, REPO / "npm" / "publish.mjs", "--check"], env={**os.environ, "GITHUB_REF_NAME": tag})
    if r.returncode != 0:
        p.append(f"C4 stamp idempotence RED (a package.json byte would change at publish; run `python scripts/set_version.py {tag.lstrip('v')}` and commit):\n{_tail(r)}")
    return p


def mirror_pin_problems(text: str, tag: str) -> list[str]:
    """L3 static: both git sources pinned to exactly `@<tag>`."""
    found: dict[str, str] = {}
    for line in text.splitlines():
        m = PIN_RE.match(line.strip())
        if m:
            found[m.group("name")] = m.group("tag")
    p: list[str] = []
    for name in ("pythscribe", "gradio_wasmfunction"):
        if name not in found:
            p.append(f"L3 requirements.txt has no `{name} @ git+https://github.com/swetmr/pythscribe.git@<tag>` pin")
        elif found[name] != tag:
            p.append(f"L3 requirements.txt pins {name} to `@{found[name]}`, not the tag being cut `@{tag}` (a stale / non-existent tag will not resolve on the mirror)")
    return p


def gate_mirror_pin(tag: str, post_tag: bool) -> list[str]:
    if not SPACE_REQUIREMENTS.is_file():
        return [f"L3 {SPACE_REQUIREMENTS} missing"]
    p = mirror_pin_problems(SPACE_REQUIREMENTS.read_text(encoding="utf-8"), tag)
    if p or not post_tag:
        return p
    venv = Path(tempfile.mkdtemp(prefix="pretag-space-"))
    r = _run([sys.executable, "-m", "venv", venv])
    py = venv / ("Scripts" if os.name == "nt" else "bin") / ("python.exe" if os.name == "nt" else "python")
    r = _run([py, "-m", "pip", "install", "-r", SPACE_REQUIREMENTS], timeout=3600)
    if r.returncode != 0:
        return [f"L3 the Space mirror pin does not resolve in a clean venv (is the public tag {tag} pushed?):\n{_tail(r)}"]
    code = ("import sys; sys.path.insert(0, %r)\nimport kernels\nfrom pythscribe import binding_of\n"
            "b = binding_of(kernels.downscale_box)\nassert b.artifact_status == 'resolved', b.artifact_status\nprint('artifact', b.mode)" % str(SPACE_REQUIREMENTS.parent))
    r = _run([py, "-c", code], cwd=SPACE_REQUIREMENTS.parent, env={k: v for k, v in os.environ.items() if k != "PYTHONPATH"})
    shutil.rmtree(venv, ignore_errors=True)
    return [] if r.returncode == 0 else [f"L3 the committed Space artifacts do not resolve from the mirror install:\n{_tail(r)}"]


def gate_smoke() -> list[str]:
    r = _run([sys.executable, "-m", "pytest", "tests/pythscribe/test_all_features_smoke.py", "-q", "-p", "no:cacheprovider"],
             env={**os.environ, "PYTHSCRIBE_REQUIRE_ORACLE": "1"})
    return [] if r.returncode == 0 else [f"L5 all-features smoke RED against the src build:\n{_tail(r)}"]


def gate_readme() -> list[str]:
    r = _run([sys.executable, SCRIPTS / "readme_spots.py", "--static"])
    return [] if r.returncode == 0 else [f"K README static claims RED:\n{_tail(r)}"]


def gate_workflows() -> list[str]:
    r = _run([sys.executable, SCRIPTS / "lint_release_workflows.py"])
    return [] if r.returncode == 0 else [f"WF workflow lint RED:\n{_tail(r)}"]


def gate_payloads() -> list[str]:
    if not (shutil.which("npm") or shutil.which("npm.cmd")):
        return ["C5 npm not found -- the prepared-payload gate (membership, LF, mirror identity) cannot run on this machine"]
    out = Path(tempfile.mkdtemp(prefix="pretag-payload-"))
    r = _run([sys.executable, SCRIPTS / "prepare_release_payloads.py", "--out", out])
    if r.returncode != 0:
        shutil.rmtree(out, ignore_errors=True)
        return [f"C5 prepare_release_payloads RED:\n{_tail(r)}"]
    p: list[str] = []
    r = _run([sys.executable, SCRIPTS / "verify_runtime_mirror.py", "--payload", out / "runtime-payload"])
    if r.returncode != 0:
        p.append(f"C5 verify_runtime_mirror RED:\n{_tail(r)}")
    # B5/S10 (added here per B6): the scaffolder mirror + LF + exact-skills gate also runs pre-tag.
    r = _run([sys.executable, SCRIPTS / "verify_scaffolder_mirror.py", "--payload", out / "scaffolder-payload"])
    if r.returncode != 0:
        p.append(f"C5 verify_scaffolder_mirror RED:\n{_tail(r)}")
    shutil.rmtree(out, ignore_errors=True)
    return p


def gate_ci(post_tag: bool) -> list[str] | str:
    """B6 / operational (codex re-review): exact-SHA CI success is MANDATORY and enforced in
    release.yml (`require_ci_success.py` in npm-publish) BEFORE any publish -- that enforcement is
    never weakened. Locally it is DEFERRED, not red-by-default: pretag runs BEFORE the release commit
    exists, so checking HEAD here would verify the WRONG (pre-commit) SHA. Only with `--post-tag`
    AND a configured repo/token does it verify the actual committed/tagged commit here. Otherwise it
    returns DEFERRED -- neither a false-GREEN (it did not run) nor a false-RED (it is not supposed to
    run pre-commit; the workflow enforces it)."""
    if not post_tag or not os.environ.get("GITHUB_REPOSITORY"):
        return DEFERRED
    sha = _run(["git", "rev-parse", "HEAD"]).stdout.strip()
    r = _run([sys.executable, SCRIPTS / "require_ci_success.py", "--sha", sha])
    return [] if r.returncode == 0 else [f"B6 exact-SHA CI success RED at {sha[:12]}:\n{_tail(r)}"]


def gates(tag: str, *, post_tag: bool, skip_wheel_build: bool) -> dict[str, Callable[[], list[str]]]:
    return {
        "layout": gate_layout,
        "licenses": lambda: gate_licenses(skip_wheel_build),
        "versions": lambda: gate_versions(tag),
        "mirror-pin": lambda: gate_mirror_pin(tag, post_tag),
        "smoke": gate_smoke,
        "readme": gate_readme,
        "workflows": gate_workflows,
        "payloads": gate_payloads,
        "ci": lambda: gate_ci(post_tag),
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="pretag_gate.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("tag", help="the tag about to be cut, e.g. v0.2.5")
    ap.add_argument("--only", nargs="*", default=None, help="run only these gates")
    ap.add_argument("--post-tag", action="store_true", help="L3: also resolve the Space mirror pin in a clean venv (after the public tag)")
    ap.add_argument("--skip-wheel-build", action="store_true", help="L2: skip the local wheel/sdist build (file-presence check only)")
    ns = ap.parse_args(argv)
    if not re.fullmatch(r"v\d+\.\d+\.\d+", ns.tag):
        print(f"pretag_gate: tag {ns.tag!r} must be v<X.Y.Z>", file=sys.stderr)
        return 2
    all_gates = gates(ns.tag, post_tag=ns.post_tag, skip_wheel_build=ns.skip_wheel_build)
    selected = ns.only if ns.only else list(all_gates)
    unknown = [g for g in selected if g not in all_gates]
    if unknown:
        print(f"pretag_gate: unknown gate(s) {unknown}; known: {list(all_gates)}", file=sys.stderr)
        return 2
    is_full = not ns.only  # B6: only a FULL run (every mandatory gate) is release-authorizing
    red: dict[str, list[str]] = {}
    deferred: list[str] = []
    for name in selected:
        problems = all_gates[name]()
        if problems == DEFERRED:
            deferred.append(name)
            print(f"[{name}] DEFERRED -- enforced in release.yml before publication (not runnable pre-commit here; "
                  "set GITHUB_REPOSITORY + a token and pass --post-tag to run it against the tagged commit)")
            continue
        print(f"[{name}] {'GREEN' if not problems else 'RED'}")
        if problems:
            red[name] = problems
    print(L4_CHECKLIST)
    if red:
        print(f"pretag_gate: RED ({len(red)} gate(s)) -- do NOT tag {ns.tag}:", file=sys.stderr)
        for name, problems in red.items():
            for x in problems:
                print(f"  [{name}] {x}", file=sys.stderr)
        return 1
    print(final_status(ns.tag, selected, is_full=is_full, deferred=deferred))
    return 0


def final_status(tag: str, selected: list[str], *, is_full: bool, deferred: list[str]) -> str:
    """The NON-RED terminal verdict line. `GREEN (release-authorizing)` is reserved for a FULL run in
    which EVERY mandatory gate actually RAN and passed -- a subset is DIAGNOSTIC, and a full run with a
    DEFERRED mandatory gate (e.g. exact-SHA CI, which release.yml enforces before publish) is
    PARTIAL/READY-EXCEPT-DEFERRED. Neither prints the authorizing GREEN (codex pass-3 operational: a
    full run with CI deferred used to print `GREEN (release-authorizing)`, contradicting the sentinel)."""
    # B6: a `--only` subset that passes is DIAGNOSTIC, never an unqualified release-authorizing GREEN
    # (the codex reproduction: `--only workflows` printed GREEN while the version guard was RED).
    if not is_full:
        return (f"pretag_gate: PARTIAL/DIAGNOSTIC -- the subset {selected} passed, but this is NOT a "
                f"release-authorizing GREEN. Run the FULL gate (`python scripts/pretag_gate.py {tag}`) before tagging.")
    if deferred:
        return (f"pretag_gate: PARTIAL/READY-EXCEPT-DEFERRED -- {len(selected) - len(deferred)} gate(s) passed but "
                f"{len(deferred)} MANDATORY gate(s) were DEFERRED ({deferred}) and did NOT run here (enforced in "
                f"release.yml before publication). This is NOT a release-authorizing GREEN -- run the deferred "
                f"gate(s) against the tagged commit (set GITHUB_REPOSITORY + a token, pass --post-tag) to authorize {tag}.")
    return (f"pretag_gate: GREEN (release-authorizing) -- all {len(selected)} gates for {tag} ran and passed; "
            f"L4 is the manual checklist line above")


if __name__ == "__main__":
    sys.exit(main())
