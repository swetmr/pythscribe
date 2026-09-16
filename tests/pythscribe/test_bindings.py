"""The cross-consumer HOST-IMPORT binding, with NO wasmtime / node / browser gate (opus m1.5
r3/N5): the codegen's `math.*` import list (bridge.rs) and its arities (emit.rs), the server's
Python twin (`_jsmath.HOST_MATH`), the browser shim's explicit table (`list_buffer.mjs`), the
component's byte-identical shim copy, and the BUILT component bundle (what the tab actually
runs) must all name EXACTLY the same 16 functions. A forgotten `gradio cc build` after editing
the shim goes RED here (the bundle's key SET is compared, not one entry -- r3/N6).

The built bundle is no longer ONE file (v0.2.5 M0 shim-split, B-1): Vite emits the shim as a
shared chunk `list_buffer-*.js`, statically imported by `index.js` (image/scalar users) AND by the
lazy `flow_island-*.js` island chunk. The bundle gate therefore scans EVERY built `*.js` and
requires the table in exactly ONE chunk that both consumers import (a dangling chunk = vacuous).
"""
from __future__ import annotations

import re

from conftest import REPO
from pythscribe.ffi import SHIM
from pythscribe.runtime import HOST_FUNCTIONS
from pythscribe.runtime._jsmath import HOST_MATH

COMPONENT = REPO / "pythscribe" / "gradio" / "wasm_function" / "backend" / "gradio_wasmfunction" / "templates" / "component"
SHIM_COPY = REPO / "pythscribe" / "gradio" / "wasm_function" / "frontend" / "list_buffer.mjs"


def test_codegen_import_list_and_arities_match_the_server_twin():
    bridge = (REPO / "crates" / "pyths_codegen_wasm" / "src" / "bridge.rs").read_text(encoding="utf-8")
    body = bridge.split("fn math_import_js(name: &str)")[1].split("_ => return None")[0]
    names = tuple(sorted(re.findall(r'"([a-z0-9]+)" => "Math\.', body)))
    assert names == HOST_FUNCTIONS == tuple(sorted(HOST_MATH)), (names, HOST_FUNCTIONS)
    emit = (REPO / "crates" / "pyths_codegen_wasm" / "src" / "emit.rs").read_text(encoding="utf-8")
    table = emit.split("pub const MATH_FUNCTIONS")[1].split("];")[0]
    arities = dict((n, int(a)) for n, a in re.findall(r'\("([a-z0-9]+)",\s*(\d)\)', table))
    assert arities, "MATH_FUNCTIONS table not found"
    for name, (arity, _) in HOST_MATH.items():
        assert arities.get(name) == arity, (name, arity, arities.get(name))


def _shim_table_names(text: str) -> tuple[str, ...]:
    table = text.split("export const HOST_MATH = {")[1].split("};")[0]
    return tuple(sorted(re.findall(r"\b([a-z0-9]+):\s*Math\.[a-z0-9]+", table)))


def test_browser_shim_resolves_the_same_names_through_an_explicit_table():
    shim = SHIM.read_text(encoding="utf-8")
    assert _shim_table_names(shim) == HOST_FUNCTIONS
    assert "fabs: Math.abs" in shim and "Math[imp.name]" not in shim  # never a name lookup (math.fabs is Math.abs)
    assert SHIM_COPY.read_bytes() == SHIM.read_bytes()  # K8: the component's copy is byte-identical


_TABLE_RE = re.compile(r"\{([^{}]*\bfabs\s*:\s*Math\.abs[^{}]*)\}")
_BY_NAME_RE = re.compile(r"Math\[[a-zA-Z_$.]+\.name\]")


def _imports_chunk(text: str, chunk: str) -> bool:
    return re.search(r'from\s+["\']\./' + re.escape(chunk) + r'["\']', text) is not None


def test_built_component_bundle_carries_the_whole_table():
    """The shim's HOST_MATH table must live in EXACTLY ONE built chunk, with key set == HOST_FUNCTIONS,
    no chunk may resolve math imports by name, and that chunk must be reachable from BOTH `index.js`
    (the image/scalar entry) and the `flow_island-*.js` island (the callback path) -- so the binding
    cannot go vacuous on a dangling chunk. RED if the table is missing, duplicated, incomplete, in a
    chunk nobody imports, or if any chunk falls back to a `Math[...name]` lookup."""
    built = {p.name: p.read_text(encoding="utf-8", errors="ignore") for p in sorted(COMPONENT.glob("*.js"))}
    assert "index.js" in built, f"no built index.js under {COMPONENT}; run `gradio cc build`"
    # the minified object that contains `fabs:Math.abs` IS the table: take its full key set
    hits = {n: m for n, m in ((n, _TABLE_RE.search(t)) for n, t in built.items()) if m}
    assert len(hits) == 1, (
        f"shim HOST_MATH table expected in exactly ONE built chunk, found in {sorted(hits)}; run `gradio cc build`"
    )
    (chunk, m), = hits.items()
    assert _TABLE_RE.search(built[chunk], m.end()) is None, f"HOST_MATH table appears twice in {chunk}"
    keys = tuple(sorted(re.findall(r"\b([a-z0-9]+)\s*:\s*Math\.[a-z0-9]+", m.group(1))))
    assert keys == HOST_FUNCTIONS, (keys, "stale bundle: run `gradio cc build`")
    for name, text in built.items():
        assert not _BY_NAME_RE.search(text), f"built chunk {name} still resolves math imports by name"
    if chunk != "index.js":  # the shim-split: the shared chunk must be reachable from the entry AND the island
        assert _imports_chunk(built["index.js"], chunk), f"index.js does not import the shim chunk {chunk}"
        islands = [n for n in built if n.startswith("flow_island-")]
        assert islands, "no flow_island-*.js chunk built (island missing; run `gradio cc build`)"
        for isl in islands:
            assert _imports_chunk(built[isl], chunk), f"{isl} does not import the shim chunk {chunk}"


def test_bundle_table_gate_negative_controls():
    """Paired controls for the bundle gate's own parsers (anti-vacuity): the table regex finds the
    real table in the real chunk set exactly once; a text mutant that DUPLICATES the table into
    index.js is detected as two hits; a mutant that swaps one entry for a by-name lookup trips the
    negative; a mutant that drops the shim import from index.js is detected as unreachable."""
    built = {p.name: p.read_text(encoding="utf-8", errors="ignore") for p in COMPONENT.glob("*.js")}
    hits = {n: m for n, m in ((n, _TABLE_RE.search(t)) for n, t in built.items()) if m}
    assert len(hits) == 1
    (chunk, m), = hits.items()
    table = m.group(0)
    # duplicate mutant: the table pasted into index.js -> 2 hits
    dup = dict(built)
    dup["index.js"] = dup["index.js"] + "\n" + table
    assert sum(1 for t in dup.values() if _TABLE_RE.search(t)) == 2
    # by-name mutant: one explicit entry replaced by `Math[imp.name]` -> the negative fires
    assert _BY_NAME_RE.search(table.replace("Math.abs", "Math[imp.name]", 1))
    # unreachable mutant: index.js no longer imports the chunk -> the reachability check fires
    if chunk != "index.js":
        assert _imports_chunk(built["index.js"], chunk)
        stripped = re.sub(r'from\s+["\']\./' + re.escape(chunk) + r'["\']', 'from "./elsewhere.js"', built["index.js"])
        assert not _imports_chunk(stripped, chunk)
