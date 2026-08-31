"""Differential plumbing + by-design normalizers for the vendored autotester.

Vendored (adapted) from reference-app `experiments/pbt-ps/differential.py` — the
run/retry plumbing and the ONLY normalizers the conformance gate may apply,
all documented by-design PythScribe deviations:

  * d1        — whole-float/bool repr unification (`1.0` -> `1`) and set/dict
                INTERNAL order (JS number/object model)
  * set_order — same multiset, different sequence order
  * ulp       — transcendental last-bit (<= 4 ULP) differences (JS Math vs libm)

Anything else is a REAL divergence. CPython is the oracle — resolved via
PYTHS_ORACLE_PYTHON (e.g. "py -3.14"); in CI, actions/setup-python pins plain
"python" AS the oracle (docs/python-oracle-policy.md). Transcrypt's own
expected outputs are never consulted (the model<->model trap).
"""
from __future__ import annotations

import math
import os
import re
import shlex
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
_EXE = "pyths.exe" if sys.platform == "win32" else "pyths"
PYTHS = os.environ.get("PYTHS_BIN") or next(
    (str(p) for p in (ROOT / "target" / "release" / _EXE,
                      ROOT / "target" / "debug" / _EXE) if p.exists()),
    str(ROOT / "target" / "release" / _EXE),
)
ORACLE = shlex.split(os.environ.get("PYTHS_ORACLE_PYTHON", "python"))
TIMEOUT = int(os.environ.get("PS_TIMEOUT", "45"))
SENTINEL = "__PS_PASS__"

_ANSI = re.compile(r"\x1b\[[0-9;]*m")
_INFRA = re.compile(r"(EAGAIN|ENOMEM|cannot allocate|resource temporarily unavailable|"
                    r"ETXTBSY|being used by another process|Access is denied|"
                    r"spawn \w+ E|EBUSY|The process cannot access the file)", re.I)
RETRIES = int(os.environ.get("PS_RETRY", "2"))


def run(cmd, timeout=None):
    """Popen + taskkill /T so the whole tree (pyths -> node) dies on timeout."""
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        out, err = p.communicate(timeout=timeout or TIMEOUT)
        return p.returncode, out.decode("utf-8", "replace"), err.decode("utf-8", "replace")
    except subprocess.TimeoutExpired:
        if sys.platform == "win32":
            subprocess.run(["taskkill", "/F", "/T", "/PID", str(p.pid)],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        else:
            p.kill()
        try:
            p.communicate(timeout=5)
        except Exception:
            pass
        return -9, "", "TIMEOUT"


def _is_infra(rc, stderr):
    return rc == -9 or (rc != 0 and bool(_INFRA.search(stderr or "")))


def run_robust(cmd, timeout=None):
    rc, out, err = run(cmd, timeout=timeout)
    tries = 0
    while _is_infra(rc, err) and tries < RETRIES:
        tries += 1
        rc, out, err = run(cmd, timeout=timeout)
    return rc, out, err


def real_error(stderr):
    stderr = _ANSI.sub("", stderr or "")
    lines = [l.strip() for l in stderr.splitlines() if l.strip()]
    for l in lines:
        if re.search(r"(ReferenceError|TypeError|RangeError|SyntaxError|Error \[|is not a function|"
                     r"is not defined|Cannot read|error:|panicked|not supported|Unexpected|Expected|"
                     r"ERR_MODULE_NOT_FOUND|No such|Unsupported)", l):
            return l[:200]
    for l in lines:
        if "Node.js execution failed" not in l and "Node.js v" not in l:
            return l[:200]
    return (lines[-1] if lines else "")[:200]


def norm_out(s):
    return (s or "").replace("\r\n", "\n").strip()


_EVAL_NS = {"inf": math.inf, "nan": math.nan, "True": True, "False": False, "None": None}


def _lit(line):
    # eval (not ast.literal_eval) is deliberate: repr lines may contain inf/nan
    # (Names, not literals). Input is stdout of testlets THIS harness runs,
    # locally; builtins are stripped. Same contained pattern as the source
    # harness (experiments/pbt-ps).
    try:
        return ("ok", eval(line, {"__builtins__": {}}, dict(_EVAL_NS)))
    except Exception:
        return ("raw", line)


def _num_norm(v):
    if isinstance(v, bool):
        return float(v)
    if isinstance(v, (int, float)):
        return float(v)
    if isinstance(v, str):
        return ("s", v)
    if isinstance(v, (list, tuple)):
        return ("seq", tuple(_num_norm(x) for x in v))
    if isinstance(v, (set, frozenset)):
        return ("set", frozenset(_num_norm(x) for x in v))
    if isinstance(v, dict):
        return ("map", frozenset((_num_norm(k), _num_norm(val)) for k, val in v.items()))
    if isinstance(v, complex):
        return ("c", v.real, v.imag)
    return ("o", repr(v))


def _multiset(v):
    n = _num_norm(v)
    def rec(x):
        if isinstance(x, tuple) and x and x[0] == "seq":
            return ("seq", tuple(sorted((rec(e) for e in x[1]), key=repr)))
        return x
    return rec(n)


def _ulp_close(a, b, n=4):
    if not (isinstance(a, float) or isinstance(b, float)):
        return False
    try:
        a, b = float(a), float(b)
    except (TypeError, ValueError):
        return False
    if a == b:
        return True
    if math.isnan(a) or math.isnan(b) or math.isinf(a) or math.isinf(b):
        return False
    return abs(a - b) <= n * math.ulp(max(abs(a), abs(b)))


def _ulp_eq_rec(a, b):
    if isinstance(a, (int, float, bool)) and isinstance(b, (int, float, bool)):
        return float(a) == float(b) or _ulp_close(a, b)
    if isinstance(a, (list, tuple)) and isinstance(b, (list, tuple)):
        return len(a) == len(b) and all(_ulp_eq_rec(x, y) for x, y in zip(a, b))
    return a == b


def classify_line(cpy, ps):
    ka, va = _lit(cpy); kb, vb = _lit(ps)
    if ka == "ok" and kb == "ok":
        if _num_norm(va) == _num_norm(vb):
            return "d1"
        if _multiset(va) == _multiset(vb):
            return "set_order"
        if _ulp_eq_rec(va, vb):
            return "ulp"
        return "real"
    return "real"


def classify_outputs(out_py, out_ps):
    """(category, examples) for two diverging outputs; category in d1|set_order|ulp|real."""
    a = norm_out(out_py).splitlines(); b = norm_out(out_ps).splitlines()
    if a and a[-1] == SENTINEL: a = a[:-1]
    if b and b[-1] == SENTINEL: b = b[:-1]
    cats = set(); examples = []
    for i in range(max(len(a), len(b))):
        av = a[i] if i < len(a) else "<none>"
        bv = b[i] if i < len(b) else "<none>"
        if av == bv:
            continue
        c = classify_line(av, bv)
        cats.add(c)
        if len(examples) < 4:
            examples.append((i, av, bv, c))
    for sev in ("real", "set_order", "ulp", "d1"):
        if sev in cats:
            return sev, examples
    return "d1", examples
