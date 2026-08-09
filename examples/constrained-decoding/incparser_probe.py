#!/usr/bin/env python3
"""incparser_probe.py — WHY does the live SynCode decoder fall back?

Isolates the exact mechanism, with no model, no GPU and no 2 GB mask store.

MECHANISM
---------
SynCode masks logits by re-parsing the partial output at EVERY decoding step:

    grammar_mask/grammar_constrainer.py:206
        res = self.inc_parser.get_acceptable_next_terminals(partial_output)
      except Exception as e:                       # :215
        ...
        logger.info("Parsing failed! Falling back to unconstrained decoding.")
        skip = True                                # :225

`skip = True` means the mask is NOT applied for that step — the model decodes
UNCONSTRAINED. So the fallback rate is exactly the rate at which SynCode's
incremental parser throws on a partial program. (Note `self.parse_failed` is a
latch, so the message is LOGGED once per generation but the skip happens on
every failing step: reading the log undercounts the fallback.)

So: feed the incremental parser successive prefixes of KNOWN-VALID .ps programs
and count how often it throws. No LLM needed — the LLM only decides which
prefixes come up.

ROOT CAUSE
----------
syncode/parsers/__init__.py:create_parser

    if grammar.name == 'python' and not use_symbol_pos_map:
        indenter = PythonIndenter()
    ...
    if grammar.name == 'python':
        return PythonIncrementalParser(base_parser, indenter, **kwargs)
    return incremental_parser.IncrementalParser(base_parser, **kwargs)

and syncode/parsers/grammars/grammar.py:Grammar.__init__ sets `self.name` to
the FILE PATH when the grammar is supplied as a `.lark` path.

Our grammar is supplied as a path, so `grammar.name != 'python'`, so:

  1. indenter is None       -> _INDENT / _DEDENT are NEVER emitted. Our grammar
                               (like Python's) needs them: `suite: _NL _INDENT
                               stmt+ _DEDENT`. Every indented block is therefore
                               unparseable, the incremental parser throws, and
                               the decoder silently drops to unconstrained.
  2. the generic IncrementalParser is used instead of PythonIncrementalParser,
     which is the class that actually manages the INDENT/DEDENT queue.
  3. Grammar.simplifications() returns {} instead of the Python terminal
     simplifications, so the LONG_STRING lookbehind is never simplified away
     (this is the `interegular` "lookbacks are not implemented" error that the
     old harness worked around by hand-rewriting the regex).

Indentation support is keyed to the literal string 'python'. It is not exposed
as an option. A custom indentation-sensitive grammar can therefore never get an
indenter through the public API.

THE FIX (entirely on our side; SynCode is not patched)
-----------------------------------------------------
  a. present the grammar under `name = 'python'`, which is what switches on the
     indenter, PythonIncrementalParser, and the terminal simplifications; and
  b. rename our newline terminal `_NEWLINE` -> `_NL` in the decoder-facing
     wrapper, because SynCode's PythonIndenter hard-codes `NL_type = "_NL"`.

Both are mechanical and apply ONLY to the copy handed to the decoder. The
canonical grammar/pyths.lark and its CI gate are untouched, and every generated
program is still re-verified against canonical grammar/pyths.lark and
`pyths check`.

USAGE
    python examples/constrained-decoding/incparser_probe.py
    python examples/constrained-decoding/incparser_probe.py --json out.json
"""
import argparse
import json
import os
import sys
import warnings

warnings.filterwarnings("ignore")

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)

from syncode_grammar import (  # noqa: E402
    build_wrapper, make_grammar, PROBE_PROGRAMS,
)


def prefixes(src, step=1):
    """Every prefix of `src` at character granularity — a superset of the
    prefixes any tokenizer can produce, so the measurement does not depend on
    a particular tokenizer's vocabulary."""
    return [src[:i] for i in range(1, len(src) + 1, step)]


def probe(mode, programs):
    """mode: 'baseline' (grammar as a path, as the old harness did) or
    'fixed' (name='python' + _NL). Returns per-program throw counts."""
    from syncode.parsers import create_parser

    grammar = make_grammar(mode)
    results = []
    for name, src in programs:
        try:
            parser = create_parser(grammar)
        except Exception as e:
            results.append({"program": name, "error": f"parser build failed: {e}",
                            "prefixes": 0, "throws": 0})
            continue
        throws, total, first_fail = 0, 0, None
        for pfx in prefixes(src):
            total += 1
            try:
                parser.reset()
                parser.get_acceptable_next_terminals(pfx)
            except Exception as e:
                throws += 1
                if first_fail is None:
                    first_fail = {"prefix": pfx[-60:], "err": str(e).split("\n")[0][:110]}
        results.append({
            "program": name, "prefixes": total, "throws": throws,
            "rate": throws / total if total else 0.0,
            "first_fail": first_fail,
        })
    return results


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", default=None)
    args = ap.parse_args()

    build_wrapper()
    print("SynCode incremental-parser probe — prefixes of KNOWN-VALID .ps programs")
    print("A throw here IS the fallback: grammar_constrainer.py sets skip=True and")
    print("the step decodes unconstrained.\n")

    record = {}
    for mode, label in [("baseline", "BASELINE (grammar as a path -> no indenter)"),
                        ("fixed", "FIXED (name='python' + _NL -> indenter attached)")]:
        print(f"== {label}")
        res = probe(mode, PROBE_PROGRAMS)
        record[mode] = res
        tot_p = sum(r["prefixes"] for r in res)
        tot_t = sum(r["throws"] for r in res)
        for r in res:
            flag = "OK " if r["throws"] == 0 else "FALLBACK"
            print(f"   {flag:9s} {r['program']:22s} "
                  f"{r['throws']:4d}/{r['prefixes']:4d} prefixes throw "
                  f"({r['rate']*100:5.1f}%)")
            if r.get("first_fail"):
                print(f"             first: {r['first_fail']['err']}")
        rate = tot_t / tot_p if tot_p else 0
        print(f"   {'TOTAL':9s} {'':22s} {tot_t:4d}/{tot_p:4d} "
              f"({rate*100:5.1f}%)\n")
        record[f"{mode}_rate"] = rate

    b, f = record["baseline_rate"], record["fixed_rate"]
    print("=" * 70)
    print(f"incremental-parser throw rate  baseline {b*100:.1f}%  ->  fixed {f*100:.1f}%")
    print("(this is the per-step probability the decoder drops the mask)")
    if args.json:
        json.dump(record, open(args.json, "w", encoding="utf-8"), indent=2)
        print(f"wrote {args.json}")


if __name__ == "__main__":
    main()
