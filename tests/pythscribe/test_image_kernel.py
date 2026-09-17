"""M1 kernel gates (no browser needed): the compiled `downscale_box`, run through the SAME
list-buffer FFI shim the component uses (under Node), must equal (K1) an independent NumPy
box reference EXACTLY, (K2) Pillow's rounded box filter within +-1, and (K3) the plain-Python
kernel (the shipped fallback) exactly -- on the committed SPOT images and on a Hypothesis
sweep of random images/factors. Paired negative controls, each provably RED:
  K4 a seeded off-by-one in the kernel (compiled from mutated source) is REFUSED by K1;
  K5 a poisoned JS twin in the glue changes NOTHING on the shim path (the twin is not on
     that path) while the same poison IS observable on the glue path (so the control is
     not vacuous): resolution, not behaviour-only;
  K6 a corrupt .wasm makes the shim THROW (never a silent fallback);
  K7 the glue's list marshaller text equals the layout the shim pins (the declared binding);
  K8 the component's shim copy is byte-identical to the canonical one.
"""
from __future__ import annotations

import re
import shutil
import sys
import tempfile
from pathlib import Path

import numpy as np
import pytest
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from conftest import REPO, gate, gate_node, import_module_from
from pythscribe import binding_of
from pythscribe.build import build_module
from pythscribe.ffi import SHIM, FfiError, run_kernel, signature_of

IMG_DIR = REPO / "examples" / "gradio-image-preprocess"
TEST_IMAGES = sorted((IMG_DIR / "test_images").glob("*.jpg"))
sys.path.insert(0, str(IMG_DIR))
import reference as R  # noqa: E402

MAX_DIM = 512


@pytest.fixture(autouse=True, scope="module")
def _need_node():
    gate_node()


@pytest.fixture(scope="module")
def kernels():
    return import_module_from(IMG_DIR / "kernels.py", "kernels_image_for_tests")


@pytest.fixture(scope="module")
def down(kernels):
    b = binding_of(kernels.downscale_box)
    gate(b.artifact is not None, f"downscale_box artifact not usable ({b.artifact_status}); run `python -m pythscribe.build examples/gradio-image-preprocess/kernels.py`")
    return b


@pytest.fixture(scope="module")
def scale_b(kernels):
    b = binding_of(kernels.box_scale)
    gate(b.artifact is not None, f"box_scale artifact not usable ({b.artifact_status})")
    return b


def shim_downscale(wasm: Path, rgb: np.ndarray, scale: int, *, param_types=None) -> tuple[int, np.ndarray, dict]:
    """Run the kernel through the shim on `rgb`; returns (returned count, HxWx3 uint8, raw)."""
    h, w = rgb.shape[:2]
    ow, oh = w // scale, h // scale
    d = Path(tempfile.mkdtemp(prefix="m1shim_"))
    try:
        inp, outp = d / "in.i32", d / "out.i32"
        R.pack(rgb).astype("<i4").tofile(inp)
        [r] = run_kernel(
            wasm, "downscale_box", param_types or ["list[int]", "int", "int", "int", "list[int]"],
            [{"args": [{"i32_file": str(inp)}, w, h, scale, {"zeros": ow * oh}], "read_back": [4], "out_files": {"4": str(outp)}}],
        )
        if not r["ok"]:
            raise FfiError(r["error"])
        out = np.fromfile(outp, dtype="<i4")
        return int(r["value"]), R.unpack(out, ow, oh), r
    finally:
        shutil.rmtree(d, ignore_errors=True)


def shim_scale(wasm: Path, w: int, h: int, max_dim: int) -> int:
    [r] = run_kernel(wasm, "box_scale", ["int", "int", "int"], [{"args": [w, h, max_dim]}])
    assert r["ok"], r
    return int(r["value"])


# ----------------------------------------------------------------------------- positives
@pytest.mark.parametrize("image", TEST_IMAGES, ids=[p.name for p in TEST_IMAGES])
def test_k1_k2_spot_images_shim_equals_numpy_exactly_and_pillow_within_1(down, scale_b, image):
    rgb = R.load_rgb(image)
    h, w = rgb.shape[:2]
    scale = shim_scale(scale_b.artifact.wasm, w, h, MAX_DIM)
    assert scale == R.choose_scale(w, h, MAX_DIM) and max(w // scale, h // scale) <= MAX_DIM
    n, got, raw = shim_downscale(down.artifact.wasm, rgb, scale)
    ref = R.box_reference(rgb, scale)
    assert n == ref.shape[0] * ref.shape[1]
    assert got.shape == ref.shape
    assert np.array_equal(got, ref), f"{image.name}: max |diff| {int(np.abs(got.astype(int) - ref.astype(int)).max())}"
    pil = R.pillow_reduce(rgb, scale)
    assert int(np.abs(got.astype(int) - pil.astype(int)).max()) <= 1
    assert R.checksum_rgb(got) == R.checksum_rgb(ref)
    assert "downscale_box" in raw["exports"] and raw["how"] == "node-file"


@pytest.mark.parametrize("w,h,scale", [(5, 5, 2), (641, 481, 2), (13, 7, 3), (64, 48, 8)])
def test_k2b_pillow_oracle_on_the_kernels_domain_non_divisible(down, w, h, scale):
    """Non-divisible SPOTs (opus r2/NS-1): the kernel drops the trailing partial block, and so
    must the Pillow oracle (`Image.reduce` alone would average it into an extra row/column).
    Reverting the crop in `pillow_reduce` makes the shape assertion go RED here."""
    rgb = np.random.default_rng(w * 1000 + h).integers(0, 256, (h, w, 3), dtype=np.uint8)
    n, got, _ = shim_downscale(down.artifact.wasm, rgb, scale)
    ref = R.box_reference(rgb, scale)
    pil = R.pillow_reduce(rgb, scale)
    assert got.shape == ref.shape == pil.shape == (h // scale, w // scale, 3)
    assert n == (h // scale) * (w // scale)
    assert np.array_equal(got, ref)
    assert int(np.abs(got.astype(int) - pil.astype(int)).max()) <= 1
    # and the UNcropped Pillow reduce is indeed a different domain (the control is not vacuous)
    from PIL import Image

    raw = np.asarray(Image.fromarray(rgb, "RGB").reduce(scale))
    assert raw.shape != ref.shape or (w % scale == 0 and h % scale == 0)


def test_k3_python_fallback_equals_reference(kernels, down):
    """The plain-Python kernel (what runs server-side without an artifact) is the same spec."""
    rgb = R.load_rgb(TEST_IMAGES[0])
    h, w = rgb.shape[:2]
    scale = binding_of(kernels.box_scale).run_python(w, h, MAX_DIM)  # the Python body, explicitly (M1.5)
    ow, oh = w // scale, h // scale
    px = R.pack(rgb).ravel().tolist()
    out = [0] * (ow * oh)
    before = down.calls()
    n = down.run_python(px, w, h, scale, out)  # the PYTHON body explicitly (M1.5: a bare call takes the server path when wasmtime is installed)
    assert down.calls() == before + 1  # the binding counted the Python run (the path marker)
    assert n == ow * oh
    assert np.array_equal(R.unpack(np.asarray(out), ow, oh), R.box_reference(rgb, scale))


@settings(max_examples=40, deadline=None, suppress_health_check=[HealthCheck.too_slow])
@given(
    w=st.integers(1, 48), h=st.integers(1, 48), scale=st.integers(1, 9),
    seed=st.integers(0, 2**31 - 1),
)
def test_k1_sweep_shim_equals_numpy_and_python(down, kernels, w, h, scale, seed):
    """Random images x random box factors: shim (WASM) == NumPy == plain Python, exactly.
    Sizes not divisible by `scale` exercise the dropped-tail clause of the contract."""
    if w // scale == 0 or h // scale == 0:
        scale = 1
    rgb = np.random.default_rng(seed).integers(0, 256, (h, w, 3), dtype=np.uint8)
    n, got, _ = shim_downscale(down.artifact.wasm, rgb, scale)
    ref = R.box_reference(rgb, scale)
    assert n == ref.shape[0] * ref.shape[1]
    assert np.array_equal(got, ref)
    ow, oh = w // scale, h // scale
    out = [0] * (ow * oh)
    assert down.run_python(R.pack(rgb).ravel().tolist(), w, h, scale, out) == n  # the Python body, explicitly (M1.5)
    assert np.array_equal(R.unpack(np.asarray(out), ow, oh), ref)


@settings(max_examples=60, deadline=None)
@given(w=st.integers(1, 20000), h=st.integers(1, 20000), max_dim=st.integers(1, 4096))
def test_box_scale_sweep(scale_b, w, h, max_dim):
    s = shim_scale(scale_b.artifact.wasm, w, h, max_dim)
    assert s == R.choose_scale(w, h, max_dim)
    assert max(w // s, h // s) <= max_dim and s >= 1


# ----------------------------------------------------------------------------- negative controls
def _build_variant(tmp_path: Path, source: str, name: str = "downscale_box") -> Path:
    tmp_path.mkdir(parents=True, exist_ok=True)
    src = tmp_path / "kernels.py"
    src.write_text(source, encoding="utf-8")
    arts = build_module(src, quiet=True)
    [a] = [a for a in arts if a.function == name]
    return a.wasm


def test_k4_seeded_off_by_one_kernel_is_refused_by_the_differential(tmp_path):
    """Mutant: `area` is one too small in the kernel -> K1 must go RED on the SPOT image."""
    source = (IMG_DIR / "kernels.py").read_text(encoding="utf-8")
    mutated = source.replace("area = scale * scale\n", "area = scale * scale - 1\n")
    assert mutated != source
    wasm = _build_variant(tmp_path, mutated)
    rgb = R.load_rgb(TEST_IMAGES[0])
    h, w = rgb.shape[:2]
    scale = R.choose_scale(w, h, MAX_DIM)
    _, got, _ = shim_downscale(wasm, rgb, scale)
    ref = R.box_reference(rgb, scale)
    assert not np.array_equal(got, ref)  # the differential SEES the mutation
    assert R.checksum_rgb(got) != R.checksum_rgb(ref)


def test_k4b_index_off_by_one_is_refused(tmp_path):
    source = (IMG_DIR / "kernels.py").read_text(encoding="utf-8")
    mutated = source.replace("v = px[row + ox * scale + dx]", "v = px[row + ox * scale + dx + 1]")
    assert mutated != source
    wasm = _build_variant(tmp_path, mutated)
    rgb = np.random.default_rng(1).integers(0, 256, (16, 24, 3), dtype=np.uint8)
    try:
        _, got, _ = shim_downscale(wasm, rgb, 4)
    except FfiError as e:
        # an out-of-bounds read trapped INSIDE the kernel: refused loudly, also RED. The error
        # must come from the WASM run, not from a build/loader failure (opus r1 nit).
        assert "RuntimeError" in str(e) or "unreachable" in str(e) or "out of bounds" in str(e) or "IndexError" in str(e), str(e)
        return
    assert not np.array_equal(got, R.box_reference(rgb, 4))


def test_k5_poisoned_js_twin_is_not_on_the_shim_path(down, tmp_path):
    """RESOLUTION marker: the shim calls the WASM export directly, so poisoning the glue's JS
    twin cannot change its output. Positive half of the control: the same poison IS
    observable through the glue's wrapper (forced onto the twin by an out-of-i64-range arg),
    proving the twin is real and reachable -- the control cannot pass vacuously."""
    glue_src = (down.artifact.dir / "downscale_box.glue.js").read_text(encoding="utf-8")
    # poison: the twin returns -1 instead of ow*oh
    poisoned = re.sub(r"(function downscale_box\(px, w, h, scale, out\) \{)", r"\1\n    return -1;", glue_src, count=1)
    assert poisoned != glue_src
    adir = tmp_path / "poisoned"
    shutil.copytree(down.artifact.dir, adir)
    (adir / "downscale_box.glue.js").write_text(poisoned, encoding="utf-8", newline="\n")
    # positive half: via the glue wrapper, `scale = 2**40` makes `area = scale * scale`
    # overflow i64 inside WASM -> the glue sets __ovf and re-runs the call on the JS twin.
    # Unpoisoned glue answers 0 (ow*oh == 0); the poisoned twin answers -1.
    from pythscribe.build.runner import run_artifact

    call = [[[0, 0, 0, 0], 2, 2, 2**40, []]]
    [clean] = run_artifact(down.artifact.entry, "downscale_box", call)
    assert clean["ok"] and clean["value"] == 0, clean
    [r] = run_artifact(adir / "downscale_box.js", "downscale_box", call)
    assert r["ok"] and r["value"] == -1, r  # the twin IS reachable through the glue
    # negative half: the shim path on the SAME poisoned directory is unaffected
    rgb = np.random.default_rng(3).integers(0, 256, (12, 16, 3), dtype=np.uint8)
    n, got, _ = shim_downscale(adir / "downscale_box.wasm", rgb, 2)
    assert n == 6 * 8 and np.array_equal(got, R.box_reference(rgb, 2))


@pytest.mark.parametrize("how", ["bad-magic", "truncated-code", "flipped-code-byte"])
def test_k6_corrupt_wasm_throws_never_falls_back(down, tmp_path, how):
    """Deterministic corruptions (review r1/S1: a flipped byte in an unused section would
    not create the claimed world): a bad magic number, a truncated module, and a flipped
    byte INSIDE the code section (located via the section header) must all make the shim
    throw -- there is no JS twin to fall back on."""
    data = bytearray(down.artifact.wasm.read_bytes())
    if how == "bad-magic":
        data[0:4] = b"\x00asn"
    elif how == "truncated-code":
        data = data[: len(data) * 2 // 3]
    else:
        # walk the sections to find the code section (id 10) and flip a byte in its body
        i = 8
        while i < len(data):
            sid = data[i]
            i += 1
            size, shift = 0, 0
            while True:  # LEB128
                b = data[i]
                i += 1
                size |= (b & 0x7F) << shift
                shift += 7
                if not b & 0x80:
                    break
            if sid == 10:
                data[i + size // 2] ^= 0xFF
                break
            i += size
        else:  # pragma: no cover
            pytest.fail("no code section found")
    bad = tmp_path / "downscale_box.wasm"
    bad.write_bytes(bytes(data))
    rgb = np.random.default_rng(5).integers(0, 256, (8, 8, 3), dtype=np.uint8)
    try:
        _, got, _ = shim_downscale(bad, rgb, 2)
    except FfiError:
        return  # refused loudly (compile/validation/trap) -- RED as required
    # a flipped code byte that still validates must at least change the output; a silently
    # correct result would mean the corruption did not reach the kernel
    assert not np.array_equal(got, R.box_reference(rgb, 2)), how


def test_k6b_shim_refuses_wrong_arity_and_types(down):
    rgb = np.zeros((4, 4, 3), dtype=np.uint8)
    with pytest.raises(FfiError):
        shim_downscale(down.artifact.wasm, rgb, 2, param_types=["list[int]", "int", "int", "int"])  # arity
    [r] = run_kernel(down.artifact.wasm, "downscale_box", ["list[str]", "int", "int", "int", "list[int]"],
                     [{"args": [[1], 1, 1, 1, [0]]}])
    assert not r["ok"] and "unsupported param type" in r["error"]
    [r] = run_kernel(down.artifact.wasm, "nope", ["int"], [{"args": [1]}])
    assert not r["ok"] and "exports no function" in r["error"]


_GLUE_LIST_TO_WASM = """function __list_to_wasm(arr, kind) {
  const n = arr.length;
  const esize = kind === 'i32' ? 4 : 8;
  const ptr = __wasm.__alloc(8 + n * esize);
  const view = new DataView(__wasm.memory.buffer);
  view.setInt32(ptr, n, true);
  view.setInt32(ptr + 4, n, true);
  for (let i = 0; i < n; i++) {
    const off = ptr + 8 + i * esize;
    if (kind === 'f64') view.setFloat64(off, arr[i], true);
    else if (kind === 'i64') {
      const b = typeof arr[i] === 'bigint' ? arr[i] : BigInt(Math.trunc(arr[i]));
      if (b > 9223372036854775807n || b < -9223372036854775808n) throw new RangeError('OverflowError: list element exceeds the i64 range of the WASM fast path');
      view.setBigInt64(off, b, true);
    }
    else view.setInt32(off, arr[i] | 0, true);
  }
  return ptr;
}
"""

_GLUE_LIST_FROM_WASM = """function __list_from_wasm(ptr, kind) {
  const view = new DataView(__wasm.memory.buffer);
  const n = view.getInt32(ptr, true);
  const esize = kind === 'i32' ? 4 : 8;
  const out = new Array(n);
  for (let i = 0; i < n; i++) {
    const off = ptr + 8 + i * esize;
    if (kind === 'f64') out[i] = view.getFloat64(off, true);
    else if (kind === 'i64') { const v = view.getBigInt64(off, true); out[i] = (v >= -9007199254740991n && v <= 9007199254740991n) ? Number(v) : v; }
    else out[i] = view.getInt32(off, true);
  }
  return out;
}
"""


def test_k7_layout_binding_glue_matches_shim(down):
    """The declared binding: the compiler's marshallers, as emitted into THIS artifact's
    glue, must be EXACTLY the [len:i32 LE][cap:i32 LE][i64 LE | f64 LE | i32 elements at
    ptr+8+i*esize] layout the shim writes and reads (the full function bodies, incl. the
    setBigInt64/getBigInt64 little-endian element writer/reader -- review r1/B7). A
    compiler-side layout change goes RED here, not in a corrupted buffer."""
    glue = (down.artifact.dir / "downscale_box.glue.js").read_text(encoding="utf-8")
    assert _GLUE_LIST_TO_WASM in glue, "the glue's __list_to_wasm no longer matches the pinned layout"
    assert _GLUE_LIST_FROM_WASM in glue, "the glue's __list_from_wasm no longer matches the pinned layout"
    assert '__list_to_wasm(px, "i64")' in glue and '__list_to_wasm(out, "i64")' in glue
    shim = SHIM.read_text(encoding="utf-8")
    assert "HEADER_BYTES = 8" in shim and '"list[int]": 8' in shim and '"list[float]": 8' in shim and '"list[bool]": 4' in shim
    # LAYOUT_VERSION is the LIST layout's compat IDENTITY -- it names the pin at which the list
    # layout LAST CHANGED (a compat key), NOT the current compiler version: arrays changed in 0.2.5
    # (-> "...-array-v2") but the list layout is unchanged since 0.2.4, so it stays "pyths-0.2.4-list-v1".
    # Auto-bumping it with the compiler pin would falsely declare an incompatible layout (a 0.2.5 runtime
    # would reject a byte-identical 0.2.4 artifact). The ONE authority is the compiler's abi.rs
    # LIST_LAYOUT_VERSION; the shim MUST mirror it exactly (the runtime refuses a mismatch at load), so
    # bind to that here -- a real layout change goes RED, a mere pin bump does not (opus r1/S2).
    _abi_rs = (REPO / "crates" / "pyths_codegen_wasm" / "src" / "abi.rs").read_text(encoding="utf-8")
    _list_layout = re.search(r'LIST_LAYOUT_VERSION: &str = "([^"]+)"', _abi_rs).group(1)
    assert f'LAYOUT_VERSION = "{_list_layout}"' in shim, \
        f"shim LAYOUT_VERSION must mirror the compiler's abi.rs LIST_LAYOUT_VERSION ({_list_layout!r})"
    assert "dv.setInt32(p, n, true)" in shim and "dv.setInt32(p + 4, n, true)" in shim  # the header the glue writes
    assert "new BigInt64Array(ex.memory.buffer, p + HEADER_BYTES, n)" in shim  # i64 elements, platform (LE) order
    # #484 (SYMMETRIC marshalling): the glue now DOES copy mutable list params
    # back on return -- via `__list_write_back` (NOT `__list_from_wasm`, which is
    # the list-RETURN path #364 still refuses). The shim is kept belt-and-
    # suspenders for its DISTINCT guarantee the glue deliberately does NOT give:
    # it refuses on overflow/trap instead of masking with the JS twin (the
    # dual-track-masking guard, K5). So write-back is present, list-return is not.
    wrapper = glue.split("export function downscale_box")[1]
    assert "__list_from_wasm(" not in wrapper, "list-RETURN path must stay refused (#364)"
    assert 'const __wb_arg_4 = __list_to_wasm(out, "i64")' in wrapper
    assert '__list_write_back(out, __wb_arg_4, "i64")' in wrapper, "out-param must be written back (#484)"
    assert "function __list_write_back(arr, ptr, kind)" in glue


def test_k7b_shim_and_glue_agree_on_the_bytes(down, tmp_path):
    """Behavioural half of the binding (not just text): the SAME list written by the glue's
    marshaller and by the shim must produce byte-identical linear memory. Runs the glue's
    __list_to_wasm (extracted from the artifact) and the shim side by side under Node."""
    glue = (down.artifact.dir / "downscale_box.glue.js").read_text(encoding="utf-8")
    start = glue.index("function __list_to_wasm(")
    end = glue.index("function __list_from_wasm(")
    marshaller = glue[start:end]
    script = tmp_path / "agree.mjs"
    script.write_text(
        "import { instantiate, call, HEADER_BYTES } from " + repr(SHIM.resolve().as_uri()) + ";\n"
        "const k = await instantiate(" + repr(down.artifact.wasm.resolve().as_posix()) + ");\n"
        "const __wasm = k.exports;\n" + marshaller + "\n"
        "const sample = [0, 1, -1, 2147483647, -2147483648, 4294967296, -4294967297, 9007199254740991, -9007199254740991, 16711935];\n"
        "const sp = __wasm.__heap_ptr.value;\n"
        "const pg = __list_to_wasm(sample, 'i64');\n"
        "const g = new Uint8Array(__wasm.memory.buffer.slice(pg, pg + HEADER_BYTES + 8 * sample.length));\n"
        "__wasm.__heap_ptr.value = sp;\n"
        "// the shim path: write through call() with a probe kernel is not available, so use its own writer via a 1-element read-back trick:\n"
        "// allocate identically and write with the shim's generic (BigInt) writer by calling the export with the list as `px`\n"
        "const r = call(k, 'downscale_box', ['list[int]','int','int','int','list[int]'], [sample, 1, 1, 1, new Int32Array(1)], { readBack: [4] });\n"
        "const ps = pg; // same heap start after restore: the shim's first alloc lands where the glue's did\n"
        "const s = new Uint8Array(__wasm.memory.buffer.slice(ps, ps + HEADER_BYTES + 8 * sample.length));\n"
        "let same = g.length === s.length; for (let i = 0; i < g.length && same; i++) same = g[i] === s[i];\n"
        "console.log(JSON.stringify({ same, len: g.length, first: Array.from(g.slice(0, 16)), first_shim: Array.from(s.slice(0, 16)) }));\n",
        encoding="utf-8",
    )
    import json
    import subprocess

    out = subprocess.run(["node", str(script)], capture_output=True, text=True, check=False)
    assert out.returncode == 0, out.stderr
    res = json.loads(out.stdout.strip().splitlines()[-1])
    assert res["same"], res


def test_k9_i64_list_read_back_is_exact(tmp_path):
    """Review r1/B1: an `out` element beyond int32 must come back EXACTLY (not its low word).
    A probe kernel writes 2**32, -1, 2**40, -(2**40)-1 and 2**53-1 into `out`."""
    values = [4294967296, -1, 1099511627776, -1099511627777, 9007199254740991, 7,
              # the int32 fast-path transitions (codex r2): both sides of every boundary
              2**31 - 1, 2**31, -(2**31), -(2**31) - 1, 2**32 - 1, -(2**53) + 1, 2**63 - 1, -(2**63)]
    body = "\n".join(f"    out[{i}] = {v}" for i, v in enumerate(values))
    src = f"from pythscribe import wasm\n\n@wasm\ndef probe(out: list[int]) -> int:\n{body}\n    return {len(values)}\n"
    wasm = _build_variant(tmp_path, src, "probe")
    zeros = [0] * len(values)
    [r] = run_kernel(wasm, "probe", ["list[int]"], [{"args": [zeros], "read_back": [0]}])
    assert r["ok"], r
    assert r["value"] == len(values)
    assert r["outs"]["0"] == values  # exact, incl. the two i64 extremes (decoded from {bigint})
    # the int32 fast path (a typed array, written to out_file) is taken iff EVERYTHING fits int32
    small = "from pythscribe import wasm\n\n@wasm\ndef probe(out: list[int]) -> int:\n    out[0] = 2147483647\n    out[1] = -2147483648\n    return 2\n"
    wasm2 = _build_variant(tmp_path / "small", small, "probe")
    of = tmp_path / "o.i32"
    [r2] = run_kernel(wasm2, "probe", ["list[int]"], [{"args": [[0, 0]], "read_back": [0], "out_files": {"0": str(of)}}])
    assert r2["outs"]["0"] == {"i32_file": str(of)} and np.fromfile(of, dtype="<i4").tolist() == [2147483647, -2147483648]
    # beyond int32 with an int32 file requested: an EXPLICIT error, never a truncated file nor a
    # silently different return shape (codex r3)
    with pytest.raises(FfiError, match="out_files\\[0\\] not written"):
        run_kernel(wasm, "probe", ["list[int]"], [{"args": [zeros], "read_back": [0], "out_files": {"0": str(of)}}])


def test_k9b_out_files_label_names_the_element_type_written(tmp_path):
    """opus r3/NB-3: a list[float] read-back written to a file is labelled f64_file (and holds
    f64 bytes); reading it as the promised type reproduces the values exactly."""
    src = "from pythscribe import wasm\n\n@wasm\ndef fprobe(out: list[float]) -> float:\n    out[0] = 1.5\n    out[1] = 2.5\n    return 0.0\n"
    wasm = _build_variant(tmp_path, src, "fprobe")
    of = tmp_path / "o.bin"
    [r] = run_kernel(wasm, "fprobe", ["list[float]"], [{"args": [[0.0, 0.0]], "read_back": [0], "out_files": {"0": str(of)}}], return_type="float")
    assert r["ok"], r
    assert r["outs"]["0"] == {"f64_file": str(of)}
    assert np.fromfile(of, dtype="<f8").tolist() == [1.5, 2.5]
    assert of.stat().st_size == 16


def test_k12_odd_length_bool_lists_keep_the_next_list_aligned(tmp_path):
    """opus r3/NB-2: the compiler's bump allocator does not align; an odd-length list[bool]
    (4 B elements) used to leave the following i64 list at 4 mod 8 (a RangeError at b2ca56ab,
    a false refusal at 1071d36f). The shim now rounds every list allocation up to 8 bytes.
    Reverting the rounding makes this RED (the alignment assert fires)."""
    src = (
        "from pythscribe import wasm\n\n@wasm\ndef boolpair(a: list[bool], b: list[bool], out: list[int]) -> int:\n"
        "    n = 0\n    for x in a:\n        if x:\n            n = n + 1\n    for y in b:\n        if y:\n            n = n + 1\n    out[0] = n\n    return n\n"
    )
    wasm = _build_variant(tmp_path, src, "boolpair")
    for la in (1, 3, 4, 5, 7):
        a = [True] * la
        b = [True, False, True]
        [r] = run_kernel(wasm, "boolpair", ["list[bool]", "list[bool]", "list[int]"], [{"args": [a, b, [0]], "read_back": [2]}])
        assert r["ok"], (la, r)
        assert r["value"] == la + 2 and r["outs"]["2"] == [la + 2]


def test_k10_int_ingress_refuses_lossy_values(down):
    """Review r1/B2: ints that JSON or i64 cannot carry exactly are REFUSED, never rounded/wrapped."""
    rgb = np.zeros((2, 2, 3), dtype=np.uint8)
    with pytest.raises(FfiError, match="exceeds JS Number precision"):
        run_kernel(down.artifact.wasm, "downscale_box", ["list[int]", "int", "int", "int", "list[int]"],
                   [{"args": [[0, 0, 0, 0], 2**60 + 1, 2, 1, [0, 0, 0, 0]]}])
    with pytest.raises(FfiError, match="outside the i64 range"):
        run_kernel(down.artifact.wasm, "downscale_box", ["list[int]", "int", "int", "int", "list[int]"],
                   [{"args": [[0, 0, 0, 0], {"bigint": str(2**64 + 1)}, 2, 1, [0, 0, 0, 0]]}])
    # a list ELEMENT beyond 2**53 (inline) is refused too, before the JSON hop
    with pytest.raises(FfiError, match="exceeds JS Number precision"):
        run_kernel(down.artifact.wasm, "downscale_box", ["list[int]", "int", "int", "int", "list[int]"],
                   [{"args": [[2**60 + 1, 0, 0, 0], 2, 2, 1, [0, 0, 0, 0]]}])
    # the escape hatch itself must not leak: a NUMERIC bigint would be rounded by JSON (codex r2)
    with pytest.raises(FfiError, match="decimal STRING"):
        run_kernel(down.artifact.wasm, "downscale_box", ["list[int]", "int", "int", "int", "list[int]"],
                   [{"args": [[0, 0, 0, 0], {"bigint": 2**60 + 1}, 2, 1, [0, 0, 0, 0]]}])
    # the JS side refuses INDEPENDENTLY of the Python guard: feed the Node runner directly
    import json
    import subprocess

    from pythscribe.ffi import _RUNNER

    def raw(args):
        req = {"fn": "downscale_box", "param_types": ["list[int]", "int", "int", "int", "list[int]"], "return_type": "int", "calls": [{"args": args}]}
        p = subprocess.run(["node", str(_RUNNER), str(down.artifact.wasm)], input=json.dumps(req), capture_output=True, text=True, check=False)
        assert p.returncode == 0, p.stderr
        return json.loads(p.stdout)[0]

    r = raw([[0, 0, 0, 0], {"bigint": str(2**63)}, 2, 1, [0, 0, 0, 0]])
    assert not r["ok"] and "i64 range" in r["error"], r
    r = raw([[0, 0, 0, 0], 2**60, 2, 1, [0, 0, 0, 0]])  # a Number past 2**53 (already rounded by JSON) is not exact
    assert not r["ok"] and "not a safe integer" in r["error"], r
    r = raw([[2**60, 0, 0, 0], 2, 2, 1, [0, 0, 0, 0]])  # ...also as a list element
    assert not r["ok"] and "not a safe integer" in r["error"], r
    # the runner itself refuses a NUMERIC bigint descriptor (scalar and list-element forms) -- codex r3
    r = raw([[0, 0, 0, 0], {"bigint": 2**60 + 1}, 2, 1, [0, 0, 0, 0]])
    assert not r["ok"] and "decimal string" in r["error"], r
    r = raw([[{"bigint": 2**60 + 1}, 0, 0, 0], 2, 2, 1, [0, 0, 0, 0]])
    assert not r["ok"] and "decimal string" in r["error"], r
    # an exact BigInt within i64 is accepted (w = 2 as {"bigint": "2"})
    [r] = run_kernel(down.artifact.wasm, "downscale_box", ["list[int]", "int", "int", "int", "list[int]"],
                     [{"args": [[0, 0, 0, 0], {"bigint": "2"}, 2, 1, [0, 0, 0, 0]], "read_back": [4]}])
    assert r["ok"] and r["value"] == 4 and r["outs"]["4"] == [0, 0, 0, 0]


def test_k11_stale_overflow_flag_is_cleared_at_call_entry(down):
    """Review r1/S3: a stale __ovf (from an earlier overflow that then trapped) must not make
    the next valid call raise OverflowError."""
    rgb = np.random.default_rng(9).integers(0, 256, (4, 4, 3), dtype=np.uint8)
    h, w = rgb.shape[:2]
    [r] = run_kernel(down.artifact.wasm, "downscale_box", ["list[int]", "int", "int", "int", "list[int]"],
                     [{"args": [R.pack(rgb).ravel().tolist(), w, h, 2, [0, 0, 0, 0]], "read_back": [4], "poison_ovf": True}])
    assert r["ok"], r
    assert np.array_equal(R.unpack(np.asarray(r["outs"]["4"]), 2, 2), R.box_reference(rgb, 2))


def test_k8_component_shim_copy_is_byte_identical():
    copy = REPO / "pythscribe" / "gradio" / "wasm_function" / "frontend" / "list_buffer.mjs"
    assert copy.read_bytes() == SHIM.read_bytes(), "frontend/list_buffer.mjs drifted from pythscribe/ffi/list_buffer.mjs"


def test_signature_grammar_is_enforced(kernels, import_source):
    assert signature_of(kernels.downscale_box) == (["list[int]", "int", "int", "int", "list[int]"], "int")
    assert signature_of(kernels.box_scale) == (["int", "int", "int"], "int")
    mod = import_source("from pythscribe import wasm\n\n@wasm\ndef f(xs: list[str]) -> int:\n    return 1\n")
    with pytest.raises(FfiError, match="outside the list-buffer FFI grammar"):
        signature_of(mod.f)
