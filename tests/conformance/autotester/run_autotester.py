#!/usr/bin/env python3
r"""Transcrypt-autotester conformance gate — PythScribe vs CPython, RECURRING.

E7 sub-parts 1 + 5: the ported Transcrypt language-conformance testlets
(testlets/*.ps, Apache-2.0 — see ATTRIBUTION.md) run as a PythScribe<->CPython
differential on every CI run, scored per-testlet AND per-check() line, and
ratcheted TWO-SIDEDLY against the committed baseline.json:

  * any REGRESSION (verdict worsens, matched count drops) fails the gate;
  * any IMPROVEMENT (a testlet starts passing, matched count rises) ALSO
    fails until baseline.json is updated in the same PR — progress is
    recorded, never silent.

Oracle discipline (v0.3 scoping §E, 2026-08-24 — load-bearing): the testlets
are Transcrypt's TEST PROGRAMS ONLY; the oracle is CPYTHON (the same pinned
oracle as tests/differential — PYTHS_ORACLE_PYTHON, plain `python` in CI).
Transcrypt's own expected outputs are never consulted (the model<->model
trap; Paper C §sec:transcrypt-oracle).

Verdicts per testlet (same taxonomy as the source harness):
  pass          transcripts equal (possibly after a by-design normalizer)
  diverge       pyths compiles+runs but the transcript differs
  compile_fail  pyths rejects the source
  ps_runtime    pyths compiles but crashes / no sentinel
  oracle_error  CPython itself fails (port defect)

baseline.json also carries the 13 FULL-SURFACE BOUNDARY rows (E7 sub-part 5):
the 2 runnable here (metaclasses, proxies) are measured live and ratcheted
like any testlet; the 11 external ones (multi-file/async/manual/
differentiation — dedicated harnesses, see the reference-app
experiments/autotester-ps FULL_SURFACE.md) are pinned as dispositioned rows
this gate re-prints, so expanding any boundary is an explicit baseline edit.

Harness-integrity (E7 sub-part 4): testlet files found == rows executed ==
rows compared == baseline rows, checked before the verdict; per-testlet
CPython check-line counts must equal the baseline `total`, so the corpus
cannot silently shrink.

Usage:
  python run_autotester.py                # gate against baseline.json
  python run_autotester.py --capture      # (re)write baseline.json
  python run_autotester.py --only classes exceptions
Env: PYTHS_BIN, PYTHS_ORACLE_PYTHON, PS_TIMEOUT.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

from diffnorm import (  # noqa: E402
    ORACLE, PYTHS, SENTINEL, classify_line, norm_out, real_error, run_robust,
)

TESTLETS = HERE / "testlets"
SHIM = (HERE / "autotester_shim.py").read_text(encoding="utf-8")
BASELINE = HERE / "baseline.json"

_ADDR = re.compile(r" at 0x[0-9A-Fa-f]+")
_COMPILE_MARK = re.compile(
    r"(error\[|SyntaxError|ParseError|Unexpected|Expected|unsupported|not supported|"
    r"Unsupported|cannot compile|unknown|no method|is not supported|"
    r"unimplemented|not implemented|unrecognized)", re.I)


def build_source(name: str, body: str) -> str:
    driver = (
        "\n\n# --- differential driver (conformance gate) ---\n"
        "_autoTester = AutoTester()\n"
        "_autoTester.run_testlet(run, %r)\n"
        "_autoTester.done()\n" % name)
    return SHIM + "\n\n# --- ported testlet: %s ---\n" % name + body + driver


def _transcript(out: str) -> list[str]:
    lines = norm_out(_ADDR.sub(" at 0xADDR", out)).splitlines()
    if lines and lines[-1] == SENTINEL:
        lines = lines[:-1]
    return lines


def _match_count(a: list[str], b: list[str]) -> int:
    """Aligned per-check score: lines equal, or by-design-normalized equal."""
    n = 0
    for i in range(min(len(a), len(b))):
        if a[i] == b[i] or classify_line(a[i], b[i]) != "real":
            n += 1
    return n


def evaluate(name: str, body: str, workdir: Path) -> dict:
    src = build_source(name, body)
    ps = workdir / (name + ".ps")
    py = workdir / (name + ".py")
    ps.write_text(src, encoding="utf-8")
    py.write_text(src, encoding="utf-8")

    os.environ.setdefault("PYTHONUTF8", "1")
    os.environ.setdefault("PYTHONIOENCODING", "utf-8")
    rc_py, out_py, err_py = run_robust([*ORACLE, "-X", "utf8", str(py)])
    t_py = _transcript(out_py)
    if rc_py != 0 or not norm_out(out_py).endswith(SENTINEL):
        return {"verdict": "oracle_error", "matched": 0, "total": len(t_py),
                "detail": real_error(err_py) or f"rc={rc_py}"}

    os.environ["PYTHS_NO_CACHE"] = "1"
    rc_ps, out_ps, err_ps = run_robust([str(PYTHS), "run", str(ps)])
    t_ps = _transcript(out_ps)
    matched = _match_count(t_py, t_ps)
    if rc_ps != 0:
        err = real_error(err_ps) or norm_out(err_ps)[-200:]
        verdict = "compile_fail" if _COMPILE_MARK.search(err_ps or "") else "ps_runtime"
        return {"verdict": verdict, "matched": matched, "total": len(t_py),
                "detail": err}
    if not norm_out(out_ps).endswith(SENTINEL):
        return {"verdict": "ps_runtime", "matched": matched, "total": len(t_py),
                "detail": "no sentinel (silent crash / truncated output)"}
    full = (matched == len(t_py) == len(t_ps))
    return {"verdict": "pass" if full else "diverge",
            "matched": matched, "total": len(t_py),
            **({} if full else {"detail": "transcript differs"})}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--capture", action="store_true",
                    help="(re)write baseline.json from this run")
    ap.add_argument("--only", nargs="+", default=None)
    args = ap.parse_args()

    if not Path(PYTHS).exists():
        sys.exit(f"pyths binary not found at {PYTHS} — run `cargo build --release -p pyths_cli`")
    rc, ver, _ = run_robust([str(PYTHS), "--version"])
    rc2, over, _ = run_robust([*ORACLE, "--version"])
    if rc2 != 0:
        sys.exit(f"oracle {' '.join(ORACLE)} not runnable")
    print(f"subject: {ver.strip()} ({PYTHS})")
    print(f"oracle : {over.strip()} ({' '.join(ORACLE)})")

    files = sorted(TESTLETS.glob("*.ps"))
    names = [f.stem for f in files]
    targets = args.only or names
    found = len([n for n in targets if n in names])

    results: dict[str, dict] = {}
    executed = 0
    with tempfile.TemporaryDirectory() as d:
        for f in files:
            if f.stem not in targets:
                continue
            body = f.read_text(encoding="utf-8")
            r = evaluate(f.stem, body, Path(d))
            executed += 1
            results[f.stem] = r
            extra = r.get("detail", "")[:70] if r["verdict"] != "pass" else ""
            print("  [%-12s] %-32s %3d/%-3d %s"
                  % (r["verdict"], f.stem, r["matched"], r["total"], extra))

    # Harness-integrity: nothing silently dropped.
    if executed != found or found == 0:
        sys.exit(f"HARNESS INTEGRITY FAILURE: found={found} executed={executed}")

    passed = sum(1 for r in results.values() if r["verdict"] == "pass")
    total_checks = sum(r["total"] for r in results.values())
    matched_checks = sum(r["matched"] for r in results.values())
    print(f"\nHEADLINE: {passed}/{len(results)} testlets pass; "
          f"per-check {matched_checks}/{total_checks}")

    if args.capture:
        if args.only:
            sys.exit("--capture with --only would write a partial baseline; refusing")
        baseline = json.loads(BASELINE.read_text(encoding="utf-8")) if BASELINE.exists() else {}
        baseline.update({
            "subject": ver.strip(),
            "oracle": over.strip(),
            "headline": {"pass": passed, "testlets": len(results),
                         "checks_matched": matched_checks, "checks_total": total_checks},
            "testlets": {n: {"verdict": r["verdict"], "matched": r["matched"],
                             "total": r["total"]} for n, r in sorted(results.items())},
        })
        BASELINE.write_text(json.dumps(baseline, indent=2) + "\n", encoding="utf-8")
        print(f"baseline captured -> {BASELINE}")
        return 0

    # ---- two-sided ratchet ------------------------------------------------
    if not BASELINE.exists():
        sys.exit("no baseline.json — run with --capture once and commit it")
    baseline = json.loads(BASELINE.read_text(encoding="utf-8"))
    base = baseline["testlets"]
    problems: list[str] = []
    if not args.only and set(base) != set(results):
        problems.append(f"testlet set drift: baseline has {len(base)}, run has {len(results)} "
                        f"(missing={sorted(set(base) - set(results))}, "
                        f"new={sorted(set(results) - set(base))})")
    for n, r in sorted(results.items()):
        b = base.get(n)
        if b is None:
            continue
        cur = (r["verdict"], r["matched"], r["total"])
        exp = (b["verdict"], b["matched"], b["total"])
        if cur != exp:
            kind = ("REGRESSION" if (r["verdict"] != "pass" and b["verdict"] == "pass")
                    or r["matched"] < b["matched"] else "IMPROVEMENT/change")
            problems.append(f"{kind}: {n}: baseline {exp} -> now {cur}")

    # Boundary rows (sub-part 5): presence + count are part of the contract.
    bounds = baseline.get("boundaries", [])
    if len(bounds) != 13:
        problems.append(f"boundary rows: expected the 13 full-surface dispositions, "
                        f"found {len(bounds)}")
    else:
        live = {b["name"]: b for b in bounds if b.get("runner") == "this-gate"}
        for n, b in live.items():
            r = results.get(n)
            if r and (r["verdict"], r["matched"], r["total"]) != \
                    (b["verdict"], b["matched"], b["total"]):
                problems.append(f"BOUNDARY moved: {n}: pinned "
                                f"({b['verdict']},{b['matched']},{b['total']}) -> now "
                                f"({r['verdict']},{r['matched']},{r['total']}) — "
                                "update the boundary row (and celebrate if it improved)")

    if problems:
        print("\nRATCHET: FAIL — baseline.json disagrees with this run "
              "(a regression must be fixed; an improvement must be committed "
              "to baseline.json in the same PR):")
        for p in problems:
            print("  - " + p)
        return 1
    print("RATCHET: OK — run matches the committed baseline exactly "
          f"(incl. {len(bounds)} boundary rows).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
