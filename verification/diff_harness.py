#!/usr/bin/env python3
"""Model-vs-implementation differential for the Lean executable tier models.

Five arms — one per CONCRETE tier (Tier C, the PSX tag-DSL, has no arm: it
is not instantiated in Lean; see verification/README.md for why):
  --tier dict    Dict tier, `$NAME` domain dictionary   (strings.rs)
  --tier kwargs  Tier B,    kwarg-position aliases      (kwargs.rs)
  --tier hooks   hooks tier, hook-call shorthand        (hooks.rs)
  --tier idioms  Tier E,    `%NAME` idiom fragments     (idioms.rs)
  --tier tiera   Tier A,    presets + decorator aliases (presets/decorators.rs)
  --tier all     all five (default)

Compares the Lean executable model (`lake exe expanddiff` — the
zone-aware expander in PythExpandVerify.lean) byte-for-byte against the
REAL compiler (`pyths expand`) on a deterministic generated corpus.
This is the §7.8 binding move (after the Move borrow-checker project):
the proofs quantify over a model; only a differential against the
shipping implementation shows the model IS the implementation.

Corpus discipline: inputs are restricted to constructs where tiers
E/C/A/B/hooks are identity (no idiom sigils, PSX shorthand, presets,
kwarg aliases, or hook shorthand), so `pyths expand` ≡ dictionary tier.
A divergence therefore means: the Lean model's classifier/lookup
semantics differ from strings.rs — exactly the drift this harness
exists to catch. Run from anywhere; uses a scratch dir without a
pyths.toml so the user dictionary is empty on the real side.

Usage: python verification/diff_harness.py [--tier dict|kwargs|all]
                                          [--pyths PATH] [--lean-exe PATH]
"""
from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

ALIASES = ["pad", "fs", "col", "bg", "c1", "c2", "p1", "p4", "brr", "mar",
           "gtc", "ff", "ff_k", "br1", "pct", "vh", "w6", "sbw", "ws", "ls"]
UNKNOWN = ["nope", "x9z", "pad_", "_pad", "padding2", "c99"]


def xorshift(seed: int):
    x = seed
    while True:
        x ^= (x >> 12) & 0xFFFFFFFFFFFFFFFF
        x = (x ^ (x << 25)) & 0xFFFFFFFFFFFFFFFF
        x ^= x >> 27
        yield (x * 0x2545F4914F6CDD1D) & 0xFFFFFFFFFFFFFFFF


def gen_cases() -> list[str]:
    rng = xorshift(0x5EED_2026_0710)
    pick = lambda xs: xs[next(rng) % len(xs)]
    cases: list[str] = []

    # 1. Every alias in plain code position.
    for a in ALIASES:
        cases.append(f"x = ${a}\n")
    # 2. Every alias protected in each zone kind.
    for a in ALIASES:
        cases.append(f'a = "${a} inside"\n')
        cases.append(f"b = '${a} inside'\n")
        cases.append(f"# comment ${a} here\n")
        cases.append(f'c = """multi\n${a}\nline"""\n')
    # 3. Unknown aliases (left verbatim, `$` emitted then ident re-scanned).
    for u in UNKNOWN:
        cases.append(f"y = ${u} + $pad\n")
    # 4. Escapes: escaped quote must NOT close the string.
    cases.append('e1 = "esc \\" $pad still-in-string" + $p1\n')
    cases.append("e2 = 'esc \\' $c1 still' + $c2\n")
    cases.append('e3 = "trailing backslash \\\\" + $pad\n')
    # 5. Adjacent / edge sigils.
    cases.append("z1 = $pad$fs\n")
    cases.append("z2 = $pad + $\n")
    cases.append("z3 = $ pad\n")
    cases.append("z4 = ($pad,$p1,$c1)\n")
    cases.append("z5$pad = 1\n")
    # 6. Triple-quote edge: quotes inside triple body; hash inside string.
    cases.append('t1 = """a " b "" c $pad"""\n')
    cases.append("t2 = '''it's $c1 fine'''\n")
    cases.append('h1 = "# not a comment $pad" + $p1\n')
    cases.append("h2 = $pad  # trailing comment $fs\n")
    # 7. Randomized composites (deterministic).
    zones = ["code", "dq", "sq", "comment", "triple"]
    for i in range(300):
        a1, a2 = pick(ALIASES), pick(ALIASES)
        z = pick(zones)
        if z == "code":
            cases.append(f"r{i} = ${a1} + ${a2}\n")
        elif z == "dq":
            cases.append(f'r{i} = "${a1}" + ${a2}\n')
        elif z == "sq":
            cases.append(f"r{i} = '${a1} ${a2}'\n")
        elif z == "comment":
            cases.append(f"r{i} = ${a1}  # ${a2}\n")
        else:
            cases.append(f'r{i} = """${a1}\n${a2}""" + ${a1}\n')
    return cases


# --- Tier B (kwargs.rs) -------------------------------------------------
# The SHIPPING alias table. Kept in lockstep with kwargs.rs by
# gen-kwarg-data.py --check (CI) and by the FNV manifest gate in
# crates/pyths_expand/tests/gates.rs, so this list cannot silently rot.
KWARG_ALIASES = ["st", "cn", "cl", "oc", "oh", "os", "oa", "ph", "dis"]
KWARG_NONALIASES = ["style", "stx", "c", "onclick", "foo", "x1", "_st", "St"]


def gen_kwarg_cases() -> list[str]:
    """Cases where tiers E/C/A/hooks/Dict are identity, so `pyths expand`
    reduces to Tier B alone: no `%` idiom sigils (the idiom map is empty
    without a pyths.toml), no `$NAME` dictionary sigils, no PSX tag-DSL
    shorthand, no presets/decorator aliases, no hook shorthand."""
    rng = xorshift(0x5EED_2026_0714)
    pick = lambda xs: xs[next(rng) % len(xs)]
    cases: list[str] = []

    # 1. Every alias, in argument position (the ONLY place it may fire).
    for a in KWARG_ALIASES:
        cases.append(f"foo({a}=1)\n")
        cases.append(f"foo(x, {a}=2)\n")
        cases.append(f"Widget(a, b, {a}=v)\n")
    # 2. Multi-line argument lists — at_arg_start persists across newlines.
    for a in KWARG_ALIASES:
        cases.append(f"foo(\n  x,\n  {a}=1,\n)\n")
    # 3. Statement position: NOT an argument, must not fire.
    for a in KWARG_ALIASES:
        cases.append(f"{a} = 1\n")
    # 4. `==` is a comparison, not a kwarg.
    for a in KWARG_ALIASES:
        cases.append(f"foo({a}==1)\n")
        cases.append(f"foo({a} == 1)\n")
    # 5. Every protected zone kind must pass the alias through verbatim.
    for a in KWARG_ALIASES:
        cases.append(f'foo(s="{a}=1")\n')
        cases.append(f"foo(s='{a}=1')\n")
        cases.append(f"# foo({a}=1)\n")
        cases.append(f'foo(s="""{a}=1\nstill {a}=2""")\n')
    # 6. Near-miss identifiers: not in the table => untouched.
    for u in KWARG_NONALIASES:
        cases.append(f"foo({u}=1)\n")
    # 7. Alias with no `=` at all; alias as a positional value.
    for a in KWARG_ALIASES:
        cases.append(f"foo({a})\n")
        cases.append(f"foo(x, {a})\n")
    # 8. Comment clears at_arg_start (kwargs.rs sets it false at `#`).
    cases.append("foo(  # note\n  cn=1)\n")
    # 9. Escapes inside strings must not close the zone early.
    cases.append('foo(s="esc \\" cn=1 still-in-string", oc=h)\n')
    cases.append("foo(s='esc \\' st=1 still', dis=True)\n")
    # 10. Nesting, adjacency, dict/set literals (`:` not `=`).
    cases.append("foo(bar(cn=1), oc=g)\n")
    cases.append("foo(cn=1,oc=2,st=3)\n")
    cases.append("d = {'cn': 1, 'oc': 2}\n")
    cases.append("foo(**{'cn': 1})\n")
    # 11. Randomized composites (deterministic).
    zones = ["arg", "stmt", "eqeq", "dq", "sq", "comment", "bare"]
    for i in range(300):
        a1, a2 = pick(KWARG_ALIASES), pick(KWARG_ALIASES)
        z = pick(zones)
        if z == "arg":
            cases.append(f"r{i}(x, {a1}=1, {a2}=2)\n")
        elif z == "stmt":
            cases.append(f"{a1} = r{i}({a2}=3)\n")
        elif z == "eqeq":
            cases.append(f"r{i}({a1}=={a2})\n")
        elif z == "dq":
            cases.append(f'r{i}(t="{a1}=1", {a2}=2)\n')
        elif z == "sq":
            cases.append(f"r{i}(t='{a1}=1 {a2}=2')\n")
        elif z == "comment":
            cases.append(f"r{i}({a1}=1)  # {a2}=2\n")
        else:
            cases.append(f"r{i}({a1}, {a2}=2)\n")
    return cases


# --- hooks tier (hooks.rs) ----------------------------------------------
# The SHIPPING hook table, kept in lockstep with hooks.rs by
# gen-hook-data.py --check (CI) and by the FNV manifest gate in
# crates/pyths_expand/tests/gates.rs.
HOOK_ALIASES = ["us", "ue", "um", "uc", "ur", "ux"]
HOOK_NONALIASES = ["use_state", "usx", "u", "uss", "foo", "_us", "Us"]


def gen_hook_cases() -> list[str]:
    """Cases where tiers E/C/A/B/Dict are identity, so `pyths expand`
    reduces to the hooks tier alone: no `%`/`$` sigils, no PSX `<tag`, no
    preset/decorator lines, and no Tier-B kwarg alias in argument position."""
    rng = xorshift(0x5EED_2026_0714_1)
    pick = lambda xs: xs[next(rng) % len(xs)]
    cases: list[str] = []

    # 1. Every alias in CALL position — the only place it may fire.
    for a in HOOK_ALIASES:
        cases.append(f"v, sv = {a}(0)\n")
        cases.append(f"x = {a}(lambda: 1, [])\n")
    # 2. Attribute access must INHIBIT (prev_was_dot), incl. spaced dots.
    for a in HOOK_ALIASES:
        cases.append(f"obj.{a}(0)\n")
        cases.append(f"obj . {a}(0)\n")
        cases.append(f"a.b.{a}(0)\n")
    # 3. Inside a longer identifier must inhibit (prev_ident_cont).
    for a in HOOK_ALIASES:
        cases.append(f"x1{a}(0)\n")
        cases.append(f"my{a}(0)\n")
        cases.append(f"{a}x(0)\n")
    # 4. Bare reference (no `(`) — never fires.
    for a in HOOK_ALIASES:
        cases.append(f"x = {a}\n")
        cases.append(f"{a} = 1\n")
        cases.append(f"f({a})\n")
    # 5. Every protected zone kind passes the alias through verbatim.
    for a in HOOK_ALIASES:
        cases.append(f'p = "{a}(0)"\n')
        cases.append(f"q = '{a}(0)'\n")
        cases.append(f"# {a}(0)\n")
        cases.append(f'r = """{a}(0)\nstill {a}(1)"""\n')
    # 6. Near-miss identifiers.
    for u in HOOK_NONALIASES:
        cases.append(f"z = {u}(0)\n")
    # 7. Nesting / adjacency.
    cases.append("f(us(0), ue(g))\n")
    cases.append("w = us(um(uc(1)))\n")
    cases.append('e = "esc \\" us(0) still-in-string" + ue(1)\n')
    # 8. Randomized composites (deterministic).
    zones = ["call", "dot", "ident", "bare", "dq", "sq", "comment"]
    for i in range(300):
        a1, a2 = pick(HOOK_ALIASES), pick(HOOK_ALIASES)
        z = pick(zones)
        if z == "call":
            cases.append(f"c{i} = {a1}({a2}(0))\n")
        elif z == "dot":
            cases.append(f"c{i} = o.{a1}(0) + {a2}(1)\n")
        elif z == "ident":
            cases.append(f"c{i} = x{a1}(0) + {a2}(1)\n")
        elif z == "bare":
            cases.append(f"c{i} = {a1} + {a2}(2)\n")
        elif z == "dq":
            cases.append(f'c{i} = "{a1}(0)" + {a2}(1)\n')
        elif z == "sq":
            cases.append(f"c{i} = '{a1}(0) {a2}(1)'\n")
        else:
            cases.append(f"c{i} = {a1}(0)  # {a2}(1)\n")
    return cases


# --- Tier E (idioms.rs) -------------------------------------------------
# Tier E has NO compiler-side table: the `%NAME` map comes from
# `[expand.idioms]` in pyths.toml and is empty by default. The differential
# therefore feeds BOTH sides the same committed fixture —
# verification/idiom-table.toml, which is also what gen-idiom-data.py
# compiles into IdiomData.lean. Fixture entries (see that file):
IDIOM_NAMES = ["HTTPCHECK", "LOG", "GUARD", "PASS", "EMPTY", "MODEXPR",
               "_priv", "X2", "10"]
IDIOM_UNKNOWN = ["NOPE", "Log", "PASS2", "_", "Z9"]


def gen_idiom_cases() -> list[str]:
    """Cases where tiers C/A/B/hooks/Dict are identity — on the INPUT *and*
    on the expanded fragments, since Tier E runs FIRST and its output flows
    through every later tier. The fixture's fragments are chosen to be fixed
    points of those tiers (no `$NAME`, no kwarg alias in arg position, no
    hook alias in call position, no `<tag`, no preset/decorator line)."""
    rng = xorshift(0x5EED_2026_0714_2)
    pick = lambda xs: xs[next(rng) % len(xs)]
    cases: list[str] = []

    # 1. Every fixture name in code position.
    for n in IDIOM_NAMES:
        cases.append(f"%{n}\n")
        cases.append(f"x = %{n}\n")
    # 2. Every name in every protected zone kind — must NOT expand.
    for n in IDIOM_NAMES:
        cases.append(f'a = "%{n}"\n')
        cases.append(f"b = '%{n}'\n")
        cases.append(f"# %{n}\n")
        cases.append(f'c = """doc %{n}\nmore %{n}"""\n')
    # 3. Unknown names pass through verbatim.
    for u in IDIOM_UNKNOWN:
        cases.append(f"y = %{u} + %PASS\n")
    # 4. Bare `%` (modulo) is not a sigil — and the DOCUMENTED HAZARD: the
    #    fixture's all-digit name `10` DOES intercept `x %10`.
    cases.append("m1 = a % b\n")
    cases.append("m2 = a %b\n")
    cases.append("m3 = 5 % 10\n")
    cases.append("m4 = 5 %10\n")
    cases.append("m5 = fmt % (1, 2)\n")
    # 5. Maximal munch on the name run: `%EMPTYb` is the name `EMPTYb`.
    cases.append("mm = %EMPTYb\n")
    cases.append("mn = %PASSX\n")
    # 6. Adjacency / edges.
    cases.append("z1 = %PASS%LOG\n")
    cases.append("z2 = %PASS + %\n")
    cases.append("z3 = % PASS\n")
    cases.append("z4 = (%PASS,%X2)\n")
    # 7. Escapes must not close a zone early.
    cases.append('e1 = "esc \\" %PASS still-in-string" + %LOG\n')
    cases.append("e2 = 'esc \\' %PASS still' + %X2\n")
    # 8. Multi-line fragment inserted mid-line (HTTPCHECK spans 3 lines).
    cases.append("async def f():\n    %HTTPCHECK\n")
    # 9. Randomized composites (deterministic).
    zones = ["code", "dq", "sq", "comment", "triple"]
    for i in range(300):
        n1, n2 = pick(IDIOM_NAMES), pick(IDIOM_NAMES)
        z = pick(zones)
        if z == "code":
            cases.append(f"r{i} = %{n1}\ns{i} = %{n2}\n")
        elif z == "dq":
            cases.append(f'r{i} = "%{n1}"\ns{i} = %{n2}\n')
        elif z == "sq":
            cases.append(f"r{i} = '%{n1} %{n2}'\n")
        elif z == "comment":
            cases.append(f"r{i} = %{n1}  # %{n2}\n")
        else:
            cases.append(f'r{i} = """%{n1}\n%{n2}"""\n')
    return cases


# --- Tier A (presets.rs + decorators.rs) --------------------------------
PRESET_MARKERS = ["R.", "R*", "R+", "A*", "T*", "T+", "D*", "W*"]
DECORATOR_ALIASES = ["@c", "@d", "@v", "@h", "@k"]
NON_MARKERS = ["Z*", "R**", "r*", "R", "@zz", "@", "@C"]


def gen_tiera_cases() -> list[str]:
    """Tier A is a per-LINE rewrite GATED BY THE SHARED ZONE CLASSIFIER. Cases
    keep the other tiers identity, on the input and on the expanded lines (the
    preset imports and decorator names are fixed points of B/hooks/Dict:
    `use_state` etc. are followed by `,` not `(`, so the hooks tier does not
    fire).

    §5 is the zone-safety arm: a marker or decorator alias alone on a line
    inside a triple-quoted string (or after a `#`) is docstring/comment TEXT
    and must be emitted byte-for-byte, while the same marker in a real code
    position — including on the line right after a closed string — must still
    expand. Rust (`zones::line_start_states`) and Lean
    (`tierA_zone_safety_chars`) must agree on all of it, byte for byte.

    Every case keeps its strings BALANCED: the harness concatenates them into
    one document, so an unterminated quote would leak into the next case."""
    rng = xorshift(0x5EED_2026_0714_3)
    pick = lambda xs: xs[next(rng) % len(xs)]
    cases: list[str] = []

    # 1. Every preset marker alone on a line, bare and indented.
    for m in PRESET_MARKERS:
        cases.append(f"{m}\n")
        cases.append(f"    {m}\n")
        cases.append(f"\t{m}\t\n")       # trailing whitespace is DROPPED
        cases.append(f"  {m}  \n")
    # 2. Every decorator alias, bare and with call-args.
    for d in DECORATOR_ALIASES:
        cases.append(f"{d}\n")
        cases.append(f"    {d}\n")
        cases.append(f"{d}(coerce=True)\n")
        cases.append(f"  {d}(x=1, y=2)\n")
    # 3. Marker NOT alone on the line => untouched.
    for m in PRESET_MARKERS:
        cases.append(f"x = {m}\n")
        cases.append(f"{m} extra\n")
        cases.append(f"{m}  # note\n")
    # 4. Near-misses.
    for u in NON_MARKERS:
        cases.append(f"{u}\n")
    # 5. ZONE SAFETY (see docstring): a marker alone on a line inside a
    #    triple-quoted string is TEXT — emitted verbatim; the same marker in a
    #    code position still expands.
    cases.append('doc = """\nR*\n@c\n"""\n')             # double-quoted triple
    cases.append("d2 = '''\nT*\n@d\n'''\n")              # single-quoted triple
    cases.append('d3 = """\n  R+  \n"""\n')              # indented + trailing ws
    for m in PRESET_MARKERS:                             # every marker, in a docstring
        cases.append(f'k = """\n{m}\n"""\n')
    for d in DECORATOR_ALIASES:                          # every alias, in a docstring
        cases.append(f"j = '''\n{d}\n'''\n")
    #    …and immediately AFTER the closed docstring, in real code (must expand).
    cases.append('d4 = """\nR*\n"""\nR*\n@c\n')
    #    Nested / adjacent quotes: the inner quotes must not close the zone.
    cases.append('d5 = """\n"R*"\n\'@c\'\n"""\n')
    cases.append('d6 = """x""" + """\nT*\n"""\n')        # closed, then reopened
    cases.append('d7 = """\n"""\nR*\n')                  # empty docstring, then code
    #    An escaped quote inside a docstring does not close it: `\\"` + `""` is
    #    only two live quotes, so `R*` on the next line is still inside.
    cases.append('d8 = """\n\\"""\nR*\n"""\n')
    #    A line that merely CONTAINS a quote (closed on the line) → next line
    #    is code and the marker there expands.
    cases.append('s2 = "x" + "y"\nR*\n@d\n')
    cases.append("s3 = 'R*'\n@c\n")
    #    Single-line docstrings on one line; the marker after them is code.
    cases.append('s4 = """R*"""\nT*\n')
    cases.append("# R*\n")               # a `#` comment line is NOT a marker line
    cases.append("# @c\n")
    cases.append('s = "R*"\n')           # marker inside a string, not alone on a line
    # 6. Blank / whitespace-only lines; EOF without a newline; CRLF.
    cases.append("\n")
    cases.append("   \n")
    cases.append("\t\n")
    cases.append("@c\r\n")
    cases.append("R*\r\n")
    # 7. Randomized composites (deterministic).
    for i in range(300):
        m, d = pick(PRESET_MARKERS), pick(DECORATOR_ALIASES)
        k = next(rng) % 5
        if k == 0:
            cases.append(f"{m}\n")
        elif k == 1:
            cases.append(f"{'    ' * (i % 3)}{d}\n")
        elif k == 2:
            cases.append(f"v{i} = {m}\n")
        elif k == 3:
            cases.append(f"{d}(n={i})\n")
        else:
            cases.append(f"  {m}  \n")
    # Tier A is line-oriented: a trailing newline keeps the last line well
    # formed for the "no trailing newline" edge below.
    cases.append("@k")
    return cases


def run(cmd: list[str], stdin_text: str | None = None,
        infile: Path | None = None, cwd: Path | None = None) -> str:
    """Run a child in BINARY mode.

    Text mode would be fatal here: on Windows, Python's text-mode stdin
    translates each `\\n` we write into `\\r\\n`, so a corpus case containing a
    CRLF (`"@c\\r\\n"`) reaches the Lean model as `@c\\r\\r\\n` — a different
    input from the one `pyths expand` reads out of the corpus FILE. The
    differential must compare the two engines on the same bytes.
    """
    args = cmd + ([str(infile)] if infile else [])
    payload = stdin_text.encode("utf-8") if stdin_text is not None else None
    r = subprocess.run(args, input=payload, capture_output=True, cwd=cwd)
    if r.returncode != 0:
        raise RuntimeError(f"{args} failed:\n{r.stderr.decode('utf-8', 'replace')}")
    return r.stdout.decode("utf-8")


def run_tier(name: str, cases: list[str], lean_args: list[str],
             pyths: str, lean_exe: str, rust_src: str,
             toml: Path | None = None) -> int:
    # All cases are balanced (strings closed), so one concatenated document
    # expands identically to per-case expansion — one process each side.
    doc = "".join(cases)

    with tempfile.TemporaryDirectory() as td:
        scratch = Path(td)  # no pyths.toml here → empty user dictionary
        if toml is not None:
            # Tier E only: give the REAL expander the same committed idiom
            # table the Lean model was generated from.
            (scratch / "pyths.toml").write_text(
                toml.read_text(encoding="utf-8"), encoding="utf-8", newline="\n")
        psc = scratch / "corpus.psc"
        psc.write_text(doc, encoding="utf-8", newline="\n")
        real = run([pyths, "expand", "--quiet"], infile=psc, cwd=scratch)
        model = run([lean_exe] + lean_args, stdin_text=doc)

    real_n = real.replace("\r\n", "\n")
    model_n = model.replace("\r\n", "\n")
    if real_n == model_n:
        print(f"[diff-harness/{name}] model == implementation on {len(cases)} "
              f"cases ({len(doc)} bytes) — byte-identical")
        return 0

    # Localize the divergence for the failure report.
    real_lines, model_lines = real_n.splitlines(), model_n.splitlines()
    shown = 0
    for idx, (r, m) in enumerate(zip(real_lines, model_lines)):
        if r != m:
            print(f"line {idx + 1}:\n  pyths: {r!r}\n  lean:  {m!r}")
            shown += 1
            if shown >= 10:
                break
    if len(real_lines) != len(model_lines):
        print(f"line-count mismatch: pyths={len(real_lines)} lean={len(model_lines)}")
    print(f"[diff-harness/{name}] DIVERGENCE — the Lean model no longer matches "
          f"{rust_src}; update the model (and its proofs) before merging.")
    return 1


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--tier", choices=["dict", "kwargs", "hooks", "idioms",
                                       "tiera", "all"], default="all")
    ap.add_argument("--pyths", default=str(ROOT / "target" / "release" /
                    ("pyths.exe" if sys.platform == "win32" else "pyths")))
    ap.add_argument("--lean-exe", default=str(ROOT / "verification" / ".lake" /
                    "build" / "bin" /
                    ("expanddiff.exe" if sys.platform == "win32" else "expanddiff")))
    opts = ap.parse_args()

    rc = 0
    if opts.tier in ("dict", "all"):
        rc |= run_tier("dict", gen_cases(), [], opts.pyths, opts.lean_exe,
                       "strings.rs")
    if opts.tier in ("kwargs", "all"):
        rc |= run_tier("kwargs", gen_kwarg_cases(), ["--kwargs"], opts.pyths,
                       opts.lean_exe, "kwargs.rs")
    if opts.tier in ("hooks", "all"):
        rc |= run_tier("hooks", gen_hook_cases(), ["--hooks"], opts.pyths,
                       opts.lean_exe, "hooks.rs")
    if opts.tier in ("idioms", "all"):
        rc |= run_tier("idioms", gen_idiom_cases(), ["--idioms"], opts.pyths,
                       opts.lean_exe, "idioms.rs",
                       toml=ROOT / "verification" / "idiom-table.toml")
    if opts.tier in ("tiera", "all"):
        rc |= run_tier("tiera", gen_tiera_cases(), ["--tiera"], opts.pyths,
                       opts.lean_exe, "presets.rs/decorators.rs")
    return rc


if __name__ == "__main__":
    sys.exit(main())
