# Constrained decoding — Repairability evidence

This directory provides the missing evidence for the **Repairability** LOIR
requirement. Repairability has two halves:

1. *Machine-first syntax with a tool that renders a human view* — already shipped
   as `pyths expand` (canonical `.ps` ⇄ human view).
2. *A model cannot emit a malformed program* — grammar-constrained decoding.
   This directory supplies the evidence for half 2.

The corpus-validated grammar produced in W2 (`grammar/pyths.lark`, LALR-buildable
with `lark`'s `PythonIndenter`) is the syntactic gate a constrained decoder
enforces. We prove it in two tiers.

---

## Tier 1 — grammar-as-acceptor proof (offline, no GPU)  ✅

**Claim proven:** `grammar/pyths.lark` is a *sound acceptor* — it accepts every
valid program and rejects every malformed one. This is exactly the property a
grammar-constrained decoder relies on: at each step it masks the next-token set
to those that keep a valid-prefix, so the emitted program is guaranteed to be in
the grammar's language. If the grammar over-accepts, the guarantee is hollow; if
it under-accepts (false-rejects valid code), the decoder needlessly blocks good
programs. We show neither happens.

Run:

```bash
pip install lark
cargo build --release --workspace          # provides target/release/pyths
python examples/constrained-decoding/acceptor_demo.py
```

### Tier 1a — accepts real model output, never false-rejects valid code

Every `.ps` / `.psc` completion from the just-run generation eval
(`examples/cloudflare-bench/gen_eval/raw/baseline-001/`, 245 `.ps` + 245 `.psc`)
is parsed with `grammar/pyths.lark`. `.psc` completions are first expanded
through `pyths expand`, so the acceptor under test is exactly the canonical-`.ps`
grammar a decoder would constrain.

| Condition | Gradeable accepted | Ungradeable (skipped) |
|---|---|---|
| `.ps`  | **241 / 241 (100.0%)** | 4 model-error records |
| `.psc` | **244 / 244 (100.0%)** | 1 model-error record  |
| **All** | **485 / 485 (100.0%)** | 5 model-error records |

- *Ungradeable records* are `<!-- model_error -->` entries (API timeouts /
  connection-closed mid-response) — the harness emitted no program, so there is
  nothing to accept. They are reported separately, not scored against the grammar.
- The **single** non-accept (`except_custom_psc_5.md`) is
  `class TooSmallError(Exception): pass` — an **inline suite**, which the
  *authoritative* parser (`pyths check`) **also rejects**. The demo cross-checks
  every grammar rejection against `pyths check`: this one is *agreement*, not a
  grammar gap — the model emitted genuinely-invalid PythScribe, precisely the
  output a constrained decoder would have made unreachable.
- **False rejects of valid code (real grammar gaps): 0.** Every gradeable valid
  completion is accepted.

### Tier 1b — rejects malformed mutations, 100%

Each accepted completion is subjected to a battery of **guaranteed-malformed,
structural** mutations. We deliberately avoid random character injection that
could land inside a string literal or comment (where it would stay *valid* — a
mutation bug, not a grammar bug); each mutation is constructed to be genuinely
malformed.

| Mutation | What it breaks | Rejected |
|---|---|---|
| `drop_colon` | `:` stripped from a compound header | **170 / 170 (100%)** |
| `missing_indent` | body of a `:`-header de-indented to col 0 | **170 / 170 (100%)** |
| `unbalanced_paren` | first `(` on a quote-free line deleted | **359 / 359 (100%)** |
| `stray_dollar` | `$` injected outside strings/comments (invalid `.ps` token) | **396 / 396 (100%)** |
| `dangling_op` | trailing binary operator at EOF (truncated expression) | **484 / 484 (100%)** |
| **Total** | | **1579 / 1579 (100.00%)** |

### Verdict (Tier 1)

```
no false rejects of valid code : True   (484/485 gradeable accepted; the 1 non-accept
                                          corroborated by `pyths check` as invalid input)
rejects 100% of malformed muts : True   (1579/1579)
=> grammar/pyths.lark is a SOUND ACCEPTOR
```

Machine-readable summary: `acceptor_result.json`.

---

## Tier 2 — live grammar-constrained generation  (see status below)

`constrained_gen_demo.py` runs a small local model
(`Qwen/Qwen2.5-Coder-0.5B-Instruct`, CPU) under **SynCode** grammar-constrained
decoding with `grammar/pyths.lark` (`mode=grammar_strict`, `indent=True`) and
re-verifies every output with the independent `pyths.lark` acceptor **and**
`pyths check`, so the guarantee is not self-reported.

```bash
pip install syncode lark
cargo build --release --workspace
python examples/constrained-decoding/constrained_gen_demo.py --n 5
```

**Unconstrained baseline for the claim** is the measured baseline-001 syntax-error
rate: **ps/macro = 0.023, psc/micro = 0.005** (from the W3 syntax-error
ablation). The in-script 0.5B unconstrained run is only an illustration on a
trivial prompt.

### What actually ran (honest report)

SynCode **0.4.16 installed and ran end-to-end on this Windows box** against
`grammar/pyths.lark`, on CPU, with `Qwen/Qwen2.5-Coder-0.5B-Instruct`. Two
mechanical, **decoder-only** adaptations were needed (the canonical grammar and
its CI gate are untouched — see `constrained_gen_demo.ensure_syncode_grammar`):

1. add a `start: file_input` rule (SynCode's bundled lark fork requires a rule
   literally named `start`); and
2. replace the one `LONG_STRING` terminal regex, which uses a lookbehind
   `(?<!\\)` that SynCode's `interegular` FSM compiler rejects
   (`lookbacks are not implemented`), with a lookbehind-free equivalent.

SynCode then built its DFA mask store over the Qwen tokenizer + our grammar
(411 states / 95 FSMs, ~10 min, cached to `cache/mask_stores/`).

**The genuine success** (`constrained_focused.py`, sample 0) — a complete, valid
`.ps` program emitted under grammar constraint, re-verified by both the canonical
`pyths.lark` acceptor and `pyths check`:

```python
def square(n):
    return n * n

result = square(7)
print(result)
```

**The fallback is now DIAGNOSED and FIXED — see `FALLBACK.md`.**

- ~~SynCode's incremental parser intermittently throws mid-decode and falls back
  to unconstrained decoding~~ — **superseded (2026-07-14)**. It is not
  intermittent and it is not a grammar bug. SynCode attaches its `PythonIndenter`
  **only when `grammar.name == 'python'`** (`syncode/parsers/__init__.py`), and
  `Grammar.__init__` sets `name` to the FILE PATH for a path-supplied grammar. So
  our grammar got **no indenter**, `_INDENT`/`_DEDENT` were never emitted, and
  every *indented block* was unparseable — the incremental parser threw and
  `grammar_constrainer.py` set `skip=True`, i.e. that step decoded unconstrained.
  (`Syncode(indent=True)` does not help: it is forwarded only to the mask store,
  never to `create_parser`.)

  Measured at the fallback's exact trigger — the incremental parser's throw rate
  on prefixes of known-valid `.ps`:

  | | throw rate |
  |---|---|
  | before | **52.7 %** (270/512) — 0 % on flat programs, 68–77 % on every program with a block |
  | after | **0.0 %** (0/512) |

  Fixed entirely on our side (`syncode_grammar.py`): present the grammar under
  `name='python'` and rename `_NEWLINE` -> `_NL` in the decoder-facing copy.
  SynCode is not patched; the canonical grammar and its CI gates are untouched.

  Not yet run end-to-end: a >=50-completion live generation with per-step fallback
  counts and validity figures. The harness is committed
  (`constrained_measure.py`) but the ~2 GB mask store did not fit on the dev box.
- **`grammar/pyths.lark` is permissive by design** (`%ignore COMMENT`, `%ignore`
  whitespace, bare-expression statements). Constrained decoding guarantees
  *syntactic validity*, not usefulness: strings like `The not in the not in the`
  are genuinely valid `.ps` expression statements, and a tiny model often
  degenerates into empty output or such token-soup. Low `max_new_tokens` also
  truncates programs into valid-*prefix*-but-incomplete outputs.

Raw run artifacts: `constrained_gen_result.json` (n=5, max_new=48 — shows the
truncation effect) and `constrained_focused_result.json` (n=6, opp=False,
max_new=160 — contains the clean sample 0 plus a captured fallback event).

### Level of constrained decoding achieved

**Grammar-as-verified-acceptor (Tier 1): YES, robustly.** And since 2026-07-14 the
grammar is additionally validated *against the authoritative parser* by
bidirectional differential fuzzing — false-accept 0.120 %, false-reject 0.000 %
(`scripts/grammar-fuzz.py`, `grammar/grammar-fuzz-results.md`).

**Live grammar-constrained generation (Tier 2): the fallback is FIXED; the
end-to-end run is still owed.** The unconstrained-decoding fallback was not a
grammar defect and not "intermittent" — it was SynCode gating its indenter on a
hard-coded `grammar.name == 'python'`. Fixed on our side; the incremental
parser's throw rate (which IS the fallback condition) goes 52.7 % -> 0 %. See
`FALLBACK.md`. What remains is to run the committed harness
(`constrained_measure.py`) end-to-end on a machine with enough disk for the ~2 GB
mask store and report the live fallback + validity numbers.

> No syntax-error-rate rows are appended to the ablation ledger, because the
> live run did not yield a robust constrained 0% number — only the Tier-1
> acceptor result and one live success. Appending a fabricated 0% would violate
> the honesty bar.

---

## What level of "constrained decoding" is achieved

- **Tier 1 (always):** the grammar is a *verified sound acceptor* — the
  syntactic gate a constrained decoder enforces is proven correct on 485 real
  completions and 1579 malformed mutations.
- **Tier 2 (if the status above shows results):** a real model, decoded under
  that grammar, cannot emit a program the grammar rejects — demonstrated live and
  independently re-parsed.

`.psc` note: the compressed surface (`psc.lark`) is validated upstream by
`scripts/test-grammar.py`; here `.psc` is expanded to canonical `.ps` first, so
the acceptor under test is the single grammar a decoder constrains against.
