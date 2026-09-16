#!/usr/bin/env python3
"""§D4 -- embedded identity: the compiler's `include_str!` runtime == the prepared payload, EXACT bytes.

    python scripts/verify_embedded_runtime.py --binary <pyths[.exe]> [--payload dist/runtime-payload]

`runtime_package_files()` (crates/pyths_runtime/src/lib.rs) embeds every file of E into the `pyths`
binary, and `pyths run` MATERIALIZES exactly those files as `<tmp>/node_modules/pyths-runtime/`
before spawning `node`. This script drives that very path: it runs `pyths run` on a trivial program
with `NODE_OPTIONS=--import=<capture.mjs>` (Node's own preload hook; `pyths run` inherits the
environment), the capture copies the materialized package out, and the result is compared to the
payload FILE-BY-FILE, EXACT BYTES: MISSING / EXTRA / CHANGED, each RED naming the file, plus the
membership check (materialized set == E). No substring search (codex M0 B1: a shortened file whose
bytes are a substring of the stale embed passed a `blob.find` check), no normalization (a CRLF embed
is CHANGED, with a hint). Needs `node` (T6: `pyths run` spawns node) -- this is the release/CI job's
gate, run where the binary was built from THIS checkout.
"""
from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from prepare_release_payloads import LIB_RS, NPM_ONLY, REPO, PayloadError, expected_runtime_membership  # noqa: E402

DEFAULT_PAYLOAD = REPO / "dist" / "runtime-payload"

_CAPTURE_MJS = """\
// D4 capture: runs BEFORE the program `pyths run` compiled (Node --import preload). argv[1] is the
// temp .mjs `pyths run` wrote; the runtime it materialized sits beside it.
import { cpSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
const src = join(dirname(process.argv[1]), "node_modules", "pyths-runtime");
if (!existsSync(src)) { console.error("D4 capture: no materialized pyths-runtime beside " + process.argv[1]); process.exit(97); }
cpSync(src, process.env.PYTHS_D4_CAPTURE_DIR, { recursive: true });
"""


def materialize_via_binary(binary: Path, workdir: Path) -> dict[str, bytes]:
    """`{package-root-relative path: bytes}` of the runtime package the binary materializes."""
    if not binary.is_file():
        raise PayloadError(f"{binary} is not a file")
    if not (shutil.which("node") or os.environ.get("PYTHS_NODE")):
        raise PayloadError("node not found on PATH (`pyths run` spawns node to materialize its runtime; set PYTHS_NODE or install Node)")
    workdir.mkdir(parents=True, exist_ok=True)
    capture = workdir / "capture.mjs"
    capture.write_text(_CAPTURE_MJS, encoding="utf-8", newline="\n")
    probe = workdir / "d4_probe.ps"
    probe.write_text('print("d4")\n', encoding="utf-8", newline="\n")
    out = workdir / "captured"
    if out.exists():
        shutil.rmtree(out)
    env = {**os.environ, "PYTHS_D4_CAPTURE_DIR": str(out), "NODE_OPTIONS": f"--import={capture.resolve().as_uri()}"}
    proc = subprocess.run([str(binary), "run", str(probe)], capture_output=True, text=True, check=False, cwd=str(workdir), env=env)
    if proc.returncode != 0 or not out.is_dir():
        raise PayloadError(f"`{binary} run` did not materialize its runtime (exit {proc.returncode}):\n{proc.stderr.strip() or proc.stdout.strip()}")
    return {p.relative_to(out).as_posix(): p.read_bytes() for p in sorted(out.rglob("*")) if p.is_file()}


def compare(materialized: dict[str, bytes], payload: dict[str, bytes], expected_e: set[str]) -> list[str]:
    problems: list[str] = []
    for rel in sorted(set(materialized) - expected_e):
        problems.append(f"EXTRA in the binary's runtime (not in runtime_package_files()): {rel}")
    for rel in sorted(expected_e - set(materialized)):
        problems.append(f"MISSING from the binary's runtime: {rel} (stale binary?)")
    for rel in sorted(expected_e - set(payload)):
        problems.append(f"payload lacks {rel}")
    for rel in sorted(expected_e & set(materialized) & set(payload)):
        a, b = payload[rel], materialized[rel]
        if a != b:
            hint = ""
            if b"\r" not in a and a.replace(b"\n", b"\r\n") == b:
                hint = " (the binary embeds a CRLF copy -- built from a non-LF checkout)"
            elif a.replace(b"\r\n", b"\n") == b.replace(b"\r\n", b"\n"):
                hint = " (line-ending-only difference)"
            problems.append(f"CHANGED bytes: {rel}{hint} (payload {len(a)} B, embedded {len(b)} B)")
    return problems


def verify(binary: Path, payload_dir: Path, lib_rs: Path = LIB_RS, workdir: Path | None = None) -> list[str]:
    expected_e = expected_runtime_membership(lib_rs) - set(NPM_ONLY)
    if not payload_dir.is_dir():
        raise PayloadError(f"{payload_dir} is not a directory (run scripts/prepare_release_payloads.py first?)")
    payload = {p.relative_to(payload_dir).as_posix(): p.read_bytes() for p in sorted(payload_dir.rglob("*")) if p.is_file()}
    tmp = None
    if workdir is None:
        tmp = tempfile.mkdtemp(prefix="pyths-d4-")
        workdir = Path(tmp)
    try:
        materialized = materialize_via_binary(binary, workdir)
    finally:
        if tmp:
            shutil.rmtree(tmp, ignore_errors=True)
    return compare(materialized, payload, expected_e)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--binary", required=True)
    ap.add_argument("--payload", default=str(DEFAULT_PAYLOAD))
    ns = ap.parse_args(argv)
    try:
        problems = verify(Path(ns.binary), Path(ns.payload))
    except PayloadError as e:
        print(f"verify_embedded_runtime: RED\n{e}", file=sys.stderr)
        return 1
    if problems:
        print(f"verify_embedded_runtime: RED -- the runtime embedded in {ns.binary} != {ns.payload} ({len(problems)} finding(s))", file=sys.stderr)
        for line in problems:
            print(f"  {line}", file=sys.stderr)
        return 1
    print(f"verify_embedded_runtime: GREEN -- the runtime {ns.binary} materializes == {ns.payload} exactly (every runtime_package_files() member, byte-for-byte)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
