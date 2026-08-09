#!/usr/bin/env python3
"""constrained_measure.py — LIVE grammar-constrained decoding, MEASURED.

Answers, with numbers rather than an anecdote:

  1. FALLBACK RATE. How often does SynCode actually drop the grammar mask and
     decode unconstrained? Counted directly, not read off the log.
  2. Does the indenter fix (see syncode_grammar.py) drive it to zero?
  3. VALIDITY. Of N constrained completions, how many are (i) accepted by the
     canonical grammar, (ii) accepted by `pyths check` (the authoritative
     parser), (iii) compiled by `pyths compile`?

HOW THE FALLBACK IS COUNTED
---------------------------
grammar_constrainer.py:GrammarConstrainer._parse_partial_output returns
`(res, skip)`. `skip=True` means the incremental parser threw, so
mask_scores() leaves the logits untouched: THAT STEP DECODES UNCONSTRAINED.
We wrap that method and count every `skip=True`.

Reading SynCode's log instead would UNDERCOUNT badly: `self.parse_failed` is a
latch, so "Falling back to unconstrained decoding" is emitted at most once per
generation no matter how many steps actually lost the mask.

We report two rates:
  step-level      — fraction of decoding steps that decoded unconstrained
  completion-level— fraction of completions with >= 1 unconstrained step
                    (a single lost step is enough to emit a malformed program)

DISK / COST
-----------
CPU only, no paid API. Reuses the cached Qwen2.5-Coder-0.5B-Instruct and the
SynCode mask store via HF_CACHE / SYNCODE_CACHE (point them at an existing
cache; the mask store is ~2 GB and takes ~10 min to build the first time for a
given grammar hash).

USAGE
    HF_CACHE=../pythscribe/cache/ SYNCODE_CACHE=../pythscribe/cache/ \
    python examples/constrained-decoding/constrained_measure.py --n 60
"""
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import warnings

warnings.filterwarnings("ignore")

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)

EXE = ".exe" if sys.platform == "win32" else ""
PYTHS = os.environ.get("PYTHS_BIN", os.path.join(ROOT, "target", "release", f"pyths{EXE}"))
CANONICAL = os.path.join(ROOT, "grammar", "pyths.lark")
ANSI = re.compile(r"\x1b\[[0-9;]*m")

# Prompts chosen so a correct answer NEEDS an indented block — that is exactly
# where the un-fixed decoder loses the mask. A prompt set of one-liners would
# flatter the baseline.
PROMPTS = [
    "Write a PythScribe function `square(n)` that returns n times n.",
    "Write a PythScribe function `total(xs)` that sums a list using a for loop.",
    "Write a PythScribe function `evens(xs)` returning the even numbers in xs.",
    "Write a PythScribe class `Counter` with an `inc` method incrementing self.n.",
    "Write a PythScribe function `safe_div(a, b)` returning None if b is zero.",
    "Write a PythScribe function `greet(name)` that returns a greeting string.",
]


def strip_ansi(s):
    return ANSI.sub("", s or "")


def pyths_run(sub, src, extra=None):
    fd, path = tempfile.mkstemp(suffix=".ps", text=True)
    out = None
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(src)
        cmd = [PYTHS, sub] + (extra or []) + [path]
        r = subprocess.run(cmd, capture_output=True, encoding="utf-8",
                           errors="replace")
        ok = r.returncode == 0
        err = strip_ansi(r.stderr or r.stdout).strip().split("\n")
        err = next((l for l in err if l.strip().startswith("Error:")), "")
        return ok, err[:120]
    finally:
        try:
            os.unlink(path)
        except OSError:
            pass
        if out and os.path.exists(out):
            os.unlink(out)


def grammar_accepts(parser, src):
    try:
        parser.parse(src if src.endswith("\n") else src + "\n")
        return True
    except Exception:
        return False


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=60, help="total completions")
    ap.add_argument("--max-new", type=int, default=96)
    ap.add_argument("--model", default="Qwen/Qwen2.5-Coder-0.5B-Instruct")
    ap.add_argument("--mode", choices=["baseline", "fixed", "both"], default="fixed")
    ap.add_argument("--json", default=None)
    ap.add_argument("--no-mask-cache", action="store_true",
                    help="do not write the ~2GB mask-store pickle (disk-constrained)")
    args = ap.parse_args()

    import lark
    from lark.indenter import PythonIndenter
    from syncode import Syncode
    import syncode.grammar_mask.grammar_constrainer as gc
    from syncode_grammar import build_wrapper, make_grammar, WRAPPER_BASELINE, WRAPPER_FIXED

    if args.no_mask_cache:
        # MaskStore.init_mask_store ends with an UNCONDITIONAL
        #     pickle.dump(mask_store, open(fsm_path, 'wb'))
        # — `use_cache=False` only skips the READ, never the write. The store is
        # ~2 GB per grammar hash. On a disk-constrained box that write fails (or
        # fills the disk), so we no-op it and rebuild the store in RAM each run
        # (~25 min). Purely an I/O workaround: the mask store itself, and every
        # number measured from it, is identical.
        import syncode.mask_store.mask_store as ms
        ms.pickle.dump = lambda obj, f, *a, **k: None

    build_wrapper()

    # Independent acceptor: the CANONICAL grammar, not the decoder's copy.
    canonical = lark.Lark.open(CANONICAL, parser="lalr", postlex=PythonIndenter(),
                               start="file_input", maybe_placeholders=False)

    # ---- instrument the fallback ------------------------------------------
    counters = {"steps": 0, "skips": 0}
    orig = gc.GrammarConstrainer._parse_partial_output

    def counting(self, idx, partial_output, remainder_bytes, accepted_generation=True):
        res, skip = orig(self, idx, partial_output, remainder_bytes,
                         accepted_generation)
        counters["steps"] += 1
        if skip:
            counters["skips"] += 1
        return res, skip

    gc.GrammarConstrainer._parse_partial_output = counting

    modes = ["baseline", "fixed"] if args.mode == "both" else [args.mode]
    record = {"model": args.model, "n": args.n, "max_new": args.max_new,
              "modes": {}}

    for mode in modes:
        grammar = make_grammar(mode)
        print(f"\n=== mode={mode}  grammar={'python-named (indenter ON)' if mode=='fixed' else 'path-named (indenter OFF)'}")
        print("    building mask store (first run for this grammar hash: ~10 min)...")

        # Syncode() re-constructs Grammar from the string we pass, which would
        # throw away our name='python'. Pin the object we built instead.
        import syncode.infer as infer
        real_grammar_cls = infer.Grammar
        infer.Grammar = lambda _spec: grammar
        try:
            syn = Syncode(
                model=args.model,
                mode="grammar_strict",
                grammar=(WRAPPER_FIXED if mode == "fixed" else WRAPPER_BASELINE),
                parse_output_only=True,
                max_new_tokens=args.max_new,
                device="cpu",
                quantize=False,   # CPU: bitsandbytes quantisation is CUDA-only
                opp=False,        # strict masking, no opportunistic shortcut
                # indent=True feeds the MASK STORE's indentation->token map
                # only; it is NOT passed to create_parser, so on its own it
                # does not give the incremental parser an indenter. See
                # syncode_grammar.py. Kept on because the mask store's
                # whitespace handling is still wanted.
                indent=True,
                seed=20260714,
                do_sample=True,
                temperature=0.8,
            )
        finally:
            infer.Grammar = real_grammar_cls

        counters["steps"] = counters["skips"] = 0
        per_completion = []
        completions = []
        for i in range(args.n):
            prompt = PROMPTS[i % len(PROMPTS)]
            before = counters["skips"]
            try:
                out = syn.infer(prompt)
                if isinstance(out, list):
                    out = out[0] if out else ""
            except Exception as e:
                out = ""
                print(f"  [{i}] generation error: {str(e)[:80]}")
            fell_back = counters["skips"] - before
            per_completion.append(fell_back)
            completions.append(out)
            if (i + 1) % 10 == 0:
                print(f"  {i+1}/{args.n}  cumulative unconstrained steps: "
                      f"{counters['skips']}/{counters['steps']}")

        # ---- validity of the constrained output ---------------------------
        g_ok = c_ok = comp_ok = 0
        empties = 0
        for out in completions:
            src = out if out.endswith("\n") else out + "\n"
            if not src.strip():
                empties += 1
                continue
            if grammar_accepts(canonical, src):
                g_ok += 1
            ok, _ = pyths_run("check", src, ["--syntax-only", "--quiet"])
            if ok:
                c_ok += 1
            ok2, _ = pyths_run("compile", src, ["--quiet"])
            if ok2:
                comp_ok += 1

        nonempty = args.n - empties
        steps = counters["steps"] or 1
        res = {
            "steps": counters["steps"],
            "unconstrained_steps": counters["skips"],
            "step_fallback_rate": counters["skips"] / steps,
            "completions_with_fallback": sum(1 for x in per_completion if x > 0),
            "completion_fallback_rate":
                sum(1 for x in per_completion if x > 0) / args.n,
            "n": args.n,
            "empty": empties,
            "nonempty": nonempty,
            "grammar_valid": g_ok,
            "pyths_check_valid": c_ok,
            "compiles": comp_ok,
            "samples": completions[:5],
        }
        record["modes"][mode] = res

        print(f"\n  -- mode={mode} --")
        print(f"  decoding steps                 : {res['steps']}")
        print(f"  UNCONSTRAINED steps (mask lost): {res['unconstrained_steps']}")
        print(f"  STEP fallback rate             : {res['step_fallback_rate']*100:.2f}%")
        print(f"  completions w/ >=1 fallback    : "
              f"{res['completions_with_fallback']}/{args.n} "
              f"({res['completion_fallback_rate']*100:.1f}%)")
        print(f"  empty completions              : {empties}/{args.n}")
        print(f"  grammar-valid (canonical lark) : {g_ok}/{args.n}")
        print(f"  `pyths check`-valid            : {c_ok}/{args.n}")
        print(f"  compiles (`pyths compile`)     : {comp_ok}/{args.n}")

    if args.json:
        json.dump(record, open(args.json, "w", encoding="utf-8"), indent=2)
        print(f"\nwrote {args.json}")


if __name__ == "__main__":
    main()
