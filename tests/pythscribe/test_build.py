"""TG2 -- the explicit build step: produces a verifiable js+wasm artifact, is idempotent and
deterministic, refuses a non-WASM result, never builds a non-statically-readable def, and
the committed-output gates live in test_artifact_committed.py (no node needed -- R2/S2)."""
from __future__ import annotations

import pytest

from conftest import DEMO_DIR, gate_node
from pythscribe import StaticDecorationError, binding_of
from pythscribe._pin import COMPILER_VERSION
from pythscribe.artifacts import MANIFEST_NAME, verify
from pythscribe.build import BuildError, build_module, find_pyths, pyths_version


@pytest.fixture(autouse=True, scope="module")
def _need_node():
    gate_node()


@pytest.fixture(scope="module")
def pyths():
    try:
        p = find_pyths()
    except BuildError as e:
        from conftest import gate

        gate(False, str(e))
    return p


def test_pin_matches_compiler(pyths):
    assert pyths_version(pyths) == COMPILER_VERSION


def test_build_is_idempotent_and_verifies(tmp_path, pyths):
    src = tmp_path / "kernels.py"
    src.write_text((DEMO_DIR / "kernels.py").read_text(encoding="utf-8"), encoding="utf-8")
    [a1] = build_module(src, quiet=True)
    assert a1.wasm.is_file() and a1.wasm.stat().st_size > 0
    assert a1.entry.is_file() and (a1.dir / "pyths-runtime" / "index.js").is_file()
    glue = (a1.dir / "rms_gain.glue.js").read_text(encoding="utf-8")
    assert '"pyths-runtime"' not in glue and "./pyths-runtime/index.js" in glue
    assert "WebAssembly.instantiate" in glue
    # pruned runtime: only the transitive import closure of the glue, no stdlib, no sourcemap refs
    copied = sorted(p.relative_to(a1.dir).as_posix() for p in (a1.dir / "pyths-runtime").rglob("*.js"))
    assert "pyths-runtime/index.js" in copied and not any(c.startswith("pyths-runtime/stdlib/") for c in copied)
    assert not any("sourceMappingURL" in p.read_text(encoding="utf-8") for p in (a1.dir / "pyths-runtime").rglob("*.js"))
    # LF everywhere (platform-independent hashes)
    for p in a1.dir.rglob("*"):
        if p.is_file() and p.suffix in (".js", ".ps", ".json"):
            assert b"\r" not in p.read_bytes(), p
    m1 = (a1.dir / MANIFEST_NAME).read_bytes()
    mtime = a1.wasm.stat().st_mtime_ns
    [a2] = build_module(src, quiet=True)  # no-op rebuild
    assert (a2.dir / MANIFEST_NAME).read_bytes() == m1
    assert a2.wasm.stat().st_mtime_ns == mtime
    verify(a1.dir, function="rms_gain", expected_source_sha256=a1.source_sha256)
    # deterministic: a FORCED rebuild is byte-identical
    snapshot = {p.relative_to(a1.dir).as_posix(): p.read_bytes() for p in a1.dir.rglob("*") if p.is_file()}
    [a3] = build_module(src, force=True, quiet=True)
    after = {p.relative_to(a3.dir).as_posix(): p.read_bytes() for p in a3.dir.rglob("*") if p.is_file()}
    assert after == snapshot

    # the decorator now resolves it
    from conftest import import_module_from

    mod = import_module_from(src)
    b = binding_of(mod.rms_gain)
    assert b.artifact_status == "resolved" and b.artifact.dir == a1.dir


def test_build_refuses_non_wasm_eligible_kernel(tmp_path, pyths):
    """A `@wasm` def the compiler cannot honour (list return) must be a BUILD ERROR, never a
    silently JS-only artifact."""
    src = tmp_path / "bad.py"
    src.write_text(
        "from pythscribe import wasm\n\n@wasm\ndef norm(xs: list[float]) -> list[float]:\n    return xs\n",
        encoding="utf-8",
    )
    with pytest.raises(BuildError, match="cannot be honored|not emit|WASM"):
        build_module(src, quiet=True)
    assert not (tmp_path / "__pythscribe__" / "norm" / MANIFEST_NAME).exists()


def test_build_rejects_non_static_decoration(tmp_path, pyths):
    src = tmp_path / "cond.py"
    src.write_text(
        "from pythscribe import wasm\nif True:\n    @wasm\n    def f(x: float) -> float:\n        return x\n",
        encoding="utf-8",
    )
    with pytest.raises(StaticDecorationError):
        build_module(src, quiet=True)


def test_build_rejects_duplicate_and_shadowed_kernels(tmp_path, pyths):
    src = tmp_path / "dup.py"
    src.write_text(
        "from pythscribe import wasm\n@wasm\ndef f(x: float) -> float:\n    return x\n"
        "@wasm\ndef f(x: float) -> float:\n    return x * 2.0\n",
        encoding="utf-8",
    )
    with pytest.raises(StaticDecorationError, match="bound/deleted again"):
        build_module(src, quiet=True)
    assert not (tmp_path / "__pythscribe__").exists()


def test_build_no_wasm_functions_is_an_error(tmp_path, pyths):
    src = tmp_path / "plain.py"
    src.write_text("def f(x):\n    return x\n", encoding="utf-8")
    with pytest.raises(BuildError, match="no top-level @wasm"):
        build_module(src, quiet=True)


def test_build_refuses_artifact_with_bare_or_missing_specifier(tmp_path, pyths, monkeypatch):
    """Review R2/S6: the post-condition over every shipped JS file -- a bare npm specifier in a
    copied runtime file is a BUILD ERROR, never a verifying artifact that fails to load in a
    browser."""
    import shutil

    from pythscribe.build import RUNTIME_ENV, find_runtime_dir

    real = find_runtime_dir(pyths)
    fake = tmp_path / "runtime"
    shutil.copytree(real, fake, ignore=shutil.ignore_patterns("*.map", "*.test.mjs", "stdlib", "web", "utils"))
    react = fake / "react.js"
    react.write_text('import React from "react";\n' + react.read_text(encoding="utf-8"), encoding="utf-8")
    monkeypatch.setenv(RUNTIME_ENV, str(fake))
    src = tmp_path / "kernels.py"
    src.write_text((DEMO_DIR / "kernels.py").read_text(encoding="utf-8"), encoding="utf-8")
    with pytest.raises(BuildError, match="bare specifier 'react'"):
        build_module(src, quiet=True)
    assert not (tmp_path / "__pythscribe__" / "rms_gain" / MANIFEST_NAME).exists()
