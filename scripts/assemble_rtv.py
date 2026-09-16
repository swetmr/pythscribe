#!/usr/bin/env python3
"""Assemble the R-TV testpypi-validation evidence record (spec 13-09-26, requirements §5.2; plan M6.3; validation §L5).

    python scripts/assemble_rtv.py --manifest release_manifest.json --legs legs/ --out evidence/R-TV.json

`legs/<triple>/` holds one `testpypi-validate` leg's outputs written by scripts/testpypi_validate.py:
  download.json  the wheel pip fetched from TestPyPI: filename + sha256 (must == manifest wheel_sha256), verdict
  a0.json        A0 on the leg (node-free PATH + no node file under the venv)          -> steps.A0
  leg.json       A1..A4 through the shipped path (scripts/wheel_acceptance.py run)     -> steps.A1..A4
  smoke.json     the L5 all-features smoke (tests/pythscribe/test_all_features_smoke.py) against the
                 installed PUBLIC artifact                                             -> all_features
Verdict `pass` iff EVERY release target is present, its downloaded wheel hashes to the manifest, every step
passed and the all-features field is `pass`. A missing leg, a missing file, or a missing all-features result is
`fail` (a skipped check is a failure; the consumer independently refuses a record without the field).
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts"))
from require_evidence import RECORD_SCHEMA, RTV_ALL_FEATURES_FIELD, RTV_REQUIRED_STEPS, WHEEL_TARGETS  # noqa: E402


def _load(p: Path) -> dict | None:
    try:
        return json.loads(p.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def leg_block(leg_dir: Path, manifest: dict, target: str) -> dict:
    tinfo = manifest["targets"][target]
    problems: list[str] = []
    steps: dict[str, str] = {}
    dl = _load(leg_dir / "download.json") or {}
    sha = dl.get("installed_wheel_sha256")
    if dl.get("verdict") != "pass" or sha != tinfo["wheel_sha256"]:
        problems.append(f"download: fetched wheel sha256 {sha} != manifest wheel_sha256 {tinfo['wheel_sha256']} (or download.json missing/failed)")
    a0 = _load(leg_dir / "a0.json")
    steps["A0"] = "pass" if a0 and a0.get("verdict") == "pass" else "fail"
    if steps["A0"] == "fail":
        problems.append("A0: " + ("; ".join(str(x) for x in (a0 or {}).get("problems", [])) or "a0.json missing/unreadable"))
    leg = _load(leg_dir / "leg.json") or {}
    for s in ("A1", "A2", "A3", "A4"):
        steps[s] = "pass" if (leg.get("steps") or {}).get(s) == "pass" else "fail"
    problems += [str(x) for x in leg.get("problems", [])]
    if not leg:
        problems.append("leg.json missing/unreadable (A1..A4 were not run)")
    smoke = _load(leg_dir / "smoke.json")
    block = {
        "wheel_tag": tinfo["wheel_tag"],
        "wheel_filename": dl.get("wheel_filename"),
        "installed_wheel_sha256": sha,
        "index_url": dl.get("index_url"),
        "topology": (a0 or {}).get("topology", "native-scrubbed-path"),
        "steps": {s: steps.get(s, "fail") for s in RTV_REQUIRED_STEPS},
        "installed_binary_sha256": leg.get("installed_binary_sha256"),
        "manifest_native_sha256": tinfo["native_sha256"],
        "a3_bits": leg.get("a3_bits"),
        "a4_counts": leg.get("a4_counts"),
        "problems": problems,
    }
    if smoke is not None:
        block[RTV_ALL_FEATURES_FIELD] = "pass" if smoke.get("verdict") == "pass" else "fail"
        block["all_features_detail"] = {k: smoke.get(k) for k in ("tests", "passed", "failed", "pythscribe_file") if k in smoke}
        if block[RTV_ALL_FEATURES_FIELD] == "fail":
            problems.append("all-features smoke failed: " + "; ".join(str(x) for x in smoke.get("problems", [])))
    else:
        problems.append("smoke.json missing: the all-features smoke did not run (the field is deliberately ABSENT; the consumer refuses it)")
    if leg.get("installed_binary_sha256") not in (None, tinfo["native_sha256"]):
        problems.append(f"A1: installed binary {leg.get('installed_binary_sha256')} != manifest native_sha256")
        block["steps"]["A1"] = "fail"
    return block


def assemble(manifest: dict, legs: Path, env: dict[str, str]) -> dict:
    targets: dict[str, dict] = {}
    for t in sorted(WHEEL_TARGETS):
        d = legs / t
        if not d.is_dir():
            targets[t] = {"steps": {s: "fail" for s in RTV_REQUIRED_STEPS}, "installed_wheel_sha256": None, "problems": [f"leg directory {d} missing"]}
            continue
        targets[t] = leg_block(d, manifest, t)
    ok = all(all(v == "pass" for v in b["steps"].values()) and b.get(RTV_ALL_FEATURES_FIELD) == "pass" and not b["problems"]
             and b.get("installed_wheel_sha256") == manifest["targets"][t]["wheel_sha256"] for t, b in targets.items())
    return {
        "record": "R-TV",
        "schema": RECORD_SCHEMA,
        "source_sha": env.get("GITHUB_SHA", ""),
        "manifest_sha256": manifest.get("manifest_sha256"),
        "producer": {
            "workflow": "publish-pypi.yml", "job": "testpypi-evidence",  # B4: the job that ASSEMBLES R-TV (not the validate matrix)
            "run_id": int(env.get("GITHUB_RUN_ID", "0") or 0), "run_attempt": int(env.get("GITHUB_RUN_ATTEMPT", "0") or 0),
            "head_sha": env.get("GITHUB_SHA", ""),
        },
        "version": manifest.get("version"),
        "targets": targets,
        "verdict": "pass" if ok else "fail",
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="assemble_rtv.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--manifest", required=True)
    ap.add_argument("--legs", required=True)
    ap.add_argument("--out", required=True)
    ns = ap.parse_args(argv)
    manifest = json.loads(Path(ns.manifest).read_text(encoding="utf-8"))
    rec = assemble(manifest, Path(ns.legs), dict(os.environ))
    out = Path(ns.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(rec, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"R-TV: {rec['verdict']} -> {out}")
    for t, b in rec["targets"].items():
        print(f"  {t}: steps={b['steps']} all_features={b.get(RTV_ALL_FEATURES_FIELD)}" + (f" problems={b['problems']}" if b.get("problems") else ""))
    return 0 if rec["verdict"] == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
