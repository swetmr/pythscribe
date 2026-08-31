#!/usr/bin/env python3
"""Shipped-binding differential for the C1C3C4 exception-MESSAGE layer (E7).

Binds verification/message-table.json (and the generated MessageData.lean the
C1C3C4Outcome model builds its live literals from) to the REAL shipped
behavior: every `witnesses` row is run as a whole program through

  * the real `pyths` binary (`pyths run` — the inline-runtime path), and
  * the pinned CPython oracle (PYTHS_ORACLE_PYTHON, e.g. "py -3.14"; CI's
    setup-python makes plain "python" the oracle),

and BOTH terminal `Kind: message` lines must equal the row's instantiated
template. Three-legged bind: table == pyths and table == CPython here;
table == Lean via gen-message-data.py --check (the generated-file drift
gate). A future runtime message change therefore turns THIS differential red,
and the table update it forces re-evaluates the Lean #guard pins — the Lean
gate can no longer stay green while asserting an obsolete message (the 3.14
oracle-bump drift, root-fixed).

Harness-integrity assertion (E7 sub-part 4): rows loaded == rows executed ==
rows compared, checked before the verdict — a batching/parse defect that
drops witnesses fails loud instead of shrinking the corpus.

Run from the repo root (requires target/{release,debug}/pyths.exe + oracle):
    python verification/message_shipped_binding.py
Env: PYTHS_BIN, PYTHS_ORACLE_PYTHON (default: "python"), PS_TIMEOUT.
"""
from __future__ import annotations

import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TABLE = ROOT / "verification" / "message-table.json"
_EXE = "pyths.exe" if sys.platform == "win32" else "pyths"
PYTHS = Path(os.environ.get("PYTHS_BIN") or next(
    (str(p) for p in (ROOT / "target" / "release" / _EXE,
                      ROOT / "target" / "debug" / _EXE) if p.exists()),
    str(ROOT / "target" / "release" / _EXE),
))
ORACLE = shlex.split(os.environ.get("PYTHS_ORACLE_PYTHON", "python"))
TIMEOUT = int(os.environ.get("PS_TIMEOUT", "30"))

# Terminal exception line: `SomeError: message` / `SomeException: message`.
# The class name must carry a Python-taxonomy suffix (or be one of the exact
# suffixless builtins) — otherwise node noise like `file: ...` / `args: [...]`
# would shadow the real line.
_EXC_LINE = re.compile(
    r"^([A-Za-z_][A-Za-z0-9_]*(?:Error|Exception|Warning)"
    r"|StopIteration|StopAsyncIteration|KeyboardInterrupt|SystemExit|GeneratorExit"
    r"):\s?(.*)$")
# Node renders a name-patched `Error` (the runtime's `e.name = "<Kind>"`
# pattern) as `Error [<Kind>]: message` — same terminal surface, bracket form.
_EXC_LINE_NODE = re.compile(r"^Error \[([A-Za-z_][A-Za-z0-9_]*)\]:\s?(.*)$")
_ANSI = re.compile(r"\x1b\[[0-9;]*m")


def run(cmd: list[str], env: dict | None = None) -> tuple[int, str, str]:
    p = subprocess.run(cmd, capture_output=True, timeout=TIMEOUT, env=env)
    return (p.returncode,
            p.stdout.decode("utf-8", "replace"),
            p.stderr.decode("utf-8", "replace"))


def last_exc_line(stderr: str) -> str | None:
    """The LAST `Kind: message` line of a traceback/node error dump."""
    best = None
    for raw in _ANSI.sub("", stderr).splitlines():
        line = raw.strip()
        m = _EXC_LINE_NODE.match(line) or _EXC_LINE.match(line)
        # Skip harness noise ("Error: Node.js execution failed") — the real
        # Python-taxonomy line names a concrete exception class.
        if m and m.group(1) not in ("Error",):
            best = f"{m.group(1)}: {m.group(2)}"
    return best


def instantiate(template: str, args: dict[str, str]) -> str:
    out = template
    for k, v in args.items():
        out = out.replace("{" + k + "}", v)
    if "{" in out:
        raise SystemExit(f"unfilled placeholder in {template!r} with {args}")
    return out


def main() -> int:
    table = json.loads(TABLE.read_text(encoding="utf-8"))
    templates = table["templates"]
    witnesses = table["witnesses"]
    loaded = len(witnesses)

    if not Path(PYTHS).exists():
        sys.exit(f"pyths binary not found at {PYTHS} — run `cargo build --release -p pyths_cli`")
    rc, _, err = run([*ORACLE, "-c", "print(1)"])
    if rc != 0:
        sys.exit(f"oracle {' '.join(ORACLE)} not runnable: {err.strip()[:200]}")
    # Review r2 (should-fix): the table's `oracle` field is an EXACT pin
    # ("cpython-X.Y.Z"), and the resolved interpreter must BE that version —
    # a floating 3.14.x drift would silently re-referent every witness.
    pinned = table.get("oracle", "")
    m = re.fullmatch(r"cpython-(\d+\.\d+\.\d+)", pinned)
    if not m:
        sys.exit(f"message-table.json `oracle` must be 'cpython-X.Y.Z', got {pinned!r}")
    rc, out, _ = run([*ORACLE, "-c", "import platform; print(platform.python_version())"])
    got = out.strip()
    if rc != 0 or got != m.group(1):
        sys.exit(f"oracle version mismatch: message-table.json pins CPython "
                 f"{m.group(1)}, resolved oracle ({' '.join(ORACLE)}) is {got!r} — "
                 f"point PYTHS_ORACLE_PYTHON at the pinned version or re-probe "
                 f"the table against the new oracle")

    executed = 0
    compared = 0
    failures: list[str] = []
    env = {**os.environ, "PYTHS_NO_CACHE": "1",
           "PYTHONUTF8": "1", "PYTHONIOENCODING": "utf-8"}

    with tempfile.TemporaryDirectory() as d:
        for w in witnesses:
            wid = w["id"]
            expected = f"{w['kind']}: {instantiate(templates[w['template']], w['args'])}"
            src = (w["setup"] + "\n" if w["setup"] else "") + w["stmt"] + "\n"
            ps = Path(d) / f"{wid}.ps"
            py = Path(d) / f"{wid}.py"
            ps.write_text(src, encoding="utf-8")
            py.write_text(src, encoding="utf-8")
            executed += 1

            rc_py, _, err_py = run([*ORACLE, "-X", "utf8", str(py)], env=env)
            got_py = last_exc_line(err_py)
            rc_ps, _, err_ps = run([str(PYTHS), "run", str(ps)], env=env)
            got_ps = last_exc_line(err_ps)
            compared += 1

            if rc_py == 0 or got_py is None:
                failures.append(f"{wid}: ORACLE did not raise (rc={rc_py}, line={got_py!r})")
                continue
            if got_py != expected:
                failures.append(f"{wid}: table != CPython oracle\n"
                                f"    table:   {expected!r}\n    cpython: {got_py!r}")
            if rc_ps == 0 or got_ps is None:
                failures.append(f"{wid}: pyths did not raise (rc={rc_ps}, line={got_ps!r}, "
                                f"stderr={err_ps.strip()[:160]!r})")
                continue
            if got_ps != expected:
                failures.append(f"{wid}: table != shipped pyths\n"
                                f"    table: {expected!r}\n    pyths: {got_ps!r}")

    # Harness-integrity: the corpus cannot silently shrink.
    if not (loaded == executed == compared) or loaded == 0:
        sys.exit(f"HARNESS INTEGRITY FAILURE: loaded={loaded} executed={executed} "
                 f"compared={compared} (must all be equal and nonzero)")

    ok = loaded - len({f.split(":")[0] for f in failures})
    print(f"[message-binding] {ok}/{loaded} witnesses: table == CPython oracle "
          f"({' '.join(ORACLE)}) == shipped pyths ({PYTHS})")
    if failures:
        print(f"{len(failures)} failure(s):")
        for f in failures:
            print("  - " + f)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
