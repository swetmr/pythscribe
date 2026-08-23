# Contributing to PythScribe

Thank you for your interest in contributing to PythScribe! This guide covers everything you need to get started.

## Development Setup

### Prerequisites

- **Rust 1.70+** — Install via [rustup](https://rustup.rs/)
- **Node.js 18+** — For running compiled output and runtime tests
- **npm** — For runtime package development

### Building from source

```bash
git clone https://github.com/swetmr/pythscribe.git
cd pythscribe
cargo build
```

### Install the git hooks (one-time per clone)

```bash
bash scripts/setup-hooks.sh
```

This points `core.hooksPath` at the committed `.githooks/` directory, so
everyone runs the same checks — nothing to copy into `.git/hooks/`, nothing
that can silently go stale. The `pre-push` hook blocks the two drift classes
that have actually broken CI on `main`:

| Check | CI job it protects | Cost | When it runs |
|---|---|---|---|
| `cargo fmt --all -- --check` | Lint | seconds | every push |
| `cargo clippy --workspace -- -D warnings` | Lint | seconds on a warm cache | every push |
| `formalization.yaml` in sync (`python comparator/axiom_footprint.py check --no-build`) | Lean proofs | tens of seconds | only when the push touches `verification/` |

Design notes:

- **pre-push, not pre-commit.** The Lean-manifest check shells out to
  `lake env lean` (`#print axioms` over every headline claim) — too slow for
  every commit, fine at push time. Formatting is cheap either way; it rides
  along at push so there is a single hook to reason about.
- **No full `lake build` in the hook.** The check runs `--no-build` against
  your existing `verification/.lake` build. If you changed `verification/`
  and have no build, the hook fails with instructions rather than silently
  skipping — a skip exactly when the risk exists is how drift reached CI
  historically.
- **`check` never rewrites the manifest.** It regenerates to a temp file via
  the exact same code path as `emit` and diffs, so the hook and CI's in-sync
  step can never disagree.

If a check fails: `cargo fmt --all` for formatting; for a stale manifest,
`cd verification && lake build && python comparator/axiom_footprint.py emit`
— then commit and push again. Never commit a regenerated manifest that adds
axioms beyond `{propext, Classical.choice, Quot.sound}` or a nonzero sorry
count: that is a real regression the manifest exists to surface, not drift.
Emergency bypass: `git push --no-verify` (CI still arbitrates).

### Running tests

```bash
# All tests (1,157 tests across all crates)
cargo test

# Specific crate
cargo test -p pyths_lexer
cargo test -p pyths_parser
cargo test -p pyths_codegen_js
cargo test -p pyths_types
cargo test -p pyths_cli

# With output (useful for debugging)
cargo test -- --nocapture

# Single test
cargo test test_name -- --exact
```

### Running benchmarks

```bash
cargo bench -p pyths_codegen_js
```

## Project Structure

```
crates/
├── pyths_lexer/        # Tokenization (logos + custom INDENT/DEDENT)
├── pyths_syntax/       # AST node definitions + Span type
├── pyths_parser/       # Recursive descent parser
├── pyths_codegen_js/   # AST → JavaScript emission
├── pyths_diagnostic/   # Error rendering (ariadne)
├── pyths_resolve/      # Name resolution (LEGB scopes)
├── pyths_types/        # Type checker
├── pyths_cli/          # CLI binary (clap)
├── pyths_runtime/      # Runtime crate (wraps JS runtime)
├── pyths_hir/          # High-level IR
└── pyths_codegen_wasm/ # WASM backend
```

### Compilation Pipeline

```
.ps source → Lexer → Tokens → Parser → AST → Resolve → TypeCheck → Codegen → .js output
```

1. **Lexer** (`pyths_lexer`): logos-based tokenization with custom INDENT/DEDENT injection for Python-style indentation
2. **Parser** (`pyths_parser`): Manual recursive descent — no parser generator. Entry point: `parse()`
3. **Name Resolution** (`pyths_resolve`): LEGB scope analysis, symbol table construction, reference tracking
4. **Type Checker** (`pyths_types`): Validates annotated types — literal mismatches, return type errors, call arity
5. **Codegen** (`pyths_codegen_js`): Direct AST → JavaScript string emission. Entry point: `codegen()`

## Coding Standards

### Rust conventions

- **Edition 2021** — All crates use Rust 2021 edition
- **No `unwrap()` in library code** — Use `Result` or `Option` propagation; `unwrap()` is acceptable in tests
- **Pattern matching** — Prefer exhaustive `match` over `if let` chains for AST traversal
- **Naming** — Follow Rust standard naming: `snake_case` functions, `CamelCase` types, `SCREAMING_SNAKE_CASE` constants

### Code style

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix warnings
- Keep functions focused — under 50 lines where possible
- Add `#[test]` for every new feature or bug fix

### Test conventions

- **Unit tests** — In the same file as the code, under `#[cfg(test)] mod tests`
- **Integration tests** — In `crates/<name>/tests/` for cross-module testing
- **CLI tests** — In `crates/pyths_cli/tests/cli_test.rs` using `std::process::Command`
- **Fixtures** — Test `.ps` files go in `tests/fixtures/`

Test naming pattern:
```rust
#[test]
fn test_<feature>_<scenario>() {
    // Arrange
    let source = "...";
    // Act
    let result = parse(source);
    // Assert
    assert!(result.is_ok());
}
```

### Adding a new AST node

1. Define the node in `crates/pyths_syntax/src/ast.rs`
2. Add parsing in `crates/pyths_parser/src/parser.rs`
3. Add codegen in `crates/pyths_codegen_js/src/emit.rs`
4. Add resolver handling in `crates/pyths_resolve/src/resolver.rs` (if it introduces scope)
5. Add type checking in `crates/pyths_types/src/checker.rs` (if it has type semantics)
6. Write unit tests for parser and codegen
7. Write integration test in `crates/pyths_codegen_js/tests/integration_test.rs`
8. Add a fixture file in `tests/fixtures/` if needed

### Adding a CLI command

1. Create `crates/pyths_cli/src/commands/<name>.rs`
2. Add variant to `Commands` enum in `main.rs`
3. Add match arm in `main()`
4. Write CLI integration test in `crates/pyths_cli/tests/cli_test.rs`

## Pull Request Workflow

1. **Fork and branch** — Create a feature branch from `main`
   ```bash
   git checkout -b feature/my-feature
   ```

2. **Make changes** — Write code, add tests

3. **Verify locally**
   ```bash
   cargo fmt --check
   cargo clippy
   cargo test --workspace
   ```

   For changes that touch the gated codegen crates (`pyths_codegen_js`,
   `pyths_codegen_wasm`, `pyths_expand`, `pyths_hir`, `pyths_types`,
   `pyths_resolve`, `pyths_parser`, `pyths_lexer`, `pyths_syntax`,
   `pyths_print`), also run the **change-impact coverage gate** — it
   hard-fails if any changed executable line is executed by zero
   oracle-backed programs (the exact class that has shipped silent
   miscompiles). Collect on *this* candidate tree, then gate:
   ```bash
   # 1. differential-oracle coverage profile (instrumented pyths driven by
   #    the CPython corpus + Livermore + K9 WASM nets) -> target/oracle-coverage.lcov
   python scripts/coverage-collect.py

   # 2. the gate: changed lines (merge-base..worktree) vs that profile
   python scripts/coverage-gate.py
   ```
   Uncovered lines require either a new covering corpus entry/sweep or an
   explicit reviewed waiver in `scripts/coverage-waivers.txt` (see the
   script docstrings). This slot sits after `cargo test --workspace`,
   alongside the planned `pbt-gate.py` fixed-seed PBT gate.

   For changes that touch the **Lean verified core** (`verification/`), also
   run the **Comparator axiom-footprint gate** — it re-derives `#print axioms`
   over every Paper-C headline claim and fails on any axiom outside the pinned
   footprint `{propext, Classical.choice, Quot.sound}` or any `sorry`, and it
   keeps `verification/formalization.yaml` in sync with the source:
   ```bash
   cd verification
   lake build
   python comparator/axiom_footprint.py gate      # L1: footprint + no-sorry gate
   python comparator/axiom_footprint.py emit       # regenerate the manifest; commit if it changed
   ```
   For the strongest check (ten-proofs Comparator — re-type-check the exported
   proof terms in the independent `nanoda_bin` kernel), run
   `bash comparator/run_comparator.sh` with `LEAN4EXPORT_BIN`/`NANODA_BIN` set
   (Linux/WSL; see `verification/comparator/comparator.md`). CI runs L1 in the
   `verification` job and the independent re-check in the `comparator` job.

4. **Commit** — Write clear commit messages
   ```
   Add match/case pattern matching

   Implement pattern matching with support for literal, capture,
   wildcard, sequence, OR, and guard patterns. Compiles to
   chained if/else with a __match temporary variable.
   ```

5. **Push and create PR** — Target the `main` branch
   ```bash
   git push origin feature/my-feature
   ```

   (With the hooks installed — `bash scripts/setup-hooks.sh` — the push is
   automatically blocked on formatting drift or a stale `formalization.yaml`.)

6. **CI checks** — All tests must pass, clippy must be clean, formatting must be correct

## Reporting Issues

- **Bug reports** — Include the `.ps` source code, expected output, actual output, and error messages
- **Feature requests** — Describe the Python syntax/semantics you want, and the expected JS output
- **Performance issues** — Include benchmark numbers and the `.ps` file that's slow

## Where to Contribute

Good first issues:
- **New lint rules** — Add rules to `crates/pyths_codegen_js/src/lint.rs`
- **Standard library modules** — Add Python stdlib ports to `runtime/src/stdlib/`
- **Error message improvements** — Better hints in `crates/pyths_parser/src/parser.rs`
- **Test fixtures** — Add `.ps` files that exercise edge cases
- **Documentation** — Improve docs, add examples

## License

By contributing, you agree that your contributions will be licensed under the Functional Source License 1.1 (Apache-2.0 Future), `FSL-1.1-ALv2`, the same license as PythScribe. See [`LICENSE.md`](./LICENSE.md).
