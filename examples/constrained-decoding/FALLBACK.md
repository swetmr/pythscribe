# The SynCode fallback: diagnosed, fixed, measured

> **Previous claim (now superseded):** "SynCode's incremental parser intermittently
> throws mid-decode on our grammar and falls back to *unconstrained* decoding …
> This is a SynCode↔grammar robustness gap."

It is not intermittent, it is not a mystery, and it is not a grammar bug. It is a
single, fully-characterised integration defect, and it is **fixed**.

| | incremental-parser throw rate on prefixes of valid `.ps` |
|---|---|
| **Before** | **52.7 %**  (270 / 512) |
| **After** | **0.0 %**  (0 / 512) |

Reproduce: `python examples/constrained-decoding/incparser_probe.py`

---

## What "fallback" actually is

SynCode masks logits by re-parsing the partial output at **every decoding step**:

```python
# syncode/grammar_mask/grammar_constrainer.py:206
try:
    res = self.inc_parser.get_acceptable_next_terminals(partial_output)
    ...
except Exception as e:                                   # :215
    ...
    logger.info("Parsing failed! Falling back to unconstrained decoding.")
    skip = True                                          # :225
```

`skip=True` means `mask_scores()` leaves the logits **untouched** — that step
decodes with **no grammar constraint at all**. So:

> **the fallback rate is exactly the rate at which SynCode's incremental parser
> throws on a partial program.**

That is a property of the parser and the grammar, *not* of the model — the model
only decides which partial programs come up. It is therefore measurable
**without a model, a GPU, or the 2 GB mask store**, by feeding the incremental
parser successive prefixes of known-valid `.ps` programs. That is
`incparser_probe.py`.

> Note: reading SynCode's log instead would badly **undercount**. `self.parse_failed`
> is a latch, so "Falling back to unconstrained decoding" is printed at most
> **once per generation** no matter how many steps actually lost the mask. The old
> report's "intermittent" reflects the latch, not the frequency.

## Root cause

```python
# syncode/parsers/__init__.py : create_parser
if grammar.name == 'python' and not use_symbol_pos_map:
    indenter = PythonIndenter()
...
if grammar.name == 'python':
    return PythonIncrementalParser(base_parser, indenter, **kwargs)
return incremental_parser.IncrementalParser(base_parser, **kwargs)
```

```python
# syncode/parsers/grammars/grammar.py : Grammar.__init__
elif name.endswith('.lark'):
    ...
    self.name = name          # <- the FILE PATH
```

We hand SynCode a `.lark` **path**, so `grammar.name` is that path, so
`grammar.name != 'python'`, so:

1. **`indenter` is `None`.** `_INDENT` / `_DEDENT` are therefore **never emitted**.
   Our grammar — like Python's — needs them: `suite: _NL _INDENT stmt+ _DEDENT`.
   Every indented block is consequently unparseable, the incremental parser
   throws, and the decoder silently drops the mask.
2. the generic `IncrementalParser` is used instead of `PythonIncrementalParser`,
   which is the class that manages the INDENT/DEDENT queue;
3. `Grammar.simplifications()` returns `{}`, so the `LONG_STRING` lookbehind is
   never simplified away — this is the `interegular` *"lookbacks are not
   implemented"* error the old harness worked around by hand-rewriting the regex.

**Indentation support is keyed to the literal string `'python'` and is not
exposed as an option.** A custom indentation-sensitive grammar can never obtain
an indenter through the public API.

`Syncode(..., indent=True)` — which the old harness passed, reasonably expecting
it to handle indentation — does **not** help: it is forwarded only to the
**mask store**'s indentation→token map
(`grammar_constrainer.py:86` → `MaskStore.init_mask_store(..., indent=indent)`).
It is never passed to `create_parser`. The incremental parser still has no
indenter.

### The evidence matches exactly

Every baseline failure is at **line 2, column 5** — the first token *after the
first indent* — and the throw rate is **0 % for every program with no indented
block** and **68–77 % for every program that has one**:

| program | baseline | fixed |
|---|---|---|
| `flat_assign` | 0.0 % | 0.0 % |
| `def_oneline` (one-line suite) | 0.0 % | 0.0 % |
| `comprehension` | 0.0 % | 0.0 % |
| `ps_operators` (`?.`, `??`) | 0.0 % | 0.0 % |
| `def_indented` | 18.8 % | **0.0 %** |
| `if_indented` | 68.5 % | **0.0 %** |
| `nested_blocks` | 75.7 % | **0.0 %** |
| `class_method` | 76.4 % | **0.0 %** |
| `with_try` | 76.5 % | **0.0 %** |
| **total** | **52.7 %** (270/512) | **0.0 %** (0/512) |

This is why the one committed success (`constrained_focused.py`, sample 0) was a
flat two-line program: flat programs are precisely the ones that never trip the
bug.

## The fix

Entirely on our side — **SynCode is not patched, and the canonical grammar and its
CI gates are untouched.** See `syncode_grammar.py`:

1. present the grammar under **`name = 'python'`** — the switch that attaches the
   indenter, `PythonIncrementalParser`, and the terminal simplifications; and
2. rename `_NEWLINE` → **`_NL`** in the decoder-facing copy, because SynCode's
   `PythonIndenter` hard-codes `NL_type = "_NL"`
   (`syncode/parsers/python_parser.py:178`).

`Grammar.hash()` keys off the grammar **body**, so the mask store stays correctly
keyed to our grammar — only the label changes. Adaptation (2) also makes the old
hand-written `LONG_STRING` patch unnecessary, since the Python simplifications now
apply.

## What is established, and what is not

**Established.** The fallback is a deterministic consequence of SynCode's indenter
being gated on a hard-coded grammar name; it fires on every step inside an
indented block; and the two-line adaptation above drives the incremental-parser
throw rate — which *is* the fallback condition — from 52.7 % to **0 % (0/512)**.

**Not established here.** An **end-to-end generation run** (≥50 completions, with
per-step fallback counts and grammar-valid / `pyths check`-valid / compiles
figures) was **not executed**, so no live validity numbers are claimed. The
harness is written, committed and re-runnable (`constrained_measure.py`, which
instruments `_parse_partial_output` and counts every `skip=True` rather than
trusting the latched log) — but SynCode's DFA mask store for our grammar over
Qwen's 151,936-token vocabulary is ~2 GB, and the machine this was developed on
had ~2.3 GB of free disk; Windows expanded the pagefile to back the build's commit
charge and drove free space to zero twice. Run it on a box with ≥6 GB free:

```bash
cargo build --release --workspace
HF_CACHE=…/cache/ SYNCODE_CACHE=…/cache/ \
  python examples/constrained-decoding/constrained_measure.py --n 60 --mode both
```

`--mode both` reports baseline and fixed side by side.

**Out of scope by construction.** Even at a 0 % fallback rate, constrained decoding
guarantees only that the output **parses**. See `syntactic_boundary.py` for a
program that is grammar-valid, `pyths check`-valid (type checker included) and
compiles, yet is complete nonsense.
