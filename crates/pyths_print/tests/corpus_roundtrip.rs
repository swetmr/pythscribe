use pyths_syntax::ast::*;
use pyths_syntax::span::Span;
use std::path::{Path, PathBuf};

fn ps_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            ps_files(&p, out);
        } else if p.extension().map_or(false, |x| x == "ps") {
            out.push(p);
        }
    }
}

// ── Span erasure ────────────────────────────────────────────────────────────
// AST-equivalence is structural (kind-level), not positional.
// After reprinting, all source spans change because the source is reformatted.
// We erase spans to Span::dummy() before comparing so that the assertion only
// checks program structure, not byte offsets.

fn erase_module(m: Module) -> Module {
    Module {
        body: m.body.into_iter().map(erase_stmt).collect(),
        span: Span::dummy(),
    }
}

fn erase_stmt(s: Stmt) -> Stmt {
    Stmt {
        kind: erase_stmt_kind(s.kind),
        span: Span::dummy(),
    }
}

fn erase_stmt_kind(k: StmtKind) -> StmtKind {
    match k {
        StmtKind::Pass => StmtKind::Pass,
        StmtKind::Break => StmtKind::Break,
        StmtKind::Continue => StmtKind::Continue,
        StmtKind::Expr(e) => StmtKind::Expr(erase_expr(e)),
        StmtKind::Assign { targets, value } => StmtKind::Assign {
            targets: targets.into_iter().map(erase_expr).collect(),
            value: erase_expr(value),
        },
        StmtKind::AugAssign { target, op, value } => StmtKind::AugAssign {
            target: erase_expr(target),
            op,
            value: erase_expr(value),
        },
        StmtKind::AnnAssign {
            target,
            annotation,
            value,
        } => StmtKind::AnnAssign {
            target: erase_expr(target),
            annotation: erase_expr(annotation),
            value: value.map(erase_expr),
        },
        StmtKind::Return(v) => StmtKind::Return(v.map(erase_expr)),
        StmtKind::Raise(v, c) => StmtKind::Raise(v.map(erase_expr), c.map(erase_expr)),
        StmtKind::FuncDef {
            name,
            params,
            body,
            decorator_list,
            return_type,
            is_async,
        } => StmtKind::FuncDef {
            name,
            params: params.into_iter().map(erase_param).collect(),
            body: body.into_iter().map(erase_stmt).collect(),
            decorator_list: decorator_list.into_iter().map(erase_expr).collect(),
            return_type: return_type.map(erase_expr),
            is_async,
        },
        StmtKind::ClassDef {
            name,
            bases,
            body,
            decorator_list,
        } => StmtKind::ClassDef {
            name,
            bases: bases.into_iter().map(erase_expr).collect(),
            body: body.into_iter().map(erase_stmt).collect(),
            decorator_list: decorator_list.into_iter().map(erase_expr).collect(),
        },
        StmtKind::If {
            test,
            body,
            elif_clauses,
            else_body,
        } => StmtKind::If {
            test: erase_expr(test),
            body: body.into_iter().map(erase_stmt).collect(),
            elif_clauses: elif_clauses
                .into_iter()
                .map(|(e, stmts)| (erase_expr(e), stmts.into_iter().map(erase_stmt).collect()))
                .collect(),
            else_body: else_body.map(|v| v.into_iter().map(erase_stmt).collect()),
        },
        StmtKind::While {
            test,
            body,
            else_body,
        } => StmtKind::While {
            test: erase_expr(test),
            body: body.into_iter().map(erase_stmt).collect(),
            else_body: else_body.map(|v| v.into_iter().map(erase_stmt).collect()),
        },
        StmtKind::For {
            target,
            iter,
            body,
            else_body,
            is_async,
        } => StmtKind::For {
            target: erase_expr(target),
            iter: erase_expr(iter),
            body: body.into_iter().map(erase_stmt).collect(),
            else_body: else_body.map(|v| v.into_iter().map(erase_stmt).collect()),
            is_async,
        },
        StmtKind::Import { names } => StmtKind::Import { names },
        StmtKind::ImportSideEffect(path) => StmtKind::ImportSideEffect(path),
        StmtKind::ImportFrom {
            module,
            names,
            level,
        } => StmtKind::ImportFrom {
            module,
            names,
            level,
        },
        StmtKind::Try {
            body,
            handlers,
            else_body,
            finally_body,
        } => StmtKind::Try {
            body: body.into_iter().map(erase_stmt).collect(),
            handlers: handlers.into_iter().map(erase_handler).collect(),
            else_body: else_body.map(|v| v.into_iter().map(erase_stmt).collect()),
            finally_body: finally_body.map(|v| v.into_iter().map(erase_stmt).collect()),
        },
        StmtKind::Assert { test, msg } => StmtKind::Assert {
            test: erase_expr(test),
            msg: msg.map(erase_expr),
        },
        StmtKind::Global(v) => StmtKind::Global(v),
        StmtKind::Nonlocal(v) => StmtKind::Nonlocal(v),
        StmtKind::Del(targets) => StmtKind::Del(targets.into_iter().map(erase_expr).collect()),
        StmtKind::With {
            items,
            body,
            is_async,
        } => StmtKind::With {
            items: items.into_iter().map(erase_with_item).collect(),
            body: body.into_iter().map(erase_stmt).collect(),
            is_async,
        },
        StmtKind::Match { subject, cases } => StmtKind::Match {
            subject: erase_expr(subject),
            cases: cases.into_iter().map(erase_match_case).collect(),
        },
    }
}

fn erase_expr(e: Expr) -> Expr {
    Expr {
        kind: erase_expr_kind(e.kind),
        span: Span::dummy(),
    }
}

fn erase_expr_kind(k: ExprKind) -> ExprKind {
    match k {
        ExprKind::IntLiteral(n) => ExprKind::IntLiteral(n),
        ExprKind::FloatLiteral(f) => ExprKind::FloatLiteral(f),
        ExprKind::ImagLiteral(f) => ExprKind::ImagLiteral(f),
        ExprKind::StringLiteral(s) => ExprKind::StringLiteral(s),
        ExprKind::BoolLiteral(b) => ExprKind::BoolLiteral(b),
        ExprKind::NoneLiteral => ExprKind::NoneLiteral,
        ExprKind::Name(n) => ExprKind::Name(n),
        ExprKind::FString { parts } => ExprKind::FString {
            parts: parts
                .into_iter()
                .map(|p| match p {
                    FStringPart::Literal(s) => FStringPart::Literal(s),
                    FStringPart::Expr(e) => FStringPart::Expr(erase_expr(e)),
                })
                .collect(),
        },
        ExprKind::BinOp { left, op, right } => ExprKind::BinOp {
            left: Box::new(erase_expr(*left)),
            op,
            right: Box::new(erase_expr(*right)),
        },
        ExprKind::UnaryOp { op, operand } => ExprKind::UnaryOp {
            op,
            operand: Box::new(erase_expr(*operand)),
        },
        ExprKind::Compare { left, comparisons } => ExprKind::Compare {
            left: Box::new(erase_expr(*left)),
            comparisons: comparisons
                .into_iter()
                .map(|(op, e)| (op, erase_expr(e)))
                .collect(),
        },
        ExprKind::Call {
            func,
            args,
            kwargs,
            optional,
        } => ExprKind::Call {
            func: Box::new(erase_expr(*func)),
            args: args.into_iter().map(erase_expr).collect(),
            kwargs: kwargs.into_iter().map(erase_keyword).collect(),
            optional,
        },
        ExprKind::Attribute {
            value,
            attr,
            optional,
        } => ExprKind::Attribute {
            value: Box::new(erase_expr(*value)),
            attr,
            optional,
        },
        ExprKind::Subscript {
            value,
            index,
            optional,
        } => ExprKind::Subscript {
            value: Box::new(erase_expr(*value)),
            index: Box::new(erase_expr(*index)),
            optional,
        },
        ExprKind::Slice { lower, upper, step } => ExprKind::Slice {
            lower: lower.map(|e| Box::new(erase_expr(*e))),
            upper: upper.map(|e| Box::new(erase_expr(*e))),
            step: step.map(|e| Box::new(erase_expr(*e))),
        },
        ExprKind::List(elts) => ExprKind::List(elts.into_iter().map(erase_expr).collect()),
        ExprKind::Tuple(elts) => ExprKind::Tuple(elts.into_iter().map(erase_expr).collect()),
        ExprKind::Dict { items } => ExprKind::Dict {
            items: items
                .into_iter()
                .map(|item| match item {
                    DictItem::KeyValue { key, value } => DictItem::KeyValue {
                        key: erase_expr(key),
                        value: erase_expr(value),
                    },
                    DictItem::Spread(e) => DictItem::Spread(erase_expr(e)),
                })
                .collect(),
        },
        ExprKind::Set(elts) => ExprKind::Set(elts.into_iter().map(erase_expr).collect()),
        ExprKind::ListComp { elt, generators } => ExprKind::ListComp {
            elt: Box::new(erase_expr(*elt)),
            generators: generators.into_iter().map(erase_comprehension).collect(),
        },
        ExprKind::DictComp {
            key,
            value,
            generators,
        } => ExprKind::DictComp {
            key: Box::new(erase_expr(*key)),
            value: Box::new(erase_expr(*value)),
            generators: generators.into_iter().map(erase_comprehension).collect(),
        },
        ExprKind::SetComp { elt, generators } => ExprKind::SetComp {
            elt: Box::new(erase_expr(*elt)),
            generators: generators.into_iter().map(erase_comprehension).collect(),
        },
        ExprKind::GeneratorExp { elt, generators } => ExprKind::GeneratorExp {
            elt: Box::new(erase_expr(*elt)),
            generators: generators.into_iter().map(erase_comprehension).collect(),
        },
        ExprKind::Lambda { params, body } => ExprKind::Lambda {
            params: params.into_iter().map(erase_param).collect(),
            body: Box::new(erase_expr(*body)),
        },
        ExprKind::IfExpr {
            test,
            body,
            else_body,
        } => ExprKind::IfExpr {
            test: Box::new(erase_expr(*test)),
            body: Box::new(erase_expr(*body)),
            else_body: Box::new(erase_expr(*else_body)),
        },
        ExprKind::Starred(inner) => ExprKind::Starred(Box::new(erase_expr(*inner))),
        ExprKind::Await(inner) => ExprKind::Await(Box::new(erase_expr(*inner))),
        ExprKind::Yield(v) => ExprKind::Yield(v.map(|e| Box::new(erase_expr(*e)))),
        ExprKind::YieldFrom(v) => ExprKind::YieldFrom(Box::new(erase_expr(*v))),
        ExprKind::NamedExpr { target, value } => ExprKind::NamedExpr {
            target: Box::new(erase_expr(*target)),
            value: Box::new(erase_expr(*value)),
        },
    }
}

fn erase_param(p: Param) -> Param {
    Param {
        name: p.name,
        annotation: p.annotation.map(|e| Box::new(erase_expr(*e))),
        default: p.default.map(erase_expr),
        is_args: p.is_args,
        is_kwargs: p.is_kwargs,
        span: Span::dummy(),
    }
}

fn erase_keyword(k: Keyword) -> Keyword {
    Keyword {
        name: k.name,
        value: erase_expr(k.value),
        span: Span::dummy(),
    }
}

fn erase_comprehension(c: Comprehension) -> Comprehension {
    Comprehension {
        target: erase_expr(c.target),
        iter: erase_expr(c.iter),
        ifs: c.ifs.into_iter().map(erase_expr).collect(),
        is_async: c.is_async,
    }
}

fn erase_handler(h: ExceptHandler) -> ExceptHandler {
    ExceptHandler {
        exc_type: h.exc_type.map(erase_expr),
        name: h.name,
        body: h.body.into_iter().map(erase_stmt).collect(),
        span: Span::dummy(),
    }
}

fn erase_with_item(w: WithItem) -> WithItem {
    WithItem {
        context_expr: erase_expr(w.context_expr),
        optional_var: w.optional_var.map(erase_expr),
    }
}

fn erase_match_case(c: MatchCase) -> MatchCase {
    MatchCase {
        pattern: erase_pattern(c.pattern),
        guard: c.guard.map(erase_expr),
        body: c.body.into_iter().map(erase_stmt).collect(),
    }
}

fn erase_pattern(p: Pattern) -> Pattern {
    match p {
        Pattern::Wildcard => Pattern::Wildcard,
        Pattern::Capture(n) => Pattern::Capture(n),
        Pattern::Literal(e) => Pattern::Literal(erase_expr(e)),
        Pattern::Class { cls, args } => Pattern::Class {
            cls,
            args: args.into_iter().map(erase_pattern).collect(),
        },
        Pattern::Sequence(pats) => Pattern::Sequence(pats.into_iter().map(erase_pattern).collect()),
        Pattern::Mapping(pairs) => Pattern::Mapping(
            pairs
                .into_iter()
                .map(|(k, v)| (erase_expr(k), erase_pattern(v)))
                .collect(),
        ),
        Pattern::Or(pats) => Pattern::Or(pats.into_iter().map(erase_pattern).collect()),
        Pattern::As { pattern, name } => Pattern::As {
            pattern: Box::new(erase_pattern(*pattern)),
            name,
        },
        Pattern::Star(n) => Pattern::Star(n),
        Pattern::Value(e) => Pattern::Value(erase_expr(e)),
    }
}

// ── Corpus test ─────────────────────────────────────────────────────────────

#[test]
fn corpus_roundtrips() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")); // crates/pyths_print
    let roots = [
        manifest.join("../../examples"),
        manifest.join("../../fuzz/seed_corpus/fuzz_check"),
    ];
    let mut files = vec![];
    for r in &roots {
        ps_files(r, &mut files);
    }
    assert!(!files.is_empty(), "no .ps corpus found");

    let mut checked = 0usize;
    let mut skipped = 0usize;
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        // Skip intentional error fixtures (error_*.ps) and anything that
        // doesn't parse — the round-trip property only applies to valid source.
        let a = match pyths_parser::parse(&src) {
            Ok(m) => m,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let canon = pyths_print::print_module(&a);
        let b = pyths_parser::parse(&canon).unwrap_or_else(|e| {
            panic!(
                "reparse failed for {}:\n{}\nerr: {:?}",
                f.display(),
                canon,
                e
            )
        });
        // Compare structural AST equality (spans differ between source and canonical form
        // because reprinting renumbers byte offsets — erase spans before comparing).
        let a_erased = erase_module(a);
        let b_erased = erase_module(b);
        assert_eq!(
            a_erased,
            b_erased,
            "AST changed after canonicalize for {}",
            f.display()
        );
        let canon2 = pyths_print::canonicalize(&canon).unwrap();
        assert_eq!(canon, canon2, "not idempotent for {}", f.display());
        checked += 1;
    }
    assert!(checked > 0, "corpus produced no parseable files");
    eprintln!(
        "corpus_roundtrips: {} checked, {} skipped (unparseable/error fixtures)",
        checked, skipped
    );
}
