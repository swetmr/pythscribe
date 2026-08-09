use std::collections::HashMap;
use std::path::PathBuf;

use pyths_syntax::ast::*;
use pyths_syntax::operators::{BinOp, UnaryOp};

use crate::errors::TypeError;
use crate::types::{self, Type};

/// A basic type checker for PythScribe programs.
pub struct TypeChecker {
    errors: Vec<TypeError>,
    /// Function signatures: name → (params: [(name, type, has_default)], return_type)
    function_sigs: HashMap<String, FuncSig>,
    /// Stack of variable type scopes (for function bodies, branches, etc.)
    scopes: Vec<HashMap<String, Type>>,
    /// Project-local `.pyi` stub directories, searched (in order) before
    /// the bundled stub table. See `crate::stubs::resolve_stub`.
    stub_paths: Vec<PathBuf>,
}

/// A collected function signature.
#[derive(Debug, Clone)]
struct FuncSig {
    params: Vec<(String, Type, bool)>, // (name, type, has_default)
    return_type: Type,
    has_args: bool, // *args parameter
}

impl TypeChecker {
    pub fn check(module: &Module) -> Vec<TypeError> {
        Self::check_with_stub_paths(module, &[])
    }

    /// Type-check `module` consulting the given project-local stub
    /// directories before bundled stubs. Pass an empty slice to get
    /// the bundled-only behavior (identical to [`check`]).
    pub fn check_with_stub_paths(module: &Module, stub_paths: &[PathBuf]) -> Vec<TypeError> {
        let mut checker = Self {
            errors: Vec::new(),
            function_sigs: HashMap::new(),
            scopes: vec![HashMap::new()], // module scope
            stub_paths: stub_paths.to_vec(),
        };

        // First pass: collect function signatures
        for stmt in &module.body {
            checker.collect_signatures(stmt);
        }

        // Second pass: check statements
        for stmt in &module.body {
            checker.check_stmt(stmt);
        }

        checker.errors
    }

    // ── Scope management ──────────────────────────────────

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Look up a variable's type across all scopes (innermost first).
    fn lookup_var(&self, name: &str) -> Option<&Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    /// Set a variable's type in the current (innermost) scope.
    fn set_var(&mut self, name: &str, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    // ── Pass 1: Signature collection ──────────────────────

    fn collect_signatures(&mut self, stmt: &Stmt) {
        if let StmtKind::FuncDef {
            name,
            params,
            return_type,
            ..
        } = &stmt.kind
        {
            let mut has_args = false;
            let param_types: Vec<(String, Type, bool)> = params
                .iter()
                .filter(|p| p.name != "self" && p.name != "cls")
                .map(|p| {
                    if p.is_args {
                        has_args = true;
                    }
                    let ty = p
                        .annotation
                        .as_ref()
                        .map(|a| types::resolve_type(a))
                        .unwrap_or(Type::Any);
                    let has_default = p.default.is_some();
                    (p.name.clone(), ty, has_default)
                })
                .collect();

            let ret_type = return_type
                .as_ref()
                .map(types::resolve_type)
                .unwrap_or(Type::Any);

            self.function_sigs.insert(
                name.clone(),
                FuncSig {
                    params: param_types,
                    return_type: ret_type,
                    has_args,
                },
            );
        }
    }

    // ── Pass 2: Statement checking ────────────────────────

    fn check_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::AnnAssign {
                target,
                annotation,
                value,
            } => {
                self.check_ann_assign(target, annotation, value.as_ref(), stmt.span);
            }
            StmtKind::FuncDef {
                name,
                params,
                body,
                return_type,
                ..
            } => {
                self.check_func_def(name, params, body, return_type.as_ref());
            }
            StmtKind::Assign { targets, value } => {
                self.check_assign(targets, value, stmt.span);
            }
            StmtKind::Expr(expr) => {
                // Check function calls at statement level
                self.check_expr_calls(expr);
            }
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                // Check the condition expression
                self.check_expr_calls(test);

                // Narrow types in branches
                let narrowings = self.extract_narrowings(test);

                // Check if-body with narrowed types
                self.push_scope();
                for (name, ty) in &narrowings {
                    self.set_var(name, ty.clone());
                }
                for s in body {
                    self.check_stmt(s);
                }
                self.pop_scope();

                // Check elif clauses
                for (cond, clause_body) in elif_clauses {
                    self.check_expr_calls(cond);
                    self.push_scope();
                    let elif_narrowings = self.extract_narrowings(cond);
                    for (name, ty) in &elif_narrowings {
                        self.set_var(name, ty.clone());
                    }
                    for s in clause_body {
                        self.check_stmt(s);
                    }
                    self.pop_scope();
                }

                // Check else body
                if let Some(eb) = else_body {
                    self.push_scope();
                    for s in eb {
                        self.check_stmt(s);
                    }
                    self.pop_scope();
                }
            }
            StmtKind::While { test, body, .. } => {
                self.check_expr_calls(test);
                for s in body {
                    self.check_stmt(s);
                }
            }
            StmtKind::For { body, .. } => {
                for s in body {
                    self.check_stmt(s);
                }
            }
            StmtKind::Return(Some(expr)) => {
                // Infer type for standalone return checks
                self.check_expr_calls(expr);
            }
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                // Stub lookup only makes sense for absolute imports (level=0)
                // pointing at a known npm/stdlib module. Relative imports
                // resolve to local source files; their types come from
                // re-parsing those files, not from bundled .pyi stubs.
                if *level == 0 {
                    self.import_stubs(module, names);
                }
            }
            _ => {}
        }
    }

    /// Walk a stub `.pyi` file's top-level FuncDef / ClassDef nodes,
    /// build `Type` representations from their annotations, and bind
    /// the requested names in the current scope. Names that aren't in
    /// the stub fall back to `Type::Any` (silent — stubs are
    /// best-effort, missing entries don't fail compilation).
    fn import_stubs(&mut self, module: &str, names: &[pyths_syntax::ast::ImportAlias]) {
        let stub_src = match crate::stubs::resolve_stub(module, &self.stub_paths) {
            Some(src) => src,
            None => {
                // No stub — bind every imported name as `Any` so the
                // checker doesn't flag references to it as undefined.
                for alias in names {
                    let local = alias.alias.as_deref().unwrap_or(&alias.name);
                    self.set_var(local, Type::Any);
                }
                return;
            }
        };

        let module_ast = match pyths_parser::parse(&stub_src) {
            Ok(m) => m,
            Err(_) => {
                // Malformed stub — fall back to Any. (Not user-facing;
                // a parse error here is an internal bug to fix.)
                for alias in names {
                    let local = alias.alias.as_deref().unwrap_or(&alias.name);
                    self.set_var(local, Type::Any);
                }
                return;
            }
        };

        // Index every top-level FuncDef / ClassDef by name → Type.
        let mut stub_types: std::collections::HashMap<String, Type> =
            std::collections::HashMap::new();
        for stmt in &module_ast.body {
            match &stmt.kind {
                StmtKind::FuncDef {
                    name,
                    params,
                    return_type,
                    ..
                } => {
                    let param_types: Vec<Type> = params
                        .iter()
                        .filter(|p| p.name != "self" && p.name != "cls")
                        .map(|p| {
                            p.annotation
                                .as_ref()
                                .map(|a| types::resolve_type(a))
                                .unwrap_or(Type::Any)
                        })
                        .collect();
                    let ret = return_type
                        .as_ref()
                        .map(types::resolve_type)
                        .unwrap_or(Type::Any);
                    stub_types.insert(name.clone(), Type::Callable(param_types, Box::new(ret)));
                }
                StmtKind::ClassDef { name, .. } => {
                    // Classes appear as the named type — instantiating
                    // them or referring to them yields the named type.
                    stub_types.insert(name.clone(), Type::Named(name.clone()));
                }
                _ => {}
            }
        }

        // Bind each requested import to its stub-resolved type, falling
        // back to Any for names the stub doesn't cover.
        for alias in names {
            let local = alias.alias.as_deref().unwrap_or(&alias.name);
            let ty = stub_types.get(&alias.name).cloned().unwrap_or(Type::Any);
            self.set_var(local, ty);
        }
    }

    fn check_ann_assign(
        &mut self,
        target: &Expr,
        annotation: &Expr,
        value: Option<&Expr>,
        span: pyths_syntax::span::Span,
    ) {
        let expected_type = types::resolve_type(annotation);

        // Record variable type
        if let ExprKind::Name(name) = &target.kind {
            self.set_var(name, expected_type.clone());
        }

        // Check value if present
        if let Some(val) = value {
            self.check_expr_calls(val);
            if let Some(actual_type) = self.infer_expr_type(val) {
                if !types::is_assignable(&expected_type, &actual_type) {
                    self.errors.push(
                        TypeError::new(
                            format!(
                                "Type mismatch: expected {}, got {}",
                                expected_type, actual_type
                            ),
                            span,
                        )
                        .with_note(format!(
                            "The annotated type is {} but the value is {}",
                            expected_type, actual_type
                        )),
                    );
                }
            }
        }
    }

    fn check_assign(&mut self, targets: &[Expr], value: &Expr, span: pyths_syntax::span::Span) {
        // Walk the RHS for expression-level diagnostics (e.g. mixed
        // str/numeric `+`) before binding targets.
        self.check_expr_calls(value);

        let value_type = self.infer_expr_type(value);

        for target in targets {
            self.bind_target(target, value_type.as_ref(), span);
        }
    }

    /// Bind a single LHS expression to the inferred value type, recursing
    /// into tuple-destructure cases. Examples:
    ///
    /// * `x = e` — bind `x` to `type_of(e)`.
    /// * `a, b = e` — LHS is `ExprKind::Tuple([a, b])`; if `e` has type
    ///   `Tuple[T1, T2]`, bind `a: T1` and `b: T2`. This is the path
    ///   that makes generic hook returns precise:
    ///   `count, set_count = use_state(0)` now types `count` as `int`
    ///   and `set_count` as `Callable[[int], None]` instead of `Any`.
    /// * If the value isn't tuple-typed but the target is a tuple, each
    ///   element binds to the whole value type (the loose fallback —
    ///   better to over-type than to drop the binding entirely).
    fn bind_target(
        &mut self,
        target: &Expr,
        value_type: Option<&Type>,
        span: pyths_syntax::span::Span,
    ) {
        match &target.kind {
            ExprKind::Name(name) => {
                if let Some(expected_type) = self.lookup_var(name).cloned() {
                    // Variable was previously typed — check compatibility
                    if let Some(actual_type) = value_type {
                        if !types::is_assignable(&expected_type, actual_type) {
                            self.errors.push(
                                TypeError::new(
                                    format!(
                                        "Type mismatch: cannot assign {} to variable '{}' of type {}",
                                        actual_type, name, expected_type
                                    ),
                                    span,
                                )
                                .with_note(format!(
                                    "'{}' was declared as {}",
                                    name, expected_type
                                )),
                            );
                        }
                    }
                } else if let Some(ty) = value_type {
                    // First assignment — track inferred type
                    self.set_var(name, ty.clone());
                }
            }
            ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                // Tuple/list destructure. If the RHS is itself a Tuple
                // of matching arity, propagate element-wise types.
                // Otherwise fall back to binding each target to the
                // whole value type (which becomes `Any` for an
                // unknown RHS).
                let elt_types: Vec<Option<Type>> = match value_type {
                    Some(Type::Tuple(ts)) if ts.len() == elts.len() => {
                        ts.iter().map(|t| Some(t.clone())).collect()
                    }
                    // List on the RHS: every element binds to the
                    // list's element type.
                    Some(Type::List(inner)) => {
                        vec![Some((**inner).clone()); elts.len()]
                    }
                    Some(other) => vec![Some(other.clone()); elts.len()],
                    None => vec![None; elts.len()],
                };
                for (elt, ty) in elts.iter().zip(elt_types.iter()) {
                    self.bind_target(elt, ty.as_ref(), span);
                }
            }
            ExprKind::Starred(inner) => {
                // `a, *rest, b = ...` — rest captures the middle as
                // a list. We don't yet refine its element type; bind
                // as List[Any] when the RHS is a known sequence.
                if let ExprKind::Name(n) = &inner.kind {
                    self.set_var(n, Type::List(Box::new(Type::Any)));
                }
            }
            _ => {
                // Attribute / subscript targets — not yet refined
                // by the type checker. No-op for now (intentional;
                // narrowing through .attr or [i] is a separate gap).
            }
        }
    }

    fn check_func_def(
        &mut self,
        _name: &str,
        params: &[Param],
        body: &[Stmt],
        return_type: Option<&Expr>,
    ) {
        // Push function scope and register parameters
        self.push_scope();
        for p in params {
            if p.name != "self" && p.name != "cls" {
                let ty = p
                    .annotation
                    .as_ref()
                    .map(|a| types::resolve_type(a))
                    .unwrap_or(Type::Any);
                self.set_var(&p.name, ty);
            }
        }

        // Check body statements
        for stmt in body {
            self.check_stmt(stmt);
        }

        // Check return type if annotated
        if let Some(ret_ann) = return_type {
            let expected_ret = types::resolve_type(ret_ann);
            if expected_ret != Type::NoneType {
                for stmt in body {
                    self.check_return_type(stmt, &expected_ret);
                }
            }
        }

        self.pop_scope();
    }

    fn check_return_type(&mut self, stmt: &Stmt, expected: &Type) {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                if let Some(actual_type) = self.infer_expr_type(expr) {
                    if !types::is_assignable(expected, &actual_type) {
                        self.errors.push(
                            TypeError::new(
                                format!(
                                    "Return type mismatch: expected {}, got {}",
                                    expected, actual_type
                                ),
                                stmt.span,
                            )
                            .with_note(format!(
                                "Function declares return type {} but returns {}",
                                expected, actual_type
                            )),
                        );
                    }
                }
            }
            StmtKind::If {
                body,
                elif_clauses,
                else_body,
                ..
            } => {
                for s in body {
                    self.check_return_type(s, expected);
                }
                for (_, clause_body) in elif_clauses {
                    for s in clause_body {
                        self.check_return_type(s, expected);
                    }
                }
                if let Some(eb) = else_body {
                    for s in eb {
                        self.check_return_type(s, expected);
                    }
                }
            }
            _ => {}
        }
    }

    // ── Expression type inference ─────────────────────────

    /// Infer the type of any expression.
    fn infer_expr_type(&self, expr: &Expr) -> Option<Type> {
        match &expr.kind {
            // Literals
            ExprKind::IntLiteral(_) => Some(Type::Int),
            ExprKind::FloatLiteral(_) => Some(Type::Float),
            ExprKind::StringLiteral(_) => Some(Type::Str),
            ExprKind::FString { .. } => Some(Type::Str),
            ExprKind::BoolLiteral(_) => Some(Type::Bool),
            ExprKind::NoneLiteral => Some(Type::NoneType),

            // Collections
            ExprKind::List(elts) => {
                let inner = self.infer_homogeneous_expr_type(elts);
                Some(Type::List(Box::new(inner)))
            }
            ExprKind::Dict { .. } => Some(Type::Dict(Box::new(Type::Any), Box::new(Type::Any))),
            ExprKind::Set(elts) => {
                let inner = self.infer_homogeneous_expr_type(elts);
                Some(Type::Set(Box::new(inner)))
            }
            ExprKind::Tuple(elts) => {
                let types: Vec<_> = elts
                    .iter()
                    .map(|e| self.infer_expr_type(e).unwrap_or(Type::Any))
                    .collect();
                Some(Type::Tuple(types))
            }

            // Variable reference — look up in scope
            ExprKind::Name(name) => self.lookup_var(name).cloned(),

            // Binary operations
            ExprKind::BinOp { left, op, right } => {
                let lt = self.infer_expr_type(left).unwrap_or(Type::Any);
                let rt = self.infer_expr_type(right).unwrap_or(Type::Any);
                Some(types::infer_binop_type(&lt, *op, &rt))
            }

            // Unary operations
            ExprKind::UnaryOp { op, operand } => {
                let ot = self.infer_expr_type(operand).unwrap_or(Type::Any);
                Some(types::infer_unaryop_type(*op, &ot))
            }

            // Comparison always returns bool
            ExprKind::Compare { .. } => Some(Type::Bool),

            // Function call — look up return type
            ExprKind::Call { func, args, .. } => self.infer_call_type(func, args),

            // Ternary expression
            ExprKind::IfExpr {
                body, else_body, ..
            } => {
                let bt = self.infer_expr_type(body);
                let et = self.infer_expr_type(else_body);
                match (bt, et) {
                    (Some(b), Some(e)) if b == e => Some(b),
                    (Some(b), Some(e)) => Some(Type::Union(vec![b, e])),
                    (Some(t), None) | (None, Some(t)) => Some(t),
                    (None, None) => None,
                }
            }

            // Subscript: x[i]
            ExprKind::Subscript { value, .. } => {
                let vt = self.infer_expr_type(value)?;
                match vt {
                    Type::List(inner) => Some(*inner),
                    Type::Dict(_, v) => Some(*v),
                    Type::Tuple(_) => Some(Type::Any), // index-dependent
                    Type::Str => Some(Type::Str),
                    _ => Some(Type::Any),
                }
            }

            // Attribute access: x.attr — limited inference
            ExprKind::Attribute { .. } => Some(Type::Any),

            // Lambda
            ExprKind::Lambda { .. } => Some(Type::Callable(vec![], Box::new(Type::Any))),

            // List comprehension
            ExprKind::ListComp { .. } => Some(Type::List(Box::new(Type::Any))),
            ExprKind::SetComp { .. } => Some(Type::Set(Box::new(Type::Any))),
            ExprKind::DictComp { .. } => Some(Type::Dict(Box::new(Type::Any), Box::new(Type::Any))),
            ExprKind::GeneratorExp { .. } => Some(Type::Any),

            // Await: unwrap Promise-like
            ExprKind::Await(inner) => self.infer_expr_type(inner),

            // Walrus operator: x := expr
            ExprKind::NamedExpr { value, .. } => self.infer_expr_type(value),

            _ => None,
        }
    }

    /// Infer the return type of a function call.
    fn infer_call_type(&self, func: &Expr, args: &[Expr]) -> Option<Type> {
        match &func.kind {
            ExprKind::Name(name) => {
                // Built-in function return types
                match name.as_str() {
                    "len" => return Some(Type::Int),
                    "int" => return Some(Type::Int),
                    "float" => return Some(Type::Float),
                    "str" => return Some(Type::Str),
                    "bool" => return Some(Type::Bool),
                    "list" => return Some(Type::List(Box::new(Type::Any))),
                    "dict" => return Some(Type::Dict(Box::new(Type::Any), Box::new(Type::Any))),
                    "set" => return Some(Type::Set(Box::new(Type::Any))),
                    "tuple" => return Some(Type::Tuple(vec![])),
                    "abs" => return Some(Type::Any), // depends on input
                    "round" => return Some(Type::Int),
                    "sorted" => return Some(Type::List(Box::new(Type::Any))),
                    "reversed" => return Some(Type::Any),
                    "enumerate" => return Some(Type::Any),
                    "zip" => return Some(Type::Any),
                    "range" => return Some(Type::Any),
                    "isinstance" => return Some(Type::Bool),
                    "type" => return Some(Type::Any),
                    "print" => return Some(Type::NoneType),
                    "input" => return Some(Type::Str),
                    _ => {}
                }
                // User-defined function
                if let Some(sig) = self.function_sigs.get(name) {
                    return Some(sig.return_type.clone());
                }
                // Stub-imported callable in scope. This is the path that
                // generic `use_state[T](initial: T) -> Tuple[T, ...]`
                // signatures take: the import binds the name as
                // `Type::Callable(params, ret)` where the body may
                // contain TypeVars; here we specialize against the
                // actual argument types so the call site sees the
                // concrete inferred return type instead of `Any`.
                if let Some(ty) = self.lookup_var(name) {
                    if let Type::Callable(params, ret) = ty.clone() {
                        return Some(self.specialize_callable(&params, &ret, args));
                    }
                }
                // Uppercase name — class constructor
                if name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    return Some(Type::Named(name.clone()));
                }
                None
            }
            _ => None,
        }
    }

    /// Specialize a stub-declared generic callable against the actual
    /// argument expressions. Returns the substituted return type.
    ///
    /// Algorithm: for each declared param that contains TypeVars,
    /// unify it with the inferred type of the corresponding actual
    /// argument; accumulate bindings; substitute in the return type.
    /// Unbound TypeVars (e.g., the user passed too few args) pass
    /// through and behave like `Any` downstream.
    fn specialize_callable(&self, params: &[Type], ret: &Type, args: &[Expr]) -> Type {
        // Fast path: no generics in the signature → return ret as-is.
        let has_generics =
            params.iter().any(types::contains_type_var) || types::contains_type_var(ret);
        if !has_generics {
            return ret.clone();
        }

        let mut bindings: types::Bindings = std::collections::HashMap::new();
        for (param_ty, arg_expr) in params.iter().zip(args.iter()) {
            if !types::contains_type_var(param_ty) {
                continue;
            }
            let arg_ty = self.infer_expr_type(arg_expr).unwrap_or(Type::Any);
            types::unify(param_ty, &arg_ty, &mut bindings);
        }
        types::substitute(ret, &bindings)
    }

    /// Try to infer a common element type from a list of expressions.
    fn infer_homogeneous_expr_type(&self, elts: &[Expr]) -> Type {
        if elts.is_empty() {
            return Type::Any;
        }
        let first = self.infer_expr_type(&elts[0]);
        if let Some(ref t) = first {
            if elts
                .iter()
                .skip(1)
                .all(|e| self.infer_expr_type(e).as_ref() == Some(t))
            {
                return t.clone();
            }
        }
        Type::Any
    }

    // ── Call argument checking ─────────────────────────────

    /// Recursively check function call expressions in an expression tree.
    fn check_expr_calls(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Call {
                func, args, kwargs, ..
            } => {
                if let ExprKind::Name(name) = &func.kind {
                    if let Some(sig) = self.function_sigs.get(name).cloned() {
                        self.check_call_args(name, &sig, args, kwargs, expr.span);
                    }
                }
                // Recurse into subexpressions
                for arg in args {
                    self.check_expr_calls(arg);
                }
                for kw in kwargs {
                    self.check_expr_calls(&kw.value);
                }
            }
            ExprKind::BinOp { left, op, right } => {
                self.check_expr_calls(left);
                self.check_expr_calls(right);
                // Reject mixed str/numeric `+`. Python raises TypeError;
                // raw JS would silently coerce (`1 + "1"` -> `"11"`).
                // Only flag when both operand types are statically known
                // and incompatible — untyped flows pass through unchanged.
                if *op == BinOp::Add {
                    if let (Some(lt), Some(rt)) =
                        (self.infer_expr_type(left), self.infer_expr_type(right))
                    {
                        let is_num = |t: &Type| matches!(t, Type::Int | Type::Float | Type::Bool);
                        let mixed =
                            (lt == Type::Str && is_num(&rt)) || (is_num(&lt) && rt == Type::Str);
                        if mixed {
                            self.errors.push(
                                TypeError::new(
                                    format!("Unsupported operand types for +: {} and {}", lt, rt),
                                    expr.span,
                                )
                                .with_note(
                                    "Python raises TypeError for mixed str/number '+'; \
                                     convert with str(...) or use an f-string"
                                        .to_string(),
                                ),
                            );
                        }
                    }
                }
            }
            ExprKind::UnaryOp { operand, .. } => {
                self.check_expr_calls(operand);
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                self.check_expr_calls(test);
                self.check_expr_calls(body);
                self.check_expr_calls(else_body);
            }
            _ => {}
        }
    }

    /// Check a function call's argument count and types.
    fn check_call_args(
        &mut self,
        name: &str,
        sig: &FuncSig,
        args: &[Expr],
        _kwargs: &[Keyword],
        span: pyths_syntax::span::Span,
    ) {
        // Skip arg count check if function has *args
        if !sig.has_args {
            let required_count = sig
                .params
                .iter()
                .filter(|(_, _, has_default)| !has_default)
                .count();
            let max_count = sig.params.len();
            let actual_count = args.len();

            if actual_count < required_count {
                self.errors.push(TypeError::new(
                    format!(
                        "Function '{}' expects at least {} argument(s), got {}",
                        name, required_count, actual_count
                    ),
                    span,
                ));
                return;
            } else if actual_count > max_count {
                self.errors.push(TypeError::new(
                    format!(
                        "Function '{}' expects at most {} argument(s), got {}",
                        name, max_count, actual_count
                    ),
                    span,
                ));
                return;
            }
        }

        // Check argument types against parameter types
        for (i, arg) in args.iter().enumerate() {
            if i >= sig.params.len() {
                break; // *args or already reported count error
            }
            let (param_name, expected_type, _) = &sig.params[i];
            if matches!(expected_type, Type::Any) {
                continue; // no annotation, skip check
            }
            if let Some(actual_type) = self.infer_expr_type(arg) {
                if !types::is_assignable(expected_type, &actual_type) {
                    self.errors.push(
                        TypeError::new(
                            format!(
                                "Argument type mismatch: parameter '{}' of '{}' expects {}, got {}",
                                param_name, name, expected_type, actual_type
                            ),
                            arg.span,
                        )
                        .with_note(format!(
                            "Expected {} for parameter '{}'",
                            expected_type, param_name
                        )),
                    );
                }
            }
        }
    }

    // ── Type narrowing ────────────────────────────────────

    /// Extract type narrowings from a condition expression.
    /// Returns pairs of (variable_name, narrowed_type).
    fn extract_narrowings(&self, test: &Expr) -> Vec<(String, Type)> {
        let mut narrowings = Vec::new();

        match &test.kind {
            // `x is not None` → narrow Optional[T] to T
            ExprKind::Compare { left, comparisons } => {
                if comparisons.len() == 1 {
                    let (op, right) = &comparisons[0];
                    match op {
                        BinOp::IsNot => {
                            if matches!(right.kind, ExprKind::NoneLiteral) {
                                if let ExprKind::Name(name) = &left.kind {
                                    if let Some(ty) = self.lookup_var(name) {
                                        narrowings.push((name.clone(), types::unwrap_optional(ty)));
                                    }
                                }
                            }
                        }
                        BinOp::Is => {
                            // `x is None` → in the else branch x is narrowed, but in the if-body x is None
                            // We don't narrow here since this is the if-body
                        }
                        _ => {}
                    }
                }
            }
            // `isinstance(x, T)` → narrow x to T
            ExprKind::Call { func, args, .. } => {
                if let ExprKind::Name(fname) = &func.kind {
                    if fname == "isinstance" && args.len() == 2 {
                        if let ExprKind::Name(var_name) = &args[0].kind {
                            let narrowed = types::resolve_type(&args[1]);
                            if narrowed != Type::Any {
                                narrowings.push((var_name.clone(), narrowed));
                            }
                        }
                    }
                }
            }
            // `not x` — negate (limited support)
            ExprKind::UnaryOp {
                op: UnaryOp::Not,
                operand,
            } => {
                // `not x` where x is Optional → we could narrow to NoneType in the if-body
                // But that's less useful, skip for now
                let _ = operand;
            }
            _ => {}
        }

        narrowings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(source: &str) -> Vec<TypeError> {
        let module = pyths_parser::parse(source).expect("Parse failed");
        TypeChecker::check(&module)
    }

    // ── Basic annotated assignment ────────────────────────

    #[test]
    fn test_annotated_assign_ok() {
        let errors = check("x: int = 5");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_annotated_assign_mismatch() {
        let errors = check("x: int = \"hello\"");
        assert_eq!(errors.len(), 1, "Expected 1 error: {:?}", errors);
        assert!(errors[0].message.contains("Type mismatch"));
    }

    // ── Return type checking ──────────────────────────────

    #[test]
    fn test_return_type_ok() {
        let errors = check("def f() -> int:\n    return 42");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_return_type_mismatch() {
        let errors = check("def f() -> int:\n    return \"hello\"");
        assert_eq!(errors.len(), 1, "Expected 1 error: {:?}", errors);
        assert!(errors[0].message.contains("Return type mismatch"));
    }

    // ── Function call argument count ──────────────────────

    #[test]
    fn test_call_arg_count_ok() {
        let errors = check("def f(a, b):\n    pass\nf(1, 2)");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_call_arg_count_wrong() {
        let errors = check("def f(a, b):\n    pass\nf(1)");
        assert_eq!(errors.len(), 1, "Expected 1 error: {:?}", errors);
        assert!(errors[0].message.contains("expects at least 2"));
    }

    // ── Type compatibility ────────────────────────────────

    #[test]
    fn test_optional_allows_none() {
        let errors = check("x: Optional[int] = None");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_int_assignable_to_float() {
        let errors = check("x: float = 5");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_unannotated_no_errors() {
        let errors = check("x = 5\ny = \"hello\"\ndef f(a, b):\n    return a + b");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_any_always_compatible() {
        let errors = check("x: Any = 5\ny: Any = \"hello\"");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    // ── Phase 5: Variable type tracking ───────────────────

    #[test]
    fn test_variable_reassign_type_mismatch() {
        let errors = check("x: int = 5\nx = \"hello\"");
        assert_eq!(errors.len(), 1, "Expected 1 error: {:?}", errors);
        assert!(errors[0].message.contains("cannot assign"));
    }

    #[test]
    fn test_variable_reassign_compatible() {
        let errors = check("x: int = 5\nx = 10");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_inferred_variable_tracking() {
        // x is inferred as int from first assignment, then reassigning str should error
        let errors = check("x = 5\nx = \"hello\"");
        assert_eq!(errors.len(), 1, "Expected 1 error: {:?}", errors);
        assert!(errors[0].message.contains("cannot assign"));
    }

    #[test]
    fn test_inferred_variable_compatible() {
        let errors = check("x = 5\nx = 10");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    // ── Phase 5: Expression type inference ────────────────

    #[test]
    fn test_binop_type_inference() {
        // x: str should not accept int + int (= int)
        let errors = check("x: str = 1 + 2");
        assert_eq!(errors.len(), 1, "Expected 1 error: {:?}", errors);
    }

    #[test]
    fn test_binop_str_concat_ok() {
        let errors = check("x: str = \"hello\" + \" world\"");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_comparison_returns_bool() {
        let errors = check("x: bool = 1 < 2");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_division_returns_float() {
        let errors = check("x: float = 10 / 3");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_floor_div_returns_int() {
        let errors = check("x: int = 10 // 3");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_variable_reference_type() {
        // a is int, b should accept a (int)
        let errors = check("a: int = 5\nb: int = a");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_variable_reference_mismatch() {
        let errors = check("a: int = 5\nb: str = a");
        assert_eq!(errors.len(), 1, "Expected 1 error: {:?}", errors);
    }

    // ── Phase 5: Function call argument types ─────────────

    #[test]
    fn test_call_arg_type_ok() {
        let errors = check("def add(a: int, b: int) -> int:\n    return a + b\nadd(1, 2)");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_call_arg_type_mismatch() {
        let errors = check("def add(a: int, b: int) -> int:\n    return a + b\nadd(1, \"hello\")");
        assert_eq!(errors.len(), 1, "Expected 1 error: {:?}", errors);
        assert!(errors[0].message.contains("Argument type mismatch"));
    }

    #[test]
    fn test_call_return_type_inference() {
        // greet returns str, assigning to int should error
        let errors =
            check("def greet(name: str) -> str:\n    return name\nx: int = greet(\"Alice\")");
        assert_eq!(errors.len(), 1, "Expected 1 error: {:?}", errors);
    }

    // ── Phase 5: Type narrowing ───────────────────────────

    #[test]
    fn test_builtin_return_types() {
        let errors = check("x: int = len([1, 2, 3])");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_bool_assignable_to_int() {
        let errors = check("x: int = True");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_fstring_is_str() {
        let errors = check("name = \"world\"\nx: str = f\"hello {name}\"");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_list_element_type_inference() {
        // [1, 2, 3] should be List[int]
        let errors = check("x: List[str] = [1, 2, 3]");
        assert_eq!(errors.len(), 1, "Expected 1 error: {:?}", errors);
    }

    #[test]
    fn test_union_type_annotation() {
        let errors = check("x: Union[int, str] = 5");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
        let errors2 = check("x: Union[int, str] = \"hello\"");
        assert!(errors2.is_empty(), "Expected no errors: {:?}", errors2);
    }

    #[test]
    fn test_unary_not_returns_bool() {
        let errors = check("x: bool = not True");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    #[test]
    fn test_negation_preserves_type() {
        let errors = check("x: int = -5");
        assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    }

    // ── .pyi stub resolution ──────────────────────────────────────

    #[test]
    fn test_stub_loaded_for_react_import() {
        // `from react import use_state` should bind `use_state` to a
        // Callable, not Any. The annotated-assign check below would
        // otherwise pass even for nonsense usage; with the stub bound,
        // wrong-type assignments to a hook return get caught.
        let errors = check("from react import use_state\nuse_state(0)");
        assert!(errors.is_empty(), "valid hook call: {:?}", errors);
    }

    #[test]
    fn test_stub_loaded_for_pyths_react_alias() {
        // `pyths.react` and `react` share the same bundled stub.
        let errors = check("from pyths.react import use_effect\nuse_effect(lambda: None)");
        assert!(errors.is_empty(), "valid effect: {:?}", errors);
    }

    #[test]
    fn test_stub_unknown_module_falls_back_to_any() {
        // Modules without bundled stubs bind imports as Any — no
        // error, even though we don't know the actual signature.
        let errors = check("from totally_random_pkg import nonsense\nx = nonsense");
        assert!(errors.is_empty(), "fallback to Any: {:?}", errors);
    }

    #[test]
    fn test_stub_unknown_name_in_known_module() {
        // Importing a name that's in the stub registry's module but
        // NOT in the stub's actual exports falls back to Any. Doesn't
        // error (stubs are best-effort).
        let errors = check("from react import nonexistent_hook\nx = nonexistent_hook");
        assert!(errors.is_empty(), "missing-name fallback: {:?}", errors);
    }

    #[test]
    fn test_stub_class_import() {
        // Class names from a stub (e.g., Fragment) bind as Type::Named.
        let errors = check("from react import Fragment\nx = Fragment");
        assert!(errors.is_empty(), "class import: {:?}", errors);
    }

    #[test]
    fn test_stub_with_user_alias() {
        // User aliases bind under the alias name. Type is from the
        // stub's original entry.
        let errors = check("from react import use_state as st\nst(0)");
        assert!(errors.is_empty(), "aliased import: {:?}", errors);
    }

    // ── Generic stubs (TypeVar inference) ──────────────────────

    fn parse_module(src: &str) -> pyths_syntax::ast::Module {
        pyths_parser::parse(src).expect("parse failed")
    }

    #[test]
    fn generic_callable_resolves_with_int_arg() {
        // A stub-declared generic function `def g(x: T) -> T`
        // imported into scope is specialized at the call site so the
        // checker knows `g(1)` returns int.
        // PythScribe stubs require the `...` body on its own line
        // (single-line `def f(): ...` isn't parsed).
        let stub = "def g(x: T) -> T:\n    ...\n";
        let user = "from my_lib import g\nresult: int = g(1)\n";

        // Write the stub to a temp dir and configure the project-stubs
        // path to point at it.
        let tmp = std::env::temp_dir().join(format!(
            "pyths_generic_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("my_lib.pyi"), stub).unwrap();

        let module = parse_module(user);
        let errors = TypeChecker::check_with_stub_paths(&module, &[tmp.clone()]);
        assert!(errors.is_empty(), "generic resolves: {:?}", errors);

        // Now verify type-mismatch is caught: `result: str = g(1)` should
        // complain that int is not str.
        let bad = "from my_lib import g\nresult: str = g(1)\n";
        let module2 = parse_module(bad);
        let errors2 = TypeChecker::check_with_stub_paths(&module2, &[tmp.clone()]);
        assert!(
            !errors2.is_empty(),
            "generic should catch int-vs-str mismatch: {:?}",
            errors2
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn generic_callable_resolves_through_tuple_return() {
        // Stub: `def use_state(initial: T) -> Tuple[T, Callable[[T], None]]: ...`
        // Call: `count, set_count = use_state(0)`
        // After specialization, `count` should be inferred as int.
        let stub = "def use_state(initial: T) -> Tuple[T, Callable[[T], None]]:\n    ...\n";
        let user = "from hooks import use_state\n\
                    count, set_count = use_state(0)\n\
                    x: int = count\n";

        let tmp = std::env::temp_dir().join(format!(
            "pyths_generic_test2_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("hooks.pyi"), stub).unwrap();

        let module = parse_module(user);
        let errors = TypeChecker::check_with_stub_paths(&module, &[tmp.clone()]);
        // The current checker doesn't yet destructure tuple-typed
        // bindings into their elements (separate gap), so we can't yet
        // assert `count: int` is enforced. What we CAN assert: the call
        // itself type-checks cleanly, and the return type's TypeVar
        // was substituted (no panics, no errors). The destructure-binding
        // refinement is queued as a follow-on.
        assert!(errors.is_empty(), "generic tuple return: {:?}", errors);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn typevar_name_recognized() {
        use crate::types::is_type_var_name;
        assert!(is_type_var_name("T"));
        assert!(is_type_var_name("U"));
        assert!(is_type_var_name("K"));
        assert!(is_type_var_name("V"));
        assert!(is_type_var_name("E"));
        assert!(is_type_var_name("T_co"));
        assert!(is_type_var_name("T_contra"));
        // Multi-char names are Named, not TypeVar.
        assert!(!is_type_var_name("MyClass"));
        assert!(!is_type_var_name("Foo"));
        // Lowercase: Any.
        assert!(!is_type_var_name("t"));
    }

    #[test]
    fn substitute_preserves_concrete_types() {
        use crate::types::{substitute, Bindings, Type};
        let mut bindings = Bindings::new();
        bindings.insert("T".to_string(), Type::Int);
        // T → Int
        assert_eq!(substitute(&Type::TypeVar("T".into()), &bindings), Type::Int);
        // Tuple[T, str] → Tuple[Int, str]
        let input = Type::Tuple(vec![Type::TypeVar("T".into()), Type::Str]);
        assert_eq!(
            substitute(&input, &bindings),
            Type::Tuple(vec![Type::Int, Type::Str])
        );
        // Unbound TypeVar passes through.
        let unbound = Type::TypeVar("U".into());
        assert_eq!(substitute(&unbound, &bindings), unbound);
    }

    #[test]
    fn unify_records_simple_binding() {
        use crate::types::{unify, Bindings, Type};
        let mut bindings = Bindings::new();
        let target = Type::TypeVar("T".into());
        let source = Type::Int;
        assert!(unify(&target, &source, &mut bindings));
        assert_eq!(bindings.get("T"), Some(&Type::Int));
    }

    #[test]
    fn unify_records_nested_binding() {
        // Unify `List[T]` against `List[str]` → T = str.
        use crate::types::{unify, Bindings, Type};
        let mut bindings = Bindings::new();
        let target = Type::List(Box::new(Type::TypeVar("T".into())));
        let source = Type::List(Box::new(Type::Str));
        assert!(unify(&target, &source, &mut bindings));
        assert_eq!(bindings.get("T"), Some(&Type::Str));
    }

    // ── Tuple destructure with element-wise types ──────────────────

    #[test]
    fn destructure_simple_tuple_literal() {
        // `a, b = (1, "x")` should bind `a: int, b: str` such that a
        // later `a: int = a` is fine but `a: str = a` errors.
        let errors = check("a, b = (1, \"x\")\nresult: int = a\n");
        assert!(errors.is_empty(), "valid destructure: {:?}", errors);

        let errors2 = check("a, b = (1, \"x\")\nresult: int = b\n");
        // b should be str; assigning to int should fail.
        assert!(
            !errors2.is_empty(),
            "b should be str, int assignment must error: {:?}",
            errors2
        );
    }

    #[test]
    fn destructure_through_generic_hook_return() {
        // `count, set_count = use_state(0)` — the bundled react stub
        // is generic, so count should be inferred as int. Assigning
        // count to a str-typed variable should fail.
        let errors = check(
            "from react import use_state\n\
             count, set_count = use_state(0)\n\
             result: int = count\n",
        );
        assert!(errors.is_empty(), "well-typed: {:?}", errors);

        let bad = check(
            "from react import use_state\n\
             count, set_count = use_state(0)\n\
             result: str = count\n",
        );
        assert!(
            !bad.is_empty(),
            "count should be int after destructure: {:?}",
            bad
        );
    }

    #[test]
    fn destructure_size_mismatch_falls_back_gracefully() {
        // RHS tuple length doesn't match LHS — fall back to binding
        // each target to the whole value type. Should NOT panic.
        let errors = check("a, b, c = (1, 2)\nresult: int = a\n");
        // We don't assert error/no-error here — just that we don't crash
        // and the program type-checks one way or the other.
        let _ = errors;
    }

    #[test]
    fn destructure_from_list_binds_element_type() {
        // `head, tail = [1, 2]` — list-of-int RHS. Both head and
        // tail bind to int. (Real Python would error on length, but
        // our type checker is lenient and binds element types.)
        let errors = check("head, tail = [1, 2]\nresult: str = head\n");
        // head should be int; result: str should error.
        assert!(
            !errors.is_empty(),
            "head should be int from list-of-int: {:?}",
            errors
        );
    }

    // ── Mixed str/numeric arithmetic rejection (JS-coercion guard) ──

    #[test]
    fn reject_int_plus_str() {
        // Python raises TypeError for `1 + "1"`; PythScribe must not
        // silently coerce to "11" the way raw JS does.
        let errors = check("y = 1 + \"1\"\n");
        assert!(
            !errors.is_empty(),
            "int + str must be a type error: {:?}",
            errors
        );
    }

    #[test]
    fn reject_str_plus_int() {
        let errors = check("y = \"1\" + 1\n");
        assert!(
            !errors.is_empty(),
            "str + int must be a type error: {:?}",
            errors
        );
    }

    #[test]
    fn reject_annotated_int_plus_str() {
        let errors = check("x: int = 5\ny = x + \"1\"\n");
        assert!(
            !errors.is_empty(),
            "annotated int + str must be a type error: {:?}",
            errors
        );
    }

    #[test]
    fn str_plus_str_ok() {
        let errors = check("y = \"a\" + \"b\"\n");
        assert!(errors.is_empty(), "str + str is valid: {:?}", errors);
    }

    #[test]
    fn int_plus_int_ok() {
        let errors = check("y = 1 + 2\n");
        assert!(errors.is_empty(), "int + int is valid: {:?}", errors);
    }

    #[test]
    fn int_plus_float_ok() {
        let errors = check("y = 1 + 2.0\n");
        assert!(errors.is_empty(), "int + float is valid: {:?}", errors);
    }

    #[test]
    fn str_times_int_ok() {
        // String repetition stays valid — only `+` mixing is rejected.
        let errors = check("y = \"ab\" * 3\n");
        assert!(errors.is_empty(), "str * int is valid: {:?}", errors);
    }
}
