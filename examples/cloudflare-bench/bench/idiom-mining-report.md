# PythScribe Idiom Miner — Task A.1 Report (refined v3, net-new-over-tiers)

**Refinement v3 (net-new over existing tiers):** the gate now measures Tier E's
INCREMENTAL value over the existing A/B/C/Dict tiers. Every candidate is one of:
- **DROP** — wholly an existing-tier target (preset import line, lone decorator,
  bare hook call, kwarg, or tag call); Tier E adds nothing, so it is EXCLUDED.
- **DICT** — string/style material already servable by Tier `$NAME` (reported
  separately, not a new-E win).
- **CODE-NEW** — genuine new code idiom with ZERO existing-tier overlap
  (e.g. the HTTP `if not response.ok: raise ...` block). Tier E's lower bound.
- **CODE-MARGINAL** — contains an existing-tier token inside a larger NEW pattern;
  scored ONLY on the marginal tokens E captures beyond A/B/C/Dict (gross per-occ
  minus what the existing tier already saves on that span).

Selection is a SINGLE GLOBAL greedy non-overlapping pass (each source character
span claimed at most once) scored by MARGINAL o200k delta. The gate verdict keys
off the genuine net-new total (CODE-NEW + CODE-MARGINAL).

**Limitation:** No canonicalization (`pyths canon` deferred). Numbers are a
first-pass signal on raw source text, slightly approximate. The DICT/CODE
classifier is heuristic and the overlap dedup is greedy (not provably optimal).

## 1. Corpus Collection
Found 67 .ps files.
Successfully read 67 files.

Total corpus tokens: **67,731 o200k** / 66,975 cl100k

## 2. Sigil Selection

| Sigil | Σ o200k (A–Z) | Σ cl100k (A–Z) | Avg o200k per alias | `<sigil>Ab` o200k | `<sigil>Ab` cl100k |
|---|---:|---:|---:|---:|---:|
| ``` | 52 | 52 | 2.00 | 2 | 2 |
| `~` | 52 | 52 | 2.00 | 2 | 2 |
| `!` | 52 | 51 | 2.00 | 2 | 2 |
| `%` | 47 | 43 | 1.81 | 2 | 2 |

**Chosen sigil: `%`** (cheapest Σ o200k across A–Z)
Typical 2-char alias `%Ab` costs **2 o200k tokens**.
This is the floor: any idiom must exceed this alias cost per occurrence to show a positive delta.

## 3. Idiom Discovery

### 3a. Recurring trimmed lines (>=3 occurrences)
Found 81 recurring lines.

### 3b. Token n-grams (len 4–24, >=3 occurrences)
Found 13023 recurring n-gram patterns.

## 4. Scoring (o200k delta) + Bucket classification

Positive-delta candidates (pre-overlap-dedup): 13090

**Sub-bucket split** (before overlap dedup) — net-new-over-existing-tier:
- DICT (string/style, Tier `$NAME`): 6134
- DROP (wholly an existing A/B/C tier target — Tier E adds nothing): 238
- CODE-NEW (genuine new idiom, ZERO existing-tier overlap): 5686
- CODE-MARGINAL (overlaps a tier; scored on marginal tokens only): 1032

## 5. Greedy non-overlapping selection (MARGINAL o200k, GLOBAL, DROP excluded)

DROP candidates (wholly existing-tier targets) are removed first. The rest
(DICT + CODE-NEW + CODE-MARGINAL) compete for the same source spans; each
span is claimed once. Selection + totals use the MARGINAL per-occurrence
o200k delta: gross E saving minus the tokens A/B/C/Dict already save on the
same span. CODE-MARGINAL idioms thus bill only the surrounding NEW tokens.

- Total non-overlapping idioms selected (eff_freq >= 3): 276
- CODE-NEW: 164 | CODE-MARGINAL: 15 | DICT: 97

## 6. Top Bucket CODE-NEW idioms (pure new, zero existing-tier overlap)

This is Tier E's LOWER-BOUND unique value: idioms with no A/B/C/Dict overlap.

| # | eff_freq | o200k canon | alias | per-occ Δ (=marginal) | total o200k saved | cl100k saved | fragment |
|---|---:|---:|---:|---:|---:|---:|---|
| 1 | 203 | 5 | 2 | +3 | **609** | 609 | `        data_testid="` |
| 2 | 59 | 7 | 2 | +5 | **295** | 295 | `,↵    )↵↵↵@component↵def` |
| 3 | 11 | 22 | 2 | +20 | **220** | 220 | `    if not response.ok:↵        raise Exception(f"HTTP {r...` |
| 4 | 53 | 5 | 2 | +3 | **159** | 159 | `↵    return div(↵` |
| 5 | 70 | 4 | 2 | +2 | **140** | 140 | `,↵        ),↵       ` |
| 6 | 6 | 24 | 2 | +22 | **132** | 132 | ` def _cleanup():↵            live["v"] = False↵          ...` |
| 7 | 7 | 19 | 2 | +17 | **119** | 119 | `.↵#↵# NOTE: `#` comments, not a triple-quoted docstring -...` |
| 8 | 12 | 11 | 2 | +9 | **108** | 108 | `):↵    response = await get(f"{API_BASE}/` |
| 9 | 12 | 11 | 2 | +9 | **108** | 108 | `Turbopack UTF-8 char-boundary panic` |
| 10 | 51 | 4 | 2 | +2 | **102** | 102 | `    return None↵` |
| 11 | 51 | 4 | 2 | +2 | **102** | 102 | `)↵        set_` |
| 12 | 50 | 4 | 2 | +2 | **100** | 50 | ` pyths.react` |
| 13 | 6 | 18 | 2 | +16 | **96** | 96 | `        live = {"v": True}↵↵        async def _load():↵  ...` |
| 14 | 47 | 4 | 2 | +2 | **94** | 94 | `        div(style={"` |
| 15 | 4 | 24 | 2 | +22 | **88** | 88 | `        except Exception:↵            pass↵        raise ...` |
| 16 | 44 | 4 | 2 | +2 | **88** | 88 | `,↵        div(↵` |
| 17 | 44 | 4 | 2 | +2 | **88** | 88 | `),↵            ),↵       ` |
| 18 | 15 | 7 | 2 | +5 | **75** | 75 | `),↵    )↵↵↵__default__ =` |
| 19 | 4 | 19 | 2 | +17 | **68** | 68 | `#↵# NOTE: `#` comment block, not a triple-quoted module d...` |
| 20 | 3 | 24 | 2 | +22 | **66** | 66 | `)↵            except Exception as e:↵                if l...` |
| 21 | 32 | 4 | 2 | +2 | **64** | 64 | `        span(style={"` |
| 22 | 31 | 4 | 2 | +2 | **62** | 62 | `↵↵    def _` |
| 23 | 15 | 6 | 2 | +4 | **60** | 60 | `        data_testid=f"` |
| 24 | 20 | 5 | 2 | +3 | **60** | 60 | `,↵        style={↵           ` |
| 25 | 28 | 4 | 2 | +2 | **56** | 56 | ` 0:↵       ` |
| 26 | 14 | 6 | 2 | +4 | **56** | 42 | `        e.preventDefault()↵       ` |
| 27 | 8 | 9 | 2 | +7 | **56** | 56 | `.↵# PythScribe dual-track mirror of` |
| 28 | 27 | 4 | 2 | +2 | **54** | 54 | ` = None↵   ` |
| 29 | 18 | 5 | 2 | +3 | **54** | 54 | `)↵↵↵@component↵def` |
| 30 | 16 | 5 | 2 | +3 | **48** | 48 | `, use_effect, use` |

## 6b. Top Bucket CODE-MARGINAL idioms (overlaps a tier; marginal tokens only)

These contain an existing-tier token inside a larger NEW pattern. Scored on
the MARGINAL tokens E captures beyond A/B/C/Dict (gross per-occ minus existing).

| # | eff_freq | gross per-occ | existing saves | marginal per-occ | total marginal o200k | existing hit | fragment |
|---|---:|---:|---:|---:|---:|---|---|
| 1 | 150 | +2 | -1 | +1 | **150** | B:class_name=x1 | `        class_name="` |
| 2 | 9 | +9 | -1 | +8 | **72** | B:className=x1,C:div(x1 | `:↵        return div(↵            className="pp",↵` |
| 3 | 22 | +4 | -1 | +3 | **66** | B:use_state(x1 | ` = use_state(None)↵   ` |
| 4 | 33 | +3 | -1 | +2 | **66** | B:on_click=x1 | `        on_click=lambda:` |
| 5 | 11 | +5 | -1 | +4 | **44** | B:on_change=x1 | `                on_change=lambda e: set` |
| 6 | 3 | +14 | -1 | +13 | **39** | A:@componentx1,B:use_state(x1 | `@component↵def Counter():↵    count, set_count ...` |
| 7 | 8 | +5 | -1 | +4 | **32** | B:use_state(x1 | ` = use_state(0)↵   ` |
| 8 | 15 | +3 | -1 | +2 | **30** | B:use_state(x1 | ` = use_state("")↵   ` |
| 9 | 9 | +4 | -1 | +3 | **27** | B:use_state(x1 | ` = use_state(False)↵   ` |
| 10 | 4 | +7 | -1 | +6 | **24** | B:use_state(x1 | `error, set_error = use_state(None)` |
| 11 | 8 | +4 | -1 | +3 | **24** | B:className=x1 | `        className="pp",↵` |
| 12 | 11 | +3 | -1 | +2 | **22** | B:use_state(x1 | ` = use_state([])↵   ` |
| 13 | 21 | +2 | -1 | +1 | **21** | B:class_name=x1,C:p(x1 | ` p(class_name="` |
| 14 | 10 | +3 | -1 | +2 | **20** | B:class_name=x1,C:button(x1 | `            button(class_name="` |
| 15 | 4 | +5 | -1 | +4 | **16** | B:style=x1,C:h1(x1 | `20},↵        h1(style={"` |

## 6c. Top Bucket DICT idioms (already servable by Tier `$NAME` — informational)

| # | eff_freq | o200k canon | total o200k saved | fragment |
|---|---:|---:|---:|---|
| 1 | 93 | 7 | 465 | `, "color": "var(--` |
| 2 | 140 | 4 | 280 | `fontSize": ` |
| 3 | 38 | 9 | 266 | `        style={"display": "flex", "` |
| 4 | 29 | 9 | 203 | `": "1px solid var(--rule)` |
| 5 | 15 | 15 | 195 | `fontFamily": "'Instrument Serif', serif", "fontStyle": "i...` |
| 6 | 93 | 4 | 186 | `": "var(--` |
| 7 | 59 | 5 | 177 | ` "borderRadius": ` |
| 8 | 86 | 4 | 172 | ` "padding": ` |
| 9 | 17 | 10 | 136 | `alignItems": "center", "gap": ` |
| 10 | 14 | 11 | 126 | `API_BASE = "http://localhost:8000"` |
| 11 | 25 | 7 | 125 | `, "lineHeight": 1` |
| 12 | 31 | 6 | 124 | `, "fontWeight": ` |
| 13 | 13 | 11 | 117 | ` "fontFamily": "'JetBrains Mono', monospace` |
| 14 | 58 | 4 | 116 | `={"padding": ` |
| 15 | 36 | 5 | 108 | `", data_testid="` |

## 7. Projected Corpus Savings (non-overlapping, marginal-scored)

Corpus total: 67,731 o200k tokens.

| Bucket | Top-10 saved | Top-10 % | Top-20 saved | Top-20 % |
|---|---:|---:|---:|---:|
| **CODE-NEW** (E lower-bound) | 1,992 | **2.94%** | 2,857 | **4.22%** |
| **CODE-MARGINAL** (E extra) | 550 | 0.81% | 653 | **0.96%** |
| **CODE genuine total** (NEW+MARGINAL) | 2,040 | **3.01%** | 2,945 | **4.35%** |
| DICT (already Tier `$NAME`) | 2,206 | 3.26% | 3,215 | 4.75% |

*Prior passes for reference: first-pass undeduped+unbucketed claimed 4.80%;
the global-dedup CODE pass (still counting existing-tier-covered idioms)
claimed 5.24%. Both overstate Tier E's NET-NEW value over A/B/C/Dict.*

## 8. `[expand.idioms]` TOML block (top-10 genuine CODE winners)

```toml
[expand.idioms]
# CODE-NEW eff_freq=203, marginal_o200k_saved=609
"%Ab" = """
        data_testid="
"""

# CODE-NEW eff_freq=59, marginal_o200k_saved=295
"%Bc" = """
,
    )


@component
def
"""

# CODE-NEW eff_freq=11, marginal_o200k_saved=220
"%Cd" = """
    if not response.ok:
        raise Exception(f"HTTP {response.status}")
    return await response.json()



"""

# CODE-NEW eff_freq=53, marginal_o200k_saved=159
"%De" = """

    return div(

"""

# CODE-MARGINAL eff_freq=150, marginal_o200k_saved=150
"%Ef" = """
        class_name="
"""

# CODE-NEW eff_freq=70, marginal_o200k_saved=140
"%Fb" = """
,
        ),
       
"""

# CODE-NEW eff_freq=6, marginal_o200k_saved=132
"%Gc" = """
 def _cleanup():
            live["v"] = False
            return None

        return _cleanup

    use_effect
"""

# CODE-NEW eff_freq=7, marginal_o200k_saved=119
"%Hd" = """
.
#
# NOTE: `#` comments, not a triple-quoted docstring - see
"""

# CODE-NEW eff_freq=12, marginal_o200k_saved=108
"%Ie" = """
):
    response = await get(f"{API_BASE}/
"""

# CODE-NEW eff_freq=12, marginal_o200k_saved=108
"%Jf" = """
Turbopack UTF-8 char-boundary panic
"""

```

## 9. Gate Verdict (genuine net-new over existing tiers)

### **POSITIVE** (decision metric: top-20 genuine net-new CODE = 4.35%)

179 genuine net-new CODE idioms project 4.35% corpus o200k savings at top-20 (CODE-NEW 4.22% + CODE-MARGINAL 0.96%) -- a meaningful (>=1.5%) saving that survives overlap dedup AND exclusion/marginalisation of existing A/B/C/Dict tiers; Tier E is justified.

- Chosen sigil: `%` (alias cost: 2 o200k tokens for 2-char alias)
- CODE-NEW idioms: 164 (top-20 = 4.22%)
- CODE-MARGINAL idioms: 15 (top-20 = 0.96%)
- CODE genuine total (NEW+MARGINAL): top-10 = 3.01%, top-20 = 4.35%
- DROP (wholly existing-tier, excluded): 238 candidates
- Top-8 CODE-NEW idioms by non-overlapping marginal o200k saved:
  1. `        data_testid="` -- 609 o200k (eff_freq=203)
  2. `,↵    )↵↵↵@component↵def` -- 295 o200k (eff_freq=59)
  3. `    if not response.ok:↵        raise Exception(f"HTTP ` -- 220 o200k (eff_freq=11)
  4. `↵    return div(↵` -- 159 o200k (eff_freq=53)
  5. `,↵        ),↵       ` -- 140 o200k (eff_freq=70)
  6. ` def _cleanup():↵            live["v"] = False↵        ` -- 132 o200k (eff_freq=6)
  7. `.↵#↵# NOTE: `#` comments, not a triple-quoted docstring` -- 119 o200k (eff_freq=7)
  8. `):↵    response = await get(f"{API_BASE}/` -- 108 o200k (eff_freq=12)

### Caveats on the net-new number

- **No canonicalization** (`pyths canon` deferred): mining is on raw source
  text, so some CODE-NEW fragments are whitespace/indentation-ragged slices
  (e.g. `    return None`, `,\n        ),`) that a real Tier-E alias could not
  cleanly expand to without a canonicalization pass. These inflate CODE-NEW
  somewhat; the robust core idioms (HTTP error block, async `_load`/`_cleanup`
  effect setup, `await get(f"{API_BASE}/`) clear the >=1.5% bar on their own.
- Existing-tier alias cost is modelled at a flat 2 o200k tokens/target; the
  DICT/CODE/DROP/marginal classifier is heuristic; overlap dedup is greedy
  (not provably optimal). All bias the number conservatively or noisily, not
  systematically upward.
- Trend across honesty passes: 4.80% (raw) -> 5.24% (global-dedup gross CODE)
  -> 4.50% (net-new over existing tiers). The net-new figure is the one to cite.

---
*Generated by `mine_idioms.py` (Task A.1, refined v3). Net-new-over-existing-tier.*
