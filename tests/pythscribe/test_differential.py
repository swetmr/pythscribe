"""D1 / D2 / D3 -- the free differential oracle (plan §5): the compiled artifact, run under
Node, must equal the plain-Python function BYTE-FOR-BYTE (IEEE-754 bits), on a pinned SPOT
battery and on a Hypothesis sweep. D3 gates the TRANSPORTED quantity (the bits string after
the JSON hop the Gradio return path takes), so -0.0 / inf cannot be silently mangled
(review R1/B2). Paired negative controls: a corrupted .wasm, a seeded off-by-one mutation
of the compiled path, and a poisoned JS twin WITH its own positive control (the twin is
provably reachable) -- never a control that can pass vacuously.
"""
from __future__ import annotations

import json
import math
import shutil

import pytest
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from conftest import gate_node
from pythscribe.artifacts import ArtifactError, verify
from pythscribe.build.runner import RunnerError, float_bits, run_artifact
from test_fallback import SPOTS, reference_rms_gain


@pytest.fixture(autouse=True, scope="module")
def _need_node():
    gate_node()


def compiled(artifact_or_entry, calls):
    return run_artifact(artifact_or_entry, "rms_gain", calls)


def python_oracle(demo_kernels, xs, t):
    """The CPython arm, EXPLICITLY (opus m1.5 r1/B1): since M1.5 a bare call of an
    artifact-bound kernel runs the in-process WASM when wasmtime is installed, which would
    collapse this differential onto the path under test. The marker assertion binds the
    oracle to the Python body so the arm can never silently move again."""
    from pythscribe import binding_of

    b = binding_of(demo_kernels.rms_gain)
    before = b.calls()
    v = b.run_python(xs, t)
    assert b.calls() == before + 1  # the Python body ran (path marker)
    return v


def test_d1_spot_battery_compiled_equals_python_bit_for_bit(demo_artifact, demo_kernels):
    res = compiled(demo_artifact, SPOTS)
    for (xs, t), r in zip(SPOTS, res):
        py = python_oracle(demo_kernels, xs, t)  # the Python body, explicitly, is the oracle
        assert r["ok"], r
        assert r["bits"] == float_bits(py), (xs, t, r, py)
        assert r["jstype"] in ("number", "object")  # 'object' == the glue's __PyFloatW box for integer-valued floats


def test_d3_transported_quantity_equals_python_after_json_hop(demo_artifact, demo_kernels):
    """The one crossed return type (float): what the adapter reconstructs on the server
    from the client's JSON (`bits`) must equal the Python oracle -- including -0.0, +inf,
    -inf, and integer-valued results (the boxed __PyFloatW). The JSON `value` is NOT the
    oracle: JSON.stringify(-0) is "0" and Infinity is null."""
    from pythscribe.gradio import float_from_bits

    cases = [([3.0, 4.0], 10.0), ([], 0.5), ([1.0], 2.0**53 + 1), ([0.5], 2.0**60), ([2.0], 1e-320),
             ([3.0, 4.0], -0.0), ([1e-160], 1e308), ([1e-160], -1e308), ([1e200], -1.0)]
    res = compiled(demo_artifact, cases)
    for (xs, t), r in zip(cases, res):
        py = python_oracle(demo_kernels, xs, t)
        hop = json.loads(json.dumps(r))  # the JSON hop the Gradio return path takes
        v = float_from_bits(hop["bits"])
        assert v is not None
        assert float_bits(v) == float_bits(py), (xs, t, v, py)
        if not math.isfinite(py) or (py == 0.0 and math.copysign(1.0, py) < 0):
            # the convenience `value` is exactly where a naive path would corrupt these
            assert hop["value"] in (0, None) or hop["value"] != py or math.copysign(1.0, hop["value"]) != math.copysign(1.0, py)
    # an int-returning kernel would cross via __i64ToJs (Number <= 2**53-1 else BigInt): NOT admitted
    # in M0 -- documented in decorators.py; the component refuses a BigInt result loudly.


@settings(max_examples=40, deadline=None, suppress_health_check=[HealthCheck.too_slow])
@given(
    xs=st.lists(st.floats(min_value=-1e100, max_value=1e100, allow_nan=False, allow_infinity=False), max_size=24),
    target=st.floats(min_value=-1e100, max_value=1e100, allow_nan=False, allow_infinity=False),
)
def test_d2_pbt_sweep_compiled_equals_python(demo_artifact, demo_kernels, xs, target):
    [r] = compiled(demo_artifact, [(xs, target)])
    py = python_oracle(demo_kernels, xs, target)
    assert r["ok"], r
    assert_bits_equal_up_to_host_libm(demo_artifact, r["bits"], py, (xs, target, r["value"], py))


def artifact_imports_host_math(artifact) -> bool:
    """True iff the artifact's glue satisfies a `math.*` WASM import with `Math.*` -- the kernel then
    inherits the HOST's libm on the Node/browser arm (V8 fdlibm) vs CPython's platform libm."""
    import re

    glue = (artifact.dir / f"{artifact.function}.glue.js").read_text(encoding="utf-8")
    m = re.search(r"^\s*const imports = (\{.*?\});\s*$", glue, re.MULTILINE)  # the DECLARED import object (r9/N9-6)
    return bool(m) and "math:" in m.group(1) and "Math." in m.group(1)


def ulp_distance(bits_a: str, b: float) -> int:
    import struct

    ia = struct.unpack("<q", bytes.fromhex(bits_a))[0]
    ib = struct.unpack("<q", struct.pack("<d", b))[0]
    # map the sign-magnitude int64 onto a monotone integer line so |ia - ib| counts ULPs across zero
    ia = ia if ia >= 0 else -(ia & 0x7FFFFFFFFFFFFFFF)
    ib = ib if ib >= 0 else -(ib & 0x7FFFFFFFFFFFFFFF)
    return abs(ia - ib)


def assert_bits_equal_up_to_host_libm(artifact, bits: str, py: float, ctx) -> None:
    """The M0 claim, stated exactly (opus m1.5 r8/N8-1): a kernel WITHOUT host imports is bit-for-bit;
    a kernel that imports `math.*` inherits the host libm on the compiled arm, so the compiled and
    CPython results may differ by ONE ULP on transcendental calls (documented in pythscribe/README.md
    and runtime/_jsmath.py) -- never more, and the tolerance is only ever granted for such a kernel."""
    if bits == float_bits(py):
        return
    assert artifact_imports_host_math(artifact), ("bit mismatch on a kernel with NO host imports", ctx)
    # a zero (either sign) or a non-finite result must match EXACTLY: those are representation
    # divergences (the D3 class), not last-ulp rounding (r9/N9-5)
    assert py != 0.0 and math.isfinite(py), ("zero/non-finite result differs on a host-math kernel", ctx, bits, float_bits(py))
    assert ulp_distance(bits, py) <= 1, ("more than 1 ULP apart on a host-math kernel", ctx, bits, float_bits(py))


def test_d2_host_libm_witness_is_within_one_ulp(demo_artifact, demo_kernels, tmp_path):
    """The Hypothesis-drawn witness (opus m1.5 r8/N8-1), pinned: V8's Math.pow(x, 0.5) vs CPython's
    libm pow -- the DOCUMENTED host-libm caveat is a divergence of AT MOST ONE ULP. It is exactly 1
    on some V8/libm builds (e.g. Windows) and 0 on others (e.g. the Linux CI runner, where V8 and
    glibc's pow agree bit-for-bit), so a cross-platform gate asserts the <=1-ULP GUARANTEE, not an
    exact 1 (an == 1 pin false-REDs wherever the two happen to agree). The wasmtime arm (host =
    CPython's math.pow) is bit-identical to CPython (tests/pythscribe/test_runtime.py::
    test_r2_host_math_pow_kernel_bit_identical, which carries this very input). A GROWTH past 1 ULP
    IS a regression and goes RED here (r9/N9-7)."""
    xs, t = [0.0] * 12 + [4.726339908522518e+99, 6.950260023638996e+99], 1.0
    [r] = compiled(demo_artifact, [(xs, t)])
    py = python_oracle(demo_kernels, xs, t)
    assert r["ok"] and artifact_imports_host_math(demo_artifact)
    assert ulp_distance(r["bits"], py) <= 1, (r["bits"], float_bits(py))  # the documented <=1-ULP guarantee; >1 = regression (RED)
    assert_bits_equal_up_to_host_libm(demo_artifact, r["bits"], py, (xs, t))
    # the paired RED half: the tolerance is REFUSED for a kernel that imports nothing from the host --
    # built in tmp_path (r9/N9-2: never write into the committed artifact directory, whose manifest
    # refuses any unlisted file and would turn the demo `unusable` inside the window)
    from types import SimpleNamespace

    d = tmp_path / "pure"
    d.mkdir()
    (d / "__pure__.glue.js").write_text("const imports = {};\n", encoding="utf-8")
    pure = SimpleNamespace(dir=d, function="__pure__")
    assert not artifact_imports_host_math(pure)
    with pytest.raises(AssertionError, match="NO host imports"):
        assert_bits_equal_up_to_host_libm(pure, r["bits"], py, (xs, t))
    # and a decoy that only MENTIONS the import object in a comment is not an import (r9/N9-6)
    (d / "__pure__.glue.js").write_text("// imports = { math: { pow: Math.pow } } was removed\nconst imports = {};\n", encoding="utf-8")
    assert not artifact_imports_host_math(pure)


D2_BATCH_INPUTS = [([0.0] * 12 + [4.726339908522518e+99 * (1 + i * 1e-3), 6.950260023638996e+99], 1.0) for i in range(20)] + [
    ([float(i % 7) * 3.5 - 8.0, float(i) * 1e-3, 1e10 / (i + 1)], float(i) - 100.5) for i in range(180)
]


def test_d2_host_libm_divergence_is_rare_not_systematic(demo_artifact, demo_kernels):
    """r9/N9-1 -- the per-example tolerance must not cost the sweep its sensitivity to a UNIFORM
    corruption of the compiled path (a 1-ULP glue/codegen error diverges on ~100% of inputs; the
    host-libm caveat on a rare few). Over 200 fixed inputs (20 deliberately near the witness), at most
    5 may differ, and each by <= 1 ULP on this host-math kernel. RED for a `__f64Box(__raw) + ulp`
    mutant (200/200 differ -- see the negative control below, which is pointed at THIS predicate)."""
    res = compiled(demo_artifact, D2_BATCH_INPUTS)
    diverged = 0
    for (xs, t), r in zip(D2_BATCH_INPUTS, res):
        assert r["ok"], r
        py = python_oracle(demo_kernels, xs, t)
        if r["bits"] != float_bits(py):
            diverged += 1
            assert_bits_equal_up_to_host_libm(demo_artifact, r["bits"], py, (xs, t))
    assert diverged <= 5, f"{diverged}/{len(D2_BATCH_INPUTS)} inputs diverge: systematic, not the host-libm caveat"


# ---------------------------------------------------------------- paired negative controls
def _mutant(tmp_path, demo_artifact, mutate):
    dst = tmp_path / "mutant"
    shutil.copytree(demo_artifact.dir, dst)
    mutate(dst)
    return dst


def test_d1_negative_control_corrupted_wasm_is_caught(tmp_path, demo_artifact, demo_kernels):
    """Truncate the .wasm. (1) verification refuses it (-> the decorator would fall back);
    (2) even bypassing verification, running it is RED (load error), never a silent value."""

    def mutate(d):
        w = d / "rms_gain.wasm"
        w.write_bytes(w.read_bytes()[: len(w.read_bytes()) // 2])

    dst = _mutant(tmp_path, demo_artifact, mutate)
    with pytest.raises(ArtifactError, match="sha256 mismatch"):
        verify(dst, function="rms_gain", expected_source_sha256=demo_artifact.source_sha256)
    with pytest.raises(RunnerError):
        compiled(dst / "rms_gain.js", SPOTS)


def test_d1_negative_control_wrong_value_wasm_is_caught(tmp_path, demo_artifact, demo_kernels):
    """Seed an off-by-one on the compiled path's export: the manifest refuses it, AND the
    differential goes RED on the SPOT battery."""

    def mutate(d):
        glue = d / "rms_gain.glue.js"
        js = glue.read_text(encoding="utf-8")
        assert "return __f64Box(__raw);" in js
        glue.write_text(js.replace("return __f64Box(__raw);", "return __f64Box(__raw + 1);"), encoding="utf-8")

    dst = _mutant(tmp_path, demo_artifact, mutate)
    with pytest.raises(ArtifactError, match="sha256 mismatch"):
        verify(dst, function="rms_gain", expected_source_sha256=demo_artifact.source_sha256)
    res = compiled(dst / "rms_gain.js", SPOTS)
    finite = {i for i, (xs, t) in enumerate(SPOTS) if math.isfinite(python_oracle(demo_kernels, xs, t))}
    mismatches = {
        i for i, ((xs, t), r) in enumerate(zip(SPOTS, res))
        if not r["ok"] or r["bits"] != float_bits(python_oracle(demo_kernels, xs, t))
    }
    assert mismatches, "the differential did NOT detect a wrong-value compiled path (vacuous oracle)"
    assert finite <= mismatches  # every finite case is refuted (inf + 1 == inf is the only pass)


def test_d2_negative_control_seeded_mutation_refuted_by_sweep(tmp_path, demo_artifact, demo_kernels):
    """The PBT sweep's inputs (a fixed, seeded sample) must refute a seeded off-by-ulp mutation."""

    def mutate(d):
        glue = d / "rms_gain.glue.js"
        js = glue.read_text(encoding="utf-8")
        glue.write_text(
            js.replace("return __f64Box(__raw);", "return __f64Box(__raw === 0 ? __raw : __raw * (1 + Number.EPSILON));"),
            encoding="utf-8",
        )

    dst = _mutant(tmp_path, demo_artifact, mutate)
    import random

    rng = random.Random(20260902)
    calls = [([rng.uniform(-1e3, 1e3) for _ in range(rng.randint(1, 12))], rng.uniform(-1e3, 1e3)) for _ in range(60)]
    res = compiled(dst / "rms_gain.js", calls)
    bad = [c for c, r in zip(calls, res) if not r["ok"] or r["bits"] != float_bits(python_oracle(demo_kernels, *c))]
    assert bad, "sweep failed to refute a 1-ulp mutation of the compiled path"

def test_d2_negative_control_uniform_ulp_mutant_trips_the_batch_bound(demo_artifact, demo_kernels, tmp_path):
    """r9/N9-1 paired control for the batch gate: a compiled path that is off by exactly ONE ULP on
    EVERY call (the mutant the per-example tolerance would forgive) trips the divergence-rate bound."""
    def mutate(d):
        g = d / "rms_gain.glue.js"
        js = g.read_text(encoding="utf-8")
        assert "__f64Box(__raw)" in js
        g.write_text(js.replace("__f64Box(__raw)", "__f64Box(__raw === 0 ? __raw : (__raw > 0 ? __raw * (1 + Number.EPSILON) : __raw * (1 - Number.EPSILON)))", 1), encoding="utf-8", newline="\n")

    dst = _mutant(tmp_path, demo_artifact, mutate)
    res = run_artifact(dst / "rms_gain.js", "rms_gain", D2_BATCH_INPUTS)
    diverged = sum(1 for (xs, t), r in zip(D2_BATCH_INPUTS, res) if not r["ok"] or r["bits"] != float_bits(python_oracle(demo_kernels, xs, t)))
    assert diverged > 5, f"the uniform 1-ULP mutant diverged on only {diverged}/{len(D2_BATCH_INPUTS)}: the batch bound would not catch it"



# ------------------------------------------------- resolution marker for the compiled arm itself
TWIN_LINE = "    let acc = __pyF(0);"  # first statement of the glue's JS twin (`__jsfb.rms_gain`)
TWIN_POISON = "    return 12345;"
WASM_CALL = "__wasm.rms_gain("


def poison_js_twin(artifact_dir):
    """Make the glue's JS twin (the #358 arbitrary-precision fallback used on WASM faults)
    return a wrong constant. If results stay correct, WASM computed them -- not the twin."""
    glue = artifact_dir / "rms_gain.glue.js"
    js = glue.read_text(encoding="utf-8")
    assert js.count(TWIN_LINE) == 1, "glue shape changed; update TWIN_LINE"
    glue.write_text(js.replace(TWIN_LINE, TWIN_POISON + "\n" + TWIN_LINE), encoding="utf-8")


def force_wasm_fault(artifact_dir):
    """Make the WASM call itself throw a WebAssembly.RuntimeError so the glue takes the twin arm."""
    glue = artifact_dir / "rms_gain.glue.js"
    js = glue.read_text(encoding="utf-8")
    assert js.count(WASM_CALL) == 1, "glue shape changed; update WASM_CALL"
    glue.write_text(js.replace(WASM_CALL, '(() => { throw new WebAssembly.RuntimeError("forced"); })('), encoding="utf-8")


def test_node_arm_resolution_marker_wasm_not_js_twin(tmp_path, demo_artifact, demo_kernels):
    dst = _mutant(tmp_path, demo_artifact, poison_js_twin)
    res = compiled(dst / "rms_gain.js", SPOTS)
    for (xs, t), r in zip(SPOTS, res):
        assert r["ok"] and r["bits"] == float_bits(python_oracle(demo_kernels, xs, t)), (xs, t, r)
        assert r["value"] != 12345


def test_node_arm_resolution_marker_positive_control_twin_is_reachable(tmp_path, demo_artifact):
    """Review R1/SF7: the positive half -- the poisoned twin IS what runs when WASM faults, so
    the marker above cannot pass vacuously (an inert poison would make this test RED)."""

    def mutate(d):
        poison_js_twin(d)
        force_wasm_fault(d)

    dst = _mutant(tmp_path, demo_artifact, mutate)
    res = compiled(dst / "rms_gain.js", SPOTS[:3])
    assert all(r["ok"] and r["value"] == 12345 for r in res), res
