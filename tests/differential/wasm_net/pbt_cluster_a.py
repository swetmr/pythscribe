#!/usr/bin/env python3
"""Property-based differential for Cluster A (#485 / #487) — the WASM expression
lowering vs the CPython oracle, on the RAW `.wasm` export (no JS-twin masking).

Two generators (Hypothesis, `py -3.14`):

  1. FIXED SHAPES, random INPUTS — the `while i < n and xs[i] …` bounds-guard idiom,
     `or` guards, nested and/or, `not (a and b)`, the IfExp test, chained comparisons
     with a guarded tail, `and`/`or` as VALUES (deciding operand, int and float incl.
     -0.0 / inf / nan), the six list comparisons (int and float lists, unequal
     lengths, prefixes) and list truthiness in every condition context.
  2. GENERATED EXPRESSIONS, random inputs — random boolean / comparison / chained /
     IfExp trees over `a, b, c: int` and a guarded `xs: list[int]` subscript
     (`i < len(xs) and xs[i] > k` — a non-short-circuiting lowering TRAPS here),
     compiled as a batch module through the SHIPPED `pyths --target js+wasm` and run
     on the raw export.

Each Hypothesis example is a BATCH (one node process per batch), so shrinking works
on the batch and the run stays fast. The outcome algebra / normalization is the
net's (`wasm_net.same`). Run from the repo root:
    python tests/differential/wasm_net/pbt_cluster_a.py
Env: PYTHS_BIN, PYTHS_ORACLE_PYTHON (the oracle must be THIS interpreter or agree with
it — the CPython arm runs in-process here), HYPOTHESIS_MAX (default 40 batches).
"""
from __future__ import annotations

import json
import math
import os
import random
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import cpython_arm  # noqa: E402  (the oracle's normalization)
import wasm_net  # noqa: E402

MAX = int(os.environ.get("HYPOTHESIS_MAX", "40"))
SCRATCH = Path(tempfile.mkdtemp(prefix="pbt_cluster_a_"))

FIXED_SRC = '''
def sc_while_and(xs: list[int], n: int) -> int:
    j = 0
    while j < n and xs[j] > 0:
        j = j + 1
    return j

def sc_or(xs: list[int], i: int) -> int:
    if i >= len(xs) or xs[i] == 0:
        return 1
    return 0

def sc_nested(xs: list[int], i: int, flag: bool) -> int:
    if flag and (i < len(xs) and xs[i] > 0):
        return 1
    if flag or (i < len(xs) and xs[i] > 0):
        return 2
    return 0

def sc_not_and(xs: list[int], i: int) -> bool:
    return not (i < len(xs) and xs[i] > 0)

def sc_ifexp(xs: list[int], i: int) -> int:
    return xs[i] if (i < len(xs) and xs[i] > 0) else -1

def sc_chain_guard(xs: list[int], i: int, n: int) -> bool:
    return i < n < xs[i]

def chain3(a: int, b: int, c: int) -> bool:
    return a < b <= c

def val_and(a: int, b: int) -> int:
    return a and b

def val_or(a: int, b: int) -> int:
    return a or b

def val_and_f(a: float, b: float) -> float:
    return a and b

def val_or_f(a: float, b: float) -> float:
    return a or b

def mix(a: int, b: int, c: int) -> int:
    return a and b or c

def cmp_eq(a: list[int], b: list[int]) -> bool:
    return a == b

def cmp_ne(a: list[int], b: list[int]) -> bool:
    return a != b

def cmp_lt(a: list[int], b: list[int]) -> bool:
    return a < b

def cmp_le(a: list[int], b: list[int]) -> bool:
    return a <= b

def cmp_gt(a: list[int], b: list[int]) -> bool:
    return a > b

def cmp_ge(a: list[int], b: list[int]) -> bool:
    return a >= b

def cmpf_lt(a: list[float], b: list[float]) -> bool:
    return a < b

def cmpf_eq(a: list[float], b: list[float]) -> bool:
    return a == b

def truth_if(xs: list[int]) -> int:
    if xs:
        return 1
    return 0

def truth_not(xs: list[int]) -> bool:
    return not xs

def truth_and(xs: list[int], k: int) -> int:
    if xs and k > 0:
        return 1
    return 0

def self_eq(fs: list[float]) -> bool:
    return fs == fs

def self_lt(fs: list[float]) -> bool:
    return fs < fs
'''

ints = st.integers(min_value=-(2**40), max_value=2**40)
small = st.integers(min_value=-4, max_value=6)
floats = st.one_of(
    st.floats(allow_nan=True, allow_infinity=True),
    st.sampled_from([0.0, -0.0, 1.0, -1.0, math.inf, -math.inf, math.nan, 1e300, 1e-300]),
)
ilist = st.lists(ints, max_size=5)
flist = st.lists(floats, max_size=4)

SPECS = {
    "sc_while_and": (["list[int]", "int"], "int", st.tuples(st.lists(small, max_size=5), small)),
    "sc_or": (["list[int]", "int"], "int", st.tuples(st.lists(small, max_size=5), small)),
    "sc_nested": (["list[int]", "int", "bool"], "int", st.tuples(st.lists(small, max_size=5), small, st.booleans())),
    "sc_not_and": (["list[int]", "int"], "bool", st.tuples(st.lists(small, max_size=5), small)),
    "sc_ifexp": (["list[int]", "int"], "int", st.tuples(st.lists(small, max_size=5), small)),
    "sc_chain_guard": (["list[int]", "int", "int"], "bool", st.tuples(st.lists(small, max_size=5), small, small)),
    "chain3": (["int", "int", "int"], "bool", st.tuples(ints, ints, ints)),
    "val_and": (["int", "int"], "int", st.tuples(ints, ints)),
    "val_or": (["int", "int"], "int", st.tuples(ints, ints)),
    "val_and_f": (["float", "float"], "float", st.tuples(floats, floats)),
    "val_or_f": (["float", "float"], "float", st.tuples(floats, floats)),
    "mix": (["int", "int", "int"], "int", st.tuples(small, small, small)),
    "cmp_eq": (["list[int]", "list[int]"], "bool", st.tuples(ilist, ilist)),
    "cmp_ne": (["list[int]", "list[int]"], "bool", st.tuples(ilist, ilist)),
    "cmp_lt": (["list[int]", "list[int]"], "bool", st.tuples(ilist, ilist)),
    "cmp_le": (["list[int]", "list[int]"], "bool", st.tuples(ilist, ilist)),
    "cmp_gt": (["list[int]", "list[int]"], "bool", st.tuples(ilist, ilist)),
    "cmp_ge": (["list[int]", "list[int]"], "bool", st.tuples(ilist, ilist)),
    "cmpf_lt": (["list[float]", "list[float]"], "bool", st.tuples(flist, flist)),
    "cmpf_eq": (["list[float]", "list[float]"], "bool", st.tuples(flist, flist)),
    "truth_if": (["list[int]"], "int", st.tuples(ilist)),
    "truth_not": (["list[int]"], "bool", st.tuples(ilist)),
    "self_eq": (["list[float]"], "bool", st.tuples(flist)),
    "self_lt": (["list[float]"], "bool", st.tuples(flist)),
    "truth_and": (["list[int]", "int"], "int", st.tuples(ilist, small)),
    # (`-> bool: return a` is deliberately NOT here: CPython returns the int itself while
    # the WASM boundary coerces to a bool -- a tracked repr divergence, NEW-W10 in the net.)
}


def compile_direct(src: str, name: str) -> Path:
    d = SCRATCH / name
    d.mkdir(parents=True, exist_ok=True)
    ps = d / f"{name}.ps"
    ps.write_text(src.lstrip("\n"), encoding="utf-8")
    out = d / f"{name}.js"
    r = wasm_net.run([str(wasm_net.PYTHS), "compile", str(ps), "--target", "js+wasm", "-o", str(out), "--verbose"],
                     env={**os.environ, "PYTHS_NO_CACHE": "1"})
    log = wasm_net.ANSI.sub("", (r.stdout or "") + (r.stderr or ""))
    assert r.returncode == 0, log
    wasm = out.with_suffix(".wasm")
    assert wasm.exists(), log
    return wasm


def run_direct(wasm: Path, spec: dict) -> dict:
    sp = wasm.with_suffix(".spec.json")
    sp.write_text(json.dumps(spec), encoding="utf-8")
    res = wasm_net.arm_json(["node", str(HERE / "run_wasm_direct.mjs"), str(wasm), str(sp)], cwd=str(SCRATCH))
    assert isinstance(res, dict), res
    return res


def py_outcome(fn, params, ret, args):
    py_args = [cpython_arm.decode_arg(a, t) for a, t in zip(args, params)]
    try:
        r = fn(*py_args)
    except Exception as e:  # noqa: BLE001
        return {"kind": "err", "exc": type(e).__name__}
    lists = {str(i): [cpython_arm.norm(e, cpython_arm.elem_type(t)) for e in py_args[i]]
             for i, t in enumerate(params) if t.startswith("list[")}
    return {"kind": "ok", "value": cpython_arm.norm(r, ret), "lists": lists}


def load_module(src: str, name: str):
    ns = {}
    exec(compile(src, name, "exec"), ns)  # noqa: S102 — the oracle IS Python
    return ns


def check_batch(mod_ns, wasm: Path, calls_by_fn: dict[str, list[list]], specs: dict) -> None:
    spec = {"functions": {n: {"params": specs[n][0], "ret": specs[n][1], "calls": calls}
                          for n, calls in calls_by_fn.items()}}
    direct = run_direct(wasm, spec)
    for n, calls in calls_by_fn.items():
        params, ret = specs[n][0], specs[n][1]
        for i, args in enumerate(calls):
            p = py_outcome(mod_ns[n], params, ret, args)
            o = direct[n][i]
            if o["kind"] == "ovf":
                continue  # the documented twin re-run; not a lowering property
            ok, note = wasm_net.same(p, o)
            assert ok, f"{n}{args}: CPython {p} vs WASM-direct {o} ({note})"


_FIXED_WASM = None
_FIXED_NS = None


def fixed():
    global _FIXED_WASM, _FIXED_NS
    if _FIXED_WASM is None:
        _FIXED_WASM = compile_direct(FIXED_SRC, "fixed")
        _FIXED_NS = load_module(FIXED_SRC, "fixed")
    return _FIXED_WASM, _FIXED_NS


batch_strategy = st.fixed_dictionaries({n: st.lists(s[2], min_size=8, max_size=24) for n, s in SPECS.items()})


@settings(max_examples=MAX, deadline=None, suppress_health_check=list(HealthCheck))
@given(batch_strategy)
def test_fixed_shapes(batch):
    wasm, ns = fixed()
    calls = {n: [[wasm_net.enc(a) for a in args] for args in tuples] for n, tuples in batch.items()}
    check_batch(ns, wasm, calls, SPECS)


# ---- generated expression trees ------------------------------------------------

ATOMS = ["a", "b", "c", "0", "1", "-1", "2"]
CMP = ["<", "<=", ">", ">=", "==", "!="]


@st.composite
def expr(draw, depth=0):
    kind = draw(st.sampled_from(["cmp", "guard", "and", "or", "not", "chain", "ifexp", "atom"] if depth < 3 else ["cmp", "guard", "atom"]))
    if kind == "atom":
        return draw(st.sampled_from(ATOMS))
    if kind == "cmp":
        return f"({draw(st.sampled_from(ATOMS))} {draw(st.sampled_from(CMP))} {draw(st.sampled_from(ATOMS))})"
    if kind == "guard":
        i = draw(st.sampled_from(["a", "b", "c"]))
        k = draw(st.sampled_from(ATOMS))
        op = draw(st.sampled_from(CMP))
        # A non-short-circuiting `and` traps here (xs[i] out of range once i >= len).
        return f"(0 <= {i} and {i} < len(xs) and xs[{i}] {op} {k})"
    if kind == "and":
        return f"({draw(expr(depth + 1))} and {draw(expr(depth + 1))})"
    if kind == "or":
        return f"({draw(expr(depth + 1))} or {draw(expr(depth + 1))})"
    if kind == "not":
        return f"(not {draw(expr(depth + 1))})"
    if kind == "chain":
        return f"({draw(st.sampled_from(ATOMS))} {draw(st.sampled_from(CMP))} {draw(expr(depth + 1))} {draw(st.sampled_from(CMP))} {draw(st.sampled_from(ATOMS))})"
    if kind == "ifexp":
        return f"({draw(expr(depth + 1))} if {draw(expr(depth + 1))} else {draw(expr(depth + 1))})"
    raise AssertionError(kind)


gen_batch = st.tuples(
    st.lists(expr(), min_size=6, max_size=14),
    st.lists(st.tuples(st.lists(small, max_size=4), small, small, small), min_size=6, max_size=12),
)


@settings(max_examples=MAX, deadline=None, suppress_health_check=list(HealthCheck))
@given(gen_batch)
def test_generated_expressions(batch):
    exprs, inputs = batch
    lines = []
    specs = {}
    for k, e in enumerate(exprs):
        # `and`/`or` over ints and bools yield an int-or-bool VALUE (never a list here);
        # a bool result is normalized under the declared int ("True" -> "1").
        lines.append(f"def g{k}(xs: list[int], a: int, b: int, c: int) -> int:\n    return {e}\n")
        specs[f"g{k}"] = (["list[int]", "int", "int", "int"], "int", None)
    src = "\n".join(lines)
    tag = f"gen_{random.randrange(1 << 30):08x}"
    wasm = compile_direct(src, tag)
    ns = load_module(src, tag)
    calls = {n: [[wasm_net.enc(a) for a in args] for args in inputs] for n in specs}
    check_batch(ns, wasm, calls, specs)


def main() -> int:
    if not Path(wasm_net.PYTHS).exists() or not shutil.which("node"):
        print("[pbt_cluster_a] pyths binary or node missing", file=sys.stderr)
        return 2
    print(f"[pbt_cluster_a] pyths={wasm_net.PYTHS} max_examples={MAX} python={sys.version.split()[0]}")
    test_fixed_shapes()
    print(f"[pbt_cluster_a] fixed shapes: {MAX} batches x {len(SPECS)} functions x 8..24 inputs — no divergence")
    test_generated_expressions()
    print(f"[pbt_cluster_a] generated expressions: {MAX} batches x 6..14 exprs x 6..12 inputs — no divergence")
    shutil.rmtree(SCRATCH, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
