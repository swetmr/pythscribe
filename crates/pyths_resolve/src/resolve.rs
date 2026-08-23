use std::collections::{HashMap, HashSet};

use crate::scope::{Scope, ScopeKind, SymbolInfo};
use pyths_syntax::ast::*;

/// Result of name resolution. Provides scope information per AST node.
#[derive(Debug, Clone)]
pub struct ResolveInfo {
    /// All scopes collected during resolution.
    pub scopes: Vec<Scope>,
    /// Map from scope index → set of names that are assigned in that scope.
    /// Available for future phases (type checking, closure analysis).
    #[allow(dead_code)]
    assigned_names: HashMap<usize, HashSet<String>>,
}

impl ResolveInfo {
    /// Check if a name has been previously defined in a scope
    /// (parameter, import, or prior assignment). The codegen calls this
    /// to decide whether to emit `let` (first assignment) or nothing
    /// (reassignment).
    pub fn is_defined_in_scope(&self, scope_id: usize, name: &str) -> bool {
        if let Some(scope) = self.scopes.get(scope_id) {
            if let Some(info) = scope.symbols.get(name) {
                return info.is_param || info.is_imported || info.is_global || info.is_nonlocal;
            }
        }
        false
    }

    /// Get symbol info for a name in a given scope.
    pub fn get_symbol(&self, scope_id: usize, name: &str) -> Option<&SymbolInfo> {
        self.scopes.get(scope_id).and_then(|s| s.symbols.get(name))
    }
}

/// Resolve names in a module, building scope and symbol information.
pub fn resolve(module: &Module) -> ResolveInfo {
    let mut resolver = Resolver::new();
    resolver.resolve_module(module);
    ResolveInfo {
        scopes: resolver.scopes,
        assigned_names: resolver.assigned_names,
    }
}

struct Resolver {
    scopes: Vec<Scope>,
    current_scope: usize,
    scope_stack: Vec<usize>,
    assigned_names: HashMap<usize, HashSet<String>>,
}

impl Resolver {
    fn new() -> Self {
        // Create the module scope
        let module_scope = Scope::new(ScopeKind::Module, None);
        Self {
            scopes: vec![module_scope],
            current_scope: 0,
            scope_stack: vec![0],
            assigned_names: HashMap::new(),
        }
    }

    fn push_scope(&mut self, kind: ScopeKind) -> usize {
        let id = self.scopes.len();
        self.scopes.push(Scope::new(kind, Some(self.current_scope)));
        self.scope_stack.push(id);
        self.current_scope = id;
        id
    }

    fn pop_scope(&mut self) {
        self.scope_stack.pop();
        self.current_scope = *self.scope_stack.last().unwrap();
    }

    fn current(&mut self) -> &mut Scope {
        &mut self.scopes[self.current_scope]
    }

    fn mark_assigned(&mut self, name: &str) {
        self.current().mark_assigned(name);
        self.assigned_names
            .entry(self.current_scope)
            .or_default()
            .insert(name.to_string());
    }

    /// Mark a name as referenced by walking up the scope chain.
    fn mark_referenced(&mut self, name: &str) {
        let mut scope_id = self.current_scope;
        loop {
            if let Some(info) = self.scopes[scope_id].symbols.get_mut(name) {
                info.is_referenced = true;
                return;
            }
            if let Some(parent) = self.scopes[scope_id].parent {
                scope_id = parent;
            } else {
                break;
            }
        }
    }

    fn resolve_module(&mut self, module: &Module) {
        for stmt in &module.body {
            self.resolve_stmt(stmt);
        }
    }

    fn resolve_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Assign { targets, value } => {
                // Resolve value first (RHS before LHS)
                self.resolve_expr(value);
                // Mark all assignment targets
                for target in targets {
                    self.resolve_assign_target(target);
                }
            }
            StmtKind::AugAssign { target, value, .. } => {
                self.resolve_expr(value);
                self.resolve_expr(target);
                // AugAssign implies the target already exists, but also assigns
                if let ExprKind::Name(name) = &target.kind {
                    self.mark_assigned(name);
                }
            }
            StmtKind::FuncDef {
                name,
                params,
                body,
                decorator_list,
                ..
            } => {
                // The function name is defined in the enclosing scope
                self.mark_assigned(name);

                // Resolve decorators in enclosing scope
                for dec in decorator_list {
                    self.resolve_expr(dec);
                }

                // Push function scope
                self.push_scope(ScopeKind::Function);

                // Mark parameters
                for param in params {
                    self.current().mark_param(&param.name);
                    if let Some(default) = &param.default {
                        self.resolve_expr(default);
                    }
                }

                // Resolve body
                for s in body {
                    self.resolve_stmt(s);
                }

                self.pop_scope();
            }
            StmtKind::ClassDef {
                name,
                bases,
                body,
                decorator_list,
            } => {
                self.mark_assigned(name);

                for dec in decorator_list {
                    self.resolve_expr(dec);
                }
                for base in bases {
                    self.resolve_expr(base);
                }

                self.push_scope(ScopeKind::Class);
                for s in body {
                    self.resolve_stmt(s);
                }
                self.pop_scope();
            }
            StmtKind::Return(expr) => {
                if let Some(e) = expr {
                    self.resolve_expr(e);
                }
            }
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                self.resolve_expr(test);
                for s in body {
                    self.resolve_stmt(s);
                }
                for (cond, stmts) in elif_clauses {
                    self.resolve_expr(cond);
                    for s in stmts {
                        self.resolve_stmt(s);
                    }
                }
                if let Some(stmts) = else_body {
                    for s in stmts {
                        self.resolve_stmt(s);
                    }
                }
            }
            StmtKind::While {
                test,
                body,
                else_body,
            } => {
                self.resolve_expr(test);
                for s in body {
                    self.resolve_stmt(s);
                }
                if let Some(stmts) = else_body {
                    for s in stmts {
                        self.resolve_stmt(s);
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
                self.resolve_expr(iter);
                self.resolve_assign_target(target);
                for s in body {
                    self.resolve_stmt(s);
                }
                if let Some(stmts) = else_body {
                    for s in stmts {
                        self.resolve_stmt(s);
                    }
                }
            }
            StmtKind::Import { names } => {
                for alias in names {
                    let local = alias.alias.as_deref().unwrap_or(&alias.name);
                    self.current().mark_imported(local);
                }
            }
            StmtKind::ImportFrom { names, .. } => {
                for alias in names {
                    let local = alias.alias.as_deref().unwrap_or(&alias.name);
                    self.current().mark_imported(local);
                }
            }
            // Side-effect import (`import "./styles.css"`) binds no name —
            // nothing to add to the symbol table, so W002 (unused import)
            // never fires for it.
            StmtKind::ImportSideEffect(_) => {}
            StmtKind::Global(names) => {
                for name in names {
                    self.current().mark_global(name);
                }
            }
            StmtKind::Nonlocal(names) => {
                for name in names {
                    self.current().mark_nonlocal(name);
                }
            }
            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                for s in body {
                    self.resolve_stmt(s);
                }
                for handler in handlers {
                    if let Some(exc_type) = &handler.exc_type {
                        self.resolve_expr(exc_type);
                    }
                    if let Some(name) = &handler.name {
                        self.mark_assigned(name);
                    }
                    for s in &handler.body {
                        self.resolve_stmt(s);
                    }
                }
                if let Some(stmts) = else_body {
                    for s in stmts {
                        self.resolve_stmt(s);
                    }
                }
                if let Some(stmts) = finally_body {
                    for s in stmts {
                        self.resolve_stmt(s);
                    }
                }
            }
            StmtKind::Raise(expr, cause) => {
                if let Some(e) = expr {
                    self.resolve_expr(e);
                }
                if let Some(c) = cause {
                    self.resolve_expr(c);
                }
            }
            StmtKind::Assert { test, msg } => {
                self.resolve_expr(test);
                if let Some(m) = msg {
                    self.resolve_expr(m);
                }
            }
            StmtKind::Del(targets) => {
                for t in targets {
                    self.resolve_expr(t);
                }
            }
            StmtKind::With { items, body, .. } => {
                for item in items {
                    self.resolve_expr(&item.context_expr);
                    if let Some(var) = &item.optional_var {
                        self.resolve_assign_target(var);
                    }
                }
                for s in body {
                    self.resolve_stmt(s);
                }
            }
            StmtKind::AnnAssign { target, value, .. } => {
                if let Some(v) = value {
                    self.resolve_expr(v);
                }
                self.resolve_assign_target(target);
            }
            StmtKind::Match { subject, cases } => {
                self.resolve_expr(subject);
                for case in cases {
                    if let Some(guard) = &case.guard {
                        self.resolve_expr(guard);
                    }
                    // Mark pattern captures as assigned
                    self.resolve_pattern_bindings(&case.pattern);
                    for s in &case.body {
                        self.resolve_stmt(s);
                    }
                }
            }
            StmtKind::Expr(expr) => {
                self.resolve_expr(expr);
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {}
        }
    }

    fn resolve_assign_target(&mut self, target: &Expr) {
        match &target.kind {
            ExprKind::Name(name) => {
                self.mark_assigned(name);
            }
            ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                for elt in elts {
                    self.resolve_assign_target(elt);
                }
            }
            ExprKind::Starred(inner) => {
                self.resolve_assign_target(inner);
            }
            ExprKind::Attribute { value, .. } => {
                self.resolve_expr(value);
            }
            ExprKind::Subscript { value, index, .. } => {
                self.resolve_expr(value);
                self.resolve_expr(index);
            }
            _ => {
                self.resolve_expr(target);
            }
        }
    }

    fn resolve_pattern_bindings(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Capture(name) => self.mark_assigned(name),
            Pattern::Sequence(pats) => {
                for p in pats {
                    self.resolve_pattern_bindings(p);
                }
            }
            Pattern::Mapping(pairs) => {
                for (_, p) in pairs {
                    self.resolve_pattern_bindings(p);
                }
            }
            Pattern::Class { args, .. } => {
                for p in args {
                    self.resolve_pattern_bindings(p);
                }
            }
            Pattern::Or(alts) => {
                for p in alts {
                    self.resolve_pattern_bindings(p);
                }
            }
            Pattern::As { pattern, name } => {
                self.mark_assigned(name);
                self.resolve_pattern_bindings(pattern);
            }
            Pattern::Star(Some(name)) => self.mark_assigned(name),
            Pattern::Wildcard | Pattern::Literal(_) | Pattern::Value(_) | Pattern::Star(None) => {}
        }
    }

    fn resolve_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::BinOp { left, right, .. } => {
                self.resolve_expr(left);
                self.resolve_expr(right);
            }
            ExprKind::UnaryOp { operand, .. } => {
                self.resolve_expr(operand);
            }
            ExprKind::Compare { left, comparisons } => {
                self.resolve_expr(left);
                for (_, e) in comparisons {
                    self.resolve_expr(e);
                }
            }
            ExprKind::Call {
                func, args, kwargs, ..
            } => {
                self.resolve_expr(func);
                for a in args {
                    self.resolve_expr(a);
                }
                for kw in kwargs {
                    self.resolve_expr(&kw.value);
                }
            }
            ExprKind::Attribute { value, .. } => {
                self.resolve_expr(value);
            }
            ExprKind::Subscript { value, index, .. } => {
                self.resolve_expr(value);
                self.resolve_expr(index);
            }
            ExprKind::List(elts) | ExprKind::Tuple(elts) | ExprKind::Set(elts) => {
                for e in elts {
                    self.resolve_expr(e);
                }
            }
            ExprKind::Dict { items } => {
                for item in items {
                    match item {
                        DictItem::KeyValue { key, value } => {
                            self.resolve_expr(key);
                            self.resolve_expr(value);
                        }
                        DictItem::Spread(expr) => {
                            self.resolve_expr(expr);
                        }
                    }
                }
            }
            ExprKind::FString { parts } => {
                for p in parts {
                    if let FStringPart::Expr(e) = p {
                        self.resolve_expr(e);
                    }
                }
            }
            ExprKind::ListComp { elt, generators }
            | ExprKind::SetComp { elt, generators }
            | ExprKind::GeneratorExp { elt, generators } => {
                // Comprehensions create an implicit scope
                self.push_scope(ScopeKind::Function);
                for gen in generators {
                    self.resolve_expr(&gen.iter);
                    self.resolve_assign_target(&gen.target);
                    for cond in &gen.ifs {
                        self.resolve_expr(cond);
                    }
                }
                self.resolve_expr(elt);
                self.pop_scope();
            }
            ExprKind::DictComp {
                key,
                value,
                generators,
            } => {
                self.push_scope(ScopeKind::Function);
                for gen in generators {
                    self.resolve_expr(&gen.iter);
                    self.resolve_assign_target(&gen.target);
                    for cond in &gen.ifs {
                        self.resolve_expr(cond);
                    }
                }
                self.resolve_expr(key);
                self.resolve_expr(value);
                self.pop_scope();
            }
            ExprKind::Lambda { params, body } => {
                self.push_scope(ScopeKind::Function);
                for param in params {
                    self.current().mark_param(&param.name);
                }
                self.resolve_expr(body);
                self.pop_scope();
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                self.resolve_expr(test);
                self.resolve_expr(body);
                self.resolve_expr(else_body);
            }
            ExprKind::Starred(e) | ExprKind::Await(e) | ExprKind::YieldFrom(e) => {
                self.resolve_expr(e);
            }
            ExprKind::Yield(e) => {
                if let Some(e) = e {
                    self.resolve_expr(e);
                }
            }
            ExprKind::NamedExpr { target, value } => {
                self.resolve_expr(value);
                // Walrus operator defines the name in the enclosing non-comprehension scope
                if let ExprKind::Name(name) = &target.kind {
                    self.mark_assigned(name);
                }
            }
            ExprKind::Slice { lower, upper, step } => {
                if let Some(e) = lower {
                    self.resolve_expr(e);
                }
                if let Some(e) = upper {
                    self.resolve_expr(e);
                }
                if let Some(e) = step {
                    self.resolve_expr(e);
                }
            }
            ExprKind::Name(name) => {
                self.mark_referenced(name);
            }
            ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::ImagLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BytesLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::NoneLiteral => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_resolve(source: &str) -> ResolveInfo {
        let module = pyths_parser::parse(source).expect("parse failed");
        resolve(&module)
    }

    #[test]
    fn test_module_scope_assignment() {
        let info = parse_and_resolve("x = 1\ny = 2");
        let scope = &info.scopes[0];
        assert!(scope.symbols.contains_key("x"));
        assert!(scope.symbols.contains_key("y"));
        assert!(scope.symbols["x"].is_assigned);
        assert!(scope.symbols["y"].is_assigned);
    }

    #[test]
    fn test_function_scope() {
        let info = parse_and_resolve("def foo(a, b):\n    c = a + b\n    return c");
        // scope 0 = module, scope 1 = foo
        assert_eq!(info.scopes.len(), 2);
        let func_scope = &info.scopes[1];
        assert!(func_scope.symbols["a"].is_param);
        assert!(func_scope.symbols["b"].is_param);
        assert!(func_scope.symbols["c"].is_assigned);

        // foo is defined in module scope
        assert!(info.scopes[0].symbols["foo"].is_assigned);
    }

    #[test]
    fn test_global_declaration() {
        let info = parse_and_resolve("x = 0\ndef inc():\n    global x\n    x = x + 1");
        let func_scope = &info.scopes[1];
        assert!(func_scope.symbols["x"].is_global);
    }

    #[test]
    fn test_import_marks_imported() {
        let info = parse_and_resolve("from react import use_state\nimport os");
        let scope = &info.scopes[0];
        assert!(scope.symbols["use_state"].is_imported);
        assert!(scope.symbols["os"].is_imported);
    }

    #[test]
    fn test_class_scope() {
        let info = parse_and_resolve("class Foo:\n    x = 1\n    def bar(self):\n        pass");
        // scope 0 = module, scope 1 = class Foo, scope 2 = bar
        assert_eq!(info.scopes.len(), 3);
        assert_eq!(info.scopes[1].kind, ScopeKind::Class);
        assert_eq!(info.scopes[2].kind, ScopeKind::Function);
    }

    #[test]
    fn test_for_loop_target() {
        let info = parse_and_resolve("for i in range(10):\n    print(i)");
        assert!(info.scopes[0].symbols["i"].is_assigned);
    }

    #[test]
    fn test_tuple_unpacking_target() {
        let info = parse_and_resolve("a, b = 1, 2");
        assert!(info.scopes[0].symbols["a"].is_assigned);
        assert!(info.scopes[0].symbols["b"].is_assigned);
    }

    #[test]
    fn test_comprehension_scope() {
        let info = parse_and_resolve("result = [x * 2 for x in items]");
        // scope 0 = module, scope 1 = comprehension
        assert_eq!(info.scopes.len(), 2);
        assert!(info.scopes[0].symbols.contains_key("result"));
        // x is defined in the comprehension scope, not module
        assert!(!info.scopes[0].symbols.contains_key("x"));
        assert!(info.scopes[1].symbols.contains_key("x"));
    }
}
