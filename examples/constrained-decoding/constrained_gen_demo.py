#!/usr/bin/env python3
"""constrained_gen_demo.py — Tier 2: LIVE grammar-constrained generation.

Runs a small local HF model (default Qwen/Qwen2.5-Coder-0.5B-Instruct) on CPU
under SynCode grammar-constrained decoding with grammar/pyths.lark, and shows
the constrained output ALWAYS parses (a malformed program is unreachable),
versus unconstrained sampling from the same model.

The independent acceptance check is `grammar/pyths.lark` via lark (the same
acceptor proven sound in acceptor_demo.py) AND `pyths check` (authoritative).
SynCode enforces the grammar during decoding; we re-verify the output with an
independent parser so the guarantee is not merely self-reported.

Baseline note: the honest unconstrained syntax-error baseline for the CLAIM is
the measured baseline-001 rate (ps/macro 0.023, psc/micro 0.005), cited in the
README. The tiny 0.5B in-script unconstrained run is only an illustration; it
may already be mostly-valid on a trivial prompt.

Repro:
    pip install syncode lark
    cargo build --release --workspace
    python examples/constrained-decoding/constrained_gen_demo.py --n 5

Flags:
    --model ID   HF model id (default Qwen/Qwen2.5-Coder-0.5B-Instruct)
    --n N        samples per condition (default 5)
    --max-new T  max new tokens (default 48)
    --json PATH  write summary
"""
import argparse
import json
import os
import subprocess
import sys
import warnings

warnings.filterwarnings("ignore")

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
EXE = ".exe" if sys.platform == "win32" else ""
PYTHS = os.environ.get("PYTHS_BIN", os.path.join(ROOT, "target", "release", f"pyths{EXE}"))
GRAMMAR = os.path.join(ROOT, "grammar", "pyths.lark")
TMPDIR = os.path.dirname(os.path.abspath(__file__))
# SynCode's bundled lark fork requires a rule literally named `start`; our
# canonical grammar uses start="file_input". This thin wrapper (generated on
# first run) adds `start: file_input` so SynCode can consume the SAME grammar
# body unchanged. The independent acceptor + `pyths check` still use canonical
# grammar/pyths.lark, so the guarantee is verified against the real grammar.
SYNCODE_GRAMMAR = os.path.join(TMPDIR, "pyths_syncode.lark")


def ensure_syncode_grammar():
    """Generate the SynCode-consumable wrapper from canonical grammar/pyths.lark.

    Two mechanical, decoder-only adaptations (the canonical grammar is untouched):
      1. add `start: file_input` — SynCode's lark fork requires a `start` rule.
      2. replace the LONG_STRING terminal's regex, which uses a lookbehind
         `(?<!\\)` (SynCode's interegular FSM compiler raises
         "lookbacks are not implemented"), with a lookbehind-free equivalent
         `(f|r|R)?(\"\"\".*?\"\"\"|'''.*?''')`. This is marginally more permissive
         on backslash-escaped triple-quotes — irrelevant for the constrained-
         decoding gate, and the independent acceptance check still uses the
         CANONICAL grammar + `pyths check`.
    """
    import re as _re
    body = open(GRAMMAR, encoding="utf-8").read()
    body = _re.sub(
        r"^LONG_STRING:.*$",
        r'LONG_STRING: /(f|r|R)?(""".*?"""|' + r"'''.*?''')/s",
        body, count=1, flags=_re.MULTILINE,
    )
    with open(SYNCODE_GRAMMAR, "w", encoding="utf-8", newline="\n") as f:
        f.write(body + "\nstart: file_input\n")

PROMPT = (
    "Write a single PythScribe function named `square` that takes one argument "
    "`n` and returns n times n. Output only the function definition, no prose, "
    "no markdown fence."
)


def build_acceptor():
    import lark
    from lark.indenter import PythonIndenter
    return lark.Lark.open(GRAMMAR, parser="lalr", postlex=PythonIndenter(),
                          start="file_input", maybe_placeholders=False)


def grammar_accepts(parser, src):
    try:
        parser.parse(src + "\n")
        return True
    except Exception:
        return False


def pyths_check_accepts(src):
    path = os.path.join(TMPDIR, ".gen_tmp.ps")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(src)
    try:
        r = subprocess.run([PYTHS, "check", path], capture_output=True,
                           encoding="utf-8", errors="replace")
    finally:
        try:
            os.remove(path)
        except OSError:
            pass
    return r.returncode == 0


def strip_fence(text):
    t = text.strip()
    if "```" in t:
        import re
        m = re.search(r"```[a-zA-Z0-9_]*\n?(.*?)```", t, re.DOTALL)
        if m:
            return m.group(1).strip()
    return t


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default="Qwen/Qwen2.5-Coder-0.5B-Instruct")
    ap.add_argument("--n", type=int, default=5)
    ap.add_argument("--max-new", type=int, default=48)
    ap.add_argument("--json", default=None)
    args = ap.parse_args()

    if not os.path.exists(PYTHS):
        print(f"FATAL: {PYTHS} missing — cargo build --release --workspace", file=sys.stderr)
        sys.exit(2)

    from syncode import Syncode

    acceptor = build_acceptor()
    print(f"model   : {args.model}  (device=cpu)")
    print(f"grammar : grammar/pyths.lark  (SynCode grammar_strict, indent=True)")
    print(f"prompt  : {PROMPT!r}\n")

    common = dict(model=args.model, device="cpu", quantize=False,
                  parse_output_only=True, indent=True,
                  max_new_tokens=args.max_new, do_sample=True,
                  temperature=0.8, seed=0)

    results = {}
    for mode_label, mode in (("unconstrained", "original"),
                             ("constrained", "grammar_strict")):
        print(f"--- {mode_label} (mode={mode}) ---")
        kwargs = dict(common)
        if mode == "grammar_strict":
            ensure_syncode_grammar()
            kwargs["grammar"] = SYNCODE_GRAMMAR
        llm = Syncode(mode=mode, **kwargs)
        ok = 0
        samples = []
        for i in range(args.n):
            try:
                out = llm.infer(PROMPT)
                out = out[0] if isinstance(out, list) else out
            except Exception as e:
                out = f"<<infer error: {type(e).__name__}: {str(e)[:120]}>>"
            code = strip_fence(out)
            g_ok = grammar_accepts(acceptor, code)
            c_ok = pyths_check_accepts(code) if g_ok else False
            parses = g_ok  # grammar acceptance is the syntactic gate
            ok += 1 if parses else 0
            samples.append({"sample": i, "grammar_ok": g_ok,
                            "pyths_check_ok": c_ok, "code": code})
            print(f"  [{i}] grammar_parses={g_ok} pyths_check={c_ok}")
            print("      " + code.replace("\n", "\n      "))
        rate = ok / args.n * 100 if args.n else 0.0
        err_rate = (args.n - ok) / args.n if args.n else 0.0
        print(f"  => parses {ok}/{args.n} ({rate:.1f}%)  syntax_error_rate={err_rate:.3f}\n")
        results[mode_label] = {"parses": ok, "n": args.n,
                               "syntax_error_rate": err_rate, "samples": samples}
        del llm

    print("=== SUMMARY ===")
    print(f"  unconstrained (0.5B, this run): syntax_error_rate="
          f"{results['unconstrained']['syntax_error_rate']:.3f}")
    print(f"  constrained   (grammar_strict): syntax_error_rate="
          f"{results['constrained']['syntax_error_rate']:.3f}")
    print("  baseline-001 measured unconstrained rate (the cited claim): "
          "ps/macro=0.023, psc/micro=0.005")
    verdict = results["constrained"]["syntax_error_rate"] == 0.0
    print(f"  constrained output ALWAYS parses: {verdict}")

    if args.json:
        with open(args.json, "w", encoding="utf-8") as f:
            json.dump({"model": args.model, "prompt": PROMPT, "results": results,
                       "constrained_always_parses": bool(verdict)}, f, indent=2)
        print(f"\nwrote {args.json}")


if __name__ == "__main__":
    main()
