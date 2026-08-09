use crate::ast::*;

/// Trait for visiting AST nodes. Override methods of interest.
pub trait Visitor {
    fn visit_module(&mut self, module: &Module) {
        for stmt in &module.body {
            self.visit_stmt(stmt);
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Expr(expr) => self.visit_expr(expr),
            StmtKind::Assign { targets, value } => {
                for t in targets {
                    self.visit_expr(t);
                }
                self.visit_expr(value);
            }
            StmtKind::AugAssign { target, value, .. } => {
                self.visit_expr(target);
                self.visit_expr(value);
            }
            StmtKind::FuncDef {
                body,
                decorator_list,
                ..
            } => {
                for d in decorator_list {
                    self.visit_expr(d);
                }
                for s in body {
                    self.visit_stmt(s);
                }
            }
            StmtKind::ClassDef {
                bases,
                body,
                decorator_list,
                ..
            } => {
                for d in decorator_list {
                    self.visit_expr(d);
                }
                for b in bases {
                    self.visit_expr(b);
                }
                for s in body {
                    self.visit_stmt(s);
                }
            }
            StmtKind::Return(expr) => {
                if let Some(e) = expr {
                    self.visit_expr(e);
                }
            }
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                self.visit_expr(test);
                for s in body {
                    self.visit_stmt(s);
                }
                for (cond, stmts) in elif_clauses {
                    self.visit_expr(cond);
                    for s in stmts {
                        self.visit_stmt(s);
                    }
                }
                if let Some(stmts) = else_body {
                    for s in stmts {
                        self.visit_stmt(s);
                    }
                }
            }
            StmtKind::While {
                test,
                body,
                else_body,
            } => {
                self.visit_expr(test);
                for s in body {
                    self.visit_stmt(s);
                }
                if let Some(stmts) = else_body {
                    for s in stmts {
                        self.visit_stmt(s);
                    }
                }
            }
            StmtKind::For {
                target,
                iter,
                body,
                else_body,
                ..
            } => {
                self.visit_expr(target);
                self.visit_expr(iter);
                for s in body {
                    self.visit_stmt(s);
                }
                if let Some(stmts) = else_body {
                    for s in stmts {
                        self.visit_stmt(s);
                    }
                }
            }
            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                for s in body {
                    self.visit_stmt(s);
                }
                for h in handlers {
                    if let Some(e) = &h.exc_type {
                        self.visit_expr(e);
                    }
                    for s in &h.body {
                        self.visit_stmt(s);
                    }
                }
                if let Some(stmts) = else_body {
                    for s in stmts {
                        self.visit_stmt(s);
                    }
                }
                if let Some(stmts) = finally_body {
                    for s in stmts {
                        self.visit_stmt(s);
                    }
                }
            }
            StmtKind::Raise(expr, cause) => {
                if let Some(e) = expr {
                    self.visit_expr(e);
                }
                if let Some(c) = cause {
                    self.visit_expr(c);
                }
            }
            StmtKind::Assert { test, msg } => {
                self.visit_expr(test);
                if let Some(m) = msg {
                    self.visit_expr(m);
                }
            }
            StmtKind::With { items, body, .. } => {
                for item in items {
                    self.visit_expr(&item.context_expr);
                    if let Some(v) = &item.optional_var {
                        self.visit_expr(v);
                    }
                }
                for s in body {
                    self.visit_stmt(s);
                }
            }
            StmtKind::AnnAssign { target, value, .. } => {
                self.visit_expr(target);
                if let Some(v) = value {
                    self.visit_expr(v);
                }
            }
            StmtKind::Match { subject, cases } => {
                self.visit_expr(subject);
                for case in cases {
                    if let Some(guard) = &case.guard {
                        self.visit_expr(guard);
                    }
                    for s in &case.body {
                        self.visit_stmt(s);
                    }
                }
            }
            StmtKind::Import { .. }
            | StmtKind::ImportSideEffect(_)
            | StmtKind::ImportFrom { .. }
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Pass
            | StmtKind::Global(_)
            | StmtKind::Nonlocal(_)
            | StmtKind::Del(_) => {}
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::BinOp { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            ExprKind::UnaryOp { operand, .. } => {
                self.visit_expr(operand);
            }
            ExprKind::Compare { left, comparisons } => {
                self.visit_expr(left);
                for (_, e) in comparisons {
                    self.visit_expr(e);
                }
            }
            ExprKind::Call {
                func, args, kwargs, ..
            } => {
                self.visit_expr(func);
                for a in args {
                    self.visit_expr(a);
                }
                for kw in kwargs {
                    self.visit_expr(&kw.value);
                }
            }
            ExprKind::Attribute { value, .. } => {
                self.visit_expr(value);
            }
            ExprKind::Subscript { value, index, .. } => {
                self.visit_expr(value);
                self.visit_expr(index);
            }
            ExprKind::List(elts) | ExprKind::Tuple(elts) | ExprKind::Set(elts) => {
                for e in elts {
                    self.visit_expr(e);
                }
            }
            ExprKind::Dict { items } => {
                for item in items {
                    match item {
                        DictItem::KeyValue { key, value } => {
                            self.visit_expr(key);
                            self.visit_expr(value);
                        }
                        DictItem::Spread(expr) => {
                            self.visit_expr(expr);
                        }
                    }
                }
            }
            ExprKind::FString { parts } => {
                for p in parts {
                    if let FStringPart::Expr(e) = p {
                        self.visit_expr(e);
                    }
                }
            }
            ExprKind::ListComp { elt, generators } => {
                self.visit_expr(elt);
                for gen in generators {
                    self.visit_expr(&gen.target);
                    self.visit_expr(&gen.iter);
                    for cond in &gen.ifs {
                        self.visit_expr(cond);
                    }
                }
            }
            ExprKind::Lambda { body, .. } => {
                self.visit_expr(body);
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                self.visit_expr(test);
                self.visit_expr(body);
                self.visit_expr(else_body);
            }
            ExprKind::Starred(e) | ExprKind::Await(e) | ExprKind::YieldFrom(e) => {
                self.visit_expr(e);
            }
            ExprKind::Yield(e) => {
                if let Some(e) = e {
                    self.visit_expr(e);
                }
            }
            ExprKind::NamedExpr { target, value } => {
                self.visit_expr(target);
                self.visit_expr(value);
            }
            ExprKind::Slice { lower, upper, step } => {
                if let Some(e) = lower {
                    self.visit_expr(e);
                }
                if let Some(e) = upper {
                    self.visit_expr(e);
                }
                if let Some(e) = step {
                    self.visit_expr(e);
                }
            }
            ExprKind::DictComp {
                key,
                value,
                generators,
            } => {
                self.visit_expr(key);
                self.visit_expr(value);
                for gen in generators {
                    self.visit_expr(&gen.target);
                    self.visit_expr(&gen.iter);
                    for cond in &gen.ifs {
                        self.visit_expr(cond);
                    }
                }
            }
            ExprKind::SetComp { elt, generators } | ExprKind::GeneratorExp { elt, generators } => {
                self.visit_expr(elt);
                for gen in generators {
                    self.visit_expr(&gen.target);
                    self.visit_expr(&gen.iter);
                    for cond in &gen.ifs {
                        self.visit_expr(cond);
                    }
                }
            }
            ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::ImagLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::NoneLiteral
            | ExprKind::Name(_) => {}
        }
    }
}
