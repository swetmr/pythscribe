#!/usr/bin/env python3
"""shadow_491 — the builtin-shadow 3-way differential (issue #491, name-binding soundness).

Every `cases/*.ps` is a plain-Python PROGRAM (no PythScribe extensions) whose printed
output pins CPython's module-scope name-binding semantics for a builtin name that a
user `def` / `import` / assignment / class / for-target / `global` / `del` rebinds:

    * a builtin name is the BUILTIN until its shadowing binder EXECUTES at module
      scope, and the user binding after (call-time resolution, not emit-order);
    * function bodies resolve at CALL time (B2/B3: a function defined BEFORE the
      shadowing def sees the builtin when called before the def, the user fn after);
    * the LAST module-level binder wins (def -> import -> def, `del` restores the
      builtin, a `global` write in a function rebinds the module cell).

Each case is run on THREE arms and diffed as exact stdout:

    cpython   the pinned oracle (PYTHS_ORACLE_PYTHON, default `py -3.12`)
    js        `pyths compile --target js`      + node
    js+wasm   `pyths compile --target js+wasm` + node  (glue entry: WASM + the JS twin)

A header directive in the case tunes the check:

    # EXPECT: match           (default) every arm's stdout == CPython's stdout
    # EXPECT: loud            the compiled arms may REFUSE (non-zero compile or a
                              thrown error) but must NEVER print a value CPython did
                              not print (stdout must be a prefix of CPython's)
    # WASM: f,g               the js+wasm arm must ADMIT exactly these functions
    # DEMOTE: h               the js+wasm arm must REFUSE these functions (stay JS)

The harness is its own negative control: at the base commit (de345491) the B2/B3/
twin/other-binder cases are RED (silent wrong values) — see the design note
`crates/pyths_codegen_wasm/SHADOW_BINDING_DESIGN.md` for the per-case mutation ledger.

Run from the repo root (needs target/{release,debug}/pyths[.exe], node, the oracle):
    python tests/differential/shadow_491/shadow_491.py [--only NAME] [--keep]
Env: PYTHS_BIN, PYTHS_ORACLE_PYTHON (e.g. "py -3.12"), SHADOW_491_SCRATCH.
"""
from __future__ import annotations

import argparse
import os
import re
import shlex
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent.parent
CASES = HERE / "cases"
_EXE = "pyths.exe" if sys.platform == "win32" else "pyths"
PYTHS = Path(os.environ.get("PYTHS_BIN") or next(
    (str(p) for p in (ROOT / "target" / "release" / _EXE, ROOT / "target" / "debug" / _EXE) if p.exists()),
    str(ROOT / "target" / "release" / _EXE),
))
ORACLE = shlex.split(os.environ.get("PYTHS_ORACLE_PYTHON", "py -3.12"))
RUNTIME_INDEX = (ROOT / "runtime" / "src" / "index.js").resolve()
SCRATCH = Path(os.environ.get("SHADOW_491_SCRATCH") or (HERE / ".scratch"))
ANSI = re.compile(r"\x1b\[[0-9;]*m")

RUNNER = """\
// shadow_491 runner: import the compiled ES module (its module body IS the program).
import { pathToFileURL } from "node:url";
try {
  await import(pathToFileURL(process.argv[2]).href);
} catch (e) {
  console.error("UNCAUGHT:", e && e.stack ? e.stack : String(e));
  process.exit(3);
}
"""


@dataclass
class Case:
    name: str
    path: Path
    expect: str = "match"
    wasm: set[str] | None = None
    demote: set[str] = field(default_factory=set)


@dataclass
class ArmResult:
    stdout: str
    rc: int
    log: str = ""
    admitted: set[str] = field(default_factory=set)
    skipped: dict[str, str] = field(default_factory=dict)


def run(cmd, cwd=None, env=None, timeout=300):
    return subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, encoding="utf-8",
                          errors="replace", timeout=timeout, check=False)


def parse_case(path: Path) -> Case:
    c = Case(path.stem, path)
    for line in path.read_text(encoding="utf-8").splitlines()[:8]:
        m = re.match(r"#\s*(EXPECT|WASM|DEMOTE):\s*(.*)$", line.strip())
        if not m:
            continue
        key, val = m.group(1), m.group(2).strip()
        if key == "EXPECT":
            c.expect = val
        elif key == "WASM":
            c.wasm = {s.strip() for s in val.split(",") if s.strip()}
        elif key == "DEMOTE":
            c.demote = {s.strip() for s in val.split(",") if s.strip()}
    assert c.expect in ("match", "loud"), f"{path}: bad EXPECT {c.expect!r}"
    return c


def rewire(path: Path) -> None:
    """Point the emitted `pyths-runtime[/subpath]` imports at the in-repo runtime source
    (the wasm_net rewiring), so the arms run against the shipped runtime copy."""
    js = path.read_text(encoding="utf-8")
    js = re.sub(r'from\s+["\']pyths-runtime["\']', f'from "{RUNTIME_INDEX.as_uri()}"', js)

    def sub(m):
        target = (RUNTIME_INDEX.parent / (m.group(1) + ".js")).resolve()
        return f'from "{target.as_uri()}"'

    js = re.sub(r'from\s+["\']pyths-runtime/([\w/.-]+)["\']', sub, js)
    path.write_text(js, encoding="utf-8")


def cpython_arm(case: Case) -> ArmResult:
    r = run([*ORACLE, str(case.path)], cwd=str(ROOT))
    return ArmResult(r.stdout.replace("\r\n", "\n"), r.returncode, r.stderr)


def compiled_arm(case: Case, target: str, scratch: Path) -> ArmResult:
    out_dir = scratch / case.name / target.replace("+", "_")
    out_dir.mkdir(parents=True, exist_ok=True)
    src = out_dir / f"{case.name}.ps"
    shutil.copyfile(case.path, src)
    out_js = out_dir / f"{case.name}.js"
    env = {**os.environ, "PYTHS_NO_CACHE": "1"}
    r = run([str(PYTHS), "compile", str(src), "--target", target, "-o", str(out_js), "--verbose"], env=env)
    log = ANSI.sub("", (r.stdout or "") + (r.stderr or ""))
    res = ArmResult("", r.returncode, log)
    res.admitted = set(re.findall(r"WASM: (\w+)", log))
    res.skipped = {m.group(1): m.group(2).strip() for m in re.finditer(r"Skipped (\w+): (.*)", log)}
    if r.returncode != 0 or not out_js.exists():
        res.rc = r.returncode or 2
        return res
    rewire(out_js)
    glue = out_js.with_suffix(".glue.js")
    if glue.exists():
        rewire(glue)
    runner = scratch / "run_program.mjs"
    if not runner.exists():
        runner.write_text(RUNNER, encoding="utf-8")
    n = run(["node", str(runner), str(out_js)], cwd=str(out_dir))
    res.stdout = n.stdout.replace("\r\n", "\n")
    res.rc = n.returncode
    res.log += "\n" + (n.stderr or "")
    return res


def check(case: Case, py: ArmResult, arm: str, r: ArmResult) -> list[str]:
    bad: list[str] = []
    if py.rc != 0:
        return [f"{arm}: CPython arm itself failed (rc={py.rc}): {py.log.strip()[-300:]}"]
    if case.expect == "match":
        if r.rc != 0:
            bad.append(f"{arm}: rc={r.rc} (CPython rc=0); log tail: {r.log.strip()[-400:]}")
        if r.stdout != py.stdout:
            bad.append(f"{arm}: stdout differs\n  cpython: {py.stdout!r}\n  {arm:7}: {r.stdout!r}")
    else:  # loud: refusal is fine, a wrong VALUE is not
        if r.rc == 0 and r.stdout != py.stdout:
            bad.append(f"{arm}: SILENT divergence (rc=0)\n  cpython: {py.stdout!r}\n  {arm:7}: {r.stdout!r}")
        if not py.stdout.startswith(r.stdout):
            bad.append(f"{arm}: printed a value CPython did not\n  cpython: {py.stdout!r}\n  {arm:7}: {r.stdout!r}")
    if arm == "js+wasm" and r.log:
        if case.wasm is not None and r.admitted != case.wasm:
            bad.append(f"{arm}: admitted {sorted(r.admitted)} != expected {sorted(case.wasm)}")
        for f in case.demote:
            if f in r.admitted:
                bad.append(f"{arm}: `{f}` must be DEMOTED (stay JS) but was admitted to WASM")
            elif f not in r.skipped:
                bad.append(f"{arm}: `{f}` expected in the Skipped list (demotion reason), not found")
    return bad


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--only", help="run one case (stem)")
    ap.add_argument("--keep", action="store_true", help="keep the scratch dir")
    args = ap.parse_args()
    if not Path(PYTHS).exists():
        print(f"[shadow_491] pyths binary not found: {PYTHS}", file=sys.stderr)
        return 2
    if SCRATCH.exists() and not args.keep:
        shutil.rmtree(SCRATCH, ignore_errors=True)
    SCRATCH.mkdir(parents=True, exist_ok=True)
    cases = [parse_case(p) for p in sorted(CASES.glob("*.ps"))]
    if args.only:
        cases = [c for c in cases if c.name == args.only]
    red = 0
    for case in cases:
        py = cpython_arm(case)
        js = compiled_arm(case, "js", SCRATCH)
        jw = compiled_arm(case, "js+wasm", SCRATCH)
        bad = check(case, py, "js", js) + check(case, py, "js+wasm", jw)
        adm = ",".join(sorted(jw.admitted)) or "-"
        if bad:
            red += 1
            print(f"RED   {case.name}  [wasm: {adm}]")
            for b in bad:
                print("      " + b.replace("\n", "\n      "))
        else:
            print(f"green {case.name}  [wasm: {adm}]  expect={case.expect}")
    print(f"[shadow_491] {len(cases) - red} green / {red} RED of {len(cases)} cases  (pyths={PYTHS})")
    return 1 if red else 0


if __name__ == "__main__":
    sys.exit(main())
