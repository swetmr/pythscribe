# Comparator — independent-kernel re-check for the PythScribe verified core

*Added 2026-08-04. Adopts the OpenAI **ten-proofs** trust discipline
(`lean_strategies.md` Part B: B1 pinned axioms, B2 Comparator / lean4export /
nanoda, B3 `formalization.yaml`) for our dependency-free Lean 4 verified core.*

The point (ten-proofs B2): **do not trust only Lean's own elaborator and
kernel.** Re-check the exported proof terms in an *independent* kernel. Because
our core is **dependency-free (no Mathlib)**, the export is self-contained and
this covers **all** declarations in `PythExpandVerify.lean`, not just the
headline claims — for free.

## Two layers

| Layer | What it checks | Deps | Where it runs |
|---|---|---|---|
| **L1 — axiom-footprint gate** | `#print axioms` (real Lean env) over every headline claim + its `*_stub_fails` witness ⊆ `{propext, Classical.choice, Quot.sound}`; 0 `sorry`/`admit`/`axiom`/`native_decide` | none beyond `lake` | **CI `verification` job** (always) |
| **L2 — independent kernel re-check** | export the whole elaborated environment with **lean4export**, re-type-check every declaration in **nanoda_bin** (an independent Rust Lean-4 kernel), with `permitted_axioms` pinned | `lean4export` (built @ v4.31.0) + `nanoda_bin` | local / Linux / WSL (see below) |

**L1 is ~80% of the value and is the CI gate.** It mechanically enforces the
`formalization.yaml` footprint from the kernel state, so the manifest can never
claim a tighter footprint than Lean actually reports. L2 is the strongest form:
a *second, independently-implemented* kernel confirms each proof term inhabits
its stated type.

## L1 — axiom-footprint gate (always available)

```sh
cd verification
lake build
python comparator/axiom_footprint.py gate            # CI uses `gate --no-build`
```

Exit nonzero on: any headline decl using an axiom outside the pinned trio, any
`sorry`/`admit`/`axiom`/`native_decide`, or any headline decl that the Lean env
does not report (typo / deleted). Regenerate the manifest with:

```sh
python comparator/axiom_footprint.py emit            # (re)writes ../formalization.yaml
```

Both share ONE extraction path (`run_print_axioms` → `lake env lean` on a probe
that `#print axioms` every headline + witness decl), so gate and manifest can
never disagree. CI also diffs a fresh `emit` against the committed file (drift
gate), exactly like the generated `*Data.lean` tables.

## L2 — the Comparator (lean4export + nanoda_bin)

### Build the two external tools (one-time)

**lean4export MUST be built against the SAME toolchain as the core
(`leanprover/lean4:v4.31.0`)** — it reads the core's `.olean`, which are
version-stamped. The upstream default toolchain is newer, so pin it:

```sh
git clone https://github.com/leanprover/lean4export.git
cd lean4export
echo 'leanprover/lean4:v4.31.0' > lean-toolchain      # pin to the core's toolchain
lake build                                            # -> .lake/build/bin/lean4export(.exe)
cd ..

git clone https://github.com/ammkrn/nanoda_lib.git    # nanoda_bin, an independent Rust Lean-4 kernel
cd nanoda_lib
cargo build --release                                 # -> target/release/nanoda_bin(.exe)
cd ..
```

Verified working set (2026-08-04): lean4export source at Lean-4.33 HEAD retargeted
to v4.31.0 (emits NDJSON **format 3.1.0**, githash `68218e8…`, lean 4.31.0);
`nanoda_lib` **v0.4.13**.

### Run the re-check

```sh
cd verification
LEAN4EXPORT_BIN=/abs/path/lean4export/.lake/build/bin/lean4export \
NANODA_BIN=/abs/path/nanoda_lib/target/release/nanoda_bin \
  bash comparator/run_comparator.sh
```

`run_comparator.sh` runs L1 unconditionally, then — if both env vars are set —
exports and re-checks:

```sh
# export the whole environment (must run inside the core's lake env)
lake env <lean4export> PythExpandVerify > comparator/pythexpand.ndjson
# re-type-check in the independent kernel with the pinned footprint
<nanoda_bin> comparator/nanoda-config.json     # export_file_path patched to the ndjson
```

`comparator/nanoda-config.json` pins:

```json
"permitted_axioms": ["propext", "Classical.choice", "Quot.sound"],
"unpermitted_axiom_hard_error": false
```

With `unpermitted_axiom_hard_error: false`, nanoda admits an axiom only when a
declaration **actually uses** it, and hard-errors on use of anything outside the
list — so this is the kernel-level footprint gate over the *entire* environment.

### `Lean.trustCompiler` — the one added permit

The export of the whole environment includes Lean-core declarations that
reference **`Lean.trustCompiler`** (a compiler-facing core axiom), so the
whole-environment re-check must permit it (`nanoda-config.json` adds it, the
README's expected default). **This does NOT touch our theorems:** L1's per-decl
`#print axioms` proves that **no** headline claim (or its witness) depends on
`Lean.trustCompiler` — the union over all 64 headline claims is exactly
`{propext, Classical.choice, Quot.sound}`. `Lean.trustCompiler` is a Lean-core
*export* artifact, not a dependency of anything we prove.

## Result (2026-08-04)

- **Export:** `lake env lean4export PythExpandVerify` → **6,903,432 lines /
  ~372 MB** NDJSON, format 3.1.0, lean 4.31.0. Clean (exit 0). The whole
  dependency-free environment is exportable and self-contained.
- **Independent re-check:** `nanoda_bin` (v0.4.13) reported
  **`Checked 64349 declarations with no errors`** in ~84 s (exit 0), with
  `permitted_axioms` = the trio + `Lean.trustCompiler`. Every declaration's proof
  term re-checked in the independent kernel.
- **Independent corroboration of the trust base:** nanoda additionally reported
  *"skipping exported but unpermitted axioms `["Lean.ofReduceNat", "sorryAx",
  "Lean.ofReduceBool"]`"* — these three are DECLARED in Lean core but, because
  `unpermitted_axiom_hard_error: false` would hard-error if any declaration
  *used* one, their appearing only in the SKIP list is an **independent second
  witness that nothing in the environment uses `sorryAx` (the `sorry` axiom),
  `Lean.ofReduceBool`, or `Lean.ofReduceNat` (the `native_decide` axioms)** —
  corroborating the CI trust-base grep from a different tool. (A cosmetic
  `print_axioms` pretty-printer note — "Unable to print axioms" — is non-fatal;
  set `"print_axioms": false` to silence it.)
- **L1 gate:** green — 64 headline claims (94 decls incl. witnesses), footprint
  ⊆ pinned, 0 sorry.

### CI wiring + platform note

L2 is wired as a **separate `comparator` CI job** (ubuntu-latest, modeled on the
existing heavy `kani` job): it builds the core, runs the L1 gate, clones+builds
`lean4export` (pinned from `verification/lean-toolchain`) and `nanoda_lib`, then
`ulimit -s unlimited` + `run_comparator.sh` (export + independent re-check). It
is kept OFF the fast `verification` job so the hot path stays quick.

`ulimit -s unlimited` is required because `nanoda_bin` type-checks on its **main
thread** with deep recursion. On **native Windows** (~1 MB main-thread stack) it
**overflows the stack** on the 372 MB whole-core export (`RUST_MIN_STACK` does
not help — it only affects *spawned* threads). Locally on Windows, run L2 under
**WSL / Linux** instead:

```sh
# WSL / Linux
ulimit -s unlimited
cargo build --release            # build nanoda_bin natively on Linux
./target/release/nanoda_bin nanoda_run.json     # export_file_path -> /mnt/c/.../pythexpand.ndjson
# => "Checked 64349 declarations with no errors", exit 0
```

This is a **platform stack-size property of nanoda**, NOT a format/version
incompatibility — lean4export@v4.31.0 (format 3.1.0) and nanoda v0.4.13 are
format-compatible (parse + typecheck succeed end-to-end on Linux). `landrun` is
Linux-only and optional (see below); it can be layered onto the CI job later.

`landrun` (Landlock sandbox, ten-proofs) is **Linux-only** and **not required**
for the re-check itself; it sandboxes the checker's filesystem access and can be
layered on the Linux job later.

## What this guarantees (and what it does not)

- **Guaranteed (L1, CI-enforced):** every Paper-C headline theorem is `sorry`-free
  and depends only on `{propext, Classical.choice, Quot.sound}` — Lean's three
  standard axioms, here **inherited from Lean core's `String`**, none of our own.
  Build-pinned per-decl by `#guard_msgs`-wrapped `#print axioms` inside
  `PythExpandVerify.lean` *and* by this gate.
- **Guaranteed (L2, run on Linux/WSL):** an **independent** kernel accepts every
  declaration's proof term at its stated type, within the pinned footprint — so
  the guarantee does not rest on Lean's own elaborator/kernel.
- **NOT guaranteed by either:** that a *statement* is the intended one. A green
  independent kernel over a *weakened* statement is still vacuous — that is the
  job of the `lean-spec-quality` statement-review gate, the `*_stub_fails`
  witnesses (`formalization.yaml` `witness:` field), and the CPython
  differentials. Comparator hardens the *proof-term trust*, not the *statement
  choice*.

## Follow-ups (fable adversarial review, 2026-08-04)

The fable review of the trust artifact fixed F-1..F-4 (statement-name honesty +
witness bookkeeping in `headline_claims.py`) and F-6 (the sorry/axiom scan now covers
ALL `verification/*.lean`, both in `axiom_footprint.py` and the CI trust-base grep). The
following three are **non-blocking follow-ups**, recorded here rather than fixed now:

- **F-5 — emitted-certificate differential.** The routing/WASM certificate theorems
  (`checkCert_sound`, `wasm_admission_sound`) are bound to the Rust side via committed
  fixtures (`route-table.txt`, `wasm-admission-table.txt`), which catch model↔table
  drift but not a synchronized model+emitter bug. Follow-up: add a differential that
  re-derives the certificate from the ACTUAL emitted artifact (not the fixture) and
  diffs it against the Lean `emitCert` model, closing the last synchronized-bug gap the
  same way the CPython differential backstops the preservation waves.
- **F-7 — nanoda `unpermitted_axiom_hard_error` semantics.** The L2 re-check runs with
  `unpermitted_axiom_hard_error: false`, relying on the observation that nanoda only
  ADMITS an axiom a declaration actually uses. Follow-up: on WSL, inject a throwaway
  declaration that DOES use an unpermitted axiom (e.g. a `sorry`) into the export and
  confirm nanoda hard-errors (or lists it as USED, not merely SKIPPED) — turning the
  "skip list = unused" inference into a positively-tested guarantee.
- **F-8 — cross-repo validators.** The `spec_validate_*.py` / `spec_lean_drift.py`
  CPython-differential validators live in `verification/`; the shipping-binding harnesses
  they mirror (`*_shipped_binding.py`) live in a separate app repo. Follow-up:
  vendor or symlink a canonical copy (or document the exact paths) so the differential
  provenance is self-contained from the pythscribe repo alone.

These do not affect the L1 gate or the axiom footprint; they harden the differential
provenance and the L2 second-kernel guarantee.

## Files

| File | Role |
|---|---|
| `../formalization.yaml` | the ten-proofs manifest (headline claims + aggregate integrity) — GENERATED |
| `headline_claims.py` | the curated headline claim SELECTION (names, witnesses, paper cross-refs) |
| `axiom_footprint.py` | `emit` (manifest) + `gate` (CI axiom-footprint gate) — shared extraction |
| `run_comparator.sh` | L1 always + L2 (export + independent re-check) when tools present |
| `nanoda-config.json` | nanoda_bin config: `permitted_axioms` pinned to the footprint |
