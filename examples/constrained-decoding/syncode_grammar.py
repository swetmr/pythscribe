#!/usr/bin/env python3
"""syncode_grammar.py — the decoder-facing view of grammar/pyths.lark.

Single source of truth for how the CANONICAL grammar is handed to SynCode, so
the probe (incparser_probe.py) and the live run (constrained_gen_demo.py)
cannot drift apart.

The canonical grammar and its CI gate (scripts/test-grammar.py,
scripts/grammar-fuzz.py) are NEVER modified. Everything here is a mechanical
adaptation of a COPY, forced by SynCode's implementation, and every generated
program is still re-verified against canonical grammar/pyths.lark AND
`pyths check`. So the guarantee is never self-reported.

THE THREE ADAPTATIONS
---------------------
1. `start: file_input`
   SynCode's bundled lark fork requires a rule literally named `start`.

2. `_NEWLINE` -> `_NL`
   SynCode's PythonIndenter hard-codes `NL_type = "_NL"`
   (syncode/parsers/python_parser.py:178). Our canonical terminal is
   `_NEWLINE`, so without this rename the indenter never fires on our newlines.

3. name = 'python'
   syncode/parsers/__init__.py:create_parser attaches the indenter and the
   INDENT/DEDENT-aware PythonIncrementalParser ONLY when `grammar.name ==
   'python'`, and Grammar.__init__ sets `name` to the FILE PATH for a
   path-supplied grammar. Indentation support is not otherwise reachable
   through the public API. Presenting our grammar under that name is what
   turns the indenter on; it also enables Grammar.simplifications(), whose
   Python terminal simplifications rewrite the LONG_STRING regex and remove
   the lookbehind that `interegular` cannot compile ("lookbacks are not
   implemented") — so the old harness's hand-written LONG_STRING patch is no
   longer needed.

Adaptation 3 is why the live decoder used to fall back to unconstrained
decoding: with no indenter, `_INDENT`/`_DEDENT` are never emitted, so every
indented block is unparseable, the incremental parser throws, and
grammar_constrainer.py sets `skip=True` (mask not applied) for that step.
"""
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
CANONICAL = os.path.join(ROOT, "grammar", "pyths.lark")

# baseline = exactly what the previous harness did (path-supplied grammar,
#            _NEWLINE kept, LONG_STRING lookbehind hand-patched).
# fixed    = the two mechanical adaptations above + name='python'.
WRAPPER_BASELINE = os.path.join(HERE, "pyths_syncode.lark")
WRAPPER_FIXED = os.path.join(HERE, "pyths_syncode_indent.lark")


def build_wrapper():
    """(Re)generate both decoder-facing grammars from the canonical one."""
    body = open(CANONICAL, encoding="utf-8").read()

    # -- baseline: the pre-existing adaptation set --------------------------
    base = re.sub(
        r"^LONG_STRING:.*$",
        r'LONG_STRING: /(f|r|R)?(""".*?"""|' + r"'''.*?''')/s",
        body, count=1, flags=re.MULTILINE,
    )
    with open(WRAPPER_BASELINE, "w", encoding="utf-8", newline="\n") as f:
        f.write(base + "\nstart: file_input\n")

    # -- fixed: rename the newline terminal to the one the indenter expects --
    # Whole-word so `_NEWLINE` never partially matches something else.
    fixed = re.sub(r"\b_NEWLINE\b", "_NL", base)
    with open(WRAPPER_FIXED, "w", encoding="utf-8", newline="\n") as f:
        f.write(fixed + "\nstart: file_input\n")
    return WRAPPER_BASELINE, WRAPPER_FIXED


def make_grammar(mode):
    """Build the SynCode Grammar object for 'baseline' or 'fixed'."""
    from syncode.parsers.grammars.grammar import Grammar

    if mode == "baseline":
        return Grammar(WRAPPER_BASELINE)

    g = Grammar(WRAPPER_FIXED)
    # This single assignment is the fix: it is the only way to reach SynCode's
    # indenter, which is gated on the literal name 'python'. The grammar BODY
    # is still ours — only the label changes, and Grammar.hash() keys off the
    # body, so the mask store stays correctly keyed to our grammar.
    g.name = "python"
    return g


# Known-valid .ps programs, re-verified by `pyths check` in the test suite.
# Chosen to cover the constructs a decoder actually has to walk through:
# indentation, nested blocks, one-line suites, comprehensions, classes, and
# the .ps-specific operators.
PROBE_PROGRAMS = [
    ("flat_assign", "x = 1\ny = 2\n"),
    ("def_oneline", "def square(n): return n * n\n"),
    ("def_indented", "def square(n):\n    return n * n\n"),
    ("if_indented", "def f(n):\n    if n > 0:\n        return n\n    return 0\n"),
    ("nested_blocks", "def f(xs):\n    total = 0\n    for x in xs:\n        if x > 0:\n            total = total + x\n    return total\n"),
    ("class_method", "class Counter:\n    def __init__(self):\n        self.n = 0\n\n    def inc(self):\n        self.n = self.n + 1\n"),
    ("comprehension", "squares = [n * n for n in range(10) if n % 2 == 0]\n"),
    ("ps_operators", "name = user?.profile?.name ?? \"anon\"\n"),
    ("with_try", "def load(p):\n    try:\n        return read(p)\n    except IOError:\n        return None\n"),
]
