"""M2.1 -- the Rust<->Python<->JS WASM ABI DRIFT GATE (spec 13-09-26-lib-selfcontained-pip-wheel,
requirements §5.6; validation §G-ABI-3). A SPOT through the SHIPPED compiler: build
`examples/hello.py` (scalar) and an `Array[...]` fixture with the shipped `pyths`, parse the
emitted module's `pyths.abi` section, and bind its labels to the runtime's DECLARED MIRRORS at
their K7 homes and to the three byte-identical `list_buffer.mjs` copies:

    section.abi          == runtime.abi.SUPPORTED_ABI_MAJOR == list_buffer.mjs::SUPPORTED_ABI_MAJOR
    section.list_layout  == runtime.LAYOUT_VERSION          == list_buffer.mjs::LAYOUT_VERSION
    section.array_layout == array_buffer.ARRAY_LAYOUT_VERSION == list_buffer.mjs::ARRAY_LAYOUT_VERSION
    section.history      == abi_golden.json (the committed pin of abi.rs::ABI_HISTORY)

Mutation controls (each verified RED by hand and recorded in the M2 commit): (i) bump
`abi.rs::LIST_LAYOUT_VERSION` alone + rebuild -> RED here (section != runtime); (ii) bump
`runtime/__init__.py::LAYOUT_VERSION` alone -> RED here AND K7 (`test_runtime.py::
test_r1_layout_constants_pin_the_shim`) RED; (iii) change a Rust string WITHOUT a new history row ->
the Rust `abi_history_last_row_is_current` test RED and the golden RED here; (iv) edit a PAST golden
row -> RED here. This gate binds LABELS across languages; the layout BYTES stay bound by the K7
byte-agreement tests + `bridge_test.rs::test_bridge_array_header_layout_byte_agreement`.
"""
from __future__ import annotations

import json
import re
from pathlib import Path

import pytest

from conftest import REPO, gate
from pythscribe._pin import COMPILER_VERSION
from pythscribe.build import BuildError, build_kernel, find_pyths
from pythscribe.ffi import SHIM
from pythscribe.runtime import LAYOUT_VERSION, abi, array_buffer, wasmtime_available

GOLDEN = Path(__file__).with_name("abi_golden.json")
ABI_RS = REPO / "crates" / "pyths_codegen_wasm" / "src" / "abi.rs"
SHIM_COPIES = [
    SHIM,
    SHIM.parent.parent / "gradio" / "wasm_function" / "frontend" / "list_buffer.mjs",
    SHIM.parent.parent / "streamlit" / "wasm_component" / "list_buffer.mjs",
]

HELLO_SRC = "def hello(x: float) -> float:\n    return x * 2.0 + 1.0\n"
ARRAY_SRC = (
    "def fill(a: Array[float64], n: int) -> float:\n"
    "    s = 0.0\n"
    "    for i in range(n):\n"
    "        a[i] = a[i] * 2.0\n"
    "        s = s + a[i]\n"
    "    return s\n"
)


def _require_compiler():
    try:
        return find_pyths()
    except BuildError as e:
        gate(False, f"shipped pyths compiler required for the drift gate ({e})")


def _build(tmp_path: Path, name: str, ksrc: str):
    src = tmp_path / f"{name}_mod.py"
    src.write_text(f"from pythscribe import wasm\n\n@wasm\n{ksrc}", encoding="utf-8")
    return build_kernel(src, name, ksrc, force=True, quiet=True)


def _shim_const(text: str, name: str):
    m = re.search(rf'^export const {name} = ("([^"]+)"|(\d+));', text, re.M)
    assert m, f"shim no longer declares {name} in the pinned form"
    return m.group(2) if m.group(2) is not None else int(m.group(3))


@pytest.fixture(scope="module")
def artifacts(tmp_path_factory):
    _require_compiler()
    d = tmp_path_factory.mktemp("abi_binding")
    return {"hello": _build(d, "hello", HELLO_SRC), "fill": _build(d, "fill", ARRAY_SRC)}


@pytest.mark.parametrize("which", ["hello", "fill"])
def test_shipped_section_binds_runtime_mirrors_and_every_shim_copy(artifacts, which):
    info = artifacts[which]
    section = abi.read_abi_section(info.wasm.read_bytes())
    # the runtime's declared mirrors (unchanged K7 homes)
    assert section["abi"] == abi.SUPPORTED_ABI_MAJOR
    assert section["list_layout"] == LAYOUT_VERSION
    assert section["array_layout"] == array_buffer.ARRAY_LAYOUT_VERSION
    assert section["compiler"] == COMPILER_VERSION
    # the three byte-identical shim copies (K7 pins identity; this pins the VALUES to the section)
    for shim in SHIM_COPIES:
        text = shim.read_text(encoding="utf-8")
        assert _shim_const(text, "SUPPORTED_ABI_MAJOR") == section["abi"], shim
        assert _shim_const(text, "LAYOUT_VERSION") == section["list_layout"], shim
        assert _shim_const(text, "ARRAY_LAYOUT_VERSION") == section["array_layout"], shim
    # and the runtime's own comparator accepts what the shipped compiler emitted
    assert abi.module_problem(info.wasm.read_bytes()) is None


def test_shipped_history_matches_the_committed_golden(artifacts):
    golden = json.loads(GOLDEN.read_text(encoding="utf-8"))
    section = abi.read_abi_section(artifacts["hello"].wasm.read_bytes())
    assert section["history"] == golden["history"], (
        "abi.rs::ABI_HISTORY drifted from tests/pythscribe/abi_golden.json -- a layout string changes "
        "ONLY by appending a new (major, list, array) row to BOTH; a past row is never edited"
    )
    assert golden["supported_abi_major"] == abi.SUPPORTED_ABI_MAJOR
    # the last row IS the current contract (a string change without a row is RED here too)
    last = section["history"][-1]
    assert last == [section["abi"], section["list_layout"], section["array_layout"]]
    # rows are immutable history: the v1 row is pinned literally
    assert golden["history"][0] == [1, "pyths-0.2.4-list-v1", "pyths-0.2.5-array-v2"]
    majors = [row[0] for row in golden["history"]]
    assert majors == sorted(majors) and len(set(majors)) == len(majors)


def test_glue_inlines_the_same_contract(artifacts):
    """The generated glue's `__PYTHS_ABI` (Rust-inlined) == the section (Rust-emitted) == the runtime."""
    for info in artifacts.values():
        glue = (info.dir / info.manifest["glue"]).read_text(encoding="utf-8")
        m = re.search(r'const __PYTHS_ABI = \{ abi: (\d+), list_layout: "([^"]+)", array_layout: "([^"]+)" \};', glue)
        assert m, "glue no longer inlines __PYTHS_ABI in the pinned form"
        assert (int(m.group(1)), m.group(2), m.group(3)) == (abi.SUPPORTED_ABI_MAJOR, LAYOUT_VERSION, array_buffer.ARRAY_LAYOUT_VERSION)
        assert 'WebAssembly.Module.customSections(module, "pyths.abi")' in glue
        # compile -> check -> instantiate (codex m2 blocker; the behavioral twin is §G-ABI-5)
        assert glue.index("__checkPythsAbi(__module);") < glue.index("WebAssembly.instantiate(__module, imports);") < glue.index("return __instance.exports;")
        assert "WebAssembly.instantiate(bytes" not in glue and "instantiateStreaming" not in glue


def test_exported_global_equals_the_major(artifacts):
    gate(wasmtime_available(), "wasmtime-py required to read the exported global")
    from pythscribe.runtime import ServerKernel

    k = ServerKernel.from_artifact(artifacts["hello"])
    assert abi.ABI_GLOBAL_EXPORT in k.exports
    inst = k.new_instance()
    assert inst.exports[abi.ABI_GLOBAL_EXPORT].value(inst.store) == abi.SUPPORTED_ABI_MAJOR
    assert k.abi["abi"] == abi.SUPPORTED_ABI_MAJOR
    k.close()


def test_rust_source_constants_match_the_shipped_binary(artifacts):
    """Dev-checkout SPOT: the abi.rs SOURCE agrees with what the shipped binary emitted (an edited
    abi.rs with a stale `target/release/pyths` is RED here -- rebuild the compiler)."""
    if not ABI_RS.is_file():
        pytest.skip("not a source checkout")
    src = ABI_RS.read_text(encoding="utf-8")
    section = abi.read_abi_section(artifacts["hello"].wasm.read_bytes())
    assert re.search(rf'pub const PYTHS_ABI_MAJOR: u32 = {section["abi"]};', src)
    assert re.search(rf'pub const LIST_LAYOUT_VERSION: &str = "{re.escape(section["list_layout"])}";', src)
    assert re.search(rf'pub const ARRAY_LAYOUT_VERSION: &str = "{re.escape(section["array_layout"])}";', src)
    for major, lst, arr in section["history"]:
        assert f'({major}, "{lst}", "{arr}")' in src, (major, lst, arr)


def test_range_pin_and_version_parser_spots():
    """Layer 2 SPOTs: the pinned range admits the pin and rejects the neighbours; parsing is total."""
    from pythscribe._pin import ACCEPTED_COMPILER_RANGE

    assert ACCEPTED_COMPILER_RANGE == ">=0.2.4,<0.3"
    assert abi.version_in_range(COMPILER_VERSION)
    for ok in ("0.2.4", "0.2.5", "0.2.5a0", "0.2.99", "v0.2.4"):
        assert abi.version_in_range(ok), ok
    for bad in ("0.2.3", "0.1.0", "0.3.0", "0.3", "1.0.0", "", "garbage", None, 0.24, "0.2.4.x"[:0]):
        assert not abi.version_in_range(bad), bad
    assert abi.parse_version("0.2.4") == (0, 2, 4) and abi.parse_version("0.2") == (0, 2, 0) and abi.parse_version("x") is None
    assert abi.compiler_version_problem("0.1.0") and "0.1.0" in abi.compiler_version_problem("0.1.0") and ACCEPTED_COMPILER_RANGE in abi.compiler_version_problem("0.1.0")
    assert abi.compiler_version_problem(COMPILER_VERSION) is None
    # the widened-range mutant (§G-ABI-3): `*`-style acceptance would admit 0.1.0 -- the pinned range does not
    assert abi.version_in_range("0.1.0", ">=0.0.0") is True and abi.version_in_range("0.1.0") is False
    with pytest.raises(ValueError):
        abi.version_in_range("0.2.4", "~=0.2")
