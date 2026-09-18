from __future__ import annotations

import importlib.util
import os
import shutil
import sys
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[2]
DEMO_DIR = REPO / "examples" / "gradio-wasm"
DEMO_ARTIFACT_DIR = DEMO_DIR / "__pythscribe__" / "rms_gain"
sys.path.insert(0, str(Path(__file__).resolve().parent))

# PYTHSCRIBE_REQUIRE_ORACLE=1: a missing prerequisite (node, playwright, the component, the
# demo artifact) is a FAILURE, not a skip. Without it (a developer's partial environment)
# the affected gates skip and say so. CI always sets it (review R1/B3: a suite that reports
# PASS with its oracle skipped must go RED). Its own negative control: test_gate_infra.py.
REQUIRE_ORACLE = os.environ.get("PYTHSCRIBE_REQUIRE_ORACLE", "").strip() not in ("", "0", "false", "no")
GATE_MISSING = "REQUIRED oracle prerequisite missing"


def gate(ok: bool, reason: str) -> None:
    if ok:
        return
    if REQUIRE_ORACLE:
        pytest.fail(f"{GATE_MISSING}: {reason}", pytrace=False)
    pytest.skip(reason)


def soft_perf(ok: bool, msg: str) -> None:
    """Speedup / latency / scaling numbers vary with the host CPU and load; the shipped wheels are
    byte-identical (reproducible build), so the CODE is identical and these numbers are NOT a correctness
    signal -- they must NEVER gate the release suite (user 2026-09-18; the recurring fanout/latency
    flakes). Record + WARN instead of asserting: a gross margin still surfaces in the warning, timing
    noise never turns the gate red. Keep the CORRECTNESS facts (results agree, sandbox contained,
    isomorphic, determinism, byte/size reductions) as hard asserts -- only the timing goes soft."""
    import warnings
    if not ok:
        warnings.warn(f"perf(non-blocking): {msg}", stacklevel=2)


def gate_import(modname: str):
    try:
        return importlib.import_module(modname)
    except ImportError as e:
        gate(False, f"{modname} not importable ({e})")
        raise  # unreachable


def gate_node() -> str:
    node = shutil.which("node")
    gate(bool(node), "node is required to run the compiled artifact")
    return node  # type: ignore[return-value]


def import_module_from(path: Path, name: str | None = None):
    """Import a source file as a fresh module (each test module gets its own file so the
    static check reads real source, exactly as a user's app would)."""
    name = name or f"_pythscribe_t_{path.stem}_{abs(hash(str(path)))}"
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    try:
        spec.loader.exec_module(mod)
    finally:
        sys.modules.pop(name, None)
    return mod


@pytest.fixture(autouse=True)
def _no_jit_by_default(monkeypatch):
    """Compile-on-first-call (`@wasm` with no pre-built artifact) is default-ON for users, but the
    rest of the suite pins the artifact/fallback machinery: with it on, a no-artifact ELIGIBLE
    kernel would compile to `server` on first call instead of taking the fallback path these tests
    assert. So the suite defaults it OFF; `test_compile_on_first_call.py` re-enables it to exercise
    the feature (and its paired negative controls)."""
    monkeypatch.setenv("PYTHSCRIBE_NO_JIT", "1")


@pytest.fixture
def import_source(tmp_path):
    """Write `source` to a temp .py file and import it. Returns the module (or raises)."""
    counter = {"n": 0}

    def _imp(source: str, stem: str = "mod"):
        counter["n"] += 1
        p = tmp_path / f"{stem}{counter['n']}.py"
        p.write_text(source, encoding="utf-8")
        return import_module_from(p)

    return _imp


@pytest.fixture(scope="session")
def demo_kernels():
    """The committed demo module (`examples/gradio-wasm/kernels.py`), imported for real."""
    return import_module_from(DEMO_DIR / "kernels.py", "kernels_demo_for_tests")


@pytest.fixture(scope="session")
def demo_artifact(demo_kernels):
    from pythscribe import binding_of

    b = binding_of(demo_kernels.rms_gain)
    gate(
        b.artifact is not None,
        f"demo artifact not usable (status={b.artifact_status}); run `python -m pythscribe.build examples/gradio-wasm/kernels.py`",
    )
    return b.artifact
