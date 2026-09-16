"""M2c: the browser shim's typed-array header/layout BYTE-AGREES with the server channel
(`pythscribe/runtime/array_buffer.py`) and the emitted glue (`bridge.rs::emit_array_helpers`),
and the two shim copies (ffi + Gradio frontend) are byte-identical. A drift on either side
goes RED here — the K7 declared-binding discipline extended to the array header."""
from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path

import pytest

from conftest import gate_node
from pythscribe.ffi import SHIM
from pythscribe.runtime import array_buffer as ab

_FRONTEND_SHIM = SHIM.parent.parent / "gradio" / "wasm_function" / "frontend" / "list_buffer.mjs"
# M3: the Streamlit component vendors a THIRD copy (it is a cross-context iframe that cannot
# reach into this package's source tree, exactly like the Gradio frontend copy).
_STREAMLIT_SHIM = SHIM.parent.parent / "streamlit" / "wasm_component" / "list_buffer.mjs"


def test_shim_twins_byte_identical():
    canonical = SHIM.read_bytes()
    assert canonical == _FRONTEND_SHIM.read_bytes(), (
        "pythscribe/ffi/list_buffer.mjs and the Gradio frontend copy must be byte-identical"
    )
    assert canonical == _STREAMLIT_SHIM.read_bytes(), (
        "pythscribe/ffi/list_buffer.mjs and the Streamlit component copy must be byte-identical"
    )


def test_all_shim_copies_are_pinned_no_eol_conversion():
    """Paired control for the byte-identity gate (review B2/SF8): EVERY `list_buffer.mjs` copy in
    the tree (found by GLOB, so a future 4th copy is caught too) must carry the `-text`
    gitattribute, so a default-autocrlf Windows clone cannot check one out as CRLF and silently
    break the identity gate / vendor a non-identical shim in the wheel."""
    import shutil
    import subprocess

    from conftest import REPO

    git = shutil.which("git")
    if git is None:
        pytest.skip("git not available")
    copies = sorted(REPO.glob("pythscribe/**/list_buffer.mjs"))
    assert len(copies) >= 3, f"expected >=3 shim copies (ffi + gradio + streamlit), found {copies}"
    assert _STREAMLIT_SHIM in copies and _FRONTEND_SHIM in copies and SHIM in copies
    rels = [str(p.relative_to(REPO)).replace("\\", "/") for p in copies]
    r = subprocess.run([git, "check-attr", "text", *rels], cwd=str(REPO), capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    lines = [ln for ln in r.stdout.splitlines() if ln.strip()]
    assert len(lines) == len(rels), r.stdout
    for ln in lines:
        assert ln.endswith(": text: unset"), f"a shim copy is not pinned -text (CRLF-safe): {ln}"


def test_shim_array_header_agrees_with_server_channel():
    gate_node()
    # read the shim's exported constants under Node (the values the browser marshals with)
    prog = (
        f"import * as m from {json.dumps(SHIM.as_uri())};\n"
        "const dt = {};\n"
        "for (const k of Object.keys(m.ARR_DTYPES)) dt[k] = { esize: m.ARR_DTYPES[k].esize, tag: m.ARR_DTYPES[k].tag, ctor: m.ARR_DTYPES[k].ctor.name };\n"
        "process.stdout.write(JSON.stringify({ ver: m.ARRAY_LAYOUT_VERSION, hdr: m.ARRAY_HEADER_BYTES, off: m.ARRAY_ELEM_OFFSET, dt }));\n"
    )
    node = shutil.which("node")
    r = subprocess.run([node, "--input-type=module", "-e", prog], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    got = json.loads(r.stdout)

    assert got["ver"] == ab.ARRAY_LAYOUT_VERSION, "array layout version drift (shim vs server channel)"
    assert got["hdr"] == ab.ARRAY_HEADER_BYTES == 16
    assert got["off"] == ab.ELEMENT_REGION_OFFSET == 16
    # per-dtype: tag == DTYPE_TAG, esize == DTYPE_ITEMSIZE; int64 uses BigInt64Array
    expect_ctor = {"int32": "Int32Array", "int64": "BigInt64Array", "float32": "Float32Array",
                   "float64": "Float64Array", "uint8": "Uint8Array"}
    assert set(got["dt"]) == set(ab.DTYPE_TAG)
    for d, tag in ab.DTYPE_TAG.items():
        assert got["dt"][d]["tag"] == tag, f"{d} tag drift: shim {got['dt'][d]['tag']} != server {tag}"
        assert got["dt"][d]["esize"] == ab.DTYPE_ITEMSIZE[d], f"{d} esize drift"
        assert got["dt"][d]["ctor"] == expect_ctor[d], f"{d} ctor drift"
