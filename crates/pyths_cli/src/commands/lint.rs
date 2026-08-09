use std::path::{Path, PathBuf};

use pyths_syntax::ast::*;
use pyths_syntax::span::Span;

use super::output::{self, Verbosity};

/// A single lint warning.
#[derive(Debug)]
struct LintWarning {
    id: &'static str,
    message: String,
    span: Span,
}

pub fn run(path: Option<&str>, verbosity: Verbosity) -> Result<(), Box<dyn std::error::Error>> {
    let target = path.map(Path::new).unwrap_or_else(|| Path::new("."));

    let files = if target.is_file() {
        vec![target.to_path_buf()]
    } else {
        discover_ps_files(target)?
    };

    if files.is_empty() {
        if verbosity != Verbosity::Quiet {
            println!("No .ps files found");
        }
        return Ok(());
    }

    let mut total_warnings = 0;

    for file in &files {
        let source = std::fs::read_to_string(file)?;
        let filename = file.file_name().unwrap().to_string_lossy();

        let module = match pyths_parser::parse(&source) {
            Ok(m) => m,
            Err(errors) => {
                if verbosity != Verbosity::Quiet {
                    let diags: Vec<_> = errors
                        .iter()
                        .map(|e| {
                            let mut diag = pyths_diagnostic::errors::Diagnostic::error(
                                &e.message,
                                Span::new(e.span.start, e.span.end),
                            );
                            for note in &e.notes {
                                diag = diag.with_note(note);
                            }
                            diag
                        })
                        .collect();
                    let rendered =
                        pyths_diagnostic::render::render_diagnostics(&source, &filename, &diags);
                    eprint!("{}", rendered);
                }
                continue;
            }
        };

        let resolve_info = pyths_resolve::resolve(&module);
        let mut warnings = Vec::new();

        // Run lint passes
        check_unused_variables(&resolve_info, &mut warnings);
        check_unused_imports(&resolve_info, &mut warnings);
        check_unreachable_code(&module.body, &mut warnings);
        check_naming_conventions(&resolve_info, &mut warnings);
        check_pass_in_non_empty(&module.body, &mut warnings);
        check_mutable_default_args(&module.body, &mut warnings);

        if !warnings.is_empty() {
            let diags: Vec<_> = warnings
                .iter()
                .map(|w| {
                    pyths_diagnostic::errors::Diagnostic::warning(
                        format!("[{}] {}", w.id, w.message),
                        w.span,
                    )
                })
                .collect();

            if verbosity != Verbosity::Quiet {
                let rendered =
                    pyths_diagnostic::render::render_diagnostics(&source, &filename, &diags);
                eprint!("{}", rendered);
            }

            total_warnings += warnings.len();
        }
    }

    if total_warnings > 0 {
        if verbosity != Verbosity::Quiet {
            output::warning_summary(&format!("{} warning(s) found", total_warnings));
        }
        return Err(format!("{} warning(s) found", total_warnings).into());
    }

    if verbosity != Verbosity::Quiet {
        output::success("No warnings found");
    }

    Ok(())
}

/// W001: Unused variable — assigned but never referenced, not a parameter, not _-prefixed.
fn check_unused_variables(info: &pyths_resolve::ResolveInfo, warnings: &mut Vec<LintWarning>) {
    for scope in info.scopes.iter() {
        // Skip module scope — top-level variables are often used externally
        if scope.kind == pyths_resolve::ScopeKind::Module {
            continue;
        }
        for (name, sym) in &scope.symbols {
            if name.starts_with('_') {
                continue;
            }
            if sym.is_assigned && !sym.is_referenced && !sym.is_param && !sym.is_imported {
                warnings.push(LintWarning {
                    id: "W001",
                    message: format!("unused variable '{}'", name),
                    span: Span::new(0, 0), // We don't track definition span per symbol yet
                });
            }
        }
    }
}

/// W002: Unused import — imported but never referenced, not _-prefixed.
fn check_unused_imports(info: &pyths_resolve::ResolveInfo, warnings: &mut Vec<LintWarning>) {
    for scope in &info.scopes {
        for (name, sym) in &scope.symbols {
            if name.starts_with('_') {
                continue;
            }
            if sym.is_imported && !sym.is_referenced {
                warnings.push(LintWarning {
                    id: "W002",
                    message: format!("unused import '{}'", name),
                    span: Span::new(0, 0),
                });
            }
        }
    }
}

/// W003: Unreachable code — statements after return/break/continue in a block.
fn check_unreachable_code(stmts: &[Stmt], warnings: &mut Vec<LintWarning>) {
    for window in stmts.windows(2) {
        let first = &window[0];
        let second = &window[1];
        let is_terminal = matches!(
            first.kind,
            StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue
        );
        if is_terminal {
            warnings.push(LintWarning {
                id: "W003",
                message: "unreachable code after return/break/continue".to_string(),
                span: second.span,
            });
        }
    }

    // Recurse into nested blocks
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FuncDef { body, .. } => check_unreachable_code(body, warnings),
            StmtKind::ClassDef { body, .. } => check_unreachable_code(body, warnings),
            StmtKind::If {
                body,
                elif_clauses,
                else_body,
                ..
            } => {
                check_unreachable_code(body, warnings);
                for (_, elif_body) in elif_clauses {
                    check_unreachable_code(elif_body, warnings);
                }
                if let Some(eb) = else_body {
                    check_unreachable_code(eb, warnings);
                }
            }
            StmtKind::While {
                body, else_body, ..
            } => {
                check_unreachable_code(body, warnings);
                if let Some(eb) = else_body {
                    check_unreachable_code(eb, warnings);
                }
            }
            StmtKind::For {
                body, else_body, ..
            } => {
                check_unreachable_code(body, warnings);
                if let Some(eb) = else_body {
                    check_unreachable_code(eb, warnings);
                }
            }
            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                check_unreachable_code(body, warnings);
                for h in handlers {
                    check_unreachable_code(&h.body, warnings);
                }
                if let Some(eb) = else_body {
                    check_unreachable_code(eb, warnings);
                }
                if let Some(fb) = finally_body {
                    check_unreachable_code(fb, warnings);
                }
            }
            StmtKind::With { body, .. } => check_unreachable_code(body, warnings),
            _ => {}
        }
    }
}

/// W004: Naming convention — classes should be CamelCase, functions/variables should be snake_case.
/// Note: This rule requires AST-level checking to distinguish class names from function names,
/// which SymbolInfo alone doesn't provide. Reserved for future enhancement.
fn check_naming_conventions(_info: &pyths_resolve::ResolveInfo, _warnings: &mut Vec<LintWarning>) {
    // Intentionally left as a stub — distinguishing class names from function/variable names
    // requires walking the AST, not just the symbol table. This will be implemented when
    // we add AST node tracking to SymbolInfo.
}

/// W005: Pass in non-empty block — a `pass` statement alongside other statements.
fn check_pass_in_non_empty(stmts: &[Stmt], warnings: &mut Vec<LintWarning>) {
    if stmts.len() > 1 {
        for stmt in stmts {
            if matches!(stmt.kind, StmtKind::Pass) {
                warnings.push(LintWarning {
                    id: "W005",
                    message: "'pass' is unnecessary in a block with other statements".to_string(),
                    span: stmt.span,
                });
            }
        }
    }

    // Recurse into nested blocks
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FuncDef { body, .. } => check_pass_in_non_empty(body, warnings),
            StmtKind::ClassDef { body, .. } => check_pass_in_non_empty(body, warnings),
            StmtKind::If {
                body,
                elif_clauses,
                else_body,
                ..
            } => {
                check_pass_in_non_empty(body, warnings);
                for (_, elif_body) in elif_clauses {
                    check_pass_in_non_empty(elif_body, warnings);
                }
                if let Some(eb) = else_body {
                    check_pass_in_non_empty(eb, warnings);
                }
            }
            StmtKind::While {
                body, else_body, ..
            } => {
                check_pass_in_non_empty(body, warnings);
                if let Some(eb) = else_body {
                    check_pass_in_non_empty(eb, warnings);
                }
            }
            StmtKind::For {
                body, else_body, ..
            } => {
                check_pass_in_non_empty(body, warnings);
                if let Some(eb) = else_body {
                    check_pass_in_non_empty(eb, warnings);
                }
            }
            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                check_pass_in_non_empty(body, warnings);
                for h in handlers {
                    check_pass_in_non_empty(&h.body, warnings);
                }
                if let Some(eb) = else_body {
                    check_pass_in_non_empty(eb, warnings);
                }
                if let Some(fb) = finally_body {
                    check_pass_in_non_empty(fb, warnings);
                }
            }
            StmtKind::With { body, .. } => check_pass_in_non_empty(body, warnings),
            _ => {}
        }
    }
}

/// W006: Mutable default argument — list/dict/set literal as function default parameter.
fn check_mutable_default_args(stmts: &[Stmt], warnings: &mut Vec<LintWarning>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FuncDef { params, body, .. } => {
                for param in params {
                    if let Some(default) = &param.default {
                        let is_mutable = matches!(
                            default.kind,
                            ExprKind::List(_) | ExprKind::Dict { .. } | ExprKind::Set(_)
                        );
                        if is_mutable {
                            warnings.push(LintWarning {
                                id: "W006",
                                message: format!(
                                    "mutable default argument for parameter '{}'",
                                    param.name
                                ),
                                span: param.span,
                            });
                        }
                    }
                }
                // Recurse
                check_mutable_default_args(body, warnings);
            }
            StmtKind::ClassDef { body, .. } => {
                check_mutable_default_args(body, warnings);
            }
            _ => {}
        }
    }
}

fn discover_ps_files(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    collect_ps_files(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_ps_files(
    dir: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let name = path.file_name().unwrap().to_string_lossy();
            if !name.starts_with('.') && name != "node_modules" && name != "target" {
                collect_ps_files(&path, files)?;
            }
        } else if path.extension().is_some_and(|e| e == "ps") {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint_warnings(source: &str) -> Vec<LintWarning> {
        let module = pyths_parser::parse(source).expect("parse failed");
        let resolve_info = pyths_resolve::resolve(&module);
        let mut warnings = Vec::new();
        check_unused_variables(&resolve_info, &mut warnings);
        check_unused_imports(&resolve_info, &mut warnings);
        check_unreachable_code(&module.body, &mut warnings);
        check_naming_conventions(&resolve_info, &mut warnings);
        check_pass_in_non_empty(&module.body, &mut warnings);
        check_mutable_default_args(&module.body, &mut warnings);
        warnings
    }

    fn has_warning(warnings: &[LintWarning], id: &str) -> bool {
        warnings.iter().any(|w| w.id == id)
    }

    #[test]
    fn test_w001_unused_variable() {
        let warnings = lint_warnings("def foo():\n    x = 1\n    return 2");
        assert!(
            has_warning(&warnings, "W001"),
            "Expected W001, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_w001_used_variable_no_warning() {
        let warnings = lint_warnings("def foo():\n    x = 1\n    return x");
        assert!(
            !has_warning(&warnings, "W001"),
            "Expected no W001, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_w002_unused_import() {
        let warnings = lint_warnings("import os\nx = 1");
        assert!(
            has_warning(&warnings, "W002"),
            "Expected W002, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_w002_used_import_no_warning() {
        let warnings = lint_warnings("import os\nprint(os)");
        assert!(
            !has_warning(&warnings, "W002"),
            "Expected no W002, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_w002_side_effect_import_no_warning() {
        // A2: `import "./styles.css"` is a side-effect-only import that
        // binds no name — it must never trigger W002 (unused import),
        // since "unused" doesn't apply to a statement with nothing to use.
        let warnings = lint_warnings("import \"./styles.css\"\nx = 1");
        assert!(
            !has_warning(&warnings, "W002"),
            "Expected no W002, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_w003_unreachable_code() {
        let warnings = lint_warnings("def foo():\n    return 1\n    x = 2");
        assert!(
            has_warning(&warnings, "W003"),
            "Expected W003, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_w003_no_unreachable() {
        let warnings = lint_warnings("def foo():\n    x = 1\n    return x");
        assert!(
            !has_warning(&warnings, "W003"),
            "Expected no W003, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_w005_pass_in_non_empty() {
        let warnings = lint_warnings("def foo():\n    x = 1\n    pass");
        assert!(
            has_warning(&warnings, "W005"),
            "Expected W005, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_w005_pass_alone_no_warning() {
        let warnings = lint_warnings("def foo():\n    pass");
        assert!(
            !has_warning(&warnings, "W005"),
            "Expected no W005, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_w006_mutable_default_arg() {
        let warnings = lint_warnings("def foo(items=[]):\n    pass");
        assert!(
            has_warning(&warnings, "W006"),
            "Expected W006, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_w006_immutable_default_no_warning() {
        let warnings = lint_warnings("def foo(x=0):\n    pass");
        assert!(
            !has_warning(&warnings, "W006"),
            "Expected no W006, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_lint_clean_file_no_warnings() {
        let warnings = lint_warnings("def greet(name):\n    return f\"Hello, {name}!\"");
        assert!(
            warnings.is_empty(),
            "Expected no warnings, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_lint_underscore_prefix_ignored() {
        let warnings = lint_warnings("def foo():\n    _unused = 1\n    return 2");
        let has_w001 = warnings
            .iter()
            .any(|w| w.id == "W001" && w.message.contains("_unused"));
        assert!(
            !has_w001,
            "Expected _unused to be ignored, got: {:?}",
            warnings
        );
    }
}
