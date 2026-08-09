# Kani — bounded model checking of the zone classifier

> **Epistemic status, stated once and precisely.** Kani is a **bounded model
> checker** (CBMC-based). Each harness is *exhaustive over every input up to a
> stated bound N* — not a sample, not a corpus, not a proof. It is **strictly
> stronger than the differential corpus** (2,039 fixed cases: those cases only)
> and **strictly weaker than the Lean proofs** (unbounded, but about a *model*).
> A green Kani run **does not license the word "verified"**. It licenses exactly:
> *"no input of at most N bytes falsifies this property."*

## Why this layer exists

The `.psc` expander's zone classifier (`crates/pyths_expand/src/zones.rs`) is the
shared core that every compression tier's zone-safety rests on. It is proved in
Lean — but about a *Lean* model, bound to the Rust only by 2,039 byte-identical
differential cases. `verification/README.md` names this gap itself ("the residue
of x18": *that the Rust byte scanner refines the Lean char scanner is **not**
proved*).

A differential cannot find a bug **both sides share**. It did not find this one:

```rust
// zones.rs, as shipped before 2026-07-14
if b == b'\\' && i + 1 < n {
    return (1 + utf8_char_len(bytes[i + 1]), Some(st));  // may exceed bytes.len()
}
```

For a **truncated** multi-byte lead byte at the end of a buffer (`bytes = [0xF0]`,
`i = 0`) `utf8_char_len` returns the length the lead byte *claims* (4) while only
1 byte remains, so `i + len > bytes.len()`. Not a live bug — real buffers come
from `&str`, and `line_start_states` clamps with `.min(n)` — but an **unstated
invariant that the `&[u8]` signature does not carry**, i.e. a trap for the next
caller. Both the Lean model and the Rust shared the assumption, so no differential
case could ever expose it.

Kani found it in **0.3 seconds**, exhaustively, from a two-line harness. That is
the entire argument for this layer.

## Running it

Kani is **not** a dependency of this workspace. There is no `kani` entry in any
`Cargo.toml`; the `kani` crate is injected by `cargo kani` itself, and every
harness lives behind `#[cfg(kani)]`. A downstream user who has never heard of
Kani builds and tests the workspace unchanged:

```bash
cargo build --release   # no Kani anywhere in the dependency graph
cargo test --workspace  # the harness module is not even compiled
```

To run the harnesses you need Kani (Linux/macOS; **not supported on Windows** —
use WSL):

```bash
cargo install --locked kani-verifier   # ~20 s
cargo kani setup                       # ~2.5 min, ~2 GB (CBMC + a pinned nightly)

cargo kani -p pyths_expand --harness string_step_progress_and_in_bounds
```

**`cargo kani -p pyths_expand` (all nine) does not currently finish** — see
"What is gated, and what is not" below. CI runs the five gated harnesses by name.
```

Harnesses: `crates/pyths_expand/src/kani_proofs.rs`.

## What is gated, and what is not

Five harnesses are **gated in CI** — every one observed green, each in under 20
seconds. Four are **not gated**, because they do not converge in a budget we have
actually paid. The split is not arbitrary; it falls exactly along one line:

> **The gated five take unconstrained `&[u8]`. The ungated four take `&str`** — so
> their harness must `assume(core::str::from_utf8(buf).is_ok())`, and CBMC pays for
> that UTF-8 validity constraint on *every* symbolic buffer. That is what explodes.
> Measured on a 15 GB box: `line_start_states` did **not** converge in **59 minutes**
> at N = 6; `code_step` passed **15 GB** of solver memory at N = 8 without
> converging. The two tier-level zone-safety harnesses inherit the same `&str`
> precondition.

| Harness | Gated? | Observed |
|---|---|---|
| `string_step_progress_and_in_bounds` | ✅ CI | SUCCESSFUL, 0.32 s |
| `string_step_is_total_on_a_truncated_lead_byte` | ✅ CI | SUCCESSFUL, 0.07 s |
| `clamp_changes_nothing_on_valid_utf8` | ✅ CI | SUCCESSFUL, 19.8 s |
| `a_backslash_escape_never_closes_a_zone` | ✅ CI | SUCCESSFUL, 0.18 s |
| `utf8_char_len_agrees_with_the_encoded_length` | ✅ CI | SUCCESSFUL, 0.47 s |
| `code_step_progress_and_in_bounds` | ❌ | did not converge (>15 GB, N = 8) |
| `line_start_states_has_one_entry_per_line` | ❌ | did not converge (>59 min, N = 6) |
| `zone_safety_dollar_sigil_...` | ❌ | not established (same `&str` cost) |
| `zone_safety_percent_sigil_...` | ❌ | not established (same `&str` cost) |

**A gate nobody has watched go green is not a gate**, so the four stay out of CI
rather than being merged on the assumption they would pass. They remain in the
source and run on demand. Closing them needs a cheaper encoding of the UTF-8
precondition (e.g. building the symbolic buffer from symbolic `char`s instead of
constraining symbolic bytes, which makes validity true by construction rather
than a solver obligation) — **open work, not a claim**.

### What this costs us, stated plainly

The two harnesses that would have checked **the zone-safety property itself over
the shipping tier rewrites** are in the ungated four. So Kani currently pins the
*classifier's* progress, in-bounds and escape properties — **not** end-to-end
tier-level zone safety. Tier-level zone safety rests, as before, on the Lean
proofs plus the 2,039-case differential. Kani is a floor under the classifier, and
today it is a narrower floor than the nine-harness table below suggests.

## What each harness checks, and at what bound

| Harness | Property | Bound | Input domain |
|---|---|---|---|
| `string_step_progress_and_in_bounds` | returned index strictly advances (`len >= 1`) **and** never leaves the buffer (`i + len <= n`) | N = 16 | **unconstrained** `[u8; 16]`, unconstrained `i`, unconstrained `ScanState` |
| `string_step_is_total_on_a_truncated_lead_byte` | regression pin: a truncated multi-byte lead byte at the buffer end advances at most to the end | N = 1 | every lead byte `>= 0xC0` |
| `clamp_changes_nothing_on_valid_utf8` | the fix is a **no-op on the real call domain** — clamped `string_step` == pre-clamp `string_step` | N = 8 | valid UTF-8, char-boundary `i` |
| `a_backslash_escape_never_closes_a_zone` | an escape pair cannot close a protected zone, whatever it escapes | N = 12 | unconstrained bytes |
| `code_step_progress_and_in_bounds` | every arm advances and stays in bounds; openers are 1 or 3 bytes | N = 12 | valid UTF-8, char-boundary `i` |
| `utf8_char_len_agrees_with_the_encoded_length` | `utf8_char_len(lead)` == the real encoded length | all `char` | every `char` |
| `line_start_states_has_one_entry_per_line` | exactly `newlines + 1` entries; no out-of-bounds index; terminates | N = 8 | valid UTF-8 |
| `zone_safety_dollar_sigil_is_never_rewritten_inside_a_protected_zone` | **the zone-safety property itself**, over the shipping `strings::substitute`: if every `$` is inside a protected zone, the output is the input byte-for-byte | N = 8 | 8-byte alphabet (below) |
| `zone_safety_percent_sigil_is_never_rewritten_inside_a_protected_zone` | same, over the shipping `idioms::substitute_with_map` with a non-empty idiom map | N = 8 | 8-byte alphabet |

Runtimes and PASS/FAIL for the pinned bounds are in the CI job log
(`.github/workflows/ci.yml` → **Kani (bounded model checking)**).

### The two honest caveats

1. **The bound.** Every result above is bounded. A bug that needs a 17-byte
   witness is not excluded by `N = 16`. Bounds were chosen so each harness
   finishes in minutes; the classifier's state is a small window (at most 3 bytes
   of lookahead + one state), so counterexamples are expected to be short — but
   "expected" is not "proved".
2. **The alphabet.** The two *tier-level* zone-safety harnesses restrict the
   symbolic bytes to `$ % ' " # \ \n a` — exhaustive over that alphabet, not over
   all 256 byte values. This keeps the entire tier rewrite (dictionary lookup and
   all) inside the CBMC budget. The alphabet contains every byte the classifier
   branches on; every other byte is, to the classifier, indistinguishable from
   `a`. That is an argument, not a proof. The *classifier* harnesses
   (`string_step`, `code_step`, escape, UTF-8) are **not** alphabet-restricted.

## Where it sits in the assurance stack

| Layer | Scope | Strength |
|---|---|---|
| Unit tests | hand-picked cases | weakest |
| Differential corpus (2,039 cases, `verification/diff_harness.py`) | Lean model vs shipping Rust, byte-identical | strong *binding*, but blind to shared assumptions |
| **Kani (this layer)** | shipping **Rust**, all inputs ≤ N bytes | exhaustive **to the bound**; finds shared-assumption bugs |
| Lean (`verification/`) | a **model** of the expander, unbounded | strongest, but about the model |

Kani is the only layer that is both *about the shipping Rust* and *exhaustive*.
It is the floor, not the ceiling.
