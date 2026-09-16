#!/usr/bin/env python3
"""Assemble the R-BA build-acceptance evidence record (spec 13-09-26, requirements §5.2; plan M4 A6).

    python scripts/assemble_rba.py --manifest release_manifest.json --legs legs/ --out evidence/R-BA.json

`legs/<triple>/` holds one leg's step outputs written by the node-free-acceptance matrix job:
  a0.json, bind.json (A0b), leg.json (A1..A4 from wheel_acceptance.py run), a4b.json (native legs only:
  native_node_boundary.py check; container legs record A4b as the pre-A5 filesystem re-check), a5.json
  (controller), a5b.json (post-A5 re-check). The per-target `node_free` verdict is computed from A0 +
  A4b + A5b ONLY (never the sampler; plan M4 step 4). The record's identity block binds the SAME run
  that produced the manifest (`require_evidence.py --role release-run` has already accepted it; its
  prerequisite set is recorded as `prerequisite_jobs_verified`). Verdict `pass` iff EVERY target of
  the release set is present and every required step passed; a missing leg is `fail` (a subset is a
  failure, never a pass with a smaller target set).
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
sys.path.insert(0, str(REPO / "scripts"))
from require_evidence import RBA_REQUIRED_STEPS, RECORD_SCHEMA, REQUIRED_PREREQ_JOBS, WHEEL_TARGETS  # noqa: E402

STEP_FILES = {"A0": "a0.json", "A0b": "bind.json", "A4b": "a4b.json", "A5": "a5.json", "A5b": "a5b.json"}


def _load(p: Path) -> dict | None:
    try:
        return json.loads(p.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def leg_block(leg_dir: Path, manifest: dict, target: str) -> dict:
    steps: dict[str, str] = {}
    problems: list[str] = []
    for step, fn in STEP_FILES.items():
        j = _load(leg_dir / fn)
        if j is None:
            steps[step] = "fail"
            problems.append(f"{step}: {fn} missing/unreadable (a skipped check is a failure)")
        else:
            steps[step] = "pass" if j.get("verdict") == "pass" else "fail"
            problems += [str(x) for x in j.get("problems", [])]
    leg = _load(leg_dir / "leg.json") or {}
    for step in ("A1", "A2", "A3", "A4"):
        steps[step] = "pass" if (leg.get("steps") or {}).get(step) == "pass" else "fail"
    problems += [str(x) for x in leg.get("problems", [])]
    node_free = all(steps.get(s) == "pass" for s in ("A0", "A4b", "A5b"))
    tinfo = manifest["targets"][target]
    block = {
        "wheel_tag": tinfo["wheel_tag"],
        "wheel_filename": tinfo["wheel_filename"],
        "wheel_sha256": (_load(leg_dir / "bind.json") or {}).get("wheel_sha256"),
        "topology": "container" if target.endswith("linux-gnu") else "native",
        "steps": {s: steps.get(s, "fail") for s in RBA_REQUIRED_STEPS},
        "node_free": node_free,
        "installed_binary_sha256": leg.get("installed_binary_sha256"),
        "record_sha256": leg.get("record_sha256"),
        "manifest_native_sha256": tinfo["native_sha256"],
        "compiler_version": leg.get("compiler_version"),
        "a3_bits": leg.get("a3_bits"),
        "a4_counts": leg.get("a4_counts"),
        "sampler_hits": (_load(leg_dir / "a5b.json") or {}).get("sampler_hits", []),
        "problems": problems,
    }
    if block["installed_binary_sha256"] != tinfo["native_sha256"] or block["record_sha256"] != tinfo["native_sha256"]:
        block["problems"].append(f"A1: installed/RECORD sha != manifest native_sha256[{target}]")
        block["steps"]["A1"] = "fail"
    return block


def assemble(manifest: dict, legs: Path, env: dict[str, str]) -> dict:
    targets: dict[str, dict] = {}
    for t in sorted(WHEEL_TARGETS):
        d = legs / t
        if not d.is_dir():
            targets[t] = {"steps": {s: "fail" for s in RBA_REQUIRED_STEPS}, "node_free": False, "problems": [f"leg directory {d} missing"]}
            continue
        targets[t] = leg_block(d, manifest, t)
    ok = all(b["node_free"] and all(v == "pass" for v in b["steps"].values()) and not b["problems"] for b in targets.values())
    return {
        "record": "R-BA",
        "schema": RECORD_SCHEMA,
        "source_sha": env.get("GITHUB_SHA", ""),
        "manifest_sha256": manifest.get("manifest_sha256"),
        "producer": {
            "workflow": "release.yml", "job": "node-free-evidence",  # B4: the job that ASSEMBLES R-BA (not the acceptance matrix)
            "run_id": int(env.get("GITHUB_RUN_ID", "0") or 0), "run_attempt": int(env.get("GITHUB_RUN_ATTEMPT", "0") or 0),
            "head_sha": env.get("GITHUB_SHA", ""),
        },
        "prerequisite_jobs_verified": sorted(REQUIRED_PREREQ_JOBS),
        "targets": targets,
        "verdict": "pass" if ok else "fail",
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="assemble_rba.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--manifest", required=True)
    ap.add_argument("--legs", required=True)
    ap.add_argument("--out", required=True)
    ns = ap.parse_args(argv)
    manifest = json.loads(Path(ns.manifest).read_text(encoding="utf-8"))
    rec = assemble(manifest, Path(ns.legs), dict(os.environ))
    out = Path(ns.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(rec, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"R-BA: {rec['verdict']} -> {out}")
    for t, b in rec["targets"].items():
        print(f"  {t}: node_free={b['node_free']} steps={b['steps']}" + (f" problems={b['problems']}" if b.get("problems") else ""))
    return 0 if rec["verdict"] == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
