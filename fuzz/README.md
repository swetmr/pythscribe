# PythScribe fuzzing

Coverage-guided fuzz targets for the PythScribe pipeline. Complements the in-tree panic-resistance suite at `crates/pyths_cli/tests/fuzz_inputs.rs` (which runs in CI on stable Rust); these targets are for **deep, coverage-guided exploration** under nightly + `libfuzzer-sys`.

This crate intentionally sits **outside the workspace** — `cargo fuzz` resolves it via its own toolchain and `libfuzzer-sys` requires nightly. Listing it in the workspace would force nightly on every `cargo build --workspace` run.

## Prerequisites

```bash
rustup install nightly
cargo install cargo-fuzz
```

## Running a target

From the repository root:

```bash
cargo +nightly fuzz run fuzz_lexer  -- -max_len=4096
cargo +nightly fuzz run fuzz_expand -- -max_len=4096
cargo +nightly fuzz run fuzz_parser -- -max_len=4096
cargo +nightly fuzz run fuzz_check  -- -max_len=4096
```

Each target generates inputs to `fuzz/corpus/<target>/` and crash reproducers to `fuzz/artifacts/<target>/` (both gitignored).

## Seeding the corpus

`fuzz/seed_corpus/<target>/` ships a curated starting set (mirroring `tests/fixtures/`, plus `.psc` samples for the expander). Copy it into the working corpus before each run:

```bash
# Unix / macOS
mkdir -p fuzz/corpus
cp -r fuzz/seed_corpus/* fuzz/corpus/

# Windows (PowerShell)
New-Item -ItemType Directory -Force fuzz/corpus | Out-Null
Copy-Item -Recurse fuzz/seed_corpus/* fuzz/corpus/
```

The seed directory is tracked in version control so corpus regressions are reproducible. The working `fuzz/corpus/` is `.gitignored` — libfuzzer writes discovered inputs there during a run, but only the seed is the load-bearing record.

## What a successful run looks like

LibFuzzer prints incremental coverage data. A clean target settles into "no new coverage" within a few CPU-hours. A failing target prints the crashing input and writes it to `fuzz/artifacts/<target>/crash-<sha>`. **Any crash is a bug.** Triage:

1. Reproduce locally: `cargo +nightly fuzz run <target> fuzz/artifacts/<target>/crash-<sha>`
2. Add a regression case to `crates/pyths_cli/tests/fuzz_inputs.rs` so CI catches it on stable.
3. Fix the panic and re-run.

## Why both in-tree and `cargo-fuzz`?

| Path | Toolchain | Coverage-guided | Runs in CI | Use for |
|---|---|---|---|---|
| `crates/pyths_cli/tests/fuzz_inputs.rs` | Stable | No (random + mutation) | Yes | Regression — protect against the *categories* of bugs already found |
| `fuzz/` (this crate) | Nightly + libfuzzer | Yes | No (manual) | Exploration — discover new bug categories |

When `cargo fuzz` surfaces a new failure mode, fold a regression test into the in-tree suite so it sticks under CI.
