"""v0.2.5 publish smoke-gate -- the SIMPLE all-features test in the minimal canonical shape, run
against the (editable) src build. It gates the PyPI publish: if `import pythscribe` + the shipped
features don't work end-to-end here, we do not ship.

Canonical idiom (compile-on-first-call: no `python -m pythscribe.build`, no `import kernels`):
`from pythscribe import wasm`, decorate a plain annotated MODULE-LEVEL function, call it, assert
`@wasm == plain Python / NumPy`. Covers: scalar kernel, 1-D + 2-D typed arrays, the fuel sandbox,
the LAYOUT-version ABI stamps (consistent across every shipped copy), and the Gradio adapter
resolving the compiled bundle. `from __future__ import annotations` keeps the `Array[...]`
annotations lazy. JIT is ON here (the suite defaults PYTHSCRIBE_NO_JIT=1; we re-enable it).
"""
from __future__ import annotations

import pathlib
import re
import time

import pytest

from conftest import gate, gate_import
from pythscribe import binding_of, wasm
from pythscribe.build import BuildError, find_pyths
from pythscribe.runtime import FuelExhausted, wasmtime_available


# --- module-level @wasm kernels (nested/method decoration is not statically readable) ---
@wasm
def sum_sq(xs: list[float]) -> float:
    acc = 0.0
    for x in xs:
        acc = acc + x * x
    return acc


@wasm
def scale_1d(a: Array[int32, 1], out: Array[int32, 1], k: int) -> int:
    n = len(a)
    for i in range(n):
        out[i] = a[i] * k
    return n


@wasm
def threshold_2d(img: Array[uint8, 2], out: Array[uint8, 2], h: int, w: int, thr: int) -> int:
    for y in range(h):
        for x in range(w):
            v = img[y][x]
            if v >= thr:
                out[y][x] = 255
            else:
                out[y][x] = 0
    return h * w


@wasm(fuel=2_000_000)
def spin(n: int) -> int:
    i = 0
    while n != 0:
        i = (i + 1) % 1000
    return i


@wasm
def rms(xs: list[float]) -> float:
    acc = 0.0
    n = 0
    for x in xs:
        acc = acc + x * x
        n = n + 1
    if n == 0:
        return 1.0
    return (acc / n) ** 0.5


def sum_sq_py(xs):
    acc = 0.0
    for x in xs:
        acc = acc + x * x
    return acc


@pytest.fixture(autouse=True)
def _jit_on(tmp_path, monkeypatch):
    monkeypatch.setenv("PYTHSCRIBE_CACHE", str(tmp_path / "jit"))
    monkeypatch.delenv("PYTHSCRIBE_NO_JIT", raising=False)


def _require_toolchain():
    try:
        find_pyths()
    except BuildError as e:
        gate(False, f"pyths compiler required for the publish smoke-gate ({e})")
    gate(wasmtime_available(), "wasmtime-py required for the server path")


def _avg_ms(fn, n=10):
    t0 = time.perf_counter()
    for _ in range(n):
        fn()
    return (time.perf_counter() - t0) / n * 1e3


def test_scalar_kernel_compiles_and_matches_cpython():
    """The headline: `@wasm` on a plain function, call it, it runs as WASM == CPython."""
    _require_toolchain()
    xs = [0.1 * i for i in range(500)]
    assert binding_of(sum_sq).mode == "fallback"   # nothing compiled before the first call
    out = sum_sq(xs)
    assert out == sum_sq_py(xs)                     # bit-for-bit
    assert binding_of(sum_sq).mode == "server"      # first call compiled + ran as WASM
    print(f"sum_sq: @wasm {_avg_ms(lambda: sum_sq(xs)):.3f} ms / py {_avg_ms(lambda: sum_sq_py(xs)):.3f} ms")


def test_typed_arrays_1d_and_2d_match_numpy():
    """Typed-array ABI: pass a NumPy buffer, get `== NumPy` bit-for-bit, 1-D and 2-D."""
    _require_toolchain()
    np = gate_import("numpy")

    a = np.arange(256, dtype=np.int32)
    o1 = np.zeros_like(a)
    scale_1d(a, o1, 5)
    assert binding_of(scale_1d).mode == "server"
    assert np.array_equal(o1, a * 5)

    rng = np.random.RandomState(3)
    img = rng.randint(0, 256, size=(16, 16), dtype=np.uint8)
    o2 = np.zeros_like(img)
    threshold_2d(img, o2, 16, 16, 128)
    assert binding_of(threshold_2d).mode == "server"
    assert np.array_equal(o2, np.where(img >= 128, 255, 0).astype(np.uint8))


@pytest.mark.timeout(60)  # if metering ever silently stops, fail as a RED, not a CI hang (needs pytest-timeout)
def test_fuel_sandbox_contains_a_runaway_kernel(request):
    """`@wasm(fuel=N)`: untrusted/runaway code TRAPS at the budget instead of hanging."""
    _require_toolchain()
    # the @timeout marker only guards a hang if pytest-timeout is installed; under REQUIRE_ORACLE a
    # missing plugin is a FAILURE (the hang-guard would otherwise silently no-op) -- see conftest.gate
    gate(request.config.pluginmanager.hasplugin("timeout"), "pytest-timeout required for the fuel hang-guard (pip install pythscribe[test])")
    assert spin(0) == 0                             # harmless; compiles + binds the sandbox
    assert binding_of(spin).mode == "server", "only run the runaway case if it is sandboxed"
    # specifically FuelExhausted (not any Exception) so a compile/marshalling error can't pass as "contained"
    with pytest.raises(FuelExhausted, match="fuel budget"):
        spin(1)                                     # infinite loop -- fuel-metered -> traps


def test_layout_version_abi_stamps_are_consistent():
    """The WASM-buffer ABI stamps must AGREE across every shipped copy (a mismatch would let a
    reader parse a buffer at the wrong width/stride). This is the publish 'LAYOUT_VERSION' check."""
    import pythscribe
    from pythscribe.runtime import LAYOUT_VERSION, array_buffer

    arr = array_buffer.ARRAY_LAYOUT_VERSION
    # every shipped .mjs copy of the buffer header must match the Python source of truth
    root = pathlib.Path(pythscribe.__file__).resolve().parent  # pythscribe/
    copies = sorted(root.rglob("list_buffer.mjs"))
    assert copies, "no shipped list_buffer.mjs copy found to cross-check"
    for p in copies:
        txt = p.read_text(encoding="utf-8")
        m_list = re.search(r'(?<![A-Z_])LAYOUT_VERSION\s*=\s*"([^"]+)"', txt)  # not the ARRAY_ one
        m_arr = re.search(r'ARRAY_LAYOUT_VERSION\s*=\s*"([^"]+)"', txt)
        assert m_list and m_list.group(1) == LAYOUT_VERSION, f"{p} list layout drift ({m_list and m_list.group(1)!r} != {LAYOUT_VERSION!r})"
        assert m_arr and m_arr.group(1) == arr, f"{p} array layout drift ({m_arr and m_arr.group(1)!r} != {arr!r})"


def test_gradio_adapter_resolves_the_compiled_bundle():
    """The Gradio adapter runs the SAME kernel in the browser: dispatch compiles-on-first-call and
    produces a bundle (no explicit build step)."""
    _require_toolchain()
    gate_import("gradio")
    from pythscribe.gradio import dispatch

    payload = dispatch(rms, [3.0, 4.0])
    assert payload["bundle"] is not None, "gradio dispatch did not produce a browser bundle"
    assert binding_of(rms).artifact_status == "resolved"
