#!/usr/bin/env python3
"""Shipped-binding differential for the JS<->WASM marshalling table.

Binds verification/marshalling-table.txt (and the Lean MarshalTable model) to
the REAL shipped boundary: compiles a boundary-crossing corpus with the real
`pyths` binary (`--target js+wasm`), runs the emitted JS+WASM under node, runs
the SAME source under CPython, and asserts the outputs agree — including every
explicit failure disposition the table records:

  * in-range i64 / f64 / bool / list crossings pass EXACTLY
    (arg/ret rows int, float, bool, list<int>, list<float>, list<bool>)
  * BigInt scalar arg beyond i64  -> exact via twin   (fault i64-arg-oob twins)
  * i64 LIST ELEMENT beyond i64   -> exact via twin   (fault list-elem-i64-oob twins)
  * in-WASM result overflow       -> exact via twin   (fault ovf-flag twins)
  * deliberate Python exception   -> propagates as the same exception
                                                      (fault py-exception)

The corpus is split into four small modules because multi-kernel modules can
hit the pre-existing #364 "type-lowering gap" compile-time fallback (correct
but JS-routed), which would make the differential vacuous for those kernels.

Anti-vacuity gates built in:
  1. The harness asserts each kernel was ACTUALLY admitted to WASM
     (`__wasm.<name>(` present in the glue) — a JS-only compile would make
     the differential vacuous.
  2. FALSE-WORLD control: it then FORGES the int-list module's glue by
     deleting the i64 element guard and asserts the differential now FAILS
     on the oob-element case (reproducing the pre-fix silent wrap
     -9223372036854775801). A harness that stays green over the forged
     boundary would prove nothing.

Deviation quotient: D1 (whole floats print `6` in JS vs `6.0` in CPython) is
normalized, as everywhere else in the project. Everything else is compared
byte-for-byte.

Run from the repo root (requires target/debug/pyths.exe, node, runtime/):
    python verification/marshalling_shipped_binding.py
"""

import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
_EXE = "pyths.exe" if sys.platform == "win32" else "pyths"
PYTHS = next(
    (p for p in (ROOT / "target" / "debug" / _EXE, ROOT / "target" / "release" / _EXE)
     if p.exists()),
    ROOT / "target" / "debug" / _EXE,
)

INT_MOD = '''\
def pick(xs: list[int]) -> int:
    return xs[0]

def total(xs: list[int]) -> int:
    s = 0
    for x in xs:
        s = s + x
    return s

def add(a: int, b: int) -> int:
    return a + b

# --- in-range crossings: pass EXACTLY (arg/ret rows int, list<int>) ---
print(pick([5]))
print(pick([9223372036854775807]))
print(pick([-9223372036854775808]))
print(total([1, 2, 3]))
print(add(4096, 1024))
print(add(9007199254740991, 0))
print(add(9007199254740993, 0))
# --- fault i64-arg-oob twins -> reroute-twin (exact, never wrapped) ---
print(add(9223372036854775815, 0))
print(add(-9223372036854775813, 0))
# --- fault list-elem-i64-oob twins -> reroute-twin (exact, never wrapped) ---
print(pick([9223372036854775815]))
print(total([9223372036854775815, 1]))
# --- fault ovf-flag twins -> reroute-twin (exact, never wrapped) ---
print(add(9223372036854775807, 1))
print(total([9223372036854775807, 1]))
print(add(-9223372036854775808, -1))
'''

FLOAT_MOD = '''\
def fsum(xs: list[float]) -> float:
    s = 0.0
    for x in xs:
        s = s + x
    return s

def fmul(a: float, b: float) -> float:
    return a * b

# --- f64 crossings: bit-identity (arg/ret rows float, list<float>) ---
print(fsum([0.5, 0.25, 0.125]))
print(fmul(1.5, -2.25))
print(fmul(2.0, 3.0))
'''

BOOL_MOD = '''\
def bcount(xs: list[bool]) -> int:
    c = 0
    for x in xs:
        if x:
            c = c + 1
    return c

def isneg(a: int) -> bool:
    return a < 0

# --- bool crossings: x?1:0 in, Boolean out (rows bool, list<bool>) ---
print(bcount([True, False, True, True]))
print(isneg(-5))
print(isneg(3))
'''

EXC_MOD = '''\
def divk(a: int, b: int) -> int:
    return a // b

# --- fault py-exception -> propagate (same exception as CPython) ---
print(divk(9, 2))
try:
    print(divk(7, 0))
except ZeroDivisionError:
    print("ZeroDivisionError caught")
'''

MODULES = [
    ("intmod", INT_MOD, ["pick", "total", "add"]),
    ("floatmod", FLOAT_MOD, ["fsum", "fmul"]),
    ("boolmod", BOOL_MOD, ["bcount", "isneg"]),
    ("excmod", EXC_MOD, ["divk"]),
]

# The exact guard line the false-world control deletes (must match bridge.rs).
ELEM_GUARD = ("      if (b > 9223372036854775807n || b < -9223372036854775808n) "
              "throw new RangeError('OverflowError: list element exceeds the i64 "
              "range of the WASM fast path');\n")

D1_WHOLE = re.compile(r"^(-?\d+)\.0$")


def normalize_d1(text: str) -> list[str]:
    """CPython prints whole floats as `6.0`; JS as `6` (documented D1)."""
    return [D1_WHOLE.sub(r"\1", line) for line in text.strip().splitlines()]


def run(cmd, **kw):
    p = subprocess.run(cmd, capture_output=True, text=True, **kw)
    if p.returncode != 0:
        sys.exit(f"FAIL: {' '.join(map(str, cmd))}\n{p.stdout}\n{p.stderr}")
    return p.stdout


def main() -> None:
    if not PYTHS.exists():
        sys.exit(f"pyths binary not found at {PYTHS} — run `cargo build -p pyths_cli`")

    total_obs = 0
    with tempfile.TemporaryDirectory() as td:
        d = Path(td)
        runtime = (ROOT / "runtime").as_posix()
        (d / "package.json").write_text(
            '{ "type": "module", "dependencies": { "pyths-runtime": "file:%s" } }'
            % runtime, encoding="utf-8")
        run(["npm", "install", "--no-audit", "--no-fund", "--silent"],
            cwd=d, shell=(sys.platform == "win32"))

        int_glue = None
        for name, src, kernels in MODULES:
            ps = d / f"{name}.ps"
            ps.write_text(src, encoding="utf-8")

            # 1. Compile with the REAL pyths, js+wasm target.
            run([str(PYTHS), "compile", str(ps), "--target", "js+wasm", "-o", name], cwd=d)
            main_js = (d / name).read_text(encoding="utf-8")
            glue = (d / f"{name}.glue.js").read_text(encoding="utf-8")

            # 2. Anti-vacuity: every kernel must ACTUALLY be WASM-admitted.
            for k in kernels:
                assert f"__wasm.{k}(" in glue, (
                    f"kernel `{k}` was NOT admitted to WASM in {name} — the "
                    f"differential would be vacuous (JS-only) for it")
            assert (d / f"{name}.wasm").stat().st_size > 0, f"empty wasm: {name}"

            # 3. ESM wiring.
            (d / f"{name}.mjs").write_text(
                main_js.replace(f"./{name}.glue.js", f"./{name}.glue.mjs"),
                encoding="utf-8")
            (d / f"{name}.glue.mjs").write_text(glue, encoding="utf-8")
            js_out = normalize_d1(run(["node", f"{name}.mjs"], cwd=d))

            # 4. CPython reference on the SAME source.
            py = d / f"{name}_ref.py"
            py.write_text(src, encoding="utf-8")
            py_out = normalize_d1(run([sys.executable, str(py)]))

            # 5. The differential.
            assert len(js_out) == len(py_out) != 0, (name, js_out, py_out)
            diverged = [(i, a, b) for i, (a, b) in enumerate(zip(js_out, py_out)) if a != b]
            assert not diverged, f"{name}: shipped js+wasm diverges from CPython: {diverged}"
            total_obs += len(js_out)
            if name == "intmod":
                int_glue = glue
                int_py_out = py_out

        print(f"shipped-binding OK: {total_obs} boundary observations across "
              f"{len(MODULES)} modules, js+wasm == CPython (D1-normalized), "
              f"all table dispositions confirmed")

        # 6. FALSE-WORLD control on the int-list module: forge the glue
        #    (delete the i64 element guard) and require the differential to
        #    CATCH the silent wrap.
        assert ELEM_GUARD in int_glue, (
            "the i64 element guard line is missing from the shipped glue — "
            "either the guard regressed or bridge.rs changed shape; update "
            "ELEM_GUARD together with the marshalling table")
        (d / "intmod.glue.mjs").write_text(
            int_glue.replace(ELEM_GUARD, ""), encoding="utf-8")
        forged_out = normalize_d1(run(["node", "intmod.mjs"], cwd=d))
        forged_div = [(i, a, b)
                      for i, (a, b) in enumerate(zip(forged_out, int_py_out)) if a != b]
        assert forged_div, (
            "FALSE-WORLD control failed: deleting the element guard did not "
            "make the differential diverge — the harness would not catch the "
            "pre-fix silent-wrap bug")
        assert any("-9223372036854775801" in a for _, a, _b in forged_div), (
            f"expected the pre-fix wrapped value in the forged run: {forged_div}")
        print(f"false-world control OK: guard-deleted glue diverges on "
              f"{len(forged_div)} case(s) (silent wrap reproduced, harness has teeth)")


if __name__ == "__main__":
    main()
