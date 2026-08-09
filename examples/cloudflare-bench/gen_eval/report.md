# Generation-token evaluation — do LLMs save output tokens writing `.ps` / `.psc`?

**Status: populated — see `bench/ablations/ledger.jsonl` (38 rows incl.
`livermore_v1`) for every published number with regeneration references.**
Headline: paired 17-18% output-token savings (3 models), behavioral macro
pass@1 lifted to 98%/89% (.ps/.psc, Opus) by the convention-aware manual.

## Claim under test

Prior `.ps`/`.psc` token measurements were **transcoding** measurements (existing
source re-encoded and counted). This eval measures the **generation** claim instead:
when an LLM is asked to *write a program from a natural-language brief*, does it emit
fewer o200k output tokens in `.ps` / `.psc` than in plain Python, **at non-inferior
correctness**? That is the token-cost claim that actually matters for LLM code
emission (the LOIR story).

## Methodology

- **Conditions** (between-subjects per sample):
  - `python` — standard Python 3, minimal prompt.
  - `ps` — PythScribe, with a distilled authoring manual (from `SKILL.md` +
    `docs/language-reference.md`) in the prompt.
  - `psc` — compressed PythScribe: the same manual **plus** the compression-tier
    guide (Tier A presets, Tier B aliases, bundled `$NAME`
    dictionary — Tier E excluded: no project `pyths.toml`). The model writes `.psc`
    directly.
- **Tasks** (`tasks/tasks.jsonl`, built + oracle-verified by `tasks/make_tasks.py`):
  - **40 micro tasks** derived from the surfaces of
    `tests/differential/cpython_corpus.json` (strings, formatting, collections,
    itertools, arithmetic/floats, decimal/fractions, control flow / functions /
    classes). Prompts are natural-language briefs that pin the exact print format;
    they never contain solution code. `expected_stdout` is computed by *running* a
    reference solution under CPython 3.12 at task-build time.
  - **9 macro tasks**: self-contained React-component briefs modeled on
    `examples/clones/shared/*` (HelloCard, Kanban, YouTube/Twitter/Spotify/
    Coursera/Netflix-style). Verified by **compile-success** only (`pyths compile`;
    plus `pyths expand` for `psc`). **Macro tasks skip the `python` condition** — a
    plain Python 3 program has no React-component equivalent, and faking one (e.g.
    grading unverifiable pseudo-UI code) would corrupt the correctness axis. The
    python column for the macro phase is therefore structurally absent, and
    ps-vs-psc is the comparison of record there.
- **Sampling**: N = 5 samples per task × condition, one model.
  **Temperature caveat:** the `claude` CLI does not expose a temperature control, so
  samples are drawn at the CLI default. We therefore report **medians and IQRs over
  N samples** rather than assuming controlled sampling variance.
- **Token counting**: o200k_base (`tiktoken`) tokens of the **extracted fenced code
  block only** — prose around the block is a prompt-compliance failure, not a token
  saving. API-reported `output_tokens` are also recorded per sample as a secondary
  column (they include any prose + wrapper).
- **Skill-overhead accounting**: writing `.ps`/`.psc` requires teaching the model
  the language in the prompt. The o200k size of each condition prompt is recorded as
  `skill_overhead_o200k` in every ledger row. Output-token savings must be read
  *against* this input-side overhead: for short sessions the manual dominates; it
  amortizes only across many generations per conversation (and is cacheable). Both
  numbers are reported; neither is netted into the other.
- **Correctness oracle**:
  - micro: byte-for-byte stdout comparison against the CPython-3.12-computed
    `expected_stdout` (after `\r\n` → `\n` normalization and stripping one trailing
    newline). `python` runs under CPython; `ps` compiles with `pyths compile` and
    runs under Node 22 with runtime imports rewired (mirroring
    `tests/differential/run.mjs`); `psc` first passes `pyths expand`, then the
    expansion is compiled and run.
  - macro: compile-success (+ expand-success for `psc`).
- **Non-inferiority framing**: the headline comparison is (median output tokens,
  pass rate) per condition. `ps`/`psc` token savings only count if pass rate is not
  meaningfully below `python`'s; `tokens-per-correct-solution`
  (= total tokens emitted / number of correct samples) is the single number that
  folds both axes together.

## Honest-reporting note

This eval is designed to be able to return a negative result. If `psc` output tokens
are **not** lower than `ps` (BPE merges already absorb most identifier-level
compression — see `docs/compression.md` § "the BPE wall"), that is the finding and it
gets reported as such. Same if `ps`/`psc` pass rates are materially below `python`:
correctness-inferior token savings are not savings. No cell in the results tables may
be filled from anything but ledger rows traceable to raw completions.

## Results

### Aggregate (per condition × phase)

*Populated by baseline run `<exp_id>` — run `python ../bench/ablations/render_report.py`.*

| phase | condition | n | o200k out median | IQR | pass rate | syntax-err rate | skill overhead (o200k) |
|---|---|---:|---:|---:|---:|---:|---:|
| micro | python | — | — | — | — | — | — |
| micro | ps | — | — | — | — | — | — |
| micro | psc | — | — | — | — | — | — |
| macro | ps | — | — | — | — | — | — |
| macro | psc | — | — | — | — | — | — |

### Tokens per correct solution

*Populated by baseline run `<exp_id>`.*

| phase | python | ps | psc |
|---|---:|---:|---:|
| micro | — | — | — |
| macro | n/a | — | — |

### Headline deltas

*Populated by baseline run `<exp_id>`.*

- `ps` vs `python` median output tokens (micro): —
- `psc` vs `ps` median output tokens (micro): —
- `psc` vs `ps` median output tokens (macro): —
- pass-rate deltas: —

## Reproduction

```bash
# 1. tasks (already committed; re-verify oracle outputs)
python tasks/make_tasks.py --check

# 2. dry-run the plumbing (3 tasks x 3 conditions x 1 sample)
node run_eval.mjs --exp-id dryrun-001 --tasks 3 --n 1

# 3. baseline (real API cost — see estimate in the harness build report)
node run_eval.mjs --exp-id base-001 --n 5

# 4. render
python ../bench/ablations/render_report.py --exp base-001
```

Artifacts: raw completions in `raw/<exp_id>/`, per-sample results in
`results/<exp_id>.jsonl`, aggregates appended to `../bench/ablations/ledger.jsonl`.
