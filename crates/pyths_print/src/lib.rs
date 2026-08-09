pub mod printer;

use pyths_syntax::ast::Module;

/// Print an AST module to canonical `.ps` source (comment-stripping).
pub fn print_module(module: &Module) -> String {
    let mut p = printer::PsPrinter::new();
    p.emit_module(module);
    p.finish()
}

/// Parse `source`, then pretty-print it in canonical form.
///
/// - Strips comments (the AST carries none).
/// - Normalises whitespace / indentation.
/// - Idempotent: `canonicalize(canonicalize(s)) == canonicalize(s)`.
pub fn canonicalize(source: &str) -> Result<String, Vec<pyths_parser::ParseError>> {
    let module = pyths_parser::parse(source)?;
    let mut p = printer::PsPrinter::new();
    p.emit_module(&module);
    // B17: the printer never returns garbage silently. If it hit a construct it
    // cannot represent faithfully (an f-string containing both `"` and `'''`),
    // report it as an error instead of handing back invalid `.ps` source that a
    // downstream re-parse or `expand --verify` would misjudge.
    if p.is_malformed() {
        return Err(vec![pyths_parser::ParseError {
            message: "cannot canonicalize: an f-string contains both a double quote \
                      and a triple-single-quote (''') and has no valid .ps representation"
                .to_string(),
            span: pyths_syntax::span::Span::new(0, source.len()),
            notes: vec![
                "rewrite the f-string to avoid mixing \" and ''' in its literal/expression text"
                    .to_string(),
            ],
        }]);
    }
    Ok(p.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotent() {
        let src = "def  f( a ,b ):\n  return a+b\n";
        let once = canonicalize(src).unwrap();
        let twice = canonicalize(&once).unwrap();
        assert_eq!(once, twice, "canonicalize must be idempotent");
    }

    #[test]
    fn strips_comments() {
        let with = canonicalize("x = 1  # hi\n").unwrap();
        let without = canonicalize("x = 1\n").unwrap();
        assert_eq!(with, without, "comments must not affect canonical form");
    }

    #[test]
    fn not_a_rejection_gate() {
        assert!(
            canonicalize("x=1").is_ok(),
            "valid-but-noncanonical Python must be accepted"
        );
    }

    // B17: an f-string whose canonical inner content contains BOTH a `"` and a
    // `'''` cannot be represented in either the f"..." or the f'''...''' form.
    // The printer used to emit knowingly-malformed source; canonicalize now
    // surfaces a clean error instead of handing back garbage.
    #[test]
    fn b17_unrepresentable_fstring_errors_not_garbage() {
        // Triple-single f-string literal holding a real `"` and an escaped `'''`.
        // After decoding, the literal part is `a"b'''c`, which trips the branch.
        let src = "x = f'''a\"b\\'\\'\\'c'''\n";
        // Sanity: it must PARSE (this is valid Python, only the *printer* can't
        // canonicalize it) — otherwise the test is exercising the wrong gate.
        assert!(
            pyths_parser::parse(src).is_ok(),
            "the source must be valid Python so we test the printer, not the parser"
        );
        let result = canonicalize(src);
        assert!(
            result.is_err(),
            "unrepresentable f-string must error, got Ok({:?})",
            result.ok()
        );
        let msg = &result.unwrap_err()[0].message;
        assert!(
            msg.contains("cannot canonicalize"),
            "want a clear malformed-output error, got: {msg}"
        );
    }

    fn ast_eq(src: &str) {
        let canon = canonicalize(src).unwrap_or_else(|e| panic!("canonicalize failed: {:?}", e));
        let a = pyths_parser::parse(src).unwrap();
        let b = pyths_parser::parse(&canon)
            .unwrap_or_else(|e| panic!("reparse failed for {:?}: {:?}", canon, e));
        // Spans differ between source and canonical form (reprinting renumbers byte offsets).
        // Compare only structural (kind-level) equality by zeroing all spans.
        let a_norm = erase_spans_module(a);
        let b_norm = erase_spans_module(b);
        assert_eq!(
            a_norm, b_norm,
            "AST changed:\n  src   = {:?}\n  canon = {:?}",
            src, canon
        );
    }

    // Recursively zero out all spans so that AST comparisons check structure only.
    fn erase_spans_module(m: pyths_syntax::ast::Module) -> pyths_syntax::ast::Module {
        use pyths_syntax::ast::*;
        use pyths_syntax::span::Span;

        fn e_expr(e: Expr) -> Expr {
            Expr {
                kind: e_ek(e.kind),
                span: Span::dummy(),
            }
        }
        fn e_ek(k: ExprKind) -> ExprKind {
            match k {
                ExprKind::BinOp { left, op, right } => ExprKind::BinOp {
                    left: Box::new(e_expr(*left)),
                    op,
                    right: Box::new(e_expr(*right)),
                },
                ExprKind::UnaryOp { op, operand } => ExprKind::UnaryOp {
                    op,
                    operand: Box::new(e_expr(*operand)),
                },
                ExprKind::Compare { left, comparisons } => ExprKind::Compare {
                    left: Box::new(e_expr(*left)),
                    comparisons: comparisons
                        .into_iter()
                        .map(|(op, e)| (op, e_expr(e)))
                        .collect(),
                },
                ExprKind::Call {
                    func,
                    args,
                    kwargs,
                    optional,
                } => ExprKind::Call {
                    func: Box::new(e_expr(*func)),
                    args: args.into_iter().map(e_expr).collect(),
                    kwargs: kwargs
                        .into_iter()
                        .map(|kw| Keyword {
                            name: kw.name,
                            value: e_expr(kw.value),
                            span: Span::dummy(),
                        })
                        .collect(),
                    optional,
                },
                ExprKind::Attribute {
                    value,
                    attr,
                    optional,
                } => ExprKind::Attribute {
                    value: Box::new(e_expr(*value)),
                    attr,
                    optional,
                },
                ExprKind::Subscript {
                    value,
                    index,
                    optional,
                } => ExprKind::Subscript {
                    value: Box::new(e_expr(*value)),
                    index: Box::new(e_expr(*index)),
                    optional,
                },
                ExprKind::Slice { lower, upper, step } => ExprKind::Slice {
                    lower: lower.map(|x| Box::new(e_expr(*x))),
                    upper: upper.map(|x| Box::new(e_expr(*x))),
                    step: step.map(|x| Box::new(e_expr(*x))),
                },
                ExprKind::List(v) => ExprKind::List(v.into_iter().map(e_expr).collect()),
                ExprKind::Tuple(v) => ExprKind::Tuple(v.into_iter().map(e_expr).collect()),
                ExprKind::Set(v) => ExprKind::Set(v.into_iter().map(e_expr).collect()),
                ExprKind::Dict { items } => ExprKind::Dict {
                    items: items
                        .into_iter()
                        .map(|i| match i {
                            DictItem::KeyValue { key, value } => DictItem::KeyValue {
                                key: e_expr(key),
                                value: e_expr(value),
                            },
                            DictItem::Spread(x) => DictItem::Spread(e_expr(x)),
                        })
                        .collect(),
                },
                ExprKind::FString { parts } => ExprKind::FString {
                    parts: parts
                        .into_iter()
                        .map(|p| match p {
                            FStringPart::Literal(s) => FStringPart::Literal(s),
                            FStringPart::Expr(e) => FStringPart::Expr(e_expr(e)),
                        })
                        .collect(),
                },
                ExprKind::ListComp { elt, generators } => ExprKind::ListComp {
                    elt: Box::new(e_expr(*elt)),
                    generators: generators.into_iter().map(e_comp).collect(),
                },
                ExprKind::DictComp {
                    key,
                    value,
                    generators,
                } => ExprKind::DictComp {
                    key: Box::new(e_expr(*key)),
                    value: Box::new(e_expr(*value)),
                    generators: generators.into_iter().map(e_comp).collect(),
                },
                ExprKind::SetComp { elt, generators } => ExprKind::SetComp {
                    elt: Box::new(e_expr(*elt)),
                    generators: generators.into_iter().map(e_comp).collect(),
                },
                ExprKind::GeneratorExp { elt, generators } => ExprKind::GeneratorExp {
                    elt: Box::new(e_expr(*elt)),
                    generators: generators.into_iter().map(e_comp).collect(),
                },
                ExprKind::Lambda { params, body } => ExprKind::Lambda {
                    params: params.into_iter().map(e_param).collect(),
                    body: Box::new(e_expr(*body)),
                },
                ExprKind::IfExpr {
                    test,
                    body,
                    else_body,
                } => ExprKind::IfExpr {
                    test: Box::new(e_expr(*test)),
                    body: Box::new(e_expr(*body)),
                    else_body: Box::new(e_expr(*else_body)),
                },
                ExprKind::Starred(x) => ExprKind::Starred(Box::new(e_expr(*x))),
                ExprKind::Await(x) => ExprKind::Await(Box::new(e_expr(*x))),
                ExprKind::Yield(v) => ExprKind::Yield(v.map(|x| Box::new(e_expr(*x)))),
                ExprKind::YieldFrom(x) => ExprKind::YieldFrom(Box::new(e_expr(*x))),
                ExprKind::NamedExpr { target, value } => ExprKind::NamedExpr {
                    target: Box::new(e_expr(*target)),
                    value: Box::new(e_expr(*value)),
                },
                other => other,
            }
        }
        fn e_comp(c: Comprehension) -> Comprehension {
            Comprehension {
                target: e_expr(c.target),
                iter: e_expr(c.iter),
                ifs: c.ifs.into_iter().map(e_expr).collect(),
                is_async: c.is_async,
            }
        }
        fn e_param(p: Param) -> Param {
            Param {
                name: p.name,
                annotation: p.annotation.map(|x| Box::new(e_expr(*x))),
                default: p.default.map(e_expr),
                is_args: p.is_args,
                is_kwargs: p.is_kwargs,
                span: Span::dummy(),
            }
        }
        fn e_stmt(s: Stmt) -> Stmt {
            Stmt {
                kind: e_sk(s.kind),
                span: Span::dummy(),
            }
        }
        fn e_sk(k: StmtKind) -> StmtKind {
            match k {
                StmtKind::Expr(e) => StmtKind::Expr(e_expr(e)),
                StmtKind::Assign { targets, value } => StmtKind::Assign {
                    targets: targets.into_iter().map(e_expr).collect(),
                    value: e_expr(value),
                },
                StmtKind::AugAssign { target, op, value } => StmtKind::AugAssign {
                    target: e_expr(target),
                    op,
                    value: e_expr(value),
                },
                StmtKind::AnnAssign {
                    target,
                    annotation,
                    value,
                } => StmtKind::AnnAssign {
                    target: e_expr(target),
                    annotation: e_expr(annotation),
                    value: value.map(e_expr),
                },
                StmtKind::Return(v) => StmtKind::Return(v.map(e_expr)),
                StmtKind::Raise(v, c) => StmtKind::Raise(v.map(e_expr), c.map(e_expr)),
                StmtKind::FuncDef {
                    name,
                    params,
                    body,
                    decorator_list,
                    return_type,
                    is_async,
                } => StmtKind::FuncDef {
                    name,
                    params: params.into_iter().map(e_param).collect(),
                    body: body.into_iter().map(e_stmt).collect(),
                    decorator_list: decorator_list.into_iter().map(e_expr).collect(),
                    return_type: return_type.map(e_expr),
                    is_async,
                },
                StmtKind::ClassDef {
                    name,
                    bases,
                    body,
                    decorator_list,
                } => StmtKind::ClassDef {
                    name,
                    bases: bases.into_iter().map(e_expr).collect(),
                    body: body.into_iter().map(e_stmt).collect(),
                    decorator_list: decorator_list.into_iter().map(e_expr).collect(),
                },
                StmtKind::If {
                    test,
                    body,
                    elif_clauses,
                    else_body,
                } => StmtKind::If {
                    test: e_expr(test),
                    body: body.into_iter().map(e_stmt).collect(),
                    elif_clauses: elif_clauses
                        .into_iter()
                        .map(|(e, ss)| (e_expr(e), ss.into_iter().map(e_stmt).collect()))
                        .collect(),
                    else_body: else_body.map(|v| v.into_iter().map(e_stmt).collect()),
                },
                StmtKind::While {
                    test,
                    body,
                    else_body,
                } => StmtKind::While {
                    test: e_expr(test),
                    body: body.into_iter().map(e_stmt).collect(),
                    else_body: else_body.map(|v| v.into_iter().map(e_stmt).collect()),
                },
                StmtKind::For {
                    target,
                    iter,
                    body,
                    else_body,
                    is_async,
                } => StmtKind::For {
                    target: e_expr(target),
                    iter: e_expr(iter),
                    body: body.into_iter().map(e_stmt).collect(),
                    else_body: else_body.map(|v| v.into_iter().map(e_stmt).collect()),
                    is_async,
                },
                StmtKind::Try {
                    body,
                    handlers,
                    else_body,
                    finally_body,
                } => StmtKind::Try {
                    body: body.into_iter().map(e_stmt).collect(),
                    handlers: handlers
                        .into_iter()
                        .map(|h| ExceptHandler {
                            exc_type: h.exc_type.map(e_expr),
                            name: h.name,
                            body: h.body.into_iter().map(e_stmt).collect(),
                            span: Span::dummy(),
                        })
                        .collect(),
                    else_body: else_body.map(|v| v.into_iter().map(e_stmt).collect()),
                    finally_body: finally_body.map(|v| v.into_iter().map(e_stmt).collect()),
                },
                StmtKind::Assert { test, msg } => StmtKind::Assert {
                    test: e_expr(test),
                    msg: msg.map(e_expr),
                },
                StmtKind::Del(v) => StmtKind::Del(v.into_iter().map(e_expr).collect()),
                StmtKind::With {
                    items,
                    body,
                    is_async,
                } => StmtKind::With {
                    items: items
                        .into_iter()
                        .map(|w| WithItem {
                            context_expr: e_expr(w.context_expr),
                            optional_var: w.optional_var.map(e_expr),
                        })
                        .collect(),
                    body: body.into_iter().map(e_stmt).collect(),
                    is_async,
                },
                StmtKind::Match { subject, cases } => StmtKind::Match {
                    subject: e_expr(subject),
                    cases: cases
                        .into_iter()
                        .map(|c| MatchCase {
                            pattern: e_pat(c.pattern),
                            guard: c.guard.map(e_expr),
                            body: c.body.into_iter().map(e_stmt).collect(),
                        })
                        .collect(),
                },
                other => other,
            }
        }
        fn e_pat(p: Pattern) -> Pattern {
            match p {
                Pattern::Literal(e) => Pattern::Literal(e_expr(e)),
                Pattern::Class { cls, args } => Pattern::Class {
                    cls,
                    args: args.into_iter().map(e_pat).collect(),
                },
                Pattern::Sequence(v) => Pattern::Sequence(v.into_iter().map(e_pat).collect()),
                Pattern::Mapping(pairs) => Pattern::Mapping(
                    pairs
                        .into_iter()
                        .map(|(k, v)| (e_expr(k), e_pat(v)))
                        .collect(),
                ),
                Pattern::Or(v) => Pattern::Or(v.into_iter().map(e_pat).collect()),
                Pattern::As { pattern, name } => Pattern::As {
                    pattern: Box::new(e_pat(*pattern)),
                    name,
                },
                Pattern::Value(e) => Pattern::Value(e_expr(e)),
                other => other,
            }
        }
        pyths_syntax::ast::Module {
            body: m.body.into_iter().map(e_stmt).collect(),
            span: Span::dummy(),
        }
    }

    #[test]
    fn precedence_preserved_add_times() {
        // (a + b) * c must NOT flatten to a + b * c
        ast_eq("y = (a + b) * c\n");
        ast_eq("y = a + b * c\n");
        ast_eq("y = a - (b - c)\n");
        ast_eq("y = -(a + b)\n");
    }

    #[test]
    fn fstring_literal_braces_roundtrip() {
        // f-string literal segments containing braces/quotes must survive
        ast_eq("s = f\"{{literal braces}} {x}\"\n");
    }

    #[test]
    fn string_backslash_n_verbatim() {
        // lexer stores `\n` as two chars (backslash + n), not a newline byte;
        // printer must NOT re-escape the backslash
        ast_eq("x = \"a\\nb\"\n");
    }

    #[test]
    fn string_double_backslash_verbatim() {
        // source `\\` stores two backslash chars; must round-trip as `\\`
        ast_eq("x = \"a\\\\b\"\n");
    }

    #[test]
    fn string_with_single_quote() {
        // single-quote inside a double-quoted string — should round-trip fine
        ast_eq("x = \"has ' quote\"\n");
    }

    #[test]
    fn fstring_with_triple_single_quote_in_literal() {
        // f-string whose literal part contains `'''` — the canonical printer
        // must not emit f'''...''' in that case (would be unbalanced / malformed)
        ast_eq("s = f\"has ''' inside {y}\"\n");
    }

    // --- A2: side-effect string-literal import (`import "./styles.css"`) ---
    //
    // Iron Rule check: every statement the parser accepts must be printable
    // back to canonical form, or `expand --verify`'s round-trip breaks for
    // any file using this statement.

    #[test]
    fn import_side_effect_roundtrip_canonical_form() {
        // parse -> print must reproduce the exact canonical form.
        assert_eq!(
            canonicalize("import \"./styles.css\"\n").unwrap(),
            "import \"./styles.css\"\n"
        );
        // Also accepts (and canonicalizes) non-canonical spacing.
        assert_eq!(
            canonicalize("import   \"./styles.css\"").unwrap(),
            "import \"./styles.css\"\n"
        );
    }

    #[test]
    fn import_side_effect_roundtrip_ast_structural() {
        ast_eq("import \"./styles.css\"\n");
        ast_eq("import \"./assets/logo.png\"\n");
    }
}
