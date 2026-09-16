#!/usr/bin/env python3
"""Generate third_party/ -- the per-target third-party licence inventory of the `pyths` compiler
binary the pip wheel bundles (spec 13-09-26, M5; requirements §5.7).

    python scripts/gen_third_party_notices.py            # (re)generate third_party/ from Cargo.lock
    python scripts/gen_third_party_notices.py --check    # RED unless a fresh generation == committed

Two INDEPENDENT sources feed it, so neither can silently drop a crate:

  * `cargo metadata --filter-platform <triple>` per release target (pythscribe/build/_native.py
    ::TRIPLE_TO_TAG, the ONE authority for the 5 targets) -> the NORMAL-dependency closure of
    `pyths_cli` minus the workspace crates = the crates linked into B_t for that target.
  * `cargo about generate --format json` (about.toml: the licence allowlist, dev/build deps
    ignored, the same 5 targets, no clearlydefined) -> the licence TEXTS, grouped by distinct text
    (an MIT text with a different copyright line is its own notice), each with its `used_by` crates.

Every crate in every target's closure MUST map to >= 1 notice text, or the generator exits RED --
a crate whose licence file cargo-about could not find is a missing notice, never a silent gap. The
output is deterministic (sorted, LF, no machine paths) so `--check` can diff it byte-for-byte:

  third_party/inventory.json          schema 1: cargo_lock_sha256 (LF-normalized), per-target crate
                                      lists with their notice files, every notice's sha256
  third_party/notices/NNN_<id>.txt    the licence texts, one file per distinct text
  third_party/THIRD_PARTY_NOTICES.md  the human-readable inventory (crate table + every text)
  third_party/README.md               what this directory is and how it is regenerated

The directory is COMMITTED and shipped by pyproject's `license-files` into every wheel's
`.dist-info/licenses/third_party/`; scripts/verify_wheel_license.py binds it to Cargo.lock (sha) and,
with --check-closure, to the live `cargo metadata` closure. Release-time requirement: cargo-about
(`cargo install cargo-about --locked --features cli`, pinned below) + `cargo fetch` before the
`--offline` generation; regenerate whenever Cargo.lock changes (the sha gate is RED until you do).
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
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from pythscribe.build._native import TRIPLE_TO_TAG  # noqa: E402

ROOT_CRATE = "pyths_cli"
OUT_DIR = ROOT / "third_party"
ABOUT_TOML = ROOT / "about.toml"
CARGO_ABOUT_VERSION = "0.9.2"  # the pinned generator; `cargo install cargo-about@0.9.2 --locked --features cli`
SCHEMA = 1
INSTALL_HINT = f"cargo install cargo-about@{CARGO_ABOUT_VERSION} --locked --features cli   # then: cargo fetch"
NOTICE_FILE = re.compile(r"^\d{3}_[A-Za-z0-9.+-]+\.txt$")


def sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def cargo_lock_sha256(lock: Path = ROOT / "Cargo.lock") -> str:
    """LF-normalized: a Windows autocrlf checkout and a Linux checkout hash identically."""
    return sha256_bytes(lock.read_bytes().replace(b"\r\n", b"\n"))


def _run(cmd: list[str], *, cwd: Path = ROOT) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, cwd=str(cwd), capture_output=True, text=True, encoding="utf-8", errors="replace", check=False)


def closure_for_target(triple: str) -> list[dict]:
    """The normal-dependency closure of ROOT_CRATE for `triple`, third-party only, sorted."""
    p = _run(["cargo", "metadata", "--format-version", "1", "--locked", "--offline", "--filter-platform", triple,
              "--manifest-path", str(ROOT / "Cargo.toml")])
    if p.returncode != 0:
        raise SystemExit(f"gen_third_party_notices: cargo metadata failed for {triple}:\n{p.stderr[-2000:]}")
    md = json.loads(p.stdout)
    packages = {pk["id"]: pk for pk in md["packages"]}
    workspace = set(md["workspace_members"])
    nodes = {n["id"]: n for n in md["resolve"]["nodes"]}
    roots = [pk["id"] for pk in md["packages"] if pk["name"] == ROOT_CRATE and pk["id"] in workspace]
    if len(roots) != 1:
        raise SystemExit(f"gen_third_party_notices: expected exactly one workspace {ROOT_CRATE}, found {roots}")
    seen: set[str] = set()
    stack = [roots[0]]
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):  # normal deps only (proc-macros included)
                stack.append(dep["pkg"])
    third = [packages[pid] for pid in seen if pid not in workspace]
    out = []
    for pk in third:
        if not pk.get("license") and not pk.get("license_file"):
            raise SystemExit(f"gen_third_party_notices: {pk['name']} {pk['version']} declares no licence (neither `license` nor `license-file`)")
        out.append({"name": pk["name"], "version": pk["version"], "license": pk.get("license") or f"(license-file: {pk['license_file']})",
                    "repository": pk.get("repository") or ""})
    return sorted(out, key=lambda c: (c["name"], c["version"]))


def cargo_about_available() -> str | None:
    p = _run(["cargo", "about", "--version"])
    if p.returncode != 0:
        return None
    return p.stdout.strip() or p.stderr.strip()


def cargo_about_licenses() -> list[dict]:
    """[{id, name, text, crates: [(name, version), ...]}] from cargo-about's JSON, texts LF-normalized."""
    ver = cargo_about_available()
    if ver is None:
        raise SystemExit(f"gen_third_party_notices: cargo-about is not installed (release-time requirement):\n  {INSTALL_HINT}")
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / "about.json"
        p = _run(["cargo", "about", "generate", "--format", "json", "-c", str(ABOUT_TOML), "-m",
                  str(ROOT / "crates" / ROOT_CRATE / "Cargo.toml"), "--offline", "--locked", "-o", str(out)])
        if p.returncode != 0 or not out.is_file():
            raise SystemExit(f"gen_third_party_notices: cargo about generate failed ({ver}); run `cargo fetch` first?\n{p.stderr[-3000:]}")
        j = json.loads(out.read_text(encoding="utf-8"))
    licenses = []
    for lic in j["licenses"]:
        text = lic["text"].replace("\r\n", "\n").rstrip() + "\n"
        crates = sorted({(u["crate"]["name"], u["crate"]["version"]) for u in lic["used_by"]})
        licenses.append({"id": lic["id"], "name": lic["name"], "text": text, "crates": crates})
    return licenses


def _slug(s: str) -> str:
    return re.sub(r"[^A-Za-z0-9.+-]+", "-", s)


def build_inventory(targets: dict[str, list[dict]], licenses: list[dict], lock_sha: str, about_version: str) -> tuple[dict, dict[str, str]]:
    """(inventory.json object, {notice file name: text}); RED if any closure crate has no notice."""
    ordered = sorted(licenses, key=lambda l: (l["id"], sha256_bytes(l["text"].encode("utf-8"))))
    files: dict[str, str] = {}
    notices: dict[str, dict] = {}
    crate_notices: dict[str, list[str]] = {}
    for i, lic in enumerate(ordered, 1):
        fname = f"{i:03d}_{_slug(lic['id'])}.txt"
        assert NOTICE_FILE.match(fname), fname
        files[fname] = lic["text"]
        keys = [f"{n}@{v}" for n, v in lic["crates"]]
        notices[fname] = {"license_id": lic["id"], "license_name": lic["name"], "sha256": sha256_bytes(lic["text"].encode("utf-8")), "crates": keys}
        for k in keys:
            crate_notices.setdefault(k, []).append(fname)
    missing = []
    inv_targets: dict[str, list[dict]] = {}
    for triple, crates in targets.items():
        rows = []
        for c in crates:
            key = f"{c['name']}@{c['version']}"
            n = crate_notices.get(key, [])
            if not n:
                missing.append(f"{triple}: {key} ({c['license']})")
            rows.append({**c, "notices": n})
        inv_targets[triple] = rows
    if missing:
        raise SystemExit("gen_third_party_notices: RED -- crates linked into the binary with NO licence notice text "
                         "(cargo-about found no licence file; fix the allowlist in about.toml or add a clarification):\n  " + "\n  ".join(missing))
    inventory = {
        "schema": SCHEMA,
        "generator": "scripts/gen_third_party_notices.py",
        "cargo_about": about_version,
        "root_crate": ROOT_CRATE,
        "cargo_lock_sha256": lock_sha,
        "wheel_tags": dict(TRIPLE_TO_TAG),
        "targets": inv_targets,
        "notices": notices,
    }
    return inventory, files


README = """# third_party/ -- licence notices of the bundled `pyths` compiler binary

GENERATED by `scripts/gen_third_party_notices.py` from `Cargo.lock` (via `cargo metadata` per release
target + `cargo about`, config `about.toml`). Do not edit by hand; regenerate whenever `Cargo.lock`
changes -- `inventory.json` records the (LF-normalized) sha256 of the `Cargo.lock` it was generated
from and `scripts/verify_wheel_license.py` / `tests/pythscribe/test_wheel_m5.py` are RED until the
two agree.

This directory ships in EVERY `pythscribe` wheel at `.dist-info/licenses/third_party/` (pyproject
`license-files`). The pip package itself is MIT; the bundled compiler `pythscribe/_bin/pyths[.exe]`
is FSL-1.1-ALv2 and statically links the crates inventoried here under their own licences.

* `inventory.json` -- per release target, every third-party crate linked into the binary (name,
  version, declared licence expression, repository) and the notice file(s) covering it; every
  notice file's sha256.
* `notices/NNN_<licence-id>.txt` -- the licence texts, one file per DISTINCT text (an MIT text with
  its own copyright line is its own notice).
* `THIRD_PARTY_NOTICES.md` -- the same inventory rendered for humans.

Regenerate: `cargo install cargo-about@{ver} --locked --features cli && cargo fetch &&
python scripts/gen_third_party_notices.py`; check freshness without writing: `--check`.
""".replace("{ver}", CARGO_ABOUT_VERSION)


def render_markdown(inv: dict, files: dict[str, str]) -> str:
    tags = inv["wheel_tags"]
    union: dict[str, dict] = {}
    for triple, rows in inv["targets"].items():
        for r in rows:
            u = union.setdefault(f"{r['name']}@{r['version']}", {**r, "targets": []})
            u["targets"].append(tags[triple])
    lines = ["# Third-party notices -- the `pyths` compiler binary bundled in the `pythscribe` wheel", "",
             "GENERATED by `scripts/gen_third_party_notices.py`; do not edit. The crates below are statically linked",
             "into `pythscribe/_bin/pyths[.exe]` (per release target). Each is used under the licence named in its",
             "`licence` column; the corresponding texts follow. The `pythscribe` Python package is MIT; the compiler",
             "binary itself is FSL-1.1-ALv2 (`LICENSE.md`); see `pythscribe/LICENSES-MAP.md`.", "",
             f"Generated from `Cargo.lock` sha256 (LF) `{inv['cargo_lock_sha256']}` with cargo-about {inv['cargo_about']}.", "",
             "## Crates", "", "| crate | version | licence (declared) | wheels | notice file(s) |", "|---|---|---|---|---|"]
    for key in sorted(union):
        u = union[key]
        lines.append(f"| {u['name']} | {u['version']} | {u['license']} | {', '.join(sorted(u['targets']))} | {', '.join(u['notices'])} |")
    lines += ["", "## Notices", ""]
    for fname in sorted(files):
        n = inv["notices"][fname]
        lines += [f"### {fname} -- {n['license_name']} ({n['license_id']})", "",
                  "Used by: " + ", ".join(n["crates"]), "", "```text", files[fname].rstrip("\n"), "```", ""]
    return "\n".join(lines) + "\n"


def generate(out_dir: Path) -> dict:
    lock_sha = cargo_lock_sha256()
    about_version = cargo_about_available()
    if about_version is None:
        raise SystemExit(f"gen_third_party_notices: cargo-about is not installed (release-time requirement):\n  {INSTALL_HINT}")
    targets = {triple: closure_for_target(triple) for triple in TRIPLE_TO_TAG}
    inv, files = build_inventory(targets, cargo_about_licenses(), lock_sha, about_version)
    if out_dir.exists():
        shutil.rmtree(out_dir)
    (out_dir / "notices").mkdir(parents=True)
    for fname, text in files.items():
        (out_dir / "notices" / fname).write_bytes(text.encode("utf-8"))
    (out_dir / "inventory.json").write_bytes((json.dumps(inv, indent=2, sort_keys=False) + "\n").encode("utf-8"))
    (out_dir / "THIRD_PARTY_NOTICES.md").write_bytes(render_markdown(inv, files).encode("utf-8"))
    (out_dir / "README.md").write_bytes(README.encode("utf-8"))
    return inv


def tree(d: Path) -> dict[str, bytes]:
    return {p.relative_to(d).as_posix(): p.read_bytes() for p in sorted(d.rglob("*")) if p.is_file()}


def check(out_dir: Path) -> list[str]:
    """RED lines: a fresh generation differs from the committed directory."""
    with tempfile.TemporaryDirectory() as td:
        fresh = Path(td) / "third_party"
        generate(fresh)
        want, have = tree(fresh), tree(out_dir) if out_dir.is_dir() else {}
    problems = [f"missing in {out_dir.name}/: {k}" for k in want if k not in have]
    problems += [f"stale extra in {out_dir.name}/: {k}" for k in have if k not in want]
    problems += [f"differs: {k}" for k in want if k in have and want[k] != have[k]]
    return problems


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--out", default=str(OUT_DIR), help="output directory (default: third_party/)")
    ap.add_argument("--check", action="store_true", help="regenerate to a temp dir and diff against --out; RED on any difference")
    ns = ap.parse_args(argv)
    out = Path(ns.out)
    if ns.check:
        problems = check(out)
        if problems:
            print(f"gen_third_party_notices --check: RED ({len(problems)}):", file=sys.stderr)
            for p in problems:
                print(f"  {p}", file=sys.stderr)
            return 1
        print(f"gen_third_party_notices --check: OK -> {out} is a fresh generation of Cargo.lock {cargo_lock_sha256()[:12]}")
        return 0
    inv = generate(out)
    n_crates = {t: len(rows) for t, rows in inv["targets"].items()}
    print(f"gen_third_party_notices: wrote {out} -> {len(inv['notices'])} notice texts; crates per target {n_crates}; "
          f"Cargo.lock {inv['cargo_lock_sha256'][:12]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
