#!/usr/bin/env python3
"""scripts/grammar-fuzz.py — bidirectional differential fuzzer:
   grammar/pyths.lark  vs  the authoritative recursive-descent parser.

WHY
---
`grammar/pyths.lark` is a *best-effort* formal grammar; `crates/pyths_parser`
is AUTHORITATIVE (see grammar/README.md). scripts/test-grammar.py proves the
grammar accepts a fixed tracked corpus. That is a one-directional, one-corpus
check: it cannot see anything the grammar accepts that the parser rejects, and
it says nothing about code outside the tracked corpus.

Equivalence of two parsers is undecidable in general and we do not claim it.
What we *can* do is BOUND THE GAP EMPIRICALLY, in both directions:

  FALSE ACCEPT  — grammar accepts, authoritative parser rejects.
      L(grammar) \\ L(parser).  These are the dangerous ones for constrained
      decoding: the decoder would happily emit them, and `pyths` would then
      refuse to compile them. Measured by generating random derivations FROM
      the grammar (so grammar-acceptance holds by construction, re-verified by
      a self-parse) and running each through the authoritative parser.

  FALSE REJECT  — parser accepts, grammar rejects.
      L(parser) \\ L(grammar).  These make a constrained decoder needlessly
      block valid programs. Measured by taking parser-VALID corpora plus
      semantics-preserving MUTATIONS of them (which the parser re-validates)
      and asking whether the grammar accepts.

The comparison is PARSE-vs-PARSE. `pyths check` runs the parser AND the type
checker; we invoke `pyths check --syntax-only` so a type error is never
miscounted as a grammar false-accept. The Lark grammar is a syntactic acceptor
and can only ever be compared against a syntactic oracle.

HELD-OUT SPLIT
--------------
The grammar was developed against the corpora in scripts/test-grammar.py
(tracked .ps/.psc, the differential corpus, the clone pairs). Measuring on
those would be measuring on the training set. `--extra-corpus DIR` supplies an
external corpus (we use the reference-app repo: its .ps/.psc sources and the
generation-eval completions — never seen by the grammar gate). That corpus is
deterministically halved by SHA-1 of the relative path:

    split A ("dev")     — may be inspected; grammar fixes may be driven by it
    split B ("frozen")  — NEVER inspected while fixing; the reported held-out rate

Anything else is grading your own homework.

USAGE
-----
    pip install lark
    cargo build --release --workspace          # provides target/release/pyths

    # CI gate (hermetic: in-repo corpora + corpus-free generation fuzzing)
    python scripts/grammar-fuzz.py --generate 10000

    # full measurement, with the external held-out corpus
    python scripts/grammar-fuzz.py --generate 10000 --extra-corpus ../reference-app

    --generate N     number of random derivations for the false-accept run
    --seed S         RNG seed (default 20260714; runs are reproducible)
    --jobs J         parallel `pyths` subprocesses (default: cpu_count)
    --json PATH      write the full result record
    --fail-on-new    exit 1 if a discrepancy appears that is not in the
                     triaged KNOWN_DIVERGENCES ledger below (this is the CI
                     contract: the measured gap may shrink, never silently grow)
"""

import argparse
import concurrent.futures
import hashlib
import json
import os
import random
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXE = ".exe" if sys.platform == "win32" else ""
PYTHS = os.environ.get("PYTHS_BIN", os.path.join(ROOT, "target", "release", f"pyths{EXE}"))

# ---------------------------------------------------------------------------
# Triaged divergence ledger — the paper trail.
#
# Each entry is a class of string the grammar accepts but the authoritative
# parser rejects, that we have DECIDED to leave in place, with the reason.
# A divergence not matching any signature here fails --fail-on-new.
#
# Rule: a divergence is only allowed to live here if tightening the grammar
# would cost more than it buys (LALR conflicts) AND the over-acceptance is
# not reachable from realistic code. Everything else gets FIXED in the
# grammar. See the "triage" section of the run report.
# ---------------------------------------------------------------------------
#
# Baseline run (seed 20260714, --generate 3000): false-accept 70.700% -> 0.167%.
# Eleven divergence classes were found and CLOSED in grammar/pyths.lark; three
# were found and closed in the authoritative parser (they were parser bugs).
# The list below is what is LEFT, and why.
#
# Closed in the grammar (each has a `// MEASURED:` note at the rule):
#   `match` as a general identifier (36.4% alone) | float/f-string literal
#   patterns | bare & parenthesised sequence patterns | `()` pattern |
#   `{**rest}` mapping pattern | keyword sub-patterns in class patterns |
#   dotted class-pattern heads | loop targets accepting arbitrary expressions
#   (`for o.x in p`, `for -x in p`, nested `* *`) | `with ... as <expr>` |
#   class bases accepting kwargs/`**` | slices inside subscript tuples |
#   trailing bare `*` in a parameter list | non-NAME keyword-argument LHS |
#   trailing comma before `:` / an augassign op | `lambda` in a comprehension
#   `if` | f-string side-effect import paths.
#
# Closed in the PARSER (crates/pyths_parser) — these were parser bugs, not
# grammar bugs. Root cause: SEMICOLON was missing from three statement-
# terminator sets, so constructs that are legal before a NEWLINE became
# errors before a `;` inside a one-line suite:
#   `if x: return;`  `if x: raise;`  `if x: a,;`  `if x: import "a.css";`
#
KNOWN_DIVERGENCES = {
    # The residual. `yield` is an `atom`, so `atom_expr` may hang a postfix
    # trailer off a bare `yield` and `yield` may appear in tuple / `del` /
    # one-line-suite positions the parser refuses.
    #
    # NOT fixed, on measurement: hoisting `yield_expr` out of `atom` up to
    # `test` (the obvious fix) makes `yield` derivable in EVERY test position
    # and drives the false-accept rate from 2.6% to 64.3%. A correct fix needs
    # a yield-aware expression level, which LALR(1) cannot express here without
    # a conflict. The over-acceptance is unreachable from realistic code: it
    # requires a bare `yield` in an operand position.
    "Unexpected token: ?.": "yield-trailer: postfix trailer on a bare `yield`",
    "Unexpected token: ,": "yield-in-tuple: bare `yield` as a tuple element",
    "Unexpected token: ;": "yield-in-inline-suite: bare `yield` before `;`",
    "Unexpected token: .": "yield-trailer: `.attr` on a bare `yield`",
    "Unexpected token: ]": "yield-in-subscript: bare `yield` inside `[...]`",
    "Unexpected token: =": "yield-as-target: bare `yield` on an assignment LHS",
    "Expected at least one statement in block":
        "yield-as-header: bare `yield` in a compound-statement header",
    "Unexpected token: DEDENT": "yield-in-match-subject: bare `yield` after `match`",
    "Unexpected token: as": "yield-in-with-item: bare `yield` before `as`",
    # match-subject soft-keyword disambiguation (parser.rs looks_like_match_stmt).
    # `match ():` / `match []:` / `match {}:` — a bracketed/parenthesised
    # LITERAL as the subject — are DELIBERATELY parsed as name-usage of `match`
    # (`match(...)` call / `match[...]` subscript), because `match` is a soft
    # keyword and `match[i]` on a variable named `match` is the common case.
    # A parenthesised subject needs no parens (`match x:`), so nothing correct
    # is lost. The grammar accepts the CPython form; the parser's deliberate
    # trade-off rejects it. Same root cause surfaces as several token errors
    # depending on the literal (`]` for a list, NEWLINE for `()`/`{}`).
    "Unexpected token: NEWLINE": "match-literal-subject: `match ():`/`match {}:` — soft-keyword name-usage disambiguation (deliberate)",
    # keyword-as-atom over-acceptance: a hard keyword (`except`, ...) rendered
    # in an operand/expression position (`if except: pass`). The grammar's NAME
    # atom is broader than the lexer's keyword set here; unreachable from real
    # code (a keyword can never be a bare identifier), same class as the yield
    # residuals above — a correct fix needs keyword-aware atom levels LALR(1)
    # cannot express without conflicts.
    "Unexpected token: except": "keyword-as-atom: a hard keyword in an operand position (unreachable from real code)",
    # one-line-suite statement-terminator over-acceptance: the grammar accepts
    # a trailing construct after a simple statement in an inline suite that the
    # parser terminates at the newline/`;`/block-end. Lexeme collapsed to X
    # above so all variants share one bucket. Unreachable from real code.
    "Unexpected token after statement: X; a statement ends at a              newline, a `;` (inside a one-line suit":
        "inline-suite-terminator: trailing construct after a simple statement in a one-line suite",
    # CONTEXT-SENSITIVE post-parse checks added by #424 (B18/B12). CPython's
    # own grammar has the identical gap: `return` at module level PARSES and
    # the compiler raises "'return' outside function" afterwards. Encoding
    # these in the CFG would require stratifying the entire statement
    # hierarchy by {in-function} x {in-loop} context — a combinatorial
    # explosion for zero constrained-decoding value (a decoder emitting
    # `return` at top level is stopped by the compiler, same as CPython).
    "'return' outside function":
        "flow-context (#424 B18): `return` outside a function body — context-sensitive, not CFG-expressible",
    "'break' outside loop":
        "flow-context (#424 B18): `break` outside a loop — context-sensitive, not CFG-expressible",
    "'continue' outside loop":
        "flow-context (#424 B18): `continue` outside a loop — context-sensitive, not CFG-expressible",
    # SEMANTIC name-uniqueness check added by #424 (B12): `def f(a, a)`.
    # Identifier equality across parameters is context-sensitive (a CFG
    # cannot compare two NAME lexemes); name collapsed to 'X' in classify().
    "duplicate parameter 'X' in function definition":
        "dup-param (#424 B12): duplicate parameter name — semantic, not CFG-expressible",
}

# ---------------------------------------------------------------------------
# Grammar loading + random derivation
# ---------------------------------------------------------------------------

NEWLINE_MARK = "\x00NL"
INDENT_MARK = "\x00IN"
DEDENT_MARK = "\x00DE"

# Identifier pool: deliberately keyword-free, so a generated NAME can never
# re-lex as a keyword and silently change the derivation.
NAMES = ["a", "b", "c", "x", "y", "n", "foo", "bar", "baz", "data",
         "value", "result", "item", "self", "obj", "fn", "acc", "tmp"]


def load_parser(path="grammar/pyths.lark"):
    import lark
    from lark.indenter import PythonIndenter
    return lark.Lark.open(
        os.path.join(ROOT, path),
        parser="lalr",
        postlex=PythonIndenter(),
        start="file_input",
        maybe_placeholders=False,
    )


class Generator:
    """Uniform-ish random derivation from the LALR rule set.

    Derives over `parser.rules` — the post-EBNF-expansion rules the parse
    table is actually built from — so every derivation is in L(grammar) by
    construction. We re-parse each rendered string anyway (`self_check`):
    rendering a token sequence back to text can in principle perturb it
    (adjacency), and a string that fails the self-parse is a GENERATOR
    artifact, not a finding. Those are excluded from the denominator and
    reported separately, so the false-accept rate is never inflated by our
    own rendering bugs.
    """

    def __init__(self, parser, rng, max_depth=18, max_tokens=220):
        self.rng = rng
        self.max_depth = max_depth
        self.max_tokens = max_tokens
        self.rules_by_origin = {}
        for r in parser.rules:
            self.rules_by_origin.setdefault(r.origin.name, []).append(r)
        self.term_pattern = {}
        for t in parser.terminals:
            p = t.pattern
            self.term_pattern[t.name] = (
                ("str", p.value) if p.type == "str" else ("re", p.value)
            )
        self._min_depth_cache = {}
        self._compute_min_depths()

    # -- termination control ------------------------------------------------
    def _compute_min_depths(self):
        """Fixpoint: cheapest derivation depth for each nonterminal.

        Without this, random derivation on a recursive grammar diverges: it
        picks `expr -> expr '+' expr` forever. Past max_depth we restrict the
        choice to rules whose minimal completion is cheapest, which guarantees
        termination.
        """
        d = {name: math_inf() for name in self.rules_by_origin}
        changed = True
        while changed:
            changed = False
            for name, rules in self.rules_by_origin.items():
                best = min(
                    (self._rule_cost(r, d) for r in rules), default=math_inf()
                )
                if best < d[name]:
                    d[name] = best
                    changed = True
        self._min_depth_cache = d

    def _rule_cost(self, rule, d):
        cost = 0
        for s in rule.expansion:
            if s.is_term:
                continue
            c = d.get(s.name, math_inf())
            if c == math_inf():
                return math_inf()
            cost = max(cost, c)
        return cost + 1

    # -- terminals ----------------------------------------------------------
    def emit_terminal(self, name):
        if name == "_NEWLINE":
            return [NEWLINE_MARK]
        if name == "_INDENT":
            return [INDENT_MARK]
        if name == "_DEDENT":
            return [DEDENT_MARK]
        kind, val = self.term_pattern.get(name, ("str", ""))
        if kind == "str":
            return [val]
        # The handful of regex terminals in pyths.lark get explicit, auditable
        # samplers. A general regex sampler would be more code and less honest
        # (it would drift from what the terminal actually admits).
        r = self.rng
        if name == "NAME":
            return [r.choice(NAMES)]
        if name == "DEC_NUMBER":
            return [r.choice(["0", "1", "7", "42", "1_000", "007"])]
        if name == "FLOAT_NUMBER":
            return [r.choice(["1.5", "0.0", "3.25e4", "2e10", "1_0.5"])]
        if name == "STRING":
            return [r.choice(['"s"', "'t'", 'f"v{x}"', 'r"\\d+"', '""'])]
        if name == "LONG_STRING":
            return [r.choice(['"""doc"""', "'''d'''"])]
        if name == "COMMENT":
            return ["# c"]
        return [""]

    # -- derivation ---------------------------------------------------------
    def derive(self, start="file_input"):
        self.count = 0
        self.overflow = False
        out = []
        self._derive(start, 0, out)
        return out

    def _derive(self, name, depth, out):
        rules = self.rules_by_origin.get(name)
        if rules is None:
            out.extend(self.emit_terminal(name))
            self.count += 1
            return
        if depth >= self.max_depth or self.count >= self.max_tokens:
            self.overflow = True
            best = min(self._rule_cost(r, self._min_depth_cache) for r in rules)
            rules = [r for r in rules
                     if self._rule_cost(r, self._min_depth_cache) == best]
        rule = self.rng.choice(rules)
        for s in rule.expansion:
            if s.is_term:
                out.extend(self.emit_terminal(s.name))
                self.count += 1
            else:
                self._derive(s.name, depth + 1, out)


def math_inf():
    return float("inf")


# -- rendering --------------------------------------------------------------
NO_SPACE_BEFORE = {")", "]", "}", ",", ":", ".", "?.", ";"}
NO_SPACE_AFTER = {"(", "[", "{", ".", "?.", "~"}


def render(tokens, indent="    "):
    """Token list -> source text, materialising INDENT/DEDENT as whitespace.

    The Lark pipeline synthesises _INDENT/_DEDENT in the PythonIndenter
    *postlexer*, from the whitespace that follows a _NEWLINE. So to render a
    derivation containing those markers we must run the indenter backwards:
    a NEWLINE's indentation is the level AFTER applying every INDENT/DEDENT
    marker that immediately follows it.
    """
    parts = []
    level = 0
    i = 0
    n = len(tokens)
    line_start = True
    while i < n:
        t = tokens[i]
        if t == NEWLINE_MARK:
            j = i + 1
            while j < n and tokens[j] in (INDENT_MARK, DEDENT_MARK):
                level += 1 if tokens[j] == INDENT_MARK else -1
                j += 1
            if level < 0:
                level = 0
            parts.append("\n" + indent * level)
            line_start = True
            i = j
            continue
        if t in (INDENT_MARK, DEDENT_MARK):
            level += 1 if t == INDENT_MARK else -1
            i += 1
            continue
        if not t:
            i += 1
            continue
        if line_start:
            parts.append(t)
            line_start = False
        else:
            prev = parts[-1] if parts else ""
            prev_tail = prev.rstrip()
            sep = ""
            if not (t in NO_SPACE_BEFORE
                    or prev_tail in NO_SPACE_AFTER
                    or prev.endswith("\n")
                    or prev_tail == ""):
                sep = " "
            parts.append(sep + t)
        i += 1
    src = "".join(parts)
    return src.strip("\n") + "\n"


# ---------------------------------------------------------------------------
# The authoritative oracle
# ---------------------------------------------------------------------------

def pyths_syntax_ok(args):
    """(source, tag) -> (tag, ok, first_error_line). PARSE-only verdict."""
    src, tag = args
    fd, path = tempfile.mkstemp(suffix=".ps", text=True)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(src)
        r = subprocess.run(
            [PYTHS, "check", "--syntax-only", "--quiet", path],
            capture_output=True, encoding="utf-8", errors="replace",
        )
        if r.returncode == 0:
            return tag, True, None
        err = strip_ansi((r.stderr or r.stdout)).strip()
        # A file can carry several diagnostics, and after a CONTEXT-CHECK
        # failure (#424 B18/B12: 'continue' outside loop, duplicate
        # parameter, ...) error RECOVERY abandons the enclosing compound
        # statement and emits cascade noise ("Unexpected token: finally" for
        # the orphaned clause) — sometimes printed BEFORE the root cause.
        # Classify by the root-cause semantic message when one is present
        # anywhere in the output, else by the first Error: line as before.
        _semantic = ("' outside function", "' outside loop",
                     "duplicate parameter ")
        msg = ""
        for line in err.splitlines():
            line = line.strip()
            if (line.startswith("Error:") or "error" in line.lower()):
                if not msg:
                    msg = line
                if any(s in line for s in _semantic):
                    msg = line
                    break
        return tag, False, (msg or err.splitlines()[0] if err else "?")[:180]
    finally:
        try:
            os.unlink(path)
        except OSError:
            pass


ANSI = re.compile(r"\x1b\[[0-9;]*m")


def strip_ansi(s):
    return ANSI.sub("", s)


def parallel_oracle(items, jobs, label):
    """items: list[(src, tag)] -> dict[tag] = (ok, err)"""
    out = {}
    total = len(items)
    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as ex:
        for k, (tag, ok, err) in enumerate(ex.map(pyths_syntax_ok, items)):
            out[tag] = (ok, err)
            if total > 200 and k % max(1, total // 20) == 0:
                pct = 100 * k // total
                print(f"\r  {label}: {pct:3d}% ({k}/{total})", end="", file=sys.stderr)
    if total > 200:
        print(f"\r  {label}: 100% ({total}/{total})", file=sys.stderr)
    return out


# ---------------------------------------------------------------------------
# Direction A — FALSE ACCEPTS (generate from grammar, test against parser)
# ---------------------------------------------------------------------------

def classify(src, err):
    """Signature = the authoritative parser's own error message, normalised.

    Deliberately NOT a hand-rolled regex taxonomy: the parser's message is the
    parser's own account of why it disagrees, so grouping by it cannot smuggle
    in our assumptions about what the divergence classes are. Spans/quoted
    lexemes are stripped so the same defect does not fan out into many buckets.
    """
    e = strip_ansi(err or "").strip()
    e = re.sub(r"^Error:\s*", "", e)
    e = re.sub(r"\d+", "N", e)
    # The "after statement" diagnostic embeds the offending lexeme mid-message
    # (`... statement: result; a statement ends ...`), which would otherwise
    # fan one defect class into a bucket per lexeme. Collapse that lexeme to X
    # (the per-token "Unexpected token: X" family has no "; a statement ends"
    # suffix, so it is unaffected).
    e = re.sub(r"(Unexpected token after statement: )[^;]+;", r"\1X;", e)
    # B12 (#424): the duplicate-parameter diagnostic embeds the parameter
    # name (`duplicate parameter 'match' ...`) — collapse it to X so one
    # semantic defect class does not fan out into a bucket per identifier.
    e = re.sub(r"(duplicate parameter )'[^']*'", r"\1'X'", e)
    return e[:110] or "?"


# -- delta-debugging shrinker ----------------------------------------------
# A raw derivation is a 200-token monster. A divergence is only actionable as a
# MINIMAL reproducer, so we shrink each representative: greedily delete tokens
# while the string stays (grammar-accepted AND parser-rejected with the SAME
# error). Preserving the error message is what keeps the shrink honest — it
# guarantees we minimise the divergence we found, not some other one we
# stumbled into on the way down.

TOKENIZE = re.compile(
    r'"""[\s\S]*?"""|\'\'\'[\s\S]*?\'\'\'|"(?:[^"\\\n]|\\.)*"|\'(?:[^\'\\\n]|\\.)*\''
    r"|\?\.|\*\*|//|<<|>>|\|>|\?\?|:=|[A-Za-z_][A-Za-z0-9_]*|\d[\d_]*\.?\d*"
    r"|\.\.\.|\n[ \t]*|[^\s]"
)


def _still_diverges(src, parser, want_err):
    try:
        parser.parse(src if src.endswith("\n") else src + "\n")
    except Exception:
        return False  # grammar no longer accepts -> not a false accept
    _tag, ok, err = pyths_syntax_ok((src, 0))
    return (not ok) and classify(src, err) == want_err


def shrink(src, parser, want_err, budget=1500):
    """Greedy token-level delta debugging. Returns the smallest string found.

    Deletion works on token SPANS of the current text, and a candidate is
    formed by slicing the original string around the span — never by
    re-joining the token list. Re-joining silently drops the whitespace
    between tokens, which both mangles indentation (the indenter is
    whitespace-sensitive) and can fuse two tokens into one (`global value` ->
    `globalvalue`, a different program that happens to still parse). Slicing
    preserves every byte we did not explicitly delete.
    """
    best = src
    calls = 0
    changed = True
    while changed and calls < budget:
        changed = False
        for span in (32, 16, 8, 4, 2, 1):
            i = 0
            while calls < budget:
                spans = [m.span() for m in TOKENIZE.finditer(best)]
                if i + span > len(spans):
                    break
                cand = best[:spans[i][0]] + best[spans[i + span - 1][1]:]
                calls += 1
                if cand.strip() and _still_diverges(cand, parser, want_err):
                    best = cand
                    changed = True
                else:
                    i += 1
    return best.strip("\n") + "\n"


def collect_small_seeds(parser, seed, jobs, want, n=4000):
    """Generate tiny derivations; return {signature: shortest example}."""
    rng = random.Random(seed + 7)
    gen = Generator(parser, rng, max_depth=9, max_tokens=28)
    cands, seen = [], set()
    for _ in range(n * 3):
        if len(cands) >= n:
            break
        try:
            src = render(gen.derive())
        except RecursionError:
            continue
        if not src.strip() or src in seen:
            continue
        try:
            parser.parse(src)
        except Exception:
            continue
        seen.add(src)
        cands.append(src)
    print(f"  triage pass: {len(cands)} short grammar-valid strings",
          file=sys.stderr)
    verdicts = parallel_oracle([(s, i) for i, s in enumerate(cands)], jobs,
                               "triage oracle")
    best = {}
    for i, src in enumerate(cands):
        ok, err = verdicts[i]
        if ok:
            continue
        sig = classify(src, err)
        if sig not in best or len(src) < len(best[sig]):
            best[sig] = src
    return best


def run_false_accepts(parser, n, seed, jobs, shrink_reps=True):
    rng = random.Random(seed)
    gen = Generator(parser, rng)
    print(f"\n[A] FALSE ACCEPTS — generating {n} random derivations from grammar/pyths.lark")

    cands, artifacts = [], 0
    seen = set()
    attempts = 0
    # Generation is cheap; the self-parse is the cost. Keep drawing until we
    # have n *self-verified distinct* strings, or we run out of patience.
    while len(cands) < n and attempts < n * 6:
        attempts += 1
        try:
            toks = gen.derive()
            src = render(toks)
        except RecursionError:
            artifacts += 1
            continue
        if not src.strip():
            continue
        if src in seen:
            continue
        # Self-check: is the RENDERED text still in L(grammar)? If not, our
        # renderer perturbed the derivation — a generator artifact, excluded.
        try:
            parser.parse(src if src.endswith("\n") else src + "\n")
        except Exception:
            artifacts += 1
            continue
        seen.add(src)
        cands.append(src)
        if len(cands) % 1000 == 0:
            print(f"\r  generated: {len(cands)}/{n}", end="", file=sys.stderr)
    print(f"\r  generated: {len(cands)} grammar-valid strings "
          f"({artifacts} render artifacts discarded, {attempts} draws)", file=sys.stderr)

    verdicts = parallel_oracle([(s, i) for i, s in enumerate(cands)], jobs, "pyths check")
    false_accepts = []
    for i, src in enumerate(cands):
        ok, err = verdicts[i]
        if not ok:
            false_accepts.append({"src": src, "err": err, "sig": classify(src, err)})

    by_sig = {}
    for fa in false_accepts:
        by_sig.setdefault(fa["sig"], []).append(fa)

    # -- triage pass --------------------------------------------------------
    # Shrinking a 300-token derivation is slow and often stalls: such a string
    # carries SEVERAL divergences at once, and deleting tokens flips which one
    # the parser reports first, which the same-signature guard (correctly)
    # refuses. So we run a second, cheap generation pass with a tight token
    # budget purely to source SHORT seeds. Small derivations exhibit one defect
    # at a time and shrink to genuinely minimal reproducers in a few hundred
    # oracle calls.
    minimal = {}
    if shrink_reps and by_sig:
        small = collect_small_seeds(parser, seed, jobs, want=set(by_sig))
        print(f"  shrinking {len(by_sig)} divergence class(es)...", file=sys.stderr)
        for sig, members in sorted(by_sig.items()):
            seed_src = small.get(sig) or min((m["src"] for m in members), key=len)
            try:
                minimal[sig] = shrink(seed_src, parser, sig, budget=1500)
            except Exception as e:
                minimal[sig] = f"<shrink failed: {e}>"

    return {
        "generated": len(cands),
        "render_artifacts": artifacts,
        "false_accepts": len(false_accepts),
        "rate": len(false_accepts) / len(cands) if cands else 0.0,
        "by_signature": {k: len(v) for k, v in sorted(by_sig.items())},
        "minimal_reproducers": minimal,
    }


# ---------------------------------------------------------------------------
# Direction B — FALSE REJECTS (parser-valid corpora + mutations vs grammar)
# ---------------------------------------------------------------------------

FENCE = re.compile(r"```(?:python|ps|psc|pythscribe|pythscribe)?\s*\n(.*?)```",
                   re.S | re.I)


def harvest_corpus(extra_dirs):
    """Return [(tag, split, source_text)].

    split: 'dev'    = in-repo, i.e. what the grammar was developed against
           'held-A' = external, may drive fixes
           'held-B' = external, FROZEN — never inspected while fixing
    """
    items = []

    def add(tag, split, src):
        if src and src.strip():
            items.append((tag, split, src))

    # -- in-repo (dev / seen) ------------------------------------------------
    for f in git_ls(ROOT, "*.ps"):
        if any(k in f for k in ("error_", "b029_")):
            continue  # negative fixtures: pyths check rejects them too
        add(f"repo:{f}", "dev", read(os.path.join(ROOT, f)))

    corpus_path = os.path.join(ROOT, "tests", "differential", "cpython_corpus.json")
    if os.path.exists(corpus_path):
        for e in json.load(open(corpus_path, encoding="utf-8")):
            setup = e.get("_setup", "")
            add(f"diff:{e['id']}", "dev",
                (setup + "\n" if setup else "") + e["expr"] + "\n")

    # -- external (held-out) -------------------------------------------------
    for d in extra_dirs:
        d = os.path.abspath(d)
        if not os.path.isdir(d):
            print(f"  warning: --extra-corpus {d} not a directory; skipped",
                  file=sys.stderr)
            continue
        for f in git_ls(d, "*.ps"):
            rel = f"reference-app:{f}"
            add(rel, held_split(rel), read(os.path.join(d, f)))
        # generation-eval / rerun completions: .ps inside markdown fences
        for root, _dirs, files in os.walk(d):
            if "geneval" not in root and "gen_eval" not in root:
                continue
            for fn in files:
                if not fn.endswith(".md"):
                    continue
                if "_psc" in fn:
                    continue  # compressed surface; psc.lark's job, not pyths.lark
                txt = read(os.path.join(root, fn))
                m = FENCE.search(txt or "")
                if not m:
                    continue
                rel = "gen:" + os.path.relpath(os.path.join(root, fn), d).replace("\\", "/")
                add(rel, held_split(rel), m.group(1))
    return items


def held_split(tag):
    """Deterministic, path-hashed 50/50 split. Stable across runs and machines."""
    h = hashlib.sha1(tag.encode("utf-8")).hexdigest()
    return "held-A" if int(h[:8], 16) % 2 == 0 else "held-B"


def read(p):
    try:
        return open(p, encoding="utf-8").read()
    except (OSError, UnicodeDecodeError):
        return None


def git_ls(root, pattern):
    r = subprocess.run(["git", "ls-files", pattern], cwd=root,
                       capture_output=True, encoding="utf-8", errors="replace")
    if r.returncode != 0:
        return []
    return [x for x in r.stdout.splitlines() if x.strip()]


# -- semantics-preserving mutations -----------------------------------------
# Each must keep the program VALID. We re-run the authoritative parser on every
# mutant and drop any the parser rejects, so a buggy mutation can never be
# scored as a grammar false-reject.

KEYWORDS = {
    "False", "None", "True", "and", "as", "assert", "async", "await", "break",
    "class", "continue", "def", "del", "elif", "else", "except", "finally",
    "for", "from", "global", "if", "import", "in", "is", "lambda", "match",
    "case", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
}
IDENT = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]*\b")
# The f/r/R PREFIX IS PART OF THE LITERAL. Matching only from the quote leaves
# the prefix exposed as an "identifier" in the surrounding chunk, and mut_rename
# then happily rewrites `f"Hi {name}"` into `z0_f"Hi {name}"` — which is a
# different program (a NAME adjacent to a plain string, not an f-string). That
# is a mutation bug, and it manufactured phantom false-rejects. Consume the
# prefix with the string.
STR_OR_COMMENT = re.compile(
    r'((?:[frR]|rf|fr|rF|Rf)?"""[\s\S]*?"""|(?:[frR])?\'\'\'[\s\S]*?\'\'\''
    r'|(?:[frR])?"(?:[^"\\\n]|\\.)*"'
    r"|(?:[frR])?'(?:[^'\\\n]|\\.)*'|#[^\n]*)"
)


def _map_outside_strings(src, fn):
    """Apply fn to the parts of src that are not strings/comments."""
    out, last = [], 0
    for m in STR_OR_COMMENT.finditer(src):
        out.append(fn(src[last:m.start()]))
        out.append(m.group(0))
        last = m.end()
    out.append(fn(src[last:]))
    return "".join(out)


def mut_rename(src, rng):
    """Rename local identifiers. Never touches keywords, attribute names
    (`.foo`), kwargs (`foo=`), or anything inside a string/comment — those
    would change meaning or (for attrs) collide with the JS-interop rule."""
    names = set()

    def collect(chunk):
        for m in IDENT.finditer(chunk):
            w = m.group(0)
            if w in KEYWORDS:
                continue
            s = m.start()
            before = chunk[:s].rstrip()
            if before.endswith(".") or before.endswith("?."):
                continue
            after = chunk[m.end():]
            if re.match(r"\s*=(?!=)", after):
                continue  # kwarg / assignment target on a call — leave it
            names.add(w)
        return chunk

    _map_outside_strings(src, collect)
    if not names:
        return None
    pick = sorted(names)
    ren = {w: f"z{i}_{w}" for i, w in enumerate(rng.sample(pick, min(4, len(pick))))}
    if not ren:
        return None

    def apply(chunk):
        res, last = [], 0
        for m in IDENT.finditer(chunk):
            w = m.group(0)
            if w in ren:
                before = chunk[:m.start()].rstrip()
                if before.endswith(".") or before.endswith("?."):
                    continue
                if re.match(r"\s*=(?!=)", chunk[m.end():]):
                    continue
                res.append(chunk[last:m.start()])
                res.append(ren[w])
                last = m.end()
        res.append(chunk[last:])
        return "".join(res)

    return _map_outside_strings(src, apply)


def mut_comments(src, rng):
    """Inject full-line comments and blank lines at correct indentation."""
    lines = src.split("\n")
    out = []
    for ln in lines:
        if rng.random() < 0.25 and ln.strip():
            ind = ln[:len(ln) - len(ln.lstrip())]
            out.append(f"{ind}# injected comment {rng.randint(0, 999)}")
        if rng.random() < 0.15:
            out.append("")
        out.append(ln)
    return "\n".join(out)


def mut_whitespace(src, rng):
    """Pad spaces around binary operators and after commas, outside strings."""
    def pad(chunk):
        chunk = re.sub(r"(?<![=!<>+\-*/%&|^~])([+\-*/%](?![*/=]))(?!=)",
                       r" \1 ", chunk)
        chunk = re.sub(r",(?!\s)", ", ", chunk)
        chunk = re.sub(r"[ \t]+\n", "\n", chunk)
        return chunk
    return _map_outside_strings(src, pad)


def mut_trailing_blank(src, rng):
    """Trailing newlines / a trailing comment with no newline — the classic
    off-by-one in indenter-based grammars."""
    choice = rng.randint(0, 3)
    if choice == 0:
        return src.rstrip("\n")               # no trailing newline at all
    if choice == 1:
        return src.rstrip("\n") + "\n\n\n"
    if choice == 2:
        return src.rstrip("\n") + "\n# trailing comment"
    return "# leading comment\n\n" + src


def mut_reindent(src, rng):
    """Re-indent with a different tab width (2 or 8 spaces)."""
    width = rng.choice([2, 8])
    lines = src.split("\n")
    out = []
    for ln in lines:
        stripped = ln.lstrip(" ")
        ind = len(ln) - len(stripped)
        if ind % 4 == 0 and ind > 0:
            out.append(" " * (ind // 4 * width) + stripped)
        else:
            out.append(ln)
    return "\n".join(out)


MUTATIONS = [
    ("rename", mut_rename),
    ("comments", mut_comments),
    ("whitespace", mut_whitespace),
    ("trailing", mut_trailing_blank),
    ("reindent", mut_reindent),
]


def run_false_rejects(parser, extra_dirs, seed, jobs):
    rng = random.Random(seed + 1)
    print("\n[B] FALSE REJECTS — parser-valid corpora + semantics-preserving mutations")
    corpus = harvest_corpus(extra_dirs)
    print(f"  harvested {len(corpus)} candidate sources")

    # 1. Establish ground truth: which sources does the AUTHORITATIVE parser
    #    accept? Only those are eligible — the grammar is not obliged to
    #    accept anything the parser itself rejects.
    verdicts = parallel_oracle([(s, t) for t, _sp, s in corpus], jobs,
                               "oracle: base")
    valid = [(t, sp, s) for t, sp, s in corpus if verdicts[t][0]]
    print(f"  {len(valid)}/{len(corpus)} are parser-valid (the eligible set)")

    # 2. Mutate. Every mutant is re-validated by the parser; mutants the parser
    #    rejects are discarded (a bad mutation must never become a "finding").
    mutants = []
    for tag, split, src in valid:
        for mname, fn in MUTATIONS:
            try:
                m = fn(src, rng)
            except Exception:
                continue
            if not m or not m.strip() or m == src:
                continue
            mutants.append((f"{tag}|{mname}", split, m))
    print(f"  {len(mutants)} mutants proposed ({len(MUTATIONS)} mutations x sources)")

    mverdicts = parallel_oracle([(s, t) for t, _sp, s in mutants], jobs,
                                "oracle: mutants")
    mut_valid = [(t, sp, s) for t, sp, s in mutants if mverdicts[t][0]]
    print(f"  {len(mut_valid)}/{len(mutants)} mutants remain parser-valid")

    # 3. The actual question: does the grammar accept every parser-valid string?
    population = valid + mut_valid
    results = {"dev": [0, 0], "held-A": [0, 0], "held-B": [0, 0]}  # [accepted, total]
    rejects = []
    for tag, split, src in population:
        results[split][1] += 1
        try:
            parser.parse(src if src.endswith("\n") else src + "\n")
            results[split][0] += 1
        except Exception as e:
            rejects.append({
                "tag": tag, "split": split,
                "err": str(e).split("\n")[0][:180],
                "src": src if len(src) < 800 else src[:800] + "\n...",
            })

    out = {"population": len(population),
           "base_sources": len(valid), "mutants": len(mut_valid),
           "splits": {}, "false_rejects": len(rejects), "rejects": rejects[:40]}
    for sp, (acc, tot) in results.items():
        if tot:
            out["splits"][sp] = {
                "accepted": acc, "total": tot,
                "false_rejects": tot - acc,
                "rate": (tot - acc) / tot,
            }
    return out


# ---------------------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--generate", type=int, default=10000)
    ap.add_argument("--seed", type=int, default=20260714)
    ap.add_argument("--jobs", type=int, default=max(4, (os.cpu_count() or 4)))
    ap.add_argument("--extra-corpus", action="append", default=[])
    ap.add_argument("--json", default=None)
    ap.add_argument("--fail-on-new", action="store_true")
    ap.add_argument("--skip-false-accept", action="store_true")
    ap.add_argument("--no-shrink", action="store_true")
    ap.add_argument("--skip-false-reject", action="store_true")
    args = ap.parse_args()

    if not os.path.exists(PYTHS):
        print(f"FATAL: {PYTHS} missing — cargo build --release --workspace",
              file=sys.stderr)
        sys.exit(2)
    try:
        import lark  # noqa: F401
    except ImportError:
        print("FATAL: lark not installed — pip install lark", file=sys.stderr)
        sys.exit(2)

    parser = load_parser()
    rec = {"seed": args.seed, "jobs": args.jobs,
           "extra_corpus": [os.path.abspath(d) for d in args.extra_corpus]}

    if not args.skip_false_accept:
        rec["false_accept"] = run_false_accepts(parser, args.generate,
                                                args.seed, args.jobs,
                                                shrink_reps=not args.no_shrink)
    if not args.skip_false_reject:
        rec["false_reject"] = run_false_rejects(parser, args.extra_corpus,
                                                args.seed, args.jobs)

    # ---- report -----------------------------------------------------------
    print("\n" + "=" * 72)
    print("grammar-fuzz — differential summary  (grammar/pyths.lark vs pyths_parser)")
    print("=" * 72)
    fa = rec.get("false_accept")
    if fa:
        print(f"\nFALSE ACCEPTS (grammar accepts, authoritative parser rejects)")
        print(f"  generated (grammar-valid) : {fa['generated']}")
        print(f"  render artifacts discarded: {fa['render_artifacts']}")
        print(f"  false accepts             : {fa['false_accepts']}")
        print(f"  FALSE-ACCEPT RATE         : {fa['rate']*100:.3f}%")
        if fa["by_signature"]:
            print("\n  divergence classes (signature = the parser's own error):")
            for k, v in sorted(fa["by_signature"].items(), key=lambda x: -x[1]):
                print(f"    [{v:5d}  {v/fa['generated']*100:6.2f}%]  {k}")
                repro = fa.get("minimal_reproducers", {}).get(k)
                if repro:
                    for ln in repro.strip("\n").split("\n"):
                        print(f"             | {ln}")
    fr = rec.get("false_reject")
    if fr:
        print(f"\nFALSE REJECTS (parser accepts, grammar rejects)")
        print(f"  population (base + valid mutants): {fr['population']}"
              f"  [{fr['base_sources']} base, {fr['mutants']} mutants]")
        for sp in ("dev", "held-A", "held-B"):
            s = fr["splits"].get(sp)
            if not s:
                continue
            note = {"dev": "in-repo (grammar was developed on this)",
                    "held-A": "held-out, fixes may be driven by it",
                    "held-B": "held-out, FROZEN"}[sp]
            print(f"  {sp:7s} {s['false_rejects']:5d}/{s['total']:<6d} "
                  f"= {s['rate']*100:7.3f}%   {note}")
        if fr["rejects"]:
            print(f"\n  first {min(6, len(fr['rejects']))} false rejects:")
            for r in fr["rejects"][:6]:
                print(f"    [{r['split']}] {r['tag']}\n        {r['err']}")

    if args.json:
        with open(args.json, "w", encoding="utf-8") as fh:
            json.dump(rec, fh, indent=2)
        print(f"\nwrote {args.json}")

    if args.fail_on_new:
        new = []
        if fa:
            for sig, cnt in fa["by_signature"].items():
                allowed = KNOWN_DIVERGENCES.get(sig)
                if allowed is None:
                    new.append(f"untriaged false-accept signature '{sig}' ({cnt})")
        if fr and fr["false_rejects"] > 0:
            new.append(f"{fr['false_rejects']} false reject(s) — the grammar must "
                       f"accept every parser-valid program")
        if new:
            print("\nFAIL (--fail-on-new):", file=sys.stderr)
            for x in new:
                print(f"  {x}", file=sys.stderr)
            sys.exit(1)
        print("\nOK — no untriaged divergence")


if __name__ == "__main__":
    main()
