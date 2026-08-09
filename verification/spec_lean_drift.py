#!/usr/bin/env python3
"""Stage-3 DRIFT GATE (lean-spec-quality, finding C10) — bind the Python
transliterations in `spec_validate_strmethods.py` / `spec_validate_dictmethods.py`
to the ACTUAL Lean model in `verification/PythExpandVerify.lean`.

The two `spec_validate_*.py` scripts are faithful Python transliterations of the
Lean model, differentially checked against CPython. On their own, a SYNCHRONIZED
mistake (the same bug typed into both the Lean model and its Python twin) passes
silently, and a later edit to the Lean model drifts away from the Python twin
with nothing to catch it. This gate closes that gap: it generates a Lean harness
that EVALUATES the real Lean model functions (`d2Lookup`, `smFindSub`, …) on a
shared corpus via `#eval`, runs it with `lake env lean`, and diffs the output
against the SAME corpus run through the Python transliterations. Any disagreement
between the Python model and the Lean model fails the gate.

This is the "execute the Lean defs" binding the skill calls for — it does not
replace the CPython differential (spec_validate_*), it binds the transliteration
to its Lean source so the differential's verdict is about the real model.

HONEST LIMIT: this gate catches DRIFT (Lean and Python disagreeing). It does NOT
catch a SYNCHRONIZED bug — the same wrong behavior typed into BOTH the Lean model
and its Python twin agrees here and passes. Only the separate CPython differential
(`spec_validate_*.py`, which compares the Python twin to real CPython) catches
that. The two gates are complementary: this one pins twin==model, that one pins
twin==CPython; together they pin model==CPython transitively.

This script forces a fresh `lake build` before `lake env lean`, so it can never
compare the Python twin against a STALE `.olean` (which would hide a Lean edit).

Usage: python verification/spec_lean_drift.py
Requires: the `verification` lake project (run from the repo; it builds first,
then uses `lake env lean`).
"""
from __future__ import annotations

import os
import subprocess
import sys
import tempfile

from spec_validate_dictmethods import (
    dict_keys, dict_len, dict_lookup, dict_mem, js_obj_lookup,
)
from spec_validate_strmethods import (
    js16_find_sub, sm_count, sm_find_sub, sm_replace, sm_starts_with,
)

HERE = os.path.dirname(os.path.abspath(__file__))


# --- shared corpus (neutral form; rendered to BOTH Lean and Python) ----------

def K(tag, val):
    return (tag, val)


# Dict entry lists (keys as ("int",n)|("str",s); values ints), incl. the
# int/str collision pairs and duplicate keys.
DICT_CORPUS = [
    [],
    [(K("int", 1), 10)],
    [(K("int", 1), 10), (K("str", "1"), 20)],           # collision pair
    [(K("int", 1), 1), (K("int", 2), 2), (K("int", 1), 3)],  # dup key
    [(K("str", "a"), 1), (K("str", ""), 2), (K("str", "a"), 3)],
    [(K("int", -1), 5), (K("str", "-1"), 6)],
    [(K("int", 10), 1), (K("str", "10"), 2), (K("int", 10), 3)],
    [(K("int", 0), 7), (K("str", "0"), 8), (K("int", 2), 9)],
]
DICT_PROBES = [K("int", 1), K("int", 0), K("int", 10), K("int", -1),
               K("str", "1"), K("str", "a"), K("str", ""), K("str", "10")]

# String (hay, needle) pairs: empty, empty needle, overlap, astral, lone surrogate.
STR_HAYS = [[], [97, 98, 97, 98], [97, 97, 97], [0x1D538, 0x78],
            [0x1F4A9, 97, 98], [0xD800, 97], [120, 120, 104, 105, 120, 120]]
STR_NEEDLES = [[], [97], [97, 98], [0x1D538], [0xD800], [104, 105]]


# --- rendering helpers (identical normal forms on both sides) -----------------

def lean_key(k):
    tag, val = k
    if tag == "int":
        return f"(DKey.kint ({val}))"          # parens: negatives, e.g. (-1)
    cps = ",".join(str(ord(c)) for c in val)
    return f"(DKey.kstr [{cps}])"


def lean_entries(es):
    return "[" + ", ".join(f"({lean_key(k)}, {v})" for k, v in es) + "]"


def lean_cps(cps):
    return "[" + ",".join(str(c) for c in cps) + "]"


def py_key_norm(k):
    tag, val = k
    return f"i{val}" if tag == "int" else "s[" + ",".join(str(ord(c)) for c in val) + "]"


def norm_opt_num(v):        # Option Int / Option Nat
    return "N" if v is None else f"S{v}"


def norm_bool(b):
    return "T" if b else "F"


def norm_num(n):
    return str(n)


def norm_list_int(l):
    return "[" + ",".join(str(x) for x in l) + "]"


def norm_keys(ks):          # ks : list of ("int"/"str", val) tuples
    return ";".join(py_key_norm(k) for k in ks)


# --- build the (label, lean_expr, python_value) triples ----------------------

def build_cases():
    cases = []
    for es in DICT_CORPUS:
        le = lean_entries(es)
        cases.append((f"d2Len {es}", f"fmtNat (d2Len {le})",
                      norm_num(dict_len(es))))
        cases.append((f"d2Keys {es}", f"fmtLK (d2Keys {le})",
                      norm_keys(dict_keys(es))))
        for k in DICT_PROBES:
            lk = lean_key(k)
            cases.append((f"d2Lookup {es} {k}", f"fmtOInt (d2Lookup {lk} {le})",
                          norm_opt_num(dict_lookup(k, es))))
            cases.append((f"d2Mem {es} {k}", f"fmtB (d2Mem {lk} {le})",
                          norm_bool(dict_mem(k, es))))
            cases.append((f"jsObjLookup {es} {k}", f"fmtOInt (jsObjLookup {lk} {le})",
                          norm_opt_num(js_obj_lookup(k, es))))
    for h in STR_HAYS:
        lh = lean_cps(h)
        for t in STR_NEEDLES:
            lt = lean_cps(t)
            cases.append((f"smFindSub {h}/{t}", f"fmtONat (smFindSub {lh} {lt})",
                          norm_opt_num(sm_find_sub(h, t))))
            cases.append((f"smCount {h}/{t}", f"fmtNat (smCount {lh} {lt})",
                          norm_num(sm_count(h, t))))
            cases.append((f"smStartsWith {h}/{t}", f"fmtB (smStartsWith {lh} {lt})",
                          norm_bool(sm_starts_with(h, t))))
            cases.append((f"smReplace {h}/{t}",
                          f"fmtLI (smReplace {lh} {lt} [45,45])",
                          norm_list_int(sm_replace(h, t, [45, 45]))))
            cases.append((f"js16FindSub {h}/{t}", f"fmtONat (js16FindSub {lh} {lt})",
                          norm_opt_num(js16_find_sub(h, t))))
    return cases


LEAN_PRELUDE = r"""import PythExpandVerify
open PythExpandVerify
def fmtOInt : Option Int → String | some x => s!"S{x}" | none => "N"
def fmtONat : Option Nat → String | some x => s!"S{x}" | none => "N"
def fmtB : Bool → String := fun b => if b then "T" else "F"
def fmtNat : Nat → String := fun x => toString x
def fmtLI (l : List Int) : String := "[" ++ String.intercalate "," (l.map toString) ++ "]"
def fmtLK (ks : List DKey) : String :=
  String.intercalate ";" (ks.map (fun k => match k with
    | .kint n => s!"i{n}"
    | .kstr cps => "s[" ++ String.intercalate "," (cps.map toString) ++ "]"))
"""


def main() -> int:
    cases = build_cases()
    lean_src = LEAN_PRELUDE + "".join(f"#eval {expr}\n" for _, expr, _ in cases)

    # Force a FRESH build so `lake env lean` cannot import a stale .olean that
    # hides a Lean-model edit (drift would slip through against an old model).
    build = subprocess.run(["lake", "build"], cwd=HERE, capture_output=True, text=True)
    if build.returncode != 0:
        print("[drift] lake build FAILED (cannot check against a stale model):")
        print(build.stdout)
        print(build.stderr)
        return 2

    tmp = os.path.join(HERE, "_drift_gen.lean")
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(lean_src)
    try:
        proc = subprocess.run(
            ["lake", "env", "lean", "_drift_gen.lean"],
            cwd=HERE, capture_output=True, text=True,
        )
    finally:
        try:
            os.remove(tmp)
        except OSError:
            pass

    if proc.returncode != 0:
        print("[drift] lake env lean FAILED:")
        print(proc.stdout)
        print(proc.stderr)
        return 2

    # Each #eval prints one quoted String line, in source order.
    lean_out = [ln[1:-1] for ln in proc.stdout.splitlines()
                if len(ln) >= 2 and ln[0] == '"' and ln[-1] == '"']

    if len(lean_out) != len(cases):
        print(f"[drift] expected {len(cases)} Lean results, got {len(lean_out)} "
              "— #eval output shape changed; refusing to compare.")
        print(proc.stdout[:2000])
        return 2

    fails = 0
    for (label, _, py_val), lean_val in zip(cases, lean_out):
        if py_val != lean_val:
            print(f"[DRIFT] {label}: python={py_val!r} lean={lean_val!r}")
            fails += 1

    if fails:
        print(f"[spec-lean-drift] {fails}/{len(cases)} cases DRIFTED — the Python "
              "transliteration disagrees with the Lean model. Re-sync before proving.")
        return 1
    print(f"[spec-lean-drift] all {len(cases)} cases agree "
          "(Python transliteration == Lean model, executed).")
    return 0


if __name__ == "__main__":
    sys.setrecursionlimit(100000)
    sys.exit(main())
