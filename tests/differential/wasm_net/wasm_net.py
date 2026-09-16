#!/usr/bin/env python3
"""wasm_net — the admitted-shape 3-way differential net (the standing WASM ratchet).

Enumerates the ENTIRE currently-admitted `@wasm` shape space (scalar params
int/float/bool; list params list[int]/list[float]/list[bool]; scalar returns; every
operator / statement / builtin the WASM backend lowers — see
`crates/pyths_hir/src/wasm_analysis.rs::{check_expr, check_stmt, check_signature}`)
as a DETERMINISTIC generated corpus and, per program, diffs

    * CPython (the pinned oracle, PYTHS_ORACLE_PYTHON; the semantic ground truth),
    * the shipped `pyths --target js` output (the JS backend — the Cluster-A reference
      lowering; a JS-vs-CPython mismatch is itself a finding: the reference was wrong),
    * the shipped `pyths --target js+wasm` output, on TWO channels:
        - `glue`   — the generated entry (what a user calls: marshalling + the exact
                     JS-twin re-run on overflow/trap),
        - `direct` — the raw `.wasm` export with NO twin (run_wasm_direct.mjs), so a
                     trap or a wrong value in the EMITTED CODE cannot be masked by the
                     twin returning the CPython answer (dual-track masking).

Outcome algebra (per call): ok(value, lists-after) | err(kind) | trap | ovf |
not-exported | compile-fail.  A `direct` `ovf` defers to the twin by contract (the
glue arm must then match CPython).  A `direct` `trap` against a CPython exception is
an error-OCCURRENCE match with the KIND unknown (counted as `kind-gap`, not RED — the
WASM path traps where it has no error model); every other disagreement is RED.

The net is its OWN negative control: at the base commit it goes RED on #485 (short-
circuit), #487 (list ==/truthiness), #484 (write-back), #486 (arity), #474
(determinism) — the committed evidence is `tests/differential/wasm_net/evidence/`.
`baseline.json` is a TWO-SIDED ratchet: a RED row not in the baseline is a regression,
a baseline row that turned GREEN is stale (the fix must shrink the baseline — the
"known-red" list can only go down), and the row COUNT must match exactly (a shrunk or
grown corpus fails until `--update-baseline` is run deliberately — never in CI).
There is NO skip flag.

Run from the repo root (needs target/{release,debug}/pyths[.exe], node, the oracle):
    python tests/differential/wasm_net/wasm_net.py [--only FAMILY] [--update-baseline]
Env: PYTHS_BIN, PYTHS_ORACLE_PYTHON (e.g. "py -3.14"), WASM_NET_SCRATCH.
"""
from __future__ import annotations

import argparse
import json
import math
import os
import re
import shlex
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent.parent
_EXE = "pyths.exe" if sys.platform == "win32" else "pyths"
PYTHS = Path(os.environ.get("PYTHS_BIN") or next(
    (str(p) for p in (ROOT / "target" / "release" / _EXE, ROOT / "target" / "debug" / _EXE) if p.exists()),
    str(ROOT / "target" / "release" / _EXE),
))
ORACLE = shlex.split(os.environ.get("PYTHS_ORACLE_PYTHON", "python"))
RUNTIME_INDEX = (ROOT / "runtime" / "src" / "index.js").resolve()
SCRATCH = Path(os.environ.get("WASM_NET_SCRATCH") or (HERE / ".scratch"))
BASELINE = HERE / "baseline.json"
DETERMINISM_N = 24

# ----------------------------------------------------------------------------- corpus


@dataclass
class Fn:
    name: str
    params: list[str]
    ret: str
    calls: list[list]
    # True = must be admitted; False = admission is expected to REFUSE (stays JS);
    # None = either is acceptable (an arity row: build refusal OR CPython's TypeError).
    expect_wasm: bool | None = True
    # [(i, j)] with i < j: argument j is the SAME object as argument i (identity semantics).
    alias: list = field(default_factory=list)

    def __post_init__(self):
        for i, j in self.alias:
            assert i < j, f"alias pairs are (src, dst) with src < dst: {self.alias}"


@dataclass
class Module:
    family: str
    name: str
    source: str
    fns: list[Fn] = field(default_factory=list)


INT_VALS = [0, 1, -1, 2, -2, 3, 7, -7, 10, 100, 2**31 - 1, -(2**31), 2**53, -(2**53), 2**62, -(2**63)]
INT_SMALL = [0, 1, -1, 2, -3, 7]
POW_EXP_INT = [0, 1, 2, 3, 10, -1, -2]
SHIFT_RHS = [0, 1, 2, 7, 31, 63, 64, 70, -1]
FLOAT_VALS = [0.0, -0.0, 1.0, -1.0, 0.5, 2.5, -3.75, 1e300, 1e-300, math.inf, -math.inf, math.nan]
FLOAT_POW_BASE = [0.0, 1.0, 2.5, 0.5, 1e300, -2.0, -0.5]
FLOAT_POW_EXP = [0.0, 1.0, 2.0, 0.5, 3.0, -1.0]
BOOL_VALS = [True, False]
VALS = {"int": INT_VALS, "float": FLOAT_VALS, "bool": BOOL_VALS}
VALS_SMALL = {"int": INT_VALS[:8], "float": FLOAT_VALS[:8], "bool": BOOL_VALS}


def enc(v):
    """JSON-encode an argument so it survives the hop exactly (big ints as strings,
    non-finite / negative-zero floats as tokens)."""
    if isinstance(v, bool):
        return v
    if isinstance(v, int):
        return str(v)
    if isinstance(v, float):
        if math.isnan(v):
            return "nan"
        if math.isinf(v):
            return "inf" if v > 0 else "-inf"
        if v == 0.0 and math.copysign(1.0, v) < 0:
            return "-0.0"
        return v
    if isinstance(v, list):
        return [enc(e) for e in v]
    raise TypeError(type(v))


def calls_of(*value_lists):
    out = []

    def rec(prefix, rest):
        if not rest:
            out.append([enc(p) for p in prefix])
            return
        for v in rest[0]:
            rec(prefix + [v], rest[1:])

    rec([], list(value_lists))
    return out


ARITH = [("add", "+"), ("sub", "-"), ("mul", "*"), ("truediv", "/"), ("floordiv", "//"), ("mod", "%"), ("pow", "**")]
BITS = [("bitand", "&"), ("bitor", "|"), ("bitxor", "^"), ("shl", "<<"), ("shr", ">>")]
CMPS = [("eq", "=="), ("ne", "!="), ("lt", "<"), ("le", "<="), ("gt", ">"), ("ge", ">=")]
LOGIC = [("and", "and"), ("or", "or")]


def join_num(t1, t2):
    return "float" if "float" in (t1, t2) else "int"


def binop_family() -> list[Module]:
    mods = []
    for t1 in ("int", "float", "bool"):
        for t2 in ("int", "float", "bool"):
            lines = []
            fns = []
            same = t1 == t2
            v1 = VALS[t1] if same else VALS_SMALL[t1]
            v2 = VALS[t2] if same else VALS_SMALL[t2]
            for nm, sym in ARITH:
                ret = "float" if nm == "truediv" else join_num(t1, t2)
                if nm == "pow":
                    if "float" in (t1, t2):
                        a = FLOAT_POW_BASE if t1 == "float" else VALS_SMALL[t1]
                        b = FLOAT_POW_EXP if t2 == "float" else POW_EXP_INT
                    else:
                        a = INT_SMALL if t1 == "int" else BOOL_VALS
                        b = POW_EXP_INT if t2 == "int" else BOOL_VALS
                    # ONLY a negative base with a FRACTIONAL exponent is a COMPLEX value in
                    # Python (outside the numeric-kernel domain); `(-2.0) ** 2.0` stays.
                    calls = [c for c in calls_of(a, b)
                             if not (float(c[0]) < 0 and isinstance(c[1], float) and c[1] != int(c[1]))]
                else:
                    calls = calls_of(v1, v2)
                lines.append(f"def op_{nm}(a: {t1}, b: {t2}) -> {ret}:\n    return a {sym} b\n")
                fns.append(Fn(f"op_{nm}", [t1, t2], ret, calls))
            if "float" not in (t1, t2):
                for nm, sym in BITS:
                    ret = "int"
                    b = SHIFT_RHS if (nm in ("shl", "shr") and t2 == "int") else v2
                    lines.append(f"def op_{nm}(a: {t1}, b: {t2}) -> {ret}:\n    return a {sym} b\n")
                    fns.append(Fn(f"op_{nm}", [t1, t2], ret, calls_of(v1, b)))
            for nm, sym in CMPS:
                lines.append(f"def op_{nm}(a: {t1}, b: {t2}) -> bool:\n    return a {sym} b\n")
                fns.append(Fn(f"op_{nm}", [t1, t2], "bool", calls_of(v1, v2)))
            for nm, sym in LOGIC:
                ret = t1 if same else join_num(t1, t2)
                lines.append(f"def op_{nm}(a: {t1}, b: {t2}) -> {ret}:\n    return a {sym} b\n")
                fns.append(Fn(f"op_{nm}", [t1, t2], ret, calls_of(v1, v2)))
            # Chained comparison over the pair (middle operand is a plain name).
            lines.append(f"def op_chain(a: {t1}, b: {t2}, c: {t1}) -> bool:\n    return a < b <= c\n")
            fns.append(Fn("op_chain", [t1, t2, t1], "bool", calls_of(VALS_SMALL[t1], VALS_SMALL[t2], VALS_SMALL[t1][:4])))
            mods.append(Module("binop", f"binop_{t1}_{t2}", "\n".join(lines), fns))
    return mods


def unop_family() -> list[Module]:
    mods = []
    for t in ("int", "float", "bool"):
        neg_ret = "int" if t == "bool" else t
        lines = [
            f"def u_neg(a: {t}) -> {neg_ret}:\n    return -a\n",
            f"def u_pos(a: {t}) -> {neg_ret}:\n    return +a\n",
            f"def u_not(a: {t}) -> bool:\n    return not a\n",
            f"def u_abs(a: {t}) -> {neg_ret}:\n    return abs(a)\n",
            f"def u_int(a: {t}) -> int:\n    return int(a)\n",
            f"def u_float(a: {t}) -> float:\n    return float(a)\n",
            f"def u_truth_if(a: {t}) -> int:\n    if a:\n        return 1\n    return 0\n",
            f"def u_truth_while(a: {t}) -> int:\n    while a:\n        return 1\n    return 0\n",
            f"def u_truth_assert(a: {t}) -> int:\n    assert a\n    return 1\n",
            f"def u_truth_ifexp(a: {t}) -> int:\n    return 1 if a else 0\n",
            f"def u_bool_ret(a: {t}) -> bool:\n    return a\n",
        ]
        fns = [
            Fn("u_neg", [t], neg_ret, calls_of(VALS[t])),
            Fn("u_pos", [t], neg_ret, calls_of(VALS[t])),
            Fn("u_not", [t], "bool", calls_of(VALS[t])),
            Fn("u_abs", [t], neg_ret, calls_of(VALS[t])),
            Fn("u_int", [t], "int", calls_of(VALS[t])),
            Fn("u_float", [t], "float", calls_of(VALS[t])),
            Fn("u_truth_if", [t], "int", calls_of(VALS[t])),
            Fn("u_truth_while", [t], "int", calls_of(VALS[t])),
            Fn("u_truth_assert", [t], "int", calls_of(VALS[t])),
            Fn("u_truth_ifexp", [t], "int", calls_of(VALS[t])),
            Fn("u_bool_ret", [t], "bool", calls_of(VALS[t])),
        ]
        if t != "float":
            lines.append(f"def u_invert(a: {t}) -> int:\n    return ~a\n")
            fns.append(Fn("u_invert", [t], "int", calls_of(VALS[t])))
        mods.append(Module("unop", f"unop_{t}", "\n".join(lines), fns))
    return mods


LIST_INPUTS = {
    "int": [[], [1], [1, 2], [1, 2, 3], [1, 3], [2, 1], [0], [0, 0], [-1, 2**62]],
    "float": [[], [1.0], [1.0, 2.0], [1.0, 2.0, 3.0], [1.0, 3.0], [2.0, 1.0], [0.0], [0.0, -0.0], [math.nan], [1.0, math.inf]],
    "bool": [[], [True], [True, False], [False, True], [False], [False, False], [True, True, True]],
}


def listcmp_family() -> list[Module]:
    """#487: list-vs-list comparison (element-wise in Python, handles on WASM) and
    list truthiness (length test in Python, constant/handle on WASM)."""
    mods = []
    for e in ("int", "float", "bool"):
        L = f"list[{e}]"
        ins = LIST_INPUTS[e]
        lines, fns = [], []
        for nm, sym in CMPS:
            lines.append(f"def cmp_{nm}(a: {L}, b: {L}) -> bool:\n    return a {sym} b\n")
            fns.append(Fn(f"cmp_{nm}", [L, L], "bool", calls_of(ins, ins)))
        lines.append(f"def cmp_chain(a: {L}, b: {L}, c: {L}) -> bool:\n    return a == b == c\n")
        fns.append(Fn("cmp_chain", [L, L, L], "bool", calls_of(ins[:5], ins[:5], ins[:5])))
        # The SAME list on both sides: CPython's identity shortcut makes `xs == xs` True
        # and `xs < xs` False even through NaN (opus r1/SF6).
        for nm, sym in CMPS:
            lines.append(f"def self_{nm}(a: {L}) -> bool:\n    return a {sym} a\n")
            fns.append(Fn(f"self_{nm}", [L], "bool", calls_of(ins)))
        # The same list passed TWICE (`f(xs, xs)`): identity holds for CPython, the JS
        # runtime (`a === b`) and the raw export (one pointer); the GLUE marshals each
        # argument into its own buffer, so it cannot see the identity (opus r2/NEW-5,
        # tracked as NEW-W11 for the marshalling chunk).
        if e == "float":
            lines.append(f"def alias_eq(a: {L}, b: {L}) -> bool:\n    return a == b\n")
            fns.append(Fn("alias_eq", [L, L], "bool", [[enc(x), enc(x)] for x in ins], alias=[(0, 1)]))
        # Rows the TYPED admission must refuse (opus r2/NEW-1, NEW-2, NEW-7): a list-valued
        # subscript / for-target in an and/or VALUE, a container under a scalar return, an
        # int stored into a list[bool] slot. `expect_wasm=False`: refusal is the property.
        if e == "int":
            lines.append("def sub_value(k: int) -> int:\n    return [[1, 2]][0] and k\n")
            fns.append(Fn("sub_value", ["int"], "int", calls_of([0, 5, 9]), expect_wasm=False))
            lines.append("def for_value(xs: list[int], k: int) -> int:\n    for b in [xs]:\n        return b and k\n    return 0\n")
            fns.append(Fn("for_value", [L, "int"], "int", calls_of(ins[:3], [0, 5]), expect_wasm=False))
            lines.append("def ret_list(xs: list[int]) -> int:\n    return xs\n")
            fns.append(Fn("ret_list", [L], "int", calls_of(ins[:3]), expect_wasm=False))
            lines.append("def ret_list_bool(xs: list[int]) -> bool:\n    return xs\n")
            fns.append(Fn("ret_list_bool", [L], "bool", calls_of(ins[:3]), expect_wasm=False))
            lines.append("def two_lists(xs: list[int], ys: list[int]) -> int:\n    if xs or ys:\n        return (xs or ys)[0]\n    return -1\n")
            fns.append(Fn("two_lists", [L, L], "int", calls_of(ins[:4], ins[:4])))
        if e == "bool":
            lines.append("def store_int(xs: list[bool], v: int) -> int:\n    xs[0] = v\n    return 0\n")
            fns.append(Fn("store_int", [L, "int"], "int", calls_of([[False, False]], [7, 1, 2**32]), expect_wasm=False))
            lines.append("def store_int_aug(xs: list[bool], v: int) -> int:\n    xs[0] += v\n    return 0\n")
            fns.append(Fn("store_int_aug", [L, "int"], "int", calls_of([[True, False]], [1, 2**32]), expect_wasm=False))
        if e == "int":
            # opus r3/NEW-r3-1: list-valued IfExpr / `+` shapes the lowering typed as I64.
            lines.append("def ret_ifexp(xs: list[int], ys: list[int], c: int) -> int:\n    return xs if c else ys\n")
            fns.append(Fn("ret_ifexp", [L, L, "int"], "int", calls_of(ins[:3], ins[:3], [0, 1]), expect_wasm=False))
            lines.append("def ret_concat(xs: list[int], ys: list[int]) -> int:\n    return xs + ys\n")
            fns.append(Fn("ret_concat", [L, L], "int", calls_of(ins[:3], ins[:3]), expect_wasm=False))
            lines.append("def ifexp_and_value(xs: list[int], ys: list[int], c: int, k: int) -> int:\n    return (xs if c else ys) and k\n")
            fns.append(Fn("ifexp_and_value", [L, L, "int", "int"], "int", calls_of(ins[:3], ins[:3], [0, 1], [0, 9]), expect_wasm=False))
            lines.append("def len_ifexp(xs: list[int], ys: list[int], c: int) -> int:\n    return len(xs if c else ys)\n")
            fns.append(Fn("len_ifexp", [L, L, "int"], "int", calls_of(ins[:3], ins[:3], [0, 1]), expect_wasm=False))
            lines.append("def assert_msg(ys: list[int], c: int) -> int:\n    assert c, ys[100]\n    return 1\n")
            fns.append(Fn("assert_msg", [L, "int"], "int", calls_of(ins[:3], [0, 1]), expect_wasm=False))
            # opus r4: the store rule over every slot type; a raising raise-argument; -> None + value.
            lines.append("def store_float_into_int(xs: list[int], v: float) -> float:\n    xs[0] = v\n    return xs[0]\n")
            fns.append(Fn("store_float_into_int", [L, "float"], "float", calls_of([[7, 8]], [2.5, 0.5, math.nan]), expect_wasm=False))
            lines.append("def store_list_into_elem(xs: list[int], ys: list[int]) -> int:\n    xs[0] = ys\n    return xs[0]\n")
            fns.append(Fn("store_list_into_elem", [L, L], "int", calls_of([[7, 8]], [[1, 2]]), expect_wasm=False))
            lines.append("def raise_msg(ys: list[int], c: int) -> int:\n    if c:\n        raise ValueError(ys[100])\n    return 0\n")
            fns.append(Fn("raise_msg", [L, "int"], "int", calls_of(ins[:2], [0, 1]), expect_wasm=False))
            lines.append("def ret_none_value(xs: list[int]) -> None:\n    return xs\n")
            fns.append(Fn("ret_none_value", [L], "None", calls_of(ins[:2]), expect_wasm=False))
            # opus r5: the local slot, the return slot and the call-argument slot.
            lines.append("def ann_int_from_float(fs: list[float]) -> float:\n    x: int = fs[0]\n    return x + 0.5\n")
            fns.append(Fn("ann_int_from_float", ["list[float]"], "float", calls_of([[2.5, 1.0], [-2.5], [0.25]]), expect_wasm=False))
            lines.append("def ann_int_from_list(xs: list[int], ys: list[int]) -> int:\n    x: int = ys\n    return x - x\n")
            fns.append(Fn("ann_int_from_list", [L, L], "int", calls_of([[7, 8]], [[1, 2]]), expect_wasm=False))
            lines.append("def ret_float_as_int(a: float) -> int:\n    return a\n")
            fns.append(Fn("ret_float_as_int", ["float"], "int", calls_of([2.5, -2.5, 0.5, math.nan]), expect_wasm=False))
            lines.append("def ret_div_as_int(a: int, b: int) -> int:\n    return a / b\n")
            fns.append(Fn("ret_div_as_int", ["int", "int"], "int", calls_of([7, 1], [2, 0]), expect_wasm=False))
            lines.append("def ret_elem_as_int(fs: list[float]) -> int:\n    return fs[0]\n")
            fns.append(Fn("ret_elem_as_int", ["list[float]"], "int", calls_of([[2.5], [1.0]]), expect_wasm=False))
            lines.append("def callee_int(n: int) -> int:\n    return n + 1\n")
            fns.append(Fn("callee_int", ["int"], "int", calls_of([0, 5])))
            lines.append("def call_arg_float_into_int(fs: list[float]) -> int:\n    return callee_int(fs[0])\n")
            fns.append(Fn("call_arg_float_into_int", ["list[float]"], "int", calls_of([[2.5], [1.0]]), expect_wasm=False))
        lines.append(f"def truth_if(xs: {L}) -> int:\n    if xs:\n        return 1\n    return 0\n")
        lines.append(f"def truth_while(xs: {L}) -> int:\n    while xs:\n        return 1\n    return 0\n")
        lines.append(f"def truth_assert(xs: {L}) -> int:\n    assert xs\n    return 1\n")
        lines.append(f"def truth_not(xs: {L}) -> bool:\n    return not xs\n")
        lines.append(f"def truth_ifexp(xs: {L}) -> int:\n    return 1 if xs else 0\n")
        lines.append(f"def truth_and(xs: {L}, k: int) -> int:\n    if xs and k > 0:\n        return 1\n    return 0\n")
        lines.append(f"def truth_or(xs: {L}, k: int) -> int:\n    if xs or k > 0:\n        return 1\n    return 0\n")
        # and/or with a list operand used as a VALUE (not a test): Python yields the
        # list or the scalar; no single WASM type carries both -> admission REFUSES it
        # (opus r1/B2; `expect_wasm=False`) and the JS path must match CPython.
        lines.append(f"def and_value(xs: {L}, k: int) -> int:\n    return xs and k\n")
        fns.append(Fn("and_value", [L, "int"], "int", calls_of(ins[:4], [0, 5]), expect_wasm=False))
        for nm in ("truth_if", "truth_while", "truth_assert", "truth_ifexp"):
            fns.append(Fn(nm, [L], "int", calls_of(ins)))
        fns.append(Fn("truth_not", [L], "bool", calls_of(ins)))
        fns.append(Fn("truth_and", [L, "int"], "int", calls_of(ins, [0, 1])))
        fns.append(Fn("truth_or", [L, "int"], "int", calls_of(ins, [0, 1])))
        if e == "int":
            # The mutation-in-branch witness from #487: CPython leaves out == [9, 2].
            lines.append("def eq_branch(out: list[int], o: list[int]) -> int:\n    if out == o:\n        out[0] = 9\n    else:\n        out[0] = 8\n    return 0\n")
            fns.append(Fn("eq_branch", [L, L], "int", calls_of([[1, 2], [1, 3], [1]], [[1, 2], [1, 2, 3], [2, 2]])))
        mods.append(Module("listcmp", f"listcmp_{e}", "\n".join(lines), fns))
    return mods


def shortcircuit_family() -> list[Module]:
    """#485: `and`/`or` must short-circuit (right operand evaluated only when it decides)
    and return the deciding OPERAND; chained comparisons must short-circuit and evaluate
    the middle operand once."""
    src = '''
def sc_while_and(xs: list[int], n: int) -> int:
    j = 0
    while j < n and xs[j] > 0:
        j = j + 1
    return j

def sc_if_and(xs: list[int], i: int) -> int:
    if i < len(xs) and xs[i] > 0:
        return 1
    return 0

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

def sc_ifexp_and(xs: list[int], i: int) -> int:
    return 1 if (i < len(xs) and xs[i] > 0) else 0

def sc_ifexp_guard(xs: list[int], i: int) -> int:
    return xs[i] if i < len(xs) else -1

def sc_chain_guard(xs: list[int], i: int, n: int) -> bool:
    return i < n < xs[i]

def sc_and_or_mix(a: int, b: int, c: int) -> int:
    return a and b or c

def sc_or_and_mix(a: int, b: int, c: int) -> int:
    return a or b and c

def sc_value_and_f(a: float, b: float) -> float:
    return a and b

def sc_value_or_f(a: float, b: float) -> float:
    return a or b

def bump(xs: list[int]) -> int:
    xs[0] = xs[0] + 1
    return xs[0]

def sc_and_bump(xs: list[int], flag: bool) -> int:
    return flag and bump(xs)

def sc_or_bump(xs: list[int], flag: bool) -> int:
    return flag or bump(xs)

def sc_chain_bump(xs: list[int]) -> bool:
    return 0 < bump(xs) < 100

def sc_chain_bump_tail(xs: list[int], k: int) -> bool:
    return k < 0 < bump(xs)
'''
    xs_in = [[1], [1, 1, 0], [0], [], [5, 5, 5]]
    idx = [0, 1, 2, 3]
    fns = [
        Fn("sc_while_and", ["list[int]", "int"], "int", calls_of(xs_in, idx)),
        Fn("sc_if_and", ["list[int]", "int"], "int", calls_of(xs_in, idx)),
        Fn("sc_or", ["list[int]", "int"], "int", calls_of(xs_in, idx)),
        Fn("sc_nested", ["list[int]", "int", "bool"], "int", calls_of(xs_in, idx, BOOL_VALS)),
        Fn("sc_not_and", ["list[int]", "int"], "bool", calls_of(xs_in, idx)),
        Fn("sc_ifexp_and", ["list[int]", "int"], "int", calls_of(xs_in, idx)),
        Fn("sc_ifexp_guard", ["list[int]", "int"], "int", calls_of(xs_in, idx)),
        Fn("sc_chain_guard", ["list[int]", "int", "int"], "bool", calls_of(xs_in, idx, idx)),
        Fn("sc_and_or_mix", ["int", "int", "int"], "int", calls_of(INT_SMALL, INT_SMALL, INT_SMALL)),
        Fn("sc_or_and_mix", ["int", "int", "int"], "int", calls_of(INT_SMALL, INT_SMALL, INT_SMALL)),
        Fn("sc_value_and_f", ["float", "float"], "float", calls_of(FLOAT_VALS, FLOAT_VALS)),
        Fn("sc_value_or_f", ["float", "float"], "float", calls_of(FLOAT_VALS, FLOAT_VALS)),
        Fn("bump", ["list[int]"], "int", calls_of([[0], [5]])),
        Fn("sc_and_bump", ["list[int]", "bool"], "int", calls_of([[0], [5]], BOOL_VALS)),
        Fn("sc_or_bump", ["list[int]", "bool"], "int", calls_of([[0], [5]], BOOL_VALS)),
        Fn("sc_chain_bump", ["list[int]"], "bool", calls_of([[0], [5], [99]])),
        Fn("sc_chain_bump_tail", ["list[int]", "int"], "bool", calls_of([[0], [5]], [-1, 1])),
    ]
    return [Module("shortcircuit", "shortcircuit", src, fns)]


def writeback_family() -> list[Module]:
    """#484: in-place mutation of a list parameter must be visible to the caller
    (CPython aliasing) on the user-facing glue path, for every element type."""
    src = '''
def fill_int(out: list[int], n: int) -> int:
    for i in range(n):
        out[i] = i * i
    return n

def fill_float(out: list[float], n: int) -> int:
    for i in range(n):
        out[i] = i * 0.5
    return n

def fill_bool(out: list[bool], n: int) -> int:
    for i in range(n):
        out[i] = i % 2 == 0
    return n

def swap_first(a: list[int], b: list[int]) -> int:
    t = a[0]
    a[0] = b[0]
    b[0] = t
    return 0

def scale_inplace(xs: list[float], k: float) -> float:
    for i in range(len(xs)):
        xs[i] = xs[i] * k
    return xs[0]

def aug_sub(xs: list[int], i: int, v: int) -> int:
    xs[i] += v
    return xs[i]
'''
    fns = [
        Fn("fill_int", ["list[int]", "int"], "int", [[enc([0, 0, 0]), enc(3)], [enc([0] * 5), enc(5)], [enc([0]), enc(1)], [enc([]), enc(0)], [enc([0, 0]), enc(3)]]),
        Fn("fill_float", ["list[float]", "int"], "int", [[enc([0.0, 0.0, 0.0]), enc(3)], [enc([0.0]), enc(1)], [enc([]), enc(0)]]),
        Fn("fill_bool", ["list[bool]", "int"], "int", [[enc([False, False, False]), enc(3)], [enc([True]), enc(1)], [enc([]), enc(0)]]),
        Fn("swap_first", ["list[int]", "list[int]"], "int", calls_of([[1, 2], [7]], [[3, 4], [9]])),
        Fn("scale_inplace", ["list[float]", "float"], "float", calls_of([[1.0, 2.0], [0.5]], [2.0, -1.0, 0.0])),
        Fn("aug_sub", ["list[int]", "int", "int"], "int", calls_of([[1, 2, 3]], [0, 2, 3], [5, -5])),
    ]
    return [Module("writeback", "writeback", src, fns)]


def writeback_refusal_family() -> list[Module]:
    """#484 fixed-capacity contract: a LENGTH-CHANGING list mutation cannot be
    reflected into the fixed-cap linear-memory buffer, so it must be REFUSED
    loudly — never silently truncated. The compiler already refuses the
    length-changing list methods at WASM admission (they "stay JS", the faithful
    path), which is the primary loud refusal; the glue's `__list_write_back`
    length guard is the belt-and-suspenders second layer. Each kernel is
    `expect_wasm=False`: the direct (raw-WASM) arm must report it not-exported
    (refused), and the JS/glue arms (which run the faithful JS twin) must match
    CPython — including the length change itself."""
    src = '''
def grow(xs: list[int], n: int) -> int:
    for i in range(n):
        xs.append(i)
    return len(xs)

def shrink(xs: list[int]) -> int:
    xs.pop()
    return len(xs)

def wipe(xs: list[int]) -> int:
    xs.clear()
    return len(xs)

def extend2(xs: list[int], ys: list[int]) -> int:
    xs.extend(ys)
    return len(xs)
'''
    fns = [
        Fn("grow", ["list[int]", "int"], "int", calls_of([[1, 2], [0]], [0, 2]), expect_wasm=False),
        Fn("shrink", ["list[int]"], "int", calls_of([[1, 2, 3], [9]]), expect_wasm=False),
        Fn("wipe", ["list[int]"], "int", calls_of([[1, 2, 3], []]), expect_wasm=False),
        Fn("extend2", ["list[int]", "list[int]"], "int", calls_of([[1, 2], [7]], [[3, 4], []]), expect_wasm=False),
    ]
    return [Module("writeback_refusal", "writeback_refusal", src, fns)]


def arity_family() -> list[Module]:
    """#486: every lowered builtin at the WRONG arity must be refused at build time
    (function stays JS, which then matches CPython) or raise CPython's TypeError. One
    module per case so a compiler panic on one cannot hide the others."""
    cases = [
        ("len2", "def len2(out: list[int], o: list[int]) -> int:\n    return len(out, o)\n", ["list[int]", "list[int]"], "int", calls_of([[1, 2, 3]], [[4]])),
        ("len0", "def len0(a: int) -> int:\n    return len()\n", ["int"], "int", calls_of([1])),
        ("abs2", "def abs2(a: int, b: int) -> int:\n    return abs(a, b)\n", ["int", "int"], "int", calls_of([-1], [2])),
        ("abs0", "def abs0(a: int) -> int:\n    return abs()\n", ["int"], "int", calls_of([1])),
        ("int2", "def int2(a: float, b: int) -> int:\n    return int(a, b)\n", ["float", "int"], "int", calls_of([2.5], [10])),
        ("int0", "def int0(a: int) -> int:\n    return int()\n", ["int"], "int", calls_of([1])),
        ("float0", "def float0(a: int) -> float:\n    return float()\n", ["int"], "float", calls_of([1])),
        ("float2", "def float2(a: int, b: int) -> float:\n    return float(a, b)\n", ["int", "int"], "float", calls_of([1], [2])),
        ("sqrt2", "import math\ndef sqrt2(a: float, b: float) -> float:\n    return math.sqrt(a, b)\n", ["float", "float"], "float", calls_of([4.0], [2.0])),
        ("pow1", "import math\ndef pow1(a: float) -> float:\n    return math.pow(a)\n", ["float"], "float", calls_of([4.0])),
        ("range4", "def range4(a: int) -> int:\n    t = 0\n    for i in range(0, a, 1, 2):\n        t += i\n    return t\n", ["int"], "int", calls_of([3])),
    ]
    mods = []
    for name, src, params, ret, calls in cases:
        mods.append(Module("arity", f"arity_{name}", src, [Fn(name, params, ret, calls, expect_wasm=None)]))
    return mods


def stmts_family() -> list[Module]:
    src = '''
import math

def while_else(n: int) -> int:
    i = 0
    s = 0
    while i < n:
        s += i
        i += 1
    else:
        s += 100
    return s

def while_break(n: int) -> int:
    i = 0
    while True:
        i += 1
        if i >= n:
            break
    return i

def for_step(a: int, b: int, s: int) -> int:
    t = 0
    for i in range(a, b, s):
        t += i
    return t

def for_list_sum(xs: list[int]) -> int:
    t = 0
    for x in xs:
        t += x
    return t

def nested_continue(n: int, m: int) -> int:
    t = 0
    for i in range(n):
        for j in range(m):
            if (i + j) % 2 == 0:
                continue
            t += i * j
    return t

def local_list(n: int) -> int:
    buf = [0] * n
    for i in range(n):
        buf[i] = i
    t = 0
    for i in range(n):
        t += buf[i]
    return t

def raise_val(a: int) -> int:
    if a < 0:
        raise ValueError("neg")
    return a

def try_zero(a: int, b: int) -> int:
    try:
        return a // b
    except ZeroDivisionError:
        return -1

def assert_pos(a: int) -> int:
    assert a > 0
    return a

def ifexp(a: int, b: int) -> int:
    return a if a > b else b

def elif_chain(a: int) -> int:
    if a < 0:
        return -1
    elif a == 0:
        return 0
    elif a < 10:
        return 1
    else:
        return 2

def math_sqrt(x: float) -> float:
    return math.sqrt(x)

def math_floor(x: float) -> int:
    return math.floor(x)

def math_fabs(x: float) -> float:
    return math.fabs(x)

def bool_ret(a: int, b: int) -> bool:
    return a < b

def none_ret(xs: list[int]) -> None:
    xs[0] = 1

def index_load(xs: list[float], i: int) -> float:
    return xs[i]

def const_pi() -> float:
    return math.pi

def augassign_all(a: int, b: int) -> int:
    a += b
    a -= 1
    a *= 2
    a //= 3
    a %= 7
    return a
'''
    fns = [
        Fn("while_else", ["int"], "int", calls_of([0, 1, 3, 5])),
        Fn("while_break", ["int"], "int", calls_of([0, 1, 3, 5])),
        Fn("for_step", ["int", "int", "int"], "int", calls_of([0, 5, -5], [0, 5, -5], [1, 2, -1, 0])),
        Fn("for_list_sum", ["list[int]"], "int", calls_of(LIST_INPUTS["int"])),
        Fn("nested_continue", ["int", "int"], "int", calls_of([0, 1, 3, 4], [0, 2, 3])),
        Fn("local_list", ["int"], "int", calls_of([0, 1, 5, 10])),
        Fn("raise_val", ["int"], "int", calls_of([-1, 0, 1])),
        Fn("try_zero", ["int", "int"], "int", calls_of([7, -7], [0, 2, -2])),
        Fn("assert_pos", ["int"], "int", calls_of([-1, 0, 1])),
        Fn("ifexp", ["int", "int"], "int", calls_of(INT_SMALL, INT_SMALL)),
        Fn("elif_chain", ["int"], "int", calls_of([-5, 0, 5, 50])),
        Fn("math_sqrt", ["float"], "float", calls_of([0.0, 4.0, 2.0, -1.0, math.inf, math.nan])),
        Fn("math_floor", ["float"], "int", calls_of([0.0, 2.5, -2.5, 1e300, math.inf, math.nan])),
        Fn("math_fabs", ["float"], "float", calls_of(FLOAT_VALS)),
        Fn("bool_ret", ["int", "int"], "bool", calls_of(INT_SMALL, INT_SMALL)),
        Fn("none_ret", ["list[int]"], "None", calls_of([[0, 0], [5]])),
        Fn("index_load", ["list[float]", "int"], "float", calls_of([[1.5, -0.0, math.nan], []], [0, 1, 2, 3, -1, 2**40])),
        Fn("const_pi", [], "float", [[]]),
        Fn("augassign_all", ["int", "int"], "int", calls_of(INT_SMALL, INT_SMALL)),
    ]
    # The try/except-IndexError subscript is kept in ITS OWN module: its emitted WASM is
    # invalid (a codegen finding), and at the base commit the compile-time fallback that
    # drops it also drops a hash-order-dependent subset of its SIBLINGS (#474 class) —
    # which would make every other `stmts` row flake. The reproducing shape (this body
    # plus the siblings) is exercised 24x by the `determinism::stmts_shape` lane.
    try_fns = [Fn("try_index", ["list[int]", "int"], "int", calls_of([[1, 2], []], [0, 1, 2, -1]))]
    return [Module("stmts", "stmts", src, fns), Module("stmts", "try_index", TRY_INDEX_SRC, try_fns)]


TRY_INDEX_SRC = """
def try_index(xs: list[int], i: int) -> int:
    try:
        v = xs[i]
    except IndexError:
        v = -1
    return v
"""


def shadow_family() -> list[Module]:
    """Name-binding arm (#491): a user `def` SHADOWS the builtin of the same name
    (CPython semantics), so the user function must win over the builtin lowering in
    EVERY call-classification path — the direct call, the return-type inference, the
    `for`-loop range dispatch, and the `from math import X as Y` alias. When a user
    shadow is INELIGIBLE for WASM the caller is DEMOTED (routed to JS, which honors the
    user def), never silently lowered as the builtin. Each row is a paired control:
    GREEN with the fix, RED on revert (the builtin/alias answer differs from the user
    fn's). CPython + the JS backend honor the user definition on every arm."""
    # Scalar direct-call shadow (the original NEW-W8 rows).
    scalar_src = '''
def len(x: int) -> int:
    return x + 1000

def use_len(x: int) -> int:
    return len(x)

def abs(x: int) -> int:
    return x - 1000

def use_abs(x: int) -> int:
    return abs(x)
'''
    scalar = Module("shadow", "shadow", scalar_src, [
        Fn("use_len", ["int"], "int", calls_of([0, 5, -5])),
        Fn("use_abs", ["int"], "int", calls_of([0, 5, -5])),
    ])

    # Blocker 1 (#491): a `for … in range(n)` where `range` is a user `def` is NOT the
    # builtin range loop. Here user `range` returns a 2-char string; CPython iterates
    # it (len 2, independent of n). WASM must NOT run builtin range(n): admission
    # demotes the caller to JS (which iterates the string). On revert the builtin range
    # loop runs `n` times -> wrong count. Refused-on-WASM (stays JS), so expect_wasm=False.
    range_src = '''
def range(n: int) -> str:
    return "xx"

def use_shadow_range(n: int) -> int:
    total = 0
    for ch in range(n):
        total += 1
    return total
'''
    shadow_range = Module("shadow_range", "shadow_range", range_src, [
        Fn("use_shadow_range", ["int"], "int", calls_of([0, 3, 5]), expect_wasm=False),
    ])

    # Blocker 2 (#491): a user `def abs` that is INELIGIBLE for WASM (returns a list).
    # `use_shadow_abs` calls it; the builtin-name whitelist must NOT rescue the caller.
    # CPython: abs(x)=[x], len=1. On revert the caller is admitted and `abs` lowers as
    # the builtin (int), then len() of a non-pointer yields 0 -> wrong. Demoted to JS.
    abs_list_src = '''
def abs(x: int) -> list[int]:
    return [x]

def use_shadow_abs(x: int) -> int:
    return len(abs(x))
'''
    shadow_abs_list = Module("shadow_abs_list", "shadow_abs_list", abs_list_src, [
        Fn("use_shadow_abs", ["int"], "int", calls_of([0, 5, -5]), expect_wasm=False),
    ])

    # Blocker 3 (#491): a prior `from math import sqrt as abs` must NOT win over a later
    # user `def abs`. Arity matches the builtin (1 arg) so the builtin-arity refusal does
    # NOT mask the bug; `abs` has no return annotation -> INELIGIBLE for WASM. With the
    # fix, admission's user-def authority DEMOTES the caller to JS (which honors the user
    # def, x+100); on revert the caller is admitted and the WASM path lowers `abs` as the
    # builtin/math alias (|x| / sqrt(x)) != x+100 -> RED. (The EMIT math-alias-vs-user-def
    # reorder for the ELIGIBLE case — where the `direct` WASM arm computes the user value,
    # not sqrt — is covered by the Rust unit test `math_alias_shadowed_by_user_def` in the
    # codegen crate; the eligible form additionally trips a pre-existing JS-GLUE
    # double-declaration collision, a separate bug out of scope for this WASM-lowering fix.)
    math_alias_src = '''
from math import sqrt as abs

def abs(x):
    return x + 100

def use_alias_abs(x: int) -> int:
    return abs(x)
'''
    shadow_math_alias = Module("shadow_math_alias", "shadow_math_alias", math_alias_src, [
        Fn("use_alias_abs", ["int"], "int", calls_of([0, 5, -5]), expect_wasm=False),
    ])

    # Blocker 4 (#491 CROSS-EMITTER): a user `def` shadowing a builtin is WASM-ELIGIBLE
    # (routed to WASM) but its CALLER is DEMOTED to JS. The JS backend must resolve the
    # builtin-named call to the (re-exported) USER function, NOT the runtime builtin
    # (pyAbs/pyLen/pySorted). Here the callers are demoted by a lambda (WASM-refused) and
    # return int (the net's supported return kinds; the coordinator's `str(...)` repro is
    # the same mechanism — the harness's value encoder has no `str`). CPython/JS honor the
    # user def; on revert the JS arm lowers pyAbs/pyLen/pySorted -> e.g. abs(-5)=5 vs user
    # -35 -> RED. This exercises the JS-emitter `is_declared_in_any_scope` wasm-skip
    # authority AND (for sorted) the `infer_wasm_type_with_locals` guard.
    demoted_src = '''
def abs(x: int) -> int:
    return x * 7

def len(x: int) -> int:
    return x + 1000

def use_abs(x: int) -> int:
    f = lambda z: z
    return f(abs(x))

def use_len(x: int) -> int:
    f = lambda z: z
    return f(len(x))
'''
    shadow_demoted = Module("shadow_demoted", "shadow_demoted", demoted_src, [
        Fn("use_abs", ["int"], "int", calls_of([0, 5, -5]), expect_wasm=False),
        Fn("use_len", ["int"], "int", calls_of([0, 5, -5]), expect_wasm=False),
    ])

    # Blocker 4 (#491 WASM def-then-import): `def abs` THEN `from math import fabs as abs`
    # — the LATER import wins (CPython abs == fabs). The def is dead-by-name (NOT routed to
    # WASM / re-exported), use_abs demotes, and JS resolves the last-wins rebind to fabs.
    # On revert the WASM-routed def wins (re-export abs == x*7) -> RED.
    reimport_src = '''
def abs(x: int) -> int:
    return x * 7

from math import fabs as abs

def use_abs(x: float) -> float:
    return abs(x)
'''
    shadow_reimport = Module("shadow_reimport", "shadow_reimport", reimport_src, [
        Fn("use_abs", ["float"], "float", calls_of([0.0, 5.0, -5.0]), expect_wasm=False),
    ])

    # 3rd-inference-path control (#491, replaces the lambda-demoted use_sorted, which was
    # HIR-rejected before inference ran): a user `def sorted`/`def filter` that is
    # WASM-ELIGIBLE (scalar return). `v = sorted(xs)` must be typed by the USER return
    # (int), not the builtin sorted's list-return — else `v + 1` mis-types and the caller
    # is NEWLY demoted. With the `infer_wasm_type_with_locals` guard the callers stay
    # WASM-eligible (expect_wasm=True) and compute the user result; on revert they demote
    # (REFUSED) -> RED.
    infer_src = '''
def sorted(xs: list[int]) -> int:
    return len(xs)

def filter(a: int, xs: list[int]) -> int:
    return len(xs) + a

def use_sorted(xs: list[int]) -> int:
    v = sorted(xs)
    return v + 1

def use_filter(xs: list[int]) -> int:
    v = filter(2, xs)
    return v + 1
'''
    shadow_infer = Module("shadow_infer", "shadow_infer", infer_src, [
        Fn("use_sorted", ["list[int]"], "int", calls_of([[1, 2, 3], [], [9]])),
        Fn("use_filter", ["list[int]"], "int", calls_of([[1, 2, 3], [], [9]])),
    ])

    return [scalar, shadow_range, shadow_abs_list, shadow_math_alias, shadow_demoted,
            shadow_reimport, shadow_infer]


def refusal_family() -> list[Module]:
    """Shapes admission REFUSES (they stay on the JS path): the net confirms the refusal
    and that the JS path matches CPython."""
    src = '''
def comp_len(n: int) -> int:
    return len([i for i in range(n)])

def lam(n: int) -> int:
    f = lambda x: x + 1
    return f(n)
'''
    fns = [
        Fn("comp_len", ["int"], "int", calls_of([0, 3]), expect_wasm=False),
        Fn("lam", ["int"], "int", calls_of([0, 3]), expect_wasm=False),
    ]
    return [Module("refusal", "refusal", src, fns)]


DETERMINISM_MODULES = {
    "mixed_lists": "def fsum(xs: list[float]) -> float:\n    t = 0.0\n    for i in range(len(xs)):\n        t += xs[i]\n    return t\n\ndef isum(xs: list[int]) -> int:\n    t = 0\n    for i in range(len(xs)):\n        t += xs[i]\n    return t\n",
    "single_kind": "def isum(xs: list[int]) -> int:\n    t = 0\n    for i in range(len(xs)):\n        t += xs[i]\n    return t\n\ndef imax(xs: list[int]) -> int:\n    return xs[0]\n",
    "scalar_list_mix": "def fsum(xs: list[float]) -> float:\n    t = 0.0\n    for i in range(len(xs)):\n        t += xs[i]\n    return t\n\ndef sq(a: int) -> int:\n    return a * a\n",
    # The shape on which the class reproduces today (2026-09-03): a module where one
    # function's emitted WASM is INVALID (the try/except-IndexError subscript) — the
    # compile-time fallback then drops a hash-order-dependent SUBSET of the other,
    # valid functions (2 of 3 compiles differed at the base commit). `stmts_shape`
    # (added below) is the stmts family source + TRY_INDEX_SRC — the exact reproducer.
    "invalid_sibling": "def keep_a(a: int) -> int:\n    return a + 1\n\ndef keep_b(a: int) -> bool:\n    return a > 0\n\ndef keep_c(xs: list[int]) -> int:\n    t = 0\n    for x in xs:\n        t += x\n    return t\n\ndef keep_d(a: int) -> int:\n    assert a > 0\n    return a\n\ndef keep_e(a: int) -> int:\n    if a < 0:\n        return -1\n    elif a == 0:\n        return 0\n    return 1\n\ndef keep_f(a: int, b: int) -> int:\n    a += b\n    a //= 3\n    return a\n\ndef try_index(xs: list[int], i: int) -> int:\n    try:\n        v = xs[i]\n    except IndexError:\n        v = -1\n    return v\n",
}


DETERMINISM_MODULES["stmts_shape"] = (
    next(m for m in stmts_family() if m.name == "stmts").source.lstrip("\n") + TRY_INDEX_SRC.lstrip("\n")
)


def corpus(only: str | None) -> list[Module]:
    mods = (binop_family() + unop_family() + listcmp_family() + shortcircuit_family() + writeback_family()
            + writeback_refusal_family() + arity_family() + stmts_family() + shadow_family() + refusal_family())
    if only:
        mods = [m for m in mods if m.family == only or m.name == only]
    return mods


# ----------------------------------------------------------------------------- driving


def run(cmd, cwd=None, env=None, timeout=900):
    return subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, encoding="utf-8",
                          errors="replace", timeout=timeout, check=False)


def rewire(path: Path) -> None:
    """Point the emitted `pyths-runtime[/subpath]` imports at the in-repo runtime source
    (the same rewiring the Livermore lane does), so the JS arms run against the shipped
    runtime copy without an npm install."""
    js = path.read_text(encoding="utf-8")
    js = re.sub(r'from\s+["\']pyths-runtime["\']', f'from "{RUNTIME_INDEX.as_uri()}"', js)

    def sub(m):
        target = (RUNTIME_INDEX.parent / (m.group(1) + ".js")).resolve()
        return f'from "{target.as_uri()}"'

    js = re.sub(r'from\s+["\']pyths-runtime/([\w/.-]+)["\']', sub, js)
    path.write_text(js, encoding="utf-8")


ANSI = re.compile(r"\x1b\[[0-9;]*m")


def compile_module(src: Path, target: str, out_js: Path) -> tuple[bool, str]:
    env = {**os.environ, "PYTHS_NO_CACHE": "1"}
    r = run([str(PYTHS), "compile", str(src), "--target", target, "-o", str(out_js), "--verbose"], env=env)
    log = ANSI.sub("", (r.stdout or "") + (r.stderr or ""))
    if r.returncode != 0 or not out_js.exists():
        return False, log
    rewire(out_js)
    glue = out_js.with_suffix(".glue.js")
    if glue.exists():
        rewire(glue)
    return True, log


def wasm_admitted(log: str) -> set[str]:
    return set(re.findall(r"WASM: (\w+)", log))


def wasm_skipped(log: str) -> dict[str, str]:
    """`Skipped <fn>: <reason>` lines — the admission / validity refusals, per function."""
    return {m.group(1): m.group(2).strip() for m in re.finditer(r"Skipped (\w+): (.*)", log)}


DETERMINISM_REPEATS = 8


def compile_wasm_repeated(src: Path, scratch: Path, name: str) -> tuple[Path | None, str, str | None]:
    """Compile the module `--target js+wasm` DETERMINISM_REPEATS times. The admitted set
    AND the `.wasm` bytes must be identical every time (#474 class: hash-order-dependent
    admission is a valid input that flakes). Returns (best_out_js, its verbose log,
    determinism note): the arms are evaluated on the compile with the LARGEST admitted
    set, so the value rows measure the LOWERING while the flake is reported exactly once
    per module in its own `__determinism` row (RED until the compiler is deterministic).
    A compile that fails outright on every attempt yields best = None."""
    env = {**os.environ, "PYTHS_NO_CACHE": "1"}
    seen = []
    for k in range(DETERMINISM_REPEATS):
        out = scratch / f"{name}.w{k}.js"
        r = run([str(PYTHS), "compile", str(src), "--target", "js+wasm", "-o", str(out), "--verbose"], env=env)
        log = ANSI.sub("", (r.stdout or "") + (r.stderr or ""))
        wasm = out.with_suffix(".wasm")
        ok = r.returncode == 0 and out.exists()
        seen.append((ok, tuple(sorted(wasm_admitted(log))), wasm.read_bytes() if wasm.exists() else None, out, log))
    note = None
    sets = {(ok, adm) for ok, adm, _, _, _ in seen}
    if len(sets) != 1:
        note = f"NONDETERMINISTIC: admitted set differs across {DETERMINISM_REPEATS} compiles of the same input: {sorted(sets)}"
    elif len({b for _, _, b, _, _ in seen}) != 1:
        note = f"NONDETERMINISTIC: .wasm bytes differ across {DETERMINISM_REPEATS} compiles of the same input"
    good = [s for s in seen if s[0]]
    if not good:
        return None, seen[0][4], note
    best = max(good, key=lambda s: len(s[1]))
    rewire(best[3])
    glue = best[3].with_suffix(".glue.js")
    if glue.exists():
        rewire(glue)
    return best[3], best[4], note


def arm_json(cmd, cwd) -> dict | str:
    r = run(cmd, cwd=cwd)
    if r.returncode != 0:
        return f"arm-failed: {r.stderr.strip()[:300]}"
    try:
        return json.loads(r.stdout)
    except ValueError:
        return f"arm-nonjson: {r.stdout[:200]!r}"


def same(a: dict, b: dict) -> tuple[bool, str]:
    """Compare a CPython outcome `a` with an arm outcome `b`. Returns (match, note)."""
    if a["kind"] == "ok" and b["kind"] == "ok":
        if a["value"] != b["value"]:
            return False, f"value {a['value']} != {b['value']}"
        if a.get("lists", {}) != b.get("lists", {}):
            return False, f"lists-after {a.get('lists')} != {b.get('lists')}"
        return True, ""
    if a["kind"] == "err" and b["kind"] == "err":
        return (a["exc"] == b["exc"]), ("" if a["exc"] == b["exc"] else f"kind {a['exc']} != {b['exc']}")
    if a["kind"] == "err" and b["kind"] == "trap":
        return True, "kind-gap"
    return False, f"{a['kind']}({a.get('value', a.get('exc', ''))}) != {b['kind']}({b.get('value', b.get('exc', ''))})"


@dataclass
class Row:
    id: str
    red: bool
    arms: dict
    notes: list[str]
    # Per-arm structured state for the ratchet: {arm: {"status", "ovf", "gaps"}}.
    # `status` is one of ok / RED / REFUSED / refused(expected) / refused(acceptable) /
    # compile-fail / arm-failed / missing (or the determinism tag). The ratchet compares
    # EVERY arm of EVERY row, not the row id (opus r1/B3: a row already RED on the glue
    # arm must not hide a NEW direct-arm failure), and pins the ovf / kind-gap counts
    # (opus r1/SF5: an `ovf` skip or a trap-for-exception degrade must not grow silently).
    state: dict = field(default_factory=dict)


def evaluate(mod: Module, py: dict, js: dict | str, glue: dict | str, direct: dict | str,
             skipped: dict[str, str], compile_fail: dict[str, str], det_note: str | None) -> list[Row]:
    rows = []
    if det_note is not None:
        rows.append(Row(f"{mod.name}::__determinism", True, {"compile": f"x{DETERMINISM_REPEATS}"}, [det_note],
                        {"compile": {"status": "RED", "ovf": 0, "gaps": 0}}))
    else:
        rows.append(Row(f"{mod.name}::__determinism", False, {"compile": f"x{DETERMINISM_REPEATS} identical"}, [],
                        {"compile": {"status": "ok", "ovf": 0, "gaps": 0}}))
    for fn in mod.fns:
        rid = f"{mod.name}::{fn.name}"
        arms: dict[str, str] = {}
        state: dict[str, dict] = {}
        notes: list[str] = []
        red = False
        for arm, res in (("js", js), ("glue", glue), ("direct", direct)):
            if arm in compile_fail:
                arms[arm] = "compile-fail"
                state[arm] = {"status": "compile-fail", "ovf": 0, "gaps": 0}
                notes.append(f"{arm}: {compile_fail[arm]}")
                red = True
                continue
            if isinstance(res, str):
                arms[arm] = "arm-failed"
                state[arm] = {"status": "arm-failed", "ovf": 0, "gaps": 0}
                notes.append(f"{arm}: {res}")
                red = True
                continue
            outs = res.get(fn.name)
            if outs is None:
                arms[arm] = "missing"
                state[arm] = {"status": "missing", "ovf": 0, "gaps": 0}
                notes.append(f"{arm}: function missing from arm output")
                red = True
                continue
            if arm == "direct" and all(o["kind"] == "not-exported" for o in outs):
                reason = skipped.get(fn.name, "no `Skipped` line in the verbose log")
                if fn.expect_wasm is True:
                    arms[arm] = "REFUSED"
                    notes.append(f"direct: admission refused a shape the net expects admitted — {reason}")
                    red = True
                else:
                    arms[arm] = "refused(expected)" if fn.expect_wasm is False else "refused(acceptable)"
                    notes.append(f"direct: refused — {reason}")
                state[arm] = {"status": arms[arm], "ovf": 0, "gaps": 0}
                continue
            if arm == "direct" and fn.expect_wasm is False:
                notes.append("direct: admission WIDENED — a shape expected refused is now compiled (re-check its rows)")
            mism = 0
            gaps = 0
            ovf = 0
            first = []
            for i, (p, o) in enumerate(zip(py[fn.name], outs)):
                if o is None:
                    o = {"kind": "missing", "exc": "no outcome recorded"}
                if arm == "direct" and o["kind"] == "ovf":
                    ovf += 1
                    continue  # by contract the twin answers; the glue arm checks it
                ok, note = same(p, o)
                if note == "kind-gap":
                    gaps += 1
                if not ok:
                    mism += 1
                    if len(first) < 3:
                        first.append(f"call#{i} {fn.calls[i]}: {note}")
            if len(py[fn.name]) != len(outs):
                mism += 1
                first.append(f"call-count {len(py[fn.name])} != {len(outs)}")
            tag = "ok" if mism == 0 else f"RED {mism}/{len(outs)}"
            if gaps:
                tag += f" (kind-gap {gaps})"
            if ovf:
                tag += f" (ovf->twin {ovf})"
            arms[arm] = tag
            state[arm] = {"status": "ok" if mism == 0 else "RED", "ovf": ovf, "gaps": gaps, "mism": mism}
            if mism:
                red = True
                notes.extend(f"{arm}: {f}" for f in first)
        rows.append(Row(rid, red, arms, notes, state))
    return rows


def run_module(mod: Module, scratch: Path) -> list[Row]:
    src_dir = scratch / "src"
    src_dir.mkdir(parents=True, exist_ok=True)
    src = src_dir / f"{mod.name}.ps"
    src.write_text(mod.source.lstrip("\n"), encoding="utf-8")
    spec = scratch / f"{mod.name}.spec.json"
    spec.write_text(json.dumps({"functions": {f.name: {"params": f.params, "ret": f.ret, "calls": f.calls, "alias": f.alias} for f in mod.fns}}), encoding="utf-8")

    py = arm_json([*ORACLE, str(HERE / "cpython_arm.py"), str(src), str(spec)], cwd=str(ROOT))
    if isinstance(py, str):
        return [Row(f"{mod.name}::{f.name}", True, {"cpython": "arm-failed"}, [py]) for f in mod.fns]

    compile_fail: dict[str, str] = {}
    js_out = scratch / f"{mod.name}.js.js"
    ok, log = compile_module(src, "js", js_out)
    if not ok:
        compile_fail["js"] = log.strip().splitlines()[-1] if log.strip() else "compile failed"
    js = arm_json(["node", str(HERE / "run_module.mjs"), str(js_out), str(spec)], cwd=str(scratch)) if ok else "compile-fail"

    w_out, log, det_note = compile_wasm_repeated(src, scratch, mod.name)
    skipped = wasm_skipped(log)
    if w_out is None:
        tail = [ln for ln in log.strip().splitlines() if ln.strip()]
        compile_fail["glue"] = compile_fail["direct"] = (tail[0] + " | " + tail[-1]) if tail else "compile failed"
        glue = direct = "compile-fail"
    else:
        glue = arm_json(["node", str(HERE / "run_module.mjs"), str(w_out), str(spec)], cwd=str(scratch))
        wasm = w_out.with_suffix(".wasm")
        if wasm.exists():
            direct = arm_json(["node", str(HERE / "run_wasm_direct.mjs"), str(wasm), str(spec)], cwd=str(scratch))
        else:
            direct = {f.name: [{"kind": "not-exported"} for _ in f.calls] for f in mod.fns}
    return evaluate(mod, py, js, glue, direct, skipped, compile_fail, det_note)


def determinism_rows(scratch: Path) -> list[Row]:
    """#474: admission must be input-order-determined — N compiles of the same module
    must admit the same function set every time."""
    rows = []
    src_dir = scratch / "src"
    src_dir.mkdir(parents=True, exist_ok=True)
    env = {**os.environ, "PYTHS_NO_CACHE": "1"}
    for name, source in DETERMINISM_MODULES.items():
        src = src_dir / f"det_{name}.ps"
        src.write_text(source, encoding="utf-8")
        expected = set(re.findall(r"^def (\w+)\(", source, re.M))
        n = DETERMINISM_N if name in ("mixed_lists", "stmts_shape") else 12
        results = []
        for k in range(n):
            out = scratch / f"det_{name}_{k}.js"
            r = run([str(PYTHS), "compile", str(src), "--target", "js+wasm", "-o", str(out), "--verbose"], env=env)
            log = ANSI.sub("", (r.stdout or "") + (r.stderr or ""))
            wasm = out.with_suffix(".wasm")
            results.append((r.returncode == 0, wasm_admitted(log), log, wasm.read_bytes() if wasm.exists() else None))
        # Two criteria: (1) every compile admits the SAME set and emits the SAME bytes
        # (determinism proper); (2) for the fully-valid modules that set is the full
        # function set (nothing valid is dropped).
        sets = {(ok, tuple(sorted(adm))) for ok, adm, _, _ in results}
        bytes_ = {b for _, _, _, b in results}
        full = sum(1 for ok, adm, _, _ in results if ok and adm == expected)
        notes = []
        if len(sets) != 1 or len(bytes_) != 1:
            notes.append(f"NONDETERMINISTIC: {len(sets)} distinct admitted sets / {len(bytes_)} distinct .wasm outputs over {n} compiles: {sorted(sets)}")
        if name not in ("invalid_sibling", "stmts_shape") and full != n:
            bad = [lg.strip().splitlines()[-1] for ok, adm, lg, _ in results if not (ok and adm == expected)]
            notes.append(f"full admission {full}/{n}; e.g. {bad[0][:200]}")
        rows.append(Row(f"determinism::{name}_{n}x", bool(notes), {"admit": f"{full}/{n}", "distinct": f"{len(sets)} sets/{len(bytes_)} wasm"}, notes,
                        {"admit": {"status": "RED" if notes else "ok", "ovf": 0, "gaps": 0}}))
    return rows


# ----------------------------------------------------------------------------- ratchet


def load_baseline() -> dict:
    if BASELINE.exists():
        return json.loads(BASELINE.read_text(encoding="utf-8"))
    return {"rows": 0, "calls": 0, "state": {}}


WORSE = ("RED", "REFUSED", "compile-fail", "arm-failed", "missing")


def ratchet(rows: list[Row], n_calls: int, base: dict) -> list[str]:
    """Two-sided, PER ARM (opus r1/B3+SF5): every arm of every row is compared with
    the pinned state; a status that went ok -> bad, or an ovf/kind-gap count that GREW,
    is a regression; a status that went bad -> ok, or a count that SHRANK, is stale
    (the baseline only improves by a deliberate --update-baseline); the row AND call
    counts are pinned so the corpus cannot shrink (or grow) unnoticed."""
    problems = []
    now = {r.id: r.state for r in rows}
    old = base.get("state", {})
    regress, stale = [], []
    for rid, st in now.items():
        prev = old.get(rid, {}).get("arms")
        if prev is None:
            regress.append(f"{rid} (new row — pin it deliberately)")
            continue
        for arm, cur in st.items():
            p = prev.get(arm)
            if p is None:
                regress.append(f"{rid}[{arm}] (new arm)")
                continue
            if cur["status"] != p["status"]:
                if cur["status"] in WORSE and p["status"] not in WORSE:
                    regress.append(f"{rid}[{arm}] {p['status']} -> {cur['status']}")
                elif p["status"] in WORSE and cur["status"] not in WORSE:
                    stale.append(f"{rid}[{arm}] {p['status']} -> {cur['status']}")
                else:
                    regress.append(f"{rid}[{arm}] {p['status']} -> {cur['status']}")
            # ovf / kind-gap / MISMATCH counts are pinned (opus r2/NEW-6: a known-RED arm
            # must not absorb new failures — `RED 3/40` growing to `RED 40/40` is a regression).
            for k in ("ovf", "gaps", "mism"):
                if cur.get(k, 0) > p.get(k, 0):
                    regress.append(f"{rid}[{arm}] {k} {p.get(k, 0)} -> {cur.get(k, 0)}")
                elif cur.get(k, 0) < p.get(k, 0):
                    stale.append(f"{rid}[{arm}] {k} {p.get(k, 0)} -> {cur.get(k, 0)}")
    missing = sorted(i for i in old if i not in now)
    if regress:
        problems.append(f"REGRESSION — {len(regress)} arm(s) worse than the baseline: {regress[:40]}")
    if stale:
        problems.append(f"STALE — {len(stale)} arm(s) better than the baseline (re-pin deliberately; the baseline only improves): {stale[:40]}")
    if missing:
        problems.append(f"SHRINK — baseline rows missing from the corpus: {missing}")
    if len(rows) != base.get("rows"):
        problems.append(f"ROW COUNT — corpus has {len(rows)} rows, baseline pins {base.get('rows')} (run --update-baseline deliberately)")
    if n_calls != base.get("calls"):
        problems.append(f"CALL COUNT — corpus has {n_calls} calls, baseline pins {base.get('calls')} (the tested surface changed)")
    return problems


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--only", help="run one family or module (dev aid; the ratchet is NOT evaluated)")
    ap.add_argument("--update-baseline", action="store_true", help="rewrite baseline.json from this run (deliberate, never in CI)")
    ap.add_argument("--report", help="write the full JSON report here")
    args = ap.parse_args()

    if not Path(PYTHS).exists():
        print(f"[wasm_net] pyths binary not found: {PYTHS}", file=sys.stderr)
        return 2
    if not shutil.which("node"):
        print("[wasm_net] node not on PATH", file=sys.stderr)
        return 2
    probe = run([*ORACLE, "--version"])
    if probe.returncode != 0:
        print(f"[wasm_net] oracle not runnable: {' '.join(ORACLE)}", file=sys.stderr)
        return 2
    ver = run([str(PYTHS), "--version"]).stdout.strip()
    sha = run(["git", "rev-parse", "--short", "HEAD"], cwd=str(ROOT)).stdout.strip()
    print(f"[wasm_net] pyths={ver} ({PYTHS}) git={sha} oracle={' '.join(ORACLE)} -> {probe.stdout.strip()}")

    if SCRATCH.exists():
        shutil.rmtree(SCRATCH)
    SCRATCH.mkdir(parents=True)

    rows: list[Row] = []
    mods = corpus(args.only)
    for mod in mods:
        rows.extend(run_module(mod, SCRATCH))
    if not args.only or args.only == "determinism":
        rows.extend(determinism_rows(SCRATCH))

    n_calls = sum(len(f.calls) for m in mods for f in m.fns)
    red = [r for r in rows if r.red]
    print(f"[wasm_net] {len(rows)} rows ({len(mods)} modules, {n_calls} calls x 3 arms): {len(rows) - len(red)} green / {len(red)} RED")
    for r in rows:
        mark = "RED  " if r.red else "ok   "
        print(f"  {mark}{r.id:40s} {r.arms}")
        for n in r.notes:
            print(f"        - {n}")

    if args.report:
        Path(args.report).write_text(json.dumps({"pyths": ver, "git": sha, "rows": [r.__dict__ for r in rows]}, indent=1), encoding="utf-8")

    if args.only:
        print("[wasm_net] --only: ratchet not evaluated")
        return 1 if red else 0

    base = load_baseline()
    if args.update_baseline:
        old_state = base.get("state", {})
        state = {}
        for r in rows:
            entry = {"arms": r.state}
            if r.red:
                entry["note"] = old_state.get(r.id, {}).get("note", "NEW: unclassified — annotate with the issue")
            state[r.id] = entry
        out = {"_note": base.get("_note", ""), "rows": len(rows), "calls": n_calls, "state": state}
        BASELINE.write_text(json.dumps(out, indent=1) + "\n", encoding="utf-8")
        print(f"[wasm_net] baseline updated: rows={len(rows)} calls={n_calls} red={len(red)}")
        return 0

    problems = ratchet(rows, n_calls, base)
    for p in problems:
        print(f"[wasm_net] FAIL: {p}")
    if not problems:
        print(f"[wasm_net] ratchet OK: {len(red)} known-RED rows (baseline), every arm / ovf / kind-gap count as pinned")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
