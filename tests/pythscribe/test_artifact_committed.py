"""Gates on the COMMITTED build outputs. These need neither node nor the compiler, so they
live outside the node-gated modules and can never be skipped by an unrelated switch
(review R2/S2). Two build outputs are committed: the demo artifact and the Svelte component
bundle; both must match their sources (R2/S3 for the bundle)."""
from __future__ import annotations

import re

import pytest

from conftest import DEMO_ARTIFACT_DIR, DEMO_DIR, REPO
from pythscribe._pin import COMPILER_VERSION
from pythscribe._static import find_wasm_defs, kernel_source, sha256_text
from pythscribe.artifacts import MANIFEST_NAME, verify
from pythscribe.build import _rewrite_runtime_imports

COMPONENT = REPO / "pythscribe" / "gradio" / "wasm_function"
BUILT_INDEX = COMPONENT / "backend" / "gradio_wasmfunction" / "templates" / "component" / "index.js"
SVELTE_SRC = COMPONENT / "frontend" / "Index.svelte"


def test_committed_demo_artifact_is_current():
    """FAILS (never skips) when the committed artifact does not verify against the committed
    kernel source + compiler pin -- e.g. after a CRLF checkout, a kernel edit without a
    rebuild, or a hand-edited manifest (review R1/B1)."""
    module_source = (DEMO_DIR / "kernels.py").read_text(encoding="utf-8")
    [node] = find_wasm_defs(module_source, "kernels.py")
    expected = sha256_text(kernel_source(module_source, node))
    assert (DEMO_ARTIFACT_DIR / MANIFEST_NAME).is_file(), "committed demo artifact is missing"
    info = verify(DEMO_ARTIFACT_DIR, function="rms_gain", expected_source_sha256=expected)
    assert info.manifest["compiler"]["version"] == COMPILER_VERSION
    for p in DEMO_ARTIFACT_DIR.rglob("*"):
        if p.is_file() and p.suffix in (".js", ".ps", ".json"):
            assert b"\r" not in p.read_bytes(), f"CRLF in committed artifact file {p}"
    # the user-facing line-ending guard the build lays beside the artifacts (R2/S9)
    assert (DEMO_ARTIFACT_DIR.parent / ".gitattributes").read_text(encoding="utf-8").strip().endswith("* -text")


M1_DIR = REPO / "examples" / "gradio-image-preprocess"

# Every example dir whose kernels ship committed __pythscribe__ artifacts is enumerated by
# `find_wasm_defs` over its kernels.py -- ONE gate for the class, no hardcoded kernel list (a
# kernel added to a covered file is gated automatically; a hand-maintained list silently
# missed `downscale_nn` -- opus fix-B r1/#6). Previously UN-GATED: a stale wasm-use-cases /
# streamlit-wasm golden could ship silently -- pre-M5 gate.
_EXAMPLE_DIRS = {
    "gradio-image-preprocess": REPO / "examples" / "gradio-image-preprocess",  # M1: box_scale, downscale_box, downscale_nn (M2c)
    "wasm-use-cases": REPO / "examples" / "wasm-use-cases",
    "streamlit-wasm": REPO / "examples" / "streamlit-wasm",
}
_EXAMPLE_CASES = []
for _label, _dir in _EXAMPLE_DIRS.items():
    _kp = _dir / "kernels.py"
    if _kp.is_file():
        for _node in find_wasm_defs(_kp.read_text(encoding="utf-8"), "kernels.py"):
            _EXAMPLE_CASES.append((_dir, _node.name))
# A renamed/removed kernels.py must go RED, not degrade the whole gate to an empty (skipped)
# parametrize -- the gate silently vanishing is exactly what pre-M5 is closing.
assert _EXAMPLE_CASES, f"no example @wasm kernels found to gate under {list(_EXAMPLE_DIRS)}"


@pytest.mark.parametrize(
    "example_dir,fn", _EXAMPLE_CASES, ids=[f"{d.name}:{fn}" for d, fn in _EXAMPLE_CASES]
)
def test_committed_example_artifacts_are_current(example_dir, fn):
    """Each committed example artifact verifies against its committed source + the compiler pin,
    and carries no CRLF. One gate for every example dir (M1, wasm-use-cases, streamlit-wasm)."""
    module_source = (example_dir / "kernels.py").read_text(encoding="utf-8")
    [node] = [n for n in find_wasm_defs(module_source, "kernels.py") if n.name == fn]
    expected = sha256_text(kernel_source(module_source, node))
    adir = example_dir / "__pythscribe__" / fn
    assert (adir / MANIFEST_NAME).is_file(), f"committed {example_dir.name} artifact {fn} is missing"
    info = verify(adir, function=fn, expected_source_sha256=expected)
    assert info.manifest["compiler"]["version"] == COMPILER_VERSION
    for p in adir.rglob("*"):
        if p.is_file() and p.suffix in (".js", ".ps", ".json"):
            assert b"\r" not in p.read_bytes(), f"CRLF in committed artifact file {p}"


def test_committed_m1_test_images_match_manifest():
    import hashlib
    import json

    manifest = json.loads((M1_DIR / "test_images" / "manifest.json").read_text(encoding="utf-8"))
    assert len(manifest) >= 3
    total = 0
    for name, meta in manifest.items():
        data = (M1_DIR / "test_images" / name).read_bytes()
        assert hashlib.sha256(data).hexdigest() == meta["sha256"], name
        assert len(data) == meta["bytes"]
        total += len(data)
    assert total < 4 * 1024 * 1024, "keep the committed SPOT set small"


def test_committed_component_bundle_carries_the_m1_image_path():
    """A forgotten `gradio cc build` after editing Index.svelte / the shim copy goes RED here.
    The shipped bundle is the WHOLE component dir: vite splits the shim into its own chunk
    (`list_buffer-<hash>.js`), so the shim-derived markers are looked up across every chunk, not
    only `index.js` (which never carried them -- a pre-existing stale read fixed in M2)."""
    js = "\n".join(p.read_text(encoding="utf-8") for p in sorted(BUILT_INDEX.parent.glob("*.js")))
    for marker in (
        "pythscribe ffi: unsupported WASM import", "readBack", "upload-original", "browser-wasm", "gradio.FileData", "wasm-file",
        # the review-round invariants the SHIPPED bundle must carry (opus r2/NS-5): exact int ingress,
        # exact i64 read-back, raw decode options, the in-tab pixel cap, per-run resource timing
        "Number.isSafeInteger", "not a safe integer", "9223372036854775807", "imageOrientation", "colorSpaceConversion",
        "in-tab cap", "startTime", "i64 overflow inside the WASM kernel",
    ):
        assert marker in js, f"built component lacks M1 marker {marker!r}; run `gradio cc build`"
    # and the shim's own distinguishing constants, straight from its source
    shim = (REPO / "pythscribe" / "ffi" / "list_buffer.mjs").read_text(encoding="utf-8")
    m = re.search(r'LAYOUT_VERSION = "([^"]+)"', shim)
    assert m, "the shim no longer declares LAYOUT_VERSION in the pinned form"
    assert m.group(1) in js, "built component bundle does not carry the shim's LAYOUT_VERSION; run `gradio cc build`"
    # M2.1: the bundled shim copy performs the WASM ABI check in the tab (spec 13-09-26 §5.6). The
    # shim is split into its own chunk by vite, so look across every shipped chunk. A bundle built
    # before the check (or a forgotten `gradio cc build` after editing the shim) is RED here.
    chunks = "\n".join(p.read_text(encoding="utf-8") for p in BUILT_INDEX.parent.glob("*.js"))
    for marker in ("pyths.abi", "WASM ABI mismatch on", "AbiMismatchError", "customSections"):
        assert marker in chunks, f"built component bundle lacks the M2.1 ABI-check marker {marker!r}; run `gradio cc build`"


def test_gitattributes_pins_artifact_line_endings():
    ga = (REPO / ".gitattributes").read_text(encoding="utf-8")
    assert "examples/**/__pythscribe__/** -text" in ga
    assert "pythscribe/gradio/wasm_function/backend/**/templates/** -text" in ga


def test_import_rewrite_touches_only_specifiers():
    """Review R1/SF13: only import/export specifiers are rewritten, never string literals,
    and an already-suffixed subpath is not double-suffixed."""
    assert _rewrite_runtime_imports('import { a } from "pyths-runtime";') == 'import { a } from "./pyths-runtime/index.js";'
    assert _rewrite_runtime_imports("import 'pyths-runtime/web';") == "import './pyths-runtime/web.js';"
    assert _rewrite_runtime_imports('await import("pyths-runtime/stdlib/math")') == 'await import("./pyths-runtime/stdlib/math.js")'
    assert _rewrite_runtime_imports('from "pyths-runtime/stdlib/math.js"') == 'from "./pyths-runtime/stdlib/math.js"'
    assert _rewrite_runtime_imports('export * from "pyths-runtime/web"') == 'export * from "./pyths-runtime/web.js"'
    assert _rewrite_runtime_imports('const s = "pyths-runtime";') == 'const s = "pyths-runtime";'
    assert _rewrite_runtime_imports("x = ['pyths-runtime/web']") == "x = ['pyths-runtime/web']"


# Every load-bearing string in Index.svelte that the built bundle must still carry. A forgotten
# `gradio cc build` after a source edit makes this RED (the committed bundle is what E2E and a
# `pip install` of the component actually run -- R2/S3).
SVELTE_INVARIANTS = [
    "browser-wasm",
    "performance",  # resource-timing evidence of the .wasm fetch
    "bundle did not load/run",  # the with_timeout message
    "BigInt result cannot cross",
    "bundle exports no function named",
]


def test_committed_component_bundle_matches_svelte_source():
    src = SVELTE_SRC.read_text(encoding="utf-8")
    built = BUILT_INDEX.read_text(encoding="utf-8")
    for s in SVELTE_INVARIANTS:
        assert s in src, f"invariant string no longer in Index.svelte: {s!r}"
        assert s in built, f"committed bundle is stale (missing {s!r}); run `gradio cc build`"
    # the nonce guard (still_current) survives minification as a `.nonce ===` / `=== x.nonce` compare
    assert re.search(r"nonce\s*===|===\s*[\w$.]+\.nonce", built), "nonce guard missing from the built bundle"
    timeout_ms = int(re.search(r"RUN_TIMEOUT_MS\s*=\s*(\d+)", src).group(1))
    # the minifier may write 15000 as 15e3
    forms = {str(timeout_ms), f"{timeout_ms // 1000}e3" if timeout_ms % 1000 == 0 else str(timeout_ms)}
    assert any(re.search(rf"(?<![\w.]){re.escape(f)}(?![\w.])", built) for f in forms), forms
