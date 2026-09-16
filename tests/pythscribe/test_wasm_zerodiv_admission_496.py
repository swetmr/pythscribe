"""#496 — a `try/except ZeroDivisionError` around a division-family op (`/`, `//`, `%`, `divmod`)
must NOT be admitted to the WASM fast path: on a zero divisor integer `//`/`%` TRAP
(`i64.div_s`/`i64.rem_s`) and true `/` yields `inf`, so the catchable ZeroDivisionError CPython
raises is never dispatched and the handler is dropped (a C3 error-occurrence divergence). The
refuse-to-admit fix (`crates/pyths_hir/src/wasm_analysis.rs`) routes the shape to the JS backend,
which raises a catchable ZeroDivisionError and runs the handler exactly as CPython does.

This is the end-to-end SPOT + PBT differential (the compiler-crate unit tests in
`crates/pyths_codegen_wasm/tests/wasm_test.rs::test_496_*` prove the admission logic directly).

Anti-vacuity (feedback_anti_vacuity_paired_control):
  * PAIRED NEGATIVE CONTROL — `_wasm_admits()` returns False for every zerodiv shape. A mutant that
    drops the refuse-to-admit guard re-admits the shape (`_wasm_admits()` -> True) and every
    `assert not _wasm_admits(...)` goes RED (and the admitted WASM path would then trap → wrong
    result). `test_496_positive_control_sound_shapes_still_admit` proves the "not admitted" check
    DISCRIMINATES (sound shapes DO admit), so the negative assertions are not vacuously green.
  * ORACLE — the JS-path result is compared to the CPython oracle (this interpreter) as a SPOT on
    the pinned repro and a PBT sweep; the handler-running iterations (i%3==0 → 446 at n=10) are the
    load-bearing part.
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

from conftest import REPO, gate, gate_node
from pythscribe.build import find_pyths
from pythscribe.runtime import ServerKernel, wasmtime_available

RUNTIME_DIR = REPO / "runtime"  # the `pyths-runtime` package the emitted JS imports


# --------------------------------------------------------------------------- helpers
def _compile(src: str, work: Path, stem: str, target: str) -> tuple[int, Path]:
    p = work / f"{stem}.ps"
    p.write_text(src, encoding="utf-8")
    out = work / (f"{stem}.js" if target == "js" else f"{stem}.wasm")
    r = subprocess.run(
        [str(find_pyths()), "compile", str(p), "--target", target, "-o", str(out)],
        capture_output=True, text=True,
    )
    return r.returncode, out


def _wasm_admits(src: str, work: Path, stem: str) -> bool:
    """True iff the (undecorated) kernel is WASM-admitted — a non-empty `.wasm` is emitted. The
    `--target wasm` compile never errors on a non-eligible function (it just stays JS), so a
    zero-byte / missing artifact means 'refused from WASM, routes to JS'."""
    rc, wasm = _compile(src, work, stem, "wasm")
    assert rc == 0, f"wasm compile errored unexpectedly for {stem}"
    return wasm.is_file() and wasm.stat().st_size > 0


def _wasm_exports(src: str, work: Path, stem: str) -> set[str]:
    """The function names the emitted `.wasm` exports (empty if none emitted). Used for the B1
    callee case, where the dividing callee IS WASM-eligible but the guarded caller must NOT be."""
    rc, wasm = _compile(src, work, stem, "wasm")
    assert rc == 0
    if not (wasm.is_file() and wasm.stat().st_size > 0):
        return set()
    return set(ServerKernel.from_wasm(wasm, name=stem).exports)


@pytest.fixture(scope="module")
def jsrun(tmp_path_factory):
    """A node runner for compiled-JS kernels: vendors `pyths-runtime` into a local node_modules so
    the emitted `import ... from "pyths-runtime"` resolves, then calls one exported function."""
    gate_node()
    work = tmp_path_factory.mktemp("zd496")
    shutil.copytree(RUNTIME_DIR, work / "node_modules" / "pyths-runtime")

    def run(src: str, stem: str, fn: str, args: list) -> object:
        rc, js = _compile(src, work, stem, "js")
        assert rc == 0, f"js compile failed for {stem}"
        drv = work / f"drv_{stem}.mjs"
        drv.write_text(
            f'import {{ {fn} }} from "./{js.name}";\n'
            f"const r = {fn}(...{json.dumps(args)});\n"
            "process.stdout.write(JSON.stringify(r));\n",
            encoding="utf-8",
        )
        p = subprocess.run([shutil.which("node"), str(drv)], capture_output=True, text=True, cwd=str(work))
        assert p.returncode == 0, f"node run failed for {stem}: {p.stderr}"
        return json.loads(p.stdout)

    return run


# --------------------------------------------------------------------------- the pinned repro
ZDIV_SRC = """\
def zdiv(n: int) -> int:
    total = 0
    i = 0
    while i < n:
        try:
            total = total + 100 // (i % 3)
        except ZeroDivisionError:
            total = total - 1
        i = i + 1
    return total
"""


def _zdiv_ref(n: int) -> int:
    total = 0
    i = 0
    while i < n:
        try:
            total = total + 100 // (i % 3)
        except ZeroDivisionError:
            total = total - 1
        i = i + 1
    return total


def test_496_zdiv_not_wasm_admitted(tmp_path):
    """PAIRED CONTROL: the pinned repro is refused from WASM (routes to JS). Mutant re-admitting → RED."""
    assert not _wasm_admits(ZDIV_SRC, tmp_path, "zdiv")


def test_496_zdiv_spot_js_equals_cpython_446(jsrun):
    """SPOT: the JS path runs the `except ZeroDivisionError` handler on the i%3==0 iterations,
    giving CPython's 446 at n=10 — no trap, exact."""
    assert _zdiv_ref(10) == 446  # the oracle value, spelled out
    assert jsrun(ZDIV_SRC, "zdiv", "zdiv", [10]) == 446


@pytest.mark.parametrize("n", [0, 1, 3, 7, 8, 10, 31, 100, 301])
def test_496_zdiv_pbt_js_equals_cpython(jsrun, n):
    assert jsrun(ZDIV_SRC, "zdiv", "zdiv", [n]) == _zdiv_ref(n), f"JS path != CPython at n={n}"


# --------------------------------------------------------------------------- siblings %, /, divmod
ZMOD_SRC = "def zmod(a: int, b: int) -> int:\n    try:\n        return 100 % b\n    except ZeroDivisionError:\n        return -1\n"
ZDIVF_SRC = "def zdivf(a: float, b: float) -> float:\n    try:\n        return a / b\n    except ZeroDivisionError:\n        return -1.0\n"
ZDM_SRC = "def zdm(a: int, b: int) -> int:\n    try:\n        return divmod(a, b)[0]\n    except ZeroDivisionError:\n        return -1\n"


def test_496_sibling_mod_not_wasm_admitted(tmp_path):
    assert not _wasm_admits(ZMOD_SRC, tmp_path, "zmod")


def test_496_sibling_truediv_not_wasm_admitted(tmp_path):
    # `/` yields inf on zero (silent-wrong, not a trap) — the refusal covers it too.
    assert not _wasm_admits(ZDIVF_SRC, tmp_path, "zdivf")


def test_496_sibling_divmod_not_wasm_admitted(tmp_path):
    # NB: `divmod` is not in WASM_CALL_BUILTINS, so this shape is already refused for that reason
    # too (not purely the #496 guard) — an already-refused sibling. The DISCRIMINATING witness is
    # the JS-path == CPython check below; the guard-specific paired control is the Rust
    # wasm_test.rs::test_496_* set. Kept here as an end-to-end sound-routing witness.
    assert not _wasm_admits(ZDM_SRC, tmp_path, "zdm")


def test_496_siblings_js_equal_cpython(jsrun):
    # `%` by zero → handler → -1; non-zero → 100 % b.
    assert jsrun(ZMOD_SRC, "zmod", "zmod", [100, 0]) == -1
    assert jsrun(ZMOD_SRC, "zmod", "zmod", [100, 7]) == 100 % 7
    # true `/` by zero → ZeroDivisionError → handler → -1.0; non-zero → exact float.
    assert jsrun(ZDIVF_SRC, "zdivf", "zdivf", [7.0, 0.0]) == -1.0
    assert jsrun(ZDIVF_SRC, "zdivf", "zdivf", [7.0, 2.0]) == 7.0 / 2.0
    # divmod by zero → ZeroDivisionError → handler → -1; non-zero → floor-quotient.
    assert jsrun(ZDM_SRC, "zdm", "zdm", [100, 0]) == -1
    assert jsrun(ZDM_SRC, "zdm", "zdm", [100, 7]) == divmod(100, 7)[0]


# --------------------------------------------------------------------------- handler-class coverage
BARE_SRC = "def zbare(a: int, b: int) -> int:\n    try:\n        return a // b\n    except:\n        return -1\n"
EXC_SRC = "def zexc(a: int, b: int) -> int:\n    try:\n        return a // b\n    except Exception:\n        return -1\n"
# `except OverflowError` is not a WASM-admitted handler type at all, so this shape is ALREADY refused
# from WASM (at the handler-name check) — the OverflowError sibling is sound without a #496 change.
OVF_SRC = "def zovf(a: int, b: int) -> int:\n    try:\n        return a // b\n    except OverflowError:\n        return -1\n"


def test_496_bare_and_exception_handlers_not_wasm_admitted(tmp_path):
    assert not _wasm_admits(BARE_SRC, tmp_path, "zbare")  # bare except catches ZeroDivisionError
    assert not _wasm_admits(EXC_SRC, tmp_path, "zexc")    # except Exception catches it


def test_496_overflowerror_handler_already_refused(tmp_path):
    """OverflowError sibling: `except OverflowError` is not WASM-admitted (not a built-in the WASM
    path handles), so a division under it is already refused — sound (routes to JS), no #496 hole."""
    assert not _wasm_admits(OVF_SRC, tmp_path, "zovf")


# --------------------------------------------------------------------------- B2: `**` zero-to-negative
# CPython: `0 ** -1` → ZeroDivisionError. On WASM `**` yields a silent wrong value (int) / inf
# (float) / OverflowError via the glue — the handler is dropped. It must be refused.
ZPOW_SRC = "def zpow(a: int, b: int) -> int:\n    try:\n        return a ** b\n    except ZeroDivisionError:\n        return -1\n"


def _zpow_ref(a: int, b: int) -> int:
    try:
        return a ** b
    except ZeroDivisionError:
        return -1


def test_496_pow_not_wasm_admitted(tmp_path):
    """PAIRED CONTROL (B2): `**` under `except ZeroDivisionError` is refused. Mutant re-admitting → RED."""
    assert not _wasm_admits(ZPOW_SRC, tmp_path, "zpow")


def test_496_pow_spot_js_equals_cpython(jsrun):
    """SPOT (B2): `0 ** -1` runs the handler on the JS path → -1, matching CPython; a normal power
    is exact."""
    assert _zpow_ref(0, -1) == -1  # CPython: ZeroDivisionError caught
    assert jsrun(ZPOW_SRC, "zpow", "zpow", [0, -1]) == -1
    assert jsrun(ZPOW_SRC, "zpow", "zpow", [2, 5]) == 2 ** 5


# --------------------------------------------------------------------------- B1: division in a CALLEE
# A division inside a WASM-admitted CALLEE, under the caller's ZeroDivisionError handler, TRAPS
# across the WASM call boundary (uncatchable) and drops the handler. The transitive `may_zerodiv`
# closure must refuse the CALLER (only the caller — the dividing callee is itself eligible).
CALLEE_SRC = (
    "def h(a: int, b: int) -> int:\n    return a // b\n\n"
    "def g(a: int, b: int) -> int:\n    try:\n        return h(a, b)\n    except ZeroDivisionError:\n        return -1\n"
)
TRANSITIVE_SRC = (
    "def h(a: int, b: int) -> int:\n    return a // b\n\n"
    "def mid(a: int, b: int) -> int:\n    return h(a, b) + 1\n\n"
    "def g(a: int, b: int) -> int:\n    try:\n        return mid(a, b)\n    except Exception:\n        return -1\n"
)


def _g_ref(a: int, b: int) -> int:
    def h(a, b):
        return a // b
    try:
        return h(a, b)
    except ZeroDivisionError:
        return -1


def test_496_callee_division_caller_not_wasm_admitted(tmp_path):
    """PAIRED CONTROL (B1): the guarded CALLER `g` is refused from WASM while the dividing callee
    `h` stays eligible. Mutant dropping the transitive `may_zerodiv` refusal → `g` re-admitted (in
    exports) → RED (and its WASM path would trap, dropping the handler)."""
    gate(wasmtime_available(), "wasmtime-py needed to read wasm exports")
    exports = _wasm_exports(CALLEE_SRC, tmp_path, "callee")
    assert "h" in exports, "the dividing callee `h` should stay WASM-eligible"
    assert "g" not in exports, "the guarded caller `g` must be refused from WASM (B1)"
    # transitive g -> mid -> h under `except Exception`
    texports = _wasm_exports(TRANSITIVE_SRC, tmp_path, "transitive")
    assert "g" not in texports, "the transitive caller `g` must be refused from WASM (B1)"


def test_496_callee_division_spot_js_equals_cpython(jsrun):
    """SPOT (B1): on the sound JS path `g` runs the handler on a zero divisor → -1 (== CPython), and
    the normal case is exact — no trap."""
    assert _g_ref(7, 0) == -1 and _g_ref(7, 2) == 3
    assert jsrun(CALLEE_SRC, "callee", "g", [7, 0]) == -1
    assert jsrun(CALLEE_SRC, "callee", "g", [7, 2]) == 3


# --------------------------------------------------------------------------- over-refusal controls
OKV_SRC = "def okv(a: int, b: int) -> int:\n    try:\n        return a // b\n    except ValueError:\n        return -1\n"
NOTRY_SRC = "def q(a: int, b: int) -> int:\n    return a // b\n"
NODIV_SRC = "def nod(a: int, b: int) -> int:\n    try:\n        return a + b\n    except ZeroDivisionError:\n        return -1\n"
# B1 over-refusal: a callee that does NOT divide, under the caller's `except ZeroDivisionError`.
CALL_NODIV_SRC = (
    "def sq(a: int) -> int:\n    return a * a\n\n"
    "def gg(a: int, b: int) -> int:\n    try:\n        return sq(a) + b\n    except ZeroDivisionError:\n        return -1\n"
)


def test_496_positive_control_sound_shapes_still_admit(tmp_path):
    """DISCRIMINATION control (so the negative assertions are not vacuous), and no-over-refusal:
      * `except ValueError` does NOT catch a ZeroDivisionError → the division is sound on WASM;
      * a bare `//` with no try/except is untouched;
      * `except ZeroDivisionError` around a try body with NO division is sound;
      * a call to a NON-dividing callee under `except ZeroDivisionError` is not over-refused (B1).
    All MUST still be WASM-admitted."""
    assert _wasm_admits(OKV_SRC, tmp_path, "okv")
    assert _wasm_admits(NOTRY_SRC, tmp_path, "q")
    assert _wasm_admits(NODIV_SRC, tmp_path, "nod")
    gate(wasmtime_available(), "wasmtime-py needed to read wasm exports")
    assert "gg" in _wasm_exports(CALL_NODIV_SRC, tmp_path, "callnodiv")
