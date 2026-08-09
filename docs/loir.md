# `.psc` as an LLM-Oriented IR (LOIR), `.ps` as the Source-of-Record Language (SORL)

> See also: [`docs/compression.md`](compression.md) (tier reference and measured numbers).

## Framing

The PLDI'26 keynote (Amarasinghe, "PL Design and Implementation in the Era of ML")
argues that when LLMs — not humans — are the primary authors of code, the language
they emit should be designed for *them*: an **LLM-Oriented IR (LOIR)**. A LOIR is
optimized for the model's economics and failure modes, not for human ergonomics, and
it is held to five requirements: **Expressive Range** (Turing-complete, no artificial
ceilings), **Semantic Precision** (one meaning per program), **Token Economy** (leave
token-expensive work to the compiler — "tokens are more expensive than processor
cycles"), **Compiler Complementarity** (LLMs do what compilers can't, and never the
reverse), and **Repairability** (machine-first syntax paired with tools that render a
human view for debugging and repair).

The keynote pairs the LOIR with a **Source-of-Record Language (SORL)**: the layer
humans read, review, and commit. A SORL is held to a different bar — human
ergonomics, semantic precision, evolvable structure, reproducible derivation, and
operational assurance. Connecting the two layers are **bi-directional toolchains**:
deterministic lowering from SORL-adjacent forms, plus LLM-assisted *lifting* of
lower-level edits back up, so multiple levels of the program can coexist in the
record without drifting apart. Prior LOIR-adjacent work includes SimPy
(arXiv 2404.16333), ShortCoder (arXiv 2601.09703), Token Sugar (arXiv 2512.08266),
and, on the constrained-decoding side, SynCode (github.com/structuredllm/syncode)
and XGrammar (arXiv 2411.15100).

PythScribe already implements this two-layer architecture, and has since before the
vocabulary existed: **`.psc` is the LOIR** — an opt-in compressed superset designed
for token-efficient code emission by LLMs — and **`.ps` is the SORL** — the canonical,
Pythonic, human-reviewed surface that the compiler pipeline actually consumes. The
two layers are joined by a deterministic expander (`pyths expand`) whose correctness
contract is the **Iron Rule**:

> `canonicalize(expand(x.psc)) == canonicalize(x.ps)`

enforced by `pyths expand --verify` (`crates/pyths_cli/src/commands/expand.rs`),
which expands the `.psc`, canonicalizes both sides through the canonical printer
(`pyths_print::canonicalize`), and fails loudly on any mismatch. This is exactly the
keynote's "reproducible derivation" property: the LOIR→SORL step is a pure function
of the source plus a committed dictionary (`pyths.toml`), so anyone can re-derive
the record from the compressed artifact and byte-compare the normal forms.

The rest of this document audits `.psc` against the five LOIR requirements and `.ps`
against the five SORL properties — honestly, with MET/PARTIAL statuses and only
measured numbers.

## `.psc` vs the five LOIR requirements

| # | Requirement | Status | Evidence |
|---|---|---|---|
| 1 | **Expressive Range** | **MET** | `.psc` is a strict superset of `.ps`: every compressing tier (A presets/decorators, B kwarg/hook aliases, `$NAME` dictionary, E `%NAME` idioms) is optional and individually disable-able, and a plain `.ps` file is already valid `.psc`. All of these stay in Pythonic **call form** — `.psc` never introduces foreign markup. (An angle-bracket markup DSL and a PySON JSON-AST format once existed alongside these; both have been **removed**. The markup DSL was **token-negative** against the call form it was meant to beat — see Honest numbers → BPE wall — and it was the only tier that could not be proved zone-safe in Lean.) Nothing expressible in PythScribe is lost — the expander only ever *adds* surface, never removes semantics. |
| 2 | **Semantic Precision** | **MET** (core rewrite properties machine-checked) | Expansion is deterministic, text-level rewriting with a fixed tier order — E → C → A → B → hooks → Dictionary (`crates/pyths_expand/src/lib.rs`, `expand_with_config`). One `.psc` program has exactly one canonical `.ps` meaning. Beyond the Iron Rule check (`pyths expand --verify`) and the corpus round-trip suite (`crates/pyths_print/tests/corpus_roundtrip.rs`), the three core properties are now **machine-checked in Lean 4** (`verification/`, 16 theorems, **0 `sorry`**, `propext`-only): **determinism** (fixed tier order + left-totality), **zone-safety** (no tier ever rewrites inside a string/comment/f-string zone — so a `$`/`%` sigil inside a literal never expands), and **alias round-trip** (`expand∘compress = id` on the committed dictionary domain). Honest scope: this verifies the rewrite layer over an assumed-correct zone partition (the byte-level classifier is covered by ~60 Rust unit tests), not end-to-end `.ps`→JS compilation. |
| 3 | **Token Economy** | **MET** (code-type-dependent) | Measured on both axes, not asserted. *Storage:* **8.0%** additional cl100k / **18.7%** byte savings over `.ps` on the React + CRM corpus, atop PythScribe's inherent **−15.1%** vs React+TS; **−2.32%** o200k realized on the reference-app frontend (dictionary **−1.74%** + Tier E **−0.58%**), 31/31 round-trip-verified. *Generation* (zero-shot, see below): at **component/app scale — PythScribe's actual workload —** `.psc` cuts output tokens by a **paired per-task median of 17–18%** vs `.ps`, replicated across **three models** (Opus/Sonnet/Haiku, 9/9 tasks each, sign-test *p*=0.004) at **equal-or-better correctness** on the larger two (pass@1 **1.00** vs 0.977). The honest dependency, hence the qualifier: on trivial one-liners the generation effect is a **wash** (nothing to compress), and the authoring-manual **input** overhead (**2981** vs 81 o200k) must be amortized via prompt caching / generation volume to net out. MET for the code types PythScribe exists to serve; the qualifier is the honesty, not a hedge. Methodology: `examples/cloudflare-bench/bench/` + the generation-eval section below. |
| 4 | **Compiler Complementarity** | **MET** | The token-expensive work stays on the compiler side of the boundary: import presets expand `R*` into the full canonical import list, kwarg/hook aliases expand to their canonical names, and the downstream pipeline contributes type inference/checking (`pyths check`), Python-faithful codegen, and WASM auto-routing. The LLM never writes what the compiler can derive — it emits the compressed intent, the toolchain reconstructs the verbose truth. |
| 5 | **Repairability** | **MET** (live decoder un-hardened) | Both halves are now in place. *Human view:* `pyths expand foo.psc` renders canonical `.ps`, and unknown `$NAME`/`%NAME` aliases pass through verbatim so the lexer errors clearly rather than guessing. *Machine-first constraint:* `grammar/pyths.lark` is an **empirically validated sound acceptor** — over 485 real model completions from the generation eval it accepts **484** (0 false-rejects of valid code; the 1 non-accept is corroborated invalid by `pyths check` itself) and rejects **1579/1579** malformed mutations across 5 structural classes (`examples/constrained-decoding/acceptor_demo.py`). A live grammar-constrained decoder (**SynCode** + `Qwen2.5-Coder-0.5B`, CPU) demonstrably runs against it and emits valid `.ps` programs re-verified by both the grammar and `pyths check`. The honest caveat: SynCode's incremental parser intermittently falls back to unconstrained decoding on our (deliberately permissive) grammar, so *hardening the live decoder to never fall back* is tracked future work — the guarantee substrate (a proven-sound grammar) is done; the hardened decoder integration is not. |

## `.ps` vs the five SORL properties

| SORL property | `.ps` fact |
|---|---|
| **Human ergonomics** | Pythonic surface — the language humans already read — plus a small, documented set of deviations from strict Python: `??` nullish coalescing, `?.` optional chaining (attr/subscript/call), `\|>` pipeline, `func(=name)` kwarg shorthand, dict spread, and the positional-after-keyword parser relaxation that makes flat PSX (`tag(prop=v, child)`) legal. All are documented in `docs/language-reference.md` and `SKILL.md` § "PythScribe Extensions (beyond Python)". |
| **Semantic precision** | One meaning per program, and one *spelling* per meaning: the canonical printer (`pyths_print`) defines the single normal form, so semantically identical programs normalize to identical text. Python semantics (not JavaScript's) are carried through codegen and pinned by differential tests. |
| **Evolvable structure** | The canonical form is stable under round-trip (parse → print → reparse, `crates/pyths_print/tests/corpus_roundtrip.rs`), which makes diffs and reviews meaningful as the language grows; compression tiers evolve independently of the canonical surface because the `auto` expand mode guarantees `.ps` files are never touched by the expander. |
| **Reproducible derivation** | `pyths expand --verify` enforces the Iron Rule per file; the `$NAME` dictionary and `%NAME` idiom table live in a committed `pyths.toml`, so the `.psc` → `.ps` derivation is a pure, replayable function of repo contents. |
| **Operational assurance** | 2,000+ automated tests, including the 1,318-entry CPython differential oracle (fully green, cross-checked on a second JS engine), the panic-resistance fuzz harness (`crates/pyths_cli/tests/fuzz_inputs.rs`) plus weekly coverage-guided `cargo-fuzz`, and the end-to-end clone apps (`examples/clones/` — six shared demos mounted in both Vite and Next.js shells). |

## Bi-directionality status

**`.psc` → `.ps` (lowering to the record): shipped and deterministic.** This is the
direction the keynote demands be reproducible, and it is — `pyths expand` is a pure
text-level pipeline with a fixed tier order, and `--verify` proves each file's
derivation against its committed sibling.

**`.ps` → `.psc` (compression): LLM/skill-driven, no auto-compressor ships.** This is
deliberate. The whole point of `.psc` is that a *zero-shot* LLM, given the alias
tables (via the authoring skill / system prompt), emits compressed source directly —
no fine-tune, no extra tool pass. An automatic `.ps`→`.psc` compressor would also have
to make the judgment calls the BPE-wall analysis shows are non-obvious (which strings
clear the token threshold, which aliases regress), and getting that wrong silently is
worse than not shipping it. A measured, miner-assisted compressor
(`examples/cloudflare-bench/bench/mine_idioms.py` already ranks candidates by o200k
delta) is future work. In keynote terms: the record-ward direction is mechanical and
verified; the LOIR-ward direction is where the LLM lives.

## Honest numbers

### Total token savings vs the React+TypeScript baseline (measured)

The baseline PythScribe compiles to is React + TypeScript (`.tsx`). Measuring the *same*
programs in all three representations (o200k tokens), the savings are strongly
**code-type-dependent** — the same honest story as the generation eval:

| Corpus | `.tsx` (base) | `.ps` | `.psc` | `.ps` vs `.tsx` | `.psc` vs `.tsx` | `.psc` vs `.ps` |
|---|--:|--:|--:|--:|--:|--:|
| **Idiomatic** (2 large-sample apps) | 10,690 | 8,361 | 7,726 | **+21.8%** | **+27.7%** | **+7.6%** |
| **Faithful ports** (6 clone components) | 18,443 | 17,272 | 17,006 | +6.3% | +7.8% | +1.5% |

Idiomatic PythScribe (written to the language's grain) saves ~22–28% of tokens vs
React+TS; a line-for-line **port** (the clones, kept 1:1 with React so the dual-track
oracle stays exact) inherits the source's token structure and saves only ~6–8% — the
compiler has little to condense. o200k tracks cl100k within 1–2 points. Note bytes and
tokens diverge: on the ports `.ps` is ~10% *larger* in bytes yet smaller in tokens (the
BPE-wall effect below). Which figure to expect depends entirely on whether code is
*written in* or *translated into* PythScribe.

### The tier increment (design-time)

The two headline figures come from different corpora and different tokenizers, and
they are never blended:

- **Headline (design-time benchmark):** `.psc` saves **8.0%** cl100k tokens /
  **18.7%** bytes over equivalent `.ps` on the 34,486-byte React + CRM benchmark
  corpus (`docs/compression.md`), on top of PythScribe's inherent **−15.1%** cl100k
  vs equivalent React+TS source.
- **Realized (deployed frontend corpus, v2 increment):** on the reference-app frontend `.psc`
  corpus (34,464 o200k tokens), the dictionary + Tier E increment realized
  **−2.32%** o200k — dictionary **−1.74%**, Tier E idioms **−0.58%** (one robust
  idiom, `%HTTP_OK`, 11 occurrences across 9 files) — round-trip-verified via
  `pyths expand --verify` on 31/31 files. The first-pass mining estimate (~4.5%)
  was raw-text-optimistic; the round-trip-verified realized figure is the one to
  cite.

**The negatives are first-class results.** The BPE wall — modern BPE vocabularies
have already merged common identifiers into single tokens — rules out whole tier
families that look obviously good on bytes:

- **HTML-tag call shortcuts** (`div(` → `dv(`, `button(` → `bt(`, …): net-zero cl100k;
  ruled out.
- **Keyword aliases** (`lambda` → `lm`): net-zero cl100k; ruled out.
- **Exception aliases** (`raise Exception(` → `raise Ex(`): net-zero cl100k; ruled out.
- **Worst case, a regression:** `Link(` → `Lk(` costs **−1** (i.e. *adds* a token) —
  `Lk` falls outside the merged vocabulary and fragments.
- **Where savings actually concentrate:** long string literals — `"http://localhost:8000"`
  → `$API` saves **+6** tokens per occurrence, multi-byte copy like `"← Back to papers"`
  → `$BACK` saves **+3**. Hence the `$NAME` dictionary is the dominant lever.

Bytes and tokens diverge sharply (18.7% bytes vs 8.0% cl100k on the same corpus):
**measure both metrics; don't extrapolate one from the other.**

> **Boxed note — storage tokens vs generation tokens are different claims.**
> Everything above measures *storage* tokens: tokenize the file on disk, compare
> counts. Whether an LLM *generating* `.psc` actually spends fewer output tokens —
> and stays correct while doing so — is a separate experiment with its own failure
> modes (the model may pad, hedge, or mis-apply aliases). We never blend the two.
> Likewise, the 10–18% savings reported by the LOIR-adjacent papers (SimPy,
> ShortCoder, Token Sugar) are **fine-tune-dependent** — models trained or adapted
> on the compressed syntax — and are not comparable to zero-shot `.psc` emission,
> which is PythScribe's operating point.

## Generation tokens (measured)

The storage numbers above answer "is the stored artifact smaller?". The stronger LOIR
question is: does a model, asked to *write* `.psc`, actually emit fewer output tokens
while staying correct? We measure this **zero-shot** — a frontier model
given the authoring manual in its system prompt, no fine-tune (three models tested; the
table below is the primary `claude-opus-4-8` run, replicated on Sonnet and Haiku under
"Cross-model replication") —
on a ~50-task suite: 40 micro-tasks drawn from the CPython differential corpus
(natural-language prompt → byte-exact expected stdout) and 9 macro-tasks that build
real clone components. Each task runs in three conditions — Python, `.ps`, `.psc` — at
N=5, counting o200k tokens on the emitted code block and verifying correctness with the
**same oracle as the shipping differential runner** (CPython for micro; `pyths compile`
+ run for `.ps`; `pyths expand` + compile + run for `.psc`). Experiment `baseline-001`
(commit `d2d0a29`, 690 calls, $13.86); every number below regenerates from
`examples/cloudflare-bench/bench/ablations/ledger.jsonl` via `render_report.py`.

| Task class | Condition | Median o200k out | IQR | pass@1 | Tokens / correct |
|---|---|---:|---:|---:|---:|
| Micro (N≈197/cond) | Python | 27 | 17.5 | 1.00 | 32.4 |
| | `.ps` | 29 | 15 | 0.995 | 33.6 |
| | `.psc` | 26 | 12 | 0.99 | 31.4 |
| Macro / component (N≈44/cond) | `.ps` | 682 | 460.5 | 0.977 | 634.1 |
| | `.psc` | **538** | 394 | **1.00** | **510.3** |

**The result is scale-dependent, and honestly so.** On **component-scale** generation —
where import presets, kwarg/hook aliases, and the `$NAME` dictionary actually have
something to compress —
zero-shot `.psc` reduces output tokens *at equal-or-better correctness* (`.psc` pass@1
1.00 with zero syntax errors vs `.ps` 0.977). The aggregate macro medians differ by
**21%** (538 vs 682), but the **more robust paired per-task median is 17.4%** — we
headline the paired figure. Macro tasks have no plain-Python equivalent (React
components), so Python is N/A there. On **micro** (trivial one-liners) the effect is a
**wash**: Python 27, `.ps` 29, `.psc` 26 median tokens — all within one IQR, because
there is almost nothing to compress and the tokenizer already handles short Python well.

**Cross-model replication.** To check the effect is not specific to one model, we re-ran
the full suite unchanged on two more models of a different capability tier. The paired
macro result replicates on **all three** — 9/9 tasks favor `.psc`, per-task median
17–18% in every case, two-sided sign-test *p* = 0.004 throughout:

| Model | Macro `.psc`<`.ps` | Per-task median saving | 95% CI | `.psc` macro pass@1 |
|---|---|---:|---:|---:|
| `claude-opus-4-8` | 9/9 | 17.4% | [11.7, 23.3]% | 45/45 |
| `claude-sonnet-5` | 9/9 | 18.1% | [16.0, 26.2]% | 45/45 |
| `claude-haiku-4-5` | 9/9 | 17.8% | [4.4, 25.4]% | 43/45 |

The one honest degradation is at the small end: on `claude-haiku-4-5`, `.psc` macro pass@1
drops to 43/45 (two syntax errors — the smaller model occasionally mis-applies a tier)
while `.ps` stays 45/45, so for the weakest model the token saving carries a slight
correctness cost the larger two do not show. (Bootstrap *B*=2000; ledger exp_ids
`baseline-001` / `baseline-sonnet` / `baseline-haiku`.)

**The input-side caveat, never netted.** The authoring manual costs input tokens: the
condition system prompts measure **81 / 1981 / 2981** o200k tokens for Python / `.ps` /
`.psc`. So `.psc` carries ~2900 tokens of one-time input overhead. The ~150-token
per-component output saving only dominates when that overhead is amortized — via prompt
caching, or across enough generations in a session. We report input overhead and output
savings as separate columns and never blend them into a single "net" figure.

**Bottom line for the Token Economy row:** generation-token economy is now *measured*,
not asserted — a real ~20% output-token win at component scale with non-inferior
correctness, a wash on trivial snippets, and an input-overhead caveat that makes the
net win conditional on caching/volume. The gap-table status is **MET
(code-type-dependent)** — the win is real at component scale with non-inferior
correctness, a wash on trivial code, and net-positive only once the input-manual
overhead amortizes; the qualifier is the honesty, not a hedge.

## Behavioral correctness (measured)

The token medians above count every *compiling* sample, and compile success is ~100% for
both `.ps` and `.psc` — but a component can compile and still render wrongly. To test
actual behavior we built a **behavioral oracle**: each of the 90 generated macro
components (45 `.ps` + 45 `.psc` per model) is rendered in a headless DOM (jsdom + a React
testing harness), and a per-task spec drives its *pinned* behavior with tolerant role/text
queries — the like button toggles Like↔Liked, the counter increments and disables at 0,
the todo input clears after Add, the search filter narrows the list, and so on. A sample
passes only if it demonstrably implements the contract.

Behavioral `pass@1` is far below compile success (**51–78%** vs ~100%), and crucially
`.psc` **tracks** `.ps` within a few points on every model — compression costs a little
behavioral correctness, not much. Among the behaviorally-correct samples the `.psc` token
saving is if anything larger (Opus **22%**, Sonnet **20%** median o200k). The dominant
failure is **shared**, not a compression artifact: the models frequently emit the Python
loop-capture idiom `on_click=lambda i=i: f(i)`, which compiles faithfully to
`(i=i) => f(i)` but misfires because React invokes handlers *with* the event, so
`todo_list` and `kanban` fail on both tracks across all three models. One clean
`.psc`-specific loss (`movie_rows` on Opus, 5/5 `.ps` vs 1/5 `.psc`) traces to a
compressed helper component not receiving the React transform — a real dual-track finding
filed upstream.

**Repairability, demonstrated (manual, not compiler).** Both dominant failures are
authoring-convention violations that compile faithfully — legal PythScribe that misbehaves
under React's event model. Because the LOIR contract locates repair in the **authoring
manual**, not the compiler, we tested that lever: we added two lines to the `.ps`/`.psc`
manual (prefer `on_click=lambda: f(i)` over the loop-capture form; decorate helper
components that return elements) and regenerated all nine macro tasks with the identical
harness and models — **no change to `pyths_expand` or codegen**. Behavioral `pass@1` rose
on every model and both tracks: `.ps` by 18–22 points (to **76–98%**) and `.psc` by 7–18
points (to **58–89%**). This exercises Repairability end-to-end — a failure mode surfaced
by the oracle, closed by an edit to human-facing text.

| Model | `.ps` `pass@1`: v1 → v2 | `.psc` `pass@1`: v1 → v2 |
|---|---|---|
| `claude-opus-4-8` | 78% → 98% (44/45) | 71% → 89% (40/45) |
| `claude-sonnet-5` | 69% → 91% (41/45) | 67% → 84% (38/45) |
| `claude-haiku-4-5` | 58% → 76% (34/45) | 51% → 58% (26/45) |

Honest caveat: this is a **tolerant-spec** oracle (role/text queries that catch wrong
behavior), not reference-equivalence against the `.tsx` — a lenient spec could pass a
component that differs from the reference in unchecked ways, and 5 samples/task/model is a
small sample. Reference-equivalence behavioral correctness is future work.

## Roadmap

The three tracks the earlier drafts of this doc listed are now **done**, with results
folded into the requirement rows above:

- **Paper-table mining (done — negative):** SimPy, ShortCoder, and Token Sugar's
  transform tables (824 rules) were run through the per-occurrence o200k screen on the
  94,008-token combined reference-app+clones corpus (`examples/cloudflare-bench/bench/paper-mining-report.md`).
  Net new gain over the `$NAME`+Tier-E baseline: **≈0%**. All three papers' headline
  gains require retraining the model/tokenizer to absorb the shorthand as single vocab
  tokens; `.psc` operates against a fixed vocabulary at the tool boundary and cannot, so
  the BPE wall dominates. SimPy rewrites *regress* zero-shot, ShortCoder's rules are
  AST-altering (violate the Iron Rule), and only 3 of Token Sugar's 799 pairs transfer.
  The dominant lever remains the domain `$NAME` dictionary. (Ledger: `bench/ablations/`.)
- **Generation eval (done):** the measured 21% aggregate / 17–18% paired component-scale
  output-token win — see the "Generation tokens (measured)" section — plus a behavioral
  oracle (render + drive pinned behavior) showing 51–78% behavioral correctness with a
  manual-only repairability lift to 76–98% — see "Behavioral correctness (measured)".
- **Verified rewriting (done):** the Lean formalization now machine-checks determinism,
  zone-safety, and alias round-trip (`verification/`, 0 `sorry`, `propext`-only).
- **Constrained decoding (done):** `grammar/pyths.lark` verified as a sound acceptor +
  a live SynCode decoder demonstrated (`examples/constrained-decoding/`).

**Remaining (honest):** a reference-equivalence macro oracle beyond the current
tolerant-spec behavioral suite; broader cross-vendor replication (our three models are one
provider family); a hardened constrained decoder that never falls back to unconstrained
(SynCode/grammar robustness); and a mechanized refinement from the Lean model to the Rust
expander, plus extending the Lean model past the zone-partition boundary. None is an
architectural gap — all are polish or scope extensions.
