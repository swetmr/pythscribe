# W1 — Paper transforms vs `.psc`: BPE-wall / applicability / Iron-Rule screen

Corpus: **106 `.ps` files** (reference-app frontend + frontend-next + examples/clones + examples), **94,008 o200k** / 92,968 cl100k tokens.
Alias-cost floor (our `%Ab` sigil): **2 o200k** / 2 cl100k per occurrence.

**Fixed-vocab, zero-shot.** The papers' headline gains require RETRAINING the
model/tokenizer to absorb the shorthand as single vocab tokens. `.psc` cannot
retrain a frontier model; it operates at the tool boundary against a FIXED o200k
vocab. Every delta below is the zero-shot delta — the only thing `.psc` realises.

## SimPy (arXiv 2404.16333) — grammar/whitespace compaction

| rule | kind | pattern → replacement | o200k Δ/occ | verdict |
|---|---|---|---:|---|
| `simpy-kw-def` | keyword | `def ` → `<def_stmt>` | -2 | DROP — regresses/neutral zero-shot (BPE wall) |
| `simpy-kw-class` | keyword | `class ` → `<class_stmt>` | -1 | DROP — regresses/neutral zero-shot (BPE wall) |
| `simpy-kw-if` | keyword | `if ` → `<if_stmt>` | -2 | DROP — regresses/neutral zero-shot (BPE wall) |
| `simpy-kw-for` | keyword | `for ` → `<for_stmt>` | -2 | DROP — regresses/neutral zero-shot (BPE wall) |
| `simpy-kw-while` | keyword | `while ` → `<while_stmt>` | -2 | DROP — regresses/neutral zero-shot (BPE wall) |
| `simpy-kw-with` | keyword | `with ` → `<with_stmt>` | -2 | DROP — regresses/neutral zero-shot (BPE wall) |
| `simpy-kw-try` | keyword | `try:` → `<try_stmt>` | -2 | DROP — regresses/neutral zero-shot (BPE wall) |
| `simpy-kw-return` | keyword | `return ` → `<return_stmt>` | -2 | DROP — regresses/neutral zero-shot (BPE wall) |
| `simpy-op-ge` | operator | `>=` → `<ge>` | -2 | DROP — regresses/neutral zero-shot (BPE wall) |
| `simpy-op-arrow` | operator | `->` → `<arrow>` | -2 | DROP — regresses/neutral zero-shot (BPE wall) |
| `simpy-colon-def` | block | `def NAME(params):` → `<def_stmt> NAME ` | n/a | DROP — normalised by canonical printer (Iron Rule) |
| `simpy-colon-if` | block | `if EXPR:` → `<if_stmt> EXPR` | n/a | DROP — normalised by canonical printer (Iron Rule) |
| `simpy-block` | block | `NEWLINE INDENT ... DED` → `<block_start> ..` | n/a | DROP — normalised by canonical printer (Iron Rule) |
| `simpy-ws-indent` | whitespace | `<indent/newline/contin` → `<elided>` | n/a | DROP — normalised by canonical printer (Iron Rule) |
| `simpy-linesep` | line-sep | `stmt NEWLINE` → `stmt [<line_sep>` | n/a | DROP — normalised by canonical printer (Iron Rule) |

**SimPy verdict: REJECTED for `.psc`.** Whitespace/line-sep/block rules are
structurally inapplicable — the canonical printer already normalises styling
whitespace, so there is nothing to save (the Iron Rule round-trips through the
canonical form). Keyword/operator rules regress hard zero-shot: the `<def_stmt>`
angle-bracket tokens fragment into MANY o200k pieces (a retrained-vocab win only).

## ShortCoder (arXiv 2601.09703) — 10 AST-altering simplification rules

| rule | before → after | o200k Δ | Iron-Rule feasible? |
|---|---|---:|---|
| `sc-01-multi-assign` | `a = 5; b = 5` → `a = b = 5` | +3 | NO — changes canonical AST |
| `sc-02-return-parens` | `return (x + y)` → `return x + y` | +2 | NO — changes canonical AST |
| `sc-03-aug-assign` | `x = x + y` → `x += y` | +2 | NO — changes canonical AST |
| `sc-04-ternary` | `if c:;  x = a; else:;  x = b` → `x = a if c else b` | +9 | NO — changes canonical AST |
| `sc-05-elif-chain` | `nested if-else` → `elif chain` | +2 | NO — changes canonical AST |
| `sc-06-listcomp` | `result = []; for i in items:;  res` → `result = [i*2 for i in i` | +5 | NO — changes canonical AST |
| `sc-07-multi-del` | `del a; del b` → `del a, b` | +1 | NO — changes canonical AST |
| `sc-08-dict-get` | `if k in d:;  v = d[k]; else:;  v =` → `v = d.get(k, default)` | +11 | NO — changes canonical AST |
| `sc-09-str-format` | `"str" + var + "end"` → `"str{0}end".format(var)` | -1 | NO — changes canonical AST |
| `sc-10-with-open` | `f = open(p); data = f.read(); f.cl` → `with open(p) as f:;  dat` | -1 | NO — changes canonical AST |

**ShortCoder verdict: REJECTED as a `.psc` tier.** These are semantics-preserving
but AST-*altering* SOURCE rewrites (`x = x + y` → `x += y`). The Iron Rule requires
`pyths expand foo.psc` to be BYTE-IDENTICAL to the canonical `.ps` the author would
write; a shorthand that expands to a *different* AST than the author's source is
not a reversible alias — it is a canonical-STYLE choice (like the curried-vs-flat
PSX canonical-style choice), out of scope for the expander. NOTE: unlike
SimPy/Token-Sugar these rules DO genuinely cut o200k tokens (see +Δ column, up to +11)
— they are real code simplifications. They are rejected purely on Iron-Rule grounds,
NOT the BPE wall. The transferable takeaway is canonical-`.ps` STYLE guidance (author
`x += y`, comprehensions, `dict.get(k,default)` directly in `.ps`), not a `.psc` tier.

## Token Sugar (arXiv 2512.08266) — 799 mined pairs

- Total mined pairs: **799** (dataset: LeetCode Python, mined on the paper's retrained tokenizer)
- Wildcard-FREE (candidate for our fixed `%NAME` idiom): **57** (7.1%). The other 742 carry `SUGARWILDCARD_N` slots and are
  **Iron-Rule-INFEASIBLE** as fixed idioms — our `%NAME` model has no parameterised
  macro form, so the variable argument content cannot be absorbed reversibly.

### Stage (a) BPE-wall screen (fixed-skeleton o200k − 2-token alias)

- Pairs whose FIXED skeleton clears the alias-cost floor (per-occ o200k > 0): **756/799**.
- (This is a GENEROUS upper bound: it credits skeleton tokens even to wildcard
  patterns that are Iron-Rule-infeasible, and ignores that the model must still
  emit the wildcard argument content next to the alias.)

### Stage (b+c) corpus applicability + Iron-Rule feasibility

- Of the 756 BPE-survivors, **136** occur >=3x in OUR corpus
  (the LeetCode-algorithmic patterns — `class Solution:`, `for i in range(n):`,
  `SUGARWILDCARD_0 += 1` — are largely absent from React-frontend `.psc`).
- Of those, **3** are ALSO wildcard-free (Iron-Rule-feasible as `%NAME`).

Top corpus-applicable Token Sugar patterns (generous skeleton scoring):

| pattern | corpus freq | skeleton o200k | per-occ o200k | wildcard-free? | total o200k (gross) |
|---|---:|---:|---:|:--:|---:|
| `SUGARWILDCARD_0 = SUGARWILDCARD_1↵SUGARW` | 292 | 4 | +2 | NO (infeasible) | 584 |
| `SUGARWILDCARD_0 = SUGARWILDCARD_1↵SUGARW` | 292 | 4 | +2 | NO (infeasible) | 584 |
| `if SUGARWILDCARD_0 == SUGARWILDCARD_1:` | 115 | 4 | +2 | NO (infeasible) | 230 |
| `if SUGARWILDCARD_0 == SUGARWILDCARD_1:↵ ` | 34 | 7 | +5 | NO (infeasible) | 170 |
| `SUGARWILDCARD_0 = SUGARWILDCARD_1[SUGARW` | 79 | 4 | +2 | NO (infeasible) | 158 |
| `@SUGARWILDCARD_0↵def SUGARWILDCARD_1(SUG` | 157 | 3 | +1 | NO (infeasible) | 157 |
| `SUGARWILDCARD_0 = [SUGARWILDCARD_1 for S` | 35 | 6 | +4 | NO (infeasible) | 140 |
| `@SUGARWILDCARD_0↵def SUGARWILDCARD_1(SUG` | 67 | 4 | +2 | NO (infeasible) | 134 |
| `def SUGARWILDCARD_0(SUGARWILDCARD_1, SUG` | 131 | 3 | +1 | NO (infeasible) | 131 |
| `for SUGARWILDCARD_0 in SUGARWILDCARD_1:` | 62 | 4 | +2 | NO (infeasible) | 124 |
| `@SUGARWILDCARD_0↵def SUGARWILDCARD_1(SUG` | 37 | 5 | +3 | NO (infeasible) | 111 |
| `def SUGARWILDCARD_0(SUGARWILDCARD_1, SUG` | 54 | 4 | +2 | NO (infeasible) | 108 |
| `if SUGARWILDCARD_0 == 0:` | 24 | 6 | +4 | NO (infeasible) | 96 |
| `SUGARWILDCARD_0 = SUGARWILDCARD_1[SUGARW` | 47 | 4 | +2 | NO (infeasible) | 94 |
| `SUGARWILDCARD_0 = SUGARWILDCARD_1[SUGARW` | 47 | 4 | +2 | NO (infeasible) | 94 |

### Realisable Token Sugar gain (wildcard-free ∩ applicable ∩ BPE-positive)

| pattern | corpus freq | per-occ o200k | total o200k |
|---|---:|---:|---:|
| `[0]` | 37 | +1 | 37 |
| `[1]` | 9 | +1 | 9 |
| `return 0` | 3 | +1 | 3 |

**Realisable Token Sugar corpus saving: 49 o200k = 0.052%** of the 94,008-token corpus.

## Verdict — do the published papers transfer?

### **REJECTED / BPE-wall confirmed** (realisable published-pair gain = 0.052% o200k)

- **SimPy:** rejected — whitespace rules normalised away by the canonical printer;
  keyword/operator rules regress zero-shot (retrained-vocab-only win).
- **ShortCoder:** rejected as an expander tier — AST-altering rewrites violate the
  Iron Rule (not reversible fixed aliases); a canonical-STYLE question at most.
- **Token Sugar:** REJECTED / BPE-wall confirmed. Its 799 pairs are Python-general (LeetCode-mined) and
  parameterised via a RETRAINED tokenizer. Zero-shot on a fixed o200k vocab, only
  the fixed skeleton is collapsible; only 57 pairs are wildcard-free, and only
  3 of those both occur in our React corpus AND clear the BPE floor.
  Its METHODOLOGY (mine YOUR corpus for high-frequency token-heavy patterns) is the
  real transferable idea — and we already run it (`mine_idioms.py` → Tier E `%NAME`).

**The dominant lever remains the domain `$NAME` dictionary** (long string literals /
multi-byte CSS/URL values that genuinely fragment into many BPE pieces). The papers'
published Python-general pairs do NOT transfer to a fixed frontier tokenizer.

---

## Token Sugar's METHODOLOGY applied to OUR domain (the real opportunity)

Token Sugar's transferable idea is not its 799 LeetCode pairs but its *method*:
mine YOUR corpus for high-frequency, token-heavy patterns. We already run exactly
this — `mine_idioms.py` (net-new-over-existing-tiers, o200k-scored, sigil `%`).

Re-run on the reference-app + clones `.ps` corpus (94k o200k):

| Measure | Value |
|---|---|
| Raw-text upper bound (top-20 CODE-NEW+MARGINAL) | **4.35%** o200k |
| Round-trip + clean-form REALIZED (shipped Tier E `%HTTP_OK`) | **−0.58%** o200k |
| Realized with `$NAME` dictionary expansion added | **−2.32%** o200k combined |

**Why the gap (4.35% → 0.58%):** most raw-mined "wins" are (a) whitespace/
indentation-ragged slices a fixed `%NAME` idiom cannot cleanly expand to without a
canonicalization pass, (b) string/style material already collapsed by Tier `$NAME`,
or (c) already covered by Tiers A/B/C. Only genuine multi-token *code* idioms
(the HTTP error block) survive the Iron-Rule round-trip and the clean-form filter.
This mirrors the honest realized figure already documented in `docs/compression.md`.

**No new tier is warranted by W1.** The three published papers contribute zero
Iron-Rule-feasible net-positive idioms; the domain-mining ceiling is already
realized by the shipped Tier E. The dominant lever remains the domain `$NAME`
dictionary (long string / multi-byte CSS/URL literals that genuinely fragment
into many BPE pieces).

## The 3 "realisable" published pairs — explicitly rejected

`[0]` (freq 37, +1), `[1]` (freq 9, +1), `return 0` (freq 3, +1) are the only
wildcard-free Token Sugar pairs that both occur in-corpus and clear the BPE floor.
Aliasing a 3-character subscript like `[0]` to a `%NAME` sigil is net-noise: it
costs teaching-table surface, harms readability, and the +1 o200k/occ is inside
measurement noise. **Rejected** — they are the BPE wall in miniature.

## By-products

- `papers/transforms.jsonl` — 824 normalized transform rows (15 SimPy + 10
  ShortCoder + 799 Token Sugar), the audit trail for this screen.
- `papers/tokensugar_mined_sugars.json` — the fetched 799-pair artifact (provenance).
- `../corpus/ps_psc_pairs.jsonl` — 9 Iron-Rule-verified `{ps,psc}` pairs: the 7
  clones plus `large-samples/{app_1000,dashboard_500}.psc`, which now pass
  `--verify` (the drift noted here previously has since been fixed).
  (`tests/b029_worker` is still skipped: its sibling `.ps` does not parse.)

## GATE VERDICT summary (per candidate)

| Candidate | Realisable o200k gain | Gate |
|---|---:|---|
| SimPy (2404.16333) | 0% (normalised away / retrained-vocab-only) | **REJECTED** |
| ShortCoder (2601.09703) | tokens DO drop, but Iron-Rule-infeasible (AST rewrite) | **REJECTED (as tier); adopt as `.ps` style** |
| Token Sugar published 799 pairs | 0.052% | **REJECTED (BPE wall)** |
| Token Sugar *methodology* (domain mine) | −0.58% realized (already shipped Tier E) | ACCEPTED — no NEW tier |
| `[0]` / `[1]` / `return 0` micro-pairs | ~0 (noise) | **REJECTED** |

**Bottom line: no substantial gains. BPE wall confirmed.** The published Python-
general pairs do not transfer to a fixed frontier tokenizer; the only working lever
(domain corpus mining) is already realized by the existing `$NAME` + Tier E surface.
ShortCoder's rules genuinely save tokens but are canonical-`.ps` *style* guidance,
not reversible `.psc` aliases.
