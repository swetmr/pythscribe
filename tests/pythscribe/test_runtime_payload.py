"""M0 gates (spec 13-09-26-lib-selfcontained-pip-wheel; validation §C + §D) -- every gate ships its
PAIRED NEGATIVE CONTROL (the anti-vacuity paired-control convention: a control that goes RED when the property is violated).

  C1  find_runtime_dir() resolves the VENDORED in-package payload (no repo runtime checkout);
      build_module produces a non-empty pyths-runtime/ closure passing _check_artifact_specifiers.
      Control: a file deleted from the vendored closure -> `runtime import target missing`.
  C2  prepare_release_payloads: tarball membership == E(70) + {LICENSE} EXACTLY.
      Controls: unregistered src/newmod.js -> RED (extra); .test.mjs leaking past the files
      allowlist -> RED (extra); asyncio.js dropped from `files` -> RED (missing).
  C3  strip_trailing_sourcemap_comment: sound (last line only). Control: the old MULTILINE regex
      normalizes the A/B fixtures identically (the reproduced codex failure), the new one does not.
  C5  LF gate: one CRLF file in the payload -> RED.
  D1  verify_runtime_mirror: vendored == payload raw bytes. Controls: one-byte edit / missing /
      extra / CRLF-only -> RED naming the file.
  D2  crates/pyths_runtime/js/{runtime,operators}.js are re-export shims (linkage), not byte-compared.
  D4  verify_embedded_runtime: the runtime the compiler MATERIALIZES (`pyths run` path) == the
      payload, exact bytes file-by-file. Runs where a `pyths` built from THIS checkout exists
      (PYTHSCRIBE_EMBED_BINARY; ci.yml sets it + PYTHSCRIBE_REQUIRE_EMBED=1 after cargo build),
      plain skip elsewhere. Controls: insertion / SHORTENING / CRLF / stale-binary -> RED.
  SF1 npm/payload-check.mjs: the .tgz publish.mjs hands to npm == the prepared payload (member set,
      per-file sha256, name@version). Control: a stale same-version tarball -> RED.
  W   the built wheel vendors exactly E + {LICENSE} at pythscribe/_runtime/pyths-runtime/, raw bytes.
npm-needing gates (C2/C5, the real payload) gate on npm; under PYTHSCRIBE_REQUIRE_ORACLE they fail.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

import pytest

from conftest import DEMO_DIR, REPO, gate
from pythscribe.build import (
    BuildError,
    RUNTIME_ENV,
    _check_artifact_specifiers,
    _copy_runtime,
    build_module,
    find_pyths,
    find_runtime_dir,
    strip_trailing_sourcemap_comment,
)
import pythscribe.build as build_mod

SCRIPTS = REPO / "scripts"
sys.path.insert(0, str(SCRIPTS))
import prepare_release_payloads as prep  # noqa: E402
import verify_runtime_mirror as mirror  # noqa: E402
import verify_embedded_runtime as embed  # noqa: E402

VENDORED = REPO / "pythscribe" / "_runtime" / "pyths-runtime"
RUNTIME = REPO / "runtime"
E = prep.embedded_membership()
EXPECTED = set(E) | {"LICENSE"}
_NPM = shutil.which("npm") or shutil.which("npm.cmd")


def _sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def _tree(root: Path) -> dict[str, bytes]:
    return {p.relative_to(root).as_posix(): p.read_bytes() for p in sorted(root.rglob("*")) if p.is_file()}


# ----------------------------------------------------------------------------- the authority E


def test_e_is_the_70_file_authority_with_34_maps():
    """rev-8 pin: E = runtime_package_files() = 70 paths incl. all 34 .js.map (requirements §2).
    Update this pin ONLY together with crates/pyths_runtime/src/lib.rs (a new entrypoint)."""
    assert len(E) == 70, (len(E), E)
    maps = [p for p in E if p.endswith(".js.map")]
    js = [p for p in E if p.endswith(".js")]
    assert len(maps) == 34 and len(js) == 34 and {f"{p}.map" for p in js} == set(maps)
    assert {"package.json", "README.md", "asyncio.js", "src/index.js", "src/core.js"} <= set(E)
    # every exports-map target of runtime/package.json is in E (the #177 class, from the Python side)
    pkg = json.loads((RUNTIME / "package.json").read_text(encoding="utf-8"))
    for target in pkg["exports"].values():
        assert target.startswith("./") and target[2:] in E, target
    assert "LICENSE" not in E and prep.NPM_ONLY == frozenset({"LICENSE"})


# ----------------------------------------------------------------------------- C1 vendored resolution


def test_c1_find_runtime_dir_returns_the_vendored_in_package_path(monkeypatch, tmp_path):
    """No env override, NO repo runtime checkout reachable (repo root + the compiler's parents point
    at nonsense) -> the in-package vendored payload is what resolves."""
    monkeypatch.delenv(RUNTIME_ENV, raising=False)
    monkeypatch.setattr(build_mod, "_REPO_ROOT", tmp_path / "no-such-repo")
    bogus_pyths = tmp_path / "elsewhere" / "target" / "release" / "pyths.exe"
    got = find_runtime_dir(bogus_pyths)
    assert got == VENDORED / "src", got
    assert (got / "index.js").is_file() and (got / "core.js.map").is_file() and (VENDORED / "LICENSE").is_file()
    assert find_runtime_dir() == VENDORED / "src"  # pyths is optional now
    # ORDER witness: with the REAL repo checkout reachable (this worktree has runtime/src/index.js)
    # the vendored copy still wins over the external dev fallback (a resolver that prefers the
    # checkout would return <repo>/runtime/src here -> RED)
    monkeypatch.setattr(build_mod, "_REPO_ROOT", REPO)
    assert (REPO / "runtime" / "src" / "index.js").is_file()
    assert find_runtime_dir(REPO / "target" / "release" / "pyths.exe") == VENDORED / "src"
    # the env override still wins (first in the order)
    monkeypatch.setenv(RUNTIME_ENV, str(RUNTIME / "src"))
    assert find_runtime_dir(bogus_pyths) == RUNTIME / "src"


def test_c1_control_vendored_copy_absent_falls_back_or_names_the_gap(monkeypatch, tmp_path):
    """Vendored copy missing AND no dev checkout -> the error names the vendored path (never a
    silent wrong directory)."""
    monkeypatch.delenv(RUNTIME_ENV, raising=False)
    monkeypatch.setattr(build_mod, "_VENDORED_RUNTIME", tmp_path / "gone" / "pyths-runtime")
    monkeypatch.setattr(build_mod, "_REPO_ROOT", tmp_path / "no-such-repo")
    with pytest.raises(BuildError, match="vendored copy .* is absent"):
        find_runtime_dir(tmp_path / "x" / "y" / "z" / "pyths")


def test_c1_control_deleted_vendored_file_is_a_missing_import_target(tmp_path):
    """Delete types.js from a copy of the vendored closure -> `runtime import target missing`."""
    src = tmp_path / "vendored-src"
    shutil.copytree(VENDORED / "src", src)
    (src / "types.js").unlink()
    with pytest.raises(BuildError, match=r"runtime import target missing: types\.js"):
        _copy_runtime(src, tmp_path / "out", ["./pyths-runtime/index.js"])


@pytest.fixture(scope="module")
def pyths():
    try:
        return find_pyths()
    except BuildError as e:
        gate(False, str(e))


def test_c1_spot_build_module_from_the_vendored_payload(tmp_path, monkeypatch, pyths):
    """EXIT witness: with PYTHSCRIBE_PYTHS at the dev binary and the resolver unable to see a repo
    runtime checkout, build_module on a kernel whose glue imports a runtime helper produces a
    non-empty pyths-runtime/ closure, copied from the VENDORED bytes, passing
    _check_artifact_specifiers, with no dangling sourcemap comment."""
    monkeypatch.delenv(RUNTIME_ENV, raising=False)
    monkeypatch.setattr(build_mod, "_REPO_ROOT", tmp_path / "no-such-repo")
    src = tmp_path / "kernels.py"
    src.write_text((DEMO_DIR / "kernels.py").read_text(encoding="utf-8"), encoding="utf-8")
    [a] = build_module(src, quiet=True)
    assert a.manifest["runtime_dir"] == "pyths-runtime"
    copied = sorted(p for p in (a.dir / "pyths-runtime").rglob("*") if p.is_file())
    assert copied and any(p.name == "index.js" for p in copied)
    for p in copied:
        rel = p.relative_to(a.dir / "pyths-runtime")
        vend = (VENDORED / "src" / rel).read_text(encoding="utf-8")
        assert p.read_text(encoding="utf-8") == strip_trailing_sourcemap_comment(vend, rel.name).rstrip("\n") + "\n", rel
        assert "sourceMappingURL" not in p.read_text(encoding="utf-8"), rel
        assert b"\r" not in p.read_bytes(), rel
    _check_artifact_specifiers(a.dir, [a.entry, a.dir / a.manifest["glue"], *copied])  # explicit: passes


def test_c1_control_build_module_red_when_a_vendored_closure_file_is_missing(tmp_path, monkeypatch, pyths):
    monkeypatch.delenv(RUNTIME_ENV, raising=False)
    monkeypatch.setattr(build_mod, "_REPO_ROOT", tmp_path / "no-such-repo")
    broken = tmp_path / "vendored"
    shutil.copytree(VENDORED, broken)
    (broken / "src" / "types.js").unlink()
    monkeypatch.setattr(build_mod, "_VENDORED_RUNTIME", broken)
    src = tmp_path / "kernels.py"
    src.write_text((DEMO_DIR / "kernels.py").read_text(encoding="utf-8"), encoding="utf-8")
    with pytest.raises(BuildError, match=r"runtime import target missing: types\.js"):
        build_module(src, quiet=True)
    assert not (tmp_path / "__pythscribe__" / "rms_gain" / "manifest.json").exists()  # no half-built dir


# ----------------------------------------------------------------------------- C2 / C5 prepared payload


def _need_npm():
    gate(bool(_NPM), "npm is required to run `npm pack` (prepare_release_payloads is a release-machine step)")


@pytest.fixture(scope="module")
def prepared(tmp_path_factory) -> Path:
    """The REAL prepared payload of this checkout (npm pack), in a temp dist/."""
    _need_npm()
    out = tmp_path_factory.mktemp("dist")
    prep.prepare_runtime_payload(out)
    return out


def _runtime_copy(tmp_path: Path) -> Path:
    dst = tmp_path / "runtime"
    shutil.copytree(RUNTIME, dst, ignore=shutil.ignore_patterns("node_modules"))
    return dst


def test_c2_prepared_payload_membership_is_exactly_e_plus_license(prepared):
    payload = _tree(prepared / "runtime-payload")
    assert set(payload) == EXPECTED, (sorted(set(payload) - EXPECTED), sorted(EXPECTED - set(payload)))
    assert (prepared / "runtime-payload.tgz").is_file()
    fj = json.loads((prepared / "runtime-payload.files.json").read_text(encoding="utf-8"))
    assert fj == {rel: _sha(b) for rel, b in payload.items()}
    # byte-identical to the checkout + LF (C5 positive)
    for rel, b in payload.items():
        assert b == (RUNTIME / rel).read_bytes(), rel
        assert b"\r" not in b, rel


def test_c2_control_unregistered_src_module_is_extra(tmp_path):
    _need_npm()
    rt = _runtime_copy(tmp_path)
    (rt / "src" / "newmod.js").write_text("export const x = 1;\n", encoding="utf-8", newline="\n")
    with pytest.raises(prep.PayloadError, match=r"EXTRA .*src/newmod\.js"):
        prep.prepare_runtime_payload(tmp_path / "dist", runtime_dir=rt)


def test_c2_control_test_files_leaking_past_the_files_allowlist_are_extra(tmp_path):
    """The pre-M0 `files: ["src/"]` shipped all 23 src/**/*.test.mjs (an .npmignore pattern does NOT
    filter inside a `files` directory) -- restoring it must be RED (extra)."""
    _need_npm()
    rt = _runtime_copy(tmp_path)
    pj = rt / "package.json"
    j = json.loads(pj.read_text(encoding="utf-8"))
    j["files"] = ["src/", "asyncio.js", "asyncio.js.map", "README.md", "LICENSE"]
    pj.write_text(json.dumps(j, indent=2) + "\n", encoding="utf-8", newline="\n")
    with pytest.raises(prep.PayloadError, match=r"EXTRA .*\.test\.mjs"):
        prep.prepare_runtime_payload(tmp_path / "dist", runtime_dir=rt)


def test_c2_control_dropped_entrypoint_is_missing(tmp_path):
    _need_npm()
    rt = _runtime_copy(tmp_path)
    pj = rt / "package.json"
    j = json.loads(pj.read_text(encoding="utf-8"))
    j["files"] = [f for f in j["files"] if f != "asyncio.js"]
    pj.write_text(json.dumps(j, indent=2) + "\n", encoding="utf-8", newline="\n")
    with pytest.raises(prep.PayloadError, match=r"MISSING .*'asyncio\.js'"):
        prep.prepare_runtime_payload(tmp_path / "dist", runtime_dir=rt)


def test_c5_control_one_crlf_file_is_red(tmp_path):
    _need_npm()
    rt = _runtime_copy(tmp_path)
    p = rt / "src" / "stdlib" / "heapq.js"
    p.write_bytes(p.read_bytes().replace(b"\n", b"\r\n"))
    with pytest.raises(prep.PayloadError, match=r"src/stdlib/heapq\.js is not LF"):
        prep.prepare_runtime_payload(tmp_path / "dist", runtime_dir=rt)


def test_c2_membership_parser_control_unregistered_include_path_mismatch(tmp_path):
    """The E parser refuses a lib.rs whose tuple path != include path (a silently mis-keyed embed)."""
    lib = tmp_path / "lib.rs"
    text = prep.LIB_RS.read_text(encoding="utf-8").replace(
        '("src/core.js", include_str!("../../../runtime/src/core.js"))',
        '("src/core2.js", include_str!("../../../runtime/src/core.js"))',
    )
    assert text != prep.LIB_RS.read_text(encoding="utf-8")
    lib.write_text(text, encoding="utf-8", newline="\n")
    with pytest.raises(prep.PayloadError, match="embedded path 'src/core2.js' != include path"):
        prep.embedded_membership(lib)


# ----------------------------------------------------------------------------- C3 sound transform

_OLD_SOURCEMAP = re.compile(r"^\s*//[#@]\s*sourceMappingURL=.*$", re.MULTILINE)  # the pre-M0 regex (unsound)


def _probe(marker: str) -> str:
    # the original codex reproducer: the directive on ITS OWN LINE inside a template literal
    return "export const s = `\n//# sourceMappingURL=" + marker + "\n`;\n//# sourceMappingURL=probe.js.map\n"


def _fixture_runtime(tmp_path: Path, marker: str) -> Path:
    src = tmp_path / f"rt-{marker}"
    src.mkdir()
    (src / "index.js").write_text('export * from "./probe.js";\n//# sourceMappingURL=index.js.map\n', encoding="utf-8", newline="\n")
    (src / "probe.js").write_text(_probe(marker), encoding="utf-8", newline="\n")
    return src


def test_c3_strip_is_last_line_only_and_touches_nothing_else():
    assert strip_trailing_sourcemap_comment("a;\n//# sourceMappingURL=core.js.map\n", "core.js") == "a;\n"
    assert strip_trailing_sourcemap_comment("a;\n//# sourceMappingURL=core.js.map", "core.js") == "a;\n"
    assert strip_trailing_sourcemap_comment("a;\n\n//# sourceMappingURL=core.js.map\n", "core.js") == "a;\n\n"  # preceding blank line kept
    assert strip_trailing_sourcemap_comment("//# sourceMappingURL=core.js.map\n", "core.js") == ""
    assert strip_trailing_sourcemap_comment("//# sourceMappingURL=core.js.map", "core.js") == ""
    # wrong name / wrong position / different syntax -> byte-for-byte untouched
    for text in (
        "a;\n//# sourceMappingURL=other.js.map\n",
        "//# sourceMappingURL=core.js.map\na;\n",
        "a;\n//# sourceMappingURL=x\nb;\n",
        "a;\n//@ sourceMappingURL=core.js.map\n",
        "a;\n// # sourceMappingURL=core.js.map\n",
        "a;\n  //# sourceMappingURL=core.js.map\nb;\n",
        "a;//# sourceMappingURL=core.js.map\n",  # not on its own line
    ):
        assert strip_trailing_sourcemap_comment(text, "core.js") == text, text


def test_c3_control_literal_match_no_whitespace_slack():
    """codex M0 SF2: the contract is LITERAL. A directive with trailing spaces, an indented one, one
    followed by a blank line, or two terminal newlines is NOT the exact last line -> preserved
    byte-for-byte (RED if stripped). The mutation that re-introduces rstrip()/strip() slack fails here."""
    for text in (
        "a;\n//# sourceMappingURL=core.js.map   \n",   # trailing spaces on the directive
        "a;\n//# sourceMappingURL=core.js.map\n\n",    # blank line after the directive
        "a;\n//# sourceMappingURL=core.js.map   \n\n",  # both (the review's example)
        "a;\n  //# sourceMappingURL=core.js.map\n",    # indented
        "a;\n//# sourceMappingURL=core.js.map\n \n",   # whitespace-only line after
        "a;\r\n//# sourceMappingURL=core.js.map\r\n",  # CRLF is not the LF contract (the copy reads text mode, so this is defensive)
    ):
        assert strip_trailing_sourcemap_comment(text, "core.js") == text, repr(text)


def test_c3_in_string_directive_survives_and_is_significant(tmp_path):
    outs = {}
    for marker in ("A", "B"):
        src = _fixture_runtime(tmp_path, marker)
        _copy_runtime(src, tmp_path / f"out-{marker}", ["./pyths-runtime/index.js"])
        outs[marker] = (tmp_path / f"out-{marker}" / "probe.js").read_bytes()
        # trailing comment gone, the template literal (incl. its directive line) byte-intact
        assert outs[marker] == ("export const s = `\n//# sourceMappingURL=" + marker + "\n`;\n").encode(), outs[marker]
    assert _sha(outs["A"]) != _sha(outs["B"])  # A->B changes the copied file's sha (manifest differs)


def test_c3_mid_file_directive_outside_strings_is_left_untouched(tmp_path):
    src = tmp_path / "rt"
    src.mkdir()
    (src / "index.js").write_text("a;\n//# sourceMappingURL=x\nb;\n//# sourceMappingURL=index.js.map\n", encoding="utf-8", newline="\n")
    _copy_runtime(src, tmp_path / "out", ["./pyths-runtime/index.js"])
    assert (tmp_path / "out" / "index.js").read_bytes() == b"a;\n//# sourceMappingURL=x\nb;\n"


def test_c3_control_the_old_multiline_regex_is_unsound_on_this_fixture():
    """The reproduced failure: the old regex deletes the in-string line, so A and B normalize
    IDENTICALLY (sha-equal) -- the fixture discriminates, and the new transform keeps them apart.
    (Mutation-verified live too: restoring `_SOURCEMAP.sub` in _copy_runtime makes
    test_c3_in_string_directive_survives_and_is_significant RED.)"""
    old = {m: _OLD_SOURCEMAP.sub("", _probe(m)).rstrip("\n") + "\n" for m in ("A", "B")}
    assert old["A"] == old["B"] and "sourceMappingURL" not in old["A"]
    new = {m: strip_trailing_sourcemap_comment(_probe(m), "probe.js").rstrip("\n") + "\n" for m in ("A", "B")}
    assert new["A"] != new["B"] and "//# sourceMappingURL=A" in new["A"]


def test_c3_real_runtime_files_copy_identically_under_old_and_new_transform():
    """Regression fence for the committed artifacts: on every REAL runtime .js (one trailing
    directive, nothing else) the sound strip yields the same bytes the old regex did."""
    for rel in E:
        if not rel.endswith(".js"):
            continue
        text = (VENDORED / rel).read_text(encoding="utf-8")
        assert text.rstrip().rsplit("\n", 1)[-1] == f"//# sourceMappingURL={Path(rel).name}.map", rel  # runtime_maps.rs invariant
        assert strip_trailing_sourcemap_comment(text, Path(rel).name).rstrip("\n") + "\n" == _OLD_SOURCEMAP.sub("", text).rstrip("\n") + "\n", rel


# ----------------------------------------------------------------------------- SF1 the PUBLISHED tarball is the payload


def _payload_check(tgz: Path, files_json: Path, name: str, version: str) -> subprocess.CompletedProcess:
    node = shutil.which("node")
    gate(bool(node), "node is required for npm/payload-check.mjs")
    return subprocess.run([node, str(REPO / "npm" / "payload-check.mjs"), str(tgz), str(files_json), name, version], capture_output=True, text=True, cwd=str(REPO))


def _repack(payload_dir: Path, tgz: Path) -> None:
    import tarfile

    with tarfile.open(tgz, "w:gz") as tf:
        for p in sorted(payload_dir.rglob("*")):
            if p.is_file():
                tf.add(p, arcname="package/" + p.relative_to(payload_dir).as_posix())


def test_sf1_publish_verifies_the_tarball_it_publishes(prepared):
    """publish.mjs hands `dist/runtime-payload.tgz` to npm; payload-check.mjs verifies THAT file's
    member set + per-file sha256 == runtime-payload.files.json and name@version -- GREEN on the
    real prepared tarball."""
    pkg = json.loads((prepared / "runtime-payload" / "package.json").read_text(encoding="utf-8"))
    r = _payload_check(prepared / "runtime-payload.tgz", prepared / "runtime-payload.files.json", "pyths-runtime", pkg["version"])
    assert r.returncode == 0 and "GREEN" in r.stdout, (r.stdout, r.stderr)
    # publish.mjs is wired to it (the check runs BEFORE any npm publish; no other path to the tgz)
    src = (REPO / "npm" / "publish.mjs").read_text(encoding="utf-8")
    assert 'import { verifyPreparedTarball } from "./payload-check.mjs"' in src
    assert src.index("verifyPreparedTarball(tgz, files, name, VERSION)") < src.index('execFileSync("npm", pubArgs')


def test_sf1_control_stale_same_version_tarball_is_red(prepared, tmp_path):
    """A .tgz that carries the right name@version but different bytes for one file (a stale pack
    beside a fresh extracted dir) -> RED naming the file; a tgz missing / adding a member -> RED."""
    pkg = json.loads((prepared / "runtime-payload" / "package.json").read_text(encoding="utf-8"))
    fj = prepared / "runtime-payload.files.json"
    stale = tmp_path / "stale"
    shutil.copytree(prepared / "runtime-payload", stale)
    p = stale / "src" / "core.js"
    p.write_bytes(p.read_bytes() + b"// stale\n")
    _repack(stale, tmp_path / "stale.tgz")
    r = _payload_check(tmp_path / "stale.tgz", fj, "pyths-runtime", pkg["version"])
    assert r.returncode == 1 and "CHANGED bytes: src/core.js" in r.stderr and "RED" in r.stderr, (r.stdout, r.stderr)
    (stale / "src" / "core.js").write_bytes((prepared / "runtime-payload" / "src" / "core.js").read_bytes())
    (stale / "src" / "types.js").unlink()
    (stale / "src" / "rogue.test.mjs").write_text("", encoding="utf-8")
    _repack(stale, tmp_path / "member.tgz")
    r = _payload_check(tmp_path / "member.tgz", fj, "pyths-runtime", pkg["version"])
    assert r.returncode == 1 and "MISSING from the tarball: src/types.js" in r.stderr and "EXTRA in the tarball (not in the prepared payload): src/rogue.test.mjs" in r.stderr, r.stderr
    # wrong version / wrong name on an otherwise identical tarball -> RED
    r = _payload_check(prepared / "runtime-payload.tgz", fj, "pyths-runtime", "9.9.9")
    assert r.returncode == 1 and "expected pyths-runtime@9.9.9" in r.stderr, r.stderr
    r = _payload_check(prepared / "runtime-payload.tgz", fj, "not-the-runtime", pkg["version"])
    assert r.returncode == 1, r.stderr


# ----------------------------------------------------------------------------- D1 mirror gate


@pytest.fixture(scope="module")
def payload_dir(tmp_path_factory) -> Path:
    """A payload to mirror-gate against: the REAL npm-packed one when npm is present, else the
    checkout's E + {LICENSE} bytes (identical by C2's identity assertion; the mirror gate's
    property is vendored == payload RAW BYTES, which this still exercises)."""
    if _NPM:
        out = tmp_path_factory.mktemp("dist")
        prep.prepare_runtime_payload(out)
        return out / "runtime-payload"
    out = tmp_path_factory.mktemp("payload-from-checkout")
    for rel in EXPECTED:
        (out / rel).parent.mkdir(parents=True, exist_ok=True)
        (out / rel).write_bytes((RUNTIME / rel).read_bytes())
    return out


def _vendored_copy(tmp_path: Path) -> Path:
    dst = tmp_path / "vendored"
    shutil.copytree(VENDORED, dst)
    return dst


def test_d1_vendored_copy_equals_payload_raw_bytes(payload_dir):
    assert mirror.verify(payload_dir, VENDORED) == []
    assert set(_tree(VENDORED)) == EXPECTED
    # CLI form: exit 0 + GREEN line
    p = subprocess.run([sys.executable, str(SCRIPTS / "verify_runtime_mirror.py"), "--payload", str(payload_dir)], capture_output=True, text=True, cwd=str(REPO))
    assert p.returncode == 0 and "GREEN" in p.stdout, (p.stdout, p.stderr)


def test_d1_control_one_byte_edit_is_red_naming_the_file(payload_dir, tmp_path):
    v = _vendored_copy(tmp_path)
    p = v / "src" / "stdlib" / "math.js"
    b = bytearray(p.read_bytes())
    b[len(b) // 2] ^= 0x01
    p.write_bytes(bytes(b))
    problems = mirror.verify(payload_dir, v)
    assert problems and all("src/stdlib/math.js" in x and x.startswith("CHANGED") for x in problems), problems
    r = subprocess.run([sys.executable, str(SCRIPTS / "verify_runtime_mirror.py"), "--payload", str(payload_dir), "--vendored", str(v)], capture_output=True, text=True, cwd=str(REPO))
    assert r.returncode == 1 and "src/stdlib/math.js" in r.stderr and "RED" in r.stderr


def test_d1_control_missing_vendored_file_is_red(payload_dir, tmp_path):
    v = _vendored_copy(tmp_path)
    (v / "src" / "web" / "router.js.map").unlink()
    problems = mirror.verify(payload_dir, v)
    assert any(x.startswith("MISSING") and "src/web/router.js.map" in x for x in problems), problems


def test_d1_control_extra_vendored_file_is_red(payload_dir, tmp_path):
    v = _vendored_copy(tmp_path)
    (v / "src" / "extra.js").write_text("export {};\n", encoding="utf-8", newline="\n")
    problems = mirror.verify(payload_dir, v)
    assert any(x.startswith("EXTRA") and "src/extra.js" in x for x in problems), problems


def test_d1_control_crlf_only_difference_is_red(payload_dir, tmp_path):
    """Replaces rev-2's 'normalization noise stays GREEN': there is no normalization, so a
    line-ending-only difference is drift and is RED (named, with the CRLF hint)."""
    v = _vendored_copy(tmp_path)
    p = v / "src" / "core.js"
    p.write_bytes(p.read_bytes().replace(b"\n", b"\r\n"))
    problems = mirror.verify(payload_dir, v)
    assert len(problems) == 1 and problems[0].startswith("CHANGED bytes: src/core.js") and "line-ending-only" in problems[0], problems


def test_d1_control_payload_outside_e_is_red_even_if_vendored_matches(payload_dir, tmp_path):
    """A wrong payload cannot make the gate vacuously green: both sides must be exactly E + LICENSE."""
    pl = tmp_path / "payload"
    shutil.copytree(payload_dir, pl)
    (pl / "src" / "rogue.test.mjs").write_text("", encoding="utf-8")
    v = _vendored_copy(tmp_path)
    (v / "src" / "rogue.test.mjs").write_text("", encoding="utf-8")
    problems = mirror.verify(pl, v)
    assert len(problems) == 2 and all("rogue.test.mjs" in x for x in problems), problems


def test_d1_vendored_matches_the_checkout_raw_bytes_no_npm_needed():
    """Node/npm-free SPOT of the same identity chain: vendored == checkout runtime/ for E + LICENSE."""
    for rel in sorted(EXPECTED):
        assert (VENDORED / rel).read_bytes() == (RUNTIME / rel).read_bytes(), rel
        assert b"\r" not in (VENDORED / rel).read_bytes(), rel


# ----------------------------------------------------------------------------- D2 shims, D4 embed


def test_d2_crate_js_shims_are_reexports_of_the_npm_runtime_not_copies():
    for name in ("runtime.js", "operators.js"):
        text = (REPO / "crates" / "pyths_runtime" / "js" / name).read_text(encoding="utf-8")
        code = [ln for ln in text.splitlines() if ln.strip() and not ln.lstrip().startswith("//")]
        assert code == [f'export * from "../../../runtime/src/{name}";'], (name, code)
        assert (RUNTIME / "src" / name).is_file()



def _embed_binary() -> Path:
    """D4 runs ONLY where a `pyths` built from THIS checkout exists (ci.yml's pythscribe job sets
    PYTHSCRIBE_EMBED_BINARY after `cargo build`, plus PYTHSCRIBE_REQUIRE_EMBED=1 so an unset or
    invalid binary there is a FAILURE). Anywhere else it is a plain skip -- NOT gate()-under-
    REQUIRE_ORACLE, which would false-fail every no-binary run (codex M0 B2)."""
    b = os.environ.get("PYTHSCRIBE_EMBED_BINARY")
    require = os.environ.get("PYTHSCRIBE_REQUIRE_EMBED", "").strip() not in ("", "0", "false", "no")
    if not b:
        if require:
            pytest.fail("PYTHSCRIBE_REQUIRE_EMBED=1 but PYTHSCRIBE_EMBED_BINARY is unset (D4 gate would be vacuous)", pytrace=False)
        pytest.skip("PYTHSCRIBE_EMBED_BINARY=<pyths built from THIS checkout> not set (D4 runs in the CI job that builds the compiler)")
    p = Path(b)
    if not p.is_file():
        pytest.fail(f"PYTHSCRIBE_EMBED_BINARY={b!r} is not a file", pytrace=False)
    if not shutil.which("node"):
        if require:
            pytest.fail("PYTHSCRIBE_REQUIRE_EMBED=1 but node is absent (D4 materializes the runtime via `pyths run`)", pytrace=False)
        pytest.skip("node is required: D4 materializes the embedded runtime via `pyths run`")
    return p


def test_d4_binary_materializes_exactly_the_payload(payload_dir, tmp_path):
    """The compiler MATERIALIZES its embedded runtime (the `pyths run` path); every file of E is
    byte-identical to the prepared payload and the set is exactly E."""
    binary = _embed_binary()
    assert embed.verify(binary, payload_dir) == []
    mat = embed.materialize_via_binary(binary, tmp_path / "w")
    assert set(mat) == set(E) and all(mat[rel] == (payload_dir / rel).read_bytes() for rel in E)


def test_d4_control_insertion_after_build_is_red(payload_dir, tmp_path):
    binary = _embed_binary()
    pl = tmp_path / "payload"
    shutil.copytree(payload_dir, pl)
    p = pl / "src" / "core.js"
    p.write_bytes(p.read_bytes().replace(b"\n", b"\n// edited after the build\n", 1))
    problems = embed.verify(binary, pl)
    assert len(problems) == 1 and problems[0].startswith("CHANGED bytes: src/core.js"), problems


def test_d4_control_shortening_after_build_is_red(payload_dir, tmp_path):
    """codex M0 B1: a SHORTENED file (its bytes a substring of the stale embed) must be RED --
    the old `blob.find` substring check passed it GREEN."""
    binary = _embed_binary()
    pl = tmp_path / "payload"
    shutil.copytree(payload_dir, pl)
    readme = pl / "README.md"
    first_line = readme.read_bytes().split(b"\n", 1)[0] + b"\n"
    assert (payload_dir / "README.md").read_bytes().startswith(first_line) and len(first_line) < 64
    readme.write_bytes(first_line)
    problems = embed.verify(binary, pl)
    assert len(problems) == 1 and problems[0].startswith("CHANGED bytes: README.md"), problems


def test_d4_control_crlf_embed_is_red_with_the_hint(payload_dir, tmp_path):
    """The measured failure (a Windows autocrlf checkout built the compiler): CRLF embed -> RED,
    named, with the hint. Simulated from the payload side (payload LF vs a CRLF 'embed')."""
    a = (payload_dir / "src" / "core.js").read_bytes()
    problems = embed.compare({"src/core.js": a.replace(b"\n", b"\r\n")}, {"src/core.js": a}, {"src/core.js"})
    assert problems == [f"CHANGED bytes: src/core.js (the binary embeds a CRLF copy -- built from a non-LF checkout) (payload {len(a)} B, embedded {len(a) + a.count(b'\n')} B)"], problems
    # and a stale binary lacking a newly registered file / carrying an unregistered one is named
    assert embed.compare({}, {"src/core.js": a}, {"src/core.js"}) == ["MISSING from the binary's runtime: src/core.js (stale binary?)"]
    assert embed.compare({"src/core.js": a, "src/x.js": b""}, {"src/core.js": a}, {"src/core.js"}) == ["EXTRA in the binary's runtime (not in runtime_package_files()): src/x.js"]


# ----------------------------------------------------------------------------- W the built wheel


@pytest.mark.skipif(os.environ.get("PYTHSCRIBE_SKIP_PACKAGING") == "1", reason="packaging gate skipped by env")
def test_w_wheel_vendors_exactly_e_plus_license_raw_bytes(tmp_path):
    out = tmp_path / "dist"
    p = subprocess.run([sys.executable, "-m", "build", "--wheel", "--outdir", str(out), str(REPO)], capture_output=True, text=True, check=False, cwd=str(REPO))
    gate(p.returncode == 0, f"python -m build --wheel failed: {p.stderr[-2000:]}")
    [whl] = out.glob("*.whl")
    prefix = "pythscribe/_runtime/pyths-runtime/"
    with zipfile.ZipFile(whl) as z:
        got = {n[len(prefix):]: z.read(n) for n in z.namelist() if n.startswith(prefix)}
    assert set(got) == EXPECTED, (sorted(set(got) - EXPECTED), sorted(EXPECTED - set(got)))
    for rel, b in got.items():
        assert b == (VENDORED / rel).read_bytes(), rel
