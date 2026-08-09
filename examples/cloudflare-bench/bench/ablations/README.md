# Ablation ledger — generation-token eval

Append-only ledger of aggregate results from the generation-token evaluation harness
(`../../gen_eval/`). The harness measures whether LLMs actually save **output tokens**
writing `.psc` (compressed PythScribe) and `.ps` (PythScribe) versus plain Python, at
non-inferior correctness.

## Files

- `ledger.jsonl` — one row per (condition × phase) aggregate per run, appended by
  `gen_eval/run_eval.mjs` at the end of every run. Append-only; never edit rows.
- `render_report.py` — renders all ledger rows + per-sample results into markdown
  tables. Every table cell is traceable: ledger rows carry `exp_id` + `raw_ref`, and
  per-sample rows live in `gen_eval/results/<exp_id>.jsonl` with raw completions under
  `gen_eval/raw/<exp_id>/`.

## Row schema

```json
{
  "exp_id":   "base-001",
  "date":     "2026-07-06",
  "commit":   "<pythscribe commit sha the run used>",
  "corpus":   "gen_eval/tasks/tasks.jsonl",
  "condition": "python | ps | psc",
  "axis": {
    "tier_subset": "A+B+C+dict(bundled) for psc, null otherwise",
    "model":       "model id the samples were generated with",
    "phase":       "micro (run-and-compare stdout) | macro (compile-success)"
  },
  "metric": {
    "o200k_out_median":     123,     // median o200k tokens of the emitted code block
    "o200k_out_iqr":        17.5,    // Q3 - Q1 over the same samples
    "pass_rate":            0.95,    // verified-correct fraction (see verdicts below)
    "syntax_err_rate":      0.0,     // no_code_block / compile / expand failures
    "skill_overhead_o200k": 2100     // o200k tokens of the condition's system prompt
  },
  "n":       200,                    // samples aggregated into this row
  "raw_ref": "gen_eval/raw/base-001/"
}
```

## Verdict definitions (per-sample, in `gen_eval/results/*.jsonl`)

| Verdict | Meaning | counts as pass | counts as syntax_err |
|---|---|---|---|
| `pass` | stdout byte-equal to CPython oracle (micro) / compiles (macro) | yes | no |
| `fail_output` | ran, wrong stdout | no | no |
| `fail_runtime` | compiled but crashed at runtime | no | no (python: yes if SyntaxError) |
| `fail_compile` | `pyths compile` rejected the code | no | yes |
| `fail_expand` | `pyths expand` rejected the `.psc` | no | yes |
| `no_code_block` | completion had no fenced code block | no | yes |
| `model_error` | CLI/API failure — excluded from aggregates | — | — |

Macro tasks run under `ps`/`psc` only: a plain Python 3 program has no equivalent for a
React component, so the `python` condition is skipped there rather than faked (see
`gen_eval/report.md` § Methodology).

## Rendering

```bash
python render_report.py            # prints markdown to stdout
python render_report.py --exp base-001   # restrict to one experiment
```
