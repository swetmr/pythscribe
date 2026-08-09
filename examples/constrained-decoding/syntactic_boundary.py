#!/usr/bin/env python3
"""syntactic_boundary.py — precisely what CFG-constrained decoding does not buy.

The Repairability claim is "a model cannot emit a MALFORMED program". This
script draws the line under it, by walking one deliberately meaningless program
(`syntactic_boundary.ps`) through every gate:

    grammar/pyths.lark  ->  pyths check --syntax-only  ->  pyths check
                        ->  pyths compile              ->  node

and showing that only the LAST one rejects it.

Why it matters for the paper: a permissive grammar accepting "type-nowhere
token soup" is often stated as a defect of the grammar. It is not — it is a
property of context-free grammars. No CFG can express "the operands of `-` have
compatible types" or "this name is bound"; those are not properties of the token
string. What IS worth stating precisely is where the remaining gates stop, and
in PythScribe the type checker is also PARTIAL, so the semantic gap is not
closed downstream either.

Usage:
    cargo build --release --workspace
    python examples/constrained-decoding/syntactic_boundary.py
"""
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
EXE = ".exe" if sys.platform == "win32" else ""
PYTHS = os.environ.get("PYTHS_BIN", os.path.join(ROOT, "target", "release", f"pyths{EXE}"))
FIXTURE = os.path.join(HERE, "syntactic_boundary.ps")
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def run(cmd):
    r = subprocess.run(cmd, capture_output=True, encoding="utf-8",
                       errors="replace", cwd=ROOT)
    err = ANSI.sub("", (r.stderr or r.stdout) or "").strip()
    first = next((l.strip() for l in err.splitlines()
                  if l.strip().startswith("Error:")), "")
    return r.returncode == 0, first[:90]


def main():
    if not os.path.exists(PYTHS):
        print(f"FATAL: {PYTHS} missing — cargo build --release --workspace")
        sys.exit(2)

    src = open(FIXTURE, encoding="utf-8").read()

    print(__doc__.split("Usage:")[0].strip())
    print("\n" + "=" * 72)
    print(f"fixture: {os.path.relpath(FIXTURE, ROOT)}")
    print("=" * 72 + "\n")

    rows = []

    # 1. the constrained-decoding gate itself
    try:
        import lark
        from lark.indenter import PythonIndenter
        p = lark.Lark.open(os.path.join(ROOT, "grammar", "pyths.lark"),
                           parser="lalr", postlex=PythonIndenter(),
                           start="file_input", maybe_placeholders=False)
        try:
            p.parse(src)
            rows.append(("grammar/pyths.lark (the decoder's gate)", True, ""))
        except Exception as e:
            rows.append(("grammar/pyths.lark (the decoder's gate)", False,
                         str(e).split("\n")[0][:60]))
    except ImportError:
        rows.append(("grammar/pyths.lark", None, "lark not installed"))

    # 2..4 the compiler's own gates
    ok, err = run([PYTHS, "check", "--syntax-only", "--quiet", FIXTURE])
    rows.append(("pyths check --syntax-only (authoritative parser)", ok, err))

    ok, err = run([PYTHS, "check", "--quiet", FIXTURE])
    rows.append(("pyths check (parser + TYPE CHECKER)", ok, err))

    out_js = os.path.join(HERE, "_syntactic_boundary_out.js")
    ok, err = run([PYTHS, "compile", "--quiet", FIXTURE, "-o", out_js])
    rows.append(("pyths compile (emits JS)", ok, err))

    # 5. runtime
    ok, err = run([PYTHS, "run", FIXTURE])
    rows.append(("node (RUNTIME)", ok, err or "throws"))
    for f in (out_js,):
        if os.path.exists(f):
            os.unlink(f)

    width = max(len(r[0]) for r in rows)
    for name, ok, err in rows:
        verdict = "n/a " if ok is None else ("ACCEPTS" if ok else "REJECTS")
        print(f"  {name:<{width}}  {verdict}   {err}")

    print(f"""
So: the grammar accepts it, the authoritative parser accepts it, the type
checker accepts it, and it compiles. It is caught only when it runs.

PRECISELY what constrained decoding on grammar/pyths.lark excludes:
  - token sequences outside L(grammar) — unbalanced brackets, a missing `:`,
    a bad dedent, a dangling operator, a stray `$`, two statements on one line
    with no separator.
PRECISELY what it does NOT exclude — and cannot, because these are not
properties of the token string:
  - operator/operand type mismatches   ("hello" - 3, "text" * "text")
  - unbound names                      (undefined_global)
  - attributes that do not exist       (None.nonexistent_method)
  - calling a non-callable             (w = 5; w())
  - arity, purity, termination, or any other semantic property

"A model cannot emit a malformed program" is therefore exactly true and no more:
malFORMED, not meaningless.""")


if __name__ == "__main__":
    main()
