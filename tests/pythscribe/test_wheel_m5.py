"""M5 gates (spec 13-09-26-lib-selfcontained-pip-wheel; validation §H1 + the M5 exit) -- mixed licence +
third-party notices, verified on the BUILT distribution; every gate ships its PAIRED NEGATIVE CONTROL
(the anti-vacuity paired-control convention), mutation-verified on a wheel built from this checkout.

  H1  every wheel's METADATA is `License-Expression: MIT AND LicenseRef-FSL-1.1-ALv2` and its
      .dist-info/licenses/ carries the MIT text (pythscribe/LICENSE), the FSL text (LICENSE.md), the
      vendored runtime's own MIT notice (pythscribe/_runtime/pyths-runtime/LICENSE), the per-path map
      (pythscribe/LICENSES-MAP.md) and the third-party inventory (third_party/inventory.json +
      notices/*.txt + THIRD_PARTY_NOTICES.md) for the wheel's target.
      Controls (each -> verify_wheel_license RED): remove any one required notice; remove or edit any
      third-party notice text; the FSL slot holding an MIT text (and vice versa); an empty notice;
      the expression reduced to `MIT` / absent; a legacy `License:` field or `License ::` classifier;
      a notice present but undeclared as License-File; a stale inventory (Cargo.lock sha mismatch);
      a crate row with no notice; an inventory target missing; a non-release platform tag; and the
      SELF-CONSISTENT tampering (a crate dropped from inventory.json AND its notice AND the .md) which
      only the independent `cargo metadata` closure catches -> RED under --check-closure (asserted
      GREEN without it: that is exactly why the closure check exists and is not skippable).
  H1' the verifier iterates ALL wheels: three tagged copies, only the third corrupted -> RED naming
      the third only.
  REPO the committed third_party/ is bound to THIS Cargo.lock (sha), lists every target's crates with
      notices whose bytes hash as recorded, equals the live `cargo metadata` closure per target, and
      (when cargo-about is installed) is byte-identical to a fresh generation (`--check`).
      Controls: the generator refuses a closure crate with no notice text; a Cargo.lock byte change
      -> the sha binding RED.
Builds a wheel from this checkout with `python -m build` (binary-less `py3-none-any` unless a
compiler is staged at pythscribe/_bin/ by the M1 suite, in which case host-tagged -- both are valid
inputs). Under PYTHSCRIBE_SKIP_PACKAGING=1 the wheel gates are skipped (same switch as the M1 suite).
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tomllib
import zipfile
from pathlib import Path

import pytest

from conftest import REPO, gate
from pythscribe.build._native import TRIPLE_TO_TAG, host_wheel_tag

SCRIPTS = REPO / "scripts"
sys.path.insert(0, str(SCRIPTS))
import gen_third_party_notices as gen  # noqa: E402
import verify_wheel_license as vl  # noqa: E402

THIRD_PARTY = REPO / "third_party"
LOCK_SHA = gen.cargo_lock_sha256()
SKIP_PACKAGING = os.environ.get("PYTHSCRIBE_SKIP_PACKAGING") == "1"
MIT_BYTES = (REPO / "pythscribe" / "LICENSE").read_bytes()
FSL_BYTES = (REPO / "LICENSE.md").read_bytes()


def _inventory() -> dict:
    return json.loads((THIRD_PARTY / "inventory.json").read_text(encoding="utf-8"))


def _has_cargo() -> bool:
    return subprocess.run(["cargo", "--version"], capture_output=True).returncode == 0


# ----------------------------------------------------------------------------- REPO-level gates (no build)


def test_pyproject_declares_the_mixed_expression_and_every_notice_file():
    py = tomllib.loads((REPO / "pyproject.toml").read_text(encoding="utf-8"))["project"]
    assert py["license"] == vl.EXPECTED_EXPRESSION
    assert py["license-files"] == ["pythscribe/LICENSE", "LICENSE.md", "pythscribe/_runtime/pyths-runtime/LICENSE",
                                   "pythscribe/LICENSES-MAP.md", "third_party/**/*"]
    for rel in py["license-files"][:4]:
        assert (REPO / rel).is_file(), rel
    # the three fixed texts are what they claim (the same fingerprints the wheel gate applies)
    assert vl.fp_mit((REPO / "pythscribe" / "LICENSE").read_text(encoding="utf-8")) == []
    assert vl.fp_mit((REPO / "pythscribe" / "_runtime" / "pyths-runtime" / "LICENSE").read_text(encoding="utf-8")) == []
    assert vl.fp_fsl((REPO / "LICENSE.md").read_text(encoding="utf-8")) == []
    assert vl.fp_map((REPO / "pythscribe" / "LICENSES-MAP.md").read_text(encoding="utf-8")) == []
    # the fingerprints discriminate (a swapped text is not silently accepted)
    assert vl.fp_fsl(MIT_BYTES.decode()) and vl.fp_mit(FSL_BYTES.decode()) and vl.fp_map("nothing")


def test_tag_map_is_the_one_authority():
    assert vl.TAG_TO_TRIPLE == {t: tr for tr, t in TRIPLE_TO_TAG.items()} and len(vl.TAG_TO_TRIPLE) == 5
    assert vl.targets_for_tag("any") == list(TRIPLE_TO_TAG)
    assert vl.targets_for_tag("win_amd64") == ["x86_64-pc-windows-msvc"]
    assert vl.targets_for_tag("linux_armv7l") is None and vl.targets_for_tag("win_arm64") is None


def test_committed_inventory_is_bound_to_this_cargo_lock_and_complete():
    inv = _inventory()
    assert inv["schema"] == 1 and inv["root_crate"] == "pyths_cli"
    assert inv["cargo_lock_sha256"] == LOCK_SHA, "third_party/ is STALE: run scripts/gen_third_party_notices.py"
    assert inv["wheel_tags"] == TRIPLE_TO_TAG
    assert set(inv["targets"]) == set(TRIPLE_TO_TAG)
    md = (THIRD_PARTY / "THIRD_PARTY_NOTICES.md").read_text(encoding="utf-8")
    on_disk = {p.name for p in (THIRD_PARTY / "notices").iterdir()}
    assert on_disk == set(inv["notices"]) and on_disk
    for fname, n in inv["notices"].items():
        b = (THIRD_PARTY / "notices" / fname).read_bytes()
        # byte-exact on EVERY checkout: `.gitattributes` keeps third_party/** `-text` (a CRLF conversion on a
        # Windows autocrlf checkout -- the win_amd64 wheel leg -- would break every recorded sha256)
        assert b"\r" not in b, f"{fname} has CRLF line endings: .gitattributes must keep `third_party/** -text`"
        assert vl.sha256_bytes(b) == n["sha256"] and b.strip(), fname
        assert n["crates"], fname
    assert (REPO / "third_party" / "inventory.json").read_bytes().count(b"\r") == 0
    attrs = (REPO / ".gitattributes").read_text(encoding="utf-8")
    assert "third_party/** -text" in attrs, "third_party/ must be checked out byte-exact on every platform"
    for triple, rows in inv["targets"].items():
        assert rows, triple
        for r in rows:
            key = f"{r['name']}@{r['version']}"
            assert r["license"] and r["notices"], key
            for f in r["notices"]:
                assert key in inv["notices"][f]["crates"], (key, f)
            assert f"| {r['name']} | {r['version']} |" in md, key
    # the Cargo.lock binding is LF-normalized (a Windows autocrlf checkout hashes like Linux) and discriminates
    lock = (REPO / "Cargo.lock").read_bytes()
    assert vl.sha256_bytes(lock.replace(b"\r\n", b"\n")) == LOCK_SHA
    assert vl.sha256_bytes(lock.replace(b"\r\n", b"\n") + b"\n# bump") != LOCK_SHA


def test_committed_inventory_equals_the_live_cargo_metadata_closure_per_target():
    gate(_has_cargo(), "cargo is required to re-derive the dependency closure")
    inv = _inventory()
    for triple in TRIPLE_TO_TAG:
        live = {f"{c['name']}@{c['version']}" for c in gen.closure_for_target(triple)}
        have = {f"{r['name']}@{r['version']}" for r in inv["targets"][triple]}
        assert live == have, (triple, sorted(live ^ have))
        assert live, triple


def test_generator_refuses_a_closure_crate_with_no_notice_text():
    targets = {"x86_64-pc-windows-msvc": [{"name": "clap", "version": "4.5.60", "license": "MIT OR Apache-2.0", "repository": ""},
                                          {"name": "ghost", "version": "0.0.1", "license": "MIT", "repository": ""}]}
    licenses = [{"id": "MIT", "name": "MIT License", "text": "MIT License\n\nCopyright (c) x\n", "crates": [("clap", "4.5.60")]}]
    with pytest.raises(SystemExit, match=r"ghost@0.0.1"):
        gen.build_inventory(targets, licenses, LOCK_SHA, "cargo-about test")
    inv, files = gen.build_inventory({"x86_64-pc-windows-msvc": targets["x86_64-pc-windows-msvc"][:1]}, licenses, LOCK_SHA, "cargo-about test")
    assert list(files) == ["001_MIT.txt"] and inv["targets"]["x86_64-pc-windows-msvc"][0]["notices"] == ["001_MIT.txt"]
    assert inv["notices"]["001_MIT.txt"]["sha256"] == vl.sha256_bytes(files["001_MIT.txt"].encode())


def test_generator_check_reproduces_the_committed_directory_byte_for_byte():
    """Release-time gate (RELEASING.md): needs cargo-about; without it this test skips (the sha binding +
    live-closure gates above still RED on a Cargo.lock bump without regeneration)."""
    if gen.cargo_about_available() is None:
        pytest.skip(f"cargo-about not installed (release-time requirement: {gen.INSTALL_HINT})")
    assert gen.check(THIRD_PARTY) == []
    # control: a byte changed in one committed notice -> --check RED naming it
    tmp = REPO / "third_party_check_tmp"
    shutil.copytree(THIRD_PARTY, tmp, dirs_exist_ok=True)
    try:
        f = next((tmp / "notices").iterdir())
        f.write_bytes(f.read_bytes() + b"\n")
        problems = gen.check(tmp)
        assert any(p.startswith("differs: notices/") for p in problems), problems
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


# ----------------------------------------------------------------------------- the built wheel


@pytest.fixture(scope="module")
def wheel(tmp_path_factory) -> Path:
    if SKIP_PACKAGING:
        pytest.skip("packaging gate skipped by env")
    out = tmp_path_factory.mktemp("dist")
    env = {k: v for k, v in os.environ.items() if k != "PYTHONPATH"}  # never the repo root on the build's sys.path (setup.py docstring)
    if any((REPO / "pythscribe" / "_bin" / n).is_file() for n in ("pyths", "pyths.exe")):
        tag = host_wheel_tag()
        gate(tag is not None, "a compiler is staged but this host is not a release target")
        env["PYTHSCRIBE_WHEEL_PLATFORM"] = tag
    p = subprocess.run([sys.executable, "-m", "build", "--wheel", "--outdir", str(out), str(REPO)], capture_output=True, text=True,
                       timeout=900, cwd=str(REPO), env=env)
    gate(p.returncode == 0, f"python -m build --wheel failed: {p.stderr[-2000:]}")
    [whl] = out.glob("*.whl")
    return whl


def _dist_info(whl: Path) -> str:
    with zipfile.ZipFile(whl) as z:
        [di] = sorted({n.split("/")[0] for n in z.namelist() if n.split("/")[0].endswith(".dist-info")})
    return di


def _mutate(src: Path, dst_dir: Path, fn, *, name: str | None = None) -> Path:
    """Rewrite the wheel through fn(member, bytes) -> bytes | None (None drops the member); LF-normalizes
    METADATA so line-based edits are platform-independent (the verifier accepts both)."""
    dst_dir.mkdir(parents=True, exist_ok=True)
    dst = dst_dir / (name or src.name)
    with zipfile.ZipFile(src) as zi, zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as zo:
        for info in zi.infolist():
            data = zi.read(info.filename)
            if info.filename.endswith("/METADATA"):
                data = data.replace(b"\r\n", b"\n")
            data = fn(info.filename, data)
            if data is not None:
                zo.writestr(info, data)
    return dst


def _drop(member: str):
    return lambda n, b: None if n == member else b


def _replace(member: str, new):
    return lambda n, b: (new(b) if callable(new) else new) if n == member else b


def _meta_edit(edit):
    return lambda n, b: edit(b) if n.endswith("/METADATA") else b


def _red(whl: Path, **kw) -> list[str]:
    return vl.verify_wheel(whl, cargo_lock_sha=LOCK_SHA, **kw)


def test_h1_green_on_the_built_wheel_with_the_exact_metadata_and_notice_tree(wheel, tmp_path):
    assert _red(wheel, check_closure=_has_cargo()) == []
    di = _dist_info(wheel)
    with zipfile.ZipFile(wheel) as z:
        meta = vl.parse_metadata(z.read(f"{di}/METADATA").decode("utf-8"))
        names = z.namelist()
    assert meta["License-Expression"] == ["MIT AND LicenseRef-FSL-1.1-ALv2"]
    assert "License" not in meta and not [c for c in meta.get("Classifier", []) if c.startswith("License ::")]
    L = f"{di}/licenses/"
    present = {n[len(L):] for n in names if n.startswith(L)}
    inv = _inventory()
    assert set(vl.REQUIRED_FILES) <= present
    assert {f"third_party/notices/{f}" for f in inv["notices"]} <= present
    assert set(meta["License-File"]) == present
    # the shipped texts ARE the checkout's
    with zipfile.ZipFile(wheel) as z:
        assert z.read(L + "pythscribe/LICENSE") == MIT_BYTES and z.read(L + "LICENSE.md") == FSL_BYTES
        assert z.read(L + "third_party/inventory.json") == (THIRD_PARTY / "inventory.json").read_bytes()
        assert z.read(L + "pythscribe/_runtime/pyths-runtime/LICENSE") == z.read("pythscribe/_runtime/pyths-runtime/LICENSE")
    # the CLI entry point on the dist dir
    assert vl.main(["--dist", str(wheel.parent), "--cargo-lock", str(REPO / "Cargo.lock")]) == 0


@pytest.mark.parametrize("rel", list(vl.REQUIRED_FILES) + ["third_party/notices/001_MIT.txt"])
def test_h1_control_removing_any_required_notice_is_red(wheel, tmp_path, rel):
    di = _dist_info(wheel)
    m = _mutate(wheel, tmp_path / rel.replace("/", "_"), _drop(f"{di}/licenses/{rel}"))
    problems = _red(m)
    assert problems and any(rel in p for p in problems), problems
    # the METADATA still declares it -> "declared but absent"; and the required-file check names it too
    assert any("absent" in p or "missing" in p for p in problems)


def test_h1_control_wrong_or_empty_texts_are_red(wheel, tmp_path):
    L = f"{_dist_info(wheel)}/licenses/"
    fsl_as_mit = _mutate(wheel, tmp_path / "a", _replace(L + "LICENSE.md", MIT_BYTES))
    assert any("not the FSL-1.1-ALv2 title" in p for p in _red(fsl_as_mit))
    mit_as_fsl = _mutate(wheel, tmp_path / "b", _replace(L + "pythscribe/LICENSE", FSL_BYTES))
    assert any("does not start with `MIT License`" in p for p in _red(mit_as_fsl))
    runtime_as_fsl = _mutate(wheel, tmp_path / "c", _replace(L + "pythscribe/_runtime/pyths-runtime/LICENSE", FSL_BYTES))
    r = _red(runtime_as_fsl)
    assert any("pyths-runtime/LICENSE: does not start with" in p for p in r) and any("differs from the payload copy" in p for p in r)
    empty = _mutate(wheel, tmp_path / "d", _replace(L + "third_party/notices/001_MIT.txt", b""))
    assert any("001_MIT.txt" in p for p in _red(empty))
    edited = _mutate(wheel, tmp_path / "e", _replace(L + "third_party/notices/002_MIT.txt", lambda b: b + b"\n-- appended"))
    assert any("002_MIT.txt bytes do not match the inventory sha256" in p for p in _red(edited))
    bad_map = _mutate(wheel, tmp_path / "f", _replace(L + "pythscribe/LICENSES-MAP.md", b"# map\nnothing here\n"))
    assert any("per-path map lacks" in p for p in _red(bad_map))


def test_h1_control_metadata_expression_and_declarations_are_red(wheel, tmp_path):
    mit_only = _mutate(wheel, tmp_path / "a", _meta_edit(lambda b: b.replace(b"License-Expression: MIT AND LicenseRef-FSL-1.1-ALv2", b"License-Expression: MIT")))
    assert any("License-Expression is ['MIT']" in p for p in _red(mit_only))
    none = _mutate(wheel, tmp_path / "b", _meta_edit(lambda b: b.replace(b"License-Expression: MIT AND LicenseRef-FSL-1.1-ALv2\n", b"")))
    assert any("License-Expression is []" in p for p in _red(none))
    undeclared = _mutate(wheel, tmp_path / "c", _meta_edit(lambda b: b.replace(b"License-File: LICENSE.md\n", b"")))
    assert any("licenses/LICENSE.md is present but not declared" in p for p in _red(undeclared))
    legacy = _mutate(wheel, tmp_path / "d", _meta_edit(lambda b: b.replace(b"License-Expression:", b"License: MIT\nLicense-Expression:", 1)))
    assert any("legacy `License:` field" in p for p in _red(legacy))
    classifier = _mutate(wheel, tmp_path / "e", _meta_edit(lambda b: b.replace(b"License-Expression:", b"Classifier: License :: OSI Approved :: MIT License\nLicense-Expression:", 1)))
    assert any("`License ::` classifiers" in p for p in _red(classifier))


def test_h1_control_inventory_binding_and_the_independent_closure(wheel, tmp_path):
    L = f"{_dist_info(wheel)}/licenses/"
    # stale: generated from another Cargo.lock
    assert any("STALE third-party inventory" in p for p in vl.verify_wheel(wheel, cargo_lock_sha="0" * 64))
    other_lock = tmp_path / "Cargo.lock"
    other_lock.write_bytes((REPO / "Cargo.lock").read_bytes() + b"\n# bumped\n")
    assert vl.main(["--dist", str(wheel.parent), "--cargo-lock", str(other_lock)]) == 1
    # a crate row with no notice / a target missing / a notice file the inventory does not list
    def no_notice(b):
        j = json.loads(b); j["targets"]["x86_64-pc-windows-msvc"][0]["notices"] = []; return json.dumps(j).encode()
    assert any("has NO notice text" in p for p in _red(_mutate(wheel, tmp_path / "a", _replace(L + "third_party/inventory.json", no_notice))))
    def drop_target(b):
        j = json.loads(b); del j["targets"]["aarch64-apple-darwin"]; return json.dumps(j).encode()
    assert any("no crates for target aarch64-apple-darwin" in p for p in _red(_mutate(wheel, tmp_path / "b", _replace(L + "third_party/inventory.json", drop_target))))
    # ... and for EVERY release target, whether or not it is this wheel's own: the inventory is one shared
    # artifact, so the control must fire on a host-tagged wheel (a compiler staged at pythscribe/_bin/ by the
    # M1/M4 suites) exactly as on a binary-less `any` wheel -- never tag-dependent
    for triple in TRIPLE_TO_TAG:
        def drop_one(b, t=triple):
            j = json.loads(b); del j["targets"][t]; return json.dumps(j).encode()
        problems = _red(_mutate(wheel, tmp_path / f"b_{triple}", _replace(L + "third_party/inventory.json", drop_one)))
        assert any(f"no crates for target {triple}" in p for p in problems), (vl.wheel_tag(wheel), triple, problems)
    # a foreign platform tag is not a release target. Rewrite the platform tag (the last dash-segment
    # before .whl) to a genuinely-foreign one REGARDLESS of what this wheel's own tag is -- a literal
    # `-any.whl`/`-win_amd64.whl` replace no-ops on the manylinux wheel the pip-suite gate builds
    # (pythscribe-*-manylinux_2_28_x86_64.whl), leaving a valid release tag so the control never fires.
    import re as _re
    foreign = _mutate(wheel, tmp_path / "c", lambda n, b: b,
                      name=_re.sub(r"-[^-]+\.whl$", "-linux_armv7l.whl", wheel.name))
    assert any("not a release target" in p for p in _red(foreign))
    # SELF-CONSISTENT tampering: clap_builder removed from inventory.json + the .md (its notice is shared) ->
    # GREEN without the independent source (documented) and RED with --check-closure
    def drop_crate(n, b):
        if n == L + "third_party/inventory.json":
            j = json.loads(b)
            for t in j["targets"]:
                j["targets"][t] = [r for r in j["targets"][t] if r["name"] != "clap_builder"]
            for f in j["notices"]:
                j["notices"][f]["crates"] = [c for c in j["notices"][f]["crates"] if not c.startswith("clap_builder@")]
            return json.dumps(j).encode()
        if n == L + "third_party/THIRD_PARTY_NOTICES.md":
            return b"\n".join(l for l in b.split(b"\n") if not l.startswith(b"| clap_builder |"))
        return b
    tampered = _mutate(wheel, tmp_path / "d", drop_crate)
    assert _red(tampered) == []
    gate(_has_cargo(), "cargo is required for the --check-closure control")
    problems = _red(tampered, check_closure=True)
    assert problems and all("ABSENT from the inventory: ['clap_builder@" in p for p in problems), problems
    assert _red(wheel, check_closure=True) == []


def test_h1_verifier_iterates_all_wheels_third_corrupt_is_red(wheel, tmp_path):
    L = f"{_dist_info(wheel)}/licenses/"
    dist = tmp_path / "dist"
    names = [wheel.name.replace(f"-{vl.wheel_tag(wheel)}.whl", f"-{t}.whl") for t in ("manylinux_2_28_x86_64", "macosx_11_0_arm64", "win_amd64")]
    for n in names[:2]:
        shutil.copy2(wheel, dist / n) if dist.is_dir() else (dist.mkdir(), shutil.copy2(wheel, dist / n))
    whls, problems = vl.verify_all(dist, cargo_lock_sha=LOCK_SHA)
    assert len(whls) == 2 and problems == []
    _mutate(wheel, dist, _drop(L + "LICENSE.md"), name=names[2])
    whls, problems = vl.verify_all(dist, cargo_lock_sha=LOCK_SHA)
    assert len(whls) == 3 and problems and all(p.startswith(names[2]) for p in problems), problems
    assert vl.main(["--dist", str(dist), "--cargo-lock", "none"]) == 1
    assert vl.verify_all(tmp_path / "empty", cargo_lock_sha=LOCK_SHA) == ([], [f"no wheel in {tmp_path / 'empty'}/"])
