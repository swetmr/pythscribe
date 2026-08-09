# PythScribe formal grammars (`grammar/`)

Best-effort formal [Lark](https://github.com/lark-parser/lark) grammars for
PythScribe source:

| File | Surface | Status |
|---|---|---|
| `pyths.lark` | `.ps` — canonical PythScribe | corpus-gated in CI |
| `psc.lark` | `.psc` — compressed PythScribe (overlay importing `pyths.lark`) | corpus-gated in CI |

## The contract

**The hand-written recursive-descent parser in
`crates/pyths_parser/src/parser.rs` is authoritative.** These grammars are
*validated against the corpus gate, not correct by construction*:
`scripts/test-grammar.py` parses every tracked `.ps` / `.psc` file, all 580
CPython differential-corpus entries, and the 7 clone pairs, and fails CI on
any unexcluded divergence. If the grammar and `pyths_parser` disagree, the
grammar is wrong — fix the grammar (or exclusion-list with a paper-trail
comment) and, if the gap is interesting, add a `pyths_parser` test upstream.

Primary intended consumer: **grammar-constrained decoding** (SynCode), plus
editor tooling and fuzzing. Not a second implementation of record.

## `.ps` deviations from Python 3 (all cross-checked against parser.rs)

| Deviation | Grammar placement | parser.rs |
|---|---|---|
| `??` nullish coalescing | binary, left-assoc, looser than `or`, tighter than `\|>` | `parse_nullish` :1313 |
| `\|>` pipeline | binary, left-assoc, between ternary/lambda and `??` | `parse_pipeline` :1295 |
| `?.` optional chaining | postfix trailer: `x?.a`, `x?.[i]`, `x?.(args)` | `parse_postfix` :1710 |
| `f(=name)` kwarg shorthand | call-arg item (`name=name` elision) | `parse_call_args` :1790 |
| positional args AFTER keyword args | call args are one free-order item list | `parse_call_args` :1774 |
| `import "./styles.css"` side-effect import | `import_side_effect` statement | `parse_import_stmt` :548 |
| decorators are full expressions | `decorator: "@" test` | `parse_decorated` :1036 |
| `await` at unary level | `factor`-level, may wrap `**` | `parse_unary` :1631 |
| `yield` / `yield from` are primaries | atom alternative | `parse_primary` :2131 |
| comprehensions: `for`+ each with `if`* | `comp_for ... comp_if*` repeats | `parse_comprehension_clauses` :2157 |
| `from ....mod import x` mixed-dot levels | `dots: ("." \| "...")+` | `parse_from_import_stmt` :609 |

And the deliberate *restrictions* vs stock Python (the .ps parser rejects
these, so the grammar does too): no `;` statement chaining, no inline suites
(`if x: y = 1`), `return`/`raise` take a single expression (no testlist, no
`raise .. from ..`), no `from x import *` / parenthesized import lists, no
hex/oct/bin/imaginary number literals (decimal only, leading zeros allowed),
only `r`/`R`/`f` string prefixes, no `@` matmul / `@=`, ASCII-only
identifiers. Full annotated list in the `pyths.lark` header.

Known permissive spots (grammar accepts a superset of parser.rs — harmless
for the corpus gate, relevant for constrained decoding): match-case pattern
internals, assignment/for/with target positions, backslash line
continuation. See the `pyths.lark` header.

## `.psc` overlay (`psc.lark`)

Adds, on top of the `.ps` grammar (expansion pipeline:
`crates/pyths_expand/src/lib.rs`):

- **Tier A presets** (`presets.rs`): `R*` `R+` `A*` `T*` `T+` `D*` `W*` as
  whole-line statements (terminal has an end-of-line lookahead, so `R * 2`
  stays an expression).
- **`$NAME` / `%NAME` sigils** (`strings.rs` / `idioms.rs`): expression
  atoms; a lone `%NAME` statement line is just an expression statement.

**Requiring NO grammar change** (verified reasoning, documented in
`psc.lark`): Tier A decorator aliases (`@c` is an ordinary `"@" test`
decorator), Tier B kwarg aliases (`st=` etc. are plain identifiers in
keyword-arg position), hook-call aliases (`us(...)` is a plain identifier
call).

psc.lark now covers the **entire** compressed surface: all 13 tracked `.psc`
files parse directly (0 via the expand-then-parse fallback).

### Historical: the PSX `#id` exception (no longer applicable)

The grammar once carried a PSX tag-DSL (angle-bracket markup), and its `#id`
shortcut attr (`<div #root>`) had to be **excluded** from psc.lark: `#` opens a
COMMENT at the lexical level, and a context-free lexer cannot disambiguate
`#root` (attribute) from `#root` (comment) without the expander's byte-level
scanner context. Such files were validated post-expansion instead. The PSX tier
has since been **removed from the expander**, so the grammar has no angle-bracket
constructs, nothing takes the fallback path, and psc.lark is a complete acceptor
for `.psc` again. The expand-then-parse fallback remains in
`scripts/test-grammar.py` as an unused safety net.

## Running the gate locally

```bash
pip install lark
python scripts/test-grammar.py
```

Parses: all tracked `.ps` files, all tracked `.psc` files (psc.lark, with
the expand-then-parse fallback), all 580 entries of
`tests/differential/cpython_corpus.json`, and the 7 clone pairs in
`examples/clones/shared/*/` (`.ps` directly + expanded `.psc`). Exclusions
live at the top of the script with paper-trail comments; exit is non-zero
on any unexcluded failure. In CI this runs as a soft-fail step during
launch week (see `.github/workflows/ci.yml`).

## SynCode usage (designed for SynCode; untested)

```python
# pip install git+https://github.com/structuredllm/syncode  (not run here)
from syncode import Syncode

grammar = open("grammar/pyths.lark").read()   # or grammar/psc.lark
llm = Syncode(model="<hf-model-id>", grammar=grammar, parse_output_only=True)
out = llm.infer("Write a PythScribe component that renders a counter:")
```

Notes for constrained decoding:
- Both grammars build with `parser='lalr'` + `postlex=PythonIndenter`
  (lark's `lark.indenter.PythonIndenter`), which is SynCode's supported
  configuration for indentation-sensitive languages (its builtin Python
  grammar works the same way).
- `psc.lark` uses `%import .pyths (...)` — keep both files in one directory
  and pass `import_paths=['grammar']` (or load via `Lark.open`).
- The permissive spots listed above mean a constrained decoder can emit a
  handful of forms `pyths_parser` rejects; run `pyths check` on generated
  code as the final gate.

## Future work

- **XGrammar**: its EBNF dialect has no indenter/postlex hook, so an
  indentation-sensitive grammar can't be ported 1:1; would need an
  indentation-free bracketed variant or token-level integration.
- Flip the CI step to hard-fail after a day green on main.
- Teach the corpus gate to diff grammar-accepts vs `pyths check`-accepts on
  the fuzz corpus (agreement testing in both directions).
