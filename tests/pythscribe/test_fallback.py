"""U1 -- the no-op Python fallback (the "safe to try" property, plan §5) + artifact resolution.

SPOT battery pinned through the SHIPPED wrapper (not a model of it): with NO artifact the
wrapper must return exactly the plain-Python result and count the call. Negative controls:
an unusable artifact (corrupt / stale / tampered manifest / unlisted file / unreadable) must
ALSO fall back (with a LOGGED warning) -- never raise, never return None, never return a
wrong value -- and that must hold under warnings-as-errors too (review R1/B4b).
"""
from __future__ import annotations

import json
import logging
import shutil
import warnings

import pytest

from pythscribe import binding_of
from pythscribe.artifacts import DISABLE_ENV, MANIFEST_NAME, ArtifactError, verify

SPOTS = [
    ([1.0, 2.0, 3.0, 4.0], 0.5),
    ([], 0.5),  # n == 0 branch -> 1.0
    ([0.0, 0.0, 0.0], 2.0),  # rms == 0 branch -> 1.0
    ([3.0, 4.0], 10.0),  # 2.8284271247461903
    ([-1.5, 2.25, -3.125], 1.0),
    ([1e-300, 1e-300], 1e-300),
    ([1e150, -1e150], 1.0),
    ([0.1, 0.2, 0.3], 0.7),
    ([3.0, 4.0], -0.0),  # -0.0 result (sign of zero must survive every path)
    ([1e-160], 1e308),  # +inf result from finite inputs
    ([1e-160], -1e308),  # -inf
    ([1e200], -1.0),  # rms overflows to inf -> -0.0
]


def reference_rms_gain(xs, target):
    """An independent transcription of the kernel (NOT the wrapped function)."""
    acc = 0.0
    n = 0
    for x in xs:
        acc = acc + x * x
        n = n + 1
    if n == 0:
        return 1.0
    rms = (acc / n) ** 0.5
    if rms == 0.0:
        return 1.0
    return target / rms


def same_float(a: float, b: float) -> bool:
    from pythscribe.build.runner import float_bits

    return float_bits(a) == float_bits(b)


@pytest.fixture
def demo_src():
    from conftest import DEMO_DIR

    return (DEMO_DIR / "kernels.py").read_text(encoding="utf-8")


def _copy_artifact_beside(kdir, demo_src, demo_artifact):
    kdir.mkdir()
    (kdir / "kernels.py").write_text(demo_src, encoding="utf-8")
    dst = kdir / "__pythscribe__" / "rms_gain"
    shutil.copytree(demo_artifact.dir, dst)
    return dst


def test_u1_fallback_returns_plain_python_result_when_no_artifact(import_source, demo_src):
    mod = import_source(demo_src, stem="kernels_noartifact")
    b = binding_of(mod.rms_gain)
    assert b.artifact is None and b.artifact_status == "absent"
    assert b.python_calls == 0
    for i, (xs, t) in enumerate(SPOTS, 1):
        got = mod.rms_gain(xs, t)
        assert got is not None
        assert isinstance(got, float)
        assert same_float(got, reference_rms_gain(xs, t)), (xs, t, got)
        assert b.python_calls == i  # the Python body really ran (path marker)


def test_u1_wrapper_preserves_identity_metadata(import_source, demo_src):
    mod = import_source(demo_src, stem="kernels_noartifact")
    assert mod.rms_gain.__name__ == "rms_gain"
    assert mod.rms_gain.__wrapped__ is binding_of(mod.rms_gain).python_fn


def test_u1_negative_control_corrupt_artifact_falls_back_with_logged_warning(demo_src, tmp_path, demo_artifact, caplog):
    """Copy the real artifact beside a copy of the kernel, corrupt the .wasm, import: the
    decorator must refuse the artifact (hash mismatch) and fall back -- not raise, not bind."""
    dst = _copy_artifact_beside(tmp_path / "corrupt", demo_src, demo_artifact)
    wasm = dst / "rms_gain.wasm"
    data = bytearray(wasm.read_bytes())
    data[-1] ^= 0xFF
    wasm.write_bytes(bytes(data))
    with pytest.raises(ArtifactError, match="sha256 mismatch"):
        verify(dst, function="rms_gain", expected_source_sha256=demo_artifact.source_sha256)

    from conftest import import_module_from

    with caplog.at_level(logging.WARNING, logger="pythscribe"):
        mod = import_module_from(dst.parents[1] / "kernels.py")
    assert any("unusable" in r.getMessage() and "sha256 mismatch" in r.getMessage() for r in caplog.records)
    b = binding_of(mod.rms_gain)
    assert b.artifact is None and b.artifact_status == "unusable"
    assert mod.rms_gain([1.0, 2.0, 3.0, 4.0], 0.5) == reference_rms_gain([1.0, 2.0, 3.0, 4.0], 0.5)


def test_u1_negative_control_fallback_survives_warnings_as_errors(demo_src, tmp_path, demo_artifact):
    """Review R1/B4b: `-W error` / filterwarnings=error must NOT turn the fallback into an
    import failure -- the notice is logged, never raised."""
    dst = _copy_artifact_beside(tmp_path / "werror", demo_src, demo_artifact)
    (dst / "rms_gain.wasm").write_bytes(b"\0asm broken")
    from conftest import import_module_from

    with warnings.catch_warnings():
        warnings.simplefilter("error")
        mod = import_module_from(dst.parents[1] / "kernels.py")
    assert binding_of(mod.rms_gain).artifact_status == "unusable"
    assert mod.rms_gain([3.0, 4.0], 10.0) == reference_rms_gain([3.0, 4.0], 10.0)


def test_u1_negative_control_stale_artifact_falls_back(demo_src, tmp_path, demo_artifact, caplog):
    """Source changed since the build -> the manifest's source hash no longer matches -> fallback."""
    kdir = tmp_path / "stale"
    kdir.mkdir()
    # a SEMANTIC edit (a trailing comment is outside the def's source segment and rightly
    # does not change the kernel hash)
    edited = demo_src.replace("    if n == 0:\n        return 1.0", "    if n == 0:\n        return 1.5")
    assert edited != demo_src
    (kdir / "kernels.py").write_text(edited, encoding="utf-8")
    shutil.copytree(demo_artifact.dir, kdir / "__pythscribe__" / "rms_gain")
    from conftest import import_module_from

    with caplog.at_level(logging.WARNING, logger="pythscribe"):
        mod = import_module_from(kdir / "kernels.py")
    assert any("STALE" in r.getMessage() for r in caplog.records)
    assert binding_of(mod.rms_gain).artifact_status == "unusable"
    assert mod.rms_gain([], 0.5) == 1.5  # the NEW Python body runs, not the old compiled one


def test_u1_negative_control_tampered_manifest_is_refused(tmp_path, demo_artifact):
    """Review R1/SF9: pasting a new source hash into the manifest must not revive a stale
    artifact -- the manifest self-hash catches it; so does an unlisted file."""
    dst = tmp_path / "a"
    shutil.copytree(demo_artifact.dir, dst)
    m = json.loads((dst / MANIFEST_NAME).read_text(encoding="utf-8"))
    m["source_sha256"] = "0" * 64
    (dst / MANIFEST_NAME).write_text(json.dumps(m), encoding="utf-8")
    with pytest.raises(ArtifactError, match="self-hash"):
        verify(dst, function="rms_gain", expected_source_sha256="0" * 64)

    dst2 = tmp_path / "b"
    shutil.copytree(demo_artifact.dir, dst2)
    (dst2 / "extra.js").write_text("export const x = 1;\n", encoding="utf-8")
    with pytest.raises(ArtifactError, match="unlisted file"):
        verify(dst2, function="rms_gain", expected_source_sha256=None)


def test_desktop_noise_files_do_not_disable_the_artifact_but_real_strays_do(tmp_path, demo_artifact):
    """Review R2/S5: .DS_Store/Thumbs.db/*.swp are tolerated (nothing imports them); any
    other unlisted file still refuses the artifact with an actionable message."""
    dst = tmp_path / "noise"
    shutil.copytree(demo_artifact.dir, dst)
    (dst / ".DS_Store").write_bytes(b"\0")
    (dst / "pyths-runtime" / "Thumbs.db").write_bytes(b"\0")
    (dst / ".rms_gain.js.swp").write_bytes(b"\0")
    verify(dst, function="rms_gain", expected_source_sha256=None)
    (dst / "notes.txt").write_text("x", encoding="utf-8")
    with pytest.raises(ArtifactError, match="unlisted file.*--force"):
        verify(dst, function="rms_gain", expected_source_sha256=None)


def test_u1_negative_control_missing_listed_file_and_bad_manifest(tmp_path, demo_artifact):
    from pythscribe.artifacts import manifest_self_hash

    dst = tmp_path / "a"
    shutil.copytree(demo_artifact.dir, dst)
    (dst / "pyths-runtime" / "index.js").unlink()
    with pytest.raises(ArtifactError, match="listed file missing"):
        verify(dst, function="rms_gain", expected_source_sha256=None)
    m = json.loads((dst / MANIFEST_NAME).read_text())
    m.pop("wasm")
    m["manifest_sha256"] = manifest_self_hash(m)  # re-sign: the self-hash must not be the only guard
    (dst / MANIFEST_NAME).write_text(json.dumps(m))
    with pytest.raises(ArtifactError):
        verify(dst, function="rms_gain", expected_source_sha256=None)


def test_u1_negative_control_unreadable_artifact_dir_falls_back(demo_src, tmp_path, demo_artifact, monkeypatch, caplog):
    """Review R1/SF10: an I/O error while verifying must become 'unusable', not escape."""
    dst = _copy_artifact_beside(tmp_path / "ioerr", demo_src, demo_artifact)
    import pythscribe.artifacts as A

    def boom(path):
        raise PermissionError(f"denied: {path}")

    monkeypatch.setattr(A, "sha256_file", boom)
    from conftest import import_module_from

    with caplog.at_level(logging.WARNING, logger="pythscribe"):
        mod = import_module_from(dst.parents[1] / "kernels.py")
    assert binding_of(mod.rms_gain).artifact_status == "unusable"
    assert mod.rms_gain([], 0.5) == 1.0


def test_disable_env_forces_fallback(monkeypatch, demo_src, tmp_path, demo_artifact):
    dst = _copy_artifact_beside(tmp_path / "disabled", demo_src, demo_artifact)
    from conftest import import_module_from

    monkeypatch.setenv(DISABLE_ENV, "1")
    mod = import_module_from(dst.parents[1] / "kernels.py")
    assert binding_of(mod.rms_gain).artifact_status == "disabled"
    monkeypatch.delenv(DISABLE_ENV)
    mod2 = import_module_from(dst.parents[1] / "kernels.py")
    assert binding_of(mod2.rms_gain).artifact_status == "resolved"
