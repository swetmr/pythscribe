use std::path::Path;

use super::output::{self, Verbosity};

/// `check` runs two independent gates: the authoritative recursive-descent
/// parser, then the type checker. `syntax_only` stops after the parser, so
/// callers can observe the *parser's* verdict in isolation.
///
/// This matters for grammar work: `grammar/pyths.lark` is a syntactic
/// acceptor, so the only meaningful comparison against the authoritative
/// implementation is parse-vs-parse. Without this flag a type error is
/// indistinguishable from a syntax error at the exit code, and any
/// differential measurement between the grammar and the parser silently
/// scores type errors as grammar false-accepts. Used by
/// `scripts/grammar-fuzz.py`.
pub fn run(
    file: &str,
    verbosity: Verbosity,
    syntax_only: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(file);
    if !path.exists() {
        return Err(format!("File not found: {}", file).into());
    }

    let raw_source = std::fs::read_to_string(path)?;
    let filename = path.file_name().unwrap().to_string_lossy();

    // Load project config for stub paths and `.psc` expansion. Like
    // `compile`, `.psc` is expanded under `Auto` mode; `.ps` passes
    // through. The user's modularity ask: `.ps` files never go through
    // the expander.
    let config = pyths_config::load_or_default();
    let is_psc = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("psc"))
        .unwrap_or(false);
    let should_expand = match config.expand.mode {
        pyths_config::ExpandMode::Always => true,
        pyths_config::ExpandMode::Never => false,
        pyths_config::ExpandMode::Auto => is_psc,
    };
    let source = if should_expand {
        // Thread both the $NAME dictionary and the %NAME idiom map, exactly as
        // `compile`/`expand` do — otherwise `check` runs a different pipeline
        // and `%NAME` idioms are left unexpanded (issue #193, "unrelated" note).
        pyths_expand::expand_with_config(
            &raw_source,
            &config.expand.dictionary,
            &config.expand.idioms,
        )
    } else {
        raw_source
    };

    let mut module = match pyths_parser::parse(&source) {
        Ok(module) => module,
        Err(errors) => {
            let diags: Vec<_> = errors
                .iter()
                .map(|e| {
                    let mut diag = pyths_diagnostic::errors::Diagnostic::error(
                        &e.message,
                        pyths_syntax::span::Span::new(e.span.start, e.span.end),
                    );
                    for note in &e.notes {
                        diag = diag.with_note(note);
                    }
                    diag
                })
                .collect();
            let rendered = pyths_diagnostic::render::render_diagnostics(&source, &filename, &diags);
            eprint!("{}", rendered);
            return Err(format!("{} error(s) found", errors.len()).into());
        }
    };

    if syntax_only {
        if verbosity != Verbosity::Quiet {
            output::success(&format!("✓ {} — parsed (syntax only)", file));
        }
        return Ok(());
    }

    // Expand relative star imports (commands::relstar) so `check` analyzes
    // the same rewritten module `compile` will emit.
    super::relstar::normalize_relative_imports(&mut module, path)?;
    let module = module;

    // Run type checker. Project-local stubs (if any) override bundled.
    let type_errors = pyths_types::check_with_stub_paths(&module, &config.stubs.paths);

    if type_errors.is_empty() {
        // public #3 GATE: `check` must also fail on CODEGEN diagnostics —
        // unimplemented builtins (`open`/`input`/`hash`/...), rejected
        // imports, unsupported methods. Previously `pyths check` PASSED
        // files that `pyths compile` rejects (or worse, that crashed at
        // runtime with a bare ReferenceError). Same gate as compile.rs: a
        // certified codegen pass whose collected errors fail the check; the
        // emitter prints each "error: ..." as it records it.
        let gate_opts = pyths_codegen_js::CodegenOptions {
            npm_imports: Some(&config.npm.imports),
            ..Default::default()
        };
        let gate = pyths_codegen_js::codegen_certified(&module, &gate_opts);
        if !gate.errors.is_empty() {
            return Err(format!("{} compile error(s) found", gate.errors.len()).into());
        }
        if verbosity != Verbosity::Quiet {
            output::success(&format!("✓ {} — no errors", file));
        }
        Ok(())
    } else {
        let diags: Vec<_> = type_errors
            .iter()
            .map(|te| {
                let mut diag = pyths_diagnostic::errors::Diagnostic::warning(&te.message, te.span);
                for note in &te.notes {
                    diag = diag.with_note(note);
                }
                diag
            })
            .collect();
        let rendered = pyths_diagnostic::render::render_diagnostics(&source, &filename, &diags);
        eprint!("{}", rendered);
        Err(format!("{} type error(s) found", type_errors.len()).into())
    }
}
