"""V-err (M-1, spec `12-09-26-lib-gradio-callback-path`): the FFI shim's `call()` sees the
compiler's `__err_code` channel — a `@wasm` kernel that `raise`s / `assert`s (or hits an OOB with
`needs_errors`) THROWS from the shim with the mapped error name, never returns the compiler's
SENTINEL silently. Before B3 the shim read only `__ovf`, so a raising kernel returned its sentinel
(the silent-wrong-value class §6 forbids). This gate drives the SHIPPED shim
(`pythscribe/ffi/list_buffer.mjs`, byte-copied into a temp dir so the runner imports `./list_buffer.mjs`
relatively) and asserts:

  V-err+  raise/assert -> throw with the right `.name` (ValueError/AssertionError/...); a custom
          exception (code >= 100, a per-module glue name the shim cannot map) -> throw `.name ==
          "Exception"` with `.code == <n>`; a normally-returning kernel still returns its value.
  V-err-  PAIRED NEGATIVE CONTROL (the `test_array_net_anti_vacuity.py` mutant pattern, no production
          flag): the SAME kernel through a MUTATED shim copy (the `__err_code` check deleted) RETURNS
          the sentinel (ok, no throw) -> proving the check is load-bearing (the "shim throws" claim is
          not vacuous).
  V-nonfinite  the value-domain divergence is PINNED (documented, not discovered): `a/b` with b=0 ->
          Infinity, `math.sqrt(-1)` -> NaN, `math.log(0)` -> -inf, `math.exp(1000)` -> inf where
          CPython raises; `isRenderableScalar` classifies each as an error state (never a rendered
          value), while a finite scalar and a legitimate large `bigint` are renderable. PAIRED
          CONTROL: a stubbed `isRenderableScalar` that always returns true would render `"NaN%"` — so
          the discriminating fact is `isRenderableScalar(NaN) === false`.

Gated (skips cleanly) without node + the pyths compiler.
"""
from __future__ import annotations

import json
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

import pytest

from conftest import REPO, gate, gate_node
from pythscribe.build import BuildError, build_kernel, find_pyths
from pythscribe.ffi import SHIM

# One kernel source exercising every V-err arm. `assert`/`raise`/custom-exception bodies flip the
# compiler's `needs_errors` ON (so `__err_code`/`__err_msg` are exported); the value-domain fns use
# raw `f64.div` / `math.*` (JS `Math.*` host imports) which return non-finite where CPython raises.
KERNEL_SRC = """
class GainError(Exception):
    pass

def by_score(a: int, b: int) -> int:
    assert a != b
    return a - b

def safe_gain(x: float, k: float) -> float:
    if k < 0.0:
        raise ValueError("negative gain")
    return x * k

def custom_raise(x: float) -> float:
    if x < 0.0:
        raise GainError("bad")
    return x

def ok_gain(x: float, k: float) -> float:
    return x * k

def ratio(a: float, b: float) -> float:
    return a / b

def root(x: float) -> float:
    return math.sqrt(x)

def lg(x: float) -> float:
    return math.log(x)

def big(x: float) -> float:
    return math.exp(x)
"""

# A minimal runner that imports the shim COPY beside it (`./list_buffer.mjs`) and reports, per call,
# {ok, value, name, code, renderable} — full fidelity incl. `.code` and `isRenderableScalar` (the
# shipped `_node_runner.mjs` reports only `error: "name: message"`; this SPOT needs `.code`).
_RUNNER_MJS = r"""
import { instantiate, call, isRenderableScalar } from "./list_buffer.mjs";
const [, , wasmPath, reqPath] = process.argv;
const k = await instantiate(wasmPath);
const req = JSON.parse(await (await import("node:fs/promises")).readFile(reqPath, "utf8"));
const out = [];
for (const c of req) {
  try {
    const r = call(k, c.fn, c.param_types, c.args, { returnType: c.return_type });
    out.push({ ok: true, value: Number.isFinite(r.value) || typeof r.value === "bigint" ? String(r.value) : String(r.value), renderable: isRenderableScalar(r.value) });
  } catch (e) {
    out.push({ ok: false, name: e && e.name, code: e && e.code, message: e && e.message });
  }
}
process.stdout.write(JSON.stringify(out));
"""


@pytest.fixture(scope="module")
def wasm(tmp_path_factory) -> Path:
    gate_node()
    try:
        pyths = find_pyths()
    except BuildError as e:
        gate(False, str(e))
    d = tmp_path_factory.mktemp("verr_kernels")
    src = d / "k.ps"
    src.write_text(KERNEL_SRC)
    info = build_kernel(src, "k", KERNEL_SRC, pyths=pyths, force=True, quiet=True)
    w = next(info.dir.glob("*.wasm"))
    # sanity: the raising kernels flipped needs_errors ON (the channel exists to test)
    return w


def _run(wasm: Path, calls: list[dict], *, shim_text: str | None = None) -> list[dict]:
    """Run `calls` through a temp-dir copy of the shim (real by default; pass `shim_text` for the
    MUTANT). The runner imports `./list_buffer.mjs` relatively, exactly the anti-vacuity mechanism."""
    node = shutil.which("node")
    with tempfile.TemporaryDirectory() as td:
        tdp = Path(td)
        (tdp / "list_buffer.mjs").write_text(shim_text if shim_text is not None else SHIM.read_text(encoding="utf-8"), encoding="utf-8")
        (tdp / "_runner.mjs").write_text(_RUNNER_MJS, encoding="utf-8")
        req = tdp / "req.json"
        req.write_text(json.dumps(calls), encoding="utf-8")
        r = subprocess.run([node, str(tdp / "_runner.mjs"), str(wasm), str(req)], capture_output=True, text=True)
        assert r.returncode == 0, f"runner failed: {r.stderr[-1200:]}"
        return json.loads(r.stdout)


# ---- V-err+ : raise/assert/custom throw; normal returns ------------------------------------------

def test_verr_raise_and_assert_throw_with_mapped_names(wasm):
    res = _run(wasm, [
        {"fn": "by_score", "param_types": ["int", "int"], "return_type": "int", "args": [5, 5]},      # assert a!=b
        {"fn": "safe_gain", "param_types": ["float", "float"], "return_type": "float", "args": [2.0, -1.0]},  # raise ValueError
    ])
    assert res[0]["ok"] is False and res[0]["name"] == "AssertionError" and res[0]["code"] == 6, res[0]
    assert res[1]["ok"] is False and res[1]["name"] == "ValueError" and res[1]["code"] == 1, res[1]


def test_verr_custom_exception_maps_to_generic_exception_with_code(wasm):
    # a custom exception class (code >= 100) lives only in the per-module glue table; the shim
    # cannot know its name -> throws `.name == "Exception"` carrying `.code == <n>` (never a sentinel)
    res = _run(wasm, [{"fn": "custom_raise", "param_types": ["float"], "return_type": "float", "args": [-1.0]}])
    assert res[0]["ok"] is False and res[0]["name"] == "Exception" and res[0]["code"] >= 100, res[0]


def test_verr_normal_kernel_still_returns(wasm):
    # regression: a non-raising kernel returns its value (the __err_code check does not disturb it)
    res = _run(wasm, [{"fn": "ok_gain", "param_types": ["float", "float"], "return_type": "float", "args": [2.0, 3.0]}])
    assert res[0]["ok"] is True and res[0]["value"] == "6", res[0]


def test_verr_err_code_does_not_poison_next_call(wasm):
    # a raising call followed by a good call: the shim resets __err_code before/after, so the good
    # call is not poisoned by the previous one's set code
    res = _run(wasm, [
        {"fn": "safe_gain", "param_types": ["float", "float"], "return_type": "float", "args": [2.0, -1.0]},  # raises
        {"fn": "ok_gain", "param_types": ["float", "float"], "return_type": "float", "args": [4.0, 5.0]},      # must be clean
    ])
    assert res[0]["ok"] is False, res[0]
    assert res[1]["ok"] is True and res[1]["value"] == "20", res[1]


# ---- V-nonfinite : value-domain divergence is pinned + isRenderableScalar classifies it ----------

def test_value_domain_divergence_is_pinned_nonfinite(wasm):
    # documented divergence (§6): the callback path mirrors the compiler's value domain (float
    # `/` + math.* domain/overflow), which differs from CPython (which raises). Each surfaces as a
    # NON-FINITE result -> an error state (never a rendered value).
    res = _run(wasm, [
        {"fn": "ratio", "param_types": ["float", "float"], "return_type": "float", "args": [1.0, 0.0]},  # /0 -> inf
        {"fn": "root", "param_types": ["float"], "return_type": "float", "args": [-1.0]},   # sqrt(-1) -> NaN
        {"fn": "lg", "param_types": ["float"], "return_type": "float", "args": [0.0]},      # log(0) -> -inf
        {"fn": "big", "param_types": ["float"], "return_type": "float", "args": [1000.0]},  # exp(1000) -> inf
    ])
    for r in res:
        # each RETURNS (no throw — float value domain has no __err_code) but is non-renderable
        assert r["ok"] is True and r["renderable"] is False, r


def test_is_renderable_scalar_unit():
    """Unit (the discriminating fact the V-nonfinite DOM control keys on): NaN/+-inf are error
    states; a finite scalar and a legitimate large BigInt are renderable. A stub returning true
    for NaN is exactly what would render an empty `"NaN%"` bar (the paired control's RED)."""
    node = shutil.which("node")
    if node is None:
        pytest.skip("node not available")
    prog = (
        f"import {{ isRenderableScalar }} from {json.dumps(SHIM.resolve().as_uri())};\n"
        "const cases = [[NaN,false],[Infinity,false],[-Infinity,false],[0,true],[-0,true],[1.5,true],[42n,true],[9007199254740993n,true]];\n"
        "let bad = [];\n"
        "for (const [v, want] of cases) if (isRenderableScalar(v) !== want) bad.push(String(v));\n"
        "process.stdout.write(JSON.stringify(bad));\n"
    )
    r = subprocess.run([node, "--input-type=module", "-e", prog], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr[-800:]
    assert json.loads(r.stdout) == [], f"isRenderableScalar misclassified: {r.stdout}"


# ---- V-err- : PAIRED NEGATIVE CONTROL — the mutant shim returns the sentinel (check load-bearing) --

# ---- SF-2 : the shim's ERROR_NAMES map is PINNED to the codegen source of truth ------------------

_BRIDGE_RS = REPO / "crates" / "pyths_codegen_wasm" / "src" / "bridge.rs"
_HIR_RS = REPO / "crates" / "pyths_hir" / "src" / "wasm_analysis.rs"
# the BUILTIN code range the three sources must agree on: everything below the custom-class range
# (codes 100+ are per-module glue names the shim cannot know; SF-A widened this from `<= 7`, which
# hid a NEW builtin code -- e.g. `"OverflowError" => 8` -- from the hir arm entirely)
_BUILTIN_MAX = 100


def _shim_error_names(text: str | None = None) -> dict[int, str]:
    text = SHIM.read_text(encoding="utf-8") if text is None else text
    block = text.split("export const ERROR_NAMES = {", 1)[1].split("};", 1)[0]
    return {int(c): n for c, n in re.findall(r'(\d+)\s*:\s*"([A-Za-z]+)"', block) if 1 <= int(c) < _BUILTIN_MAX}


def _bridge_error_names(text: str | None = None) -> dict[int, str]:
    """bridge.rs builds `__ERROR_NAMES` with push_str string literals: pull the `N: 'Name'` pairs
    from the block between `const __ERROR_NAMES = {` and `function __check_err`."""
    bridge = _BRIDGE_RS.read_text(encoding="utf-8") if text is None else text
    block = bridge.split("const __ERROR_NAMES = {", 1)[1].split("function __check_err", 1)[0]
    return {int(c): n for c, n in re.findall(r"(\d+)\s*:\s*'([A-Za-z]+)'", block) if 1 <= int(c) < _BUILTIN_MAX}


def _hir_exception_codes(text: str | None = None) -> dict[str, int]:
    """The builtin exception -> code map in pyths_hir::exception_code (every code below the
    custom-class 100+ range; ignores the `Exception => 7` alias, which is not a distinct name)."""
    hir = _HIR_RS.read_text(encoding="utf-8") if text is None else text
    body = hir.split("pub fn exception_code", 1)[1].split("}", 1)[0]
    out: dict[str, int] = {}
    for name, code in re.findall(r'"([A-Za-z]+)"\s*=>\s*(\d+)', body):
        c = int(code)
        if 1 <= c < _BUILTIN_MAX and name != "Exception":  # Exception aliases 7 (RuntimeError); not a distinct name
            out[name] = c
    return out


# the 1-7 core every parser must at least find (the non-vacuity anchor: proves the parsers parsed)
_CORE = {1: "ValueError", 2: "TypeError", 3: "IndexError", 4: "KeyError",
         5: "ZeroDivisionError", 6: "AssertionError", 7: "RuntimeError"}


def test_sf2_shim_error_names_pinned_to_codegen():
    """SF-2 (+SF-A): the shim's code->name map (`ERROR_NAMES`), `bridge.rs __ERROR_NAMES` (the
    js+wasm glue's map) and `pyths_hir::exception_code` (the code ASSIGNMENT) must be the SAME map
    over the whole builtin range (1 <= code < 100). A new builtin code added to ANY ONE of the three
    without the other two would make the shim (or the glue) throw a MIS-NAMED `.name=='Exception'`
    silently; asserting all three KEY SETS (and names) equal makes drift in any one go RED here.
    The 1-7 core is pinned as the non-vacuity anchor (each parser actually parsed its source)."""
    shim = _shim_error_names()
    bridge = _bridge_error_names()
    hir_inv = {v: k for k, v in _hir_exception_codes().items()}  # hir stores name->code
    for label, m in (("shim ERROR_NAMES", shim), ("bridge.rs __ERROR_NAMES", bridge), ("pyths_hir::exception_code", hir_inv)):
        assert _CORE.items() <= m.items(), f"{label} lost part of the 1-7 core (parser or source drift): {m}"
    assert set(shim) == set(bridge) == set(hir_inv), (
        f"builtin error-code KEY SETS drifted: shim={sorted(shim)} bridge={sorted(bridge)} hir={sorted(hir_inv)}"
    )
    assert shim == bridge == hir_inv, f"builtin error-code NAMES drifted: shim={shim} bridge={bridge} hir={hir_inv}"


def test_sf2_negative_control_new_code_in_one_source_goes_red():
    """SF-A PAIRED CONTROL (the reviewer's P3 text mutants): a NEW builtin code `8: OverflowError`
    added to exactly ONE of the three sources must make the three-way equality FAIL -- for EACH of
    the three (the old `<= 7` filter left the hir-only mutant GREEN). Text-level mutants of the real
    sources, driven through the SAME parsers the pin uses."""
    shim_src = SHIM.read_text(encoding="utf-8")
    bridge_src = _BRIDGE_RS.read_text(encoding="utf-8")
    hir_src = _HIR_RS.read_text(encoding="utf-8")
    real = (_shim_error_names(shim_src), _bridge_error_names(bridge_src), {v: k for k, v in _hir_exception_codes(hir_src).items()})
    assert real[0] == real[1] == real[2], "the real sources must agree for the control to be meaningful"

    m_shim = shim_src.replace('7: "RuntimeError",', '7: "RuntimeError", 8: "OverflowError",', 1)
    m_bridge = bridge_src.replace("7: 'RuntimeError',", "7: 'RuntimeError', 8: 'OverflowError',", 1)
    m_hir = hir_src.replace('"RuntimeError" => 7,', '"RuntimeError" => 7,\n        "OverflowError" => 8,', 1)
    assert m_shim != shim_src and m_bridge != bridge_src and m_hir != hir_src, "mutation anchors not found (control vacuous)"

    mut_shim = _shim_error_names(m_shim)
    mut_bridge = _bridge_error_names(m_bridge)
    mut_hir = {v: k for k, v in _hir_exception_codes(m_hir).items()}
    # each single-source mutant now carries code 8 -> visible to its parser (not filtered away) ...
    assert mut_shim[8] == "OverflowError" and mut_bridge[8] == "OverflowError" and mut_hir[8] == "OverflowError"
    # ... and breaks the three-way key-set equality against the two unmutated sources (RED in each arm)
    assert not (set(mut_shim) == set(real[1]) == set(real[2])), "shim-only new code was NOT detected"
    assert not (set(real[0]) == set(mut_bridge) == set(real[2])), "bridge-only new code was NOT detected"
    assert not (set(real[0]) == set(real[1]) == set(mut_hir)), "hir-only new code was NOT detected (the SF-A blind spot)"


def test_verr_negative_control_mutant_shim_returns_sentinel(wasm):
    """Delete the `__err_code` check from a COPY of the shim (temp dir, relative import — no
    production flag) and run the SAME raising kernel: it RETURNS the sentinel (ok, no throw),
    proving the real check is load-bearing (the shim-throws claim is not vacuous)."""
    import re

    shim = SHIM.read_text(encoding="utf-8")
    # strip the whole `if (ex.__err_code && ex.__err_code.value) { ... throw e; }` block in call()
    mutated = re.sub(
        r"\n    if \(ex\.__err_code && ex\.__err_code\.value\) \{\n(?:.*\n)*?      throw e;\n    \}\n",
        "\n",
        shim,
        count=1,
    )
    assert mutated != shim, "the __err_code check block must be present to remove (else the control is vacuous)"

    real = _run(wasm, [{"fn": "safe_gain", "param_types": ["float", "float"], "return_type": "float", "args": [2.0, -1.0]}])
    assert real[0]["ok"] is False, f"the REAL shim must throw on a raising kernel: {real[0]}"

    mut = _run(wasm, [{"fn": "safe_gain", "param_types": ["float", "float"], "return_type": "float", "args": [2.0, -1.0]}], shim_text=mutated)
    assert mut[0]["ok"] is True, f"the MUTANT (check removed) must RETURN the sentinel, not throw: {mut[0]}"
    # the mutant surfaces a value where the real shim throws — that difference is the whole point
    assert real[0]["ok"] != mut[0]["ok"]


# ---- SF-3 : the island's kernel cache EVICTS a rejected promise (a transient fetch fail retries) --

_FRONTEND = REPO / "pythscribe" / "gradio" / "wasm_function" / "frontend"
_KCACHE = _FRONTEND / "kernel_cache.mjs"

# Drives the SHIPPED kernel_cache.mjs (react-free, so it imports under Node). Reports, for a
# REJECTED load and a SUCCESSFUL one: whether the cache holds the key before/after the promise
# settles, whether a retry after a rejection returns a FRESH promise (only possible if the rejected
# entry was evicted), and whether a success stays deduped.
_KC_RUNNER = r"""
import { instantiateOnce, _kernelCache, kernelKey } from KCACHE_URL;
const goodWasm = process.argv[2] || null;
const out = {};
const specNull = { fn: "x", wasm: null, source_sha256: "sha-null", param_types: [], return_type: "float", shape: "node" };
const p1 = instantiateOnce(specNull);
out.cached_before_settle = _kernelCache.has(kernelKey(specNull));   // set synchronously
try { await p1; out.p1 = "resolved"; } catch { out.p1 = "rejected"; }
out.evicted_after_settle = !_kernelCache.has(kernelKey(specNull));  // SF-3: rejection evicts
const p2 = instantiateOnce(specNull);
out.retry_new_promise = p1 !== p2;   // discriminating: a poisoned cache would return the SAME p1
try { await p2; } catch {}
_kernelCache.delete(kernelKey(specNull));
if (goodWasm) {
  const specGood = { fn: "ok_gain", wasm: goodWasm, source_sha256: "sha-good", param_types: ["float","float"], return_type: "float", shape: "node" };
  const g1 = instantiateOnce(specGood);
  const g2 = instantiateOnce(specGood);
  out.dedup_same_promise = g1 === g2;   // one fetch+compile per distinct kernel
  try { const h = await g1; out.good = (h && h.exports && typeof h.exports.ok_gain === "function") ? "ok" : "no-fn"; }
  catch (e) { out.good = "err:" + (e && e.message); }
  out.good_still_cached = _kernelCache.has(kernelKey(specGood));  // a success STAYS cached
}
process.stdout.write(JSON.stringify(out));
"""


def _run_kc(runner_mjs: str, kcache_url: str, *args: str) -> dict:
    node = shutil.which("node")
    with tempfile.TemporaryDirectory() as td:
        r = Path(td) / "_kc_runner.mjs"
        r.write_text(runner_mjs.replace("KCACHE_URL", json.dumps(kcache_url)), encoding="utf-8")
        p = subprocess.run([node, str(r), *args], capture_output=True, text=True)
        assert p.returncode == 0, f"kernel-cache runner failed: {p.stderr[-1500:]}"
        return json.loads(p.stdout)


def test_sf3_kernel_cache_evicts_rejected_promise(wasm):
    """SF-3: a REJECTED `.wasm` load must be EVICTED from `_kernelCache`, so a later remount RETRIES
    instead of returning the same poisoned rejection forever. Drives the shipped
    `frontend/kernel_cache.mjs` under Node; the paired NEGATIVE CONTROL is a temp-dir copy with the
    `.catch(...)`-eviction removed -- there the retry returns the SAME rejected promise (poisoned)."""
    gate_node()
    real = _run_kc(_KC_RUNNER, _KCACHE.resolve().as_uri(), str(wasm))
    assert real["cached_before_settle"] is True, real     # the entry is set synchronously
    assert real["p1"] == "rejected", real
    assert real["evicted_after_settle"] is True, f"a rejected load was NOT evicted (SF-3): {real}"
    assert real["retry_new_promise"] is True, f"a remount did not retry (poisoned cache): {real}"
    # a successful load stays deduped (the eviction is scoped to rejections only)
    assert real["dedup_same_promise"] is True and real["good"] == "ok" and real["good_still_cached"] is True, real

    # PAIRED NEGATIVE CONTROL: strip the eviction `.catch(...)` from a COPY -> the rejected promise
    # is cached forever, so the retry returns the SAME promise (retry_new_promise == False).
    src = _KCACHE.read_text(encoding="utf-8")
    mutated = re.sub(r"\n\t\tp\.catch\(\(\) => \{\n(?:.*\n)*?\t\t\}\);\n", "\n", src, count=1)
    assert mutated != src, "the eviction .catch block must be present to remove (else the control is vacuous)"
    with tempfile.TemporaryDirectory() as td:
        tdp = Path(td)
        (tdp / "kernel_cache.mjs").write_text(mutated, encoding="utf-8")
        (tdp / "list_buffer.mjs").write_text((_FRONTEND / "list_buffer.mjs").read_text(encoding="utf-8"), encoding="utf-8")
        mut = _run_kc(_KC_RUNNER, (tdp / "kernel_cache.mjs").resolve().as_uri())
    assert mut["evicted_after_settle"] is False, f"mutant must NOT evict: {mut}"
    assert mut["retry_new_promise"] is False, f"mutant (no eviction) must return the SAME poisoned promise: {mut}"
    assert real["retry_new_promise"] != mut["retry_new_promise"]  # the discriminating difference
