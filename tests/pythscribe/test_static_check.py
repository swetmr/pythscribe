"""U2 / U3 -- the static-readability check and its PAIRED NEGATIVE CONTROLS.

The load-bearing property (plan §5): a function the build cannot see must be REJECTED at
import, never silently left unrouted. Every negative case below must go RED
(StaticDecorationError) while the literal top-level `@wasm` forms are accepted.
"""
from __future__ import annotations

import py_compile

import pytest

from pythscribe import ArtifactNotFoundError, StaticDecorationError, binding_of, wasm
from pythscribe._static import find_wasm_defs

POSITIVE = """
from pythscribe import wasm
import pythscribe

@wasm
def f(x: float) -> float:
    return x * 2.0

@pythscribe.wasm
def g(x: float) -> float:
    return x + 1.0

@pythscribe.decorators.wasm
def h(x: float) -> float:
    return x - 1.0

@wasm()
def k(x: float) -> float:
    return x
"""


def test_u2_positive_literal_forms_accepted(import_source):
    mod = import_source(POSITIVE)
    for name in ("f", "g", "h", "k"):
        b = binding_of(getattr(mod, name))
        assert b.name == name
        assert b.artifact is None and b.artifact_status == "absent"
        assert b.kernel_source.startswith("@wasm\ndef " + name)
    assert mod.f(2.0) == 4.0 and mod.g(1.0) == 2.0 and mod.h(1.0) == 0.0 and mod.k(3.0) == 3.0
    # the build (AST-only) sees exactly the same four defs
    assert [n.name for n in find_wasm_defs(POSITIVE)] == ["f", "g", "h", "k"]


NEGATIVE = {
    "aliased": """
from pythscribe import wasm
w = wasm
@w
def f(x: float) -> float:
    return x
""",
    "aliased_import": """
from pythscribe import wasm as w
@w
def f(x: float) -> float:
    return x
""",
    "dynamic_application": """
from pythscribe import wasm
def f(x: float) -> float:
    return x
f = wasm(f)
""",
    "conditional": """
from pythscribe import wasm
if True:
    @wasm
    def f(x: float) -> float:
        return x
""",
    "nested_in_function": """
from pythscribe import wasm
def outer():
    @wasm
    def f(x: float) -> float:
        return x
    return f
f = outer()
""",
    "stacked_decorators": """
import functools
from pythscribe import wasm
@functools.lru_cache(maxsize=None)
@wasm
def f(x: float) -> float:
    return x
""",
    "stacked_decorators_outer": """
import functools
from pythscribe import wasm
@wasm
@functools.lru_cache(maxsize=None)
def f(x: float) -> float:
    return x
""",
    "shadowed_name": """
import pythscribe
def wasm(fn):
    return pythscribe.wasm(fn)
@wasm
def f(x: float) -> float:
    return x
""",
    # review R1/SF1: a shim object named `pythscribe` whose .wasm delegates to the real decorator
    "shim_attribute_form": """
import types
import pythscribe as _real
pythscribe = types.SimpleNamespace(wasm=lambda fn: _real.wasm(fn))
@pythscribe.wasm
def f(x: float) -> float:
    return x
""",
    "unadmitted_attribute_chain": """
import pythscribe
@pythscribe.decorators.wasm.__call__
def f(x: float) -> float:
    return x
""",
    "method": """
from pythscribe import wasm
class K:
    @wasm
    def f(self, x: float) -> float:
        return x
""",
    "try_block": """
from pythscribe import wasm
try:
    @wasm
    def f(x: float) -> float:
        return x
except ValueError:  # StaticDecorationError is a TypeError: it must propagate out of the try
    pass
""",
    "async_def": """
from pythscribe import wasm
@wasm
async def f(x: float) -> float:
    return x
""",
    # review R1/SF11: a kernel shadowed by a later top-level binding of the same name
    "shadowed_by_later_def": """
from pythscribe import wasm
@wasm
def f(x: float) -> float:
    return x
def f(x):
    return x + 1
""",
    "duplicate_wasm_defs": """
from pythscribe import wasm
@wasm
def f(x: float) -> float:
    return x
@wasm
def f(x: float) -> float:
    return x * 2.0
""",
    "rebound_by_assignment": """
from pythscribe import wasm
@wasm
def f(x: float) -> float:
    return x
f = None
""",
    # review R2/S7: the other top-level rebinding/deleting forms
    "rebound_by_for": """
from pythscribe import wasm
@wasm
def f(x: float) -> float:
    return x
for f in [1]:
    pass
""",
    "rebound_by_with": """
from contextlib import nullcontext
from pythscribe import wasm
@wasm
def f(x: float) -> float:
    return x
with nullcontext(1) as f:
    pass
""",
    "rebound_by_walrus": """
from pythscribe import wasm
@wasm
def f(x: float) -> float:
    return x
print((f := 1))
""",
    "deleted": """
from pythscribe import wasm
@wasm
def f(x: float) -> float:
    return x
del f
""",
    "rebound_by_match": """
from pythscribe import wasm
@wasm
def f(x: float) -> float:
    return x
match 1:
    case f:
        pass
""",
    "rebound_by_later_import": """
from pythscribe import wasm
@wasm
def f(x: float) -> float:
    return x
from math import sqrt as f
""",
}


@pytest.mark.parametrize("case", sorted(NEGATIVE))
def test_u2_negative_control_non_static_decoration_is_rejected(import_source, case):
    """RED on every non-statically-readable form. If this test ever passes without raising,
    the static check has gone vacuous."""
    with pytest.raises(StaticDecorationError):
        import_source(NEGATIVE[case], stem=case)


BUILD_SIDE_REJECTS = ["conditional", "nested_in_function", "stacked_decorators", "stacked_decorators_outer",
                      "method", "try_block", "async_def", "shadowed_by_later_def", "duplicate_wasm_defs",
                      "rebound_by_assignment", "rebound_by_for", "rebound_by_with", "rebound_by_walrus",
                      "deleted", "rebound_by_match", "rebound_by_later_import"]


@pytest.mark.parametrize("case", BUILD_SIDE_REJECTS)
def test_u2_build_side_ast_check_rejects_same_forms(case):
    """The build's AST-only predicate refuses the same structurally-visible forms."""
    with pytest.raises(StaticDecorationError):
        find_wasm_defs(NEGATIVE[case], case)


def test_u2_import_before_the_def_is_not_a_shadow(import_source):
    """R2/S7 false positive: `from m import x as f` BEFORE `@wasm def f` is replaced by the def."""
    mod = import_source(
        "from math import sqrt as f\nfrom pythscribe import wasm\n@wasm\ndef f(x: float) -> float:\n    return x\n",
        stem="import_before",
    )
    assert binding_of(mod.f).name == "f" and mod.f(2.0) == 2.0


def test_u2_async_gets_the_async_message():
    with pytest.raises(StaticDecorationError, match="async"):
        find_wasm_defs(NEGATIVE["async_def"])


def test_u2_negative_control_exec_source_is_rejected():
    ns: dict = {"wasm": wasm}
    with pytest.raises(StaticDecorationError):
        exec("@wasm\ndef f(x: float) -> float:\n    return x\n", ns)


def test_u2_negative_control_lambda_and_non_function_rejected():
    with pytest.raises(StaticDecorationError):
        wasm(lambda x: x)
    with pytest.raises(StaticDecorationError):
        wasm(42)  # type: ignore[arg-type]


def test_sourceless_deployment_falls_back_instead_of_raising(tmp_path):
    """Review R1/B4a: a .pyc-only module (zipapp / frozen / stripped wheel) has nothing to
    check or build -> plain-Python fallback with status 'unreadable-source', never an
    import error."""
    from conftest import DEMO_DIR

    src = tmp_path / "k2.py"
    src.write_text((DEMO_DIR / "kernels.py").read_text(encoding="utf-8"), encoding="utf-8")
    pyc = tmp_path / "k2.pyc"
    py_compile.compile(str(src), cfile=str(pyc), doraise=True)
    src.unlink()
    from conftest import import_module_from

    mod = import_module_from(pyc, "k2_sourceless")
    b = binding_of(mod.rms_gain)
    assert b.artifact is None and b.artifact_status == "unreadable-source"
    assert mod.rms_gain([3.0, 4.0], 10.0) == 10.0 / ((25.0 / 2) ** 0.5)
    assert b.python_calls == 1


def test_u3_explicit_missing_artifact_errors_not_silent_noop(import_source, tmp_path):
    """A decoration that NAMES an artifact target that does not exist must error."""
    src = f"""
from pythscribe import wasm
@wasm(artifact={str(tmp_path / 'does-not-exist')!r})
def f(x: float) -> float:
    return x
"""
    with pytest.raises(ArtifactNotFoundError):
        import_source(src, stem="explicit_missing")


def test_u3_explicit_present_artifact_is_used(demo_artifact, import_source):
    """The positive twin of U3: an explicit artifact that verifies is bound.

    NOTE: the explicit artifact must match the function's CURRENT source hash, so the test
    reuses the demo kernel's exact source text (byte-identical def) with an explicit path."""
    from conftest import DEMO_DIR

    body = (DEMO_DIR / "kernels.py").read_text(encoding="utf-8").split("@wasm\n", 1)[1]
    src = f"from pythscribe import wasm\n@wasm(artifact={str(demo_artifact.dir)!r})\n{body}"
    mod = import_source(src, stem="explicit_present")
    b = binding_of(mod.rms_gain)
    assert b.artifact is not None and b.artifact_status == "resolved"
    assert b.artifact.dir == demo_artifact.dir
