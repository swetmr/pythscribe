#!/usr/bin/env python3
"""coverage-collect.py -- differential-oracle coverage profile for the pyths binary.

Phase 0/1 of the change-impact coverage gate (scoping doc:
reference-app/scoping_coverage_gate.md). Sibling of the planned pbt-gate.py in the
local pre-merge checklist.

What it does
  1. Runs `cargo llvm-cov show-env` with CARGO_TARGET_DIR pinned to
     target/llvm-cov-target, so the instrumented build NEVER disturbs
     target/release or target/debug. (cargo-llvm-cov 0.8.x uses a
     RUSTC_WRAPPER whose default target dir is the plain `target/`; the
     explicit CARGO_TARGET_DIR override is what isolates it. The wrapper
     RUSTFLAGS value contains literal \\x1f separator bytes -- parsed and
     forwarded verbatim.)
  2. Builds the instrumented `pyths` binary (`cargo build --release --bin
     pyths` under that env) -> target/llvm-cov-target/release/pyths(.exe).
  3. Drives it over the differential corpus (or a --subset slice), one
     compiler process per program, so LLVM writes one .profraw per compile
     (LLVM_PROFILE_FILE=...pyths-%p.profraw).
  4. Merges everything into lcov via `cargo llvm-cov report --lcov
     --output-path target/oracle-coverage.lcov`.
  5. Deletes the .profraw temp files (unless --keep-profraw).

Semantics: "covered" = this compiler line executed while compiling a program
whose output is asserted against a CPython byte-differential oracle. That is
stronger than bare coverage, but it is NOT input-adequacy: a region covered
only by benign inputs still passes (scoping doc section 4).

Drivers
  --driver direct    (default when --subset is given) spawn `pyths compile`
                     per corpus entry directly from Python. Compiler-side
                     coverage is identical to run.mjs for the same entries
                     (same argv, PYTHS_NO_CACHE=1); it skips the CPython/node
                     oracle re-execution, which never touches the Rust binary.
                     Corpus entries are already oracle-green in CI.
  --driver run-mjs   full fidelity: drives tests/differential/run.mjs with
                     PYTHS_BIN pointed at the instrumented binary (uses the
                     PYTHS_BIN env override in run.mjs). Whole corpus, incl.
                     oracle comparison. NOTE: ~1,270 profraws at ~240 KB
                     each -- a few hundred MB transiently; mind free disk.

WASM drivers (the pyths_codegen_wasm side of the profile)
  The corpus above compiles --target js only, so it leaves pyths_codegen_wasm
  at ZERO coverage. Two additional oracle-backed drivers close that hole and
  run BY DEFAULT on full (run-mjs) collections (skipped on --subset spikes
  unless forced):
  --livermore        tests/differential/livermore/run.mjs -- the 24 Livermore
                     kernels, tri-way CPython vs --target js vs js+wasm, with
                     a routed-to-WASM assertion per kernel.
  --k9               the K9 generated WASM-kernel net (#358/#363/#364 families
                     + the directed floor-div battery) from the reference-app
                     pbt-ps harness (env PBT_PS_DIR or ../reference-app/
                     experiments/pbt-ps). Deterministic per --k9-seed.
                     Requires `hypothesis` importable by this python.
  Both drivers use per-process %p profraw names (the child spawns are made by
  node/python drivers, so per-program names are not settable from here).
  Windows PID reuse loses a fraction of profiles (observed ~15-20%); for the
  boolean whole-corpus union this is acceptable -- the generator families
  re-hit the same emit paths across many processes.

Usage
  python scripts/coverage-collect.py --subset 40          # Phase-0 spike
  python scripts/coverage-collect.py                      # AUTHORITATIVE:
      # full corpus + livermore + K9 -> target/oracle-coverage.lcov
  python scripts/coverage-collect.py --subset 40 --skip-build
  python scripts/coverage-collect.py --subset 3 --skip-build --livermore \
      --k9 --k9-n 10                                      # WASM-driver smoke
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
COV_TARGET = REPO / "target" / "llvm-cov-target"
CORPUS = REPO / "tests" / "differential" / "cpython_corpus.json"
DEFAULT_LCOV = REPO / "target" / "oracle-coverage.lcov"
# Two Windows-verified quirks of cargo-llvm-cov 0.8.7 (Phase-0 findings):
#   1. `report` only discovers profraws matching its own package-prefixed
#      pattern at the target-dir ROOT (a custom subdir/name fails with
#      "not found *.profraw files").
#   2. show-env's default pattern ends in `-%p-%16m.profraw`; the %Nm
#      merge-pool mode silently writes NOTHING on this Windows setup, so we
#      override to a per-process `-%p` pattern (which is also what
#      per-program attribution needs).
#   3. pyths' main() always ends in std::process::exit(), which on Windows
#      is ExitProcess -- CRT atexit handlers (incl. the LLVM profile
#      writer) never run, so profraws come out 0 bytes. Verified with a
#      minimal rustc -Cinstrument-coverage repro: normal-return main writes
#      240 bytes, process::exit main writes 0. Fix WITHOUT touching the
#      compiler: continuous mode (%c) mmaps the counters into the profile
#      file as the program runs, needing no exit hook.
#   4. On non-Darwin platforms %c additionally requires the binary to be
#      built with -Cllvm-args=-runtime-counter-relocation; without it the
#      profraw has the right shape but ALL counters silently stay zero.
#      We append that flag to the \x1f-separated wrapper RUSTFLAGS.
#      Verified end-to-end on rustc 1.94.0 / Windows 10.
PROFRAW_GLOB = "pythscribe-*.profraw"
SCRATCH = COV_TARGET / "diff-scratch"
BIN_NAME = "pyths.exe" if os.name == "nt" else "pyths"


def log(msg: str) -> None:
    print(f"[collect] {msg}", flush=True)


def llvm_cov_env() -> dict:
    """Env dict for instrumented build + report, isolated to llvm-cov-target."""
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(COV_TARGET)
    r = subprocess.run(
        ["cargo", "llvm-cov", "show-env"],
        cwd=REPO, env=env, capture_output=True,
    )
    if r.returncode != 0:
        sys.exit(f"[collect] cargo llvm-cov show-env failed:\n"
                 f"{r.stderr.decode('utf-8', errors='replace')}")
    # Lines look like KEY='value' (or `export KEY=...` with --sh). Values may
    # contain \x1f separator bytes -- keep them byte-faithful.
    for raw in r.stdout.decode("utf-8", errors="surrogateescape").splitlines():
        line = raw.strip()
        for pre in ("export ", "set ", "$env:"):
            if line.startswith(pre):
                line = line[len(pre):]
        if "=" not in line or line.startswith("#"):
            continue
        k, v = line.split("=", 1)
        v = v.strip()
        if len(v) >= 2 and v[0] == v[-1] and v[0] in ("'", '"'):
            v = v[1:-1]
        env[k.strip()] = v
    # Per-process profile per corpus program, at the target root with the
    # package prefix so `report` discovers it; %c (continuous mode) because
    # pyths exits via process::exit -- see PROFRAW_GLOB notes 2-4.
    env["LLVM_PROFILE_FILE"] = str(COV_TARGET / "pythscribe-%p%c.profraw")
    wrapper_flags_key = "__CARGO_LLVM_COV_RUSTC_WRAPPER_RUSTFLAGS"
    reloc = "-Cllvm-args=-runtime-counter-relocation"
    if wrapper_flags_key in env and reloc not in env[wrapper_flags_key]:
        env[wrapper_flags_key] += "\x1f" + reloc
    return env


def build(env: dict) -> Path:
    log("building instrumented pyths (release, target dir: "
        "target/llvm-cov-target) ...")
    t0 = time.time()
    r = subprocess.run(["cargo", "build", "--release", "--bin", "pyths"],
                       cwd=REPO, env=env)
    if r.returncode != 0:
        sys.exit("[collect] instrumented build failed")
    log(f"build done in {time.time() - t0:.0f}s")
    binary = COV_TARGET / "release" / BIN_NAME
    if not binary.exists():
        sys.exit(f"[collect] expected instrumented binary missing: {binary}")
    return binary


def drive_direct(env: dict, binary: Path, subset: int) -> None:
    corpus = json.loads(CORPUS.read_text(encoding="utf-8"))
    entries = corpus[: subset] if subset else corpus
    SCRATCH.mkdir(parents=True, exist_ok=True)
    cenv = dict(env)
    cenv["PYTHS_NO_CACHE"] = "1"
    log(f"direct driver: compiling {len(entries)} corpus program(s), "
        f"one process each ...")
    t0 = time.time()
    fails = []
    for e in entries:
        setup = e.get("_setup", "")
        src = (setup + "\n" if setup else "") + f"print(repr({e['expr']}))\n"
        ps = SCRATCH / f"{e['id']}.ps"
        ps.write_text(src, encoding="utf-8")
        # Per-program profile name: avoids PID-reuse collisions (observed:
        # 40 spawns -> 33 %p files) and is the natural hook for Phase-2
        # per-program attribution.
        cenv["LLVM_PROFILE_FILE"] = str(
            COV_TARGET / f"pythscribe-{e['id']}%c.profraw")
        r = subprocess.run([str(binary), "compile", str(ps)],
                           env=cenv, capture_output=True)
        if r.returncode != 0:
            fails.append(e["id"])
    log(f"compiled {len(entries) - len(fails)}/{len(entries)} in "
        f"{time.time() - t0:.0f}s"
        + (f" (compile failures: {', '.join(fails[:10])})" if fails else ""))


def drive_run_mjs(env: dict, binary: Path) -> None:
    cenv = dict(env)
    cenv["PYTHS_BIN"] = str(binary)
    log("run.mjs driver: full corpus under the CPython oracle ...")
    r = subprocess.run(["node", str(REPO / "tests" / "differential" / "run.mjs")],
                       cwd=REPO, env=cenv)
    if r.returncode != 0:
        log("WARNING: run.mjs exited non-zero (oracle failures); "
            "profraws from completed compiles are still valid")


def drive_livermore(env: dict, binary: Path) -> None:
    """24 Livermore kernels, tri-way (CPython / js / js+wasm) -- the in-repo
    oracle-backed driver that exercises pyths_codegen_wasm."""
    cenv = dict(env)
    cenv["PYTHS_BIN"] = str(binary)
    # Child pyths spawns come from node -- per-process %p is the only handle.
    cenv["LLVM_PROFILE_FILE"] = str(COV_TARGET / "pythscribe-lmk-%p%c.profraw")
    # The harness rewires generated .js/.glue.js in place, so leftovers from a
    # previous run no longer hash-match pyths' generation record and trip the
    # cache-substitution overwrite guard (#416). Start from a clean scratch.
    shutil.rmtree(REPO / "tests" / "differential" / "livermore" / ".scratch",
                  ignore_errors=True)
    log("livermore driver: 24 kernels, tri-way incl. --target js+wasm ...")
    t0 = time.time()
    r = subprocess.run(
        ["node", str(REPO / "tests" / "differential" / "livermore" / "run.mjs")],
        cwd=REPO, env=cenv)
    if r.returncode != 0:
        log("WARNING: livermore run.mjs exited non-zero (oracle failures); "
            "profraws from completed compiles are still valid")
    log(f"livermore done in {time.time() - t0:.0f}s")


def default_k9_dir() -> Path:
    """The pbt-ps harness home: $PBT_PS_DIR, else the reference-app sibling."""
    envd = os.environ.get("PBT_PS_DIR")
    if envd:
        return Path(envd)
    return REPO.parent / "reference-app" / "experiments" / "pbt-ps"


def drive_k9(env: dict, binary: Path, k9_dir: Path, n: int, seed: int) -> None:
    """K9 generated WASM-kernel net (pbt_k9.py): forces generated numeric
    kernels onto --target js+wasm under a 3-way byte oracle. This is what
    lifts pyths_codegen_wasm from 0/0 to real oracle-backed coverage."""
    runner = k9_dir / "pbt_k9.py"
    if not runner.exists():
        log(f"WARNING: K9 runner not found at {runner} -- skipping "
            f"(set PBT_PS_DIR or --k9-dir). pyths_codegen_wasm coverage "
            f"will be limited to the livermore kernels.")
        return
    cenv = dict(env)
    cenv["PYTHS_BIN"] = str(binary)
    # The instrumented binary lives in target/llvm-cov-target/release/, so
    # K9's repo-relative runtime derivation (parents[2]) would miss -- point
    # it at the real runtime explicitly.
    cenv["PYTHS_RUNTIME"] = str(REPO / "runtime" / "src" / "index.js")
    cenv["LLVM_PROFILE_FILE"] = str(COV_TARGET / "pythscribe-k9-%p%c.profraw")
    log(f"K9 driver: {n} generated WASM kernels (seed {seed}) + directed "
        f"floor-div battery, from {k9_dir} ...")
    t0 = time.time()
    r = subprocess.run(
        [sys.executable, str(runner), "--n", str(n), "--seed", str(seed)],
        cwd=k9_dir, env=cenv)
    if r.returncode != 0:
        log("WARNING: pbt_k9.py exited non-zero; profraws from completed "
            "compiles are still valid")
    log(f"K9 done in {time.time() - t0:.0f}s")


def report(env: dict, out: Path) -> None:
    n = len(list(COV_TARGET.glob(PROFRAW_GLOB)))
    log(f"merging {n} profraw file(s) -> {out}")
    # --release is required: the binary was built --release, and report
    # otherwise searches only <target>/debug for object files.
    r = subprocess.run(
        ["cargo", "llvm-cov", "report", "--release", "--lcov",
         "--output-path", str(out)],
        cwd=REPO, env=env,
    )
    if r.returncode != 0:
        sys.exit("[collect] cargo llvm-cov report failed")


def lcov_stats(path: Path) -> str:
    files = instrumented = covered = 0
    with path.open(encoding="utf-8", errors="replace") as f:
        for line in f:
            if line.startswith("SF:"):
                files += 1
            elif line.startswith("DA:"):
                instrumented += 1
                if not line.rstrip().endswith(",0"):
                    covered += 1
    pct = 100.0 * covered / instrumented if instrumented else 0.0
    return (f"{files} source files, {covered}/{instrumented} "
            f"instrumented lines hit ({pct:.1f}%)")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--subset", type=int, default=0,
                    help="only the first N corpus entries (0 = all)")
    ap.add_argument("--driver", choices=["direct", "run-mjs"], default=None,
                    help="default: direct if --subset given, else run-mjs")
    ap.add_argument("--skip-build", action="store_true",
                    help="reuse existing instrumented binary")
    ap.add_argument("--keep-profraw", action="store_true")
    ap.add_argument("--output", type=Path, default=DEFAULT_LCOV)
    ap.add_argument("--livermore", action=argparse.BooleanOptionalAction,
                    default=None,
                    help="run the Livermore tri-way WASM driver "
                         "(default: yes on full runs, no on --subset spikes)")
    ap.add_argument("--k9", action=argparse.BooleanOptionalAction, default=None,
                    help="run the K9 generated-WASM-kernel net "
                         "(default: yes on full runs, no on --subset spikes)")
    ap.add_argument("--k9-n", type=int, default=150,
                    help="K9 generated kernels (default 150)")
    ap.add_argument("--k9-seed", type=int, default=9)
    ap.add_argument("--k9-dir", type=Path, default=None,
                    help="pbt-ps harness dir (default: $PBT_PS_DIR or "
                         "../reference-app/experiments/pbt-ps)")
    args = ap.parse_args()
    driver = args.driver or ("direct" if args.subset else "run-mjs")
    if driver == "run-mjs" and args.subset:
        sys.exit("[collect] --subset requires --driver direct "
                 "(run.mjs always runs the full corpus)")
    full_run = driver == "run-mjs"
    do_livermore = full_run if args.livermore is None else args.livermore
    do_k9 = full_run if args.k9 is None else args.k9

    env = llvm_cov_env()
    # Clean stale profraws so the merged profile reflects exactly this run.
    for stray in COV_TARGET.glob("*.profraw"):
        stray.unlink()

    binary = (COV_TARGET / "release" / BIN_NAME) if args.skip_build \
        else build(env)
    if args.skip_build and not binary.exists():
        sys.exit(f"[collect] --skip-build but no binary at {binary}")

    if driver == "direct":
        drive_direct(env, binary, args.subset)
    else:
        drive_run_mjs(env, binary)
    if do_livermore:
        drive_livermore(env, binary)
    if do_k9:
        drive_k9(env, binary, args.k9_dir or default_k9_dir(),
                 args.k9_n, args.k9_seed)

    report(env, args.output)
    log(f"lcov: {lcov_stats(args.output)}")
    if not args.keep_profraw:
        for p in COV_TARGET.glob(PROFRAW_GLOB):
            p.unlink()
        shutil.rmtree(SCRATCH, ignore_errors=True)
        log("cleaned .profraw temp files and scratch .ps files")
    scope = f"subset of {args.subset}" if args.subset else "FULL corpus"
    wasm = (" + livermore" if do_livermore else "") + \
        (f" + K9(n={args.k9_n})" if do_k9 else "")
    log(f"done ({scope}, driver={driver}{wasm}). Gate with: "
        f"python scripts/coverage-gate.py --lcov {args.output}")


if __name__ == "__main__":
    main()
