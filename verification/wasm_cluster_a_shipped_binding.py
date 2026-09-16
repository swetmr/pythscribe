#!/usr/bin/env python3
"""Shipped-binding differential for the Lean truthiness / arith models on the WASM path
(v0.2.5 Cluster A, #485 / #487).

The Lean models `pyBoolM` (PythExpandVerify.lean, the `#guard` pins) and `AOp` /
`preservationC1C3C4` (C1C3C4Outcome.lean) are `binds`-edges to the JS runtime
(`types.js::pyBool`, `operators.js`). The WASM backend now lowers truthiness through ONE
authority (`emit.rs::emit_truthiness`) and `and`/`or`/comparisons through the same value
contract — so the model literals must ALSO equal what the real `pyths --target js+wasm`
`.wasm` computes, run on the RAW export (no JS twin), with CPython as the third leg:

    Lean literal  ==  real pyths WASM (direct)  ==  CPython           (three-legged bind)

This is a differential against the SHIPPED artifact — never model == model.

Rows:
  * every `#guard pyBoolM (<value>) = <bool>` pin whose value is WASM-representable
    (`.list …` -> a `list[int]` param, `.num n` -> int, `.bool b` -> bool) is run through
    `1 if x else 0` / `not x` (every truthiness lowering site); pins on `.str` / `.dict`
    / `.set` / `.lenObj` / `.none` are reported as OUTSIDE the WASM domain (admission
    refuses those types, #364) — printed, never silently dropped;
  * the `pyBoolM` DEFINITION clauses for num / bool / list are READ FROM THE LEAN SOURCE
    (`| .num n => n ≠ 0`, `| .list xs => 0 < xs.length`, `| .bool b => b`) and evaluated
    on extra literals (nonzero ints, 2^32, a non-empty all-zero list, NaN, -0.0) — the
    expected value comes from the parsed clause, so editing the Lean definition changes
    the expectation (a `n > 0` edit turns the `-3` row RED);
  * `AOp` (`-`, `//`) rows are a CPYTHON DIFFERENTIAL of the shipped WASM lowering on the
    theorem's operand domain (int / float pairs): they are NOT a Lean-literal bind — the
    Lean reference side `refArith` is CPython's semantics by construction (the theorem
    proves target == reference; this script checks shipped-WASM == CPython on the same
    inputs) — and are labelled as such in the output; the C4 error-KIND row (`7 // 0`)
    asserts CPython and the GLUE arm (the twin) raise ZeroDivisionError AND that the raw
    export TRAPS (the boundary is a gate, not a remark: if the raw export ever returns a
    value here the class has changed and this goes RED).

Harness-integrity: the pin COUNT is pinned (EXPECTED_PINS: a regex drift that drops a
`#guard` line fails loud), and rows loaded == rows with BOTH outcomes present == rows compared.

Run from the repo root (needs target/{release,debug}/pyths[.exe], node, the oracle):
    python verification/wasm_cluster_a_shipped_binding.py
Env: PYTHS_BIN, PYTHS_ORACLE_PYTHON (e.g. "py -3.14").
"""
from __future__ import annotations

import json
import os
import re
import shutil
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NET = ROOT / "tests" / "differential" / "wasm_net"
sys.path.insert(0, str(NET))
import wasm_net  # noqa: E402

LEAN = ROOT / "verification" / "PythExpandVerify.lean"
GUARD = re.compile(r"^#guard pyBoolM \((.+?)\) = (true|false)", re.M)
# The `#guard pyBoolM` pins in PythExpandVerify.lean at this commit (a pin added or
# dropped must be acknowledged here, never absorbed silently).
EXPECTED_PINS = 6
CLAUSE = re.compile(r"^def pyBoolM : PyValue → Bool\n((?:  \| .*\n)+)", re.M)


def lean_clauses(text: str) -> dict[str, tuple[str, str]]:
    """The `pyBoolM` definition clauses, keyed by constructor: {".num": ("n", "n ≠ 0"), ...}."""
    m = CLAUSE.search(text)
    assert m, "pyBoolM definition not found in the Lean source"
    out = {}
    for line in m.group(1).splitlines():
        cm = re.match(r"\s*\| \.(\w+)(?: (\w+))? => (.+)$", line)
        assert cm, line
        out["." + cm.group(1)] = (cm.group(2) or "", cm.group(3).strip())
    return out


def eval_clause(clause: tuple[str, str], value) -> bool:
    """Evaluate a Lean truthiness clause on a modeled value with a tiny translator
    (`≠` -> `!=`, `xs.length` -> `len(xs)`, `decide (..)` -> `(..)`, `true/false`)."""
    var, body = clause
    # Token-level translation (word boundaries: a clause body containing e.g. `decidable`
    # or `construe` is not corrupted); an untranslatable clause fails as a clean RED.
    expr = re.sub(r"≠", "!=", body)
    expr = re.sub(r"\b(\w+)\.length\b", r"len(\1)", expr)
    expr = re.sub(r"\bdecide\b", "", expr)
    expr = re.sub(r"\btrue\b", "True", expr)
    expr = re.sub(r"\bfalse\b", "False", expr)
    env = {var: value} if var else {}
    env["len"] = len
    # eval is deliberate here: the expression is the pyBoolM clause text read from the
    # repository-controlled Lean source (never external input), evaluated with no builtins.
    try:
        return bool(eval(expr, {"__builtins__": {}}, env))  # noqa: S307
    except Exception as e:  # noqa: BLE001
        raise SystemExit(f"[wasm_cluster_a_binding] FAIL: cannot evaluate the Lean clause {body!r}: {e}") from e

PROBE = '''
def t_list(xs: list[int]) -> int:
    return 1 if xs else 0

def n_list(xs: list[int]) -> bool:
    return not xs

def t_num(a: int) -> int:
    return 1 if a else 0

def n_num(a: int) -> bool:
    return not a

def t_float(a: float) -> int:
    return 1 if a else 0

def t_bool(a: bool) -> int:
    return 1 if a else 0

def a_sub_ii(a: int, b: int) -> int:
    return a - b

def a_sub_ff(a: float, b: float) -> float:
    return a - b

def a_sub_if(a: int, b: float) -> float:
    return a - b

def a_fdiv_ii(a: int, b: int) -> int:
    return a // b

def a_fdiv_ff(a: float, b: float) -> float:
    return a // b

def a_fdiv_fi(a: float, b: int) -> float:
    return a // b
'''


def lean_value_to_input(v: str):
    """Map a modeled `PyValue` literal to (probe fn, param type, JSON arg) or None when
    the value is outside the WASM domain."""
    v = v.strip()
    if v.startswith(".list"):
        body = v[len(".list"):].strip()
        assert body.startswith("[") and body.endswith("]"), v
        inner = body[1:-1].strip()
        elems = [] if not inner else [int(re.search(r"-?\d+", e).group()) for e in inner.split(",")]
        return "t_list", "n_list", "list[int]", [str(e) for e in elems]
    if v.startswith(".num"):
        return "t_num", "n_num", "int", str(int(v[len(".num"):].strip().strip("()")))
    if v.startswith(".bool"):
        return "t_bool", None, "bool", v.endswith("true")
    return None


def main() -> int:
    if not Path(wasm_net.PYTHS).exists() or not shutil.which("node"):
        print("[wasm_cluster_a_binding] pyths binary or node missing", file=sys.stderr)
        return 2
    text = LEAN.read_text(encoding="utf-8")
    pins = GUARD.findall(text)
    assert len(pins) == EXPECTED_PINS, f"{len(pins)} `#guard pyBoolM` pins found, {EXPECTED_PINS} pinned — acknowledge the change"
    clauses = lean_clauses(text)
    for k in (".num", ".list", ".bool"):
        assert k in clauses, f"pyBoolM clause {k} missing: {clauses}"

    rows = []  # (id, fn, params, ret, args, expected-normalized-value or None, note)
    outside = []
    for val, lit in pins:
        m = lean_value_to_input(val)
        if m is None:
            outside.append((val, lit))
            continue
        tfn, nfn, pty, arg = m
        want_t = "1" if lit == "true" else "0"
        rows.append((f"pin:{val}:truth", tfn, [pty], "int", [arg], want_t))
        if nfn:
            rows.append((f"pin:{val}:not", nfn, [pty], "bool", [arg], "False" if lit == "true" else "True"))
    # Definition clauses on extra literals: the EXPECTED value is computed from the
    # parsed Lean clause (`.num n => n ≠ 0` etc.), not written by hand here. Lean's
    # `.num` carries an Int; the float rows apply its `n ≠ 0` rule to a Python float
    # (the model's documented "numeric zero, int/float alike" reading; NaN is non-zero).
    def lit(b: bool) -> str:
        return "1" if b else "0"

    num, lst, bl = clauses[".num"], clauses[".list"], clauses[".bool"]
    extra = [
        ("def:num 5", "t_num", ["int"], "int", ["5"], lit(eval_clause(num, 5))),
        ("def:num -3", "t_num", ["int"], "int", ["-3"], lit(eval_clause(num, -3))),
        ("def:num 2^40", "t_num", ["int"], "int", [str(2**40)], lit(eval_clause(num, 2**40))),
        ("def:num 2^32 (would wrap to 0)", "t_num", ["int"], "int", [str(2**32)], lit(eval_clause(num, 2**32))),
        ("def:list [0,0] (non-empty all-zero)", "t_list", ["list[int]"], "int", [["0", "0"]], lit(eval_clause(lst, [0, 0]))),
        ("def:list [7]", "t_list", ["list[int]"], "int", [["7"]], lit(eval_clause(lst, [7]))),
        ("def:float 0.0", "t_float", ["float"], "int", [0.0], lit(eval_clause(num, 0.0))),
        ("def:float -0.0", "t_float", ["float"], "int", ["-0.0"], lit(eval_clause(num, -0.0))),
        ("def:float nan (truthy)", "t_float", ["float"], "int", ["nan"], lit(eval_clause(num, float("nan")))),
        ("def:float 2.5", "t_float", ["float"], "int", [2.5], lit(eval_clause(num, 2.5))),
        ("def:bool false", "t_bool", ["bool"], "int", [False], lit(eval_clause(bl, False))),
    ]
    rows.extend(extra)
    # AOp C1 value rows: the model's reference side is CPython; the WASM lowering must
    # agree with CPython (checked below via the CPython arm, no literal here).
    aop = []
    for a, b in [(7, 3), (-7, 3), (7, -3), (0, 5), (2**40, 7), (-(2**40), 7), (5, 2), (-5, 2)]:
        aop.append((f"AOp:osub {a} {b}", "a_sub_ii", ["int", "int"], "int", [str(a), str(b)], None))
        aop.append((f"AOp:ofdiv {a} {b}", "a_fdiv_ii", ["int", "int"], "int", [str(a), str(b)], None))
    for a, b in [(7.5, 2.0), (-7.5, 2.0), (7.5, -2.0), (0.5, 0.25), (1e300, 3.0)]:
        aop.append((f"AOp:osub {a} {b}", "a_sub_ff", ["float", "float"], "float", [a, b], None))
        aop.append((f"AOp:ofdiv {a} {b}", "a_fdiv_ff", ["float", "float"], "float", [a, b], None))
    aop.append(("AOp:osub 7 2.5", "a_sub_if", ["int", "float"], "float", ["7", 2.5], None))
    aop.append(("AOp:ofdiv 7.5 2", "a_fdiv_fi", ["float", "int"], "float", [7.5, "2"], None))
    rows.extend(aop)
    loaded = len(rows)

    scratch = Path(tempfile.mkdtemp(prefix="wasm_cluster_a_binding_"))
    src = scratch / "probe.ps"
    src.write_text(PROBE.lstrip("\n"), encoding="utf-8")
    out = scratch / "probe.js"
    r = wasm_net.run([str(wasm_net.PYTHS), "compile", str(src), "--target", "js+wasm", "-o", str(out), "--verbose"],
                     env={**os.environ, "PYTHS_NO_CACHE": "1"})
    log = wasm_net.ANSI.sub("", (r.stdout or "") + (r.stderr or ""))
    if r.returncode != 0 or not out.with_suffix(".wasm").exists():
        print(log)
        return 1
    admitted = wasm_net.wasm_admitted(log)
    need = {fn for _, fn, *_ in rows}
    missing = sorted(need - admitted)
    assert not missing, f"probe functions not admitted to WASM: {missing}\n{log}"
    wasm_net.rewire(out)
    glue = out.with_suffix(".glue.js")
    if glue.exists():
        wasm_net.rewire(glue)

    # Group rows per function into one spec.
    spec = {"functions": {}}
    index = {}
    for rid, fn, params, ret, args, want in rows:
        f = spec["functions"].setdefault(fn, {"params": params, "ret": ret, "calls": []})
        index[rid] = (fn, len(f["calls"]))
        f["calls"].append(args)
    sp = scratch / "probe.spec.json"
    sp.write_text(json.dumps(spec), encoding="utf-8")
    direct = wasm_net.arm_json(["node", str(NET / "run_wasm_direct.mjs"), str(out.with_suffix(".wasm")), str(sp)], cwd=str(scratch))
    py = wasm_net.arm_json([*wasm_net.ORACLE, str(NET / "cpython_arm.py"), str(src), str(sp)], cwd=str(ROOT))
    assert isinstance(direct, dict), direct
    assert isinstance(py, dict), py
    # Integrity (opus r3/NEW-r3-4, r4): every arm returned EXACTLY one outcome per call of
    # every probe function — a runner that truncates or drops a call fails here, loudly.
    for fname, f in spec["functions"].items():
        n = len(f["calls"])
        assert len(direct.get(fname, [])) == n, f"direct arm returned {len(direct.get(fname, []))} outcomes for {fname}, {n} calls"
        assert len(py.get(fname, [])) == n, f"cpython arm returned {len(py.get(fname, []))} outcomes for {fname}, {n} calls"

    executed = 0
    missing_outcomes = 0
    failures = []
    for rid, fn, params, ret, args, want in rows:
        f, i = index[rid]
        o = direct[f][i]
        p = py[f][i]
        if o is None or p is None:
            failures.append(f"{rid}: an arm produced no outcome (direct={o}, cpython={p})")
            missing_outcomes += 1
            continue
        executed += 1
        ok, note = wasm_net.same(p, o)
        if not ok:
            failures.append(f"{rid}: CPython {p} != WASM-direct {o} ({note})")
            continue
        if want is not None and (o["kind"] != "ok" or o["value"] != want):
            failures.append(f"{rid}: Lean literal {want} != WASM-direct {o}")
        elif want is not None and p["value"] != want:
            failures.append(f"{rid}: Lean literal {want} != CPython {p}")

    # C4 error-KIND boundary: `7 // 0` — CPython ZeroDivisionError; the raw export
    # TRAPS (no error model on the WASM path); the glue's twin re-run yields the kind.
    c4_spec = {"functions": {"a_fdiv_ii": {"params": ["int", "int"], "ret": "int", "calls": [["7", "0"]]}}}
    c4p = scratch / "c4.spec.json"
    c4p.write_text(json.dumps(c4_spec), encoding="utf-8")
    d4 = wasm_net.arm_json(["node", str(NET / "run_wasm_direct.mjs"), str(out.with_suffix(".wasm")), str(c4p)], cwd=str(scratch))
    g4 = wasm_net.arm_json(["node", str(NET / "run_module.mjs"), str(out), str(c4p)], cwd=str(scratch))
    p4 = wasm_net.arm_json([*wasm_net.ORACLE, str(NET / "cpython_arm.py"), str(src), str(c4p)], cwd=str(ROOT))
    c4_direct = d4["a_fdiv_ii"][0]
    c4_glue = g4["a_fdiv_ii"][0]
    c4_py = p4["a_fdiv_ii"][0]
    if not (c4_py["kind"] == "err" and c4_py["exc"] == "ZeroDivisionError"):
        failures.append(f"C4 oracle: expected ZeroDivisionError, got {c4_py}")
    if not (c4_glue["kind"] == "err" and c4_glue["exc"] == "ZeroDivisionError"):
        failures.append(f"C4 glue arm: expected ZeroDivisionError (via the JS twin), got {c4_glue}")
    if c4_direct["kind"] != "trap":
        failures.append(f"C4 raw export: the documented boundary is a TRAP (no error model on the raw WASM path); "
                        f"got {c4_direct} — the class changed, re-state the boundary")

    # (Row accounting: a null outcome is a recorded failure; the real integrity gate is
    # the per-function outcome COUNT check above, which a truncating runner fails.)
    if missing_outcomes:
        failures.append(f"integrity: {missing_outcomes} row(s) produced no outcome on an arm")
    print(f"[wasm_cluster_a_binding] pyths={wasm_net.PYTHS} oracle={' '.join(wasm_net.ORACLE)}")
    print(f"[wasm_cluster_a_binding] pyBoolM pins found: {len(pins)}; bound on the WASM path: {len(pins) - len(outside)}; "
          f"outside the WASM domain (admission refuses the type): {[v for v, _ in outside]}")
    print(f"[wasm_cluster_a_binding] rows loaded/executed(both outcomes, compared): {loaded}/{executed} "
          f"(pyBoolM: {2 * (len(pins) - len(outside)) - 1} pin rows + {len(extra)} Lean-clause rows = Lean literal == WASM == CPython; "
          f"AOp: {len(aop)} rows = CPython DIFFERENTIAL of the shipped WASM lowering on the theorem's operand domain, not a Lean-literal bind)")
    print(f"[wasm_cluster_a_binding] C4 kind boundary: CPython={c4_py['exc']}; raw export={c4_direct['kind']}"
          f"{'(' + c4_direct.get('exc', '') + ')' if c4_direct['kind'] != 'ok' else ''} — kind NOT carried on the raw "
          f"WASM path (documented boundary, NEW-W5 class); glue arm={c4_glue.get('exc')} (bound through the twin)")
    for f in failures:
        print(f"[wasm_cluster_a_binding] FAIL: {f}")
    shutil.rmtree(scratch, ignore_errors=True)
    if failures:
        return 1
    print("[wasm_cluster_a_binding] OK: Lean literal == WASM-direct == CPython on every pyBoolM row; WASM-direct == CPython on every AOp row; C4 boundary as stated")
    return 0


if __name__ == "__main__":
    sys.exit(main())
